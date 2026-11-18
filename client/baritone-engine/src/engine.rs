use crate::abi::{BAD_REQUEST, HOST_UNAVAILABLE, MAX_PATH, MAX_TARGETS, UNSUPPORTED};
use crate::baritone_astar::{AStarPathFinder, Status};
use crate::baritone_block_state::BlockState;
use crate::baritone_block_state_interface::BlockStateInterface;
use crate::baritone_calculation_context::{CalculationContext, CostHooks};
use crate::baritone_costs::COST_INF;
use crate::baritone_executor::{ExecEnv, PathExecutor};
use crate::baritone_goals::{GoalBlock, GoalComposite, GoalRef, rc};
use crate::baritone_movement_helper as mh;
use crate::baritone_movement_state::Input;
use crate::baritone_player_context::PlayerContext;
use crate::baritone_rotation::Rotation;
use crate::baritone_settings::Settings;
use crate::baritone_utils::Pos;
use crate::host::{Facts, Host, HostWorld};
use crate::predicted::{self, Directive, Observation, Outcome, Phase, PredictedMineProcess};
use crate::protocol::{self, Frame, Identity};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const EXPANSIONS_PER_TICK: usize = 2048;
const SEARCH_TIME_PER_TICK: Duration = Duration::from_millis(3);
const MAX_SEARCH_NODES: usize = 65_536;
const MAX_EXECUTION_SEGMENT: usize = 256;
const MAX_EXECUTOR_REPLANS: u32 = 3;
const SCAN_BLOCKS_PER_TICK: usize = 4096;

#[derive(Clone)]
pub struct Limits {
    pub anchor: [f64; 3],
    pub radius: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub max_targets: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            anchor: [0.0, 64.0, 0.0],
            radius: 128.0,
            min_y: -64,
            max_y: 319,
            max_targets: 64,
        }
    }
}

impl Limits {
    pub fn accepts(&self, p: Pos) -> bool {
        p[1] >= self.min_y
            && p[1] <= self.max_y
            && (0..3)
                .map(|i| (f64::from(p[i]) - self.anchor[i]).powi(2))
                .sum::<f64>()
                <= self.radius * self.radius
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub world: u64,
    pub replay: u64,
    pub dimension: String,
}
impl Source {
    fn read(v: &Value) -> protocol::Result<Self> {
        let source = Self {
            world: protocol::id(v, "world")?,
            replay: protocol::id(v, "replay")?,
            dimension: protocol::dimension(&v["dimension"])?,
        };
        if source.world == 0 || source.replay == 0 {
            return Err(BAD_REQUEST);
        }
        Ok(source)
    }
    fn json(&self) -> Value {
        json!({"world":self.world.to_string(),"replay":self.replay.to_string(),"dimension":self.dimension})
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    Goto,
    Visible,
    Predicted,
}
#[derive(Clone, Copy)]
struct Target {
    pos: Pos,
    kind: u8,
}
struct Active {
    process: PredictedMineProcess,
    prior_stance: Option<Pos>,
}

#[derive(Default)]
struct Counters {
    mined: u32,
    skipped: u32,
    mismatch: u32,
    unconfirmed: u32,
    unreachable: u32,
    attempted: u32,
}
impl Counters {
    fn json(&self) -> Value {
        json!({"mined":self.mined,"skipped":self.skipped,"mismatch":self.mismatch,
        "unconfirmed":self.unconfirmed,"unreachable":self.unreachable,"attempted":self.attempted})
    }
    fn record(&mut self, o: Outcome) {
        match o {
            Outcome::Mined => self.mined += 1,
            Outcome::Mismatch => self.mismatch += 1,
            Outcome::Unconfirmed => self.unconfirmed += 1,
            Outcome::Unreachable => {
                self.unreachable += 1;
                self.skipped += 1;
            }
            Outcome::Skipped | Outcome::Rejected => self.skipped += 1,
        }
    }
}

struct Protect(Option<Pos>);
impl CostHooks for Protect {
    fn break_cost_multiplier_at(
        &self,
        _ctx: &CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        _state: &BlockState,
    ) -> Option<f64> {
        (self.0 == Some([x, y, z])).then_some(COST_INF)
    }
    fn cost_of_placing_at(
        &self,
        _ctx: &CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        _state: &BlockState,
    ) -> Option<f64> {
        (self.0 == Some([x, y, z])).then_some(COST_INF)
    }
}

#[derive(Default)]
struct Navigator {
    finder: Option<AStarPathFinder>,
    executor: Option<PathExecutor>,
    goals: Vec<Pos>,
    revision: Option<(u64, u64, u64)>,
    search_ticks: u32,
    failed: bool,
    executor_replans: u32,
    last_diagnostic: String,
}

// Host observations are synchronous calls back into the client thread. A node
// budget alone can monopolize that thread for seconds in dense underground
// terrain. Yield between individual nodes as well as at the existing node cap;
// the finder keeps its open set and resumes on the next simulation tick.
// One node (and an individual host callback) is the atomic unit and may overrun.
fn search_slice(finder: &mut AStarPathFinder, calc: &CalculationContext) -> Status {
    let began = Instant::now();
    for _ in 0..EXPANSIONS_PER_TICK {
        let status = finder.run(calc, 1);
        if !matches!(status, Status::Running) || began.elapsed() >= SEARCH_TIME_PER_TICK {
            return status;
        }
    }
    Status::Running
}

impl Navigator {
    fn clear(&mut self) {
        *self = Self::default();
    }
    fn diagnostic(&mut self, message: &str) {
        self.last_diagnostic = message.chars().take(512).collect();
        let _ = writeln!(
            std::io::stderr().lock(),
            "[Sunset Baritone] {}",
            self.last_diagnostic
        );
    }
    fn replan_failed_segment(&mut self, world: &HostWorld, reason: &str) {
        self.finder = None;
        self.search_ticks = 0;
        world.clear_all_keys();
        // Like upstream finish_current, drop the ended segment and search from
        // current feet toward the original goal. Unlike the unbounded upstream
        // loop, allow only three failed-segment replans per navigation goal.
        if self.executor_replans < MAX_EXECUTOR_REPLANS {
            self.executor_replans += 1;
            self.diagnostic(&format!(
                "replanning segment {}/{} from {:?}: {reason}",
                self.executor_replans,
                MAX_EXECUTOR_REPLANS,
                world.player_feet()
            ));
        } else {
            self.failed = true;
            self.diagnostic(&format!(
                "retry budget exhausted ({MAX_EXECUTOR_REPLANS} replans): {reason}"
            ));
        }
    }
    fn path(&self) -> Vec<Pos> {
        self.executor
            .as_ref()
            .map(|e| {
                e.path
                    .positions
                    .iter()
                    .skip(e.position())
                    .take(MAX_PATH)
                    .copied()
                    .collect()
            })
            .or_else(|| {
                self.finder
                    .as_ref()
                    .and_then(|s| s.best_so_far(s.num_nodes))
                    .map(|p| p.positions.into_iter().take(MAX_PATH).collect())
            })
            .unwrap_or_default()
    }
    fn dest(&self) -> Option<Pos> {
        self.executor.as_ref().map(|e| e.path.dest())
    }
    fn tick(
        &mut self,
        world: &HostWorld,
        settings: &Settings,
        mut goals: Vec<Pos>,
        protected: Option<Pos>,
    ) -> bool {
        // Stances arrive ordered by current distance to the player. The goal
        // of a composite is a set: merely reordering the same stances must not
        // discard an unfinished search as the player moves.
        goals.sort_unstable();
        goals.dedup();
        let revision = (
            world.frame.registry,
            world.frame.revision,
            world.frame.settings_revision,
        );
        let goals_changed = goals != self.goals;
        if goals_changed {
            self.clear();
        }
        if goals_changed || self.revision != Some(revision) {
            if let Some(nodes) = self
                .finder
                .as_ref()
                .map(|finder| finder.num_nodes)
                .filter(|&n| n > 0)
            {
                self.diagnostic(&format!(
                    "restarting search after {nodes} nodes: revision {:?} -> {revision:?}, goals_changed={goals_changed}",
                    self.revision
                ));
            }
            // Never resume an open set or any retained block/mining cache across
            // revisions. A completed path also receives a fresh live validation.
            self.finder = None;
            self.search_ticks = 0;
            // Block revisions invalidate searches, but must not replenish the
            // failed-segment retry budget or revive a terminal failure.
            if goals_changed
                || self
                    .revision
                    .is_none_or(|v| v.0 != revision.0 || v.2 != revision.2)
            {
                self.executor = None;
            }
            self.goals = goals;
            self.revision = Some(revision);
        }
        let mut calc = CalculationContext::new(settings, world, world, &world.calculation_facts());
        calc.hooks = Some(Rc::new(Protect(protected)));
        let at_goal = self.goals.contains(&world.player_feet());
        if at_goal {
            self.executor = None;
            self.finder = None;
            world.clear_all_keys();
            return true;
        }
        if self.goals.is_empty() {
            self.failed = true;
            self.diagnostic("no usable navigation goal");
            world.clear_all_keys();
            return false;
        }
        if self.failed {
            world.clear_all_keys();
            return false;
        }
        if let Some(mut executor) = self.executor.take() {
            // All pending break/place operations are checked for the protected
            // final target before and after the upstream executor runs.
            let e = ExecEnv {
                ctx: world,
                calc: &calc,
                settings,
                in_progress_best: None,
                in_progress: false,
                ticks: world.frame.tick,
            };
            // Upstream returns whether cancellation/segment handoff is safe,
            // including during a running movement. It is not a finished flag.
            executor.on_tick(&e);
            let mut failure_reason = None;
            for line in executor.log.drain(..) {
                self.diagnostic(&line);
                failure_reason = Some(line);
            }
            if executor.failed() {
                self.replan_failed_segment(
                    world,
                    failure_reason.as_deref().unwrap_or("path executor failed"),
                );
                return false;
            }
            if executor.to_break().iter().any(|p| Some(*p) == protected)
                || executor.to_place().iter().any(|p| Some(*p) == protected)
            {
                self.failed = true;
                self.diagnostic("path attempts to break or place the protected target");
                world.clear_all_keys();
                return false;
            }
            // The executor consumes Input::Sprint while deciding whether the
            // next tick should sprint, then exposes the result separately.
            world.set_sprinting(executor.is_sprinting());
            if executor.finished() {
                world.clear_all_keys();
            } else {
                self.executor = Some(executor);
            }
            return false;
        }
        if self.finder.is_none() {
            let goal: GoalRef = rc(GoalComposite(
                self.goals.iter().map(|p| rc(GoalBlock(*p))).collect(),
            ));
            self.finder = Some(AStarPathFinder::new(
                world.player_feet(),
                world.player_feet(),
                goal,
                HashMap::new(),
                1024,
                true,
            ));
        }
        let finder = self.finder.as_mut().expect("finder was constructed");
        self.search_ticks += 1;
        let result = search_slice(finder, &calc);
        let mut path = match result {
            Status::Done(path) => Some(path),
            Status::Exhausted => finder.best_so_far(finder.num_nodes),
            Status::Running if finder.num_nodes >= MAX_SEARCH_NODES || self.search_ticks >= 200 => {
                finder.best_so_far(finder.num_nodes)
            }
            Status::Running => return false,
        };
        self.finder = None;
        if let Some(mut raw) = path.take() {
            // Post-processing and executor lookahead also run on the client
            // thread. Bound their work; the next search continues at this
            // prefix's endpoint with the original goal still in force.
            raw.positions.truncate(MAX_EXECUTION_SEGMENT);
            let path = raw.post_process(&calc);
            if path.movements.is_empty() {
                self.failed = true;
                self.diagnostic(
                    path.note
                        .as_deref()
                        .unwrap_or("path search produced no executable movement"),
                );
            } else {
                self.executor = Some(PathExecutor::new(path));
            }
        } else {
            self.failed = true;
            self.diagnostic("path search exhausted without a usable segment");
        }
        false
    }
}

pub struct Engine {
    settings: Settings,
    limits: Limits,
    identity: Option<Identity>,
    mode: Mode,
    goal: Option<Pos>,
    source: Option<Source>,
    source_revision: u64,
    pending: bool,
    queue: VecDeque<Target>,
    attempted: HashSet<Pos>,
    active: Option<Active>,
    navigator: Navigator,
    previous_look: Option<Rotation>,
    facts: RefCell<HashMap<u32, Facts>>,
    registry: Option<u64>,
    last_tick: Option<u64>,
    empty_since: Option<u64>,
    counters: Counters,
    misses: u32,
    status: String,
    detail: String,
    events: Vec<Value>,
    visible_names: HashSet<String>,
    scan_chunks: Vec<[i32; 2]>,
    scan_chunk: usize,
    scan_offset: usize,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
impl Engine {
    pub fn new() -> Self {
        let mut settings = Settings::default();
        for (key, value) in [
            ("allowPlace", "false"),
            ("allowParkour", "false"),
            ("allowParkourPlace", "false"),
            ("allowParkourAscend", "false"),
        ] {
            settings.set(key, value).expect("upstream setting exists");
        }
        Self {
            settings,
            limits: Limits::default(),
            identity: None,
            mode: Mode::Idle,
            goal: None,
            source: None,
            source_revision: 0,
            pending: false,
            queue: VecDeque::new(),
            attempted: HashSet::new(),
            active: None,
            navigator: Navigator::default(),
            previous_look: None,
            facts: RefCell::new(HashMap::new()),
            registry: None,
            last_tick: None,
            empty_since: None,
            counters: Counters::default(),
            misses: 0,
            status: "Idle".into(),
            detail: String::new(),
            events: Vec::new(),
            visible_names: HashSet::new(),
            scan_chunks: Vec::new(),
            scan_chunk: 0,
            scan_offset: 0,
        }
    }
    pub fn cancel(&mut self, reason: &str) {
        self.mode = Mode::Idle;
        self.goal = None;
        self.queue.clear();
        self.active = None;
        self.navigator.clear();
        self.previous_look = None;
        self.source = None;
        self.pending = false;
        self.events.clear();
        self.empty_since = None;
        self.attempted.clear();
        self.scan_chunks.clear();
        self.visible_names.clear();
        self.status = "Idle".into();
        self.detail = reason.chars().take(512).collect();
    }
    fn start(&mut self, identity: Identity, mode: Mode) {
        self.cancel("");
        self.identity = Some(identity);
        self.mode = mode;
        self.last_tick = None;
        self.counters = Counters::default();
        self.misses = 0;
        self.source_revision = 0;
        self.scan_chunk = 0;
        self.scan_offset = 0;
        self.status = "Pathing".into();
    }
    pub fn dispatch(
        &mut self,
        operation: u32,
        v: &Value,
        host: Option<&dyn Host>,
    ) -> protocol::Result<Value> {
        if v["schema"].as_u64() != Some(1) {
            self.cancel("bad schema");
            return Err(BAD_REQUEST);
        }
        let result = match operation {
            1 => self.configure(v),
            2 => self.start_goto(v),
            3 => self.start_visible(v),
            4 => self.replace_targets(v),
            5 => match host {
                Some(host) => self.tick(v, host),
                None => Err(HOST_UNAVAILABLE),
            },
            6 => {
                self.cancel(v["reason"].as_str().unwrap_or("cancelled"));
                Ok(self.snapshot(None, false))
            }
            7 => Ok(self.snapshot(None, false)),
            _ => Err(UNSUPPORTED),
        };
        if result.is_err() {
            self.cancel("engine request failed");
        }
        result
    }
    fn configure(&mut self, v: &Value) -> protocol::Result<Value> {
        let mut settings = self.settings.clone();
        if let Some(values) = v["settings"].as_object() {
            for (key, value) in values {
                let text = value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string());
                settings.set(key, &text).map_err(|_| BAD_REQUEST)?;
            }
        }
        // V1 executes ordinary walking, stepping and safe falls only.
        for name in ["allowParkour", "allowParkourPlace", "allowParkourAscend"] {
            settings.set(name, "false").map_err(|_| BAD_REQUEST)?;
        }
        let anchor = if v["anchor"].is_null() {
            self.limits.anchor
        } else {
            protocol::vec3(&v["anchor"])?
        };
        let radius = v["radius"].as_f64().unwrap_or(self.limits.radius);
        let min_y = v["min_y"].as_i64().unwrap_or(i64::from(self.limits.min_y));
        let max_y = v["max_y"].as_i64().unwrap_or(i64::from(self.limits.max_y));
        let max_targets = v["max_targets"]
            .as_u64()
            .unwrap_or(self.limits.max_targets as u64);
        if !radius.is_finite()
            || !(8.0..=256.0).contains(&radius)
            || !(-4096..=4095).contains(&min_y)
            || !(-4096..=4095).contains(&max_y)
            || min_y > max_y
            || max_targets == 0
            || max_targets > MAX_TARGETS as u64
        {
            return Err(BAD_REQUEST);
        }
        self.cancel("configured");
        self.settings = settings;
        self.limits = Limits {
            anchor,
            radius,
            min_y: min_y as i32,
            max_y: max_y as i32,
            max_targets: max_targets as usize,
        };
        Ok(self.snapshot(None, false))
    }
    fn start_goto(&mut self, v: &Value) -> protocol::Result<Value> {
        let identity = Identity::read(v)?;
        let goal = protocol::pos(&v["goal"])?;
        if !(-4096..=4095).contains(&goal[1]) {
            return Err(BAD_REQUEST);
        }
        self.start(identity, Mode::Goto);
        self.goal = Some(goal);
        Ok(self.snapshot(None, false))
    }
    fn start_visible(&mut self, v: &Value) -> protocol::Result<Value> {
        let identity = Identity::read(v)?;
        let names = v["blocks"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 256)
            .ok_or(BAD_REQUEST)?;
        let mut parsed = HashSet::new();
        for name in names {
            let text = protocol::dimension(name)?;
            parsed.insert(text);
        }
        self.start(identity, Mode::Visible);
        self.visible_names = parsed;
        self.pending = true;
        Ok(self.snapshot(None, false))
    }
    fn replace_targets(&mut self, v: &Value) -> protocol::Result<Value> {
        let identity = Identity::read(v)?;
        let source = Source::read(&v["source"])?;
        if source.world != identity.world || source.dimension != identity.dimension {
            return Err(BAD_REQUEST);
        }
        let revision = protocol::id(v, "revision")?;
        let pending = v["pending"].as_bool().ok_or(BAD_REQUEST)?;
        let targets = v["targets"]
            .as_array()
            .filter(|a| a.len() <= MAX_TARGETS)
            .ok_or(BAD_REQUEST)?;
        let mut parsed = VecDeque::new();
        let mut seen = HashSet::new();
        for target in targets {
            let pos = protocol::pos(&target["pos"])?;
            let kind = target["kind"]
                .as_u64()
                .filter(|n| *n < 10)
                .ok_or(BAD_REQUEST)? as u8;
            if self.limits.accepts(pos) && seen.insert(pos) {
                parsed.push_back(Target { pos, kind });
            }
        }
        if self.mode == Mode::Idle && self.identity.as_ref() == Some(&identity) {
            return Ok(self.snapshot(None, false));
        }
        if self.mode != Mode::Predicted || self.identity.as_ref() != Some(&identity) {
            self.start(identity, Mode::Predicted);
            self.source = Some(source);
        } else if self.source.as_ref() != Some(&source) {
            // Source changes invalidate the run; a new explicit job generation
            // is required before another source may move the player.
            self.cancel("prediction source changed");
            return Ok(self.snapshot(None, false));
        } else if revision < self.source_revision {
            return Err(BAD_REQUEST);
        }
        self.source_revision = revision;
        self.pending = pending;
        parsed.retain(|t| !self.attempted.contains(&t.pos));
        parsed.truncate(self.limits.max_targets.saturating_sub(self.attempted.len()));
        self.queue = parsed;
        Ok(self.snapshot(None, false))
    }
    fn scan(&mut self, world: &HostWorld) {
        if self.scan_chunks.is_empty() && self.scan_chunk == 0 {
            self.scan_chunks = world.loaded_chunks();
            self.scan_chunks.retain(|p| {
                let x = f64::from(p[0]) * 16.0 + 8.0 - self.limits.anchor[0];
                let z = f64::from(p[1]) * 16.0 + 8.0 - self.limits.anchor[2];
                x * x + z * z <= (self.limits.radius + 12.0).powi(2)
            });
            self.scan_chunks.sort_by(|a, b| {
                let d = |p: &[i32; 2]| {
                    (f64::from(p[0]) * 16.0 + 8.0 - self.limits.anchor[0]).powi(2)
                        + (f64::from(p[1]) * 16.0 + 8.0 - self.limits.anchor[2]).powi(2)
                };
                d(a).total_cmp(&d(b)).then(a.cmp(b))
            });
        }
        let min = self.limits.min_y.max(world.min_y());
        let max = self.limits.max_y.min(world.max_y());
        if min > max {
            self.pending = false;
            return;
        }
        let height = (max - min + 1) as usize;
        let volume = 256 * height;
        let mut queued: HashSet<Pos> = self.queue.iter().map(|t| t.pos).collect();
        for _ in 0..SCAN_BLOCKS_PER_TICK {
            if self.scan_chunk >= self.scan_chunks.len() {
                self.pending = false;
                break;
            }
            if self.queue.len() + self.attempted.len() >= self.limits.max_targets {
                self.pending = false;
                break;
            }
            let chunk = self.scan_chunks[self.scan_chunk];
            let n = self.scan_offset;
            let p = [
                chunk[0].saturating_mul(16) + (n % 16) as i32,
                min + (n / 256) as i32,
                chunk[1].saturating_mul(16) + ((n / 16) % 16) as i32,
            ];
            self.scan_offset += 1;
            if self.scan_offset >= volume {
                self.scan_offset = 0;
                self.scan_chunk += 1;
            }
            if !self.limits.accepts(p) || self.attempted.contains(&p) || queued.contains(&p) {
                continue;
            }
            let b = world.read(p);
            if b.loaded == 0 || b.prediction_pending != 0 {
                continue;
            }
            let state = world.state(b.state_id);
            if self.visible_names.contains(&*state.name) {
                let kind = (0..10)
                    .find(|k| predicted::matches_kind(*k, &state.name))
                    .unwrap_or(255);
                self.queue.push_back(Target { pos: p, kind });
                queued.insert(p);
            }
        }
        let mut targets: Vec<_> = self.queue.drain(..).collect();
        let feet = world.position();
        targets.sort_by(|a, b| {
            let d = |p: Pos| {
                (0..3)
                    .map(|i| (f64::from(p[i]) - feet[i]).powi(2))
                    .sum::<f64>()
            };
            d(a.pos)
                .total_cmp(&d(b.pos))
                .then(a.pos.cmp(&b.pos))
                .then(a.kind.cmp(&b.kind))
        });
        self.queue = targets.into();
    }
    fn tick(&mut self, v: &Value, host: &dyn Host) -> protocol::Result<Value> {
        let frame = Frame::read(v)?;
        if self.identity.as_ref() != Some(&frame.identity) || !frame.input_available {
            self.cancel("movement lease or world unavailable");
            self.identity = Some(frame.identity.clone());
            return Ok(self.snapshot(Some(&frame), true));
        }
        if self.last_tick.is_some_and(|tick| frame.tick <= tick) {
            return Err(BAD_REQUEST);
        }
        self.last_tick = Some(frame.tick);
        if self.registry != Some(frame.registry) {
            self.facts.borrow_mut().clear();
            self.navigator.clear();
            self.previous_look = None;
            self.registry = Some(frame.registry);
        }
        if self.mode == Mode::Idle {
            return Ok(self.snapshot(Some(&frame), true));
        }
        let anchor = (self.mode != Mode::Goto).then_some((self.limits.anchor, self.limits.radius));
        // Take the cold-facts cache out temporarily, so the synchronous read view
        // does not borrow Engine while the process policy advances.
        let facts = RefCell::new(std::mem::take(&mut *self.facts.borrow_mut()));
        let world =
            HostWorld::new(host, &frame, &facts, anchor)?.with_previous_look(self.previous_look);
        let result = self.tick_world(&world);
        let mut response = self.snapshot(Some(&frame), true);
        if self.mode != Mode::Idle {
            let protected = self
                .active
                .as_ref()
                .filter(|a| a.process.phase != Phase::Mine)
                .map(|a| a.process.pos);
            let (movement, attack, use_target) = world.resolved_actions(protected);
            let intents = world.intents.borrow();
            response["movement"] = json!(movement);
            response["yaw"] = json!(intents.look.map(|r| r.yaw));
            response["pitch"] = json!(intents.look.map(|r| r.pitch));
            response["selected_slot"] =
                json!(intents.slot.filter(|slot| *slot != frame.selected_slot));
            response["attack"] = attack.unwrap_or(Value::Null);
            response["use"] = use_target.unwrap_or(Value::Null);
        }
        let check = world.check();
        self.previous_look = (self.mode != Mode::Idle)
            .then(|| world.intents.borrow().look)
            .flatten();
        drop(world);
        *self.facts.borrow_mut() = facts.into_inner();
        result?;
        check?;
        Ok(response)
    }
    fn tick_world(&mut self, world: &HostWorld) -> protocol::Result<()> {
        let frame = world.frame;
        if self.mode == Mode::Goto {
            let goal = self.goal.ok_or(BAD_REQUEST)?;
            if goal[1] < frame.min_y || goal[1] > frame.max_y {
                return Err(BAD_REQUEST);
            }
            if self.navigator.tick(world, &self.settings, vec![goal], None) {
                self.mode = Mode::Idle;
                self.status = "Idle".into();
                self.detail = "goal reached".into();
                self.navigator.clear();
            } else if self.navigator.failed {
                let diagnostic = self.navigator.last_diagnostic.clone();
                self.cancel(&format!("goal is unreachable: {diagnostic}"));
            } else {
                self.status = "Pathing".into();
                self.detail = self.navigator.last_diagnostic.clone();
            }
            return Ok(());
        }
        if self.mode == Mode::Visible && self.pending {
            self.scan(world);
        }
        if self.active.is_none() {
            while let Some(target) = self.queue.pop_front() {
                if self.attempted.len() >= self.limits.max_targets {
                    self.queue.clear();
                    break;
                }
                if !self.attempted.insert(target.pos) {
                    continue;
                }
                self.counters.attempted += 1;
                self.active = Some(Active {
                    process: PredictedMineProcess::new(target.pos, target.kind, frame.tick),
                    prior_stance: None,
                });
                self.empty_since = None;
                self.navigator.clear();
                break;
            }
        }
        if self.active.is_none() {
            if self.pending && self.attempted.len() < self.limits.max_targets {
                let start = *self.empty_since.get_or_insert(frame.tick);
                if frame.tick.saturating_sub(start) < predicted::EMPTY_TICKS {
                    self.status = "Waiting for seed/replay".into();
                    return Ok(());
                }
                self.detail = "no available targets after 200 ticks".into();
            } else {
                self.detail = "bounded target queue exhausted".into();
            }
            self.mode = Mode::Idle;
            self.status = "Idle".into();
            self.navigator.clear();
            return Ok(());
        }
        let mut active = self.active.take().expect("active target exists");
        let pos = active.process.pos;
        let kind = active.process.kind;
        let b = world.read(pos);
        let state = world.state(b.state_id);
        let matching = if self.mode == Mode::Visible {
            self.visible_names.contains(&*state.name)
        } else {
            predicted::matches_kind(kind, &state.name)
        };
        let server_matching = if b.prediction_pending != 0 {
            let state = world.state(b.server_state_id);
            if self.mode == Mode::Visible {
                self.visible_names.contains(&*state.name)
            } else {
                predicted::matches_kind(kind, &state.name)
            }
        } else {
            matching
        };
        let rotation = world.reachable_rotation(pos);
        let tool = (matching && self.settings.flag("allowBreak"))
            .then(|| world.best_tool(b.state_id, true))
            .flatten();
        let standing_on_target = world.player_feet() == [pos[0], pos[1] + 1, pos[2]];
        let ray_hit = !standing_on_target
            && rotation.is_some_and(|r| {
                world
                    .hit(r, frame.reach(), false)
                    .is_some_and(|(p, _)| p == pos)
            });
        let stances = self.stances(
            world,
            pos,
            active.prior_stance.filter(|_| active.process.alternate),
        );
        let at_stance = stances.contains(&world.player_feet()) && rotation.is_some();
        let directive = active.process.step(
            frame.tick,
            Observation {
                loaded: b.loaded != 0,
                air: state.is_air(),
                matching,
                plausible: self.mode == Mode::Predicted
                    && predicted::plausible_host(kind, &frame.identity.dimension, &state.name),
                pending: b.prediction_pending != 0,
                server_matching,
                at_stance,
                reachable: rotation.is_some(),
                ray_hit,
                harvestable: tool.is_some(),
                safe_to_break: !matching || self.safe_final_break(world, pos, &state),
                path_failed: self.navigator.failed,
            },
        );
        match directive {
            Directive::Approach { alternate } => {
                if alternate && active.prior_stance.is_none() {
                    active.prior_stance =
                        self.navigator.dest().or_else(|| stances.first().copied());
                    self.navigator.clear();
                }
                let stances = if alternate {
                    self.stances(world, pos, active.prior_stance)
                } else {
                    stances
                };
                if self
                    .navigator
                    .tick(world, &self.settings, stances, Some(pos))
                {
                    active.prior_stance = Some(world.player_feet());
                }
                self.status = "Pathing".into();
                self.detail = self.navigator.last_diagnostic.clone();
            }
            Directive::Wait => {
                self.navigator.clear();
                world.clear_all_keys();
                self.status = if active.process.phase == Phase::AwaitConfirmation {
                    "Mining"
                } else {
                    "Verifying"
                }
                .into();
                if let Some(rotation) = rotation {
                    world.update_look_target(rotation, true);
                }
            }
            Directive::Attack => {
                self.navigator.clear();
                world.clear_all_keys();
                if let (Some(rotation), Some(tool)) = (rotation, tool) {
                    world.update_look_target(rotation, true);
                    world.select_hotbar_slot(tool.slot);
                    world.set_input_force_state(Input::ClickLeft, true);
                    self.status = "Mining".into();
                }
            }
            Directive::Terminal(outcome) => {
                world.clear_all_keys();
                self.navigator.clear();
                self.counters.record(outcome);
                if matches!(outcome, Outcome::Mismatch | Outcome::Unconfirmed) {
                    self.misses += 1;
                } else if outcome == Outcome::Mined {
                    self.misses = 0;
                }
                self.events.push(json!({"source":self.source.as_ref().map(Source::json),"pos":pos,"kind":kind,"outcome":outcome.name()}));
                if self.misses >= predicted::MISS_LIMIT {
                    self.mode = Mode::Idle;
                    self.queue.clear();
                    self.status = "Idle".into();
                    self.detail = "eight consecutive mismatched/unconfirmed targets".into();
                }
                self.active = None;
                return Ok(());
            }
            Directive::ResourceFailure => {
                self.counters.record(Outcome::Rejected);
                self.events.push(json!({"source":self.source.as_ref().map(Source::json),"pos":pos,"kind":kind,"outcome":"rejected"}));
                self.mode = Mode::Idle;
                self.queue.clear();
                self.navigator.clear();
                self.status = "Idle".into();
                self.detail = "no effective harvestable hotbar tool".into();
                world.clear_all_keys();
                self.active = None;
                return Ok(());
            }
        }
        self.active = Some(active);
        Ok(())
    }
    fn stances(&self, world: &HostWorld, target: Pos, exclude: Option<Pos>) -> Vec<Pos> {
        let mut calc =
            CalculationContext::new(&self.settings, world, world, &world.calculation_facts());
        calc.hooks = Some(Rc::new(Protect(Some(target))));
        let mut result = Vec::new();
        let occupied = world.player_feet();
        let current_exposes = world.reachable_rotation(target).is_some();
        for dy in [-1, 0] {
            // Clearing cardinally adjacent feet/head cells exposes a target
            // face. Distance-two cells can leave an intervening wall untouched.
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let p = [target[0] + dx, target[1] + dy, target[2] + dz];
                let head = [p[0], p[1] + 1, p[2]];
                if Some(p) == exclude
                    || p == target
                    || head == target
                    || (p == occupied && !current_exposes)
                    || !world.contains(p)
                    || !world.contains(head)
                    || !world.is_loaded(p[0], p[2])
                    || !mh::can_walk_on_at(&calc, p[0], p[1] - 1, p[2])
                {
                    continue;
                }
                if [p, head].iter().any(|p| {
                    let state = world.get0(p[0], p[1], p[2]);
                    state.has_fluid()
                        || mh::get_mining_duration_ticks(&calc, p[0], p[1], p[2], &state, false)
                            >= COST_INF
                }) {
                    continue;
                }
                result.push(p);
            }
        }
        let feet = world.position();
        result.sort_by(|a, b| {
            let d = |p: Pos| {
                (0..3)
                    .map(|i| (f64::from(p[i]) + 0.5 - feet[i]).powi(2))
                    .sum::<f64>()
            };
            d(*a).total_cmp(&d(*b)).then(a.cmp(b))
        });
        result.truncate(16);
        result
    }
    fn safe_final_break(&self, world: &HostWorld, pos: Pos, state: &BlockState) -> bool {
        let calc =
            CalculationContext::new(&self.settings, world, world, &world.calculation_facts());
        if mh::avoid_breaking(&calc, pos[0], pos[1], pos[2], state) {
            return false;
        }
        // Read the actual neighboring states even at the run radius: navigation
        // boundaries must not hide a liquid immediately outside that boundary.
        // Unknown neighbors never authorize a final break.
        for [x, y, z] in [
            [1, 0, 0],
            [-1, 0, 0],
            [0, 1, 0],
            [0, -1, 0],
            [0, 0, 1],
            [0, 0, -1],
        ] {
            let b = world.read([pos[0] + x, pos[1] + y, pos[2] + z]);
            if b.loaded == 0 || world.state(b.state_id).has_fluid() {
                return false;
            }
        }
        true
    }
    fn snapshot(&mut self, frame: Option<&Frame>, take_events: bool) -> Value {
        let events = if take_events {
            std::mem::take(&mut self.events)
        } else {
            Vec::new()
        };
        let target = self.active.as_ref().map(|a| a.process.pos);
        let path = self.navigator.path();
        let mut value = json!({"schema":1,"engine":"sunset-baritone/1.1.52",
            "build_marker":option_env!("SUNSET_BARITONE_BUILD_MARKER").unwrap_or("stock"),
            "capabilities":["goto","visible_mine","predicted_mine"],"world":"0","dimension":"",
            "job_generation":"0","lease_generation":"0","tick":frame.map_or("0".into(),|f|f.tick.to_string()),
            "movement":0,"yaw":null,"pitch":null,"selected_slot":null,"attack":null,"use":null,
            "status":self.status,"detail":self.detail,"active":self.mode!=Mode::Idle,
            "counters":self.counters.json(),"events":events,"draw":{"path":path,"next_path":[],
                "goal":self.goal.or(target),"target":target,"breaking":[],"placing":[]}});
        if let Some(e) = self.navigator.executor.as_ref() {
            let mut breaking: Vec<_> = e.to_break().iter().copied().collect();
            breaking.sort_unstable();
            breaking.truncate(128);
            let mut placing: Vec<_> = e.to_place().iter().copied().collect();
            placing.sort_unstable();
            placing.truncate(128);
            value["draw"]["breaking"] = json!(breaking);
            value["draw"]["placing"] = json!(placing);
        }
        if self.status == "Mining" && target.is_some() {
            value["draw"]["breaking"] = json!([target.expect("checked target")]);
        }
        if let Some(identity) = frame.map(|f| &f.identity).or(self.identity.as_ref()) {
            identity.write(&mut value);
        }
        value
    }
}
