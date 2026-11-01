// Sword PvP Bot, from Rust's PR #16 (its settings and what it does; written small here): fights the nearest enemy
// player with your sword by itself. It turns you towards them at a human pace, walks and sprints in, keeps its
// distance (Patient) or presses on (Aggressive), strafes, jumps for critical hits and on being hit (Jump reset), and
// eats a golden apple when low. Hits go through the strikes queue like Kill Aura's. Your own movement keys take over.
use super::sunset::{self, Category, Module, hold, own_id};
use super::sunset_util as util;
use super::{Game, Screen};
use std::collections::HashSet;
use std::sync::Mutex;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

const NAME: &str = "Sword PvP Bot";
const KEYS: [KeyCode; 6] = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD, KeyCode::ControlLeft, KeyCode::Space];
// The vanilla reach to a player's box, with a little kept back.
const REACH: f64 = 2.9;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Fights the nearest enemy player with your sword by itself: turns to them, walks in, strafes, jumps for critical hits and eats a golden apple when low. Patient keeps its distance between hits, Aggressive presses on. Your own movement keys take over.")
            .pick("Style", &["Patient", "Aggressive"])
            .num("Fight range", 16.0, 6.0, 32.0, 1.0)
            .num("Turn speed", 1.0, 0.5, 1.5, 0.05)
            .bool("Strafe", true)
            .bool("Jump crits", true)
            .bool("Jump reset", true)
            .num("Gapple below HP", 10.0, 0.0, 20.0, 1.0),
    ]
}

#[derive(Default)]
struct Bot {
    // The keys the bot holds down (never one you hold yourself).
    pressed: HashSet<KeyCode>,
    // Strafing right (else left), and the tick it turns the other way.
    right: bool,
    flip_at: u64,
    // Eating: the slot to go back to.
    eating: Option<usize>,
    health: f32,
    // Ticks the hit has been ready for, on the ground, waiting for a jump to fall.
    ready_for: u32,
}

static BOT: Mutex<Option<Bot>> = Mutex::new(None);

fn bot<R>(f: impl FnOnce(&mut Bot) -> R) -> R {
    f(BOT.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Bot::default))
}

// Lets go of the keys the bot holds and stops eating.
pub(super) fn release(game: &mut Game<'_>) {
    let (pressed, eating) = bot(|b| (std::mem::take(&mut b.pressed), b.eating.take()));
    for key in pressed {
        if !game.physical_keys.contains(&key) {
            game.keys.remove(&key);
        }
    }
    if let Some(back) = eating {
        let _ = game.play_mouse(MouseButton::Right, false);
        hold(game, back);
    }
}

fn press(game: &mut Game<'_>, want: &[KeyCode]) {
    let before = bot(|b| std::mem::take(&mut b.pressed));
    let mut pressed = HashSet::new();
    for key in KEYS {
        if want.contains(&key) {
            game.keys.insert(key);
            pressed.insert(key);
        } else if before.contains(&key) && !game.physical_keys.contains(&key) {
            game.keys.remove(&key);
        }
    }
    bot(|b| b.pressed = pressed);
}

// Turns the view towards (yaw, pitch) by at most `step` degrees a tick.
fn turn(game: &mut Game<'_>, yaw: f32, pitch: f32, step: f32) {
    let dy = (yaw - game.yaw + 540.0).rem_euclid(360.0) - 180.0;
    game.yaw += dy.clamp(-step, step);
    game.pitch += (pitch - game.pitch).clamp(-step, step);
}

// Ground a step ahead along `dir`: a drop of more than 2 blocks, lava or fire there is no place to walk.
fn safe_ahead(game: &Game<'_>, dir: [f64; 2]) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let (x, z) = ((p.position.x + dir[0]).floor() as i32, (p.position.z + dir[1]).floor() as i32);
    let y = p.position.y.floor() as i32;
    let burns = |y: i32| {
        let name = &game.res.facts().get(game.world.block(x, y, z)).name;
        name.ends_with(":lava") || name.ends_with(":fire")
    };
    if burns(y) || burns(y - 1) {
        return false;
    }
    (1..=3).any(|d| !game.res.facts().is_air(game.world.block(x, y - d, z)))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else {
        if bot(|b| !b.pressed.is_empty() || b.eating.is_some()) {
            release(game);
        }
        return;
    };
    let live = game.remote.is_some()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    // Your own movement keys: the bot lets go and waits.
    let yours = KEYS.iter().any(|k| game.physical_keys.contains(k));
    let me = own_id(game);
    let range = m.n("Fight range");
    let target = game
        .mobs
        .iter()
        .filter(|mob| Some(mob.id) != me && util::player_enemy(game, mob))
        .map(util::mob_target)
        .filter(|t| game.player.as_ref().is_some_and(|p| util::dist([p.position.x, p.position.y, p.position.z], t.pos) <= range))
        .min_by(|a, b| {
            let at = game.player.as_ref().map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
            util::dist(at, a.pos).total_cmp(&util::dist(at, b.pos))
        });
    let (Some(t), Some(eye), true, false) = (target, game.eye_position(), live, yours) else {
        release(game);
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (health, ground, falling, in_water) = (
        p.health.health() + p.health.absorption(),
        p.ground.on_ground,
        !p.ground.on_ground && p.velocity.y < 0.0,
        p.fluids.was_touching_water,
    );
    let hurt = bot(|b| std::mem::replace(&mut b.health, health)) > health + 0.01;
    // A golden apple when low; the fight waits while it eats.
    let low = f64::from(health) < m.n("Gapple below HP");
    let eating = bot(|b| b.eating);
    match (eating, low) {
        (Some(_), true) => {
            press(game, &[KeyCode::KeyS]);
            if game.using.is_none() {
                let _ = game.play_mouse(MouseButton::Right, true);
            }
            return;
        }
        (Some(_), false) => release(game),
        (None, true) => {
            if let Some(slot) = util::find_held(game, |n| n.ends_with("golden_apple")) {
                let back = game.selected;
                press(game, &[KeyCode::KeyS]);
                hold(game, slot);
                let _ = game.play_mouse(MouseButton::Right, true);
                bot(|b| b.eating = Some(back));
                return;
            }
        }
        (None, false) => {}
    }
    if let Some(sword) = util::find_held(game, |n| n.ends_with("_sword"))
        && !game.held().is_some_and(|s| s.item.ends_with("_sword") || s.item.ends_with("_axe"))
    {
        hold(game, sword);
    }
    // Aim at the nearest point of their box to your eyes' height, turned to at Turn speed (40 degrees a tick at 1).
    let aim = [(t.lo[0] + t.hi[0]) / 2.0, eye[1].clamp(t.lo[1] + 0.2, t.hi[1] - 0.2), (t.lo[2] + t.hi[2]) / 2.0];
    let (yaw, pitch) = sunset::look_at(eye, aim);
    turn(game, yaw, pitch, 40.0 * m.n("Turn speed") as f32);
    let near = [(t.lo[0] - eye[0]).max(eye[0] - t.hi[0]).max(0.0), (t.lo[1] - eye[1]).max(eye[1] - t.hi[1]).max(0.0), (t.lo[2] - eye[2]).max(eye[2] - t.hi[2]).max(0.0)];
    let reach = (near[0] * near[0] + near[1] * near[1] + near[2] * near[2]).sqrt();
    let off = ((yaw - game.yaw + 540.0).rem_euclid(360.0) - 180.0).abs();
    let ready = game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 1.0;
    // The hit: cooldown full, in reach and in view; with Jump crits, on the way down from a jump (or after 10 ticks
    // ready on the ground, so it never stalls).
    let ready_for = bot(|b| {
        b.ready_for = if ready && reach <= REACH && ground { b.ready_for + 1 } else { 0 };
        b.ready_for
    });
    let crit_ok = !m.b("Jump crits") || falling || in_water || ready_for > 10;
    if ready && reach <= REACH && off < 30.0 && crit_ok && !sunset::strike_waiting() {
        sunset::strike(game, t.id);
    }
    // Moving: in (sprinting) while out of reach; Patient backs off to 2.2 blocks while the cooldown fills, Aggressive
    // stays on them. Strafing turns the other way every 20 to 40 ticks.
    let mut want = Vec::new();
    let dir = [-(game.yaw.to_radians() as f64).sin(), (game.yaw.to_radians() as f64).cos()];
    let gap = util::dist([eye[0], 0.0, eye[2]], [t.pos[0], 0.0, t.pos[2]]);
    if gap > 3.0 || (m.p("Style") == "Aggressive" && gap > 1.5) {
        if safe_ahead(game, dir) {
            want.extend([KeyCode::KeyW, KeyCode::ControlLeft]);
        }
    } else if m.p("Style") == "Patient" && !ready && gap < 2.2 && safe_ahead(game, [-dir[0], -dir[1]]) {
        want.push(KeyCode::KeyS);
    }
    if m.b("Strafe") && gap < 6.0 {
        let now = game.ticks;
        let right = bot(|b| {
            if now >= b.flip_at {
                b.right = !b.right;
                b.flip_at = now + 20 + (now * 7919 % 21);
            }
            b.right
        });
        let side = if right { [-dir[1], dir[0]] } else { [dir[1], -dir[0]] };
        if safe_ahead(game, side) {
            want.push(if right { KeyCode::KeyD } else { KeyCode::KeyA });
        }
    }
    // A jump: for the critical hit as the cooldown comes round in reach, or as a hit lands (Jump reset).
    let soon = game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 0.85;
    if ground && !in_water && ((m.b("Jump crits") && soon && reach <= REACH + 0.5) || (m.b("Jump reset") && hurt)) {
        want.push(KeyCode::Space);
    }
    press(game, &want);
}
