// Original manual explosive transactions. Every sequence originates at a real
// right-click ray; no enemy search, guessed entity id or predicted world edit.
use super::sunset::{self, Category, Module, gui_open, hold, with};
use super::sunset_explosive_damage::{
    Blast, clear_line, local_damage, safety_damage, target_damage,
};
use super::sunset_explosive_policy::{Budget, Limits, in_range, safe};
use super::sunset_macro_policy::{self as policy, Kind, Sequence, Step, Wait};
use super::sunset_util::{self as util, OFFHAND};
use super::{Dim, Game};
use horizon_protocol::{
    connection::Counters,
    packets::{
        Empty,
        play::{self, Serverbound as S},
    },
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::event::MouseButton;

const CRYSTAL: &str = "Crystal Macro";
const ANCHOR: &str = "Anchor Macro";
type Session = Arc<Mutex<Option<Arc<Counters>>>>;

pub(super) fn defaults() -> Vec<Module> {
    let common = |m: Module| {
        m.bool("Switch back", true)
            .num("Max self damage", 8., 0., 36., 0.5)
            .pick("Damage model", &["Observed equipment", "Unmitigated"])
            .num("Max friend damage", 0., 0., 36., 0.5)
            .num("Min remaining health", 4., 1., 20., 0.5)
    };
    vec![
        common(Module::new(CRYSTAL, Category::Combat, "Manual right-click crystal cycle. Waits for observed base, crystal and removal; checks self and friend safety before exploding.")
            .num("Place delay", 2., 0., 20., 1.).num("Break delay", 2., 0., 20., 1.)
            .bool("Auto obsidian", true).num("Obsidian delay", 2., 0., 20., 1.)),
        common(Module::new(ANCHOR, Category::Combat, "Manual anchor sequence outside anchor-compatible dimensions. Double performs two observed cycles at the clicked position. Guard fallback defaults to cancel.")
            .pick("Mode", &["Single", "Double"]).num("Delay", 2., 0., 20., 1.)
            .bool("Safe anchor", true).pick("If unguarded", &["Cancel", "Detonate"])),
    ]
}

#[derive(Clone, Copy, Debug)]
struct Click {
    pos: [i32; 3],
    face: i32,
    hit: [f64; 3],
}
#[derive(Clone, Copy, Debug)]
enum Action {
    Use {
        step: Step,
        click: Click,
        slot: usize,
        destination: [i32; 3],
        before: u32,
    },
    Attack {
        id: u32,
        at: [f64; 3],
        aim: [f64; 3],
    },
}
impl Action {
    fn step(self) -> Step {
        match self {
            Self::Use { step, .. } => step,
            Self::Attack { .. } => Step::CrystalBreak,
        }
    }
    fn aim(self) -> [f64; 3] {
        match self {
            Self::Use { click, .. } => click.hit,
            Self::Attack { aim, .. } => aim,
        }
    }
}
#[derive(Clone)]
struct Run {
    owner: &'static str,
    settings: Arc<Module>,
    sequence: Sequence,
    pos: [i32; 3],
    previous: usize,
    owned: usize,
    session: Session,
    entity: i32,
    dimension: Dim,
    pending: Option<(Action, Option<u64>)>,
    ready: u64,
    crystal: Option<u32>,
    guard: Option<[i32; 3]>,
    token: u64,
    queued_at: Option<Instant>,
    deadline: u64,
}
struct State {
    run: Option<Run>,
    token: u64,
    anchor_latched: bool,
    retries: policy::Retries,
    last_session: Option<Session>,
}
static STATE: Mutex<State> = Mutex::new(State {
    run: None,
    token: 0,
    anchor_latched: false,
    retries: policy::Retries::new(),
    last_session: None,
});
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}
static USER_RIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn user_mouse(game: &Game<'_>, button: MouseButton, pressed: bool) {
    if button == MouseButton::Right {
        let user = !pressed || (super::sunset_combat_control::owner().is_none_or(|o| o == CRYSTAL || o == ANCHOR)
            && game.native_playing() && !gui_open());
        if user { USER_RIGHT.store(pressed, std::sync::atomic::Ordering::Relaxed); }
    }
}
pub(super) fn focus_lost() { USER_RIGHT.store(false, std::sync::atomic::Ordering::Relaxed); }
fn down(game: &Game<'_>) -> bool {
    game.physical_buttons.contains(&MouseButton::Right) || USER_RIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

fn allowed(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && util::in_world(game)
        && !gui_open()
        && !game.creative
        && !game.spectator()
        && game.selected < 9
        && game.using.is_none()
        && game.mining.is_none()
        && !game.left
        && !super::sunset_player::gap_eating()
        && util::inventory_idle(game)
        && !game.keys.contains(&winit::keyboard::KeyCode::ShiftLeft)
        && !game.keys.contains(&winit::keyboard::KeyCode::ShiftRight)
        && !with(|s| ["Blink", "Auto EXP"].iter().any(|n| s.is_on(n)))
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.)
        && game.mobs.len() <= 256
        && game.crystals.len() <= 256
}
fn same_session(game: &Game<'_>, run: &Run) -> bool {
    game.dim == run.dimension
        && game.remote.as_ref().is_some_and(|r| {
            r.entity_id == run.entity && Arc::ptr_eq(&r.session.counters, &run.session)
        })
}
fn enabled(run: &Run) -> bool {
    with(|s| {
        s.module(run.owner)
            .is_some_and(|m| m.on && m == run.settings.as_ref())
    })
}

// GUI/death/world transitions cancel the transaction; restoration is allowed
// only while the same session and selected slot are still owned by this run.
pub(super) fn reset(game: &mut Game<'_>) {
    let run = state(|s| {
        s.token = s.token.wrapping_add(1);
        s.run.take()
    });
    sunset::cancel_turns(CRYSTAL);
    sunset::cancel_turns(ANCHOR);
    if let Some(run) = run {
        if super::sunset_combat_control::owns(run.owner)
            && same_session(game, &run)
            && util::in_world(game)
            && !gui_open()
            && game.using.is_none()
            && !super::sunset_player::gap_eating()
        {
            if let Some(previous) = policy::restore(
                run.previous,
                run.owned,
                game.selected,
                run.settings.b("Switch back"),
            ) {
                hold(game, previous);
            }
        }
        super::sunset_combat_control::release(run.owner);
    }
}
pub(super) fn active() -> bool {
    state(|s| s.run.is_some())
}

fn stack(game: &Game<'_>, slot: usize, name: &str) -> bool {
    let stack = if slot == OFFHAND {
        game.offhand.as_ref()
    } else {
        game.inventory.get(slot).and_then(|v| v.as_ref())
    };
    stack.is_some_and(|s| s.count > 0 && util::short(&s.item) == name)
}
fn item_slot(game: &Game<'_>, name: &str) -> Option<usize> {
    util::find_held(game, |n| n == name).filter(|&slot| stack(game, slot, name))
}
fn anchor_charge(game: &Game<'_>, p: [i32; 3]) -> Option<u8> {
    if !game.world.loaded(p[0], p[2]) {
        return None;
    }
    let b = game.world.block(p[0], p[1], p[2]);
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
fn center(p: [i32; 3], crystal: bool) -> [f64; 3] {
    [
        f64::from(p[0]) + 0.5,
        f64::from(p[1]) + if crystal { 1. } else { 0.5 },
        f64::from(p[2]) + 0.5,
    ]
}
fn name_is(game: &Game<'_>, p: [i32; 3], name: &str) -> bool {
    util::short(&util::facts(game, p).name) == name
}
fn inert(game: &Game<'_>, p: [i32; 3]) -> bool {
    game.world.loaded(p[0], p[2])
        && util::facts(game, p).full_collision
        && matches!(
            util::short(&util::facts(game, p).name),
            "stone"
                | "cobblestone"
                | "obsidian"
                | "crying_obsidian"
                | "bedrock"
                | "deepslate"
                | "cobbled_deepslate"
                | "dirt"
                | "grass_block"
                | "netherrack"
                | "end_stone"
                | "netherite_block"
        )
}
fn visible(game: &Game<'_>, click: Click, budget: &mut Budget) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let Some(end) = policy::outside(click.hit, click.face) else {
        return false;
    };
    game.world.loaded(click.pos[0], click.pos[2])
        && in_range(eye, click.hit, 4.5)
        && clear_line(game, eye, end, None, budget)
}
fn support(game: &Game<'_>, p: [i32; 3], budget: &mut Budget) -> Option<Click> {
    if !game.world.loaded(p[0], p[2]) || !util::can_place(game, p, true) {
        return None;
    }
    for (d, face) in [
        ([0, -1, 0], 1),
        ([0, 1, 0], 0),
        ([0, 0, -1], 3),
        ([0, 0, 1], 2),
        ([-1, 0, 0], 5),
        ([1, 0, 0], 4),
    ] {
        let n = std::array::from_fn(|i| p[i] + d[i]);
        if !inert(game, n) {
            continue;
        }
        let hit = std::array::from_fn(|i| f64::from(n[i]) + 0.5 - f64::from(d[i]) * 0.5);
        let click = Click { pos: n, face, hit };
        if visible(game, click, budget) {
            return Some(click);
        }
    }
    None
}
fn anchor_click(game: &Game<'_>, p: [i32; 3], budget: &mut Budget) -> Option<Click> {
    let eye = game.eye_position()?;
    let c = center(p, false);
    let axis = (0..3).max_by(|&a, &b| (eye[a] - c[a]).abs().total_cmp(&(eye[b] - c[b]).abs()))?;
    let positive = eye[axis] >= c[axis];
    let mut hit = c;
    hit[axis] += if positive { 0.5 } else { -0.5 };
    let face = match (axis, positive) {
        (0, false) => 4,
        (0, true) => 5,
        (1, false) => 0,
        (1, true) => 1,
        (2, false) => 2,
        _ => 3,
    };
    let click = Click { pos: p, face, hit };
    visible(game, click, budget).then_some(click)
}
fn crystal_room(game: &Game<'_>, p: [i32; 3]) -> bool {
    if p[1] < -64
        || p[1] >= 318
        || !game.world.loaded(p[0], p[2])
        || !matches!(
            util::short(&util::facts(game, p).name),
            "obsidian" | "bedrock"
        )
        || !util::is_air(game, [p[0], p[1] + 1, p[2]])
    {
        return false;
    }
    let c = center(p, true);
    let hits = |b: horizon_core::geometry::Aabb| {
        let (a, z) = (b.min_position(), b.max_position());
        a.x < c[0] + 0.5
            && z.x > c[0] - 0.5
            && a.y < c[1] + 2.
            && z.y > c[1]
            && a.z < c[2] + 0.5
            && z.z > c[2] - 0.5
    };
    !game.player.as_ref().is_some_and(|p| hits(p.bounds()))
        && !game.mobs.iter().any(|m| !m.removed && hits(m.bounds()))
        && !game.crystals.iter().any(|e| {
            (e.position.x - c[0]).abs() < 1.5
                && (e.position.z - c[2]).abs() < 1.5
                && e.position.y < c[1] + 2.
                && e.position.y + 2. > c[1]
        })
}
fn crystal_on(game: &Game<'_>, p: [i32; 3]) -> Option<u32> {
    let c = center(p, true);
    game.crystals
        .iter()
        .find(|e| {
            e.id != 0
                && !e.invulnerable
                && in_range(c, [e.position.x, e.position.y, e.position.z], 0.25)
        })
        .map(|e| e.id)
}
fn direction(game: &Game<'_>) -> [f64; 3] {
    let (y, p) = (
        f64::from(game.yaw).to_radians(),
        f64::from(game.pitch).to_radians(),
    );
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}
fn crosshair_crystal(game: &Game<'_>, budget: &mut Budget) -> Option<(u32, [f64; 3], [f64; 3])> {
    let eye = game.eye_position()?;
    let dir = direction(game);
    let mut best = None;
    for e in &game.crystals {
        if e.id == 0 || e.invulnerable {
            continue;
        }
        let at = [e.position.x, e.position.y, e.position.z];
        let Some(hit) = policy::ray(
            eye,
            dir,
            [at[0] - 1., at[1], at[2] - 1.],
            [at[0] + 1., at[1] + 2., at[2] + 1.],
            3.,
        ) else {
            continue;
        };
        let d = util::dist(eye, hit);
        if best.as_ref().is_some_and(|(old, _, _, _)| *old <= d) {
            continue;
        }
        let end = std::array::from_fn(|i| hit[i] + (eye[i] - hit[i]) * 1.0e-5);
        if clear_line(game, eye, end, None, budget) {
            best = Some((d, e.id, at, hit));
        }
    }
    best.map(|(_, id, at, aim)| (id, at, aim))
}
fn safe_blast(game: &mut Game<'_>, settings: &Module, blast: Blast, budget: &mut Budget) -> bool {
    let Some(own) =
        local_damage(game, blast, budget).map(|p| safety_damage(p, settings.p("Damage model")))
    else {
        return false;
    };
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let health = f64::from(p.health.health() + p.health.absorption());
    let names = with(|s| s.friends.clone());
    let own_id = sunset::own_id(game);
    let mut friends = Vec::new();
    for m in &game.mobs {
        if m.removed || !m.alive() || m.kind.id != "minecraft:player" || Some(m.id) == own_id {
            continue;
        }
        let Some(name) = game.remote_player_name(m.id) else {
            return false;
        };
        let t = util::mob_target(m);
        if !t
            .pos
            .iter()
            .chain(t.lo.iter())
            .chain(t.hi.iter())
            .all(|n| n.is_finite())
        {
            return false;
        }
        if names.iter().any(|f| f.eq_ignore_ascii_case(name)) && in_range(blast.center, t.pos, 12.)
        {
            friends.push(t);
        }
    }
    let mut friend: f64 = 0.;
    for t in friends {
        let Some(p) = target_damage(game, &t, blast, budget) else {
            return false;
        };
        friend = friend.max(p.upper);
    }
    safe(
        Limits {
            min_target: 0.,
            max_self: settings.n("Max self damage"),
            max_friend: settings.n("Max friend damage"),
            min_remaining: settings.n("Min remaining health"),
            anti_suicide: true,
        },
        0.,
        own,
        health,
        friend,
    )
}

fn covered(game: &Game<'_>, anchor: [i32; 3], _budget: &mut Budget) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let bounds = p.bounds();
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    let mut blocked = 0;
    let blast = center(anchor, false);
    for y in [lo.y + 0.2, (lo.y + hi.y) * 0.5, hi.y - 0.2] {
        let from = [(lo.x + hi.x) * 0.5, y, (lo.z + hi.z) * 0.5];
        // Only strict intersection with a known inert full cube proves cover.
        // Missed thin cover gives a conservative false negative.
        for sample in 1..16 {
            let t = f64::from(sample) / 16.;
            let at: [f64; 3] = std::array::from_fn(|i| from[i] + (blast[i] - from[i]) * t);
            let cell = at.map(|n| n.floor() as i32);
            if cell != anchor
                && inert(game, cell)
                && (0..3).all(|i| {
                    at[i] - f64::from(cell[i]) > 1.0e-5 && at[i] - f64::from(cell[i]) < 1. - 1.0e-5
                })
            {
                blocked += 1;
                break;
            }
        }
    }
    blocked >= 2
}
fn guard_place(
    game: &Game<'_>,
    anchor: [i32; 3],
    budget: &mut Budget,
) -> Option<([i32; 3], Click)> {
    let at = util::block_pos(game)?;
    let dx = anchor[0] - at[0];
    let dz = anchor[2] - at[2];
    let d = if dx.abs() >= dz.abs() {
        [dx.signum(), 0, 0]
    } else {
        [0, 0, dz.signum()]
    };
    if d == [0, 0, 0] {
        return None;
    }
    let near = [at[0] + d[0], at[1], at[2] + d[2]];
    let far = [anchor[0] - d[0], anchor[1], anchor[2] - d[2]];
    for p in [
        near,
        far,
        [near[0], near[1] + 1, near[2]],
        [far[0], far[1] + 1, far[2]],
    ] {
        if p == anchor {
            continue;
        }
        if let Some(click) = support(game, p, budget) {
            return Some((p, click));
        }
    }
    None
}
fn use_action(
    game: &Game<'_>,
    step: Step,
    click: Click,
    slot: usize,
    destination: [i32; 3],
) -> Action {
    Action::Use {
        step,
        click,
        slot,
        destination,
        before: game
            .world
            .block(destination[0], destination[1], destination[2])
            .0,
    }
}
fn detonator(game: &Game<'_>) -> Option<usize> {
    // Partial charged anchors pass to offhand glowstone; never accidentally
    // charge during a transaction that was approved as a detonation.
    if stack(game, OFFHAND, "glowstone") {
        return None;
    }
    std::iter::once(game.selected).chain(0..9).find(|&slot| {
        game.inventory
            .get(slot)
            .and_then(|s| s.as_ref())
            .is_none_or(|s| {
                s.count == 0
                    || matches!(util::short(&s.item), "totem_of_undying" | "stick")
                    || s.item.ends_with("_sword")
                    || s.item.ends_with("_axe")
                    || matches!(util::short(&s.item), "mace" | "trident")
            })
    })
}
fn recognized_hand(game: &Game<'_>, kind: Kind) -> bool {
    let held = game.inventory.get(game.selected).and_then(|s| s.as_ref());
    let combat = held.is_none_or(|s| {
        s.item.ends_with("_sword")
            || s.item.ends_with("_axe")
            || s.item.ends_with("_pickaxe")
            || matches!(
                util::short(&s.item),
                "mace" | "trident" | "totem_of_undying"
            )
    });
    match kind {
        Kind::Crystal => {
            stack(game, game.selected, "end_crystal")
                || (combat && item_slot(game, "end_crystal").is_some())
        }
        Kind::Anchor => {
            combat
                || stack(game, game.selected, "respawn_anchor")
                || stack(game, game.selected, "glowstone")
        }
    }
}

// Called immediately before vanilla's native use-item operation. True consumes
// that one operation, including repeats during an owned manual sequence.
pub(super) fn intercept_use(game: &mut Game<'_>) -> bool {
    if !down(game) {
        return false;
    }
    if active() {
        return true;
    }
    let anchor = with(|s| s.module(ANCHOR).filter(|m| m.on).cloned());
    if state(|s| s.anchor_latched) && anchor.is_some() {
        return true;
    }
    if !allowed(game) {
        return false;
    }
    let crystal = with(|s| s.module(CRYSTAL).filter(|m| m.on).cloned());
    let mut budget = Budget::new(16);
    let pick = game.pick().map(|hit| (hit.pos, hit.face));
    let on_anchor = pick.is_some_and(|(p, _)| anchor_charge(game, p).is_some());
    let anchor_intent = on_anchor || stack(game, game.selected, "respawn_anchor");
    let (settings, kind, pos, step, cid) = if anchor_intent {
        let Some(m) = anchor.filter(|_| recognized_hand(game, Kind::Anchor)) else {
            return false;
        };
        if game.anchor_works() || stack(game, OFFHAND, "glowstone") {
            return false;
        }
        let Some((p, face)) = pick else {
            return false;
        };
        let target = if on_anchor {
            p
        } else {
            if !inert(game, p) || policy::face(face).is_none() {
                return false;
            }
            std::array::from_fn(|i| p[i] + face[i])
        };
        if !on_anchor && support(game, target, &mut budget).is_none() {
            return false;
        }
        if anchor_charge(game, target) == Some(0) && item_slot(game, "glowstone").is_none() {
            return false;
        }
        (
            m,
            Kind::Anchor,
            target,
            if on_anchor {
                Step::Guard
            } else {
                Step::AnchorPlace
            },
            None,
        )
    } else {
        let Some(m) = crystal.filter(|_| recognized_hand(game, Kind::Crystal)) else {
            return false;
        };
        if let Some((cid, at, _)) = crosshair_crystal(game, &mut budget) {
            let p = [
                at[0].floor() as i32,
                at[1].floor() as i32 - 1,
                at[2].floor() as i32,
            ];
            (m, Kind::Crystal, p, Step::CrystalBreak, Some(cid))
        } else {
            let Some((p, face)) = pick else {
                return false;
            };
            if matches!(
                util::short(&util::facts(game, p).name),
                "obsidian" | "bedrock"
            ) {
                if !crystal_room(game, p) {
                    return false;
                }
                (m, Kind::Crystal, p, Step::CrystalPlace, None)
            } else {
                if !m.b("Auto obsidian") || !inert(game, p) || item_slot(game, "obsidian").is_none()
                {
                    return false;
                }
                let target = std::array::from_fn(|i| p[i] + face[i]);
                if support(game, target, &mut budget).is_none() {
                    return false;
                }
                (m, Kind::Crystal, target, Step::Base, None)
            }
        }
    };
    let owner = if kind == Kind::Anchor {
        ANCHOR
    } else {
        CRYSTAL
    };
    if !super::sunset_combat_control::claim(owner) {
        return false;
    }
    let Some(remote) = game.remote.as_ref() else {
        super::sunset_combat_control::release(owner);
        return false;
    };
    state(|s| {
        if s.last_session
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &remote.session.counters))
        {
            s.retries.clear();
            s.last_session = Some(remote.session.counters.clone());
        }
        s.token = s.token.wrapping_add(1);
        if kind == Kind::Anchor {
            s.anchor_latched = true;
        }
        s.run = Some(Run {
            owner,
            sequence: Sequence::new(kind, step, settings.p("Mode") == "Double"),
            settings: Arc::new(settings),
            pos,
            previous: game.selected,
            owned: game.selected,
            session: remote.session.counters.clone(),
            entity: remote.entity_id,
            dimension: game.dim,
            pending: None,
            ready: game.ticks,
            crystal: cid,
            guard: None,
            token: s.token,
            queued_at: None,
            deadline: game.ticks.saturating_add(60),
        });
    });
    // Ownership starts before the intercepted vanilla packet could be sent.
    true
}

fn observed(game: &Game<'_>, action: Action) -> bool {
    match action {
        Action::Attack { id, .. } => !game.crystals.iter().any(|c| c.id == id),
        Action::Use {
            step,
            destination,
            before,
            ..
        } => match step {
            Step::Base | Step::Guard => name_is(game, destination, "obsidian"),
            Step::CrystalPlace => crystal_on(game, destination).is_some(),
            Step::AnchorPlace => anchor_charge(game, destination).is_some(),
            Step::Charge => {
                game.world
                    .block(destination[0], destination[1], destination[2])
                    .0
                    != before
                    && anchor_charge(game, destination).is_some_and(|n| n > 0)
            }
            Step::Detonate => !game.is_anchor(game.world.block(
                destination[0],
                destination[1],
                destination[2],
            )),
            _ => false,
        },
    }
}
fn next_action(game: &mut Game<'_>, run: &Run, budget: &mut Budget) -> Option<Action> {
    let p = run.pos;
    let step = run.sequence.step;
    match step {
        Step::Base | Step::Guard => {
            let destination = if step == Step::Guard { run.guard? } else { p };
            Some(use_action(
                game,
                step,
                support(game, destination, budget)?,
                item_slot(game, "obsidian")?,
                destination,
            ))
        }
        Step::CrystalPlace => {
            if !crystal_room(game, p) {
                return None;
            }
            let click = Click {
                pos: p,
                face: 1,
                hit: center(p, true),
            };
            if !visible(game, click, budget)
                || !safe_blast(
                    game,
                    &run.settings,
                    Blast {
                        center: center(p, true),
                        power: 6.,
                        ignored_block: None,
                    },
                    budget,
                )
            {
                return None;
            }
            Some(use_action(
                game,
                step,
                click,
                item_slot(game, "end_crystal")?,
                p,
            ))
        }
        Step::CrystalBreak => {
            let id = run.crystal.or_else(|| crystal_on(game, p))?;
            if !state(|s| s.retries.allows(id, game.ticks)) {
                return None;
            }
            // The manual crystal must remain on the real camera ray. Silent
            // automation never searches for a different nearby crystal.
            let (hit_id, at, aim) = crosshair_crystal(game, budget)?;
            if hit_id != id
                || !safe_blast(
                    game,
                    &run.settings,
                    Blast {
                        center: at,
                        power: 6.,
                        ignored_block: None,
                    },
                    budget,
                )
            {
                return None;
            }
            Some(Action::Attack { id, at, aim })
        }
        Step::AnchorPlace => {
            if game.anchor_works() {
                return None;
            }
            Some(use_action(
                game,
                step,
                support(game, p, budget)?,
                item_slot(game, "respawn_anchor")?,
                p,
            ))
        }
        Step::Charge => {
            if game.anchor_works() || anchor_charge(game, p).is_none_or(|n| n >= 4) {
                return None;
            }
            Some(use_action(
                game,
                step,
                anchor_click(game, p, budget)?,
                item_slot(game, "glowstone")?,
                p,
            ))
        }
        Step::Detonate => {
            if game.anchor_works() || anchor_charge(game, p).is_none_or(|n| n == 0) {
                return None;
            }
            if run.settings.b("Safe anchor")
                && run.settings.p("If unguarded") == "Cancel"
                && !covered(game, p, budget)
            {
                return None;
            }
            if !safe_blast(
                game,
                &run.settings,
                Blast {
                    center: center(p, false),
                    power: 5.,
                    ignored_block: Some(p),
                },
                budget,
            ) {
                return None;
            }
            Some(use_action(
                game,
                step,
                anchor_click(game, p, budget)?,
                detonator(game)?,
                p,
            ))
        }
        Step::Done => None,
    }
}

pub(super) fn tick(game: &mut Game<'_>, _modules: &[Module], _ticks: u64) {
    if !game.native_playing() || gui_open() { focus_lost(); }
    if game.ticks % 20 == 0 {
        state(|s| {
            s.retries
                .retain(|id| game.crystals.iter().any(|c| c.id == id))
        });
    }
    if !down(game) {
        state(|s| s.anchor_latched = false);
    }
    let Some(mut run) = state(|s| s.run.clone()) else {
        return;
    };
    if !allowed(game)
        || !same_session(game, &run)
        || !enabled(&run)
        || game.selected != run.owned
        || game.ticks >= run.deadline
        || (run.sequence.kind == Kind::Crystal && !down(game))
        || !super::sunset_combat_control::claim(run.owner)
    {
        reset(game);
        return;
    }
    if let Some((action, sent)) = run.pending {
        let Some(sent) = sent else {
            if run
                .queued_at
                .is_some_and(|t| t.elapsed() > Duration::from_millis(200))
            {
                reset(game);
            }
            return;
        };
        match policy::wait(sent, game.ticks, observed(game, action)) {
            Wait::Pending => return,
            Wait::Expired => {
                reset(game);
                return;
            }
            Wait::Acknowledged => {
                if action.step() == Step::CrystalPlace {
                    run.crystal = crystal_on(game, run.pos);
                }
                run.sequence.acknowledged(action.step());
                run.pending = None;
                state(|s| {
                    if s.run.as_ref().is_some_and(|r| r.token == run.token) {
                        s.run = Some(run.clone());
                    }
                });
            }
        }
    }
    if run.sequence.step == Step::Done {
        reset(game);
        return;
    }
    if game.ticks < run.ready {
        return;
    }
    let mut budget = Budget::new(24);
    if run.sequence.step == Step::Guard {
        if !run.settings.b("Safe anchor") || covered(game, run.pos, &mut budget) {
            run.sequence.acknowledged(Step::Guard);
        } else if run.guard.is_none() {
            run.guard = guard_place(game, run.pos, &mut budget).map(|(p, _)| p);
            if run.guard.is_none() || item_slot(game, "obsidian").is_none() {
                if run.settings.p("If unguarded") == "Cancel" {
                    reset(game);
                    return;
                }
                run.sequence.acknowledged(Step::Guard);
            }
        }
    }
    if run.sequence.step == Step::Charge && anchor_charge(game, run.pos).is_some_and(|n| n > 0) {
        run.sequence.acknowledged(Step::Charge);
    }
    state(|s| {
        if s.run.as_ref().is_some_and(|r| r.token == run.token) {
            s.run = Some(run.clone());
        }
    });
    let Some(action) = next_action(game, &run, &mut budget) else {
        reset(game);
        return;
    };
    queue(game, &run, action);
}

fn queue(game: &mut Game<'_>, run: &Run, action: Action) {
    let Some(eye) = game.eye_position() else {
        reset(game);
        return;
    };
    let token = run.token;
    let owner = run.owner;
    let queued = Instant::now();
    state(|s| {
        if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
            r.pending = Some((action, None));
            r.queued_at = Some(queued);
        }
    });
    let (yaw, pitch) = sunset::look_at(eye, action.aim());
    let accepted = sunset::try_turn_then(owner, yaw, pitch, move |game| {
        let Some(run) = state(|s| s.run.clone())
            .filter(|r| r.token == token && r.pending.is_some_and(|(_, sent)| sent.is_none()))
        else {
            return;
        };
        if queued.elapsed() > Duration::from_millis(200)
            || !allowed(game)
            || !same_session(game, &run)
            || !enabled(&run)
            || game.selected != run.owned
            || !super::sunset_combat_control::owns(owner)
            || !game
                .eye_position()
                .is_some_and(|now| in_range(now, eye, 0.5))
            || (run.sequence.kind == Kind::Crystal && !down(game))
        {
            reset(game);
            return;
        }
        let mut budget = Budget::new(24);
        let Some(fresh) = next_action(game, &run, &mut budget) else {
            reset(game);
            return;
        };
        // A stale look must never be reused for a changed hand, face or entity.
        let same = match (action, fresh) {
            (Action::Attack { id, at, .. }, Action::Attack { id: b, at: c, .. }) => {
                id == b && in_range(at, c, 0.01)
            }
            (
                Action::Use {
                    step,
                    click,
                    slot,
                    destination,
                    before,
                },
                Action::Use {
                    step: b,
                    click: c,
                    slot: d,
                    destination: e,
                    before: f,
                },
            ) => {
                step == b
                    && slot == d
                    && destination == e
                    && before == f
                    && click.pos == c.pos
                    && click.face == c.face
                    && click.hit == c.hit
            }
            _ => false,
        };
        if !same {
            reset(game);
            return;
        }
        match action {
            Action::Attack { id, .. } => {
                game.send(S::Attack(play::Attack { id: id as i32 }));
                game.reset_attack_strength();
                state(|s| s.retries.sent(id, game.ticks));
            }
            Action::Use { click, slot, .. } => {
                if slot != OFFHAND {
                    hold(game, slot);
                    state(|s| {
                        if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
                            r.owned = slot;
                        }
                    });
                }
                let sequence = util::sequence(game);
                game.send(S::UseItemOn(play::UseItemOn {
                    hand: i32::from(slot == OFFHAND),
                    pos: click.pos,
                    face: click.face,
                    cursor: std::array::from_fn(|i| {
                        (click.hit[i] - f64::from(click.pos[i])) as f32
                    }),
                    inside: false,
                    world_border: false,
                    sequence,
                }));
            }
        }
        game.swing_arm();
        game.send(S::Punch(Empty));
        let delay = match action.step() {
            Step::Base => run.settings.n("Obsidian delay"),
            Step::CrystalPlace => run.settings.n("Place delay"),
            Step::CrystalBreak => run.settings.n("Break delay"),
            _ => run.settings.n("Delay"),
        };
        state(|s| {
            if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
                r.pending = Some((action, Some(game.ticks)));
                r.queued_at = None;
                r.ready = game.ticks.saturating_add(delay.clamp(0., 20.) as u64 + 1);
                r.deadline = r.ready.saturating_add(60);
            }
        });
    });
    if !accepted {
        state(|s| {
            if let Some(r) = s.run.as_mut().filter(|r| r.token == token) {
                r.pending = None;
                r.queued_at = None;
            }
        });
    }
}
