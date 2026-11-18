// Runtime part (see runtime.rs): small shared helpers: chat lines, numbers, keys, players, aiming.
use crate::game::sunset_compat::SunsetGame as _;

use super::{Game, Screen};
use super::sunset_bind_state::canonical;
use horizon_client::text::Text;
use winit::keyboard::KeyCode;
use super::sunset::*;

// ---------------------------------------------------------------- helpers

pub(super) fn on_off(b: bool) -> &'static str {
    if b { "ON" } else { "OFF" }
}

pub(super) fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

pub(super) fn round_to(v: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return v;
    }
    (v / step).round() * step
}

// Lowercase letters and digits only: how commands and the config name things.
pub(super) fn norm(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

// A key or mouse button typed in a command (`r`, `rshift`, `f6`, `mouse4`, `key.keyboard.r`) as the game names it.
pub(super) fn key_from_text(t: &str) -> Option<String> {
    canonical(t)
}

pub(super) fn key_label(key: Option<&str>) -> String {
    match key {
        None => "None".into(),
        Some(k) => {
            if let Some(button) = k.strip_prefix("key.mouse.") {
                return format!("Mouse {}", match button {
                    "left" => "Left",
                    "right" => "Right",
                    "middle" => "Middle",
                    "4" => "Back",
                    "5" => "Forward",
                    other => other,
                });
            }
            let short = k.strip_prefix("key.keyboard.").or_else(|| k.strip_prefix("key.mouse.")).unwrap_or(k);
            let mut out = String::new();
            for (i, part) in short.split('.').enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                let mut c = part.chars();
                if let Some(f) = c.next() {
                    out.extend(f.to_uppercase());
                    out.push_str(c.as_str());
                }
            }
            out
        }
    }
}

// A module turned on or off: a toast, replacing that module's last one, unless Notifications' Module toggles is off
// (a typed .t is still answered then).
pub(super) fn announce_toggle(name: &'static str, on: bool) {
    use super::sunset_toast::{Kind, push_keyed, toggles_shown};
    if running() && (toggles_shown() || REPLYING.load(std::sync::atomic::Ordering::Relaxed)) {
        push_keyed(if on { Kind::Success } else { Kind::Info }, &format!("{name} {}", if on { "on" } else { "off" }), Some(name));
    }
}

// Set while a command typed in the chat runs: its replies are lines in the chat, under the command.
pub(super) static REPLYING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// A message from Sunset: a toast (Notifications), or a chat line when it answers a command typed in the chat.
pub(super) fn say(game: &mut Game<'_>, msg: &str) {
    if !running() {
        return;
    }
    if REPLYING.load(std::sync::atomic::Ordering::Relaxed) {
        chat_line(game, msg);
    } else {
        super::sunset_toast::push(super::sunset_toast::kind_of(msg), msg);
    }
}

// A line in chat: Sunset's tag, then the message.
pub(super) fn chat_line(game: &mut Game<'_>, msg: &str) {
    if !running() {
        return;
    }
    let line = Text::literal("[Sunset] ")
        .with_color(accent())
        .append(Text::literal(msg).with_color(horizon_client::text::WHITE));
    let focused = game.screen == Screen::Chat;
    let atlas = &game.gui.atlas;
    game.ui.chat.add(
        atlas,
        line,
        None,
        horizon_client::chat::ChatSource::Client,
        game.ticks,
        focused,
    );
}

pub(super) fn own_id(game: &Game<'_>) -> Option<u32> {
    game.remote.as_ref().map(|r| r.entity_id as u32)
}

// The move keys held now: forward minus back, left minus right.
pub(super) fn move_input(game: &Game<'_>) -> (f64, f64) {
    if !game.native_playing() {
        return (0.0, 0.0);
    }
    let held = |k: KeyCode| f64::from(u8::from(game.keys.raw(&k)));
    (
        held(KeyCode::KeyW) - held(KeyCode::KeyS),
        held(KeyCode::KeyA) - held(KeyCode::KeyD),
    )
}

// The direction the move keys point, flat, length 1 (or zero).
pub(super) fn move_direction(game: &Game<'_>) -> Option<[f64; 2]> {
    let (f, s) = move_input(game);
    if f == 0.0 && s == 0.0 {
        return None;
    }
    let yaw = f64::from(game.yaw).to_radians();
    // Yaw 0 faces south (+z); left of it is east (+x).
    let x = -yaw.sin() * f + yaw.cos() * s;
    let z = yaw.cos() * f + yaw.sin() * s;
    let len = (x * x + z * z).sqrt();
    Some([x / len, z / len])
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Player,
    Hostile,
    Passive,
}

pub(super) fn kind_of(mob: &horizon_client::mobs::Mob) -> Kind {
    if mob.kind.id == "minecraft:player" {
        Kind::Player
    } else if mob.hostile() || mob.kind.category == "monster" {
        Kind::Hostile
    } else {
        Kind::Passive
    }
}

// A mob of a kind switched on, or of a type in the module's Entities list.
pub(super) fn wanted(m: &Module, kind: Kind, id: &str) -> bool {
    let of_kind = match kind {
        Kind::Player => m.b("Players"),
        Kind::Hostile => m.b("Hostiles"),
        Kind::Passive => m.b("Passive"),
    };
    super::sunset_util::kind_wanted(of_kind, m.t("Entities"), m.p("List mode") == "Blacklist", id)
}

pub(super) fn is_friend(friends: &[String], names: &[(u32, String)], id: u32) -> bool {
    names
        .iter()
        .find(|(i, _)| *i == id)
        .is_some_and(|(_, n)| friends.iter().any(|f| f.eq_ignore_ascii_case(n)))
}

pub(super) fn look_at(eye: [f64; 3], at: [f64; 3]) -> (f32, f32) {
    let (dx, dy, dz) = (at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]);
    let yaw = (-dx).atan2(dz).to_degrees();
    let pitch = -dy.atan2((dx * dx + dz * dz).sqrt()).to_degrees();
    (yaw as f32, pitch as f32)
}
