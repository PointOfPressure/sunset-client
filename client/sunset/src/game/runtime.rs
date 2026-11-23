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
pub(crate) const VERSION: &str = "2.1.0";
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
    std::sync::atomic::AtomicU32::new(0x3F80_0000),
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


// The colours of Sunset's menus and HUD surfaces, one table per ClickGUI Theme. Grey is the look before OLED existed;
// OLED keeps every surface true black and lifts controls, tracks and edges just enough to read on it.
pub(super) struct Palette {
    // Dropdown panels (written at 0.75, through bg), the settings window, the picker and editor dialogs.
    pub panel: u32,
    pub frame: u32,
    pub dialog: u32,
    // A 1px edge round panels, windows and dialogs (0: none).
    pub border: u32,
    // Search and text fields, value boxes, slider tracks, switches off, a switch's knob, a list's box unticked, a
    // dialog's second button.
    pub field: u32,
    pub value_box: u32,
    pub track: u32,
    pub toggle_off: u32,
    pub knob: u32,
    pub check_off: u32,
    pub button: u32,
    pub tooltip: u32,
    // Lines between rows and under headers.
    pub line: u32,
    pub rule: u32,
    // The Dashboard style.
    pub dash_surface: u32,
    pub dash_hairline: u32,
    pub dash_control: u32,
    pub dash_card: u32,
    pub dash_toggle_off: u32,
    // The profile menu, toasts and the HUD lists drawn like them, labels in the world (Nametags and the like), small
    // HUD boxes, container previews, the loadout buttons (and under the pointer).
    pub profile_panel: u32,
    pub profile_tab: u32,
    pub toast: u32,
    pub label_bg: u32,
    pub hud_box: u32,
    pub grid: u32,
    pub hud_button: u32,
    pub hud_button_hover: u32,
}

const GREY: Palette = Palette {
    panel: 0xC00E_0E0E,
    frame: 0xF812_1212,
    dialog: 0xF814_1414,
    border: 0,
    field: 0xFF08_0808,
    value_box: 0xFF1E_1E1E,
    track: 0xFF2A_2A2A,
    toggle_off: 0xFF5A_5A5A,
    knob: 0xFF10_1010,
    check_off: 0xFF3A_3A3A,
    button: 0xFF3A_3A3A,
    tooltip: 0xF80A_0A0A,
    line: 0x12FF_FFFF,
    rule: 0x24FF_FFFF,
    dash_surface: 0xFF12_1214,
    dash_hairline: 0xFF26_262B,
    dash_control: 0xFF1C_1C20,
    dash_card: 0xFF17_171A,
    dash_toggle_off: 0xFF2E_2E33,
    profile_panel: 0xFF12_1214,
    profile_tab: 0xFF28_282C,
    toast: 0xE814_1418,
    label_bg: 0x7800_0000,
    hud_box: 0x960E_0E0E,
    grid: 0xE018_1818,
    hud_button: 0xC030_3030,
    hud_button_hover: 0xC050_5050,
};

const OLED: Palette = Palette {
    panel: 0xC000_0000,
    frame: 0xFF00_0000,
    dialog: 0xFF00_0000,
    border: 0xFF2C_2C2C,
    field: 0xFF14_1414,
    value_box: 0xFF1A_1A1A,
    track: 0xFF33_3333,
    toggle_off: 0xFF40_4040,
    knob: 0xFF00_0000,
    check_off: 0xFF33_3333,
    button: 0xFF22_2222,
    tooltip: 0xFF00_0000,
    line: 0x1AFF_FFFF,
    rule: 0x30FF_FFFF,
    dash_surface: 0xFF00_0000,
    dash_hairline: 0xFF2C_2C2C,
    dash_control: 0xFF14_1414,
    dash_card: 0xFF0C_0C0C,
    dash_toggle_off: 0xFF2A_2A2A,
    profile_panel: 0xFF00_0000,
    profile_tab: 0xFF1A_1A1A,
    toast: 0xF400_0000,
    label_bg: 0xC000_0000,
    hud_box: 0xD000_0000,
    grid: 0xF400_0000,
    hud_button: 0xE016_1616,
    hud_button_hover: 0xE02E_2E2E,
};

// ClickGUI's Theme: OLED (true) or Grey (false, the default).
pub(super) static OLED_THEME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn pal() -> &'static Palette {
    if OLED_THEME.load(Ordering::Relaxed) { &OLED } else { &GREY }
}

// A rounded panel with the theme's edge round it, px wide (a pixel at the GUI's scale).
pub(super) fn framed(out: &mut Vec<Shape>, [x, y, w, h]: [f32; 4], radius: f32, fill: u32, px: f32) {
    let border = pal().border;
    if border != 0 {
        out.push(Shape::Round([x - px, y - px, w + 2.0 * px, h + 2.0 * px], radius + px, border));
    }
    out.push(Shape::Round([x, y, w, h], radius, fill));
}
