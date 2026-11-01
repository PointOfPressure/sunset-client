// Anchor Aura (Rust's, PR #16): places, charges and sets off respawn anchors on enemy players outside the Nether.
// Each step waits for the server to show the block changed; the client never predicts it.
use super::sunset::{Category, Module, cancel_turns, gui_open, look_at, try_turn_then, with};
use super::sunset_anchor_state::{self as order, Step, Wait};
use super::sunset_explosive_aura::Rating;
use super::sunset_explosive_damage::{Blast, clear_line, local_damage};
use super::sunset_explosive_policy::{Budget, in_range};
use super::sunset_util::{self as util, OFFHAND, Target};
use super::{Dim, Game, Screen};
use super::{sunset_explosive_choice::Kind, sunset_explosive_tactics as tactics};
use super::{sunset_faceplace_policy as face, sunset_prediction as prediction};
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
const MAX_EVALUATED: usize = 4;
const MAX_FAILED: usize = 32;
const ACK_TIMEOUT: u64 = 30;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Places, charges and sets off respawn anchors on enemy players (not in the Nether), never more than Max self damage to you or a friend.")
            .num("Min damage", 6.0, 0.0, 20.0, 0.5)
            .num("Max self damage", 6.0, 0.0, 20.0, 0.5)
            .bool("Render", true)
            .text("Render color", "#FFFF9A3C")
            .num("Turn speed", 4.5, 1.0, 10.0, 0.5)
            .bool("Protect loot", true),
    ]
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
    ticks: u64,
    module: Module,
}

#[derive(Default)]
struct State {
    session: Option<(i32, Dim)>,
    session_arc:
        Option<std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<horizon_protocol::connection::Counters>>>>>,
    last_tick: u64,
    cursor: usize,
    origin: Option<[i32; 3]>,
    anchors: Vec<[i32; 3]>,
    pending: Option<Pending>,
    failed: Vec<([i32; 3], BlockStateId, u64)>,
    aim_wait: Option<([i32; 3], u64)>,
    spent: Option<([i32; 3], u64)>,
    token: u64,
    next_action: u64,
    plan: Option<Plan>,
    lookahead: Option<(Action, Rating)>,
    rival: f64,
    placed_turn: Option<f32>,
    probe: usize,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}

// Input contention cancels work that has not been sent. A real pending use and
// its acknowledgement/retry history survive until observation or scene reset.
pub(super) fn suspend() {
    super::sunset_combat_control::release(NAME);
    cancel_turns(NAME);
    state(|s| {
        s.plan = None;
        s.lookahead = None;
        if s.pending.is_some_and(|p| p.sent.is_none()) {
            s.pending = None;
        }
    });
}

pub(super) fn reset() {
    super::sunset_explosive_aura::clear_view(true);
    super::sunset_combat_control::release(NAME);
    cancel_turns(NAME);
    state(|s| {
        let token = s.token.wrapping_add(1);
        *s = State { token, ..State::default() };
    });
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
        && [tactics::anchor_range(game), 0.0, 10.0, 0.0].iter().all(|n| n.is_finite() && *n >= 0.0)
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
        && game.using.is_none()
        && game.mining.is_none()
        && !game.player.as_ref().is_some_and(|p| p.shift_key_down)
        && !fuel(game, OFFHAND)
        && !tactics::fixed_look(game)
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
    game.pack.assets.registry.state(b)?.properties.get("charges")?.parse::<u8>().ok().filter(|n| *n <= 4)
}
fn fuel(game: &Game<'_>, slot: usize) -> bool {
    if slot == OFFHAND { game.offhand.as_ref() } else { game.inventory.get(slot).and_then(|s| s.as_ref()) }
        .is_some_and(|s| s.count > 0 && util::short(&s.item) == "glowstone")
}
fn item(game: &Game<'_>, slot: usize, name: &str) -> bool {
    if slot == OFFHAND { game.offhand.as_ref() } else { game.inventory.get(slot).and_then(|s| s.as_ref()) }
        .is_some_and(|s| s.count > 0 && util::short(&s.item) == name)
}
fn slot_for(game: &Game<'_>, step: Step) -> Option<usize> {
    let selected = game.selected.min(8);
    match step {
        Step::Place => {
            if item(game, selected, "respawn_anchor") {
                Some(selected)
            } else {
                util::find_hotbar(game, |n| n == "respawn_anchor")
            }
        }
        Step::Charge => {
            if fuel(game, selected) {
                Some(selected)
            } else {
                util::find_hotbar(game, |n| n == "glowstone")
            }
        }
        Step::Detonate => {
            if fuel(game, OFFHAND) {
                return None;
            }
            if tactics::combat_item(game, game.inventory.get(selected).and_then(|s| s.as_ref())) {
                return Some(selected);
            }
            util::find_hotbar(game, |n| n == "totem_of_undying").or_else(|| {
                (0..9).find(|&slot| tactics::combat_item(game, game.inventory.get(slot).and_then(|s| s.as_ref())))
            })
        }
    }
}

// Bound population processing as well as explosion work. Overflow fails closed
// so a crowded world cannot silently drop a protected player from safety checks.
fn players(game: &Game<'_>, _m: &Module) -> Option<(Vec<Target>, Vec<Target>)> {
    tactics::players(game)
}
fn optimistic(p: [i32; 3], targets: &[Target]) -> f64 {
    let at = p.map(|v| f64::from(v) + 0.5);
    targets
        .iter()
        .map(|t| {
            let raw = super::sunset_explosive_policy::raw_damage(util::dist(at, t.pos), 5.0, 1.0).unwrap_or(0.0);
            let hp = f64::from(t.health).max(0.01);
            let damage = f64::from(raw) * 1.5;
            damage.min(hp) + if damage >= hp { 1000.0 } else { 0.0 }
        })
        .fold(0.0, f64::max)
}
fn cap(m: &Module) -> f64 {
    m.n("Max self damage")
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
    let (now, opening) = damage_pair(game, m, p, targets, friends, step, budget)?;
    if step == Step::Place { now.or(opening) } else { now }
}
fn damage_pair(
    game: &mut Game<'_>,
    m: &Module,
    p: [i32; 3],
    targets: &[Target],
    friends: &[Target],
    step: Step,
    budget: &mut Budget,
) -> Option<(Option<(u32, Rating)>, Option<(u32, Rating)>)> {
    let blast = Blast::anchor(p);
    if !super::sunset_loot_guard::allows(game, blast, m.b("Protect loot")) {
        return None;
    }
    let own = local_damage(game, blast, budget)?.estimate;
    let player = game.player.as_ref()?;
    let health = f64::from(player.health.health() + player.health.absorption());
    let max = cap(m);
    if own > max || tactics::endangered(own, health, game.creative) {
        return None;
    }
    for f in friends {
        let projected = prediction::target(game, f, prediction::rtt(game).min(5))?;
        if !in_range(blast.center, f.pos, 12.0) && !in_range(blast.center, projected.pos, 12.0) {
            continue;
        }
        let damage = tactics::friend_damage(game, &projected, blast, budget)?;
        let creative = super::remote::sunset_combat_remote::player_mode(game, f.id) == Some(1);
        if damage > max || tactics::endangered(damage, tactics::friend_health(game, f), creative) {
            return None;
        }
    }
    let mut best: [Option<(u32, Rating)>; 2] = [None, None];
    for t in targets {
        let projected = prediction::target(game, t, prediction::rtt(game).min(5))?;
        let ahead = match step {
            Step::Detonate => 0,
            Step::Charge => 1,
            Step::Place => 2,
        };
        let opens = prediction::opens_in(t.id, prediction::rtt(game));
        let opening = super::sunset_explosive_choice::opening_horizon(ahead, opens);
        let (damage, _) =
            prediction::estimate_pair(game, &projected, blast, ahead, opening, prediction::rtt(game), budget)?;
        let minimum = m.n("Min damage");
        let needed = face::anchor(minimum, prediction::effective_health(game, t))?;
        for i in 0..2 {
            if damage[i] < needed || step == Step::Charge && i == 0 && opens > 1 {
                continue;
            }
            let kind = match step {
                Step::Detonate => Kind::AnchorHit,
                Step::Charge => Kind::Charge,
                Step::Place => Kind::AnchorPlace,
            };
            let mut rating = Rating::new(
                kind,
                damage[i],
                prediction::effective_health(game, t).unwrap_or(f64::from(t.health)),
                own,
                prediction::held_totem(t.id),
                ahead as u16,
            );
            rating.full_damage = damage[2];
            if best[i].as_ref().is_none_or(|(_, old)| prefer(rating, *old)) {
                best[i] = Some((t.id, rating));
            }
        }
    }
    Some((best[0], best[1]))
}

fn prefer(next: Rating, old: Rating) -> bool {
    super::sunset_explosive_choice::prefer_first(next, old, true)
}

fn reachable(
    game: &Game<'_>,
    _m: &Module,
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
    in_range(eye, hit, tactics::anchor_range(game).clamp(0.0, 64.0)) && clear_line(game, eye, ray_end, None, budget)
}
// Click a full solid neighbor face; never air-place or click an anchor/container.
fn placement(game: &Game<'_>, m: &Module, p: [i32; 3], budget: &mut Budget) -> Option<([i32; 3], i32, [f64; 3])> {
    if !util::can_place(game, p, true) || !game.world.loaded(p[0], p[2]) {
        return None;
    }
    let eye = game.eye_position()?;
    for (d, face) in [([0, -1, 0], 1), ([0, 1, 0], 0), ([0, 0, -1], 3), ([0, 0, 1], 2), ([-1, 0, 0], 5), ([1, 0, 0], 4)]
    {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        if !game.world.loaded(n[0], n[2]) {
            continue;
        }
        let f = util::facts(game, n);
        if !f.full_collision || f.block_entity || f.fluid_level > 0 || !order::inert_support(&f.name) {
            continue;
        }
        let c = util::center_of(n);
        let hit = [c[0] - f64::from(d[0]) * 0.5, c[1] - f64::from(d[1]) * 0.5, c[2] - f64::from(d[2]) * 0.5];
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
    let axis = (0..3).max_by(|&a, &b| delta[a].abs().total_cmp(&delta[b].abs())).unwrap_or(1);
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
fn proposal(game: &Game<'_>, m: &Module, p: [i32; 3], target: u32, budget: &mut Budget) -> Option<Action> {
    let before = block(game, p);
    let charge = charges(game, p);
    if game.is_anchor(before) && charge.is_none() {
        return None;
    }
    let step = order::next(charge, fuel(game, OFFHAND));
    if step == Step::Place {
        if super::sunset_crystal_aura::reserved_cell(p) {
            return None;
        }

        let bounds = horizon_core::geometry::Aabb::block(horizon_core::position::BlockPos::new(p[0], p[1], p[2]));
        for mob in game.mobs.iter().filter(|m| m.kind.id == "minecraft:player" && !m.removed && m.alive()) {
            if Some(mob.id) == super::sunset::own_id(game)
                || super::remote::sunset_combat_remote::player_mode(game, mob.id) == Some(3)
            {
                continue;
            }
            let projected = prediction::target(game, &util::mob_target(mob), prediction::rtt(game).min(5))?;
            let future = horizon_core::geometry::Aabb::new(
                horizon_core::geometry::Vec3::new(projected.lo[0], projected.lo[1], projected.lo[2]),
                horizon_core::geometry::Vec3::new(projected.hi[0], projected.hi[1], projected.hi[2]),
            );
            if bounds.intersects(mob.bounds()) || bounds.intersects(future) {
                return None;
            }
        }
    }
    if step == Step::Place && tactics::fixed_look(game) {
        return None;
    }
    let slot = slot_for(game, step)?;
    if step == Step::Detonate && !order::can_detonate(charge?, fuel(game, slot), fuel(game, OFFHAND)) {
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
    Some(Action { pos: p, before, step, slot, clicked, face, hit, target })
}

fn observe(game: &Game<'_>) -> bool {
    let p = state(|s| s.pending);
    let Some(p) = p else {
        return false;
    };
    let now = game.ticks;
    if let Some(sent) = p.sent {
        match order::observed(p.action.before.0, block(game, p.action.pos).0, sent, now, ACK_TIMEOUT) {
            Wait::Pending => true,
            result => {
                state(|s| {
                    s.pending = None;
                    if result == Wait::Changed {}
                    if result == Wait::Expired {
                        if s.failed.len() < MAX_FAILED {
                            s.failed.push((p.action.pos, p.action.before, now + 40));
                        } else {
                        }
                    }
                });
                false
            }
        }
    } else if now < p.queued || now.saturating_sub(p.queued) > 2 || p.queued_real.elapsed() > Duration::from_millis(200)
    {
        cancel_turns(NAME);
        state(|s| s.pending = None);
        false
    } else {
        true
    }
}

fn retry_blocked(game: &Game<'_>, p: [i32; 3]) -> bool {
    state(|s| s.failed.iter().any(|&(cell, b, until)| cell == p && b == block(game, p) && game.ticks < until))
}

// Returns true only when this aura owns a queued/new deed. ACK-only waiting
// yields to the other aura. Call from
// the shared aura dispatcher; inventory switches occur only inside its deed.
pub(super) fn prepare(game: &mut Game<'_>, m: &Module, ticks: u64) -> Option<Rating> {
    state(|s| {
        s.plan = None;
        s.lookahead = None;
    });
    if !ready(game, m) {
        suspend();
        return None;
    }
    idle_hand(game);
    let Some(origin) = util::block_pos(game) else {
        reset();
        return None;
    };
    let session = (game.remote.as_ref().map_or(0, |r| r.entity_id), game.dim);
    let restart = state(|s| {
        s.session != Some(session)
            || s.session_arc.as_ref().is_none_or(|arc| {
                game.remote.as_ref().is_none_or(|r| !std::sync::Arc::ptr_eq(arc, &r.session.counters))
            })
            || ticks < s.last_tick
            || s.origin.is_some_and(|p| (0..3).any(|i| (i64::from(p[i]) - i64::from(origin[i])).abs() > 12))
    });
    if restart {
        let value = state(|s| s.rival);
        reset();
        rival(value);
    }
    state(|s| {
        s.session = Some(session);
        s.session_arc = game.remote.as_ref().map(|r| r.session.counters.clone());
        s.last_tick = ticks;
    });
    let waiting = observe(game);
    if state(|s| game.ticks < s.next_action) {
        return None;
    }
    let Some((targets, friends)) = players(game, m) else {
        return None;
    };
    let Some(target) = targets.first() else {
        return None;
    };
    // Incremental cube scan: at most 128 block reads each tick, 64 retained.
    let radius = tactics::anchor_range(game).ceil().clamp(1.0, 64.0) as i32;
    let side = (radius * 2 + 1) as usize;
    let volume = side * side * side;
    let mut candidates = state(|s| {
        if s.origin != Some(origin) {
            s.origin = Some(origin);
            s.cursor = 0;
            s.anchors.clear();
        }
        for _ in 0..SCAN {
            let i = s.cursor;
            s.cursor = (i + 1) % volume;
            let p = [
                origin[0] + (i % side) as i32 - radius,
                origin[1] + (i / side % side) as i32 - radius,
                origin[2] + (i / (side * side)) as i32 - radius,
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
        s.failed.retain(|&(p, b, until)| game.ticks < until && block(game, p) == b);

        s.anchors.clone()
    });
    candidates.sort_by(|a, b| optimistic(*b, &targets).total_cmp(&optimistic(*a, &targets)));
    let probe = state(|s| {
        let n = s.probe;
        s.probe = s.probe.wrapping_add(1);
        n
    });
    if candidates.len() > 2 {
        let at = 1 + probe % (candidates.len() - 1);
        candidates.swap(1, at);
    }
    let mut stage = super::sunset_explosive_choice::Stage::default();
    let mut checked = Vec::new();
    let mut budget = Budget::new(48);
    for &cell in candidates.iter().take(64) {
        if retry_blocked(game, cell) {
            continue;
        }
        if state(|s| s.spent.is_some_and(|(p, until)| p == cell && game.ticks < until)) {
            continue;
        }
        let Some(action) = proposal(game, m, cell, target.id, &mut budget) else {
            continue;
        };
        if checked.len() >= MAX_EVALUATED - 1 {
            break;
        }
        let pair = damage_pair(game, m, cell, &targets, &friends, action.step, &mut budget);
        stage.existing_anchor_now |= pair.as_ref().is_some_and(|p| p.0.is_some());
        stage.existing_anchor_opening |= pair.as_ref().is_some_and(|p| p.1.is_some());
        checked.push((cell, action, pair));
    }
    if stage.new_anchor() && !waiting {
        // A bounded neighborhood, not an entire range cube of damage queries.
        let mut places = Vec::with_capacity(300);
        for target in &targets {
            let p = target.pos.map(|n| n.floor() as i32);
            for y in -1..=2 {
                for x in -2..=2 {
                    for z in -2..=2 {
                        let at = [p[0] + x, p[1] + y, p[2] + z];
                        if !candidates.contains(&at) && !places.contains(&at) && util::can_place(game, at, true) {
                            places.push(at);
                        }
                    }
                }
            }
        }
        places.sort_by(|a, b| optimistic(*b, &targets).total_cmp(&optimistic(*a, &targets)));
        if !places.is_empty() {
            let n = probe % places.len();
            places.rotate_left(n);
        }
        candidates.extend(places);
    }
    let mut evaluated = checked.len();
    let mut best: Option<(Action, Rating)> = None;
    for p in candidates {
        if state(|s| s.spent.is_some_and(|(cell, until)| cell == p && game.ticks < until)) {
            continue;
        }

        if retry_blocked(game, p) {
            continue;
        }
        let cached = checked.iter().find(|(cell, _, _)| *cell == p);
        let (mut a, pair) = if let Some((_, action, pair)) = cached {
            (*action, *pair)
        } else {
            if game.is_anchor(block(game, p)) {
                continue;
            }
            if evaluated >= MAX_EVALUATED {
                break;
            }
            let Some(action) = proposal(game, m, p, target.id, &mut budget) else {
                continue;
            };
            evaluated += 1;
            let pair = damage_pair(game, m, p, &targets, &friends, action.step, &mut budget);
            (action, pair)
        };
        let Some((mut now, opening)) = pair else {
            continue;
        };
        if a.step == Step::Place {
            now = now.or(opening);
        }
        if a.step == Step::Charge && now.is_some_and(|(_, r)| state(|s| s.rival > 0.0 && r.value() <= s.rival + 1.0)) {
            continue;
        }
        if waiting || now.is_none() {
            if let Some((_, rating)) = opening {
                preaim(game, m, a, rating);
            }
            continue;
        }
        let Some((id, rating)) = now else {
            continue;
        };

        a.target = id;
        if best.as_ref().is_none_or(|(_, old)| prefer(rating, *old)) {
            best = Some((a, rating));
        }
    }
    let Some((action, rating)) = best else {
        return None;
    };
    state(|s| {
        s.plan = Some(Plan { action, ticks, module: m.clone() });
    });
    super::sunset_explosive_aura::show(game, true, action.pos, rating);
    Some(rating)
}

pub(super) fn execute(game: &mut Game<'_>, m: &Module, ticks: u64) -> bool {
    let plan = state(|s| s.plan.take());
    let Some(plan) = plan else {
        return false;
    };
    if plan.ticks != ticks || plan.module != *m || !ready(game, m) {
        return false;
    }
    let mut action = plan.action;
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let turn = tactics::next_look(game, look_at(eye, action.hit), m.n("Turn speed"));
    let Some(contact) = util::block_contact(game, turn, action.clicked, action.face, tactics::anchor_range(game))
    else {
        let expired = state(|s| {
            let start = s.aim_wait.filter(|(cell, _)| *cell == action.pos).map_or(game.ticks, |(_, at)| at);
            s.aim_wait = Some((action.pos, start));
            game.ticks.saturating_sub(start) >= 12
        });
        if expired {
            state(|s| {
                if s.failed.len() < MAX_FAILED {
                    s.failed.push((action.pos, action.before, game.ticks + 40));
                }
                s.aim_wait = None;
            });
        } else if !tactics::fixed_look(game) {
            super::sunset::turn(NAME, turn.0, turn.1);
        }
        return false;
    };
    state(|s| s.aim_wait = None);
    action.hit = contact;
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
    let from = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    let turn_amount = (turn.0 - from.0).abs();
    if action.step == Step::Place
        && turn_amount > 2.0
        && state(|s| s.placed_turn.is_some_and(|old| (old - turn_amount).abs() < 1.0e-4))
    {
        return false;
    }
    let initial_slot = game.selected;
    let (yaw, pitch) = turn;
    // Revalidate after rotation, including current state, materials and damage.
    if !super::sunset_combat_control::claim(NAME) {
        return false;
    }
    let finish = super::sunset_combat_control::finish(NAME);
    let accepted = try_turn_then(NAME, yaw, pitch, move |game| {
        let _finish = finish;
        if !_finish.current() {
            return;
        }
        let pending = state(|s| s.pending);
        if pending.is_none_or(|p| p.token != token || p.sent.is_some()) {
            return;
        }
        let current = with(|s| s.find(NAME).map(|i| s.modules[i].clone()));
        let ranges = tactics::refresh(game);
        let valid = ranges
            && game.selected == initial_slot
            && current.as_ref().is_some_and(|m| *m == expected && ready(game, m));
        let mut budget = Budget::new(48);
        let fresh = if valid
            && game.dim == session.1
            && game
                .remote
                .as_ref()
                .is_some_and(|r| r.entity_id == session.0 && std::sync::Arc::ptr_eq(&connection, &r.session.counters))
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
                {
                    return None;
                }
                damage_ok(game, &expected, a.pos, std::slice::from_ref(target), &friends, a.step, &mut budget)?;
                let sent = super::sunset_combat_wire::sent_look(game)?;
                let contact = util::block_contact(game, sent, a.clicked, a.face, tactics::anchor_range(game))?;
                Some(Action { hit: contact, ..a })
            })
        } else {
            None
        };
        let Some(a) = fresh else {
            state(|s| s.pending = None);
            return;
        };
        if a.step == Step::Detonate {
            let Some((targets, _)) = players(game, &expected) else {
                return;
            };
            if !prediction::arm_blast(
                game,
                super::sunset_hurt_policy::ShotSource::Anchor(a.pos.map(|v| f64::from(v) + 0.5)),
                Blast::anchor(a.pos),
                &targets,
                a.target,
                prediction::rtt(game).min(5),
                &mut budget,
            ) {
                return;
            }
        }
        take_hand(game, a.slot);
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
        if true {
            game.swing_arm();
            game.send(S::Punch(Empty));
        }

        state(|s| {
            if let Some(p) = s.pending.as_mut().filter(|p| p.token == token) {
                p.sent = Some(game.ticks);
            }
            if a.step == Step::Place {
                s.placed_turn = Some(turn_amount);
            }
            if a.step == Step::Detonate {
                s.spent = Some((a.pos, game.ticks + u64::from(prediction::rtt(game).saturating_add(2).clamp(4, 20))));
            }
            s.next_action = game.ticks.saturating_add(0);
        });
    });
    if accepted {
        state(|s| s.pending = Some(Pending { action, token, queued, queued_real, sent: None }));
    } else {
    }
    accepted
}

pub(super) fn rival(value: f64) {
    state(|s| s.rival = if value.is_finite() { value.max(0.0) } else { 0.0 });
}
fn take_hand(game: &mut Game<'_>, slot: usize) {
    super::sunset_crystal_aura::take_shared(game, slot, NAME);
}
fn restore_hand(game: &mut Game<'_>) {
    super::sunset_crystal_aura::restore_shared(game, NAME, true);
}
fn idle_hand(game: &mut Game<'_>) {
    super::sunset_crystal_aura::idle_shared(game, NAME, true);
}
pub(super) fn disabled(game: &mut Game<'_>) {
    restore_hand(game);
    reset();
}
pub(super) fn paused(game: &mut Game<'_>) {
    restore_hand(game);
    suspend();
}
pub(super) fn idle(game: &mut Game<'_>) {
    idle_hand(game);
    suspend();
}
fn preaim(_game: &Game<'_>, _m: &Module, action: Action, rating: Rating) {
    state(|s| {
        if s.lookahead.as_ref().is_none_or(|(_, old)| prefer(rating, *old)) {
            s.lookahead = Some((action, rating));
        }
    });
}
pub(super) fn lookahead() -> Option<Rating> {
    state(|s| s.lookahead.map(|(_, r)| r))
}
pub(super) fn advance_look(game: &mut Game<'_>, m: &Module) {
    if !ready(game, m) || tactics::fixed_look(game) || super::sunset_combat_control::owner().is_some() {
        return;
    }
    let Some((action, _)) = state(|s| s.lookahead) else {
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let turn = tactics::next_look(game, look_at(eye, action.hit), m.n("Turn speed"));
    super::sunset::turn(NAME, turn.0, turn.1);
}
