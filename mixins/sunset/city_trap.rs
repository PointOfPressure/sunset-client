// Original native city mining and acknowledged obsidian placement.
use super::sunset::{self, Canvas, Category, Module, hold};
use super::sunset_city_policy::{self as policy, Ledger, SIDES};
use super::sunset_combat_control as control;
use super::sunset_util::{self as util, Target};
use super::{Game, Screen};
use horizon_protocol::packets::{
    Empty,
    play::{self, Clientbound as C, Serverbound as S},
};
use horizon_world::registry::BlockStateId;
use std::{
    any::Any,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const CITY: &str = "Auto City";
const TRAP: &str = "Auto Trap";
type Session = (i32, super::Dim, Arc<dyn Any + Send + Sync>);

pub(super) fn defaults() -> Vec<Module> {
    vec![
    Module::new(CITY,Category::Combat,"Mines one reachable surround block of an enemy in a hole using native break progress. Stops on server-confirmed opening, manual input, or a bounded timeout.")
        .num("Target range",6.0,1.0,10.0,0.5).num("Range",4.5,1.0,4.5,0.1)
        .pick("Damage model",&["Observed equipment","Unmitigated"])
        .num("Min remaining health",4.0,1.0,20.0,0.5)
        .num("Max self damage",12.0,0.0,36.0,0.5)
        .bool("Anti bot",true).bool("Ignore teams",true),
    Module::new(TRAP,Category::Combat,"Places obsidian around an enemy's feet and head. Top block adds one outside support and the roof; Finish roof jump may make two native jumps to reach their faces.")
        .num("Range",4.0,1.0,4.5,0.1).bool("Rotate",true)
        .num("Blocks per tick",4.0,1.0,8.0,1.0).bool("Top block",true).bool("Finish roof jump",true).bool("Show target",true)
        .bool("Anti bot",true).bool("Ignore teams",true),
]
}

#[derive(Clone, Copy, PartialEq)]
struct Face {
    block: [i32; 3],
    normal: [i32; 3],
    point: [f64; 3],
}
#[derive(Clone)]
struct Mining {
    target: u32,
    feet: [i32; 3],
    block: [i32; 3],
    before: BlockStateId,
    tool: usize,
    started: u64,
    waiting: Option<u64>,
    server_air: bool,
    restore: Option<usize>,
    settings: Module,
}
#[derive(Default)]
struct State {
    session: Option<Session>,
    tick: u64,
    mine: Option<Mining>,
    city_finish: Option<control::Finish>,
    city_queued: Option<Instant>,
    trap_queued: Option<Instant>,
    trap_key: Option<(u32, [i32; 3])>,
    trap_target: Option<Target>,
    trap_plan: Vec<[i32; 3]>,
    trap_cursor: usize,
    ledger: Ledger,
    roof_jumps: policy::RoofJumps,
    generation: u64,
    reason: &'static str,
    placements: u64,
    mining_steps: u64,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}
fn same(game: &Game<'_>, old: &Session) -> bool {
    game.remote.as_ref().is_some_and(|r| {
        r.entity_id == old.0
            && game.dim == old.1
            && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ()
    })
}
fn session(game: &Game<'_>) -> Option<Session> {
    let r = game.remote.as_ref()?;
    Some((r.entity_id, game.dim, r.session.counters.clone()))
}
fn basic(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && game.remote.is_some()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && !game.spectator()
        && !game.creative
        && game.selected < 9
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && game.mobs.len() <= 512
        && game.crystals.len() <= 512
}
fn clicks(game: &Game<'_>) -> bool {
    game.left
        || game.right
        || game
            .physical_buttons
            .contains(&winit::event::MouseButton::Left)
        || game
            .physical_buttons
            .contains(&winit::event::MouseButton::Right)
}
fn hands_free(game: &Game<'_>) -> bool {
    game.using.is_none()
        && game.carried.is_none()
        && util::inventory_idle(game)
        && game.riding.is_none()
        && !sunset::with(|s| s.is_on("Blink"))
}
fn totem_due(game: &Game<'_>) -> bool {
    game.offhand.is_none()
        && sunset::with(|s| s.is_on("Auto Totem") || s.is_on("Auto Inventory Totem"))
        && game
            .inventory
            .iter()
            .take(36)
            .flatten()
            .any(|s| &*s.item == "minecraft:totem_of_undying" && s.count == 1)
}

pub(super) fn reset() {
    sunset::cancel_turns(CITY);
    sunset::cancel_turns(TRAP);
    control::release(TRAP);
    state(|s| {
        let generation = s.generation.wrapping_add(1);
        *s = State::default();
        s.generation = generation;
    });
}
fn restore_tool(game: &mut Game<'_>, mine: &Mining) {
    if let Some(slot) = mine.restore {
        if game.selected == mine.tool && slot < 9 {
            hold(game, slot);
        }
    }
}
fn city_current() -> bool {
    state(|s| s.city_finish.as_ref().is_some_and(|guard| guard.current()))
}
fn city_generation_current(generation: u64) -> bool {
    state(|s| {
        s.generation == generation && s.city_finish.as_ref().is_some_and(|guard| guard.current())
    })
}
fn pause_city(game: &mut Game<'_>) {
    sunset::cancel_turns(CITY);
    let (mine, guard) = state(|s| {
        s.generation = s.generation.wrapping_add(1);
        s.city_queued = None;
        let mine = s.mine.clone();
        if let Some(mine) = &mut s.mine {
            mine.restore = None;
        }
        (mine, s.city_finish.take())
    });
    if guard.as_ref().is_some_and(|guard| guard.current()) {
        if let Some(mine) = mine {
            super::remote::sunset_city_mining::stop(game, mine.block);
            restore_tool(game, &mine);
        }
    }
    drop(guard);
}
fn stop_city(game: &mut Game<'_>, reason: &'static str, disable: bool) {
    sunset::cancel_turns(CITY);
    let (mine, guard) = state(|s| {
        s.generation = s.generation.wrapping_add(1);
        s.city_queued = None;
        s.reason = reason;
        (s.mine.take(), s.city_finish.take())
    });
    if guard.as_ref().is_some_and(|guard| guard.current()) {
        if let Some(mine) = &mine {
            super::remote::sunset_city_mining::stop(game, mine.block);
            restore_tool(game, mine);
        }
    }
    drop(guard);
    if disable {
        util::toggle_off(game, CITY);
        sunset::say(game, &format!("Auto City: {reason}"));
    }
}
pub(super) fn cancel(game: &mut Game<'_>) {
    let valid = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    let active = state(|s| s.mine.is_some() || s.city_queued.is_some());
    if valid {
        stop_city(game, "Cancelled", active);
    }
    reset();
    // A screen pause ends active one-shot work but is not a reconnect. Keep
    // the observed connection so an explicit GUI/chat enable can start once
    // that screen closes. Only a real session/dimension change disables saved on.
    if valid {
        state(|s| { s.session=session(game);s.tick=game.ticks; });
    }
}

fn opponents_with(game: &Game<'_>, m: &Module, range: f64, only: Option<u32>) -> Vec<Target> {
    let Some(eye) = game.eye_position() else {
        return Vec::new();
    };
    let friends = sunset::with(|s| s.friends.clone());
    let mut found = Vec::with_capacity(8);
    for mob in &game.mobs {
        if only.is_some_and(|id| id != mob.id) {
            continue;
        }
        if mob.removed
            || !mob.alive()
            || mob.kind.id != "minecraft:player"
            || game
                .remote
                .as_ref()
                .is_some_and(|r| r.entity_id as u32 == mob.id)
            || util::spared(game, m, mob.id, true)
        {
            continue;
        }
        let Some(name) = game.remote_player_name(mob.id) else {
            continue;
        };
        if friends.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            continue;
        }
        let t = util::mob_target(mob);
        if !policy::valid(t.pos)
            || !policy::valid(t.lo)
            || !policy::valid(t.hi)
            || !policy::near(eye, t.pos, range)
        {
            continue;
        }
        found.push(t);
    }
    found.sort_unstable_by(|a, b| util::dist(eye, a.pos).total_cmp(&util::dist(eye, b.pos)));
    found.truncate(3);
    found
}
fn opponents(game: &Game<'_>, m: &Module, range: f64) -> Vec<Target> {
    opponents_with(game, m, range, None)
}
fn opponent(game: &Game<'_>, m: &Module, range: f64, id: u32) -> Option<Target> {
    opponents_with(game, m, range, Some(id)).into_iter().next()
}
fn same_hole(target: &Target, feet: [i32; 3]) -> bool {
    let current = target.pos.map(|n| n.floor() as i32);
    current[0] == feet[0]
        && current[2] == feet[2]
        && (i64::from(current[1]) - i64::from(feet[1])).abs() <= 1
}

fn face_aim(game: &Game<'_>, block: [i32; 3], range: f64) -> Option<Face> {
    face_aim_with(game, block, range, None, &mut 48)
}
fn face_aim_with(
    game: &Game<'_>,
    block: [i32; 3],
    range: f64,
    desired: Option<[i32; 3]>,
    budget: &mut usize,
) -> Option<Face> {
    if !game.world.loaded(block[0], block[2]) {
        return None;
    }
    let eye = game.eye_position()?;
    let mut best = None;
    let faces = [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ];
    // Native outline boxes include stairs and context-sensitive outlines.
    for bounds in game.outline_boxes(block).into_iter().take(8) {
        for normal in faces {
            if desired.is_some_and(|wanted| wanted != normal) {
                continue;
            }
            let mut point = [
                (bounds[0] + bounds[3]) / 2.0,
                (bounds[1] + bounds[4]) / 2.0,
                (bounds[2] + bounds[5]) / 2.0,
            ];
            for i in 0..3 {
                if normal[i] != 0 {
                    point[i] = if normal[i] > 0 {
                        bounds[i + 3]
                    } else {
                        bounds[i]
                    };
                }
            }
            if !policy::near(eye, point, range.min(game.reach())) {
                continue;
            }
            let inside: [f64; 3] = std::array::from_fn(|i| point[i] - f64::from(normal[i]) * 0.025);
            let delta: [f64; 3] = std::array::from_fn(|i| inside[i] - eye[i]);
            let distance = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
            if !distance.is_finite() || distance <= 1e-8 {
                continue;
            }
            if *budget == 0 {
                return best.map(|(_, face)| face);
            }
            *budget -= 1;
            let Some(hit) = game.clip_ray(eye, delta.map(|v| v / distance), distance) else {
                continue;
            };
            if hit.pos != block || hit.face != normal {
                continue;
            }
            let d = util::dist(eye, hit.point);
            if best.is_none_or(|(old, _)| d < old) {
                best = Some((
                    d,
                    Face {
                        block,
                        normal,
                        point: hit.point,
                    },
                ));
            }
        }
    }
    best.map(|(_, face)| face)
}
fn safe_support(game: &Game<'_>, block: [i32; 3]) -> bool {
    let f = util::facts(game, block);
    if f.replaceable || f.block_entity || f.fluid_level > 0 {
        return false;
    }
    ![
        "CraftingTableBlock",
        "RespawnAnchorBlock",
        "AnvilBlock",
        "LoomBlock",
        "CartographyTableBlock",
        "StonecutterBlock",
        "GrindstoneBlock",
        "DoorBlock",
        "TrapDoorBlock",
        "FenceGateBlock",
        "ButtonBlock",
        "LeverBlock",
        "BedBlock",
        "ComposterBlock",
        "CakeBlock",
    ]
    .iter()
    .any(|name| f.has_class(name))
}
fn support(game: &Game<'_>, cell: [i32; 3], range: f64, budget: &mut usize) -> Option<Face> {
    for d in [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ] {
        let block = std::array::from_fn(|i| cell[i] + d[i]);
        if !game.world.loaded(block[0], block[2]) || !safe_support(game, block) {
            continue;
        }
        let normal = d.map(|n| -n);
        let Some(face) = face_aim_with(game, block, range, Some(normal), budget) else {
            continue;
        };
        if face.normal == normal {
            return Some(face);
        }
    }
    None
}
fn can_fill(game: &Game<'_>, cell: [i32; 3]) -> bool {
    util::can_place(game, cell, true)
        && !game.crystals.iter().any(|c| {
            let p = c.position;
            p.x - 1.0 < f64::from(cell[0]) + 1.0
                && p.x + 1.0 > f64::from(cell[0])
                && p.y < f64::from(cell[1]) + 1.0
                && p.y + 2.0 > f64::from(cell[1])
                && p.z - 1.0 < f64::from(cell[2]) + 1.0
                && p.z + 1.0 > f64::from(cell[2])
        })
}
fn tool(game: &Game<'_>, block: [i32; 3]) -> Option<usize> {
    let facts = util::facts(game, block);
    if !facts.hardness.is_finite() || facts.hardness < 0.0 {
        return None;
    }
    let mut best = None;
    for slot in 0..9 {
        let Some(item) = game.inventory[slot].as_ref() else {
            continue;
        };
        if item.count == 0
            || !item.item.ends_with("_pickaxe")
            || item.extra.as_ref().is_some_and(|s| !s.other.is_empty())
            || !game.items().correct_for_drops(Some(item), &facts.name)
        {
            continue;
        }
        let Some(info) = game.items().get(&item.item) else {
            continue;
        };
        if info.max_damage > 0
            && item
                .damage
                .saturating_add(info.tool_damage_per_block.unwrap_or(1))
                >= info.max_damage
        {
            continue;
        }
        let mut speed = game.items().mining_speed(Some(item), &facts.name);
        if speed > 1.0 {
            speed += game
                .stations()
                .enchantments
                .attribute_add(Some(item), "minecraft:mining_efficiency")
                as f32;
        }
        speed *= game.potions.effects.mining_speed_factor();
        if speed.is_finite() && speed > 0.0 && best.is_none_or(|(_, old)| speed > old) {
            best = Some((slot, speed));
        }
    }
    best.map(|(slot, _)| slot)
}
fn in_hole(game: &Game<'_>, feet: [i32; 3]) -> bool {
    SIDES.iter().all(|d| {
        let p = std::array::from_fn(|i| feet[i] + d[i]);
        game.world.loaded(p[0], p[2]) && util::facts(game, p).explosion_resistance >= 600.0
    })
}

fn city_score(
    game: &mut Game<'_>,
    m: &Module,
    target: &Target,
    block: [i32; 3],
    budget: &mut super::sunset_explosive_policy::Budget,
) -> Option<f64> {
    let blast = super::sunset_explosive_damage::Blast::explosion(
        [
            f64::from(block[0]) + 0.5,
            f64::from(block[1]),
            f64::from(block[2]) + 0.5,
        ],
        6.0,
    )
    .ignoring(block);
    let own = super::sunset_explosive_damage::safety_damage(
        super::sunset_explosive_damage::local_damage(game, blast, budget)?,
        m.p("Damage model"),
    );
    let damage =
        super::sunset_explosive_damage::target_damage(game, target, blast, budget)?.estimate;
    let health = game
        .player
        .as_ref()
        .map(|p| f64::from(p.health.health() + p.health.absorption()))?;
    if ![own, damage, health]
        .iter()
        .all(|n| n.is_finite() && *n >= 0.0)
        || own > m.n("Max self damage")
        || health <= own
        || health - own < m.n("Min remaining health")
    {
        return None;
    }
    let score = damage - own * 0.5;
    (score > 0.0).then_some(score)
}

fn select_city(game: &mut Game<'_>, m: &Module, tick: u64) -> Option<Mining> {
    let targets = opponents(game, m, m.n("Target range").clamp(1.0, 10.0));
    let mut budget = super::sunset_explosive_policy::Budget::new(24);
    let mut best = None;
    for target in targets {
        let feet = target.pos.map(|n| n.floor() as i32);
        if !in_hole(game, feet) {
            continue;
        }
        for d in SIDES {
            let block = std::array::from_fn(|i| feet[i] + d[i]);
            let base = [block[0], block[1] - 1, block[2]];
            if !matches!(
                &*util::facts(game, base).name,
                "minecraft:obsidian" | "minecraft:bedrock"
            ) {
                continue;
            }
            let Some(slot) = tool(game, block) else {
                continue;
            };
            if face_aim(game, block, m.n("Range")).is_none() {
                continue;
            }
            let Some(score) = city_score(game, m, &target, block, &mut budget) else {
                continue;
            };
            if best.as_ref().is_none_or(|(old, _)| score > *old) {
                best = Some((
                    score,
                    Mining {
                        target: target.id,
                        feet,
                        block,
                        before: game.world.block(block[0], block[1], block[2]),
                        tool: slot,
                        started: tick,
                        waiting: None,
                        server_air: false,
                        restore: None,
                        settings: m.clone(),
                    },
                ));
            }
        }
    }
    best.map(|(_, mine)| mine)
}

fn city_tick(game: &mut Game<'_>, m: &Module, tick: u64) {
    if clicks(game) {
        stop_city(game, "Stopped for manual input", true);
        return;
    }
    if city_current() {
        let _ = control::claim(CITY);
    }
    if !city_current() && super::remote::sunset_city_mining::busy(game) {
        stop_city(game, "Existing mining has priority", true);
        return;
    }
    if state(|s| s.mine.is_none()) {
        let Some(mine) = select_city(game, m, tick) else {
            stop_city(game, "No safe reachable city block", true);
            return;
        };
        state(|s| s.mine = Some(mine));
    }
    let Some(mut mine) = state(|s| s.mine.clone()) else {
        return;
    };
    if mine.server_air {
        stop_city(game, "Surround opening confirmed", true);
        return;
    }
    if tick < mine.started || tick.saturating_sub(mine.started) >= 200 {
        stop_city(game, "Mining timeout", true);
        return;
    }
    if mine.settings != *m {
        stop_city(game, "Settings changed", true);
        return;
    }
    if city_current()
        && super::remote::sunset_city_mining::progress(game, mine.block).is_some()
        && game.selected != mine.tool
    {
        stop_city(game, "Stopped for hotbar input", true);
        return;
    }
    let target = opponent(game, m, m.n("Target range").clamp(1.0, 10.0), mine.target);
    let Some(target) = target else {
        stop_city(game, "Target unavailable", true);
        return;
    };
    if !same_hole(&target, mine.feet) {
        stop_city(game, "Target left the hole", true);
        return;
    }
    if let Some(waiting) = mine.waiting {
        if tick.saturating_sub(waiting) >= 30 {
            stop_city(game, "No server opening acknowledgement", true);
        }
        return;
    }
    let current = game
        .world
        .block(mine.block[0], mine.block[1], mine.block[2]);
    if current != mine.before {
        if super::remote::sunset_city_mining::predicted(game, mine.block) {
            mine.waiting = Some(tick);
            state(|s| s.mine = Some(mine));
        } else {
            stop_city(game, "Block changed before confirmation", true);
        }
        return;
    }
    if !hands_free(game) || totem_due(game) || !control::available(CITY) {
        pause_city(game);
        return;
    }
    let Some(slot) = tool(game, mine.block) else {
        stop_city(game, "No usable pickaxe", true);
        return;
    };
    if slot != mine.tool {
        stop_city(game, "Mining tool changed", true);
        return;
    }
    let Some(face) = face_aim(game, mine.block, m.n("Range")) else {
        stop_city(game, "Lost block sight or reach", true);
        return;
    };
    if city_current()
        && state(|s| {
            s.city_queued
                .is_some_and(|at| at.elapsed() <= Duration::from_millis(200))
        })
    {
        control::claim(CITY);
        return;
    }
    sunset::cancel_turns(CITY);
    let continuing = city_current();
    if !control::claim(CITY) {
        return;
    }
    if !continuing {
        mine.restore = Some(game.selected);
        let guard = control::finish(CITY);
        if !guard.current() {
            return;
        }
        state(|s| {
            s.generation = s.generation.wrapping_add(1);
            s.city_finish = Some(guard);
            s.mine = Some(mine.clone());
        });
    } else if mine.restore.is_none() {
        mine.restore = Some(game.selected);
        state(|s| s.mine = Some(mine.clone()));
    }
    let Some(connection) = session(game) else {
        return;
    };
    let snapshot = m.clone();
    let selected_before_queue = game.selected;
    let original_tool = game.inventory[mine.tool].clone();
    let queued = Instant::now();
    let generation = state(|s| {
        s.city_queued = Some(queued);
        s.generation
    });
    let eye = game.eye_position().unwrap_or([f64::NAN; 3]);
    let (yaw, pitch) = sunset::look_at(eye, face.point);
    if !sunset::try_turn_then(CITY, yaw, pitch, move |game| {
        if !city_generation_current(generation) {
            return;
        }
        state(|s| s.city_queued = None);
        if same(game, &connection) && game.selected != selected_before_queue {
            stop_city(game, "Stopped for hotbar input", true);
            return;
        }
        if queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !basic(game)
            || !hands_free(game)
            || clicks(game)
            || totem_due(game)
            || !city_generation_current(generation)
            || game.inventory[mine.tool] != original_tool
            || !sunset::with(|s| s.module(CITY).is_some_and(|m| m.on && *m == snapshot))
            || game
                .world
                .block(face.block[0], face.block[1], face.block[2])
                != mine.before
        {
            return;
        }
        let Some(target) = opponent(
            game,
            &snapshot,
            snapshot.n("Target range").clamp(1.0, 10.0),
            mine.target,
        ) else {
            return;
        };
        if !same_hole(&target, mine.feet) {
            return;
        }
        let Some(fresh) = face_aim(game, face.block, snapshot.n("Range")) else {
            return;
        };
        if fresh.normal != face.normal || !policy::near(fresh.point, face.point, 0.1) {
            return;
        }
        hold(game, mine.tool);
        let mut safety_budget = super::sunset_explosive_policy::Budget::new(2);
        if city_score(game, &snapshot, &target, mine.block, &mut safety_budget).is_none() {
            stop_city(game, "Current health or equipment safety changed", true);
            return;
        }
        if !city_generation_current(generation) {
            return;
        }
        if super::remote::sunset_city_mining::step(game, face.block, face.normal, yaw, pitch) {
            game.swing_arm();
            game.send(S::Punch(Empty));
            state(|s| s.mining_steps = s.mining_steps.saturating_add(1));
            if super::remote::sunset_city_mining::predicted(game, face.block) {
                state(|s| {
                    if let Some(m) = &mut s.mine {
                        m.waiting = Some(game.ticks);
                    }
                });
            }
        }
    }) {
        pause_city(game);
    }
}

fn trap_tick(game: &mut Game<'_>, m: &Module, tick: u64) {
    state(|s| s.ledger.expire(tick));
    if clicks(game) || !hands_free(game) || totem_due(game) || !control::available(TRAP) {
        sunset::cancel_turns(TRAP);
        state(|s| s.trap_queued = None);
        control::release(TRAP);
        return;
    }
    if state(|s| {
        s.trap_queued
            .is_some_and(|at| at.elapsed() <= Duration::from_millis(200))
    }) {
        control::claim(TRAP);
        return;
    }
    sunset::cancel_turns(TRAP);
    state(|s| s.trap_queued = None);
    let Some(target) = opponents(game, m, m.n("Range") + 1.5).into_iter().next() else {
        state(|s| {
            s.trap_target = None;
            s.trap_plan.clear();
        });
        return;
    };
    let feet = target.pos.map(|n| n.floor() as i32);
    let key = (target.id, feet);
    let eye = game.eye_position().unwrap_or([f64::NAN; 3]);
    let Some(mut plan) = policy::trap_cells_from(target.lo, target.hi, m.b("Top block"), eye)
    else {
        return;
    };
    let roof_y = (target.lo[1] + 0.01).floor() as i32 + 2;
    // Check the full lower ring before collision filtering. An empty cell
    // occupied by another player is not a completed support column.
    let lower_complete = plan.iter().filter(|p| p[1] < roof_y).all(|p| {
        let facts = util::facts(game, *p);
        game.world.loaded(p[0], p[2]) && !facts.replaceable && facts.full_collision
    });
    plan.sort_unstable_by(|a, b| {
        a[1].cmp(&b[1]).then_with(|| {
            util::dist(eye, util::center_of(*a)).total_cmp(&util::dist(eye, util::center_of(*b)))
        })
    });
    plan.retain(|p| game.world.loaded(p[0], p[2]) && can_fill(game, *p));
    state(|s| {
        if s.trap_key != Some(key) {
            s.ledger.clear();
            s.roof_jumps.clear();
            s.trap_cursor = 0;
            s.trap_key = Some(key);
        }
        s.trap_target = Some(target.clone());
        s.trap_plan = plan.clone();
    });
    let Some(slot) = util::find_hotbar(game, |n| n == "obsidian") else {
        return;
    };
    let mut selected = Vec::with_capacity(8);
    let count = game.inventory[slot]
        .as_ref()
        .map_or(0, |s| s.count as usize);
    let limit = (if m.b("Rotate") {
        1
    } else {
        m.n("Blocks per tick").clamp(1.0, 8.0) as usize
    })
    .min(count);
    if plan.is_empty() || limit == 0 {
        return;
    }
    let start = state(|s| s.trap_cursor % plan.len());
    let mut budget = 96;
    let mut evaluated = 0;
    for offset in 0..plan.len().min(12) {
        if selected.len() == limit {
            break;
        }
        let cell = plan[(start + offset) % plan.len()];
        evaluated += 1;
        let before = game.world.block(cell[0], cell[1], cell[2]).0;
        if !state(|s| s.ledger.can_try(cell, before)) {
            continue;
        }
        let Some(face) = support(game, cell, m.n("Range"), &mut budget) else {
            if budget == 0 {
                break;
            }
            continue;
        };
        if !super::remote::sunset_city_mining::preview(
            game,
            cell,
            face.block,
            face.normal,
            face.point,
            slot,
        ) {
            continue;
        }
        selected.push((cell, before, face));
    }
    state(|s| s.trap_cursor = (start + evaluated) % plan.len());
    if selected.is_empty() {
        let jump = m.b("Top block")
            && m.b("Finish roof jump")
            && lower_complete
            && eye[1] <= f64::from(roof_y) + 0.1
            && plan.iter().all(|p| p[1] == roof_y)
            && state(|s| s.ledger.waiting() == 0)
            && !game
                .physical_keys
                .contains(&winit::keyboard::KeyCode::Space)
            && game.player.as_ref().is_some_and(|p| {
                p.ground.on_ground
                    && !p.flying
                    && !p.fall_flying
                    && !p.fluids.was_touching_water
                    && !p.is_in_lava()
                    && p.last_climbable.is_none()
            });
        if jump && control::claim(TRAP) && state(|s| s.roof_jumps.request(tick)) {
            super::sunset_melee_assist::request_jump(TRAP);
        } else {
            control::release(TRAP);
        }
        return;
    }
    if !control::claim(TRAP) {
        return;
    }
    let Some(connection) = session(game) else {
        control::release(TRAP);
        return;
    };
    let finish = control::finish(TRAP);
    if !finish.current() {
        return;
    }
    let snapshot = m.clone();
    let original = game.inventory[slot].clone();
    let queued = Instant::now();
    let generation = state(|s| {
        s.trap_queued = Some(queued);
        s.generation
    });
    let look = if m.b("Rotate") {
        sunset::look_at(eye, selected[0].2.point)
    } else {
        (game.yaw, game.pitch)
    };
    if !sunset::try_turn_then(TRAP, look.0, look.1, move |game| {
        let _finish = finish;
        if !_finish.current() || state(|s| s.generation != generation) {
            return;
        }
        state(|s| s.trap_queued = None);
        if queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !basic(game)
            || !hands_free(game)
            || clicks(game)
            || !_finish.current()
            || state(|s| s.generation != generation)
            || game.inventory[slot] != original
            || !sunset::with(|s| s.module(TRAP).is_some_and(|m| m.on && *m == snapshot))
        {
            return;
        }
        let fresh = opponent(game, &snapshot, snapshot.n("Range") + 1.5, target.id);
        if fresh.is_none_or(|t| !policy::near(t.pos, target.pos, 0.75)) {
            return;
        }
        let before_slot = game.selected;
        hold(game, slot);
        let mut budget = 64;
        for (cell, before, face) in selected {
            if !can_fill(game, cell)
                || game.world.block(cell[0], cell[1], cell[2]).0 != before
                || !face_aim_with(
                    game,
                    face.block,
                    snapshot.n("Range"),
                    Some(face.normal),
                    &mut budget,
                )
                .is_some_and(|f| {
                    f.block == face.block
                        && f.normal == face.normal
                        && policy::near(f.point, face.point, 0.05)
                })
                || !super::remote::sunset_city_mining::preview(
                    game,
                    cell,
                    face.block,
                    face.normal,
                    face.point,
                    slot,
                )
            {
                continue;
            }
            if snapshot.b("Rotate") {
                let Some(eye) = game.eye_position() else {
                    continue;
                };
                if !game
                    .clip_ray(eye, super::view_vector(look.0, look.1), snapshot.n("Range"))
                    .is_some_and(|hit| hit.pos == face.block && hit.face == face.normal)
                {
                    continue;
                }
            }
            if !state(|s| s.ledger.sent(cell, before, game.ticks)) {
                continue;
            }
            let sequence = util::sequence(game);
            game.send(S::UseItemOn(play::UseItemOn {
                hand: 0,
                pos: face.block,
                face: super::remote::sunset_city_mining::face_index(face.normal),
                cursor: std::array::from_fn(|i| (face.point[i] - f64::from(face.block[i])) as f32),
                inside: false,
                world_border: false,
                sequence,
            }));
            game.swing_arm();
            game.send(S::Punch(Empty));
            state(|s| s.placements = s.placements.saturating_add(1));
        }
        if game.selected == slot {
            hold(game, before_slot);
        }
    }) {
        state(|s| s.trap_queued = None);
    }
}

fn observed(game: &Game<'_>, pos: [i32; 3], id: u32) {
    let state_id = BlockStateId(id);
    if game.pack.assets.registry.state(state_id).is_none() {
        return;
    }
    state(|s| {
        s.ledger.observe(pos);
        if let Some(mine) = &mut s.mine {
            if mine.block == pos && game.res.facts().is_air(state_id) {
                mine.server_air = true;
            }
        }
    });
}
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if !state(|s| s.session.as_ref().is_some_and(|old| same(game, old))) {
        return;
    }
    match packet {
        C::BlockUpdate(p) => {
            if let Ok(id) = u32::try_from(p.state) {
                observed(game, p.pos, id);
            }
        }
        C::SectionBlocksUpdate(p) => {
            for change in p.changes.iter().take(4096) {
                if let Some((pos, id)) = policy::unpack_section(p.section, *change) {
                    observed(game, pos, id);
                }
            }
        }
        _ => {}
    }
}
// Native multiplayer input otherwise aborts autonomous mining every tick when
// the physical attack button is up. Suppress only that owned idle abort.
pub(super) fn suppress_idle_abort(game: &Game<'_>, down: bool) -> bool {
    !down
        && basic(game)
        && hands_free(game)
        && !clicks(game)
        && !totem_due(game)
        && city_current()
        && state(|s| {
            s.mine.as_ref().is_some_and(|m| {
                m.waiting.is_none()
                    && super::remote::sunset_city_mining::progress(game, m.block).is_some()
            })
        })
}
pub(super) fn tick(game: &mut Game<'_>, modules: &[Module], tick: u64) {
    let changed = state(|s| s.session.as_ref().is_none_or(|old| !same(game, old)) || tick < s.tick);
    if changed {
        reset();
        state(|s| s.session = session(game));
        // Auto City is a one-shot command, never an enabled-on-reconnect action.
        if modules.iter().any(|m| m.name == CITY && m.on) {
            util::toggle_off(game, CITY);
        }
        return;
    }
    state(|s| s.tick = tick);
    if !basic(game) {
        cancel(game);
        return;
    }
    let city = modules.iter().find(|m| m.name == CITY && m.on);
    if let Some(m) = city {
        city_tick(game, m, tick);
    } else if state(|s| s.mine.is_some() || s.city_queued.is_some()) {
        stop_city(game, "Disabled", false);
    }
    if let Some(m) = modules.iter().find(|m| m.name == TRAP && m.on) {
        trap_tick(game, m, tick);
    } else {
        sunset::cancel_turns(TRAP);
        control::release(TRAP);
        state(|s| {
            s.trap_queued = None;
            s.trap_plan.clear();
            s.trap_target = None;
            s.ledger.clear();
            s.roof_jumps.clear();
        });
    }
}
pub(super) fn draw(game: &Game<'_>, camera: &sunset::Camera, out: &mut impl Canvas, width: f32) {
    if !sunset::with(|s| s.module(TRAP).is_some_and(|m| m.on && m.b("Show target"))) {
        return;
    }
    let (target, plan) = state(|s| (s.trap_target.clone(), s.trap_plan.clone()));
    if let Some(t) = target {
        super::sunset_visual::draw_box(
            camera,
            out,
            &super::sunset_visual::Look::BOXED,
            t.lo,
            t.hi,
            0xFFFF_A040,
            width,
        );
    }
    for p in plan {
        let lo = p.map(f64::from);
        super::sunset_visual::draw_box(
            camera,
            out,
            &super::sunset_visual::Look::BOXED,
            lo,
            lo.map(|v| v + 1.0),
            0x8080_FFFF,
            width,
        );
    }
    let _ = game;
}
pub(super) fn debug() -> serde_json::Value {
    state(|s| {
        serde_json::json!({"city_block":s.mine.as_ref().map(|m|m.block),
    "city_target":s.mine.as_ref().map(|m|m.target),"city_waiting_server":s.mine.as_ref().is_some_and(|m|m.waiting.is_some()),
    "city_steps":s.mining_steps,"reason":s.reason,"trap_plan":s.trap_plan.len(),"trap_waiting":s.ledger.waiting(),"trap_sent":s.placements})
    })
}
