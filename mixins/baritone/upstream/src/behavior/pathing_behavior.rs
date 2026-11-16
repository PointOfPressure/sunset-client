// baritone.behavior.PathingBehavior: owns the goal, the path being walked (`current`), the one planned
// to follow it (`next`) and the search in progress. Each tick it lets the executor walk, starts a search
// when there is none, plans the next segment when the current one is nearly over, and cuts a path off
// where it should be. The Java runs the search on another thread; here it runs in slices of time each
// tick (`SEARCH_SLICE`), with the Java's timeouts counted in ticks.
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};
use super::baritone_astar::{AStarPathFinder, Status, COEFFICIENTS};
use super::baritone_avoidance::Avoidance;
use super::baritone_calc_path::Path;
use super::baritone_block_state_interface::SearchBlocks;
use super::baritone_calculation_context::{CalculationContext, CostHooks, SearchMining, ToolSet};
use super::baritone_executor::{ExecEnv, PathExecutor};
use super::baritone_goals::{rc, GoalRef, GoalXZ};
use super::baritone_movement_helper as mh;
use super::baritone_player_context::PlayerContext;
use super::baritone_settings::Settings;
use super::baritone_utils::{pos_label, Pos};
use std::collections::HashMap;
use std::rc::Rc;

/// How long a search may run in one tick. The Java has a thread of its own for it; here it takes this out of the game's 50 ms.
const SEARCH_SLICE: Duration = Duration::from_millis(25);
/// How many nodes between looks at the clock.
const NODES_BETWEEN_CHECKS: usize = 128;

/// What a process asks of the pathing behavior this tick (baritone.api.utils.PathingCommand).
pub enum PathingCommand {
    /// Head for the goal: search if there is no path, and walk it.
    SetGoalAndPath(GoalRef),
    /// Hold still and keep the path.
    RequestPause,
    /// Drop the path and take this goal.
    CancelAndSetGoal(GoalRef),
    /// Drop the path and the goal (`new PathingCommand(null, CANCEL_AND_SET_GOAL)`).
    CancelAndClearGoal,
    /// Take this goal, and drop the path if it no longer leads there.
    RevalidateGoalAndPath(GoalRef),
    /// The same, and search at once.
    ForceRevalidateGoalAndPath(GoalRef),
    SetGoalAndPause(GoalRef),
    /// This process has nothing to say: the next one decides.
    Defer,
}

impl PathingCommand {
    /// How `#proc` prints it.
    pub fn describe(&self) -> String {
        let (kind, goal) = match self {
            PathingCommand::SetGoalAndPath(g) => ("SET_GOAL_AND_PATH", Some(g)),
            PathingCommand::RequestPause => ("REQUEST_PAUSE", None),
            PathingCommand::CancelAndSetGoal(g) => ("CANCEL_AND_SET_GOAL", Some(g)),
            PathingCommand::CancelAndClearGoal => ("CANCEL_AND_SET_GOAL", None),
            PathingCommand::RevalidateGoalAndPath(g) => ("REVALIDATE_GOAL_AND_PATH", Some(g)),
            PathingCommand::ForceRevalidateGoalAndPath(g) => ("FORCE_REVALIDATE_GOAL_AND_PATH", Some(g)),
            PathingCommand::SetGoalAndPause(g) => ("SET_GOAL_AND_PAUSE", Some(g)),
            PathingCommand::Defer => ("DEFER", None),
        };
        format!("PathingCommand{{commandType={kind}, goal={}}}", goal.map_or("null".to_owned(), |g| g.label()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathEvent {
    CalcStarted,
    CalcFinishedNowExecuting,
    CalcFailed,
    NextSegmentCalcStarted,
    NextSegmentCalcFinished,
    NextCalcFailed,
    DiscardNext,
    ContinuingOntoPlannedNext,
    SplicingOntoNextEarly,
    AtGoal,
    Canceled,
    PathFinishedNextStillCalculating,
}

/// A search being worked on.
struct Search {
    finder: AStarPathFinder,
    start: Pos,
    goal: GoalRef,
    talk_about_it: bool,
    started_tick: u64,
    started_at: Instant,
    /// `claimSearchCaches`: they last as long as this search.
    block_cache: Rc<SearchBlocks>,
    mining_cache: Rc<SearchMining>,
    primary_timeout_ticks: u64,
    failure_timeout_ticks: u64,
    is_next_segment: bool,
    cancelled: bool,
    hooks: Option<Rc<dyn CostHooks>>,
}

pub struct PathingBehavior {
    /// `logDirect`: lines to say whatever chatDebug is, for `Core::path_tick`.
    pub direct: Vec<String>,
    /// Set when a path ended in the goal; `Core::path_tick` takes it, to leave the world when disconnectOnArrival says so.
    pub arrived_at_goal: bool,
    pub current: Option<PathExecutor>,
    pub next: Option<PathExecutor>,
    goal: Option<GoalRef>,
    ticks_elapsed_so_far: i32,
    start_position: Option<Pos>,
    safe_to_cancel: bool,
    pause_requested_last_tick: bool,
    unpaused_last_tick: bool,
    paused_this_tick: bool,
    cancel_requested: bool,
    calc_failed_last_tick: bool,
    in_progress: Option<Search>,
    expected_segment_start: Pos,
    /// `context`: what the process in control overrides of the calculation context, for the searches.
    hooks: Option<Rc<dyn CostHooks>>,
    events: Vec<PathEvent>,
    /// The debug lines of the Java's `logDebug`, for the caller to print when chatDebug is on.
    pub log: Vec<String>,
}

impl Default for PathingBehavior {
    fn default() -> Self {
        Self::new()
    }
}

impl PathingBehavior {
    pub fn new() -> Self {
        Self {
            direct: Vec::new(),
            arrived_at_goal: false,
            current: None,
            next: None,
            goal: None,
            ticks_elapsed_so_far: 0,
            start_position: None,
            safe_to_cancel: false,
            pause_requested_last_tick: false,
            unpaused_last_tick: false,
            paused_this_tick: false,
            cancel_requested: false,
            calc_failed_last_tick: false,
            in_progress: None,
            expected_segment_start: [0, 0, 0],
            hooks: None,
            events: Vec::new(),
            log: Vec::new(),
        }
    }

    fn debug(&mut self, text: impl Into<String>) {
        self.log.push(text.into());
    }

    fn queue(&mut self, e: PathEvent) {
        self.events.push(e);
    }

    pub fn goal(&self) -> Option<&GoalRef> {
        self.goal.as_ref()
    }

    pub fn calc_failed_last_tick(&self) -> bool {
        self.calc_failed_last_tick
    }

    pub fn has_path(&self) -> bool {
        self.current.is_some()
    }

    pub fn is_pathing(&self) -> bool {
        self.has_path() && !self.paused_this_tick
    }

    /// `onPlayerSprintState`: `None` when no path is being walked.
    pub fn sprint_state(&self) -> Option<bool> {
        self.current.as_ref().filter(|_| self.is_pathing()).map(PathExecutor::is_sprinting)
    }

    pub fn is_calculating(&self) -> bool {
        self.in_progress.is_some()
    }

    /// `current != null || inProgress != null`: anything going on.
    pub fn is_busy(&self) -> bool {
        self.current.is_some() || self.in_progress.is_some()
    }

    /// What the search being run has so far, for the renderer: its best path so far, and the path to the node it considered last.
    pub fn in_progress_paths(&self) -> Option<(Option<super::baritone_calc_path::Path>, Option<super::baritone_calc_path::Path>)> {
        self.in_progress.as_ref().map(|s| (s.finder.best_so_far(0), s.finder.path_to_most_recent_node_considered()))
    }

    #[allow(dead_code)]
    pub fn in_progress_start(&self) -> Option<Pos> {
        self.in_progress.as_ref().map(|s| s.start)
    }

    pub fn expected_segment_start(&self) -> Pos {
        self.expected_segment_start
    }

    pub fn request_pause(&mut self) {
        self.pause_requested_last_tick = true;
    }

    pub fn secret_internal_set_goal(&mut self, goal: Option<GoalRef>) {
        self.goal = goal;
    }

    pub fn is_safe_to_cancel(&self) -> bool {
        if self.current.is_none() {
            return true;
        }
        self.safe_to_cancel
    }

    /// `ticksRemainingInSegment`.
    pub fn ticks_remaining_in_segment(&self) -> Option<f64> {
        self.current.as_ref().map(|c| c.path.ticks_remaining_from(c.position()))
    }

    /// `estimatedTicksToGoal`.
    pub fn estimated_ticks_to_goal(&mut self, feet: Pos) -> Option<f64> {
        let goal = self.goal.clone()?;
        let start = self.start_position?;
        if goal.is_in_goal(feet) {
            self.reset_estimated_ticks_to_goal(self.expected_segment_start);
            return Some(0.0);
        }
        if self.ticks_elapsed_so_far == 0 {
            return None;
        }
        let current = goal.heuristic(feet);
        let start_h = goal.heuristic(start);
        if current == start_h {
            // Cannot check above: current and start can be equal even if the positions are not.
            return None;
        }
        Some((current - goal.heuristic0()).abs() * f64::from(self.ticks_elapsed_so_far) / (start_h - current).abs())
    }

    fn reset_estimated_ticks_to_goal(&mut self, start: Pos) {
        self.ticks_elapsed_so_far = 0;
        self.start_position = Some(start);
    }

    /// `pathStart`: where a path should start from: the feet, or the block that holds the player up.
    pub fn path_start(ctx: &dyn PlayerContext, calc: &CalculationContext) -> Pos {
        let feet = ctx.player_feet();
        if !mh::can_walk_on_at(calc, feet[0], feet[1] - 1, feet[2]) {
            if ctx.on_ground() {
                let p = ctx.position();
                let mut closest: Vec<Pos> = Vec::new();
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        closest.push([feet[0] + dx, feet[1], feet[2] + dz]);
                    }
                }
                closest.sort_by(|a, b| {
                    let d = |pos: &Pos| (f64::from(pos[0]) + 0.5 - p[0]).powi(2) + (f64::from(pos[2]) + 0.5 - p[2]).powi(2);
                    d(a).total_cmp(&d(b))
                });
                for possible_support in closest.iter().take(4) {
                    let x_dist = (f64::from(possible_support[0]) + 0.5 - p[0]).abs();
                    let z_dist = (f64::from(possible_support[2]) + 0.5 - p[2]).abs();
                    if x_dist > 0.8 && z_dist > 0.8 {
                        continue;
                    }
                    let [x, y, z] = *possible_support;
                    if mh::can_walk_on_at(calc, x, y - 1, z) && mh::can_walk_through_at(calc, x, y, z) && mh::can_walk_through_at(calc, x, y + 1, z) {
                        return *possible_support;
                    }
                }
            } else if mh::can_walk_on_at(calc, feet[0], feet[1] - 2, feet[2]) {
                return [feet[0], feet[1] - 1, feet[2]];
            }
        }
        feet
    }

    /// The first half of onTick: dispatch the events of the last tick and work out where a path would start.
    pub fn begin_tick(&mut self, ctx: &dyn PlayerContext, settings: &Settings, tools: &dyn ToolSet) {
        self.dispatch_events();
        let facts = ctx.calculation_facts();
        let calc = CalculationContext::new(settings, ctx.bsi(), tools, &facts);
        self.expected_segment_start = Self::path_start(ctx, &calc);
    }

    /// The second half of onTick, after the control manager's preTick: walk the path.
    pub fn run_tick(&mut self, ctx: &dyn PlayerContext, settings: &Settings, tools: &dyn ToolSet, ticks: u64) {
        let facts = ctx.calculation_facts();
        let calc = CalculationContext::new(settings, ctx.bsi(), tools, &facts);
        self.tick_path(ctx, settings, &calc, ticks);
        self.ticks_elapsed_so_far += 1;
        self.dispatch_events();
    }

    /// PathingControlManager.preTick, given what `executeProcesses` returned.
    pub fn pre_tick(&mut self, ctx: &dyn PlayerContext, settings: &Settings, command: Option<&PathingCommand>, hooks: Option<Rc<dyn CostHooks>>, real_process_changed: bool, ticks: u64) {
        let Some(command) = command else {
            self.cancel_segment_if_safe(ctx);
            self.secret_internal_set_goal(None);
            return;
        };
        if real_process_changed && !matches!(command, PathingCommand::RequestPause) {
            // Control changed from a real process to another real process that wants to do something: get
            // rid of the in-progress stuff from the last process.
            self.cancel_segment_if_safe(ctx);
        }
        match command {
            PathingCommand::SetGoalAndPause(g) => {
                self.secret_internal_set_goal_and_path(ctx, settings, Some(g.clone()), hooks.clone(), ticks);
                self.request_pause();
            }
            PathingCommand::RequestPause => self.request_pause(),
            PathingCommand::CancelAndSetGoal(g) => {
                self.secret_internal_set_goal(Some(g.clone()));
                self.cancel_segment_if_safe(ctx);
            }
            PathingCommand::CancelAndClearGoal => {
                self.secret_internal_set_goal(None);
                self.cancel_segment_if_safe(ctx);
            }
            PathingCommand::ForceRevalidateGoalAndPath(g) | PathingCommand::RevalidateGoalAndPath(g) => {
                if !self.is_pathing() && !self.is_calculating() {
                    self.secret_internal_set_goal_and_path(ctx, settings, Some(g.clone()), hooks.clone(), ticks);
                }
            }
            PathingCommand::SetGoalAndPath(g) => {
                self.secret_internal_set_goal_and_path(ctx, settings, Some(g.clone()), hooks.clone(), ticks);
            }
            PathingCommand::Defer => {}
        }
    }

    /// PathingControlManager.postTick: after the path has been walked, cancel and recalculate if the goal changed.
    pub fn post_tick(&mut self, ctx: &dyn PlayerContext, settings: &Settings, command: Option<&PathingCommand>, hooks: Option<Rc<dyn CostHooks>>, ticks: u64) {
        match command {
            Some(PathingCommand::ForceRevalidateGoalAndPath(g)) => {
                if self.force_revalidate(g) || self.revalidate_goal(g) {
                    self.soft_cancel_if_safe();
                }
                self.secret_internal_set_goal_and_path(ctx, settings, Some(g.clone()), hooks.clone(), ticks);
            }
            Some(PathingCommand::RevalidateGoalAndPath(g)) => {
                if settings.flag("cancelOnGoalInvalidation") && self.revalidate_goal(g) {
                    self.soft_cancel_if_safe();
                }
                self.secret_internal_set_goal_and_path(ctx, settings, Some(g.clone()), hooks.clone(), ticks);
            }
            _ => {}
        }
    }

    /// PathingControlManager.forceRevalidate.
    fn force_revalidate(&self, new_goal: &GoalRef) -> bool {
        let Some(current) = &self.current else { return false };
        if new_goal.is_in_goal(current.path.dest()) {
            return false;
        }
        new_goal.label() != current.path.goal.label()
    }

    /// PathingControlManager.revalidateGoal: the path used to end in the goal and no longer would.
    fn revalidate_goal(&self, new_goal: &GoalRef) -> bool {
        let Some(current) = &self.current else { return false };
        let end = current.path.dest();
        current.path.goal.is_in_goal(end) && !new_goal.is_in_goal(end)
    }

    /// One whole tick with no control manager in between (the tests).
    #[allow(dead_code)]
    pub fn on_tick(&mut self, ctx: &dyn PlayerContext, settings: &Settings, tools: &dyn ToolSet, ticks: u64) {
        self.begin_tick(ctx, settings, tools);
        self.run_tick(ctx, settings, tools, ticks);
    }

    fn dispatch_events(&mut self) {
        let curr = std::mem::take(&mut self.events);
        self.calc_failed_last_tick = curr.contains(&PathEvent::CalcFailed);
    }

    fn tick_path(&mut self, ctx: &dyn PlayerContext, settings: &Settings, calc: &CalculationContext, ticks: u64) {
        self.paused_this_tick = false;
        if self.pause_requested_last_tick && self.safe_to_cancel {
            self.pause_requested_last_tick = false;
            if self.unpaused_last_tick {
                ctx.clear_all_keys();
                ctx.stop_breaking_block();
            }
            self.unpaused_last_tick = false;
            self.paused_this_tick = true;
            return;
        }
        self.unpaused_last_tick = true;
        if self.cancel_requested {
            self.cancel_requested = false;
            ctx.clear_all_keys();
        }
        // Work on the search in progress.
        self.search_tick(ctx, settings, calc, ticks);
        if let Some(search) = &mut self.in_progress {
            let calc_from = search.start;
            let current_best = search.finder.best_so_far(0);
            let feet = ctx.player_feet();
            let expected = self.expected_segment_start;
            let current_ends_in_start = self.current.as_ref().is_some_and(|c| c.path.dest() == calc_from);
            // If current ends in inProgress's start, or current starts in the feet or the path start, all is well.
            if !current_ends_in_start
                && calc_from != feet
                && calc_from != expected
                && current_best.as_ref().is_none_or(|b| !b.positions.contains(&feet) && !b.positions.contains(&expected))
            {
                search.cancelled = true; // cancellation does not dispatch any events
                self.in_progress = None;
            }
        }
        if self.current.is_none() {
            return;
        }
        let best = self.in_progress.as_ref().and_then(|s| s.finder.best_so_far(0));
        let in_progress = self.in_progress.is_some();
        let mut current = self.current.take().expect("checked above");
        let e = ExecEnv { ctx, calc, settings, in_progress_best: best.as_ref(), in_progress, ticks };
        self.safe_to_cancel = current.on_tick(&e);
        let (failed, finished) = (current.failed(), current.finished());
        for line in current.log.drain(..) {
            self.log.push(line);
        }
        if failed || finished {
            self.finish_current(ctx, settings, calc, ticks, current);
            return;
        }
        if self.safe_to_cancel {
            if let Some(next) = self.next.as_mut() {
                if next.snipsnapifpossible(&e) {
                    self.debug("Splicing into planned next path early...");
                    self.queue(PathEvent::SplicingOntoNextEarly);
                    let mut now = self.next.take().expect("checked above");
                    now.on_tick(&e);
                    self.current = Some(now);
                    return;
                }
            }
        }
        if settings.flag("splicePath") {
            if let Some(spliced) = current.try_splice(self.next.as_ref(), &e) {
                current = spliced;
            }
        }
        if let Some(next) = &self.next {
            if current.path.dest() == next.path.dest() {
                self.next = None;
            }
        }
        let dest = current.path.dest();
        let remaining = current.path.ticks_remaining_from(current.position());
        self.current = Some(current);
        if self.in_progress.is_some() || self.next.is_some() {
            return;
        }
        let Some(goal) = self.goal.clone() else { return };
        if goal.is_in_goal(dest) {
            return;
        }
        if remaining < settings.float("planningTickLookahead") {
            self.debug("Path almost over. Planning ahead...");
            self.queue(PathEvent::NextSegmentCalcStarted);
            self.find_path(ctx, settings, dest, false, ticks, true);
        }
    }

    /// The part of tickPath after the executor says it failed or finished.
    fn finish_current(&mut self, ctx: &dyn PlayerContext, settings: &Settings, calc: &CalculationContext, ticks: u64, _ended: PathExecutor) {
        let feet = ctx.player_feet();
        self.current = None;
        if self.goal.as_ref().is_none_or(|g| g.is_in_goal(feet)) {
            self.debug(format!("All done. At {}", self.goal.as_ref().map_or("None".into(), |g| g.label())));
            self.queue(PathEvent::AtGoal);
            self.next = None;
            self.arrived_at_goal = true;
            return;
        }
        let expected = self.expected_segment_start;
        if let Some(next) = &self.next {
            if !next.path.positions.contains(&feet) && !next.path.positions.contains(&expected) {
                self.debug("Discarding next path as it does not contain current position");
                self.queue(PathEvent::DiscardNext);
                self.next = None;
            }
        }
        if let Some(mut next) = self.next.take() {
            self.debug("Continuing on to planned next path");
            self.queue(PathEvent::ContinuingOntoPlannedNext);
            let e = ExecEnv { ctx, calc, settings, in_progress_best: None, in_progress: self.in_progress.is_some(), ticks };
            next.on_tick(&e); // do not waste a tick doing nothing, get started right away
            self.current = Some(next);
            return;
        }
        if self.in_progress.is_some() {
            self.queue(PathEvent::PathFinishedNextStillCalculating);
            return;
        }
        self.queue(PathEvent::CalcStarted);
        let start = self.expected_segment_start;
        self.find_path(ctx, settings, start, true, ticks, false);
    }

    /// secretInternalSetGoalAndPath: set the goal, and start a search if there is nothing going on.
    pub fn secret_internal_set_goal_and_path(&mut self, ctx: &dyn PlayerContext, settings: &Settings, goal: Option<GoalRef>, hooks: Option<Rc<dyn CostHooks>>, ticks: u64) -> bool {
        self.goal = goal;
        self.hooks = hooks;
        let Some(goal) = self.goal.clone() else { return false };
        if goal.is_in_goal(ctx.player_feet()) {
            return false;
        }
        if self.current.is_some() || self.in_progress.is_some() {
            return false;
        }
        self.queue(PathEvent::CalcStarted);
        let start = self.expected_segment_start;
        self.find_path(ctx, settings, start, true, ticks, false);
        true
    }

    /// findPathInNewThread: set a search going (it is worked on in `search_tick`).
    fn find_path(&mut self, ctx: &dyn PlayerContext, settings: &Settings, start: Pos, talk_about_it: bool, ticks: u64, is_next_segment: bool) {
        assert!(self.in_progress.is_none(), "Already doing it");
        let Some(goal) = self.goal.clone() else {
            self.debug("no goal");
            return;
        };
        let (mut primary_ms, mut failure_ms) = if self.current.is_none() {
            (settings.int("primaryTimeoutMS"), settings.int("failureTimeoutMS"))
        } else {
            (settings.int("planAheadPrimaryTimeoutMS"), settings.int("planAheadFailureTimeoutMS"))
        };
        if settings.flag("slowPath") {
            self.debug(format!("slowPath is on, path timeout will be {}ms instead of {primary_ms}ms", settings.int("slowPathTimeoutMS")));
            primary_ms = settings.int("slowPathTimeoutMS");
            failure_ms = primary_ms;
        }
        let previous = self.current.as_ref().map(|c| c.path.clone());
        let finder = self.create_pathfinder(ctx, settings, start, &goal, previous.as_ref());
        if talk_about_it {
            self.debug(format!("Starting to search for path from {} to {}", pos_label(start), goal.label()));
        }
        self.in_progress = Some(Search {
            finder,
            start,
            goal,
            talk_about_it,
            started_tick: ticks,
            started_at: Instant::now(),
            block_cache: Rc::default(),
            mining_cache: Rc::default(),
            primary_timeout_ticks: (primary_ms.max(0) as u64).div_ceil(50),
            failure_timeout_ticks: (failure_ms.max(0) as u64).div_ceil(50),
            is_next_segment,
            cancelled: false,
            hooks: self.hooks.clone(),
        });
    }

    /// createPathfinder.
    fn create_pathfinder(&mut self, ctx: &dyn PlayerContext, settings: &Settings, start: Pos, goal: &GoalRef, previous: Option<&Path>) -> AStarPathFinder {
        let mut transformed = goal.clone();
        if settings.flag("simplifyUnloadedYCoord") {
            if let Some(pos) = goal.render_pos() {
                if !ctx.bsi().world_contains_loaded_chunk(pos[0], pos[2]) {
                    transformed = rc(GoalXZ(pos[0], pos[2]));
                    self.debug(format!("Simplifying {} to GoalXZ due to distance", goal.label()));
                }
            }
        }
        // Favoring: the previous path costs backtrackCostFavoringCoefficient times as much.
        let mut favoring: HashMap<Pos, f64> = HashMap::new();
        let coeff = settings.float("backtrackCostFavoringCoefficient");
        if coeff != 1.0 {
            if let Some(previous) = previous {
                for p in &previous.positions {
                    favoring.insert(*p, coeff);
                }
            }
        }
        // Mobs and mob spawners make their surroundings dearer.
        for avoid in Avoidance::create(settings, &|| ctx.mobs_to_avoid(), &|| ctx.mob_spawners_near()) {
            avoid.apply_spherical(&mut favoring);
        }
        self.debug(format!("Favoring size: {}", favoring.len()));
        let feet = ctx.player_feet();
        let mut real_start = start;
        let sub = [feet[0] - start[0], feet[1] - start[1], feet[2] - start[2]];
        if sub[1] == 0 && sub[0].abs() <= 1 && sub[2].abs() <= 1 {
            real_start = feet;
        }
        AStarPathFinder::new(real_start, start, transformed, favoring, settings.int("pathingMaxChunkBorderFetch").max(1) as usize, settings.flag("minimumImprovementRepropagation")).with_map_size(settings.int("pathingMapDefaultSize").max(0) as usize)
    }

    /// Work on the search for a slice, and finish it if it is done or out of time.
    fn search_tick(&mut self, ctx: &dyn PlayerContext, settings: &Settings, calc: &CalculationContext, ticks: u64) {
        let Some(search) = &mut self.in_progress else { return };
        if search.cancelled {
            self.in_progress = None;
            return;
        }
        let hooked = calc.clone().with_hooks(search.hooks.clone()).with_mining_cache(search.mining_cache.clone());
        let calc = &hooked;
        calc.bsi.claim_cache(search.block_cache.clone());
        let begun = Instant::now();
        let ran = catch_unwind(AssertUnwindSafe(|| {
            if settings.flag("slowPath") {
                // slowPath sleeps slowPathTimeDelayMS before each node, which leaves a tick room for this many.
                return search.finder.run(calc, (50 / settings.int("slowPathTimeDelayMS").max(1) as usize).max(1));
            }
            loop {
                let status = search.finder.run(calc, NODES_BETWEEN_CHECKS);
                if !matches!(status, Status::Running) || begun.elapsed() >= SEARCH_SLICE {
                    break status;
                }
            }
        }));
        calc.bsi.release_cache();
        // AbstractNodeCostSearch.calculate: an exception ends this search, and only this search.
        let Ok(status) = ran else {
            self.in_progress = None;
            self.direct.push("Pathing exception: the search stopped on an error, which is in the game's log".to_owned());
            return;
        };
        let elapsed = ticks.saturating_sub(search.started_tick);
        let timed_out = elapsed >= search.failure_timeout_ticks || (!search.finder.failing && elapsed >= search.primary_timeout_ticks);
        let took = format!("Took {}ms, {} movements considered", search.started_at.elapsed().as_millis(), search.finder.num_movements_considered);
        let mut said: Vec<String> = Vec::new();
        let result: Option<Option<Path>> = match status {
            Status::Done(p) => {
                said.push(took);
                Some(Some(p))
            }
            Status::Running if !timed_out => None,
            _ => {
                match search.finder.best_so_far_report() {
                    Ok(coefficient) => {
                        said.push(format!("A* cost coefficient {coefficient:?}"));
                        said.push(took);
                    }
                    Err(best) => {
                        said.push(format!("Even with a cost coefficient of {:?}, I couldn't get more than {best} blocks", COEFFICIENTS[COEFFICIENTS.len() - 1]));
                        said.push("No path found =(".to_owned());
                        if settings.flag("desktopNotifications") {
                            super::baritone_notification::notify("No path found =(", true);
                        }
                    }
                }
                Some(search.finder.best_so_far(search.finder.num_nodes))
            }
        };
        let Some(path) = result else { return };
        let search = self.in_progress.take().expect("checked above");
        for line in said {
            self.debug(line);
        }
        self.finish_search(ctx, settings, calc, search, path);
    }

    /// AbstractNodeCostSearch.calculate's tail, then PathingBehavior's handling of the result.
    fn finish_search(&mut self, ctx: &dyn PlayerContext, settings: &Settings, calc: &CalculationContext, search: Search, path: Option<Path>) {
        let processed = path.map(|p| {
            let mut p = p.post_process(calc);
            if let Some(note) = p.note.take() {
                self.debug(note);
            }
            let previous = p.length();
            let p = p.cutoff_at_loaded_chunks(settings.flag("cutoffAtLoadBoundary"), &|x, z| calc.bsi.world_contains_loaded_chunk(x, z));
            if p.length() < previous {
                self.debug("Cutting off path at edge of loaded chunks");
                self.debug(format!("Length decreased by {}", previous - p.length()));
            } else {
                self.debug("Path ends within loaded chunks");
            }
            let previous = p.length();
            let p = p.static_cutoff(settings.int("pathCutoffMinimumLength").max(0) as usize, settings.float("pathCutoffFactor"));
            if p.length() < previous {
                self.debug(format!("Static cutoff {previous} to {}", p.length()));
            }
            p
        });
        let executor = processed.map(PathExecutor::new);
        if self.current.is_none() && !search.is_next_segment {
            match executor {
                Some(executor) => {
                    if executor.path.positions.contains(&self.expected_segment_start) {
                        self.queue(PathEvent::CalcFinishedNowExecuting);
                        self.reset_estimated_ticks_to_goal(search.start);
                        self.log_found(&search, &executor);
                        self.current = Some(executor);
                    } else {
                        self.debug("Warning: discarding orphan path segment with incorrect start");
                    }
                }
                None => self.queue(PathEvent::CalcFailed),
            }
        } else if self.current.is_some() {
            if self.next.is_none() {
                match executor {
                    Some(executor) => {
                        if self.current.as_ref().is_some_and(|c| executor.path.src() == c.path.dest()) {
                            self.queue(PathEvent::NextSegmentCalcFinished);
                            self.next = Some(executor);
                        } else {
                            self.debug("Warning: discarding orphan next segment with incorrect start");
                        }
                    }
                    None => self.queue(PathEvent::NextCalcFailed),
                }
            } else {
                self.debug("Warning: PathingBehavior illegal state! Discarding invalid path!");
            }
        }
        let _ = ctx;
    }

    fn log_found(&mut self, search: &Search, executor: &PathExecutor) {
        if !search.talk_about_it {
            return;
        }
        if search.goal.is_in_goal(executor.path.dest()) {
            self.debug(format!("Finished finding a path from {} to {}. {} nodes considered", pos_label(search.start), search.goal.label(), executor.path.num_nodes));
        } else {
            self.debug(format!("Found path segment from {} towards {}. {} nodes considered", pos_label(search.start), search.goal.label(), executor.path.num_nodes));
        }
    }

    // ---- cancelling

    /// secretInternalSegmentCancel.
    pub fn secret_internal_segment_cancel(&mut self, ctx: &dyn PlayerContext) {
        self.queue(PathEvent::Canceled);
        if let Some(s) = &mut self.in_progress {
            s.cancelled = true;
        }
        self.in_progress = None;
        if self.current.is_some() {
            self.current = None;
            self.next = None;
            ctx.clear_all_keys();
            ctx.stop_breaking_block();
        }
    }

    pub fn cancel_segment_if_safe(&mut self, ctx: &dyn PlayerContext) -> bool {
        if self.is_safe_to_cancel() {
            self.secret_internal_segment_cancel(ctx);
            return true;
        }
        false
    }

    /// softCancelIfSafe.
    pub fn soft_cancel_if_safe(&mut self) {
        if let Some(s) = &mut self.in_progress {
            s.cancelled = true; // only cancel ours
        }
        self.in_progress = None;
        if !self.is_safe_to_cancel() {
            return;
        }
        self.current = None;
        self.next = None;
        self.cancel_requested = true;
    }

    /// forceCancel (the processes are cancelled by the control manager).
    pub fn force_cancel(&mut self, ctx: &dyn PlayerContext) {
        self.secret_internal_segment_cancel(ctx);
        self.in_progress = None;
    }
}

