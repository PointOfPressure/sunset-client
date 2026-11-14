// Sunset: Horizon's first cheat client, a utility mod, rebuilt in Rust as a
// Horizon native mod. Modules in categories, each with settings and a key
// bind, a menu on vanilla's own Options > Mods page, `.` chat commands, and a
// HUD with the active modules. The bundle adds this file to the game as
// `crate::game::sunset`, so it reads and changes the game's own state.
//
// The runtime is split into runtime_*.rs, each its own module beside this one (the
// bundle's modules cannot be nested). This module re-exports them all, so the rest of
// Sunset and plugins keep using `sunset::...`; a part reaches the others through
// `use super::sunset::*;`, and an item one part shares with another is `pub(super)`.
//
// Ordinary comments only: module payloads cannot hold inner attributes.

use std::sync::atomic::Ordering;

pub(super) use super::sunset_runtime_module::*;
pub(super) use super::sunset_runtime_catalog::*;
pub(super) use super::sunset_runtime_state::*;
pub(super) use super::sunset_runtime_config::*;
pub(super) use super::sunset_runtime_helpers::*;
pub(super) use super::sunset_runtime_menu::*;
pub(super) use super::sunset_runtime_tick::*;
pub(super) use super::sunset_runtime_player::*;
pub(super) use super::sunset_runtime_inventory::*;
pub(super) use super::sunset_runtime_world::*;
pub(super) use super::sunset_runtime_movement::*;
pub(super) use super::sunset_runtime_combat::*;
pub(super) use super::sunset_runtime_packets::*;
pub(super) use super::sunset_runtime_commands::*;
pub(super) use super::sunset_runtime_draw::*;
pub(super) use super::sunset_runtime_hud::*;
pub(super) use super::sunset_runtime_overlays::*;
pub(super) use super::sunset_runtime_gui::*;
pub(super) use super::sunset_runtime_gui_input::*;
pub(super) use super::sunset_runtime_gui_draw::*;

pub(crate) const ID: &str = "sunset";
pub(crate) const NAME: &str = "Sunset";
pub(crate) const VERSION: &str = "1.1.0";
pub(crate) const DESCRIPTION: &str =
    "Utility client: combat, movement and render modules, a menu, key binds and . commands.";
// The menu key's name on the mod's page and its default (Right Shift).
pub(crate) const MENU_KEY: &str = "menu";
pub(crate) const MENU_DEFAULT: &str = "key.keyboard.grave.accent";
// A second key for the same thing, Sunset's own default.
pub(crate) const MENU2_KEY: &str = "menu2";
pub(crate) const MENU2_DEFAULT: &str = "key.keyboard.right.shift";
// Sunset's sun gold.
// The menu's look, set by the ClickGUI module (accent colour, panel width, background opacity and dim).
// [accent, wanted panel width, opacity, dim, the panel width in use: what fits the screen, 0 before the first draw,
// text indent, wanted GUI scale, the GUI scale in use, panel radius, control radius, compact spacing (0 or 1)] (floats as
// bits).
pub(super) static LOOK: [std::sync::atomic::AtomicU32; 11] = [
    std::sync::atomic::AtomicU32::new(0xFFB020),
    std::sync::atomic::AtomicU32::new(0x42F0_0000),
    std::sync::atomic::AtomicU32::new(0x3F73_3333),
    std::sync::atomic::AtomicU32::new(35),
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0x4000_0000),
    std::sync::atomic::AtomicU32::new(0x3F80_0000),
    std::sync::atomic::AtomicU32::new(0x3F80_0000),
    std::sync::atomic::AtomicU32::new(0x4080_0000),
    std::sync::atomic::AtomicU32::new(0x4040_0000),
    std::sync::atomic::AtomicU32::new(0),
];

pub(super) fn accent() -> u32 {
    LOOK[0].load(Ordering::Relaxed)
}

// ClickGUI's Panel width as set, before any screen limits it.
pub(super) fn wanted_panel_w() -> f32 {
    f32::from_bits(LOOK[1].load(Ordering::Relaxed))
}

pub(super) fn panel_w() -> f32 {
    f32::from_bits(LOOK[4].load(Ordering::Relaxed))
}

// How far a module's name sits in from its row's edge; its settings sit 4 further in.
pub(super) fn indent() -> f32 {
    f32::from_bits(LOOK[5].load(Ordering::Relaxed))
}

pub(super) fn gui_scale() -> f32 {
    f32::from_bits(LOOK[7].load(Ordering::Relaxed))
}

pub(super) fn panel_radius() -> f32 {
    f32::from_bits(LOOK[8].load(Ordering::Relaxed))
}

pub(super) fn control_radius() -> f32 {
    f32::from_bits(LOOK[9].load(Ordering::Relaxed))
}

// A menu background colour with the ClickGUI's opacity (the colours are written at 0.75).
pub(super) fn bg(argb: u32) -> u32 {
    let k = f32::from_bits(LOOK[2].load(Ordering::Relaxed)) / 0.75;
    let a = ((argb >> 24) as f32 * k).round().clamp(0.0, 255.0) as u32;
    (a << 24) | (argb & 0x00FF_FFFF)
}
