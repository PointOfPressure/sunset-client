// Sunset's combat modules beyond the core ones, ported from Meteor's source:
// Surround, Self Trap, Self Web, Auto Web, Hole Filler, Burrow,
// Offhand and Auto EXP.

use super::sunset::{Category, Module, gui_open, hold, with};
use super::sunset_util::*;
use super::Game;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

const HORIZONTAL: [[i32; 3]; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

pub(super) fn defaults() -> Vec<Module> {
    use Category::Combat;
    let mut modules = vec![
        Module::new("Surround", Combat, "Surrounds you in blocks to prevent massive crystal damage.")
            .text("Blocks", TRAP_BLOCKS)
            .num("Delay (ticks)", 0.0, 0.0, 10.0, 1.0)
            .num("Blocks per tick", 1.0, 1.0, 8.0, 1.0)
            .pick("Center", &["Never", "On activate", "Always", "Incomplete"])
            .bool("Double height", false)
            .bool("Only on ground", true)
            .bool("Air place", true)
            .bool("Rotate", true)
            .bool("Toggle on Y change", true)
            .bool("Toggle on complete", false),
        Module::new("Self Trap", Combat, "Places blocks above your head.")
            .text("Blocks", "obsidian,netherite_block")
            .pick("Top mode", &["Top", "Anti face place", "Full", "None"])
            .pick("Bottom mode", &["None", "Single"])
            .num("Place delay", 1.0, 0.0, 10.0, 1.0)
            .bool("Center", true)
            .bool("Turn off", true)
            .bool("Rotate", true),
        Module::new("Self Web", Combat, "Automatically places webs on you.")
            .pick("Mode", &["Normal", "Smart"])
            .num("Range", 3.0, 1.0, 7.0, 0.5)
            .bool("Double place", false)
            .bool("Auto toggle", true)
            .bool("Rotate", true),
        Module::new("Auto Web", Combat, "Automatically places webs on other players.")
            .num("Place range", 4.0, 1.0, 6.0, 0.5)
            .num("Walls range", 4.0, 1.0, 6.0, 0.5)
            .pick("Priority", PRIORITIES)
            .num("Target range", 10.0, 1.0, 20.0, 1.0)
            .bool("Predict movement", true)
            .num("Ticks to predict", 10.0, 1.0, 30.0, 1.0)
            .bool("Doubles", false)
            .bool("Rotate", true),
        Module::new("Hole Filler", Combat, "Fills holes with specified blocks.")
            .text("Blocks", "obsidian,crying_obsidian,netherite_block,respawn_anchor,cobweb")
            .num("Search radius", 5.0, 1.0, 8.0, 1.0)
            .num("Place range", 4.5, 1.0, 6.0, 0.5)
            .num("Walls range", 4.5, 1.0, 6.0, 0.5)
            .bool("Doubles", true)
            .bool("Rotate", false)
            .num("Place delay", 1.0, 0.0, 10.0, 1.0)
            .num("Blocks per tick", 3.0, 1.0, 8.0, 1.0)
            .bool("Smart", true)
            .bool("Predict movement", true)
            .num("Ticks to predict", 10.0, 1.0, 30.0, 1.0)
            .bool("Ignore safe", true)
            .bool("Only moving", true)
            .num("Target range", 7.0, 1.0, 20.0, 1.0)
            .num("Feet range", 1.5, 0.5, 4.0, 0.5),
        Module::new("Burrow", Combat, "Jumps and places a block in the space you stood in, so you end up inside it.")
            .pick("Block", &["E-Chest", "Obsidian", "Anvil", "Held"])
            .num("Trigger height", 1.12, 0.5, 1.5, 0.01)
            .num("Rubberband height", 12.0, 3.0, 20.0, 1.0)
            .bool("Center", true)
            .bool("Rotate", true),
        Module::new("Offhand", Combat, "Holds the chosen item in your offhand, and a totem when your health is low (Meteor's Offhand). Right gapple and Sword pot swap in a gapple or a potion while you hold right click.")
            .num("Item switch delay", 0.0, 0.0, 20.0, 1.0)
            .pick("Item", &["Crystal", "Totem", "Gapple", "Enchanted gapple", "Shield", "Potion"])
            .bool("Hotbar", false)
            .bool("Right gapple", false)
            .bool("Sword gapple", false)
            .bool("Always gap on sword", false)
            .bool("Always pot on sword", false)
            .bool("Sword pot", false)
            .num("Min health", 10.0, 0.0, 36.0, 1.0)
            .bool("Elytra", false)
            .bool("Falling", false)
            .bool("Explosion", true),
        Module::new("Aim Assist", Combat, "Eases your crosshair toward an enemy player while you move the mouse.")
            .num("Range", 4.0, 3.0, 6.0, 0.1)
            .num("FOV", 60.0, 10.0, 180.0, 5.0)
            .num("Speed", 60.0, 10.0, 180.0, 5.0)
            .bool("Weapons Only", true)
            .bool("Vertical", true),
        Module::new("Auto EXP", Combat, "Automatically repairs your armor and tools in pvp.")
            .pick("Mode", &["Both", "Armor", "Hands"])
            .bool("Replenish", true)
            .bool("Only on ground", false)
            .num("EXP slot", 6.0, 1.0, 9.0, 1.0)
            .num("Min threshold %", 30.0, 1.0, 100.0, 1.0)
            .num("Max threshold %", 80.0, 1.0, 100.0, 1.0),
    ];
    modules.extend(super::sunset_city_trap::defaults());
    modules.push(super::sunset_crystal_optimizer::defaults());
    modules
}

#[derive(Default)]
struct Cs {
    on: HashSet<String>,
    surround_timer: u32,
    trap_delay: u32,
    trap_placed: bool,
    trap_positions: Vec<[i32; 3]>,
    web_target: Option<u32>,
    hole_timer: i32,
    burrow: Option<([i32; 3], bool)>,
    aim_before: [f32; 2],
    aim_frame: Option<Instant>,
    aim_target: Option<u32>,
    aim_easing: super::sunset_aim_policy::Easing,
}

static CS: Mutex<Option<Cs>> = Mutex::new(None);

fn cs<R>(f: impl FnOnce(&mut Cs) -> R) -> R {
    let mut g = CS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Cs::default))
}

// True on the tick a module turns on: Meteor's onActivate.
fn activated(name: &str, on: bool) -> bool {
    cs(|c| {
        if on {
            c.on.insert(name.to_owned())
        } else {
            c.on.remove(name);
            false
        }
    })
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    show_clear();
    super::sunset_combat_inventory::tick(game, mods, game.ticks);
    super::sunset_crystal_optimizer::tick(game, game.ticks);
    super::sunset_city_trap::tick(game, mods, game.ticks);
    if super::sunset_combat_control::owner().is_some() { return; }
    let get = |n: &str| mods.iter().find(|m| m.name == n);
    for name in ["Surround", "Self Trap", "Self Web", "Auto Web", "Hole Filler", "Burrow"] {
        let Some(m) = get(name).filter(|m| m.on) else {
            activated(name, false);
            continue;
        };
        // Meteor's onActivate waits for the world: a module turned on from
        // the chat box starts the tick after.
        if !in_world(game) {
            continue;
        }
        let just = activated(name, true);
        match name {
            "Surround" => surround(game, m, just),
            "Self Trap" => self_trap(game, m, just),
            "Self Web" => self_web(game, m),
            "Auto Web" => auto_web(game, m),
            "Hole Filler" => hole_filler(game, m, just),
            "Burrow" => burrow(game, m, just),
            _ => {},
        }
    }
}

fn blocks_slot(game: &Game<'_>, list: &str) -> Option<usize> {
    find_held(game, |n| listed(list, n))
}

// ---- Surround

fn surround(game: &mut Game<'_>, m: &Module, just: bool) {
    if just && m.p("Center") == "On activate" {
        center_player(game);
    }
    let delay = m.n("Delay (ticks)") as u32;
    let waited = cs(|c| {
        let t = c.surround_timer;
        c.surround_timer += 1;
        t
    });
    if waited < delay {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    if m.b("Toggle on Y change") && (game.previous_position.y - p.position.y).abs() > 1e-9 {
        toggle_off(game, "Surround");
        return;
    }
    if m.b("Only on ground") && !p.ground.on_ground {
        return;
    }
    let Some(slot) = blocks_slot(game, m.t("Blocks")) else {
        return;
    };
    if m.p("Center") == "Always" {
        center_player(game);
    }
    let Some(at) = block_pos(game) else {
        return;
    };
    let (per_tick, rotate) = (m.n("Blocks per tick") as u32, m.b("Rotate"));
    let mut placed = 0;
    let mut complete = true;
    let put = |game: &mut Game<'_>, p: [i32; 3], placed: &mut u32| -> bool {
        show(p, 0x40FF_5050);
        place_block(game, p, slot, rotate, true) && {
            *placed += 1;
            *placed >= per_tick
        }
    };
    'sides: for d in HORIZONTAL {
        let p = [at[0] + d[0], at[1], at[2] + d[2]];
        if !m.b("Air place") && !has_support(game, p) && replaceable(game, p) {
            let below = [p[0], p[1] - 1, p[2]];
            if put(game, below, &mut placed) {
                break 'sides;
            }
            if replaceable(game, below) {
                complete = false;
            }
        }
        if put(game, p, &mut placed) {
            break 'sides;
        }
        if replaceable(game, p) {
            complete = false;
        }
    }
    if m.b("Double height") && complete {
        for d in HORIZONTAL {
            let p = [at[0] + d[0], at[1] + 1, at[2] + d[2]];
            if put(game, p, &mut placed) {
                break;
            }
            if replaceable(game, p) {
                complete = false;
            }
        }
    }
    cs(|c| c.surround_timer = 0);
    // Complete once every block is in the world: one sent this tick isn't yet.
    if complete && placed == 0 && m.b("Toggle on complete") {
        toggle_off(game, "Surround");
        return;
    }
    if !complete && m.p("Center") == "Incomplete" {
        center_player(game);
    }
}

// ---- Self Trap

fn self_trap(game: &mut Game<'_>, m: &Module, just: bool) {
    if just {
        cs(|c| {
            c.trap_positions.clear();
            c.trap_delay = 0;
            c.trap_placed = false;
        });
        if m.b("Center") {
            center_player(game);
        }
    }
    let slot = blocks_slot(game, m.t("Blocks"));
    let (placed, empty) = cs(|c| (c.trap_placed, c.trap_positions.is_empty()));
    if m.b("Turn off") && ((placed && empty) || slot.is_none()) {
        if slot.is_none() {
            report(game, "No Self Trap blocks in your hotbar or offhand.");
        }
        toggle_off(game, "Self Trap");
        return;
    }
    let Some(slot) = slot else {
        cs(|c| c.trap_positions.clear());
        return;
    };
    let Some(at) = feet_block(game) else {
        return;
    };
    let mut want: Vec<[i32; 3]> = Vec::new();
    let sides = |y: i32| HORIZONTAL.iter().map(move |d| [d[0], y, d[2]]);
    match m.p("Top mode") {
        "Full" => {
            want.push([0, 2, 0]);
            want.extend(sides(1));
        }
        "Top" => want.push([0, 2, 0]),
        "Anti face place" => want.extend(sides(1)),
        _ => {}
    }
    if m.p("Bottom mode") == "Single" {
        want.push([0, -1, 0]);
    }
    let positions: Vec<[i32; 3]> = want
        .into_iter()
        .map(|d| [at[0] + d[0], at[1] + d[1], at[2] + d[2]])
        .filter(|&p| can_place(game, p, true))
        .collect();
    for &p in &positions {
        show(p, 0x40FF_5050);
    }
    let delay = m.n("Place delay") as u32;
    let go = cs(|c| {
        c.trap_positions = positions.clone();
        let go = c.trap_delay >= delay && !positions.is_empty();
        if !go {
            c.trap_delay += 1;
        }
        go
    });
    if go {
        let p = *positions.last().unwrap_or(&at);
        if place_block(game, p, slot, m.b("Rotate"), true) {
            cs(|c| {
                c.trap_positions.retain(|x| *x != p);
                c.trap_placed = true;
            });
        }
        cs(|c| c.trap_delay = 0);
    }
}

// ---- Auto Trap

fn target_by_id(game: &Game<'_>, id: u32) -> Option<Target> {
    other_players(game).into_iter().find(|t| t.id == id)
}

fn bad_target(game: &Game<'_>, t: &Option<Target>, range: f64) -> bool {
    match (t, game.eye_position()) {
        (Some(t), Some(eye)) => t.health <= 0.0 || dist(eye, t.pos) > range,
        _ => true,
    }
}



// ---- Webs

fn self_web(game: &mut Game<'_>, m: &Module) {
    let go = match m.p("Mode") {
        "Smart" => player_target(game, m.n("Range"), "Lowest distance").is_some(),
        _ => true,
    };
    if !go {
        return;
    }
    let Some(slot) = find_held(game, |n| n == "cobweb") else {
        report(game, "No cobwebs in your hotbar or offhand.");
        if m.b("Auto toggle") {
            toggle_off(game, "Self Web");
        }
        return;
    };
    if let Some(at) = feet_block(game) {
        place_block(game, at, slot, m.b("Rotate"), false);
        if m.b("Double place") {
            place_block(game, [at[0], at[1] + 1, at[2]], slot, m.b("Rotate"), false);
        }
    }
    if m.b("Auto toggle") {
        toggle_off(game, "Self Web");
    }
}

fn auto_web(game: &mut Game<'_>, m: &Module) {
    let range = m.n("Target range");
    let id = cs(|c| c.web_target);
    let mut target = id.and_then(|i| target_by_id(game, i));
    if bad_target(game, &target, range) {
        target = player_target(game, range, m.p("Priority"));
        cs(|c| c.web_target = target.as_ref().map(|t| t.id));
    }
    let Some(t) = target else {
        return;
    };
    let Some(slot) = find_held(game, |n| n == "cobweb") else {
        return;
    };
    let mut pos = t.pos;
    if m.b("Predict movement") {
        let k = m.n("Ticks to predict");
        for i in 0..3 {
            pos[i] += (t.pos[i] - t.prev[i]) * k;
        }
    }
    let at = [pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32];
    let (range, walls) = (m.n("Place range"), m.n("Walls range"));
    let ok = |game: &Game<'_>, p: [i32; 3]| replaceable(game, p) && in_reach(game, p, range, walls);
    if ok(game, at) {
        place_block(game, at, slot, m.b("Rotate"), false);
        show(at, 0x40FF_FFFF);
    }
    let up = [at[0], at[1] + 1, at[2]];
    if m.b("Doubles") && ok(game, up) {
        place_block(game, up, slot, m.b("Rotate"), false);
        show(up, 0x40FF_FFFF);
    }
}

// ---- Hole Filler

// `getExplosionResistance() >= 600`: obsidian, bedrock and the like.
fn hard(game: &Game<'_>, p: [i32; 3]) -> bool {
    !is_air(game, p) && facts(game, p).explosion_resistance >= 600.0
}

fn hole_filler(game: &mut Game<'_>, m: &Module, _just: bool) {
    let Some(me) = block_pos(game) else {
        return;
    };
    let Some(slot) = blocks_slot(game, m.t("Blocks")) else {
        return;
    };
    let smart = m.b("Smart");
    let targets: Vec<Target> = if smart {
        let eye = game.eye_position().unwrap_or([0.0; 3]);
        other_players(game)
            .into_iter()
            .filter(|t| dist(eye, t.pos) <= m.n("Target range"))
            .filter(|t| !(m.b("Ignore safe") && is_surrounded_hard(game, t)))
            .filter(|t| !(m.b("Only moving") && t.pos == t.prev))
            .collect()
    } else {
        Vec::new()
    };
    let (range, walls) = (m.n("Place range"), m.n("Walls range"));
    let close = |t: &Target, p: [i32; 3]| {
        let mut pos = t.pos;
        if m.b("Predict movement") {
            let k = m.n("Ticks to predict");
            for i in 0..3 {
                pos[i] += (t.pos[i] - t.prev[i]) * k;
            }
        }
        let d = [pos[0] - (f64::from(p[0]) + 0.5), pos[1] - (f64::from(p[1]) + 1.0), pos[2] - (f64::from(p[2]) + 0.5)];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < m.n("Feet range")
    };
    let valid = |game: &Game<'_>, p: [i32; 3]| -> bool {
        if !can_place(game, p, true) || !replaceable(game, [p[0], p[1] + 1, p[2]]) {
            return false;
        }
        let top = [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.999, f64::from(p[2]) + 0.5];
        let d = game.eye_position().map_or(f64::MAX, |eye| dist(eye, top));
        // `isOutOfRange`: past the range, or in the way and past the walls range.
        if d > range || (!clear_line(game, top, p) && d > walls) {
            return false;
        }
        !smart || targets.iter().any(|t| t.pos[1] > f64::from(p[1]) && close(t, p))
    };
    let r = m.n("Search radius") as i32;
    let mut holes: Vec<[i32; 3]> = Vec::new();
    const DIRS: [[i32; 3]; 5] = [[0, -1, 0], [1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
    for x in me[0] - r..=me[0] + r {
        for y in me[1] - r..=me[1] + r {
            for z in me[2] - r..=me[2] + r {
                let p = [x, y, z];
                if !valid(game, p) {
                    continue;
                }
                let mut surround = 0;
                let mut air: Option<[i32; 3]> = None;
                let mut bad = false;
                for d in DIRS {
                    let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
                    if hard(game, n) {
                        surround += 1;
                    } else if d == [0, -1, 0] {
                        bad = true;
                        break;
                    } else if valid(game, n) && air.is_none() {
                        for e in DIRS {
                            if e == [-d[0], -d[1], -d[2]] {
                                continue;
                            }
                            let n2 = [n[0] + e[0], n[1] + e[1], n[2] + e[2]];
                            if hard(game, n2) {
                                surround += 1;
                            } else {
                                bad = true;
                                break;
                            }
                        }
                        if bad {
                            break;
                        }
                        air = Some(d);
                    }
                }
                if bad {
                    continue;
                }
                if surround == 5 && air.is_none() || surround == 8 && m.b("Doubles") && air.is_some() {
                    holes.push(p);
                }
            }
        }
    }
    for &p in &holes {
        show(p, 0x4000_FF80);
    }
    let timer = cs(|c| {
        c.hole_timer -= 1;
        c.hole_timer
    });
    if timer > 0 || holes.is_empty() {
        return;
    }
    let mut placed = 0;
    for p in holes {
        if placed >= m.n("Blocks per tick") as u32 {
            break;
        }
        if place_block(game, p, slot, m.b("Rotate"), true) {
            placed += 1;
        }
    }
    cs(|c| c.hole_timer = m.n("Place delay") as i32);
}

fn is_surrounded_hard(game: &Game<'_>, t: &Target) -> bool {
    let p = [t.pos[0].floor() as i32, t.pos[1].floor() as i32, t.pos[2].floor() as i32];
    HORIZONTAL.iter().all(|d| hard(game, [p[0] + d[0], p[1], p[2] + d[2]]))
}

// ---- Burrow

fn burrow_slot(game: &Game<'_>, m: &Module) -> Option<usize> {
    match m.p("Block") {
        "E-Chest" => find_hotbar(game, |n| n == "ender_chest"),
        "Anvil" => find_hotbar(game, |n| n.ends_with("anvil")),
        "Held" => Some(game.selected),
        _ => find_hotbar(game, |n| n == "obsidian" || n == "crying_obsidian"),
    }
}

fn burrow(game: &mut Game<'_>, m: &Module, just: bool) {
    let Some(at) = block_pos(game) else {
        return;
    };
    if just {
        // The checks of Meteor's onActivate.
        if !replaceable(game, at) {
            report(game, "Already burrowed.");
            toggle_off(game, "Burrow");
            return;
        }
        let head = [at[0], at[1] + 2, at[2]];
        if !replaceable(game, head) {
            report(game, "Not enough headroom to burrow.");
            toggle_off(game, "Burrow");
            return;
        }
        if burrow_slot(game, m).is_none() {
            report(game, "No burrow block in your hotbar.");
            toggle_off(game, "Burrow");
            return;
        }
        cs(|c| c.burrow = Some((at, false)));
        if let Some(p) = game.player.as_mut() {
            if p.ground.on_ground {
                p.velocity.y = 0.42;
            }
        }
        return;
    }
    let Some((start, _)) = cs(|c| c.burrow) else {
        return;
    };
    let Some(y) = game.player.as_ref().map(|p| p.position.y) else {
        return;
    };
    if y <= f64::from(start[1]) + m.n("Trigger height") {
        return;
    }
    if m.b("Center") {
        center_player(game);
    }
    let rubberband = m.n("Rubberband height");
    // The block first, then the rubberband (the next move packet): a server that took the move first would find the
    // block out of reach.
    let lift = move |game: &mut Game<'_>| {
        if let Some(p) = game.player.as_mut() {
            p.position.y += rubberband;
        }
    };
    match burrow_slot(game, m) {
        Some(slot) => turned_to(game, "Burrow", m.b("Rotate"), center_of(start), move |game| {
            let before = game.selected;
            hold(game, slot);
            let seq = sequence(game);
            game.send(S::UseItemOn(play::UseItemOn {
                hand: 0,
                pos: start,
                face: 1,
                cursor: [0.5, 0.5, 0.5],
                inside: false,
                world_border: false,
                sequence: seq,
            }));
            hold(game, before);
            lift(game);
        }),
        None => lift(game),
    }
    cs(|c| c.burrow = None);
    toggle_off(game, "Burrow");
}

// Offhand, Auto Totem and Auto EXP now share the original acknowledged
// inventory driver in combat_inventory.rs. Their existing module names remain.

// ---- Aim Assist: native mouse-count assistance and current player geometry.

use horizon_client::collisions::SceneCollisions;
use horizon_client::mobs::Mob;
use horizon_core::geometry::{Vec3, VoxelShape};
use horizon_core::position::BlockPos;
use horizon_world::collision_context::{CollisionContext, CollisionEntity, CollisionItem};
use horizon_world::entity_obstruction::EntityIdentity;
use horizon_world::registry::BlockStateId;
use horizon_world::world_ray::{RayBlockMode, RayFluidMode, RayWorld, clip_world};

fn yaw_of(v: [f64; 3]) -> f64 {
    (-v[0]).atan2(v[2]).to_degrees()
}

fn pitch_of(v: [f64; 3]) -> f64 {
    -v[1].atan2((v[0] * v[0] + v[2] * v[2]).sqrt()).to_degrees()
}

// Mth.wrapDegrees.
fn wrap(d: f64) -> f64 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

fn view_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

// A box as its low and high corners.
type Box3 = [[f64; 3]; 2];

fn centre(b: &Box3) -> [f64; 3] {
    std::array::from_fn(|i| (b[0][i] + b[1][i]) / 2.0)
}

fn core_box(mob: &Mob) -> Box3 {
    let bounds = mob.bounds();
    let lo = bounds.min_position();
    let hi = bounds.max_position();
    let low = [lo.x, lo.y, lo.z];
    let high = [hi.x, hi.y, hi.z];
    let inset = [(hi.x-lo.x)*0.25, (hi.y-lo.y)*0.1, (hi.x-lo.x)*0.25];
    [std::array::from_fn(|i| low[i]+inset[i]), std::array::from_fn(|i| high[i]-inset[i])]
}

// The point of the box closest to the look ray, at the box's distance.
fn closest_point(b: &Box3, eye: [f64; 3], look: [f64; 3]) -> [f64; 3] {
    let d = dist(eye, centre(b));
    std::array::from_fn(|i| (eye[i] + look[i] * d).clamp(b[0][i], b[1][i]))
}

// `Level.clip` with `ClipContext.Block.COLLIDER`, no fluids and your collision context.
struct ColliderRays<'a, 'r> {
    scene: SceneCollisions<'a, 'r>,
    context: CollisionContext<'static>,
}

impl RayWorld for ColliderRays<'_, '_> {
    type Error = std::io::Error;
    type Block = BlockStateId;
    type Fluid = ();
    fn block_state(&mut self, p: BlockPos) -> std::io::Result<BlockStateId> {
        Ok(self.scene.world.block(p.x, p.y, p.z))
    }
    fn fluid_state(&mut self, _: BlockPos) -> std::io::Result<()> {
        Ok(())
    }
    fn block_shape(&mut self, state: BlockStateId, p: BlockPos, _: RayBlockMode) -> std::io::Result<VoxelShape> {
        let binding = self.scene.binding;
        binding
            .collision_shape_with_context(&mut self.scene, state, p, &self.context)
            .map_err(|e| std::io::Error::other(format!("{e:?}")))
    }
    fn interaction_shape(&mut self, _: BlockStateId, _: BlockPos) -> std::io::Result<VoxelShape> {
        Ok(VoxelShape::empty())
    }
    fn fluid_is_source(&mut self, _: ()) -> std::io::Result<bool> {
        Ok(false)
    }
    fn fluid_is_empty(&mut self, _: ()) -> std::io::Result<bool> {
        Ok(true)
    }
    fn fluid_is_water(&mut self, _: ()) -> std::io::Result<bool> {
        Ok(false)
    }
    fn fluid_shape(&mut self, _: (), _: BlockPos) -> std::io::Result<VoxelShape> {
        Ok(VoxelShape::empty())
    }
}

// Nothing solid between the eyes and `to` (`d` away): the ray misses, or stops no nearer than 0.01 short of it.
fn clip_clear(game: &Game<'_>, eye: [f64; 3], to: [f64; 3], d: f64) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let mut rays = ColliderRays {
        scene: SceneCollisions { world: &game.world, binding: game.collision, subject: None },
        context: CollisionContext::for_entity(
            CollisionEntity {
                identity: EntityIdentity(0),
                y: p.position.y,
                descending: p.shift_key_down,
                main_hand: Some(CollisionItem::EMPTY),
            },
            false,
        ),
    };
    let from = Vec3::new(eye[0], eye[1], eye[2]);
    clip_world(&mut rays, from, Vec3::new(to[0], to[1], to[2]), RayBlockMode::Collider, RayFluidMode::None, 4096)
        .is_ok_and(|r| r.miss || r.hit.location.distance_squared(from).sqrt() >= d - 0.01)
}

// Return no rotation when the look segment already enters the target's core.
fn aim_error(game: &Game<'_>, eye: [f64; 3], look: [f64; 3], core: &Box3, reach: f64) -> Option<[f64; 2]> {
    use horizon_core::geometry::Aabb;
    let bounds = Aabb::new(Vec3::new(core[0][0], core[0][1], core[0][2]), Vec3::new(core[1][0], core[1][1], core[1][2]));
    let start = Vec3::new(eye[0],eye[1],eye[2]);
    let end = Vec3::new(eye[0]+look[0]*(reach+1.0),eye[1]+look[1]*(reach+1.0),eye[2]+look[2]*(reach+1.0));
    if bounds.clip(start,end).is_some() { return None; }
    let to = sub(closest_point(core,eye,look),eye);
    Some([wrap(yaw_of(to)-f64::from(game.yaw)),pitch_of(to)-f64::from(game.pitch)])
}

fn aim_allowed(game: &Game<'_>, module: &Module) -> bool {
    super::sunset_melee_assist::aim_gate(game) && in_world(game) && !gui_open()
        && !game.spectator() && !super::sunset::turn_pending()
        && !super::sunset_render::camera_takes_mouse()
        && (!module.b("Weapons Only") || game.inventory.get(game.selected).and_then(Option::as_ref)
            .is_some_and(|stack| super::sunset_melee_assist::melee_item(game,Some(stack))))
}

pub(super) fn aim_before(game: &Game<'_>) {
    cs(|state| state.aim_before=[game.yaw,game.pitch]);
}

fn aim_turn(game: &Game<'_>, module: &Module, seconds: f64, user: [f64; 2]) -> [f64; 2] {
    use super::sunset_aim_policy as policy;
    let Some(eye)=game.eye_position() else { return [0.0;2]; };
    let look=view_vector(game.yaw,game.pitch);
    let range=module.n("Range");
    let candidates:Vec<_>=game.mobs.iter().filter(|mob| player_enemy(game,mob))
        .filter(|mob| !mob.invisible || mob.extra.armor.iter().any(Option::is_some))
        .filter_map(|mob| {
            let bounds=mob.bounds();
            let low=bounds.min_position(); let high=bounds.max_position();
            let near=[eye[0].clamp(low.x,high.x),eye[1].clamp(low.y,high.y),eye[2].clamp(low.z,high.z)];
            let distance=dist(eye,near);
            if distance>range+0.5 {return None;}
            let core=core_box(mob);
            let angle=aim_error(game,eye,look,&core,range).map_or(0.0,|e| e[0].hypot(e[1]));
            if angle>module.n("FOV")*0.5*1.3 {return None;}
            let middle=centre(&core);
            if !clip_clear(game,eye,middle,dist(eye,middle)) { return None; }
            Some(policy::Candidate{id:mob.id,distance,angle})
        }).collect();
    let previous=cs(|state| state.aim_target);
    let target=policy::choose(&candidates,range,module.n("FOV"),super::sunset_melee_assist::recent_hit(),previous);
    let error=target.and_then(|id| game.mobs.iter().find(|mob| mob.id==id))
        .and_then(|mob| aim_error(game,eye,look,&core_box(mob),range));
    let milliseconds=game.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let quantum=horizon_client::mouse::degrees_per_count(game.look_options());
    cs(|state| {
        state.aim_target=target;
        state.aim_easing.follow(target,milliseconds);
        state.aim_easing.nudge(milliseconds,user,error,seconds,module.n("Speed"),module.b("Vertical"),quantum)
    })
}

pub(super) fn aim_after(game: &mut Game<'_>) {
    let device=horizon_client::mouse::sunset_mouse_input::take();
    if game.native_window() && game.native_on(super::sunset::ID) && game.native_playing() && !gui_open() {
        super::sunset_crystal_bot::mouse_delta(game,device);
    }
    let responsive=with(|s| {
        if !s.is_on("Auto Totem") || s.is_on("Auto Inventory Totem") {return None;}
        s.module("Auto Totem").cloned()
    });
    if let Some(module)=responsive {
        super::sunset_combat_inventory::responsive(game,std::slice::from_ref(&module));
    }
    let before=cs(|state| state.aim_before);
    let user=[wrap(f64::from(game.yaw-before[0])),f64::from(game.pitch-before[1])];
    if user.iter().any(|v| v.is_finite() && v.abs()>0.0001) {
        super::sunset_sword_bot::mouse_input();
    }
    let now=Instant::now();
    let seconds=cs(|state| state.aim_frame.replace(now)).map_or(0.0,|old| now.saturating_duration_since(old).as_secs_f64());
    let module=with(|s| s.module("Aim Assist").filter(|m| m.on).cloned()).filter(|m| aim_allowed(game,m));
    let Some(module)=module else {
        cs(|state| {state.aim_target=None;state.aim_easing.reset();});
        return;
    };
    let turn=aim_turn(game,&module,seconds,user);
    if turn != [0.0;2] { game.turn_player(turn[0]/0.15,turn[1]/0.15); }
}
