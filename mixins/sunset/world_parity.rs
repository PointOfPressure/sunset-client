// Meteor's movement and world modules (batch F), written fresh from what they do: Enderman Look, Click TP, Auto
// Sign, Build Height and Trident Boost.

use super::sunset::{Category, Module, look_at, turn};
use super::sunset_util::*;
use super::Game;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

// Auto Sign and Build Height on, for what goes out (read where the module list may be locked).
static SIGN: AtomicBool = AtomicBool::new(false);
static HEIGHT: AtomicBool = AtomicBool::new(false);

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Enderman Look", Category::World, "Looks at or away from endermen.")
            .pick("Look mode", &["Away", "At"])
            .bool("Stun hostiles", true),
        Module::new("Click TP", Category::Movement, "Teleports you to the block you right-click.")
            .num("Max distance", 8.0, 1.0, 9.5, 0.5),
        Module::new("Auto Sign", Category::World, "Writes your first sign's text on every sign.")
            .num("Delay", 10.0, 0.0, 100.0, 1.0),
        Module::new("Build Height", Category::World, "Lets you place against the build limit."),
        Module::new("Trident Boost", Category::Movement, "Boosts riptide.")
            .num("Boost", 2.0, 0.1, 10.0, 0.1),
    ]
}

#[derive(Default)]
struct Wp {
    // Auto Sign: the first text written and its side, and the signs waiting to be written with it.
    sign_text: Option<[String; 4]>,
    signs: VecDeque<play::SignUpdate>,
    sign_timer: u32,
    // Click TP: use held last tick; Trident Boost: riptide's ticks last tick.
    using: bool,
    spin: i32,
}

static WP: Mutex<Option<Wp>> = Mutex::new(None);

fn wp<R>(f: impl FnOnce(&mut Wp) -> R) -> R {
    f(WP.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Wp::default))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    SIGN.store(get("Auto Sign").is_some(), Ordering::Relaxed);
    HEIGHT.store(get("Build Height").is_some(), Ordering::Relaxed);
    // Auto Sign closes the sign editor's screen, so it runs with that screen open.
    match get("Auto Sign").filter(|_| game.remote.is_some() && game.player.is_some()) {
        Some(m) => auto_sign(game, m),
        None => wp(|w| {
            w.sign_text = None;
            w.signs.clear();
        }),
    }
    if !in_world(game) {
        return;
    }
    if let Some(m) = get("Enderman Look") {
        enderman_look(game, m);
    }
    match get("Click TP") {
        Some(m) => click_tp(game, m),
        None => wp(|w| w.using = game.right),
    }
    let spin = game.player.as_ref().map_or(0, |p| p.auto_spin_attack_ticks);
    let started = wp(|w| std::mem::replace(&mut w.spin, spin) == 0 && spin > 0);
    if let Some(m) = get("Trident Boost").filter(|_| started) {
        // Riptide's push has just been given: more of it.
        if let Some(p) = game.player.as_mut() {
            p.velocity = p.velocity.scale(m.n("Boost"));
        }
    }
}

// What goes out: Auto Sign keeps the first text you write; Build Height turns a click on the top of a block at the build
// limit into one on its underside, which the server takes.
pub(super) fn outgoing(game: &Game<'_>, packet: &S) -> Option<S> {
    match packet {
        S::SignUpdate(p) if SIGN.load(Ordering::Relaxed) => {
            wp(|w| {
                if w.sign_text.is_none() && p.lines.iter().any(|l| !l.is_empty()) {
                    w.sign_text = Some(p.lines.clone());
                }
            });
            None
        }
        S::UseItemOn(p) if HEIGHT.load(Ordering::Relaxed) && p.face == 1 && p.pos[1] + 1 >= top(game) => {
            Some(S::UseItemOn(play::UseItemOn { face: 0, ..p.clone() }))
        }
        _ => None,
    }
}

// One above the highest block the dimension holds.
fn top(game: &Game<'_>) -> i32 {
    game.world.chunks.iter().next().map_or(320, |(_, c)| c.max_y())
}

// ---- Enderman Look

// An enderman takes being looked in the eyes as a stare (Enderman.isBeingStaredBy): the view within a narrow cone of
// its eyes, closer the narrower.
fn stared(eye: [f64; 3], look: [f64; 3], at: [f64; 3]) -> bool {
    let to = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
    let d = dist(eye, at);
    d > 0.0 && (look[0] * to[0] + look[1] * to[1] + look[2] * to[2]) / d > 1.0 - 0.025 / d
}

fn enderman_look(game: &mut Game<'_>, m: &Module) {
    // A carved pumpkin (or creative) and nothing happens.
    if game.creative || game.armor[3].as_ref().is_some_and(|s| short(&s.item) == "carved_pumpkin") {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let away = m.p("Look mode") == "Away";
    for e in game.mobs.iter().filter(|x| x.alive() && !x.removed && short(&x.kind.id) == "enderman") {
        let head = [e.position.x, e.position.y + 2.55, e.position.z];
        if !clear_line(game, head, [i32::MIN; 3]) {
            continue;
        }
        // extra.flag: the enderman is screaming (angry).
        let angry = e.extra.flag;
        let (y, p) = look_at(eye, head);
        if away && angry && m.b("Stun hostiles") || !away && !angry {
            turn("Enderman Look", y, p);
            return;
        }
        if away && stared(eye, look, head) {
            turn("Enderman Look", game.yaw, 90.0);
            return;
        }
    }
}

// ---- Click TP

// Items a right click uses: Click TP leaves those (and blocks to place) to the game.
const USED: &str = "bow,crossbow,trident,shield,spyglass,ender_pearl,ender_eye,snowball,egg,fishing_rod,wind_charge,splash_potion,lingering_potion,experience_bottle,fire_charge,flint_and_steel,bucket,water_bucket,lava_bucket,goat_horn,brush,mace,lead,name_tag,saddle";

// A right click (each press) on a block within Max distance: you stand on it (beside it for a side), one move away, as
// the server allows a move of up to 10 blocks in a tick. A held block, something to use, or a block that does something
// when clicked is left to the game.
fn click_tp(game: &mut Game<'_>, m: &Module) {
    let pressed = wp(|w| !std::mem::replace(&mut w.using, game.right)) && game.right;
    if !pressed {
        return;
    }
    if let Some(info) = super::sunset::item_info(game, game.selected) {
        if info.block.is_some() || info.food.is_some() || info.consumable.is_some() || listed(USED, &info.id) {
            return;
        }
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let range = m.n("Max distance") + 3.0;
    let mut last = eye.map(|v| v.floor() as i32);
    let mut hit = None;
    for i in 1..=(range * 20.0) as i32 {
        let t = f64::from(i) / 20.0;
        let at = [0, 1, 2].map(|k| (eye[k] + look[k] * t).floor() as i32);
        if at == last {
            continue;
        }
        if !is_air(game, at) && facts(game, at).full_collision {
            hit = Some((at, [at[0] - last[0], at[1] - last[1], at[2] - last[2]]));
            break;
        }
        last = at;
    }
    let Some((block, step)) = hit else {
        return;
    };
    let f = facts(game, block);
    let name = short(&f.name);
    if f.block_entity || ["door", "button", "lever", "gate", "anvil", "table", "bed"].iter().any(|k| name.contains(k)) {
        return;
    }
    // On top of it; a side clicked: on top of the block beside it, Meteor's way.
    let to = [f64::from(block[0]) + 0.5 - f64::from(step[0].signum()), f64::from(block[1]) + 1.0, f64::from(block[2]) + 0.5 - f64::from(step[2].signum())];
    let to = if step[1] != 0 { [f64::from(block[0]) + 0.5, f64::from(block[1]) + 1.0, f64::from(block[2]) + 0.5] } else { to };
    let Some(p) = game.player.as_mut() else {
        return;
    };
    if dist([p.position.x, p.position.y, p.position.z], to) > m.n("Max distance") {
        return;
    }
    p.position.x = to[0];
    p.position.y = to[1];
    p.position.z = to[2];
    p.velocity = p.velocity.scale(0.0);
}

// ---- Auto Sign

// A sign's editor opening once a text is kept: closed at once, the text sent for it after Delay ticks (some servers
// refuse a sign written too soon after the click).
fn auto_sign(game: &mut Game<'_>, m: &Module) {
    if let Some(lines) = wp(|w| w.sign_text.clone())
        && let Some(edit) = game.sign_edit.take()
    {
        let (x, y, z) = edit.pos;
        game.capture(true);
        wp(|w| w.signs.push_back(play::SignUpdate { pos: [x, y, z], lines, front: edit.front }));
    }
    let next = wp(|w| {
        if w.signs.is_empty() {
            w.sign_timer = 0;
            return None;
        }
        w.sign_timer += 1;
        if f64::from(w.sign_timer) <= m.n("Delay") {
            return None;
        }
        w.sign_timer = 0;
        w.signs.pop_front()
    });
    if let Some(p) = next {
        game.send(S::SignUpdate(p));
    }
}

#[cfg(test)]
mod tests {
    use super::stared;

    #[test]
    fn a_stare_is_a_narrow_cone_at_the_eyes() {
        let eye = [0.0, 1.62, 0.0];
        assert!(stared(eye, [0.0, 0.0, 1.0], [0.0, 1.62, 10.0]));
        assert!(!stared(eye, [0.0, 0.0, 1.0], [2.0, 1.62, 10.0]));
        assert!(!stared(eye, [1.0, 0.0, 0.0], [0.0, 1.62, 10.0]));
    }
}
