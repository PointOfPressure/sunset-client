// Solstice's combat modules beyond the core ones, ported from Meteor's source:
// Surround, Self Trap, Auto Trap, Self Web, Auto Web, Hole Filler, Burrow,
// Offhand and Auto EXP.

use super::solstice::{Category, Module, hold};
use super::solstice_util::*;
use super::{Game, Screen};
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::HashSet;
use std::sync::Mutex;

const HORIZONTAL: [[i32; 3]; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

pub(super) fn defaults() -> Vec<Module> {
    use Category::Combat;
    vec![
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
        Module::new("Auto Trap", Combat, "Traps people in a box to prevent them from moving.")
            .text("Blocks", "obsidian,crying_obsidian")
            .num("Place range", 4.0, 1.0, 6.0, 0.5)
            .num("Walls range", 4.0, 1.0, 6.0, 0.5)
            .pick("Priority", PRIORITIES)
            .num("Target range", 3.0, 1.0, 10.0, 0.5)
            .num("Place delay", 1.0, 0.0, 10.0, 1.0)
            .num("Blocks per tick", 1.0, 1.0, 8.0, 1.0)
            .pick("Top blocks", &["Full", "Face", "Top", "None"])
            .pick("Bottom blocks", &["Platform", "Full", "Single", "None"])
            .bool("Self toggle", true)
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
        Module::new("Offhand", Combat, "Holds the chosen item in your offhand, and a totem when your health is low.")
            .pick("Item", &["Crystal", "Totem", "Gapple", "Enchanted gapple", "Shield", "Potion"])
            .bool("Hotbar", false)
            .num("Switch delay (ticks)", 0.0, 0.0, 20.0, 1.0)
            .num("Min health", 10.0, 1.0, 36.0, 1.0)
            .bool("Elytra", false),
        Module::new("Aim Assist", Combat, "Adds to your own aim toward a player or mob in front of you; never moves you by itself (as Duelkit's).")
            .num("Range", 4.5, 1.0, 8.0, 0.5)
            .num("Cone (degrees)", 40.0, 10.0, 120.0, 5.0)
            .num("Pull", 0.3, 0.0, 1.0, 0.05)
            .num("Stickiness", 0.3, 0.0, 1.0, 0.05)
            .num("Tracking", 0.5, 0.0, 1.0, 0.05)
            .num("Track within (degrees)", 8.0, 2.0, 30.0, 1.0)
            .num("Keep target", 1.5, 1.0, 3.0, 0.1)
            .num("Inset", 0.1, 0.0, 0.4, 0.05)
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", false)
            .bool("Only with a weapon", true),
        Module::new("Crystal Optimizer", Combat, "Breaks the end crystal you hit on your screen at once, without waiting for the server (as Marlow's Crystal Optimizer).")
            .bool("Instant break", true)
            .num("Reach", 4.5, 3.0, 6.0, 0.5)
            .bool("Swing", true),
        Module::new("Auto EXP", Combat, "Automatically repairs your armor and tools in pvp.")
            .pick("Mode", &["Both", "Armor", "Hands"])
            .bool("Replenish", true)
            .bool("Only on ground", false)
            .num("EXP slot", 6.0, 1.0, 9.0, 1.0)
            .num("Min threshold %", 30.0, 1.0, 100.0, 1.0)
            .num("Max threshold %", 80.0, 1.0, 100.0, 1.0),
    ]
}

#[derive(Default)]
struct Cs {
    on: HashSet<String>,
    surround_timer: u32,
    trap_delay: u32,
    trap_placed: bool,
    trap_positions: Vec<[i32; 3]>,
    auto_trap_target: Option<u32>,
    auto_trap_timer: u32,
    auto_trap_placed: bool,
    web_target: Option<u32>,
    hole_timer: i32,
    burrow: Option<([i32; 3], bool)>,
    offhand_ticks: u32,
    exp_repairing: Option<usize>,
    aim_target: Option<u32>,
    aim_last: [f64; 2],
    aim_user: [f64; 2],
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
    let get = |n: &str| mods.iter().find(|m| m.name == n);
    for name in ["Surround", "Self Trap", "Auto Trap", "Self Web", "Auto Web", "Hole Filler", "Burrow", "Offhand", "Auto EXP"] {
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
            "Auto Trap" => auto_trap(game, m, just),
            "Self Web" => self_web(game, m),
            "Auto Web" => auto_web(game, m),
            "Hole Filler" => hole_filler(game, m, just),
            "Burrow" => burrow(game, m, just),
            "Offhand" => offhand(game, m),
            _ => auto_exp(game, m),
        }
    }
}

fn blocks_slot(game: &Game<'_>, list: &str) -> Option<usize> {
    find_hotbar(game, |n| listed(list, n))
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
    let mut put = |game: &mut Game<'_>, p: [i32; 3], placed: &mut u32| -> bool {
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
    if complete && m.b("Toggle on complete") {
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
        toggle_off(game, "Self Trap");
        return;
    }
    let Some(slot) = slot else {
        cs(|c| c.trap_positions.clear());
        return;
    };
    let Some(at) = block_pos(game) else {
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

fn auto_trap(game: &mut Game<'_>, m: &Module, just: bool) {
    if just {
        cs(|c| {
            c.auto_trap_target = None;
            c.auto_trap_timer = 0;
            c.auto_trap_placed = false;
        });
    }
    let (placed, target_id) = cs(|c| (c.auto_trap_placed, c.auto_trap_target));
    let _ = placed;
    let Some(slot) = blocks_slot(game, m.t("Blocks")) else {
        return;
    };
    let range = m.n("Target range");
    let mut target = target_id.and_then(|id| target_by_id(game, id));
    if bad_target(game, &target, range) {
        target = player_target(game, range, m.p("Priority"));
        cs(|c| c.auto_trap_target = target.as_ref().map(|t| t.id));
        if target.is_none() {
            // Everything placed and nobody left to trap.
            if m.b("Self toggle") && cs(|c| std::mem::take(&mut c.auto_trap_placed)) {
                toggle_off(game, "Auto Trap");
            }
            return;
        }
    }
    let Some(t) = target else {
        return;
    };
    let (place_range, walls) = (m.n("Place range"), m.n("Walls range"));
    let eps = 1e-5;
    let y = t.lo[1].floor() as i32;
    let mut corners: Vec<[i32; 2]> = vec![
        [t.lo[0].floor() as i32, t.lo[2].floor() as i32],
        [t.lo[0].floor() as i32, (t.hi[2] - eps).floor() as i32],
        [(t.hi[0] - eps).floor() as i32, t.lo[2].floor() as i32],
        [(t.hi[0] - eps).floor() as i32, (t.hi[2] - eps).floor() as i32],
    ];
    corners.dedup();
    let mut positions: Vec<[i32; 3]> = Vec::new();
    let mut add = |game: &Game<'_>, p: [i32; 3]| {
        if !positions.contains(&p) && can_place(game, p, true) && in_reach(game, p, place_range, walls) {
            positions.push(p);
        }
    };
    for [x, z] in corners {
        let at = |dx: i32, dy: i32, dz: i32| [x + dx, y + dy, z + dz];
        match m.p("Top blocks") {
            "Full" => {
                for d in [at(0, 2, 0), at(1, 1, 0), at(-1, 1, 0), at(0, 1, 1), at(0, 1, -1)] {
                    add(game, d);
                }
            }
            "Face" => {
                for d in [at(1, 1, 0), at(-1, 1, 0), at(0, 1, 1), at(0, 1, -1)] {
                    add(game, d);
                }
            }
            "Top" => add(game, at(0, 2, 0)),
            _ => {}
        }
        match m.p("Bottom blocks") {
            "Platform" => {
                for d in [at(0, -1, 0), at(1, -1, 0), at(-1, -1, 0), at(0, -1, 1), at(0, -1, -1)] {
                    add(game, d);
                }
            }
            "Full" => {
                for d in [at(0, -1, 0), at(1, 0, 0), at(-1, 0, 0), at(0, 0, -1), at(0, 0, 1)] {
                    add(game, d);
                }
            }
            "Single" => add(game, at(0, -1, 0)),
            _ => {}
        }
    }
    // The furthest first.
    if let Some(me) = player_pos(game) {
        positions.sort_by(|a, b| dist(me, center_of(*b)).total_cmp(&dist(me, center_of(*a))));
    }
    for &p in &positions {
        show(p, 0x40FF_5050);
    }
    if m.b("Self toggle") && cs(|c| c.auto_trap_placed) && positions.is_empty() {
        cs(|c| c.auto_trap_placed = false);
        toggle_off(game, "Auto Trap");
        return;
    }
    let delay = m.n("Place delay") as u32;
    let ready = cs(|c| {
        if c.auto_trap_timer >= delay && !positions.is_empty() {
            c.auto_trap_timer = 0;
            true
        } else {
            c.auto_trap_timer += 1;
            false
        }
    });
    if ready {
        let mut count = 0;
        for p in positions {
            if count >= m.n("Blocks per tick") as u32 {
                break;
            }
            if place_block(game, p, slot, m.b("Rotate"), true) {
                cs(|c| c.auto_trap_placed = true);
                count += 1;
            }
        }
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
    let Some(slot) = find_hotbar(game, |n| n == "cobweb") else {
        if m.b("Auto toggle") {
            toggle_off(game, "Self Web");
        }
        return;
    };
    if let Some(at) = block_pos(game) {
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
    let Some(slot) = find_hotbar(game, |n| n == "cobweb") else {
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
    if let Some(slot) = burrow_slot(game, m) {
        let hit = center_of(start);
        if m.b("Rotate") {
            if let Some(eye) = game.eye_position() {
                let (yaw, pitch) = super::solstice::look_at(eye, hit);
                game.send(S::MovePlayerRot(play::MovePlayerRot { yaw, pitch, flags: 0 }));
            }
        }
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
    }
    if let Some(p) = game.player.as_mut() {
        p.position.y += m.n("Rubberband height");
    }
    cs(|c| c.burrow = None);
    toggle_off(game, "Burrow");
}

// ---- Offhand

fn offhand_wants(m: &Module) -> &'static [&'static str] {
    match m.p("Item") {
        "Totem" => &["totem_of_undying"],
        "Gapple" => &["golden_apple"],
        "Enchanted gapple" => &["enchanted_golden_apple"],
        "Shield" => &["shield"],
        "Potion" => &["potion", "splash_potion"],
        _ => &["end_crystal"],
    }
}

fn offhand(game: &mut Game<'_>, m: &Module) {
    use horizon_client::menu::Input::Pickup;
    if !inventory_idle(game) {
        return;
    }
    let ticks = cs(|c| {
        c.offhand_ticks += 1;
        c.offhand_ticks
    });
    if f64::from(ticks) <= m.n("Switch delay (ticks)") {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let low = f64::from(p.health.health()) <= m.n("Min health");
    let gliding = m.b("Elytra") && p.fall_flying;
    let wants: &[&str] = if low || gliding { &["totem_of_undying"] } else { offhand_wants(m) };
    if game.offhand.as_ref().is_some_and(|s| wants.contains(&short(&s.item))) {
        return;
    }
    let from = if m.b("Hotbar") { 0 } else { 9 };
    let found = (from..36).find(|&i| {
        game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| wants.contains(&short(&s.item)))
    });
    let found = found.or_else(|| (0..9).find(|&i| {
        low && game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| wants.contains(&short(&s.item)))
    }));
    if let Some(i) = found {
        // Pick it up, put it in the offhand slot, put back what was there.
        super::solstice::menu_click(game, &[(i, Pickup), (504, Pickup), (i, Pickup)]);
        cs(|c| c.offhand_ticks = 0);
    }
}

// ---- Auto EXP

fn needs_repair(game: &Game<'_>, stack: &Option<horizon_client::items::ItemStack>, threshold: f64) -> bool {
    let Some(s) = stack else {
        return false;
    };
    if s.enchantment_level("minecraft:mending") == 0 {
        return false;
    }
    let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
    max > 0 && f64::from(max.saturating_sub(s.damage)) / f64::from(max) * 100.0 <= threshold
}

fn auto_exp(game: &mut Game<'_>, m: &Module) {
    if m.b("Only on ground") && !game.player.as_ref().is_some_and(|p| p.ground.on_ground) {
        return;
    }
    // 0 to 3 armor, 10 the main hand, 11 the offhand.
    let item_at = |game: &Game<'_>, i: usize| -> Option<horizon_client::items::ItemStack> {
        match i {
            0..=3 => game.armor[i].clone(),
            10 => game.inventory.get(game.selected).cloned().flatten(),
            _ => game.offhand.clone(),
        }
    };
    let mut repairing = cs(|c| c.exp_repairing);
    if repairing.is_none() {
        if m.p("Mode") != "Hands" {
            repairing = (0..4).find(|&i| needs_repair(game, &game.armor[i], m.n("Min threshold %")));
        }
        if m.p("Mode") != "Armor" && repairing.is_none() {
            repairing = [10usize, 11].into_iter().find(|&i| needs_repair(game, &item_at(game, i), m.n("Min threshold %")));
        }
    }
    cs(|c| c.exp_repairing = repairing);
    let Some(i) = repairing else {
        return;
    };
    if !needs_repair(game, &item_at(game, i), m.n("Max threshold %")) {
        cs(|c| c.exp_repairing = None);
        return;
    }
    let slot = match find_hotbar(game, |n| n == "experience_bottle") {
        Some(s) => s,
        None => {
            if !m.b("Replenish") {
                return;
            }
            let Some(src) = find_inventory(game, |n| n == "experience_bottle") else {
                return;
            };
            // Into the chosen hotbar slot.
            let to = m.n("EXP slot") as usize - 1;
            super::solstice::menu_click(game, &[(src, horizon_client::menu::Input::Pickup), (to, horizon_client::menu::Input::Pickup), (src, horizon_client::menu::Input::Pickup)]);
            return;
        }
    };
    // Throw it at your feet.
    let yaw = game.yaw;
    game.send(S::MovePlayerRot(play::MovePlayerRot { yaw, pitch: 90.0, flags: 1 }));
    let before = game.selected;
    hold(game, slot);
    let seq = sequence(game);
    game.send(S::UseItem(play::UseItem { hand: 0, sequence: seq, yaw, pitch: 90.0 }));
    hold(game, before);
}

#[allow(dead_code)]
fn unused(_: Screen) {}

// ---- Aim Assist (Duelkit's, ported)

use std::sync::atomic::{AtomicBool, Ordering};

static AIM_BUSY: AtomicBool = AtomicBool::new(false);

fn yaw_of(v: [f64; 3]) -> f64 {
    (-v[0]).atan2(v[2]).to_degrees()
}

fn pitch_of(v: [f64; 3]) -> f64 {
    -v[1].atan2((v[0] * v[0] + v[2] * v[2]).sqrt()).to_degrees()
}

fn wrap(d: f64) -> f64 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

fn view_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

fn angle_between(a: [f64; 3], b: [f64; 3]) -> f64 {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
    ((a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / n).clamp(-1.0, 1.0).acos()
}

// The point of the target's box nearest the line of sight, the box shrunk by `inset`.
fn closest_point(t: &Target, eye: [f64; 3], look: [f64; 3], inset: f64) -> ([f64; 3], [f64; 3]) {
    let lo = [t.lo[0] + inset, t.lo[1] + inset, t.lo[2] + inset];
    let hi = [t.hi[0] - inset, t.hi[1] - inset, t.hi[2] - inset];
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let d = dist(eye, centre);
    let ray = [eye[0] + look[0] * d, eye[1] + look[1] * d, eye[2] + look[2] * d];
    (
        [ray[0].clamp(lo[0], hi[0]), ray[1].clamp(lo[1], hi[1]), ray[2].clamp(lo[2], hi[2])],
        centre,
    )
}

fn aim_valid(game: &Game<'_>, m: &Module, t: &Target, eye: [f64; 3], look: [f64; 3], half_cone: f64) -> bool {
    let hostile = t.kind.contains("zombie") || t.kind.contains("skeleton") || t.kind.contains("creeper") || t.kind.contains("spider") || t.kind.contains("witch") || t.kind.contains("enderman") || t.kind.contains("husk") || t.kind.contains("drowned") || t.kind.contains("phantom") || t.kind.contains("pillager") || t.kind.contains("vindicator") || t.kind.contains("blaze") || t.kind.contains("piglin");
    let wanted = if t.player { m.b("Players") } else if hostile { m.b("Hostiles") } else { m.b("Passive") };
    if !wanted || t.health <= 0.0 {
        return false;
    }
    let (aim, _) = closest_point(t, eye, look, m.n("Inset"));
    let to = [aim[0] - eye[0], aim[1] - eye[1], aim[2] - eye[2]];
    let d = dist(eye, aim);
    if d < 1e-4 || angle_between(to, look).to_degrees() > half_cone || d > m.n("Range") {
        return false;
    }
    clear_line(game, aim, [i32::MIN; 3])
}

// The user's own mouse movement, noted for the frame's pass (not changed).
pub(super) fn aim_note_user(xo: f64, yo: f64) {
    if AIM_BUSY.load(Ordering::Relaxed) {
        return;
    }
    cs(|c| {
        c.aim_user[0] += xo * 0.15;
        c.aim_user[1] += yo * 0.15;
    });
}

// Once a frame, after the mouse: Duelkit's pull, stickiness and tracking.
pub(super) fn aim_frame(game: &mut Game<'_>, mods: &[Module]) {
    let user = cs(|c| std::mem::take(&mut c.aim_user));
    let Some(m) = mods.iter().find(|m| m.name == "Aim Assist" && m.on) else {
        cs(|c| c.aim_target = None);
        return;
    };
    if !in_world(game) || game.screen != Screen::Playing {
        cs(|c| c.aim_target = None);
        return;
    }
    if m.b("Only with a weapon") {
        let weapon = game.held().is_some_and(|s| {
            let n = short(&s.item);
            n.ends_with("_sword") || n.ends_with("_axe") || n == "mace" || n == "trident" || n.ends_with("spear")
        });
        if !weapon {
            cs(|c| c.aim_target = None);
            return;
        }
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let (yaw, pitch) = (game.yaw, game.pitch);
    let look = view_vector(yaw, pitch);
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let list: Vec<Target> = every_living(game).into_iter().filter(|t| Some(t.id) != own).collect();
    let (cone, keep) = (m.n("Cone (degrees)"), m.n("Keep target"));
    let previous = cs(|c| c.aim_target);
    let mut target = previous.and_then(|id| list.iter().find(|t| t.id == id).cloned());
    if !target.as_ref().is_some_and(|t| aim_valid(game, m, t, eye, look, cone / 2.0 * keep)) {
        let mut best: Option<(f64, Target)> = None;
        for t in &list {
            if !aim_valid(game, m, t, eye, look, cone / 2.0) {
                continue;
            }
            let (aim, _) = closest_point(t, eye, look, m.n("Inset"));
            let a = angle_between([aim[0] - eye[0], aim[1] - eye[1], aim[2] - eye[2]], look);
            if best.as_ref().is_none_or(|(b, _)| a < *b) {
                best = Some((a, t.clone()));
            }
        }
        target = best.map(|(_, t)| t);
    }
    let Some(t) = target else {
        cs(|c| c.aim_target = None);
        return;
    };
    cs(|c| c.aim_target = Some(t.id));
    let (aim, centre) = closest_point(&t, eye, look, m.n("Inset"));
    let to_centre = [centre[0] - eye[0], centre[1] - eye[1], centre[2] - eye[2]];
    let (yaw_to, pitch_to) = (yaw_of(to_centre), pitch_of(to_centre));
    let to = [aim[0] - eye[0], aim[1] - eye[1], aim[2] - eye[2]];
    let gap_yaw = wrap(yaw_of(to) - f64::from(yaw));
    let gap_pitch = pitch_of(to) - f64::from(pitch);
    let gap = gap_yaw.hypot(gap_pitch);
    let on_target = gap < 0.05;
    let (mut turn_yaw, mut turn_pitch) = (0.0, 0.0);
    let last = cs(|c| c.aim_last);
    let within = m.n("Track within (degrees)");
    if previous == Some(t.id) && gap < within {
        let k = m.n("Tracking") * (1.0 - gap / within);
        turn_yaw += wrap(yaw_to - last[0]) * k;
        turn_pitch += (pitch_to - last[1]) * k;
    }
    let user_len = user[0].hypot(user[1]);
    if user_len > 1e-4 {
        let stick = m.n("Stickiness") * (1.0 - gap).clamp(0.0, 1.0);
        turn_yaw -= user[0] * stick;
        turn_pitch -= user[1] * stick;
        if !on_target && gap_yaw * user[0] + gap_pitch * user[1] > 0.0 {
            let extra = gap.min(m.n("Pull") * user_len);
            turn_yaw += gap_yaw / gap * extra;
            turn_pitch += gap_pitch / gap * extra;
        }
    }
    cs(|c| c.aim_last = [yaw_to, pitch_to]);
    if turn_yaw != 0.0 || turn_pitch != 0.0 {
        AIM_BUSY.store(true, Ordering::Relaxed);
        game.turn_player(turn_yaw / 0.15, turn_pitch / 0.15);
        AIM_BUSY.store(false, Ordering::Relaxed);
    }
}

// ---- Crystal Optimizer

// Called when you attack: if an end crystal is on the crosshair, hit it now and
// remove it from your screen, then the click is done. Some(instant) when it
// took the click.
pub(super) fn crystal_attack(game: &mut Game<'_>) -> Option<bool> {
    let m = super::solstice::with(|s| s.module("Crystal Optimizer").filter(|m| m.on).cloned())?;
    if !m.b("Instant break") || !in_world(game) {
        return None;
    }
    let eye = game.eye_position()?;
    let dir = view_vector(game.yaw, game.pitch);
    let reach = m.n("Reach");
    let mut best: Option<(f64, u32)> = None;
    for c in &game.crystals {
        if c.id == 0 || c.invulnerable {
            continue;
        }
        let lo = horizon_core::geometry::Vec3::new(c.position.x - 1.0, c.position.y, c.position.z - 1.0);
        let hi = horizon_core::geometry::Vec3::new(c.position.x + 1.0, c.position.y + 2.0, c.position.z + 1.0);
        let b = horizon_core::geometry::Aabb::new(lo, hi);
        if let Some(d) = super::ray_box(eye, dir, reach, b)
            && best.is_none_or(|(bd, _)| d < bd)
        {
            let at = [eye[0] + dir[0] * d, eye[1] + dir[1] * d, eye[2] + dir[2] * d];
            if clear_line(game, at, [i32::MIN; 3]) {
                best = Some((d, c.id));
            }
        }
    }
    let (_, id) = best?;
    game.send(S::Attack(play::Attack { id: id as i32 }));
    if m.b("Swing") {
        game.swing_arm();
        game.send(S::Punch(horizon_protocol::packets::Empty));
    }
    game.reset_attack_strength();
    game.crystals.retain(|c| c.id != id);
    Some(false)
}
