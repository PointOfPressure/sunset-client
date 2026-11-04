// Meteor's smaller combat and player modules, written from their behaviour: Anti Anvil, Self Anvil, Auto Anvil,
// Rotations, No Status Effects (render.rs keeps the effects aside), Mine Through Mobs
// (player.rs's crosshair pick reads it), Portals, Multitask, Fake Player (single player only), Quiver, Arrow Dodge
// (with vape-rs's AntiFireball as Hit fireballs), Liquid Interact and No Interact (with vape-rs's HitSelect in
// click_fix.rs).
use super::{Game, Screen};
use super::sunset::{Category, Module, with};
use super::sunset_util::{self as util, facts, feet_block, find_held, is_air, listed, place_block, short, toggle_off};
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event::MouseButton;

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        // Anti Anvil, Self Anvil and Auto Anvil (Meteor's three modules) as sections; Rotate is all three's.
        Module::new("Anvil", Combat, "Blocks anvils over you, drops them on players.")
            .bool("Anti Anvil", true)
            .bool("Self Anvil", false)
            .bool("Auto Anvil", false)
            .num("Target range", 4.0, 0.0, 6.0, 0.5)
            .num("Height", 2.0, 2.0, 5.0, 1.0)
            .num("Delay", 10.0, 0.0, 50.0, 1.0)
            .bool("Place at feet", true)
            .bool("Multi place", true)
            .bool("Rotate", true),
        Module::new("Rotations", Player, "Keeps your look when the server turns you, or locks it.")
            .bool("No rotate", true)
            .bool("Lock", false)
            .pick("Yaw lock", &["Simple", "Smart", "None"])
            .num("Yaw angle", 0.0, 0.0, 360.0, 1.0)
            .pick("Pitch lock", &["Simple", "Smart", "None"])
            .num("Pitch angle", 0.0, -90.0, 90.0, 1.0),
        Module::new("No Status Effects", Player, "Ignores the effects you list.")
            .text("Effects", "levitation,jump_boost,slow_falling,dolphins_grace"),
        Module::new("Mine Through Mobs", Player, "Mines through mobs in the way.")
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
            .bool("Ignore own", true)
            .bool("Dodge", true)
            .bool("Hit fireballs", false)
            .bool("Fireball silent aim", true),
        Module::new("Liquid Interact", Player, "Lets you place on water and lava."),
        Module::new("No Interact", Player, "Stops clicks on what you list.")
            .text("Mine blocks", "")
            .pick("Mine mode", &["Blacklist", "Whitelist"])
            .text("Use blocks", "")
            .pick("Use mode", &["Blacklist", "Whitelist"])
            .text("Hit entities", "")
            .pick("Hit mode", &["Blacklist", "Whitelist"])
            .text("Use entities", "")
            .pick("Use entity mode", &["Blacklist", "Whitelist"])
            .pick("Friends", &["None", "Hit", "Use", "Both"])
            .pick("Babies", &["None", "Hit", "Use", "Both"])
            .pick("Nametagged", &["None", "Hit", "Use", "Both"])
            .bool("Hit fix", false)
            .bool("Hit fix weapons only", true)
            .bool("Hit fix hurt time", true)
            .bool("Hit fix on ground", true)
            .pick("Hit select", &["Off", "Cancel", "Sprint reset"])
            .num("Hit select chance (%)", 90.0, 0.0, 100.0, 1.0)
            .pick("Hit select favours", &["KB reduction", "Critical hits"])
            .bool("No miss delay", false)
            .num("Humanize (%)", 0.0, 0.0, 100.0, 1.0)
            .bool("In combat only", false)
            .num("Combat range", 5.0, 3.0, 8.0, 0.5)
            .bool("Anti action", false)
            .bool("Double glowstone", true)
            .bool("Open ender chest", true)
            .bool("Place glowstone", true)
            .bool("Ground firework", true)
            .bool("Sword on shield", true)
            .bool("Ghost place", false)
            .bool("Cobweb only", true)
            .bool("Use offhand", false)
            .bool("Anti trap", false)
            .bool("Hide armor stands", true)
            .bool("Hide minecarts", true)
            .bool("Hide item frames", false)
            .bool("Hide boats", false)
            .bool("Hide end crystals", false)
            .num("Trap range", 128.0, 0.0, 256.0, 8.0)
            .bool("Drop spawns", true)
            .bool("Remove loaded", true)
            .bool("Anti sign", true),
        Module::new("Fake Player", Player, "A copy of you to test modules on.")
            .num("Health", 20.0, 1.0, 100.0, 1.0)
            .bool("Copy inventory", true),
    ]
}

#[derive(Default)]
struct Parity {
    // Rotations' No rotate: your look before the server's teleport turned it.
    look: Option<(f32, f32)>,
    // Portals: the screen a portal's effect would have closed.
    screen: Option<Screen>,
    // Multitask: left clicks while you used an item, since the last tick.
    clicks: u32,
    // Auto Anvil: ticks since its last anvils.
    anvil_timer: u32,
    // Self Anvil: on last tick, and its anvil not yet placed since it came on.
    self_anvil_was: bool,
    self_anvil_due: bool,
    // Fake Player: it is in the world.
    fake: bool,
    // Quiver: where its shot is, the slot you held before the bow, and the effects shot since it came on.
    quiver: Quiver,
    quiver_back: Option<usize>,
    quiver_shot: Vec<String>,
    // Arrow Dodge's Hit fireballs: the fireball to hit at the start of the next tick, and not before this tick again.
    fireball: Option<u32>,
    fireball_next: u64,
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

// Liquid Interact is on (read by the crosshair's pick, which runs inside the game's frames).
static LIQUID: AtomicBool = AtomicBool::new(false);

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

// Rotations' No rotate (Meteor's No Rotate): a teleport from the server is about to set your look; it is kept to put
// back. Lock then holds it to its angles as before the teleport.
pub(super) fn correction(game: &Game<'_>) {
    if module("Rotations").is_some_and(|m| m.b("No rotate")) {
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
    // Arrow Dodge's Hit fireballs: the hit, once the server has the look at it.
    if let Some(id) = parity(|p| p.fireball.take())
        && !super::sunset::strike_waiting()
        && game.creatures.projectiles.iter().any(|f| f.id == id && fireball_in_reach(game, f))
    {
        game.send(S::Attack(play::Attack { id: id as i32 }));
        game.swing_arm();
        game.send(S::Punch(Empty));
        game.reset_attack_strength();
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
    LIQUID.store(get("Liquid Interact").is_some(), Ordering::Relaxed);
    fake_player(game, get("Fake Player"));
    match get("Quiver") {
        Some(m) => quiver(game, m),
        None => quiver_done(game),
    }
    if let Some(m) = get("Arrow Dodge") {
        if m.b("Dodge") {
            arrow_dodge(game, m);
        }
        if m.b("Hit fireballs") {
            aim_fireball(game, m);
        }
    }
    if let Some(m) = get("Rotations").filter(|m| m.b("Lock")) {
        rotation(game, m);
    }
    // Anvil's Self Anvil drops one anvil each time it comes on (the module, or the section while the module is on).
    let anvil = get("Anvil");
    let self_on = anvil.is_some_and(|m| m.b("Self Anvil"));
    parity(|p| {
        p.self_anvil_due = self_on && (p.self_anvil_due || !p.self_anvil_was);
        p.self_anvil_was = self_on;
    });
    let Some(feet) = feet_block(game) else {
        return;
    };
    let above = |n: i32| [feet[0], feet[1] + n, feet[2]];
    if let Some(m) = anvil {
        if m.b("Anti Anvil") {
            anti_anvil(game, m, feet);
        }
        // Placed: with no other section on, Anvil goes off, as Self Anvil alone did.
        if parity(|p| p.self_anvil_due)
            && let Some(slot) = find_held(game, |n| matches!(n, "anvil" | "chipped_anvil" | "damaged_anvil"))
            && place_block(game, above(2), slot, m.b("Rotate"), true)
        {
            parity(|p| p.self_anvil_due = false);
            if !m.b("Anti Anvil") && !m.b("Auto Anvil") {
                toggle_off(game, "Anvil");
            }
        }
        if m.b("Auto Anvil") {
            auto_anvil(game, m);
        }
    }
}

// Fake Player (Meteor's): while on, a player where you stood when it came on, with Health and (Copy inventory) what
// you hold and wear; gone when it goes off. Single player only: a server never sent it, so a hit at it is an
// interaction with nothing, which anticheats flag. On a server it goes straight off with one line saying so.
fn fake_player(game: &mut Game<'_>, m: Option<&Module>) {
    if m.is_some() && game.integrated.is_none() {
        toggle_off(game, "Fake Player");
        super::sunset::say(game, "Fake Player works in single player only: servers flag hits on a player they never sent.");
        return;
    }
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
// A large fireball is within a hit of your eyes (3 blocks to its middle, and half its size).
fn fireball_in_reach(game: &Game<'_>, f: &horizon_client::projectiles::Projectile) -> bool {
    game.eye_position().is_some_and(|eye| util::dist(eye, [f.position.x, f.position.y + 0.5, f.position.z]) <= 3.5)
}

// Hit fireballs (vape-rs's AntiFireball): a ghast's large fireball coming at you, whose line passes within 4 blocks
// of your eyes, is looked at (the server's look, or yours too without Fireball silent aim) and hit back once in
// reach, 4 ticks apart at most.
fn aim_fireball(game: &mut Game<'_>, m: &Module) {
    use horizon_client::projectiles::ProjectileKind;
    let Some(eye) = game.eye_position() else {
        return;
    };
    if game.ticks < parity(|p| p.fireball_next) || game.screen != Screen::Playing {
        return;
    }
    let passes = |f: &&horizon_client::projectiles::Projectile| {
        let (pos, vel) = ([f.position.x, f.position.y + 0.5, f.position.z], [f.velocity.x, f.velocity.y, f.velocity.z]);
        let to = [eye[0] - pos[0], eye[1] - pos[1], eye[2] - pos[2]];
        let speed = vel.iter().map(|v| v * v).sum::<f64>();
        let along = (0..3).map(|i| to[i] * vel[i]).sum::<f64>();
        if speed < 1e-6 || along <= 0.0 {
            return false;
        }
        let t = along / speed;
        let near = (0..3).map(|i| (pos[i] + vel[i] * t - eye[i]).powi(2)).sum::<f64>().sqrt();
        near <= 4.0 && fireball_in_reach(game, f)
    };
    let Some(f) = game
        .creatures
        .projectiles
        .iter()
        .filter(|f| matches!(f.kind, ProjectileKind::LargeFireball { .. }) && f.id != 0)
        .filter(passes)
        .min_by(|a, b| {
            let d = |f: &horizon_client::projectiles::Projectile| util::dist(eye, [f.position.x, f.position.y, f.position.z]);
            d(a).total_cmp(&d(b))
        })
    else {
        return;
    };
    let (id, (yaw, pitch)) = (f.id, super::sunset::look_at(eye, [f.position.x, f.position.y + 0.5, f.position.z]));
    if !m.b("Fireball silent aim") {
        game.yaw = yaw;
        game.pitch = pitch;
    }
    parity(|p| p.fireball_next = game.ticks + 4);
    super::sunset::turn_then("Arrow Dodge", yaw, pitch, move |_| parity(|p| p.fireball = Some(id)));
}

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

// No Interact (Meteor's): whether your click may go to what the crosshair is on: a mob (Hit or Use entities, and
// friends, babies and named mobs when their switch covers the click) before the block behind it (Mine or Use blocks),
// each list a blacklist or a whitelist.
fn allowed(m: &Module, list: &str, mode: &str, name: &str) -> bool {
    listed(m.t(list), name) == (m.p(mode) == "Whitelist")
}

fn interact_ok(game: &Game<'_>, hit: bool) -> bool {
    let Some(m) = module("No Interact") else {
        return true;
    };
    if let Some((i, distance)) = game.mob_hit()
        && game.before_block(distance)
    {
        let mob = &game.mobs[i];
        let covers = |switch: &str| matches!(m.p(switch), "Both") || m.p(switch) == if hit { "Hit" } else { "Use" };
        let friend = mob.kind.id == "minecraft:player"
            && super::sunset::is_friend(&with(|s| s.friends.clone()), &game.remote_player_names(), mob.id);
        if (covers("Friends") && friend) || (covers("Babies") && mob.is_baby()) || (covers("Nametagged") && mob.custom_name.is_some()) {
            return false;
        }
        let (list, mode) = if hit { ("Hit entities", "Hit mode") } else { ("Use entities", "Use entity mode") };
        return allowed(&m, list, mode, &mob.kind.id);
    }
    let Some(block) = game.pick() else {
        return true;
    };
    let (list, mode) = if hit { ("Mine blocks", "Mine mode") } else { ("Use blocks", "Use mode") };
    allowed(&m, list, mode, &facts(game, block.pos).name)
}

// The attack hooks: a hit or a mining start No Interact stops.
pub(super) fn hit_blocked(game: &Game<'_>) -> bool {
    !interact_ok(game, true)
}

// The use hook: a right click No Interact stops.
pub(super) fn use_blocked(game: &Game<'_>) -> bool {
    !interact_ok(game, false)
}

// Liquid Interact (Meteor's): the crosshair's pick (the pick hook, in its place: None leaves it to the game), as
// the game makes it, or when that meets no block, the first water or lava along the look within reach as the block it
// hits (its face the one the look comes in by).
pub(super) fn liquid_pick(game: &Game<'_>) -> Option<Option<super::Hit>> {
    if !LIQUID.load(Ordering::Relaxed) || game.forced_hit.is_some() || game.tick_hit.is_some() {
        return None;
    }
    let Some((eye, dir)) = game.aim() else {
        return Some(None);
    };
    let reach = game.reach();
    Some(game.clip_ray(eye, dir, reach).or_else(|| liquid_ray(game, eye, dir, reach)))
}

fn liquid_ray(game: &Game<'_>, eye: [f64; 3], dir: [f64; 3], reach: f64) -> Option<super::Hit> {
    let mut last = eye.map(|v| v.floor() as i32);
    let mut t = 0.0;
    while t <= reach {
        let point = [eye[0] + dir[0] * t, eye[1] + dir[1] * t, eye[2] + dir[2] * t];
        let pos = point.map(|v| v.floor() as i32);
        if pos != last {
            let f = facts(game, pos);
            if f.fluid_level > 0 || matches!(short(&f.name), "water" | "lava") {
                // The way in: one axis, the first that changed.
                let axis = (0..3).find(|&a| last[a] != pos[a]).unwrap_or(1);
                let mut face = [0; 3];
                face[axis] = (last[axis] - pos[axis]).signum();
                return Some(super::Hit { pos, face, point });
            }
            last = pos;
        }
        t += 0.02;
    }
    None
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

// Auto Anvil (Meteor's): the player in Target range with the least health (not you, not a friend) gets a button or
// pressure plate at their feet (Place at feet: the anvil lands on it and breaks, and the next one falls too) and, every
// Delay ticks, anvils from Height down to 2 over their head (only the highest with Multi place off).
fn auto_anvil(game: &mut Game<'_>, m: &Module) {
    let Some(eye) = game.eye_position() else {
        return;
    };
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let (friends, names) = (with(|s| s.friends.clone()), game.remote_player_names());
    let target = game
        .mobs
        .iter()
        .filter(|mob| mob.kind.id == "minecraft:player" && mob.alive() && !mob.removed && Some(mob.id) != own && !fake(mob.id))
        .filter(|mob| !super::sunset::is_friend(&friends, &names, mob.id))
        .filter(|mob| util::dist(eye, [mob.position.x, mob.position.y, mob.position.z]) <= m.n("Target range"))
        .min_by(|a, b| a.health.total_cmp(&b.health))
        .map(|mob| [mob.position.x.floor() as i32, mob.position.y.floor() as i32, mob.position.z.floor() as i32]);
    let Some(feet) = target else {
        return;
    };
    if m.b("Place at feet")
        && let Some(slot) = find_held(game, |n| n.ends_with("_button") || n.ends_with("_pressure_plate"))
    {
        place_block(game, feet, slot, m.b("Rotate"), false);
    }
    let due = parity(|p| {
        p.anvil_timer += 1;
        let due = p.anvil_timer > m.n("Delay") as u32;
        if due {
            p.anvil_timer = 0;
        }
        due
    });
    let Some(slot) = find_held(game, |n| matches!(n, "anvil" | "chipped_anvil" | "damaged_anvil")).filter(|_| due) else {
        return;
    };
    for i in (2..=m.n("Height") as i32).rev() {
        let at = [feet[0], feet[1] + 1 + i, feet[2]];
        if place_block(game, at, slot, m.b("Rotate"), true) && !m.b("Multi place") {
            return;
        }
    }
}

// Standing in a hole: the four sides and the floor of your block are blocks that do not burn or break by a blast.
pub(super) fn in_hole(game: &Game<'_>, feet: [i32; 3]) -> bool {
    let hard = |p: [i32; 3]| matches!(short(&facts(game, p).name), "obsidian" | "bedrock" | "crying_obsidian" | "netherite_block" | "respawn_anchor" | "ender_chest");
    [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, -1, 0]].iter().all(|d| hard([feet[0] + d[0], feet[1] + d[1], feet[2] + d[2]]))
}

