// Sunset's combat modules beyond the core ones, ported from Meteor's source:
// Surround, Self Trap, Auto Trap, Self Web, Auto Web, Hole Filler, Burrow,
// Offhand and Auto EXP; and Auto City (Meteor's, as Rust's PR #16 has it).

use super::sunset::{Category, Module, gui_open, hold, is_friend, own_id, with};
use super::sunset_util::*;
use super::Game;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

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
        Module::new("Auto City", Combat, "Mines a block of the wall round the nearest player's feet with your best pickaxe, then turns off. Support puts obsidian under it first, so a crystal can go there.")
            .num("Target range", 5.5, 0.0, 6.0, 0.5)
            .num("Break range", 4.5, 0.0, 6.0, 0.5)
            .bool("Support", true)
            .bool("Rotate", true)
            .bool("Notifications", true),
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
        Module::new("Aim Assist", Combat, "Adds to your own aim while you hold a weapon: never moves you on its own, never pulls against you, never attacks. Falling onto someone (mace dive) it pulls you onto them. Duelkit's.")
            .bool("Players", true)
            .bool("Hostiles", true)
            .num("Pull", 0.4, 0.0, 2.0, 0.05)
            .num("Stickiness", 0.25, 0.0, 0.9, 0.05)
            .num("Tracking", 0.5, 0.0, 1.0, 0.05)
            .num("Cone (degrees)", 30.0, 10.0, 120.0, 5.0)
            .num("Range", 6.0, 2.0, 8.0, 0.5)
            .pick("Range shape", &["Sphere", "Column"])
            .num("Column height", 0.0, 0.0, 64.0, 1.0)
            .bool("Mace dive", true)
            .num("Dive magnet", 6.0, 0.0, 30.0, 0.5)
            .num("Dive from fall of", 3.0, 1.0, 20.0, 0.5)
            .num("Dive cone (degrees)", 90.0, 10.0, 180.0, 5.0)
            .num("Dive range", 12.0, 3.0, 32.0, 1.0)
            .num("Dive tracking", 1.0, 0.0, 1.0, 0.05)
            .num("Aim inset", 0.15, 0.0, 0.3, 0.01)
            .num("Track within (degrees)", 3.0, 0.0, 15.0, 0.5)
            .num("Keep target", 1.5, 1.0, 3.0, 0.1)
            .bool("Only with a weapon", true)
            .bool("Anti bot", false)
            .bool("Ignore teams", false),
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
    // Offhand: ticks since its last move, and whether its warning was said.
    offhand_ticks: u32,
    offhand_warned: bool,
    exp_repairing: Option<usize>,
    aim_before: [f32; 2],
    aim_frame: Option<Instant>,
    aim_target: Option<u32>,
    aim_last: [f64; 2],
    city: Option<City>,
}

// Auto City's block: its target, the slot to go back to, the progress so far, and once the stop went the ticks
// left to wait for the server's answer.
#[derive(Clone, Copy)]
struct City {
    pos: [i32; 3],
    target: u32,
    back: usize,
    done: f32,
    waiting: Option<u32>,
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
    for name in ["Surround", "Self Trap", "Auto City", "Auto Trap", "Self Web", "Auto Web", "Hole Filler", "Burrow", "Auto EXP"] {
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
            "Auto City" => auto_city(game, m, just),
            "Auto Trap" => auto_trap(game, m, just),
            "Self Web" => self_web(game, m),
            "Auto Web" => auto_web(game, m),
            "Hole Filler" => hole_filler(game, m, just),
            "Burrow" => burrow(game, m, just),
            _ => auto_exp(game, m),
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

// ---- Auto City

fn auto_city(game: &mut Game<'_>, m: &Module, just: bool) {
    let note = |game: &mut Game<'_>, msg: &str| {
        if m.b("Notifications") {
            super::sunset_util::report(game, msg);
        }
    };
    if just {
        let found = city_block(game, m);
        let Some((t, pos)) = found else {
            note(game, "Auto City: no player next to a block to mine.");
            cs(|c| c.city = None);
            return toggle_off(game, "Auto City");
        };
        let Some(pick) = find_held(game, |n| n.ends_with("_pickaxe")) else {
            note(game, "Auto City: no pickaxe in the hotbar.");
            return toggle_off(game, "Auto City");
        };
        // Support: obsidian under the block, for the crystal that goes where it was.
        let below = [pos[0], pos[1] - 1, pos[2]];
        if m.b("Support")
            && let Some(slot) = blocks_slot(game, "obsidian")
            && let Some(eye) = game.eye_position()
            && dist(eye, center_of(below)) <= m.n("Break range")
        {
            place_block(game, below, slot, m.b("Rotate"), true);
        }
        let back = game.selected;
        hold(game, pick);
        cs(|c| c.city = Some(City { pos, target: t, back, done: 0.0, waiting: None }));
        start_city(game, pos, m.b("Rotate"), 0);
        return;
    }
    let Some(mut city) = cs(|c| c.city) else {
        return;
    };
    show(city.pos, 0x40FF_2020);
    let gone = game.res.facts().is_air(game.world.block(city.pos[0], city.pos[1], city.pos[2]));
    let target = target_by_id(game, city.target);
    let eye = game.eye_position();
    let (msg, over) = if gone {
        (format!("Auto City: mined {} {} {}.", city.pos[0], city.pos[1], city.pos[2]), true)
    } else if bad_target(game, &target, m.n("Target range")) || eye.is_none_or(|e| dist(e, center_of(city.pos)) > m.n("Break range")) {
        ("Auto City: the target or the block went out of range.".to_owned(), true)
    } else if city.waiting == Some(0) {
        ("Auto City: the server kept the block.".to_owned(), true)
    } else {
        (String::new(), false)
    };
    if over {
        note(game, &msg);
        hold(game, city.back);
        cs(|c| c.city = None);
        return toggle_off(game, "Auto City");
    }
    if let Some(left) = city.waiting {
        city.waiting = Some(left - 1);
    } else {
        // The game's own progress a tick with what is held (the pickaxe, its enchantments, haste, off the ground...).
        city.done += game.destroy_progress(city.pos);
        // The server lets a stop through once the progress reaches 0.7, counting the tick it started in.
        if city.done >= 0.7 {
            start_city(game, city.pos, m.b("Rotate"), 3);
            city.waiting = Some(10);
        }
    }
    cs(|c| c.city = Some(city));
}

// The nearest player in Target range, and the block of the wall round their feet that is closest to your eyes in
// Break range (not air, not unbreakable).
fn city_block(game: &Game<'_>, m: &Module) -> Option<(u32, [i32; 3])> {
    let eye = game.eye_position()?;
    let t = player_target(game, m.n("Target range"), "Lowest distance")?;
    let feet = [t.pos[0].floor() as i32, t.pos[1].floor() as i32, t.pos[2].floor() as i32];
    HORIZONTAL
        .iter()
        .map(|d| [feet[0] + d[0], feet[1], feet[2] + d[2]])
        .filter(|&p| {
            let f = facts(game, p);
            !game.res.facts().is_air(game.world.block(p[0], p[1], p[2])) && f.hardness >= 0.0 && dist(eye, center_of(p)) <= m.n("Break range")
        })
        .min_by(|a, b| dist(eye, center_of(*a)).total_cmp(&dist(eye, center_of(*b))))
        .map(|p| (t.id, p))
}

// START (0) or STOP (3) breaking the block, facing it first when Rotate is on.
fn start_city(game: &mut Game<'_>, pos: [i32; 3], rotate: bool, action: i32) {
    let send = move |game: &mut Game<'_>| {
        let sequence = sequence(game);
        game.send(S::PlayerAction(play::PlayerAction { action, pos, direction: 1, sequence }));
        game.swing_arm();
    };
    match game.eye_position() {
        Some(eye) if rotate => {
            let (yaw, pitch) = super::sunset::look_at(eye, center_of(pos));
            super::sunset::turn_then("Auto City", yaw, pitch, send);
        }
        _ => send(game),
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
    let target_id = cs(|c| c.auto_trap_target);
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

// ---- Offhand (its module is in combat_inventory.rs, with the careful totem moves)

// What the offhand should hold this tick.
pub(super) enum Want {
    Nothing,
    Totem,
    Item(&'static str),
}

// Meteor's Offhand and AutoTotem: a totem while your health (less what a blast or the fall could take) is at Health
// or you glide, else the chosen Item, a golden apple or potion while you hold right click with Right gapple or Sword
// pot (or always with a sword or axe in hand).
pub(super) fn offhand_want(game: &Game<'_>, m: &Module) -> Want {
    use winit::event::MouseButton;
    let Some(p) = game.player.as_ref() else {
        return Want::Nothing;
    };
    let is_totem = |s: &Option<horizon_client::items::ItemStack>| s.as_ref().is_some_and(|s| short(&s.item) == "totem_of_undying");
    let locked = (0..36).any(|i| game.inventory.get(i).is_some_and(is_totem)) && {
        let at = [p.position.x, p.position.y, p.position.z];
        let mut loss = 0.0;
        if m.b("Explosion") {
            loss = f64::max(loss, crystal_damage(game, at));
        }
        if m.b("Fall") {
            loss = f64::max(loss, fall_damage(game));
        }
        f64::from(p.health.health() + p.health.absorption()) - loss <= m.n("Health") || (m.b("Elytra") && p.fall_flying)
    };
    if locked {
        return Want::Totem;
    }
    let main = game.inventory.get(game.selected).and_then(|s| s.as_ref()).map(|s| short(&s.item).to_owned()).unwrap_or_default();
    let (sword, axe) = (main.ends_with("_sword"), main.ends_with("_axe"));
    // A right click that is not using what the main hand holds.
    let usable = matches!(main.as_str(), "bow" | "trident" | "crossbow") || game.res.items().get(&format!("minecraft:{main}")).is_some_and(|i| i.food.is_some());
    let clicking = (game.physical_buttons.contains(&MouseButton::Right) || game.right) && game.native_playing() && !usable;
    let item = if m.b("Right gapple") {
        if clicking && (!m.b("Sword gapple") || sword) { "Enchanted gapple" } else { m.p("Item") }
    } else if (sword || axe) && m.b("Always gap on sword") {
        "Enchanted gapple"
    } else if m.b("Sword pot") {
        if sword && clicking { "Potion" } else { m.p("Item") }
    } else if (sword || axe) && m.b("Always pot on sword") {
        "Potion"
    } else {
        m.p("Item")
    };
    match item {
        "Totem" => Want::Totem,
        "Gapple" => Want::Item("golden_apple"),
        "Enchanted gapple" => Want::Item("enchanted_golden_apple"),
        "Shield" => Want::Item("shield"),
        "Potion" => Want::Item("potion"),
        "Crystal" => Want::Item("end_crystal"),
        _ => Want::Nothing,
    }
}

// The quick way: pick the item up, put it in the offhand slot, put back what was there; at most every Delay ticks,
// from the main inventory (and the hotbar with Hotbar; a totem from anywhere).
pub(super) fn offhand_click(game: &mut Game<'_>, m: &Module, want: &str) {
    use horizon_client::menu::Input::Pickup;
    if !inventory_idle(game) {
        return;
    }
    let has = |s: &Option<horizon_client::items::ItemStack>| s.as_ref().is_some_and(|s| short(&s.item) == want);
    let ready = cs(|c| {
        c.offhand_ticks += 1;
        c.offhand_ticks
    }) > m.n("Delay (ticks)") as u32;
    if has(&game.offhand) || !ready {
        return;
    }
    let totem = want == "totem_of_undying";
    if totem && has(&game.inventory[game.selected]) {
        return;
    }
    let from = if m.b("Hotbar") || totem { 0 } else { 9 };
    match (from..36).find(|&i| game.inventory.get(i).is_some_and(has)) {
        None => {
            if !totem && !cs(|c| std::mem::replace(&mut c.offhand_warned, true)) {
                super::sunset_util::report(game, "Offhand: chosen item not found.");
            }
        }
        Some(i) => {
            super::sunset::menu_click(game, &[(i, Pickup), (504, Pickup), (i, Pickup)]);
            cs(|c| {
                c.offhand_warned = false;
                c.offhand_ticks = 0;
            });
        }
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
            super::sunset::menu_click(game, &[(src, horizon_client::menu::Input::Pickup), (to, horizon_client::menu::Input::Pickup), (src, horizon_client::menu::Input::Pickup)]);
            return;
        }
    };
    // Throw it at your feet.
    let yaw = game.yaw;
    super::sunset::turn_then("EXP Thrower", yaw, 90.0, move |game| {
        let before = game.selected;
        hold(game, slot);
        let seq = sequence(game);
        game.send(S::UseItem(play::UseItem { hand: 0, sequence: seq, yaw, pitch: 90.0 }));
        hold(game, before);
    });
}

// ---- Aim Assist (Duelkit's AimAssist, around your own mouse turn once a frame)
//
// Only ever adds to what your hand is doing. Pull: moving the mouse toward the target turns you a bit further toward
// it. Stickiness: while the crosshair is on the target your own turning is slowed. Tracking: while on (or almost on)
// the target, follows part of its movement across your screen. One target is kept until it leaves the cone, so it
// does not flick between people. Mace dive: falling onto someone the cone and range widen, tracking is full, and it
// also pulls on its own.

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

fn angle_between(a: [f64; 3], b: [f64; 3]) -> f64 {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
    ((a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / n).clamp(-1.0, 1.0).acos()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

// A box as its low and high corners.
type Box3 = [[f64; 3]; 2];

fn centre(b: &Box3) -> [f64; 3] {
    std::array::from_fn(|i| (b[0][i] + b[1][i]) / 2.0)
}

// The box where this frame draws a mob, shrunk by `inset` (AABB.deflate, which keeps low below high).
fn drawn_box(mob: &Mob, alpha: f64, inset: f64) -> Box3 {
    let b = mob.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    let (prev, now) = ([mob.previous.x, mob.previous.y, mob.previous.z], [mob.position.x, mob.position.y, mob.position.z]);
    let mut out = [[0.0; 3]; 2];
    for i in 0..3 {
        let off = (prev[i] - now[i]) * (1.0 - alpha);
        let (a, z) = (lo[i] + off + inset, hi[i] + off - inset);
        out[0][i] = a.min(z);
        out[1][i] = a.max(z);
    }
    out
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

// `Entity.isInvisibleTo`: invisible, unless on your team and the team sees friendly invisibles.
fn invisible_to_you(game: &Game<'_>, e: &Mob) -> bool {
    let own = game.score.board.players_team(&game.options.player_name());
    e.invisible && !(horizon_client::scoreboard::teams_allied(own, game.mob_team(e.id)) && own.is_some_and(|t| t.see_friendly_invisibles))
}

// `Weapons.isWeapon`: a sword, axe or spear (by their item tags), a mace or a trident in the main hand.
fn holds_weapon(game: &Game<'_>) -> bool {
    let tags = &game.res.items().item_tags;
    game.inventory.get(game.selected).and_then(|s| s.as_ref()).is_some_and(|s| {
        let item = &*s.item;
        item == "minecraft:mace"
            || item == "minecraft:trident"
            || ["minecraft:swords", "minecraft:axes", "minecraft:spears"].iter().any(|t| tags.get(*t).is_some_and(|t| t.contains(item)))
    })
}

// One frame's view: the eyes and the look at the frame's partial tick, and the settings in force.
struct Aim<'a, 'g> {
    game: &'a Game<'g>,
    m: &'a Module,
    eye: [f64; 3],
    look: [f64; 3],
    alpha: f64,
    range: f64,
    own: Option<u32>,
    names: Vec<(u32, String)>,
    friends: Vec<String>,
}

impl<'a> Aim<'a, '_> {
    fn boxed(&self, e: &Mob) -> Box3 {
        drawn_box(e, self.alpha, self.m.n("Aim inset"))
    }

    // `Targets.matches`: alive and visible to you, never yourself or an armour stand (Horizon keeps those apart);
    // players and hostile mobs as chosen. Friends, bots (Anti bot) and teammates (Ignore teams) are left alone.
    fn matches(&self, e: &Mob) -> bool {
        let player = e.kind.id == "minecraft:player";
        let wanted = if player { self.m.b("Players") } else { e.hostile() && self.m.b("Hostiles") };
        wanted
            && Some(e.id) != self.own
            && e.alive()
            && !e.removed
            && !invisible_to_you(self.game, e)
            && !(player && is_friend(&self.friends, &self.names, e.id))
            && !spared(self.game, self.m, e.id, player)
    }

    // A target, within `half_cone` degrees of the crosshair, in range and not behind a block.
    fn valid(&self, e: &Mob, half_cone: f64) -> bool {
        if !self.matches(e) {
            return false;
        }
        let aim = closest_point(&self.boxed(e), self.eye, self.look);
        let to = sub(aim, self.eye);
        let d = dist(self.eye, aim);
        if d < 1e-4 || angle_between(to, self.look).to_degrees() > half_cone {
            return false;
        }
        if self.m.p("Range shape") == "Column" {
            let height = self.m.n("Column height");
            if to[0].hypot(to[2]) > self.range || (height > 0.0 && to[1].abs() > height) {
                return false;
            }
        } else if d > self.range {
            return false;
        }
        clip_clear(self.game, self.eye, aim, d)
    }

    // The target nearest the crosshair.
    fn pick(&self, cone: f64) -> Option<&'a Mob> {
        let game: &'a Game<'_> = self.game;
        let angle = |e: &Mob| angle_between(sub(closest_point(&self.boxed(e), self.eye, self.look), self.eye), self.look);
        game.mobs.iter().filter(|e| self.valid(e, cone / 2.0)).min_by(|a, b| angle(a).total_cmp(&angle(b)))
    }
}

// In the world with no screen open, not a spectator, holding a weapon (with Only with a weapon), and the mouse
// turning you rather than a free camera.
fn aim_allowed(game: &Game<'_>, m: &Module) -> bool {
    in_world(game)
        && !gui_open()
        && !game.spectator()
        && !super::sunset_render::camera_takes_mouse()
        && (!m.b("Only with a weapon") || holds_weapon(game))
}

// Before your own mouse turn this frame.
pub(super) fn aim_before(game: &Game<'_>) {
    let rot = [game.yaw, game.pitch];
    cs(|c| c.aim_before = rot);
}

// After it: how far to turn you on top of it, in degrees.
fn aim_turn(game: &Game<'_>, m: &Module, dt: f64) -> [f64; 2] {
    let Some(p) = game.player.as_ref() else {
        return [0.0; 2];
    };
    let dive = m.b("Mace dive") && !p.ground.on_ground && !p.fall_flying && p.velocity.y < 0.0 && p.fall_distance >= m.n("Dive from fall of");
    let (cone, range, tracking) = if dive {
        (m.n("Dive cone (degrees)"), m.n("Dive range"), m.n("Dive tracking"))
    } else {
        (m.n("Cone (degrees)"), m.n("Range"), m.n("Tracking"))
    };
    let alpha = game.clock.camera_fraction();
    let lerp = |a: f64, b: f64| a + alpha * (b - a);
    let was = game.previous_position;
    let eye = [
        lerp(was.x, p.position.x),
        lerp(was.y, p.position.y) + f64::from(p.dimensions.eye_height),
        lerp(was.z, p.position.z),
    ];
    let look = view_vector(game.yaw, game.pitch);
    let aim = Aim { game, m, eye, look, alpha, range, own: own_id(game), names: game.remote_player_names(), friends: with(|s| s.friends.clone()) };
    let (previous, last, before) = cs(|c| (c.aim_target, c.aim_last, c.aim_before));
    let kept = previous
        .and_then(|id| game.mobs.iter().find(|e| e.id == id))
        .filter(|e| aim.valid(e, cone / 2.0 * m.n("Keep target")));
    let Some(target) = kept.or_else(|| aim.pick(cone)) else {
        cs(|c| c.aim_target = None);
        return [0.0; 2];
    };
    let b = aim.boxed(target);
    let middle = sub(centre(&b), eye);
    let (yaw_to, pitch_to) = (yaw_of(middle), pitch_of(middle));
    // The gap from the crosshair to the nearest point of the hitbox, zero when already on it.
    let to = sub(closest_point(&b, eye, look), eye);
    let (yaw, pitch) = (f64::from(game.yaw), f64::from(game.pitch));
    let gap_yaw = wrap(yaw_of(to) - yaw);
    let gap_pitch = pitch_of(to) - pitch;
    let gap = gap_yaw.hypot(gap_pitch);
    let on_target = gap < 0.05;
    let mut turn = [0.0; 2];
    // Tracking fades out as the crosshair drifts off the hitbox, instead of switching off at a hard edge.
    let within = m.n("Track within (degrees)");
    if previous == Some(target.id) && gap < within {
        let t = tracking * (1.0 - gap / within);
        turn[0] += wrap(yaw_to - last[0]) * t;
        turn[1] += (pitch_to - last[1]) * t;
    }
    if dive && !on_target && dt > 0.0 {
        let f = 1.0 - (-m.n("Dive magnet") * dt).exp();
        turn[0] += gap_yaw * f;
        turn[1] += gap_pitch * f;
    }
    let user = [wrap(yaw - f64::from(before[0])), pitch - f64::from(before[1])];
    let moved = user[0].hypot(user[1]);
    if moved > 1e-4 {
        // Stickiness ramps in over the last degree, so there is no hard edge to stutter on.
        let stick = m.n("Stickiness") * (1.0 - gap).clamp(0.0, 1.0);
        turn[0] -= user[0] * stick;
        turn[1] -= user[1] * stick;
        if !on_target && gap_yaw * user[0] + gap_pitch * user[1] > 0.0 {
            let extra = gap.min(m.n("Pull") * moved);
            turn[0] += gap_yaw / gap * extra;
            turn[1] += gap_pitch / gap * extra;
        }
    }
    let id = target.id;
    cs(|c| {
        c.aim_target = Some(id);
        c.aim_last = [yaw_to, pitch_to];
    });
    turn
}

// After your own mouse turn this frame.
pub(super) fn aim_after(game: &mut Game<'_>) {
    let now = Instant::now();
    let dt = cs(|c| c.aim_frame.replace(now)).map_or(0.0, |t| now.duration_since(t).as_secs_f64().min(0.05));
    let Some(m) = with(|s| s.module("Aim Assist").filter(|m| m.on).cloned()).filter(|m| aim_allowed(game, m)) else {
        cs(|c| c.aim_target = None);
        return;
    };
    let turn = aim_turn(game, &m, dt);
    // turn_player takes mouse units, 0.15 degrees each (Entity.turn), and keeps the interpolation in step.
    if turn != [0.0; 2] {
        game.turn_player(turn[0] / 0.15, turn[1] / 0.15);
    }
}

// ---- Crystal Optimizer

// Called when you attack: if an end crystal is on the crosshair, hit it now and
// remove it from your screen, then the click is done. Some(instant) when it
// took the click.
pub(super) fn crystal_attack(game: &mut Game<'_>) -> Option<bool> {
    let m = super::sunset::with(|s| s.module("Crystal Optimizer").filter(|m| m.on).cloned())?;
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
