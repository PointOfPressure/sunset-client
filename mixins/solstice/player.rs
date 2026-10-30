// More of Meteor's player and misc modules: Auto Fish, Auto Gap, Fast Use,
// Spam, and Anchor.

use super::solstice::{Category, Module, hold};
use super::solstice_util::*;
use super::{Game, Screen};
use std::sync::Mutex;
use horizon_protocol::packets::{Empty, play::{self, Serverbound as S}};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        Module::new("Auto Fish", Player, "Casts and reels in for you.")
            .bool("Auto switch", true)
            .bool("Anti break", true)
            .bool("Auto cast", true)
            .num("Cast delay (ticks)", 14.0, 0.0, 60.0, 1.0)
            .num("Catch delay (ticks)", 6.0, 0.0, 60.0, 1.0),
        Module::new("Auto Gap", Player, "Eats golden apples when your health is low or an effect is missing.")
            .bool("Allow egap", true)
            .bool("Always", false)
            .bool("Potions regeneration", false)
            .bool("Potions fire resistance", true)
            .bool("Potions absorption", false)
            .bool("Health enabled", true)
            .num("Health threshold", 20.0, 1.0, 40.0, 1.0),
        Module::new("Fast Use", Player, "Lets you use items at very high speeds.")
            .pick("Mode", &["All", "Some"])
            .text("Items", "ender_pearl,experience_bottle,snowball,egg")
            .bool("Blocks", false)
            .num("Cooldown (ticks)", 0.0, 0.0, 4.0, 1.0),
        Module::new("Spam", Player, "Spams chat messages. Separate several messages with |.")
            .text("Messages", "Solstice on top!")
            .num("Delay (ticks)", 20.0, 1.0, 400.0, 1.0)
            .bool("Randomise", false)
            .bool("Bypass", false)
            .num("Bypass length", 16.0, 4.0, 32.0, 1.0),
        Module::new("Reach", Player, "Longer arms for blocks (on a vanilla server only about 4 blocks more, and only for chests and vehicles).")
            .num("Extra block reach", 1.0, 0.0, 6.0, 0.5),
        Module::new("Air Place", Player, "Right click places the block you hold where the crosshair points, with nothing to place it against.")
            .num("Range", 5.0, 2.0, 6.0, 0.5),
        Module::new("Anchor", Category::Movement, "Stops your movement over a hole so you drop into it.")
            .num("Max height", 10.0, 1.0, 20.0, 1.0)
            .num("Min pitch", 0.0, -90.0, 90.0, 5.0)
            .bool("Pull", false)
            .num("Pull speed", 0.3, 0.05, 1.0, 0.05),
    ]
}

#[derive(Default)]
struct Ps {
    fish_state: u8,
    cast_left: u32,
    catch_left: u32,
    gap_eating: bool,
    gap_prev: usize,
    spam_timer: u32,
    spam_index: usize,
    rng: u64,
    afk_jump: bool,
    afk_sneak: u32,
    afk_strafe: i32,
    afk_right: bool,
    afk_key: Option<KeyCode>,
    afk_yaw: Option<f32>,
    afk_message: i32,
    afk_index: usize,
}

static PS: Mutex<Option<Ps>> = Mutex::new(None);

fn ps<R>(f: impl FnOnce(&mut Ps) -> R) -> R {
    let mut g = PS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Ps::default))
}

fn rand(n: usize) -> usize {
    ps(|p| {
        p.rng = p.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407 + n as u64);
        ((p.rng >> 33) as usize) % n.max(1)
    })
}

// Reach's extra, read by the reach hook (f64 bits; None when off).
static REACH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

pub(super) fn reach_extra() -> Option<f64> {
    let bits = REACH.load(std::sync::atomic::Ordering::Relaxed);
    (bits != u64::MAX).then(|| f64::from_bits(bits))
}

// Right mouse button down while playing: Air Place.
pub(super) fn right_click(game: &mut Game<'_>) {
    let Some(m) = super::solstice::with(|s| s.module("Air Place").filter(|m| m.on).cloned()) else {
        return;
    };
    if !in_world(game) || game.pick().is_some() || game.entity_targeted() {
        return;
    }
    let held = game.held().is_some_and(|s| game.res.items().get(&s.item).is_some_and(|i| i.block.is_some()));
    if !held {
        return;
    }
    let (Some(eye), yaw, pitch) = (game.eye_position(), f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians()) else {
        return;
    };
    let d = m.n("Range");
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let at = [
        (eye[0] + look[0] * d).floor() as i32,
        (eye[1] + look[1] * d).floor() as i32,
        (eye[2] + look[2] * d).floor() as i32,
    ];
    let slot = game.selected;
    place_block(game, at, slot, false, true);
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    REACH.store(
        get("Reach").map_or(u64::MAX, |m| m.n("Extra block reach").to_bits()),
        std::sync::atomic::Ordering::Relaxed,
    );
    if !in_world(game) {
        return;
    }
    if let Some(m) = get("Auto Fish") {
        auto_fish(game, m);
    } else {
        ps(|p| p.fish_state = 0);
    }
    if let Some(m) = get("Auto Gap") {
        auto_gap(game, m);
    } else if ps(|p| std::mem::take(&mut p.gap_eating)) {
        let _ = game.play_mouse(MouseButton::Right, false);
    }
    if let Some(m) = get("Fast Use") {
        fast_use(game, m);
    }
    if let Some(m) = get("Anti AFK") {
        anti_afk(game, m);
    } else {
        ps(|p| {
            p.afk_yaw = None;
            p.afk_message = 0;
        });
    }
    if let Some(m) = get("Spam") {
        spam(game, m);
    } else {
        ps(|p| p.spam_timer = 0);
    }
    if let Some(m) = get("Anchor") {
        anchor(game, m);
    }
}

// ---- Auto Fish

fn rod_ok(game: &Game<'_>, slot: usize, anti_break: bool) -> Option<u32> {
    let s = game.inventory.get(slot)?.as_ref()?;
    if short(&s.item) != "fishing_rod" {
        return None;
    }
    let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
    if anti_break && max > 0 && s.damage + 1 >= max {
        return None;
    }
    let lvl = |e: &str| s.enchantment_level(e);
    Some(lvl("minecraft:luck_of_the_sea") + lvl("minecraft:lure") + lvl("minecraft:mending") + lvl("minecraft:unbreaking"))
}

fn use_rod(game: &mut Game<'_>, m: &Module) -> bool {
    let anti = m.b("Anti break");
    let selected = game.selected;
    let mut best: Option<(usize, u32)> = rod_ok(game, selected, anti).map(|s| (selected, s));
    for i in 0..9 {
        if i == selected {
            continue;
        }
        if let Some(score) = rod_ok(game, i, anti) {
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((i, score));
            }
        }
    }
    let Some((slot, _)) = best else {
        return false;
    };
    if slot != selected {
        if !m.b("Auto switch") {
            return false;
        }
        hold(game, slot);
    }
    let _ = game.play_mouse(MouseButton::Right, true);
    let _ = game.play_mouse(MouseButton::Right, false);
    ps(|p| p.cast_left = m.n("Cast delay (ticks)") as u32);
    true
}

fn auto_fish(game: &mut Game<'_>, m: &Module) {
    match super::fishing::solstice_fishing::state(game) {
        None => {
            // Idle: cast again after the delay.
            ps(|p| p.fish_state = 0);
            if !m.b("Auto cast") {
                return;
            }
            if ps(|p| {
                if p.cast_left > 0 {
                    p.cast_left -= 1;
                    true
                } else {
                    false
                }
            }) {
                return;
            }
            if use_rod(game, m) {
                ps(|p| p.fish_state = 1);
            }
        }
        Some((bobbing, biting, hooked)) => {
            let state = ps(|p| p.fish_state);
            match state {
                0 => ps(|p| p.fish_state = 1),
                1 => {
                    if hooked {
                        if use_rod(game, m) {
                            ps(|p| p.fish_state = 0);
                        }
                    } else if bobbing && biting {
                        ps(|p| {
                            p.catch_left = m.n("Catch delay (ticks)") as u32;
                            p.fish_state = 2;
                        });
                    }
                }
                _ => {
                    let wait = ps(|p| {
                        if p.catch_left > 0 {
                            p.catch_left -= 1;
                            true
                        } else {
                            false
                        }
                    });
                    if hooked || !wait {
                        if use_rod(game, m) {
                            ps(|p| p.fish_state = 0);
                        }
                    }
                }
            }
        }
    }
}

// ---- Auto Gap

fn gap_slot(game: &Game<'_>, m: &Module, need_egap: bool) -> Option<usize> {
    let plain = if need_egap { None } else { find_hotbar(game, |n| n == "golden_apple") };
    plain.or_else(|| if m.b("Allow egap") || need_egap { find_hotbar(game, |n| n == "enchanted_golden_apple") } else { None })
}

fn auto_gap(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let effects = &game.potions.effects;
    let missing = |name: &str| effects.amplifier(name).is_none();
    let mut need_egap = false;
    let mut should = m.b("Always");
    if !should && m.b("Potions regeneration") && missing("regeneration") {
        should = true;
    }
    if !should && m.b("Potions fire resistance") && missing("fire_resistance") {
        should = true;
        need_egap = true;
    }
    if !should && m.b("Potions absorption") && missing("absorption") {
        should = true;
        need_egap = true;
    }
    if !should && m.b("Health enabled") {
        let health = (p.health.health() + p.health.absorption()).round();
        should = f64::from(health) < m.n("Health threshold");
    }
    let eating = ps(|s| s.gap_eating);
    if eating {
        if should {
            match gap_slot(game, m, need_egap) {
                Some(slot) => {
                    hold(game, slot);
                    if game.using.is_none() {
                        let _ = game.play_mouse(MouseButton::Right, true);
                    }
                }
                None => stop_eating(game),
            }
        } else {
            stop_eating(game);
        }
    } else if should {
        if let Some(slot) = gap_slot(game, m, need_egap) {
            let prev = game.selected;
            ps(|s| {
                s.gap_prev = prev;
                s.gap_eating = true;
            });
            hold(game, slot);
            let _ = game.play_mouse(MouseButton::Right, true);
        }
    }
}

fn stop_eating(game: &mut Game<'_>) {
    let _ = game.play_mouse(MouseButton::Right, false);
    let prev = ps(|s| {
        s.gap_eating = false;
        s.gap_prev
    });
    hold(game, prev);
}

// ---- Fast Use

fn fast_use(game: &mut Game<'_>, m: &Module) {
    let item = game.held().map(|s| short(&s.item).to_owned());
    let applies = m.p("Mode") == "All"
        || item.as_ref().is_some_and(|i| {
            listed(m.t("Items"), i)
                || m.b("Blocks") && game.res.items().get(&format!("minecraft:{i}")).is_some_and(|inf| inf.block.is_some())
        });
    if applies && game.use_delay > m.n("Cooldown (ticks)") as u32 {
        game.use_delay = m.n("Cooldown (ticks)") as u32;
    }
}

// ---- Anti AFK (Meteor's AntiAFK)

// The keys it holds this tick: a jump now and then, sneaking, and strafing left and right.
pub(super) fn afk_keys(m: &Module) -> Vec<KeyCode> {
    let mut keys = Vec::new();
    if m.b("Jump") {
        let jump = ps(|p| {
            p.afk_jump = !p.afk_jump && rand(99) == 0;
            p.afk_jump
        });
        if jump {
            keys.push(KeyCode::Space);
        }
    }
    if m.b("Sneak") {
        let sneak = ps(|p| {
            let held = p.afk_sneak < m.n("Sneak time") as u32;
            p.afk_sneak += 1;
            held
        });
        if sneak {
            keys.push(KeyCode::ShiftLeft);
        } else if rand(99) == 0 {
            ps(|p| p.afk_sneak = 0);
        }
    }
    if m.b("Strafe") {
        let key = ps(|p| {
            p.afk_strafe -= 1;
            if p.afk_strafe <= 0 {
                p.afk_key = Some(if p.afk_right { KeyCode::KeyD } else { KeyCode::KeyA });
                p.afk_right = !p.afk_right;
                p.afk_strafe = 20;
            }
            p.afk_key
        });
        keys.extend(key);
    }
    keys
}

fn anti_afk(game: &mut Game<'_>, m: &Module) {
    if !in_world(game) {
        return;
    }
    if m.b("Swing") && rand(99) == 0 {
        game.swing_arm();
        game.send(S::Punch(Empty));
    }
    if m.b("Spin") {
        let yaw = ps(|p| {
            let y = p.afk_yaw.unwrap_or(game.yaw) + m.n("Speed") as f32;
            p.afk_yaw = Some(y);
            y
        });
        if m.p("Spin mode") == "Client" {
            game.yaw = yaw;
        } else {
            let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
            game.send(S::MovePlayerRot(play::MovePlayerRot {
                yaw,
                pitch: m.n("Pitch") as f32,
                flags: u8::from(ground),
            }));
        }
    }
    if m.b("Send messages") {
        let messages: Vec<&str> = m.t("Messages").split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
        if messages.is_empty() {
            return;
        }
        let due = ps(|p| {
            p.afk_message -= 1;
            p.afk_message <= 0
        });
        if due {
            let i = if m.b("Random") {
                rand(messages.len())
            } else {
                ps(|p| {
                    p.afk_index = (p.afk_index + 1) % messages.len();
                    p.afk_index
                })
            };
            game.send_chat(messages[i]);
            ps(|p| p.afk_message = m.n("Delay (seconds)") as i32 * 20);
        }
    }
}

// ---- Spam

fn spam(game: &mut Game<'_>, m: &Module) {
    if game.remote.is_none() {
        toggle_off(game, "Spam");
        return;
    }
    if ps(|p| {
        if p.spam_timer > 0 {
            p.spam_timer -= 1;
            true
        } else {
            false
        }
    }) {
        return;
    }
    let messages: Vec<&str> = m.t("Messages").split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
    if messages.is_empty() {
        return;
    }
    let i = if m.b("Randomise") {
        rand(messages.len())
    } else {
        ps(|p| {
            let i = p.spam_index % messages.len();
            p.spam_index = i + 1;
            i
        })
    };
    let mut text = messages[i].to_owned();
    if m.b("Bypass") {
        const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        text.push(' ');
        for _ in 0..m.n("Bypass length") as usize {
            text.push(LETTERS[rand(LETTERS.len())] as char);
        }
    }
    text.truncate(256);
    game.send_chat(&text);
    ps(|p| p.spam_timer = m.n("Delay (ticks)") as u32);
}

// ---- Anchor

fn hole_block(game: &Game<'_>, p: [i32; 3]) -> bool {
    let n = facts(game, p).name.clone();
    matches!(short(&n), "bedrock" | "obsidian" | "crying_obsidian")
}

fn is_hole(game: &Game<'_>, x: i32, y: i32, z: i32) -> bool {
    hole_block(game, [x, y - 1, z])
        && hole_block(game, [x + 1, y, z])
        && hole_block(game, [x - 1, y, z])
        && hole_block(game, [x, y, z + 1])
        && hole_block(game, [x, y, z - 1])
}

fn anchor(game: &mut Game<'_>, m: &Module) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    if game.pitch < m.n("Min pitch") as f32 {
        return;
    }
    let (px, py, pz) = (p.position.x, p.position.y, p.position.z);
    let (x, mut y, z) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
    if is_hole(game, x, y, z) {
        return;
    }
    let mut found = None;
    for _ in 0..m.n("Max height") as i32 {
        y -= 1;
        if y <= -64 || !(is_air(game, [x, y, z]) || !facts(game, [x, y, z]).full_collision) {
            break;
        }
        if is_hole(game, x, y, z) {
            found = Some([f64::from(x) + 0.5, f64::from(z) + 0.5]);
            break;
        }
    }
    if let Some([hx, hz]) = found {
        let pull = if m.b("Pull") { m.n("Pull speed") } else { 0.0 };
        if let Some(p) = game.player.as_mut() {
            p.velocity.x = (hx - px).clamp(-0.05, 0.05);
            p.velocity.z = (hz - pz).clamp(-0.05, 0.05);
            p.velocity.y -= pull;
        }
    }
}

#[allow(dead_code)]
fn unused(_: Screen) {}
