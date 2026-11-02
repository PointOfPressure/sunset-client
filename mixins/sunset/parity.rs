// Meteor's smaller combat and player modules, written from their behaviour: Anti Anvil, Self Anvil,
// Anti Bed, Rotation, No Rotate, No Status Effects (render.rs keeps the effects aside), Potion Saver, No Mining
// Trace (player.rs's crosshair pick reads it), Portals and Multitask.
use super::{Game, Screen};
use super::sunset::{Category, Module, with};
use super::sunset_util::{self as util, facts, feet_block, find_held, is_air, listed, place_block, short, toggle_off};
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::HashMap;
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        Module::new("Anti Anvil", Combat, "Blocks falling anvils above you.")
            .bool("Rotate", true),
        Module::new("Self Anvil", Combat, "Drops an anvil into your hole.")
            .bool("Rotate", true),
        Module::new("Anti Bed", Combat, "String in your hole against beds.")
            .bool("String top", false)
            .bool("String middle", true)
            .bool("String bottom", false)
            .bool("Only in hole", true),
        Module::new("Rotation", Player, "Locks your yaw and pitch.")
            .pick("Yaw lock", &["Simple", "Smart", "None"])
            .num("Yaw angle", 0.0, 0.0, 360.0, 1.0)
            .pick("Pitch lock", &["Simple", "Smart", "None"])
            .num("Pitch angle", 0.0, -90.0, 90.0, 1.0),
        Module::new("No Rotate", Player, "Keeps your look when the server turns you."),
        Module::new("No Status Effects", Player, "Ignores the effects you list.")
            .text("Effects", "levitation,jump_boost,slow_falling,dolphins_grace"),
        Module::new("Potion Saver", Player, "Your effects' timers pause here.")
            .text("Effects", "strength,absorption,resistance,fire_resistance,speed,haste,regeneration,water_breathing,saturation,luck,slow_falling,dolphins_grace,conduit_power,hero_of_the_village")
            .bool("Only when still", false),
        Module::new("No Mining Trace", Player, "Mines through mobs in the way.")
            .text("Entities", "")
            .bool("Only with a pickaxe", true),
        Module::new("Portals", Player, "Keeps menus open in nether portals."),
        Module::new("Multitask", Player, "Hits while you eat or use items."),
    ]
}

#[derive(Default)]
struct Parity {
    // No Rotate: your look before the server's teleport turned it.
    look: Option<(f32, f32)>,
    // Anti Bed: a bed over your head was broken last tick (the stop goes this tick).
    breaking: Option<[i32; 3]>,
    // Potion Saver: each effect's time last tick.
    timers: HashMap<String, i32>,
    // Portals: the screen a portal's effect would have closed.
    screen: Option<Screen>,
    // Multitask: left clicks while you used an item, since the last tick.
    clicks: u32,
}

static PARITY: Mutex<Option<Parity>> = Mutex::new(None);

fn parity<R>(f: impl FnOnce(&mut Parity) -> R) -> R {
    f(PARITY.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Parity::default))
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

// No Rotate (Meteor's): a teleport from the server is about to set your look; it is kept to put back.
pub(super) fn correction(game: &Game<'_>) {
    if module("No Rotate").is_some() {
        parity(|p| p.look = Some((game.yaw, game.pitch)));
    }
}

// Before the tick's packets: the look a teleport took comes back (a hair off, so the tick's move packet carries it).
// Multitask (Meteor's): your clicks at a mob while you use an item (the game takes none then) become hits, through
// the strikes queue, in this tick before its move packet.
pub(super) fn pre_tick(game: &mut Game<'_>) {
    if let Some((yaw, pitch)) = parity(|p| p.look.take()) {
        game.yaw = yaw + 0.000_001;
        game.pitch = pitch;
    }
    if parity(|p| std::mem::take(&mut p.clicks)) > 0
        && game.using.is_some()
        && game.screen == Screen::Playing
        && let Some((i, distance)) = game.mob_hit()
        && game.before_block(distance)
    {
        let id = game.mobs[i].id;
        super::sunset::strike(game, id);
    }
}

// The mouse hook: a left press while an item is in use, with Multitask on.
pub(super) fn clicked(game: &Game<'_>, left: bool) {
    if left && game.using.is_some() && module("Multitask").is_some() {
        parity(|p| p.clicks += 1);
    }
}

// Portals (Meteor's): in a nether portal the game closes every screen but the chat; with Portals on it sees the chat
// while its portal effect runs, and the screen comes back after.
pub(super) fn portal_before(game: &mut Game<'_>) {
    if !matches!(game.screen, Screen::Playing | Screen::Chat | Screen::Dead) && module("Portals").is_some() {
        let open = std::mem::replace(&mut game.screen, Screen::Chat);
        parity(|p| p.screen = Some(open));
    }
}

pub(super) fn portal_after(game: &mut Game<'_>) {
    if let Some(open) = parity(|p| p.screen.take()) {
        game.screen = open;
    }
}

// The effects No Status Effects keeps from you (render.rs takes them off the player while it is on).
pub(super) fn ignored_effect(id: &str) -> bool {
    with(|s| s.module("No Status Effects").filter(|m| m.on).is_some_and(|m| listed(m.t("Effects"), id)))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    if !game.native_playing() || game.remote.is_none() || game.player.is_none() {
        return;
    }
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    if let Some(m) = get("Rotation") {
        rotation(game, m);
    }
    if let Some(m) = get("Potion Saver") {
        potion_saver(game, m);
    } else {
        parity(|p| p.timers.clear());
    }
    let Some(feet) = feet_block(game) else {
        return;
    };
    let above = |n: i32| [feet[0], feet[1] + n, feet[2]];
    if let Some(m) = get("Anti Anvil") {
        anti_anvil(game, m, feet);
    }
    if let Some(m) = get("Self Anvil")
        && let Some(slot) = find_held(game, |n| matches!(n, "anvil" | "chipped_anvil" | "damaged_anvil"))
        && place_block(game, above(2), slot, m.b("Rotate"), true)
    {
        toggle_off(game, "Self Anvil");
    }
    if let Some(m) = get("Anti Bed") {
        anti_bed(game, m, feet);
    } else {
        parity(|p| p.breaking = None);
    }
}

// Rotation (Meteor's): the yaw and pitch set to their angles (Simple) or to the nearest 45 and 30 degrees (Smart).
fn rotation(game: &mut Game<'_>, m: &Module) {
    match m.p("Yaw lock") {
        "Simple" => game.yaw = m.n("Yaw angle") as f32,
        "Smart" => game.yaw = ((game.yaw + 1.0) / 45.0).round() * 45.0,
        _ => {}
    }
    match m.p("Pitch lock") {
        "Simple" => game.pitch = m.n("Pitch angle") as f32,
        "Smart" => game.pitch = ((game.pitch + 1.0) / 30.0).round() * 30.0,
        _ => {}
    }
}

// Anti Anvil (Meteor's): an anvil above you within reach with air under it gets obsidian under it; one already
// falling onto your block, obsidian over your head.
fn anti_anvil(game: &mut Game<'_>, m: &Module, feet: [i32; 3]) {
    let Some(slot) = find_held(game, |n| n == "obsidian") else {
        return;
    };
    for i in 0..=5 {
        let at = [feet[0], feet[1] + 3 + i, feet[2]];
        let below = [at[0], at[1] - 1, at[2]];
        if short(&facts(game, at).name).ends_with("anvil") && is_air(game, below) && place_block(game, below, slot, m.b("Rotate"), true) {
            return;
        }
    }
    let head = [feet[0], feet[1] + 2, feet[2]];
    let falling = game.falling.iter().any(|f| {
        [f.position.x.floor() as i32, f.position.z.floor() as i32] == [feet[0], feet[2]]
            && f.position.y > f64::from(head[1])
            && short(&game.res.facts().get(f.state).name).ends_with("anvil")
    });
    if falling && is_air(game, head) {
        place_block(game, head, slot, m.b("Rotate"), true);
    }
}

// Anti Bed (Meteor's): in a hole (or anywhere with Only in hole off), a bed over your head is broken (start and stop
// of a dig; the next tick the dig is called off) and string goes where its switches say: your feet, your head's
// block, the block above.
fn anti_bed(game: &mut Game<'_>, m: &Module, feet: [i32; 3]) {
    if m.b("Only in hole") && !in_hole(game, feet) {
        return;
    }
    let head = [feet[0], feet[1] + 1, feet[2]];
    if let Some(pos) = parity(|p| p.breaking.take()) {
        game.send(S::PlayerAction(play::PlayerAction { action: 2, pos, direction: 1, sequence: 0 }));
        game.send(S::Punch(Empty));
    } else if short(&facts(game, head).name).ends_with("_bed") {
        for action in [0, 3] {
            let sequence = util::sequence(game);
            game.send(S::PlayerAction(play::PlayerAction { action, pos: head, direction: 1, sequence }));
        }
        parity(|p| p.breaking = Some(head));
    }
    let Some(slot) = find_held(game, |n| n == "string") else {
        return;
    };
    for (switch, y) in [("String top", 2), ("String middle", 1), ("String bottom", 0)] {
        let at = [feet[0], feet[1] + y, feet[2]];
        if m.b(switch) && short(&facts(game, at).name) != "tripwire" && util::replaceable(game, at) {
            place_block(game, at, slot, false, false);
        }
    }
}

// Standing in a hole: the four sides and the floor of your block are blocks that do not burn or break by a blast.
fn in_hole(game: &Game<'_>, feet: [i32; 3]) -> bool {
    let hard = |p: [i32; 3]| matches!(short(&facts(game, p).name), "obsidian" | "bedrock" | "crying_obsidian" | "netherite_block" | "respawn_anchor" | "ender_chest");
    [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, -1, 0]].iter().all(|d| hard([feet[0] + d[0], feet[1] + d[1], feet[2] + d[2]]))
}

// Potion Saver (Meteor's): the listed effects' timers here do not run down (with Only when still, while you stand
// still); what the server sends still counts.
fn potion_saver(game: &mut Game<'_>, m: &Module) {
    let moving = game.player.as_ref().is_some_and(|p| {
        (p.position.x - game.previous_position.x).abs() > 1e-4 || (p.position.z - game.previous_position.z).abs() > 1e-4
    });
    let hold = !m.b("Only when still") || !moving;
    let list = m.t("Effects");
    parity(|p| {
        for e in game.potions.effects.list.iter_mut().filter(|e| listed(list, e.id)) {
            if hold && p.timers.get(e.id).is_some_and(|&was| e.duration == was - 1) {
                e.duration += 1;
            }
            p.timers.insert(e.id.to_owned(), e.duration);
        }
    });
}
