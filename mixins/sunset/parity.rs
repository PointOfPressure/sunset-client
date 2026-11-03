// Meteor's smaller combat and player modules, written from their behaviour: Anti Anvil, Self Anvil,
// Anti Bed, Rotation, No Rotate, No Status Effects (render.rs keeps the effects aside), Potion Saver, No Mining
// Trace (player.rs's crosshair pick reads it), Portals, Multitask, Fake Player, Quiver and Arrow Dodge.
use super::{Game, Screen};
use super::sunset::{Category, Module, with};
use super::sunset_util::{self as util, facts, feet_block, find_held, is_air, listed, place_block, short, toggle_off};
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::HashMap;
use std::sync::Mutex;
use winit::event::MouseButton;

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
        Module::new("Quiver", Combat, "Shoots tipped arrows at yourself.")
            .text("Potions", "strength")
            .num("Cooldown", 10.0, 0.0, 40.0, 1.0)
            .bool("Check effects", true)
            .bool("Only in hole", true)
            .bool("Only on ground", true)
            .num("Min health", 10.0, 0.0, 36.0, 1.0),
        Module::new("Arrow Dodge", Combat, "Steps out of arrows' way.")
            .num("Move speed", 1.0, 0.05, 5.0, 0.05)
            .num("Distance check", 1.0, 0.1, 5.0, 0.1)
            .bool("Ground check", true)
            .bool("All projectiles", false)
            .bool("Ignore own", true),
        Module::new("Fake Player", Player, "A copy of you to test modules on.")
            .num("Health", 20.0, 1.0, 100.0, 1.0)
            .bool("Copy inventory", true),
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
    // Fake Player: it is in the world.
    fake: bool,
    // Quiver: where its shot is, the slot you held before the bow, and the effects shot since it came on.
    quiver: Quiver,
    quiver_back: Option<usize>,
    quiver_shot: Vec<String>,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum Quiver {
    #[default]
    Idle,
    // The bow drawn for so many ticks.
    Drawing(u32),
    // The look up goes out with this tick's move packet.
    Aim,
    // The server has the look up, and the next move packet keeps it: the bow is let go.
    Loose,
    // Ticks to wait before the next arrow.
    Rest(u32),
}

// Fake Player's entity id, far above any a server gives out.
const FAKE: u32 = 0x7FFF_FFF0;

// A hit on Fake Player stays here: the server knows no such entity.
pub(super) fn fake(id: u32) -> bool {
    id == FAKE
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
    // Quiver: the bow is let go while the server has the look up.
    if parity(|p| p.quiver) == Quiver::Loose {
        let _ = game.play_mouse(MouseButton::Right, false);
        let rest = with(|s| s.module("Quiver").map_or(0, |m| m.n("Cooldown") as u32));
        parity(|p| p.quiver = Quiver::Rest(rest));
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
    fake_player(game, get("Fake Player"));
    match get("Quiver") {
        Some(m) => quiver(game, m),
        None => quiver_done(game),
    }
    if let Some(m) = get("Arrow Dodge") {
        arrow_dodge(game, m);
    }
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

// Fake Player (Meteor's): while on, a player where you stood when it came on, with Health and (Copy inventory) what
// you hold and wear; gone when it goes off.
fn fake_player(game: &mut Game<'_>, m: Option<&Module>) {
    let here = game.mobs.iter().any(|mob| mob.id == FAKE);
    match m {
        Some(m) if !here => {
            let (Some(kind), Some(p)) = (game.res.player_type().clone(), game.player.as_ref()) else {
                return;
            };
            let mut mob = horizon_client::mobs::Mob::new(FAKE, kind, p.position, game.yaw);
            mob.health = m.n("Health") as f32;
            if m.b("Copy inventory") {
                let e = &mut mob.extra;
                e.set_main_hand_stack(game.held().cloned());
                e.off_hand = game.offhand.as_ref().map(|s| s.item.clone());
                e.armor = game.armor.clone();
                [e.head, e.chest, e.legs, e.feet] = std::array::from_fn(|i| game.armor[i].as_ref().map(|s| s.item.clone()));
            }
            game.mobs.push(mob);
            parity(|p| p.fake = true);
        }
        None if here && parity(|p| std::mem::take(&mut p.fake)) => game.mobs.retain(|mob| mob.id != FAKE),
        _ => {}
    }
}

// The effect a tipped arrow's potion gives ("long_swiftness": speed).
fn potion_effect(potion: &str) -> &str {
    match short(potion).trim_start_matches("long_").trim_start_matches("strong_") {
        "swiftness" => "speed",
        "leaping" => "jump_boost",
        "healing" => "instant_health",
        "harming" => "instant_damage",
        "slowness" => "slowness",
        other => other,
    }
}

// Quiver (Meteor's): in a hole (Only in hole), on the ground, above Min health and with room over your head, each
// effect a tipped arrow's potion gives (Potions lists it; with Check effects one you lack) is shot once straight up
// to fall on you: the arrow moved to where the bow takes its arrow from first, the bow drawn 4 ticks, the look turned
// up in two move packets and the bow let go between them. Off when none is left.
fn quiver(game: &mut Game<'_>, m: &Module) {
    let state = parity(|p| p.quiver);
    match state {
        Quiver::Rest(t) if t > 0 => return parity(|p| p.quiver = Quiver::Rest(t - 1)),
        Quiver::Loose => return,
        Quiver::Aim => {
            // Up for one more move packet: the let-go lands between two that look up.
            super::sunset::turn("Quiver", game.yaw, -90.0);
            return parity(|p| p.quiver = Quiver::Loose);
        }
        Quiver::Drawing(t) => {
            if game.using.is_none() {
                parity(|p| p.quiver = Quiver::Idle);
            } else if t + 1 >= 4 {
                super::sunset::turn("Quiver", game.yaw, -90.0);
                parity(|p| p.quiver = Quiver::Aim);
            } else {
                parity(|p| p.quiver = Quiver::Drawing(t + 1));
            }
            return;
        }
        _ => {}
    }
    let Some(feet) = feet_block(game) else {
        return;
    };
    let p = game.player.as_ref();
    let health = p.map_or(0.0, |p| p.health.health());
    let open = (1..=2).all(|y| !facts(game, [feet[0], feet[1] + y, feet[2]]).full_collision || is_air(game, [feet[0], feet[1] + y, feet[2]]));
    let arrow = |s: &horizon_client::items::ItemStack| s.item.ends_with("arrow");
    let shot = parity(|p| p.quiver_shot.clone());
    let wanted = |s: &horizon_client::items::ItemStack| {
        short(&s.item) == "tipped_arrow"
            && s.potion().is_some_and(|potion| {
                let effect = potion_effect(potion);
                listed(m.t("Potions"), short(potion).trim_start_matches("long_").trim_start_matches("strong_"))
                    && !shot.iter().any(|e| e == effect)
                    && !(m.b("Check effects") && game.potions.effects.get(effect).is_some())
            })
    };
    let bow = util::find_hotbar(game, |n| n == "bow");
    let next = (0..36).find(|&i| game.inventory[i].as_ref().is_some_and(wanted));
    let safe = (!m.b("Only in hole") || in_hole(game, feet))
        && (!m.b("Only on ground") || p.is_some_and(|p| p.ground.on_ground))
        && f64::from(health) >= m.n("Min health")
        && open
        && !game.offhand.as_ref().is_some_and(arrow);
    let (Some(bow), Some(next), true) = (bow, next, safe) else {
        toggle_off(game, "Quiver");
        return;
    };
    if !util::inventory_idle(game) {
        return;
    }
    let effect = game.inventory[next].as_ref().and_then(|s| s.potion()).map(|potion| potion_effect(potion).to_owned());
    // The bow takes the first arrow of the inventory: this one goes there.
    if let Some(first) = (0..36).find(|&i| game.inventory[i].as_ref().is_some_and(arrow))
        && first != next
    {
        use horizon_client::menu::Input::Pickup;
        super::sunset::menu_click(game, &[(next, Pickup), (first, Pickup), (next, Pickup)]);
    }
    parity(|p| {
        p.quiver_back = p.quiver_back.or(Some(game.selected));
        p.quiver_shot.extend(effect);
    });
    super::sunset::hold(game, bow);
    let _ = game.play_mouse(MouseButton::Right, true);
    parity(|p| p.quiver = Quiver::Drawing(0));
}

// Quiver off: the bow let go if it is drawn, and the slot from before back in hand.
fn quiver_done(game: &mut Game<'_>) {
    let (state, back) = parity(|p| {
        p.quiver_shot.clear();
        (std::mem::take(&mut p.quiver), p.quiver_back.take())
    });
    if matches!(state, Quiver::Drawing(_) | Quiver::Aim | Quiver::Loose) {
        let _ = game.play_mouse(MouseButton::Right, false);
    }
    if let Some(slot) = back {
        super::sunset::hold(game, slot);
    }
}

// Arrow Dodge (Meteor's, Velocity mode): each arrow in flight (any projectile with All projectiles; not yours with
// Ignore own) is followed for up to 200 ticks, falling as an arrow does, until it meets a block. When a point of a
// path comes within Distance check of your feet or head, you are pushed (your velocity, no packets of its own) the
// first of eight ways that keeps you clear, onto free blocks with ground under them (Ground check), Move speed
// blocks a tick, further if none is.
fn arrow_dodge(game: &mut Game<'_>, m: &Module) {
    use horizon_client_core::who::Who;
    use horizon_client::projectiles::ProjectileKind;
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let mut points: Vec<[f64; 3]> = Vec::new();
    for p in game.creatures.projectiles.iter().filter(|p| !p.stuck) {
        let arrow = matches!(p.kind, ProjectileKind::Arrow { .. });
        let mine = matches!(p.owner, Some(Who::Player)) || own.is_some_and(|id| p.owner == Some(Who::Mob(id)));
        if (!arrow && !m.b("All projectiles")) || (mine && m.b("Ignore own")) {
            continue;
        }
        let (mut at, mut v) = ([p.position.x, p.position.y, p.position.z], [p.velocity.x, p.velocity.y, p.velocity.z]);
        for _ in 0..200 {
            points.push(at);
            at = [at[0] + v[0], at[1] + v[1], at[2] + v[2]];
            v = [v[0] * 0.99, v[1] * 0.99 - 0.05, v[2] * 0.99];
            let block = at.map(|c| c.floor() as i32);
            if !is_air(game, block) && facts(game, block).full_collision {
                break;
            }
        }
    }
    let Some(me) = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]) else {
        return;
    };
    let reach = m.n("Distance check");
    let clear = |at: [f64; 3]| {
        points.iter().all(|q| {
            let d = |dy: f64| ((q[0] - at[0]).powi(2) + (q[1] - at[1] - dy).powi(2) + (q[2] - at[2]).powi(2)).sqrt();
            d(0.0) >= reach && d(1.0) >= reach
        })
    };
    if points.is_empty() || clear(me) {
        return;
    }
    let free = |b: [i32; 3]| is_air(game, b) || !facts(game, b).full_collision;
    const WAYS: [[f64; 2]; 8] = [[1.0, 1.0], [0.0, 1.0], [-1.0, 1.0], [1.0, 0.0], [-1.0, 0.0], [1.0, -1.0], [0.0, -1.0], [-1.0, -1.0]];
    let start = (game.ticks % 8) as usize;
    for step in 1..=10 {
        let speed = m.n("Move speed") * f64::from(step);
        for k in 0..8 {
            let [dx, dz] = WAYS[(start + k) % 8];
            let at = [me[0] + dx * speed, me[1], me[2] + dz * speed];
            let b = at.map(|c| c.floor() as i32);
            let ground = !free([b[0], b[1] - 1, b[2]]);
            if clear(at) && free(b) && free([b[0], b[1] + 1, b[2]]) && (ground || !m.b("Ground check")) {
                if let Some(p) = game.player.as_mut() {
                    p.velocity.x = dx * speed;
                    p.velocity.z = dz * speed;
                }
                return;
            }
        }
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
