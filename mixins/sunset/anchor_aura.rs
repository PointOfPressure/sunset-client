// Original native Anchor Aura. Each use waits for an observed block-state
// change; the client never predicts charge, removal, or explosion damage.
use super::sunset::{Category, Module, cancel_turns, gui_open, hold, look_at, try_turn_then, with};
use super::sunset_anchor_state::{self as order, Step, Wait};
use super::sunset_explosive_aura::Rating;
use super::sunset_explosive_damage::{
    Blast, clear_line, local_damage, safety_damage, target_damage,
};
use super::sunset_explosive_policy::{Budget, Limits, in_range, safe};
use super::sunset_util::{self as util, OFFHAND, Target};
use super::{Dim, Game, Screen};
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use horizon_world::registry::BlockStateId;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const NAME: &str = "Anchor Aura";
const SCAN: usize = 128;
const RADIUS: i32 = 5;
const SIDE: i32 = RADIUS * 2 + 1;
const VOLUME: usize = (SIDE * SIDE * SIDE) as usize;
const MAX_EVALUATED: usize = 4;
const MAX_FAILED: usize = 32;
const ACK_TIMEOUT: u64 = 30;

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new(NAME, Category::Combat,
        "Observed anchor sequence with target estimates and selected self-damage model. Hidden server rules remain unknown; friends use unmitigated damage.")
        .bool("Place", true)
        .text("Entities", "player")
        .num("Target range", 10.0, 1.0, 16.0, 0.5)
        .num("Range", 4.5, 1.0, 5.0, 0.1)
        .num("Walls range", 0.0, 0.0, 5.0, 0.1)
        .num("Min damage", 6.0, 0.0, 36.0, 0.5)
        .num("Max self damage", 6.0, 0.0, 36.0, 0.5)
        .pick("Damage model", &["Observed equipment", "Unmitigated"])
        .num("Max friend damage", 0.0, 0.0, 36.0, 0.5)
        .bool("Anti suicide", true)
        .num("Min health remaining", 4.0, 0.0, 36.0, 0.5)
        .num("Action delay", 2.0, 1.0, 20.0, 1.0)
        .bool("Pause on use", true)
        .bool("Anti bot", true)
        .bool("Ignore teams", true)
        .bool("Swing", true)]
}

#[derive(Clone, Copy)]
struct Action {
    pos: [i32; 3],
    before: BlockStateId,
    step: Step,
    slot: usize,
    clicked: [i32; 3],
    face: i32,
    hit: [f64; 3],
    target: u32,
}
#[derive(Clone, Copy)]
struct Pending {
    action: Action,
    token: u64,
    queued: u64,
    queued_real: Instant,
    sent: Option<u64>,
}
struct Plan {
    action: Action,
    rating: Rating,
    ticks: u64,
    module: Module,
}
#[derive(Default)]
struct State {
    session: Option<(i32, Dim)>,
    last_tick: u64,
    cursor: usize,
    origin: Option<[i32; 3]>,
    anchors: Vec<[i32; 3]>,
    pending: Option<Pending>,
    failed: Vec<([i32; 3], BlockStateId)>,
    token: u64,
    next_action: u64,
    exhausted: bool,
    plan: Option<Plan>,
    phase: &'static str,
    reason: &'static str,
    last_active_phase: &'static str,
    last_active_reason: &'static str,
    sent_count: u64,
    ack_count: u64,
    last_sent: Option<u64>,
    last_ack: Option<u64>,
    evaluated: usize,
    last_prediction: Option<(f64, f64, f64)>,
}
impl State {
    fn status(&mut self, phase: &'static str, why: &'static str) {
        self.phase = phase;
        self.reason = why;
        if phase != "paused" {
            self.last_active_phase = phase;
            self.last_active_reason = why;
        }
    }
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}

pub(super) fn reset() {
    super::sunset_combat_control::release(NAME);
    cancel_turns(NAME);
    state(|s| {
        let token = s.token.wrapping_add(1);
        let (sent_count, ack_count, last_sent, last_ack) =
            (s.sent_count, s.ack_count, s.last_sent, s.last_ack);
        let (last_active_phase, last_active_reason, last_prediction) =
            (s.last_active_phase, s.last_active_reason, s.last_prediction);
        *s = State {
            token,
            sent_count,
            ack_count,
            last_sent,
            last_ack,
            last_active_phase,
            last_active_reason,
            last_prediction,
            phase: "idle",
            reason: "reset",
            ..State::default()
        };
    });
}

fn reason(phase: &'static str, why: &'static str) {
    state(|s| s.status(phase, why));
}
pub(super) fn diagnostics() -> String {
    state(|s| {
        format!(
            "Anchor phase={} reason={} last_active={}:{} sent={} ack={} last_sent={:?} last_ack={:?} pending={:?} plan={:?} damage=(self,target,friend){:?} scanned={} evaluated={} blocked={}",
            s.phase,
            s.reason,
            s.last_active_phase,
            s.last_active_reason,
            s.sent_count,
            s.ack_count,
            s.last_sent,
            s.last_ack,
            s.pending.map(|p| (p.action.step, p.action.pos, p.sent)),
            s.plan.as_ref().map(|p| (
                p.action.step,
                p.action.target,
                p.rating.target_damage,
                p.rating.self_damage
            )),
            s.last_prediction,
            s.cursor,
            s.evaluated,
            s.failed.len()
        )
    })
}

fn ready(game: &Game<'_>, m: &Module) -> bool {
    m.on && super::sunset_explosive_aura::can_act(game)
        && game.native_playing()
        && matches!(game.screen, Screen::Playing)
        && !gui_open()
        && game.remote.is_some()
        && !game.anchor_works()
        && !game.spectator()
        && game.selected < 9
        && [
            m.n("Range"),
            m.n("Walls range"),
            m.n("Target range"),
            m.n("Action delay"),
        ]
        .iter()
        .all(|n| n.is_finite() && *n >= 0.0)
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health() > 0.0)
        && (!m.b("Pause on use") || (game.using.is_none() && game.mining.is_none() && !game.right))
        && !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft)
        && super::sunset::CRIT_STAGE.load(Ordering::Relaxed) == 0
        && !with(|s| s.is_on("Blink"))
}

fn block(game: &Game<'_>, p: [i32; 3]) -> BlockStateId {
    game.world.block(p[0], p[1], p[2])
}
fn charges(game: &Game<'_>, p: [i32; 3]) -> Option<u8> {
    let b = block(game, p);
    if !game.is_anchor(b) {
        return None;
    }
    game.pack
        .assets
        .registry
        .state(b)?
        .properties
        .get("charges")?
        .parse::<u8>()
        .ok()
        .filter(|n| *n <= 4)
}
fn fuel(game: &Game<'_>, slot: usize) -> bool {
    if slot == OFFHAND {
        game.offhand.as_ref()
    } else {
        game.inventory.get(slot).and_then(|s| s.as_ref())
    }
    .is_some_and(|s| s.count > 0 && util::short(&s.item) == "glowstone")
}
fn item(game: &Game<'_>, slot: usize, name: &str) -> bool {
    if slot == OFFHAND {
        game.offhand.as_ref()
    } else {
        game.inventory.get(slot).and_then(|s| s.as_ref())
    }
    .is_some_and(|s| s.count > 0 && util::short(&s.item) == name)
}
fn slot_for(game: &Game<'_>, step: Step) -> Option<usize> {
    match step {
        Step::Place => util::find_held(game, |n| n == "respawn_anchor")
            .filter(|&s| item(game, s, "respawn_anchor")),
        Step::Charge => util::find_held(game, |n| n == "glowstone").filter(|&s| fuel(game, s)),
        Step::Detonate => {
            let selected = game.selected.min(8);
            if !fuel(game, selected) {
                Some(selected)
            } else {
                (0..9).find(|&s| !fuel(game, s))
            }
        }
    }
}

// Bound population processing as well as explosion work. Overflow fails closed
// so a crowded world cannot silently drop a protected player from safety checks.
fn players(game: &Game<'_>, m: &Module) -> Option<(Vec<Target>, Vec<Target>)> {
    let eye = game.eye_position()?;
    let own = game.remote.as_ref()?.entity_id as u32;
    let friends = with(|s| s.friends.clone());
    let (mut targets, mut protected) = (Vec::new(), Vec::new());
    for (i, mob) in game.mobs.iter().enumerate() {
        if i >= 256 {
            return None;
        }
        if mob.id == own || !mob.alive() || mob.removed {
            continue;
        }
        let t = util::mob_target(mob);
        if !t
            .pos
            .iter()
            .chain(t.lo.iter())
            .chain(t.hi.iter())
            .all(|n| n.is_finite())
        {
            return None;
        }
        if !in_range(eye, t.pos, 26.0) {
            continue;
        }
        let name = game.remote_player_name(mob.id);
        let friend = name.is_some_and(|n| friends.iter().any(|f| f.eq_ignore_ascii_case(n)));
        // Unknown names may hide a friend: protect rather than attack them.
        if friend || (t.player && name.is_none()) || util::spared(game, m, mob.id, t.player) {
            if protected.len() == 16 {
                return None;
            }
            protected.push(t);
        } else if in_range(eye, t.pos, m.n("Target range"))
            && order::target_available(f64::from(t.health), mob.hurt_time > 0)
            && super::sunset_aura::entity_listed(m.t("Entities"), &t)
        {
            targets.push(t);
        }
    }
    util::sort_targets(game, &mut targets, "Lowest distance");
    targets.truncate(4);
    Some((targets, protected))
}

fn limits(m: &Module) -> Limits {
    Limits {
        min_target: m.n("Min damage"),
        max_self: m.n("Max self damage"),
        max_friend: m.n("Max friend damage"),
        min_remaining: m.n("Min health remaining"),
        anti_suicide: m.b("Anti suicide"),
    }
}
fn damage_ok(
    game: &mut Game<'_>,
    m: &Module,
    p: [i32; 3],
    targets: &[Target],
    friends: &[Target],
    step: Step,
    budget: &mut Budget,
) -> Option<(u32, Rating)> {
    let blast = Blast::anchor(p);
    let own = safety_damage(local_damage(game, blast, budget)?, m.p("Damage model"));
    let health = game
        .player
        .as_ref()
        .map(|p| f64::from(p.health.health() + p.health.absorption()))?;
    let l = limits(m);
    if own > l.max_self {
        reason("searching", "self damage exceeds cap");
        return None;
    }
    if l.anti_suicide && (health <= own || health - own < l.min_remaining) {
        reason("searching", "anti suicide or health reserve");
        return None;
    }
    let mut friend_upper: f64 = 0.0;
    for f in friends {
        if !in_range(blast.center, f.pos, 12.0) {
            continue;
        }
        friend_upper = friend_upper.max(target_damage(game, f, blast, budget)?.upper);
    }
    if friend_upper > l.max_friend {
        reason("searching", "protected player damage exceeds cap");
        return None;
    }
    let mut best: Option<(u32, Rating)> = None;
    for target in targets {
        let Some(damage) = target_damage(game, target, blast, budget).map(|p| p.estimate) else {
            continue;
        };
        let mut l = limits(m);
        state(|s| s.last_prediction = Some((own, damage, friend_upper)));
        l.min_target = order::target_min(l.min_target, f64::from(target.health))?;
        if !safe(l, damage, own, health, friend_upper) {
            reason("searching", "target threshold or invalid damage policy");
            continue;
        }
        let rating = Rating {
            target_damage: damage,
            target_health: f64::from(target.health),
            self_damage: own,
            attack: step == Step::Detonate,
        };
        if best.as_ref().is_none_or(|(_, old)| prefer(rating, *old)) {
            best = Some((target.id, rating));
        }
    }
    best
}

fn prefer(next: Rating, old: Rating) -> bool {
    match (
        order::opportunity(
            next.target_damage,
            next.target_health,
            next.self_damage,
            next.attack,
        ),
        order::opportunity(
            old.target_damage,
            old.target_health,
            old.self_damage,
            old.attack,
        ),
    ) {
        (Some(n), Some(o)) => order::better(n, o),
        (Some(_), None) => true,
        _ => false,
    }
}

fn reachable(
    game: &Game<'_>,
    m: &Module,
    eye: [f64; 3],
    hit: [f64; 3],
    clicked: [i32; 3],
    face: i32,
    budget: &mut Budget,
) -> bool {
    // Stop immediately outside the selected face. Ignoring the whole clicked
    // block would incorrectly permit clicking its hidden back face.
    let Some(ray_end) = order::face_ray_end(hit, face) else {
        return false;
    };
    if !game.world.loaded(clicked[0], clicked[2]) {
        return false;
    }
    in_range(eye, hit, m.n("Range").clamp(0.0, 5.0))
        && (in_range(
            eye,
            hit,
            m.n("Walls range").clamp(0.0, m.n("Range").clamp(0.0, 5.0)),
        ) || clear_line(game, eye, ray_end, None, budget))
}
// Click a full solid neighbor face; never air-place or click an anchor/container.
fn placement(
    game: &Game<'_>,
    m: &Module,
    p: [i32; 3],
    budget: &mut Budget,
) -> Option<([i32; 3], i32, [f64; 3])> {
    if !util::can_place(game, p, true) || !game.world.loaded(p[0], p[2]) {
        return None;
    }
    let eye = game.eye_position()?;
    for (d, face) in [
        ([0, -1, 0], 1),
        ([0, 1, 0], 0),
        ([0, 0, -1], 3),
        ([0, 0, 1], 2),
        ([-1, 0, 0], 5),
        ([1, 0, 0], 4),
    ] {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        if !game.world.loaded(n[0], n[2]) {
            continue;
        }
        let f = util::facts(game, n);
        if !f.full_collision
            || f.block_entity
            || f.fluid_level > 0
            || !order::inert_support(&f.name)
        {
            continue;
        }
        let c = util::center_of(n);
        let hit = [
            c[0] - f64::from(d[0]) * 0.5,
            c[1] - f64::from(d[1]) * 0.5,
            c[2] - f64::from(d[2]) * 0.5,
        ];
        if reachable(game, m, eye, hit, n, face, budget) {
            return Some((n, face, hit));
        }
    }
    None
}
// Choose the face toward the eyes; interaction range uses the clicked surface.
fn anchor_hit(eye: [f64; 3], p: [i32; 3]) -> (i32, [f64; 3]) {
    let mut hit = util::center_of(p);
    let delta = [eye[0] - hit[0], eye[1] - hit[1], eye[2] - hit[2]];
    let axis = (0..3)
        .max_by(|&a, &b| delta[a].abs().total_cmp(&delta[b].abs()))
        .unwrap_or(1);
    let positive = delta[axis] >= 0.0;
    hit[axis] += if positive { 0.5 } else { -0.5 };
    let face = match (axis, positive) {
        (0, false) => 4,
        (0, true) => 5,
        (1, false) => 0,
        (1, true) => 1,
        (2, false) => 2,
        _ => 3,
    };
    (face, hit)
}
fn proposal(
    game: &Game<'_>,
    m: &Module,
    p: [i32; 3],
    target: u32,
    budget: &mut Budget,
) -> Option<Action> {
    let before = block(game, p);
    let charge = charges(game, p);
    if game.is_anchor(before) && charge.is_none() {
        return None;
    }
    let step = order::next(charge, fuel(game, OFFHAND));
    if step == Step::Place && !m.b("Place") {
        return None;
    }
    let slot = slot_for(game, step)?;
    if step == Step::Detonate
        && !order::can_detonate(charge?, fuel(game, slot), fuel(game, OFFHAND))
    {
        return None;
    }
    let (clicked, face, hit) = if step == Step::Place {
        placement(game, m, p, budget)?
    } else {
        let eye = game.eye_position()?;
        let (face, hit) = anchor_hit(eye, p);
        if !reachable(game, m, eye, hit, p, face, budget) {
            return None;
        }
        (p, face, hit)
    };
    Some(Action {
        pos: p,
        before,
        step,
        slot,
        clicked,
        face,
        hit,
        target,
    })
}

fn observe(game: &Game<'_>) -> bool {
    let p = state(|s| s.pending);
    let Some(p) = p else {
        return false;
    };
    let now = game.ticks;
    if let Some(sent) = p.sent {
        match order::observed(
            p.action.before.0,
            block(game, p.action.pos).0,
            sent,
            now,
            ACK_TIMEOUT,
        ) {
            Wait::Pending => {
                reason("awaiting state", "server state unchanged");
                true
            }
            result => {
                state(|s| {
                    s.pending = None;
                    if result == Wait::Changed {
                        s.ack_count = s.ack_count.saturating_add(1);
                        s.last_ack = Some(now);
                        s.status("observed", "block state changed");
                    }
                    if result == Wait::Expired {
                        s.status("blocked", "state confirmation timeout");
                        if s.failed.len() < MAX_FAILED {
                            s.failed.push((p.action.pos, p.action.before));
                        } else {
                            s.exhausted = true;
                        }
                    }
                });
                false
            }
        }
    } else if now < p.queued
        || now.saturating_sub(p.queued) > 2
        || p.queued_real.elapsed() > Duration::from_millis(200)
    {
        cancel_turns(NAME);
        state(|s| s.pending = None);
        reason("idle", "queued action expired");
        false
    } else {
        true
    }
}

// Returns true only when this aura owns a queued/new deed. ACK-only waiting
// yields to the other aura. Call from
// the shared aura dispatcher; inventory switches occur only inside its deed.
pub(super) fn prepare(game: &mut Game<'_>, m: &Module, ticks: u64) -> Option<Rating> {
    state(|s| {
        s.plan = None;
        s.evaluated = 0;
    });
    if !ready(game, m) {
        reset();
        reason(
            "paused",
            if game.anchor_works() {
                "Nether anchors do not explode"
            } else {
                "disabled or input/world/inventory pause"
            },
        );
        return None;
    }
    state(|s| s.last_prediction = None);
    let Some(origin) = util::block_pos(game) else {
        reset();
        reason("paused", "missing player position");
        return None;
    };
    let session = (game.remote.as_ref().map_or(0, |r| r.entity_id), game.dim);
    let restart = state(|s| {
        s.session != Some(session)
            || ticks < s.last_tick
            || s.origin.is_some_and(|p| {
                (0..3).any(|i| (i64::from(p[i]) - i64::from(origin[i])).abs() > 12)
            })
    });
    if restart {
        reset();
    }
    state(|s| {
        s.session = Some(session);
        s.last_tick = ticks;
    });
    if observe(game) {
        return None;
    }
    if state(|s| s.exhausted || game.ticks < s.next_action) {
        reason("waiting", "action delay or failed-state limit");
        return None;
    }
    let Some((targets, friends)) = players(game, m) else {
        reason("paused", "protected population overflow/invalid state");
        return None;
    };
    let Some(target) = targets.first() else {
        reason("searching", "no eligible target (including hurt immunity)");
        return None;
    };
    // Incremental cube scan: at most 128 block reads each tick, 64 retained.
    let mut candidates = state(|s| {
        if s.origin != Some(origin) {
            s.origin = Some(origin);
            s.cursor = 0;
            s.anchors.clear();
        }
        for _ in 0..SCAN {
            let i = s.cursor;
            s.cursor = (i + 1) % VOLUME;
            let p = [
                origin[0] + (i % SIDE as usize) as i32 - RADIUS,
                origin[1] + (i / SIDE as usize % SIDE as usize) as i32 - RADIUS,
                origin[2] + (i / (SIDE * SIDE) as usize) as i32 - RADIUS,
            ];
            if game.world.loaded(p[0], p[2])
                && game.is_anchor(block(game, p))
                && !s.anchors.contains(&p)
                && s.anchors.len() < 64
            {
                s.anchors.push(p);
            }
        }
        s.anchors.retain(|&p| game.is_anchor(block(game, p)));
        s.failed.retain(|&(p, b)| block(game, p) == b);
        s.anchors.clone()
    });
    candidates.sort_by(|a, b| {
        util::dist(util::center_of(*a), target.pos)
            .total_cmp(&util::dist(util::center_of(*b), target.pos))
    });
    if m.b("Place") {
        // A bounded neighborhood, not an entire range cube of damage queries.
        let mut places = Vec::with_capacity(108);
        for target in &targets {
            let p = target.pos.map(|n| n.floor() as i32);
            for y in 0..=2 {
                for x in -1..=1 {
                    for z in -1..=1 {
                        let at = [p[0] + x, p[1] + y, p[2] + z];
                        if !candidates.contains(&at)
                            && !places.contains(&at)
                            && util::can_place(game, at, true)
                        {
                            places.push(at);
                        }
                    }
                }
            }
        }
        places.sort_by(|a, b| {
            util::dist(util::center_of(*a), target.pos)
                .total_cmp(&util::dist(util::center_of(*b), target.pos))
        });
        candidates.extend(places);
    }
    let mut budget = Budget::new(48);
    reason("searching", "no candidate passed reach/material checks");
    let mut evaluated = 0;
    let mut best: Option<(Action, Rating)> = None;
    for p in candidates {
        if state(|s| s.failed.contains(&(p, block(game, p)))) {
            continue;
        }
        let Some(mut a) = proposal(game, m, p, target.id, &mut budget) else {
            continue;
        };
        if evaluated == MAX_EVALUATED {
            break;
        }
        evaluated += 1;
        let Some((id, rating)) = damage_ok(game, m, p, &targets, &friends, a.step, &mut budget)
        else {
            continue;
        };
        a.target = id;
        if best.as_ref().is_none_or(|(_, old)| prefer(rating, *old)) {
            best = Some((a, rating));
        }
    }
    state(|s| s.evaluated = evaluated);
    let Some((action, rating)) = best else {
        return None;
    };
    state(|s| {
        s.plan = Some(Plan {
            action,
            rating,
            ticks,
            module: m.clone(),
        });
        s.status("prepared", "safe candidate available");
    });
    Some(rating)
}

pub(super) fn execute(game: &mut Game<'_>, m: &Module, ticks: u64) -> bool {
    let plan = state(|s| s.plan.take());
    let Some(plan) = plan else {
        return false;
    };
    if plan.ticks != ticks || plan.module != *m || !ready(game, m) {
        reason("idle", "prepared plan expired or settings changed");
        return false;
    }
    let action = plan.action;
    let session = (game.remote.as_ref().map_or(0, |r| r.entity_id), game.dim);
    let token = state(|s| {
        s.token = s.token.wrapping_add(1);
        s.token
    });
    let queued = game.ticks;
    let queued_real = Instant::now();
    // Retain the Arc so a disconnected session cannot reuse its allocation.
    let Some(connection) = game.remote.as_ref().map(|r| r.session.counters.clone()) else {
        return false;
    };
    let expected = m.clone();
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let (yaw, pitch) = look_at(eye, action.hit);
    // Revalidate after rotation, including current state, materials and damage.
    if !super::sunset_combat_control::claim(NAME) { return false; }
    let finish = super::sunset_combat_control::finish(NAME);
    let accepted = try_turn_then(NAME, yaw, pitch, move |game| {
        let _finish = finish;
        if !_finish.current() { return; }
        let pending = state(|s| s.pending);
        if pending.is_none_or(|p| p.token != token || p.sent.is_some()) {
            return;
        }
        let current = with(|s| s.find(NAME).map(|i| s.modules[i].clone()));
        let valid = current
            .as_ref()
            .is_some_and(|m| *m == expected && ready(game, m));
        let mut budget = Budget::new(48);
        let fresh = if valid
            && game.dim == session.1
            && game.remote.as_ref().is_some_and(|r| {
                r.entity_id == session.0 && std::sync::Arc::ptr_eq(&connection, &r.session.counters)
            })
            && queued_real.elapsed() <= Duration::from_millis(200)
            && game.ticks >= queued
            && game.ticks - queued <= 2
            && block(game, action.pos) == action.before
        {
            players(game, &expected).and_then(|(targets, friends)| {
                let target = targets.iter().find(|t| t.id == action.target)?;
                let a = proposal(game, &expected, action.pos, action.target, &mut budget)?;
                // Never send a packet for a different hit face than the look
                // carried by the queued rotation.
                if a.step != action.step
                    || a.slot != action.slot
                    || a.clicked != action.clicked
                    || a.face != action.face
                    || a.hit != action.hit
                {
                    return None;
                }
                damage_ok(
                    game,
                    &expected,
                    a.pos,
                    std::slice::from_ref(target),
                    &friends,
                    a.step,
                    &mut budget,
                )?;
                Some(a)
            })
        } else {
            None
        };
        let Some(a) = fresh else {
            state(|s| s.pending = None);
            reason("idle", "deferred revalidation rejected stale/unsafe action");
            return;
        };
        let before = game.selected;
        if a.slot != OFFHAND {
            hold(game, a.slot);
        }
        let sequence = util::sequence(game);
        let cursor = [
            (a.hit[0] - f64::from(a.clicked[0])) as f32,
            (a.hit[1] - f64::from(a.clicked[1])) as f32,
            (a.hit[2] - f64::from(a.clicked[2])) as f32,
        ];
        game.send(S::UseItemOn(play::UseItemOn {
            hand: i32::from(a.slot == OFFHAND),
            pos: a.clicked,
            face: a.face,
            cursor,
            inside: false,
            world_border: false,
            sequence,
        }));
        if expected.b("Swing") {
            game.swing_arm();
            game.send(S::Punch(Empty));
        }
        if a.slot != OFFHAND && game.selected == a.slot {
            hold(game, before);
        }
        state(|s| {
            if let Some(p) = s.pending.as_mut().filter(|p| p.token == token) {
                p.sent = Some(game.ticks);
            }
            s.sent_count = s.sent_count.saturating_add(1);
            s.last_sent = Some(game.ticks);
            let phase = match a.step {
                Step::Place => "place sent",
                Step::Charge => "charge sent",
                Step::Detonate => "detonate sent",
            };
            s.status(phase, "awaiting observed server block change");
            s.next_action = game
                .ticks
                .saturating_add(expected.n("Action delay").clamp(1.0, 20.0) as u64);
        });
    });
    if accepted {
        state(|s| {
            s.status("queued", "accepted shared rotation claim");
            s.pending = Some(Pending {
                action,
                token,
                queued,
                queued_real,
                sent: None,
            })
        });
    } else {
        reason("waiting", "shared rotation claim busy");
    }
    accepted
}
