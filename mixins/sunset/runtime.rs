// Sunset: Horizon's first cheat client, a Meteor Client style utility mod, rebuilt in Rust as a
// Horizon native mod. Modules in categories, each with settings and a key
// bind, a menu on vanilla's own Options > Mods page, `.` chat commands, and a
// HUD with the active modules. The bundle adds this file to the game as
// `crate::game::sunset`, so it reads and changes the game's own state.
//
// Ordinary comments only: module payloads cannot hold inner attributes.

use super::{Game, Screen};
use super::sunset_visual::{Draw, Kinds, Look, distance_colour, draw_box, faded, potato};
use super::sunset_bind_state::{Held, Inputs, canonical, holds};
use super::sunset_text_edit::{Draft, EditKey};
use super::sunset_util::{Sweep, block_pos};
use std::fmt::Write as _;
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;
use horizon_client::text::Text;
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::KeyCode;

pub(crate) const ID: &str = "sunset";
pub(crate) const NAME: &str = "Sunset";
pub(crate) const VERSION: &str = "0.8.1";
pub(crate) const DESCRIPTION: &str =
    "Utility client: combat, movement and render modules, a menu, key binds and . commands.";
// The menu key's name on the mod's page and its default (Right Shift).
pub(crate) const MENU_KEY: &str = "menu";
pub(crate) const MENU_DEFAULT: &str = "key.keyboard.grave.accent";
// A second key for the same thing, Meteor's own default.
pub(crate) const MENU2_KEY: &str = "menu2";
pub(crate) const MENU2_DEFAULT: &str = "key.keyboard.right.shift";
// Sunset's sun gold.
// The menu's look, set by the ClickGUI module (accent colour, panel width, background opacity and dim).
// [accent, wanted panel width, opacity, dim, the panel width in use: what fits the screen, 0 before the first draw,
// text indent, wanted GUI scale, the GUI scale in use, panel radius, control radius, compact spacing (0 or 1)] (floats as
// bits).
static LOOK: [std::sync::atomic::AtomicU32; 11] = [
    std::sync::atomic::AtomicU32::new(0xFFB020),
    std::sync::atomic::AtomicU32::new(0x42C8_0000),
    std::sync::atomic::AtomicU32::new(0x3F66_6666),
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0x40C0_0000),
    std::sync::atomic::AtomicU32::new(0x3F80_0000),
    std::sync::atomic::AtomicU32::new(0x3F80_0000),
    std::sync::atomic::AtomicU32::new(0x40C0_0000),
    std::sync::atomic::AtomicU32::new(0x4080_0000),
    std::sync::atomic::AtomicU32::new(0),
];

pub(super) fn accent() -> u32 {
    LOOK[0].load(Ordering::Relaxed)
}

fn panel_w() -> f32 {
    f32::from_bits(LOOK[4].load(Ordering::Relaxed))
}

// How far a module's name sits in from its row's edge; its settings sit 4 further in.
fn indent() -> f32 {
    f32::from_bits(LOOK[5].load(Ordering::Relaxed))
}

fn gui_scale() -> f32 {
    f32::from_bits(LOOK[7].load(Ordering::Relaxed))
}

fn panel_radius() -> f32 {
    f32::from_bits(LOOK[8].load(Ordering::Relaxed))
}

fn control_radius() -> f32 {
    f32::from_bits(LOOK[9].load(Ordering::Relaxed))
}

// A menu background colour with the ClickGUI's opacity (the colours are written at 0.75).
fn bg(argb: u32) -> u32 {
    let k = f32::from_bits(LOOK[2].load(Ordering::Relaxed)) / 0.75;
    let a = ((argb >> 24) as f32 * k).round().clamp(0.0, 255.0) as u32;
    (a << 24) | (argb & 0x00FF_FFFF)
}

// ---------------------------------------------------------------- modules

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Combat,
    Movement,
    Player,
    Render,
    World,
}

const CATEGORIES: [Category; 5] = [
    Category::Combat,
    Category::Movement,
    Category::Player,
    Category::Render,
    Category::World,
];

impl Category {
    fn label(self) -> &'static str {
        match self {
            Category::Combat => "Combat",
            Category::Movement => "Movement",
            Category::Player => "Player",
            Category::Render => "Render",
            Category::World => "World",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    Bool(bool),
    Num { v: f64, min: f64, max: f64, step: f64 },
    Pick { i: usize, of: &'static [&'static str] },
    // A comma separated list (blocks, items, entities): set with `.set`.
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Setting {
    pub(crate) name: &'static str,
    pub(crate) value: Value,
}

impl Setting {
    fn shown(&self) -> String {
        format!("{}: {}", self.name, self.value_text())
    }
    fn value_text(&self) -> String {
        match &self.value {
            Value::Bool(b) => on_off(*b).to_owned(),
            Value::Num { v, .. } => num(*v),
            Value::Pick { i, of } => of[*i].to_owned(),
            Value::Text(t) if t.is_empty() => "(empty)".to_owned(),
            Value::Text(t) => t.chars().map(|c| if c.is_control() { '?' } else { c }).collect(),
        }
    }
    fn stored(&self) -> String {
        match &self.value {
            Value::Bool(b) => b.to_string(),
            Value::Num { v, .. } => num(*v),
            Value::Pick { i, of } => of[*i].to_owned(),
            Value::Text(t) => t.clone(),
        }
    }
    // A click: a switch flips, a number goes up a step (down with Shift) and
    // wraps at its ends, a choice moves to the next.
    fn click(&mut self, down: bool) {
        match &mut self.value {
            Value::Bool(b) => *b = !*b,
            Value::Num { v, min, max, step } => {
                let mut n = if down { *v - *step } else { *v + *step };
                if n > *max + 1e-9 {
                    n = *min;
                } else if n < *min - 1e-9 {
                    n = *max;
                }
                *v = round_to(n, *step);
            }
            Value::Text(_) => {}
            Value::Pick { i, of } => {
                *i = if down {
                    (*i + of.len() - 1) % of.len()
                } else {
                    (*i + 1) % of.len()
                };
            }
        }
    }
    // From text (`.set`, the config): whether it was understood.
    fn parse(&mut self, text: &str) -> bool {
        let t = text.trim();
        match &mut self.value {
            Value::Bool(b) => match t.to_ascii_lowercase().as_str() {
                "true" | "on" | "yes" | "1" => *b = true,
                "false" | "off" | "no" | "0" => *b = false,
                "toggle" => *b = !*b,
                _ => return false,
            },
            Value::Num { v, min, max, .. } => match t.parse::<f64>() {
                Ok(n) if n.is_finite() => *v = n.clamp(*min, *max),
                _ => return false,
            },
            Value::Pick { i, of } => match of.iter().position(|o| norm(o) == norm(t)) {
                Some(p) => *i = p,
                None => return false,
            },
            Value::Text(v) => *v = t.to_owned(),
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Module {
    pub(crate) name: &'static str,
    pub(crate) category: Category,
    pub(crate) about: &'static str,
    pub(crate) on: bool,
    pub(crate) key: Option<String>,
    pub(crate) settings: Vec<Setting>,
}

impl Module {
    pub(super) fn new(name: &'static str, category: Category, about: &'static str) -> Self {
        Self {
            name,
            category,
            about,
            on: false,
            key: None,
            settings: Vec::new(),
        }
    }
    pub(super) fn on(mut self) -> Self {
        self.on = true;
        self
    }
    pub(super) fn bool(mut self, name: &'static str, v: bool) -> Self {
        self.settings.push(Setting {
            name,
            value: Value::Bool(v),
        });
        self
    }
    pub(super) fn num(mut self, name: &'static str, v: f64, min: f64, max: f64, step: f64) -> Self {
        self.settings.push(Setting {
            name,
            value: Value::Num { v, min, max, step },
        });
        self
    }
    pub(super) fn text(mut self, name: &'static str, default: &str) -> Self {
        self.settings.push(Setting {
            name,
            value: Value::Text(default.to_owned()),
        });
        self
    }
    pub(super) fn pick(mut self, name: &'static str, of: &'static [&'static str]) -> Self {
        self.settings.push(Setting {
            name,
            value: Value::Pick { i: 0, of },
        });
        self
    }
    pub(super) fn setting(&self, name: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.name == name)
    }
    pub(super) fn b(&self, name: &str) -> bool {
        matches!(self.setting(name), Some(Setting { value: Value::Bool(true), .. }))
    }
    pub(super) fn n(&self, name: &str) -> f64 {
        match self.setting(name) {
            Some(Setting { value: Value::Num { v, .. }, .. }) => *v,
            _ => 0.0,
        }
    }
    // A list setting's text.
    pub(super) fn t(&self, name: &str) -> &str {
        match self.setting(name) {
            Some(Setting { value: Value::Text(t), .. }) => t,
            _ => "",
        }
    }
    pub(super) fn p(&self, name: &str) -> &'static str {
        match self.setting(name) {
            Some(Setting { value: Value::Pick { i, of }, .. }) => of[*i],
            _ => "",
        }
    }
    // What the module list shows beside the name (as Meteor's info string).
    pub(super) fn info(&self) -> Option<String> {
        match self.name {
            "Flight" => Some(self.p("Mode").to_owned()),
            "Kill Aura" => Some(num(self.n("Range"))),
            "Fast Place" => Some(num(self.n("Speed"))),
            "Speed" | "Criticals" => Some(self.p("Mode").to_owned()),
            _ => None,
        }
    }

    fn setting_visible(&self, j: usize) -> bool {
        let name = self.settings[j].name;
        let advanced = self.b("Advanced");
        match self.name {
            "Speed" => match name {
                "Vanilla speed" => self.p("Mode") != "Strafe",
                "Strafe speed" | "Speed limit" => self.p("Mode") == "Strafe",
                "Ground multiplier" => advanced && self.p("Mode") == "Custom",
                "Air multiplier" => advanced && matches!(self.p("Mode"), "Smooth" | "Low Hop" | "Custom") && !self.b("Only on ground"),
                "Acceleration" | "Deceleration" | "Strafe strength" => advanced && matches!(self.p("Mode"), "Smooth" | "Custom"),
                "Jump height" => advanced && self.p("Mode") == "Low Hop",
                "Max speed" | "Sprint multiplier" => advanced && matches!(self.p("Mode"), "Smooth" | "Low Hop" | "Custom"),
                "Knockback pause" | "Stop at wall" => advanced,
                _ => true,
            },
            "Velocity" => match name {
                "Use air values" => advanced,
                "Air knockback horizontal" | "Air knockback vertical" => advanced && self.b("Use air values"),
                "Blend %" => self.p("Mode") == "Blend",
                _ => true,
            },
            "Scaffold" => match name {
                "Safe walk" | "Validate ray" | "Max placement distance" | "Duplicate cooldown" => advanced,
                "Rotation speed" => advanced && self.b("Rotate"),
                _ => true,
            },
            _ => true,
        }
    }
}

// Every module with its defaults, in menu order.
pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    let mut v = vec![
        // Combat
        Module::new("Trigger Bot", Combat, "Attacks what your crosshair is on.")
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", false)
            .bool("Anti bot", false)
            .bool("Ignore teams", false)
            .bool("Wait for cooldown", true)
            .num("Delay (ticks)", 10.0, 0.0, 20.0, 1.0),
        Module::new("Criticals", Combat, "Makes your hits critical hits. Packet hops off the ground over two ticks before the hit; Jump really jumps and Mini jump hops, and the hit comes on the way down.")
            .pick("Mode", &["Packet", "Jump", "Mini jump"])
            .bool("Only Kill Aura", false)
            .num("Additional height", 0.0, 0.0, 1.0, 0.05)
            .bool("Mace smash", true)
            .bool("Only with a weapon", false)
            .bool("Only on ground", true),
        Module::new("Auto Totem", Combat, "Moves a totem from your inventory to your offhand.")
            .pick("Mode", &["Smart", "Strict"])
            .num("Delay (ticks)", 0.0, 0.0, 20.0, 1.0)
            .num("Health", 10.0, 0.0, 36.0, 1.0)
            .bool("Elytra", true)
            .bool("Fall", true)
            .bool("Explosion", true),
        // Movement
        Module::new("Sprint", Movement, "Sprints for you whenever you walk forward."),
        Module::new("Auto Walk", Movement, "Walks forward for you."),
        Module::new("Flight", Movement, "Lets you fly. Your packets claim the ground while it is on, so landing never hurts.")
            .pick("Mode", &["Abilities", "Velocity"])
            .num("Speed", 0.1, 0.0, 5.0, 0.01)
            .bool("Vertical speed match", false)
            .bool("No sneak", false)
            .pick("Anti kick mode", &["Packet", "Normal", "None"])
            .num("Delay", 20.0, 1.0, 200.0, 1.0)
            .num("Off time", 1.0, 1.0, 20.0, 1.0),
        Module::new("Speed", Movement, "Modifies your movement speed when moving on the ground.")
            .pick("Mode", &["Vanilla", "Strafe", "Smooth", "Low Hop", "Custom"])
            .num("Vanilla speed", 5.6, 0.0, 20.0, 0.1)
            .num("Strafe speed", 1.6, 0.0, 3.0, 0.05)
            .bool("Speed limit", false)
            .num("Timer", 1.0, 0.01, 10.0, 0.01)
            .bool("In liquids", false)
            .bool("When sneaking", false)
            .bool("Only on ground", false)
            .bool("Advanced", false)
            .num("Ground multiplier", 1.0, 0.1, 2.0, 0.05)
            .num("Air multiplier", 0.8, 0.1, 2.0, 0.05)
            .num("Acceleration", 0.08, 0.005, 0.5, 0.005)
            .num("Deceleration", 0.06, 0.005, 0.5, 0.005)
            .num("Strafe strength", 0.7, 0.0, 1.0, 0.05)
            .num("Jump height", 0.30, 0.2, 0.42, 0.01)
            .num("Max speed", 1.5, 0.1, 2.0, 0.05)
            .num("Knockback pause", 3.0, 0.0, 10.0, 1.0)
            .num("Sprint multiplier", 1.0, 0.5, 1.5, 0.05)
            .bool("Stop at wall", false),
        Module::new("No Fall", Movement, "Stops fall damage (Meteor's No Fall). Packet tells the server you are on the ground as you fall; No ground never does; Air place puts a block under you; Place puts water (or the Placed item) where you land and picks it back up.")
            .pick("Mode", &["Packet", "No ground", "Air place", "Place"])
            .pick("Placed item", &["Bucket", "Powder snow", "Hay bale", "Cobweb", "Slime block"])
            .pick("Air place mode", &["Before death", "Before damage"])
            .bool("Rotate", true)
            .bool("Anchor", true)
            .bool("Pause on mace", true),
        Module::new("Velocity", Movement, "Prevents you from being moved by external forces.")
            .pick("Mode", &["Scale", "Blend"])
            .bool("Knockback", true)
            .num("Knockback horizontal", 0.0, 0.0, 1.0, 0.05)
            .num("Knockback vertical", 0.0, 0.0, 1.0, 0.05)
            .bool("Explosions", true)
            .num("Explosions horizontal", 0.0, 0.0, 1.0, 0.05)
            .num("Explosions vertical", 0.0, 0.0, 1.0, 0.05)
            .bool("Liquids", true)
            .num("Liquids horizontal", 0.0, 0.0, 1.0, 0.05)
            .num("Liquids vertical", 0.0, 0.0, 1.0, 0.05)
            .bool("Entity push", true)
            .num("Entity push amount", 0.0, 0.0, 1.0, 0.05)
            .bool("Sinking", false)
            .bool("Advanced", false)
            .bool("Use air values", false)
            .num("Air knockback horizontal", 1.0, 0.0, 1.0, 0.05)
            .num("Air knockback vertical", 1.0, 0.0, 1.0, 0.05)
            .num("Blend %", 50.0, 0.0, 100.0, 5.0),
        Module::new("Spider", Movement, "Climbs walls.")
            .num("Climb speed", 0.2, 0.1, 1.0, 0.1),
        Module::new("Step", Movement, "Walks up full blocks and more without jumping (step up).")
            .num("Height", 1.25, 0.5, 5.0, 0.25)
            .pick("Active when", &["Always", "Sneaking", "Not sneaking"])
            .bool("Safe step", false)
            .num("Step health", 5.0, 1.0, 36.0, 1.0),
        Module::new("GUI Move", Movement, "Lets you walk, jump and look around while a menu is open.")
            .pick("Screens", &["All menus", "Inventory"])
            .bool("Jump", true)
            .bool("Sneak", true)
            .bool("Arrows", true)
            .bool("Sprint", true)
            .num("Ground speed %", 100.0, 20.0, 150.0, 5.0)
            .num("Air speed %", 100.0, 20.0, 150.0, 5.0),
        Module::new("Glide", Movement, "Falls slowly, and never takes fall damage while it does.")
            .num("Fall speed", 2.5, 0.5, 8.0, 0.5),
        Module::new("No Jump Delay", Movement, "Jumps again the moment you land when you hold the jump key."),
        Module::new("Air Jump", Movement, "Jumps again in mid-air."),
        Module::new("Elytra Boost", Movement, "Speeds up gliding while you hold forward.")
            .num("Boost", 0.05, 0.01, 0.2, 0.01)
            .num("Max speed", 2.0, 0.5, 5.0, 0.5),
        Module::new("Anti Void", Movement, "Bounces you out of the void."),
        // Player
        Module::new("Fast Place", Player, "Places blocks faster while you hold use. Speed 1 is 6 blocks a second, 2 is 8, 3 is 10, 4 is 14 and 5 is 20 (the game's own is 5).")
            .num("Speed", 5.0, 1.0, 5.0, 1.0),
        Module::new("Break Delay", Player, "Changes the delay between breaking one block and the next (Meteor's Break Delay): 0 means none.")
            .num("Cooldown (ticks)", 0.0, 0.0, 5.0, 1.0),
        Module::new("Auto Respawn", Player, "Respawns as soon as you die."),
        Module::new("Panic", Player, "Turns every module off at once. Bind it to a key."),
        // Render
        Module::new("ClickGUI", Render, "How this menu looks: accent colour, size, panel width, corner radius, text indent, background opacity and a dim behind it. Menu dim is the game's own dim and blur behind the inventory and the Esc menu.")
            .colour("Accent color", "Gold")
            .num("GUI scale", 1.0, 0.75, 2.0, 0.05)
            .num("Panel width", 100.0, 60.0, 140.0, 2.0)
            .num("Panel radius", 6.0, 0.0, 12.0, 1.0)
            .num("Control radius", 4.0, 0.0, 6.0, 1.0)
            .pick("Spacing", &["Comfortable", "Compact"])
            .num("Text indent", 6.0, 0.0, 12.0, 1.0)
            .pick("Settings", &["Window", "Inline"])
            .num("Background opacity", 0.9, 0.2, 1.0, 0.05)
            .num("Background dim", 20.0, 0.0, 100.0, 5.0)
            .bool("Menu dim", false),
        Module::new("HUD", Render, "The watermark, the active modules, your coordinates and more.")
            .on()
            .pick("Font", &horizon_client::font::sunset_fontdata::LABELS)
            .bool("Watermark", true)
            .bool("Module list", true)
            .bool("Coordinates", true)
            .bool("FPS", true)
            .bool("Speed", false)
            .bool("Facing", true)
            .bool("Totems", false)
            .bool("Target HUD", true)
            .pick("Target HUD mode", &["Full", "Potato"]),
        Module::new("Fullbright", Render, "Lights everything up."),
        Module::new("ESP", Render, "Draws boxes around entities, through walls. Outline and Potato (a dot on each) are the cheapest.")
            .pick("Mode", &["3D box", "2D box", "Outline", "Potato"])
            .pick("Shape mode", &["Lines", "Sides", "Both"])
            .num("Fill opacity", 0.3, 0.0, 1.0, 0.05)
            .num("Fade distance", 3.0, 0.0, 12.0, 0.5)
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", true)
            .bool("Items", false)
            .bool("Show friend colors", true)
            .colour("Players color", "White")
            .colour("Hostiles color", "Red")
            .colour("Passive color", "Green")
            .colour("Items color", "Orange")
            .num("Range", 64.0, 8.0, 256.0, 8.0)
            .num("Max", 256.0, 8.0, 1024.0, 8.0),
        Module::new("Tracers", Render, "Draws lines from your crosshair to entities. Potato draws the lines alone, to the middle.")
            .pick("Mode", &["Lines", "Potato"])
            .pick("Target", &["Body", "Head", "Feet"])
            .pick("Start", &["Crosshair", "Bottom"])
            .bool("Distance colors", false)
            .bool("Players", true)
            .bool("Hostiles", false)
            .bool("Passive", false)
            .bool("Show friend colors", true)
            .colour("Players color", "White")
            .colour("Hostiles color", "Red")
            .colour("Passive color", "Green")
            .num("Range", 64.0, 8.0, 256.0, 8.0)
            .num("Max", 256.0, 8.0, 1024.0, 8.0),
        Module::new("Nametags", Render, "Big name tags with health and distance, through walls. Potato shows the name alone.")
            .pick("Mode", &["Full", "Potato"])
            .bool("Players", true)
            .bool("Mobs", false)
            .bool("Ignore friends", false)
            .bool("Health", true)
            .bool("Distance", true)
            .bool("Background", true)
            .colour("Name color", "White")
            .num("Range", 64.0, 8.0, 256.0, 8.0)
            .num("Max", 64.0, 1.0, 512.0, 1.0),
        Module::new("Storage ESP", Render, "Draws boxes around chests and other storage. Outline and Potato (a dot on each) are the cheapest.")
            .pick("Mode", &["3D box", "2D box", "Outline", "Potato"])
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.16, 0.0, 1.0, 0.02)
            .num("Fade distance", 3.0, 0.0, 12.0, 0.5)
            .bool("Chests", true)
            .bool("Ender chests", true)
            .bool("Barrels", true)
            .bool("Shulkers", true)
            .bool("Other", true)
            .num("Range", 64.0, 8.0, 128.0, 8.0)
            .num("Max", 256.0, 8.0, 1024.0, 8.0),
        // Added in 0.3
        Module::new("Auto Weapon", Combat, "Switches to your best weapon before Kill Aura or Trigger Bot hits.")
            .bool("Prefer axes", false),
        Module::new("Auto Log", Combat, "Leaves the server when your health is low.")
            .num("Health", 6.0, 1.0, 19.0, 1.0),
        Module::new("Sneak", Movement, "Sneaks for you."),
        Module::new("Safe Walk", Movement, "Sneaks at the edge of a drop, so you don't walk off it."),
        Module::new("Fast Climb", Movement, "Climbs ladders and vines faster.")
            .num("Speed", 0.3, 0.1, 1.0, 0.05),
        Module::new("High Jump", Movement, "Jumps higher.")
            .num("Multiplier", 2.0, 1.0, 5.0, 0.5),
        Module::new("Parkour", Movement, "Jumps for you at the edge of a block, on the last tick you still stand on it.")
            .num("Edge distance", 0.001, 0.001, 0.1, 0.001)
            .pick("Jump when", &["Last tick", "Early"])
            .bool("Only while sprinting", false)
            .bool("Only when moving forward", false)
            .bool("Ignore sneaking", false),
        Module::new("Blink", Movement, "Holds back your movement packets, then sends them all at once when off.")
            .num("Max ticks", 0.0, 0.0, 200.0, 10.0),
        Module::new("Scaffold", Movement, "Automatically places blocks under you.")
            .text("Blocks", "")
            .pick("Blocks filter", &["Blacklist", "Whitelist"])
            .bool("Fast tower", false)
            .num("Tower speed", 0.5, 0.0, 1.0, 0.05)
            .bool("While moving", false)
            .bool("Only on click", false)
            .bool("Auto switch", true)
            .bool("Rotate", true)
            .bool("Air place", false)
            .num("Ahead distance", 0.0, 0.0, 1.0, 0.05)
            .num("Closest block range", 4.0, 0.0, 8.0, 0.5)
            .num("Radius", 0.0, 0.0, 6.0, 0.5)
            .num("Blocks per tick", 3.0, 1.0, 10.0, 1.0)
            .bool("Render", true)
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.04, 0.0, 1.0, 0.02)
            .colour("Color", "Purple")
            .num("Delay (ticks)", 0.0, 0.0, 10.0, 1.0)
            .bool("Advanced", false)
            .bool("Safe walk", false)
            .bool("Validate ray", false)
            .num("Max placement distance", 6.0, 2.0, 6.0, 0.25)
            .num("Duplicate cooldown", 0.0, 0.0, 8.0, 1.0)
            .num("Rotation speed", 180.0, 5.0, 180.0, 5.0),
        Module::new("Anti Hunger", Player, "Spends less food by hiding that you are sprinting and on the ground.")
            .bool("Sprint", true)
            .bool("Ground", true),
        Module::new("Anti AFK", Player, "Performs different actions to prevent getting kicked while AFK.")
            .bool("Jump", true)
            .bool("Swing", false)
            .bool("Sneak", false)
            .num("Sneak time", 5.0, 1.0, 100.0, 1.0)
            .bool("Strafe", false)
            .bool("Spin", true)
            .pick("Spin mode", &["Server", "Client"])
            .num("Speed", 7.0, 1.0, 30.0, 1.0)
            .num("Pitch", 0.0, -90.0, 90.0, 5.0)
            .bool("Send messages", false)
            .bool("Random", false)
            .num("Delay (seconds)", 15.0, 1.0, 120.0, 1.0)
            .text("Messages", "Sunset on top!|Sunset on crack!"),
        Module::new("Auto Clicker", Player, "Clicks for you while you hold a mouse button.")
            .bool("Left", true)
            .bool("Right", false)
            .num("Delay (ticks)", 4.0, 1.0, 20.0, 1.0),
        Module::new("Auto Eat", Player, "Eats food when your hunger or health gets low (Meteor's Auto Eat).")
            .text("Blacklist", "enchanted_golden_apple,golden_apple,chorus_fruit,poisonous_potato,pufferfish,chicken,rotten_flesh,spider_eye,suspicious_stew")
            .bool("Pause auras", true)
            .bool("Search inventory", false)
            .pick("Food priority", &["Saturation", "Combined", "Hunger"])
            .pick("Threshold mode", &["Any", "Health", "Hunger", "Both"])
            .num("Health threshold", 10.0, 0.0, 19.0, 1.0)
            .num("Hunger threshold", 16.0, 1.0, 19.0, 1.0),
        Module::new("Zoom", Render, "Zooms in.")
            .num("Factor", 4.0, 1.5, 10.0, 0.5)
            .bool("Smooth", true),
        Module::new("Breadcrumbs", Render, "Draws a line along where you have been. Potato: a dot on each point.")
            .pick("Mode", &["Line", "Potato"])
            .num("Length", 300.0, 20.0, 1000.0, 20.0),
        Module::new("Block ESP", Render, "Draws boxes around ores and other blocks, through walls. Outline and Potato (a dot on each) are the cheapest.")
            .pick("Mode", &["Box", "Outline", "Potato"])
            .pick("Blocks", &["Ores", "Diamond", "Ancient Debris", "Spawners", "Portals"])
            .num("Range", 32.0, 8.0, 48.0, 4.0)
            .bool("Fill", true)
            .num("Max", 1000.0, 50.0, 4000.0, 50.0),
        Module::new("Waypoints", Render, "Shows the places saved with .waypoint, on screen. Potato: a dot, no name.")
            .pick("Mode", &["Full", "Potato"])
            .num("Range", 1000.0, 50.0, 10000.0, 50.0),
        // Added in 0.4
        Module::new("Auto Armor", Combat, "Wears the best armor in your inventory.")
            .num("Delay (ticks)", 10.0, 2.0, 40.0, 2.0),
        Module::new("Reverse Step", Movement, "Falls fast when you step off a block.")
            .num("Speed", 1.0, 0.5, 3.0, 0.5),
        Module::new("Long Jump", Movement, "Jumps further.")
            .num("Multiplier", 2.0, 1.5, 5.0, 0.5),
        Module::new("Auto Jump", Movement, "Jumps for you whenever you walk."),
        Module::new("Chest Swap", Movement, "Swaps your chestplate and elytra, once, then turns itself off."),
        Module::new("Auto Replenish", Player, "Refills hotbar stacks from your inventory when they run low.")
            .num("Threshold", 8.0, 1.0, 32.0, 1.0)
            .num("Delay (ticks)", 10.0, 2.0, 40.0, 2.0),
        Module::new("Auto Tool", Player, "Switches to the hotbar tool that breaks the block you mine best (Meteor's Auto Tool).")
            .pick("Prefer", &["Fortune", "None", "Silk touch"])
            .bool("Silk touch for ender chest", true)
            .bool("Fortune for ores and crops", false)
            .bool("Anti break", false)
            .num("Anti break percentage", 10.0, 1.0, 100.0, 1.0)
            .bool("Switch back", false)
            .num("Switch delay", 0.0, 0.0, 20.0, 1.0)
            .pick("List mode", &["Blacklist", "Whitelist"])
            .text("Whitelist", "")
            .text("Blacklist", ""),
        Module::new("Middle Click Extra", Player, "Middle click uses an item from your hotbar, then switches back.")
            .pick("Item", &["Pearl", "Rocket", "XP bottle"]),
        Module::new("Trajectories", Render, "Draws where a bow shot or a throw will land. Potato: a dot where it lands.")
            .pick("Mode", &["Line", "Potato"]),
        Module::new("Logout Spots", Render, "Marks where players were when they logged out. Potato: a dot, no name.")
            .pick("Mode", &["Box", "Outline", "Potato"])
            .num("Minutes", 10.0, 1.0, 60.0, 1.0),
        Module::new("Bow Spam", Combat, "Fires arrows as fast as you can draw them while you hold use.")
            .num("Charge (ticks)", 5.0, 3.0, 20.0, 1.0),
        Module::new("Notifier", Player, "Says in chat when a player joins or leaves the server.")
            .bool("Join", true)
            .bool("Leave", true),
        Module::new("Weather Changer", World, "Sets the weather you see.")
            .pick("Weather", &["Clear", "Rain", "Thunder"]),
        Module::new("Hole ESP", Render, "Marks holes walled with bedrock or obsidian, where explosions hurt less (Meteor's Hole ESP).")
            .pick("Mode", &["3D box", "2D box", "Outline", "Potato"])
            .num("Horizontal radius", 10.0, 1.0, 32.0, 1.0)
            .num("Vertical radius", 5.0, 1.0, 16.0, 1.0)
            .num("Min height", 3.0, 1.0, 5.0, 1.0)
            .bool("Doubles", true)
            .bool("Ignore own", false)
            .bool("Webs", false)
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.25, 0.0, 1.0, 0.05)
            .num("Height", 0.2, 0.0, 1.0, 0.05)
            .colour("Bedrock color", "Lime")
            .colour("Obsidian color", "Red")
            .colour("Mixed color", "Orange")
            .num("Max", 200.0, 10.0, 1000.0, 10.0),
        Module::new("Nuker", World, "Breaks blocks around you.")
            .pick("Shape", &["Sphere", "Cube", "Uniform cube"])
            .pick("Mode", &["Flatten", "All", "Smash"])
            .num("Range", 4.0, 0.0, 6.0, 0.5)
            .num("Up", 1.0, 0.0, 6.0, 1.0)
            .num("Down", 1.0, 0.0, 6.0, 1.0)
            .num("Left", 1.0, 0.0, 6.0, 1.0)
            .num("Right", 1.0, 0.0, 6.0, 1.0)
            .num("Forward", 1.0, 0.0, 6.0, 1.0)
            .num("Back", 1.0, 0.0, 6.0, 1.0)
            .num("Walls range", 4.0, 0.0, 6.0, 0.5)
            .num("Delay", 0.0, 0.0, 20.0, 1.0)
            // A block is one packet (START), two with Packet mine (STOP), with a swing and the look each tick: at 6 a
            // tick that is at most 280 a second, well under Paper's limit of 500 (24 sent about 560 and got kicked).
            .num("Max blocks per tick", 6.0, 1.0, 256.0, 1.0)
            .pick("Sort mode", &["Closest", "None", "Furthest", "Top down"])
            .bool("Packet mine", false)
            .bool("Only suitable tools", false)
            .bool("Interact", false)
            .bool("Rotate", true)
            .pick("List mode", &["Blacklist", "Whitelist"])
            .text("Blacklist", "")
            .text("Whitelist", "")
            .bool("Swing", true)
            .bool("Instant only", false)
            .bool("Bounding box", true)
            .bool("Broken blocks", true)
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.3, 0.0, 1.0, 0.05)
            .colour("Box color", "Cyan")
            .colour("Broken color", "Red"),
        Module::new("Time Changer", World, "Sets the time of day you see.")
            .num("Time", 6000.0, 0.0, 24000.0, 1000.0),
    ];
    v.splice(0..0, super::sunset_aura::defaults());
    v.extend(super::sunset_combat::defaults());
    v.extend(super::sunset_render::defaults());
    v.extend(super::sunset_radar::defaults());
    v.extend(super::sunset_player::defaults());
    v.extend(super::sunset_world::defaults());
    // Modules that only draw can be held on with their bind instead of toggled.
    for m in v.iter_mut().filter(|m| holds(m.name)) {
        m.settings.push(Setting { name: "Bind mode", value: Value::Pick { i: 0, of: &["Toggle", "Hold"] } });
    }
    v
}

// ---------------------------------------------------------------- state

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Page {
    Main,
    Category(Category),
    Module(usize),
}

pub(super) struct State {
    pub(crate) modules: Vec<Module>,
    page: Page,
    // A module waiting for its key bind (its index).
    binding: Option<usize>,
    // Keys and buttons down, and the modules Hold binds have on.
    inputs: Inputs,
    pub(super) friends: Vec<String>,
    // The config file as read: modules a plugin registers later take their saved settings from it.
    raw: String,
    // Keys this mod held down for the tick (Auto Walk, Sprint).
    injected: Vec<KeyCode>,
    loaded: bool,
    ticks: u64,
    trigger_wait: u32,
    respawn_wait: u32,
    // Flight's Abilities mode is holding the player up.
    flying: bool,
    // The Step modifier is on the player.
    stepping: bool,
    high_jumping: bool,
    // Auto Totem moved a totem from this hotbar slot and should swap back.
    // The player's velocity when the last tick ended: what changes it before
    // the next one is the server's push (Velocity).
    velocity: Option<[f64; 3]>,
    strafe: Strafe,
    knockback_ticks: u32,
    // Flight: Meteor's anti kick counters, the yaw it keeps, which way it nudged last, and whether it let go of sneak.
    fly_delay: i32,
    fly_off: i32,
    fly_last_yaw: f32,
    fly_flip: bool,
    unsneaked: bool,
    // What explosions pushed the player by since the last tick.
    blast: [f64; 3],
    gui: Gui,
    crumbs: Vec<[f64; 3]>,
    waypoints: Vec<Waypoint>,
    // Block ESP's finds: where, and which colour.
    blocks: Sweep<u32>,
    scaffold_wait: u32,
    scaffold_recent: Vec<([i32; 3], u64)>,
    scaffold_rotation: Option<(f32, f32)>,
    // Scaffold's placed blocks, drawn for 8 ticks as they fade.
    scaffold_placed: Vec<([i32; 3], u8)>,
    click_wait: u32,
    blink_ticks: u32,
    eating: bool,
    // Auto Eat: the slot it eats from (None: the off hand), the one held before, and Kill Aura turned off meanwhile.
    eat_slot: Option<usize>,
    eat_prev: usize,
    eat_paused_aura: bool,
    // Auto Tool's switch waiting out Switch delay: the slot and the ticks left.
    tool_wait: Option<(usize, u32)>,
    was_ground: bool,
    crumb_at: u64,
    // Criticals: who the waiting hit is for, the ticks it has waited, the ticks you have fallen meanwhile, and the
    // look it is made with.
    crit_target: Option<u32>,
    crit_ticks: u32,
    crit_falls: u32,
    crit_look: Option<(f32, f32)>,
    notify: Option<Vec<String>>,
    // Hole ESP's finds: where, and what walls them (0 bedrock, 1 obsidian, 2 both).
    holes: Sweep<u8>,
    tool_back: Option<usize>,
    mc_back: Option<(usize, u32)>,
    nuke: Option<([i32; 3], u32)>,
    // Blocks Nuker just stopped on and the tick to forget them at.
    nuke_skip: Vec<([i32; 3], u64)>,
    // Nuker (Meteor's): its delay, the first-block flag, the last block, the ticks with nothing to break, the blocks
    // it right-clicked (Interact), its Cube box and the blocks it broke (drawn for 8 ticks).
    nuke_timer: u32,
    nuke_first: bool,
    nuke_last: Option<[i32; 3]>,
    nuke_idle: u32,
    nuke_interacted: Vec<[i32; 3]>,
    nuke_box: Option<[[f64; 3]; 2]>,
    nuke_broken: Vec<([i32; 3], u8)>,
    armor_wait: u32,
    replenish_wait: u32,
    last_pos: Vec<(String, [f64; 3], String)>,
    spots: Vec<Spot>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Spot {
    name: String,
    pos: [f64; 3],
    dim: String,
    at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Waypoint {
    name: String,
    pos: [f64; 3],
    dim: String,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            modules: defaults(),
            page: Page::Main,
            binding: None,
            inputs: Inputs::default(),
            friends: Vec::new(),
            raw: String::new(),
            injected: Vec::new(),
            loaded: false,
            ticks: 0,
                    trigger_wait: 0,
            respawn_wait: 0,
            flying: false,
            stepping: false,
            high_jumping: false,
                    velocity: None,
                    strafe: Strafe::default(),
            knockback_ticks: 0,
                    fly_delay: 0,
                    fly_off: 0,
                    fly_last_yaw: 0.0,
                    fly_flip: false,
                    unsneaked: false,
                    blast: [0.0; 3],
            gui: Gui::default(),
            crumbs: Vec::new(),
            waypoints: Vec::new(),
            blocks: Sweep::new(),
                    scaffold_wait: 0,
            scaffold_recent: Vec::new(),
            scaffold_rotation: None,
                    scaffold_placed: Vec::new(),
            click_wait: 0,
            blink_ticks: 0,
            eating: false,
            eat_slot: None,
            eat_prev: 0,
            eat_paused_aura: false,
            tool_wait: None,
            was_ground: false,
            crumb_at: 0,
            crit_target: None,
            crit_ticks: 0,
            crit_falls: 0,
            crit_look: None,
            notify: None,
            holes: Sweep::new(),
            tool_back: None,
            mc_back: None,
            nuke: None,
            nuke_skip: Vec::new(),
            nuke_timer: 0,
            nuke_first: true,
            nuke_last: None,
            nuke_idle: 0,
            nuke_interacted: Vec::new(),
            nuke_box: None,
            nuke_broken: Vec::new(),
            armor_wait: 0,
            replenish_wait: 0,
            last_pos: Vec::new(),
            spots: Vec::new(),
        }
    }
    pub(super) fn find(&self, name: &str) -> Option<usize> {
        let n = norm(name);
        self.modules.iter().position(|m| norm(m.name) == n)
    }
    fn save_text(&self) -> String {
        let mut out = String::new();
        for (i, m) in self.modules.iter().enumerate() {
            let id = norm(m.name);
            out.push_str(&format!("{id}.on={}\n", self.inputs.saved_on(i, m.on)));
            out.push_str(&format!("{id}.key={}\n", m.key.as_deref().unwrap_or("none")));
            for s in &m.settings {
                out.push_str(&format!("{id}.{}={}\n", norm(s.name), s.stored()));
            }
        }
        out.push_str(&format!("friends={}\n", self.friends.join(",")));
        for w in &self.waypoints {
            out.push_str(&format!("waypoint={}|{}|{}|{}|{}\n", w.name, w.pos[0], w.pos[1], w.pos[2], w.dim));
        }
        for (c, p) in CATEGORIES.iter().zip(&self.gui.panels) {
            out.push_str(&format!("panel.{}={},{},{}\n", norm(c.label()), p.x, p.y, p.collapsed));
        }
        out
    }
    fn load_text(&mut self, text: &str) {
        self.waypoints.clear();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            if let Some(cat) = k.strip_prefix("panel.") {
                let parts: Vec<&str> = v.split(',').collect();
                if let (Some(i), [x, y, c]) = (CATEGORIES.iter().position(|c| norm(c.label()) == cat), parts.as_slice())
                    && let (Ok(x), Ok(y)) = (x.parse(), y.parse())
                {
                    self.gui.panels[i] = Panel { x, y, collapsed: *c == "true", scroll: 0.0 };
                }
                continue;
            }
            if k == "waypoint" {
                let f: Vec<&str> = v.split('|').collect();
                if let [name, x, y, z, dim] = f.as_slice()
                    && let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse())
                {
                    self.waypoints.push(Waypoint { name: (*name).to_owned(), pos: [x, y, z], dim: (*dim).to_owned() });
                }
                continue;
            }
            if k == "friends" {
                self.friends = v
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(str::to_owned)
                    .collect();
                continue;
            }
            self.set_field(k, v);
        }
    }
    // One "module.field=value" line of a config or a profile.
    fn set_field(&mut self, k: &str, v: &str) {
        let Some((id, field)) = k.split_once('.') else {
            return;
        };
        let Some(m) = self.modules.iter_mut().find(|m| norm(m.name) == id) else {
            return;
        };
        match field {
            "on" => m.on = v == "true",
            "key" => m.key = key_from_text(v),
            _ => {
                // Settings from the earlier local movement build used different names and percentage units.
                let migrated = match (id, field) {
                    ("speed", "speed") => Some(("vanillaspeed", v.to_owned())),
                    ("speed", "mode") if norm(v) == "ground" => Some(("mode", "Vanilla".to_owned())),
                    ("velocity", "horizontal") => v.parse::<f64>().ok().filter(|n| n.is_finite())
                        .map(|n| ("knockbackhorizontal", (n / 100.0).to_string())),
                    ("velocity", "vertical") => v.parse::<f64>().ok().filter(|n| n.is_finite())
                        .map(|n| ("knockbackvertical", (n / 100.0).to_string())),
                    ("velocity", "airhorizontal") => v.parse::<f64>().ok().filter(|n| n.is_finite())
                        .map(|n| ("airknockbackhorizontal", (n / 100.0).to_string())),
                    ("velocity", "airvertical") => v.parse::<f64>().ok().filter(|n| n.is_finite())
                        .map(|n| ("airknockbackvertical", (n / 100.0).to_string())),
                    ("scaffold", "rotation") => Some(("rotate", (norm(v) != "none").to_string())),
                    ("scaffold", "maxdistance") => Some(("maxplacementdistance", v.to_owned())),
                    _ => None,
                };
                if let Some((target, value)) = migrated {
                    if let Some(s) = m.settings.iter_mut().find(|s| norm(s.name) == target) { s.parse(&value); }
                    return;
                }
                // No Render's switches were called "Fog", "Water fog" and so on before 0.8.1.
                let legacy = format!("hide{field}");
                if let Some(s) = m.settings.iter_mut().find(|s| norm(s.name) == field || (id == "norender" && norm(s.name) == legacy)) {
                    s.parse(v);
                }
            }
        }
    }
    // The modules' states, settings and keys: a profile.
    fn profile_text(&self) -> String {
        self.save_text().lines().filter(|l| !l.starts_with("panel.") && !l.starts_with("waypoint=") && !l.starts_with("friends=")).collect::<Vec<_>>().join("\n")
    }
    fn load_profile(&mut self, text: &str) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                self.set_field(k, v.trim());
            }
        }
    }
    // A profile file read whole before anything changes: its header, then lines this version knows, with values that
    // fit them. The lines, without the header.
    fn check_profile<'a>(&self, text: &'a str) -> Result<&'a str, String> {
        let body = text.strip_prefix(super::sunset_profiles::HEADER).and_then(|t| t.strip_prefix('\n')).ok_or("not a Sunset profile")?;
        for (n, line) in body.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
            let bad = |what: &str| format!("line {}: {what}", n + 2);
            let (k, v) = line.split_once('=').ok_or_else(|| bad("no ="))?;
            let (id, field) = k.split_once('.').ok_or_else(|| bad("no module"))?;
            let m = self.modules.iter().find(|m| norm(m.name) == id).ok_or_else(|| bad(&format!("no module called {id}")))?;
            let fits = match field {
                "on" => matches!(v.trim(), "true" | "false"),
                "key" => !v.chars().any(char::is_control),
                _ => m.settings.iter().find(|s| norm(s.name) == field).is_some_and(|s| s.clone().parse(v)),
            };
            if !fits {
                return Err(bad(&format!("{k} can't be {}", v.chars().take(40).collect::<String>())));
            }
        }
        Ok(body)
    }
    pub(super) fn module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }
    pub(super) fn is_on(&self, name: &str) -> bool {
        self.module(name).is_some_and(|m| m.on)
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

// The packet hooks read these: they run inside the game's own sends, which
// may happen while a module holds the state.
// No Fall's move packets: 0 leaves them, 1 claims the ground while falling (Packet), 2 never does (No ground); and
// whether holding a mace pauses it.
static NO_FALL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static NO_FALL_MACE: AtomicBool = AtomicBool::new(true);
static BLINK: AtomicBool = AtomicBool::new(false);
// Flight's position packets claim the ground, so nothing counts as a fall; Glide's too while you are not flying.
static FLIGHT_GROUND: AtomicBool = AtomicBool::new(false);
static GLIDE_GROUND: AtomicBool = AtomicBool::new(false);
// Flight's Packet anti kick: every move packet carries the position, and while FLY_DOWN the height sent sinks a
// little below the last one sent (FLY_LAST_Y, f64 bits).
static FLY_PACKET: AtomicBool = AtomicBool::new(false);
static FLY_DOWN: AtomicBool = AtomicBool::new(false);
static FLY_LAST_Y: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
static ANTI_SPRINT: AtomicBool = AtomicBool::new(false);
static ANTI_GROUND: AtomicBool = AtomicBool::new(false);
static WELCOMED: AtomicBool = AtomicBool::new(false);
// The server takes one position packet per tick and disconnects a client that
// sends more ("Invalid move player packet received"). Criticals hops over two
// ticks instead: stage 1 and 2 turn the next position packets into the hop up
// (CRIT_HOP blocks, f64 bits) and the hop back (both off the ground), stage 3
// lets the hit follow. Stage 4 waits for you to fall (Jump, Mini jump, a hit in
// the air), and CRIT_JUMP presses jump for the next tick.
pub(super) static CRIT_STAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static CRIT_HOP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CRIT_JUMP: AtomicBool = AtomicBool::new(false);
// A move packet went out this tick (the game's or Sunset's): Sunset never sends a second.
static MOVED_THIS_TICK: AtomicBool = AtomicBool::new(false);
// Blink held at least one move packet.
static BLINK_HELD: AtomicBool = AtomicBool::new(false);
// Scaffold's block placements count up from here (the game's own are small).

pub(super) fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::new))
}

// A module from a plugin: added once (by name), with its saved settings.
pub(super) fn register_module(m: Module) {
    with(|s| {
        if s.modules.iter().any(|x| x.name == m.name) {
            return;
        }
        s.modules.push(m);
        let raw = s.raw.clone();
        s.load_text(&raw);
        sync_flags(s);
    });
}

pub(super) fn snapshot() -> Vec<Module> {
    with(|s| s.modules.clone())
}

pub(super) fn sync_flags(s: &State) {
    let g = s.module("ClickGUI");
    LOOK[0].store(g.map_or(0xFFB020, |m| m.c("Accent color") & 0x00FF_FFFF), Ordering::Relaxed);
    LOOK[1].store(g.map_or(100.0, |m| m.n("Panel width") as f32).to_bits(), Ordering::Relaxed);
    LOOK[2].store(g.map_or(0.9, |m| m.n("Background opacity") as f32).to_bits(), Ordering::Relaxed);
    LOOK[3].store(g.map_or(20.0, |m| m.n("Background dim")) as u32, Ordering::Relaxed);
    LOOK[5].store(g.map_or(6.0, |m| m.n("Text indent") as f32).to_bits(), Ordering::Relaxed);
    LOOK[6].store(g.map_or(1.0, |m| m.n("GUI scale") as f32).to_bits(), Ordering::Relaxed);
    LOOK[8].store(g.map_or(6.0, |m| m.n("Panel radius") as f32).to_bits(), Ordering::Relaxed);
    LOOK[9].store(g.map_or(4.0, |m| m.n("Control radius") as f32).to_bits(), Ordering::Relaxed);
    LOOK[10].store(u32::from(g.is_some_and(|m| m.p("Spacing") == "Compact")), Ordering::Relaxed);
    let no_fall = s.module("No Fall").filter(|m| m.on);
    NO_FALL.store(no_fall.map_or(0, |m| match m.p("Mode") {
        "Packet" => 1,
        "No ground" => 2,
        _ => 0,
    }), Ordering::Relaxed);
    NO_FALL_MACE.store(no_fall.is_some_and(|m| m.b("Pause on mace")), Ordering::Relaxed);
    let flight = s.module("Flight").filter(|m| m.on);
    FLIGHT_GROUND.store(flight.is_some(), Ordering::Relaxed);
    GLIDE_GROUND.store(s.is_on("Glide"), Ordering::Relaxed);
    FLY_PACKET.store(flight.is_some_and(|m| m.p("Anti kick mode") == "Packet"), Ordering::Relaxed);
    BLINK.store(s.is_on("Blink"), Ordering::Relaxed);
    let hunger = s.module("Anti Hunger").filter(|m| m.on);
    ANTI_SPRINT.store(hunger.is_some_and(|m| m.b("Sprint")), Ordering::Relaxed);
    ANTI_GROUND.store(hunger.is_some_and(|m| m.b("Ground")), Ordering::Relaxed);
}

// Sunset was Solstice before 0.8.1: with no settings of its own yet, it takes over the old mod's (the config, the profiles).
fn take_over_old_settings(game: &Game<'_>) {
    if game.native_setting(ID, "config").is_some() || game.native_setting("solstice", "config").is_none() {
        return;
    }
    for file in std::fs::read_dir(game.native_data_dir("solstice")).into_iter().flatten().flatten() {
        if let Some(key) = file.file_name().to_str()
            && let Some(value) = game.native_setting("solstice", key)
        {
            game.native_set_setting(ID, key, &value);
        }
    }
}

pub(super) fn ensure_loaded(game: &Game<'_>) {
    if with(|s| s.loaded) {
        return;
    }
    take_over_old_settings(game);
    let text = game.native_setting(ID, "config").unwrap_or_default();
    with(|s| {
        s.raw = text.clone();
        s.load_text(&text);
        s.loaded = true;
        sync_flags(s);
    });
}

pub(super) fn save(game: &Game<'_>) {
    let text = with(|s| {
        release_changed_binds(s);
        sync_flags(s);
        s.save_text()
    });
    game.native_set_setting(ID, "config", &text);
}

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

// A line in chat: Sunset's tag, then the message.
pub(super) fn say(game: &mut Game<'_>, msg: &str) {
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
    let held = |k: KeyCode| f64::from(u8::from(game.keys.contains(&k)));
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

pub(super) fn wanted(m: &Module, kind: Kind) -> bool {
    match kind {
        Kind::Player => m.b("Players"),
        Kind::Hostile => m.b("Hostiles"),
        Kind::Passive => m.b("Passive"),
    }
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

// ---------------------------------------------------------------- the menu

pub(super) fn listed(mods: &mut Vec<super::native_hooks::NativeMod>) {
    mods.push(super::native_hooks::NativeMod {
        id: ID.into(),
        title: format!("{NAME} {VERSION}"),
        description: DESCRIPTION.into(),
        keys: vec![
            [MENU_KEY.into(), "Open Sunset menu".into(), MENU_DEFAULT.into()],
            [MENU2_KEY.into(), "Open Sunset menu (2nd key)".into(), MENU2_DEFAULT.into()],
        ],
    });
}

pub(super) fn mod_index(game: &Game<'_>) -> Option<usize> {
    game.settings.mods.installed.iter().position(|m| m.id == ID)
}

pub(super) fn refresh(game: &mut Game<'_>) {
    if let Some(i) = mod_index(game) {
        game.native_settings_refresh(i);
    }
}

// Either of the two keys that open the click GUI.
pub(super) fn is_menu_key(game: &Game<'_>, key: &str) -> bool {
    key == game.native_mod_key(ID, MENU_KEY, MENU_DEFAULT) || key == game.native_mod_key(ID, MENU2_KEY, MENU2_DEFAULT)
}

pub(super) fn open_menu(game: &mut Game<'_>) {
    release_binds();
    let Some(i) = mod_index(game) else {
        say(game, "Turn Sunset on in Options > Mods first.");
        return;
    };
    with(|s| {
        s.page = Page::Main;
        s.binding = None;
    });
    game.open_options_at(false, horizon_client::options_ui::Page::NativeMod(i));
    game.native_settings_refresh(i);
}

pub(super) fn buttons(game: &Game<'_>, out: &mut Vec<String>) {
    ensure_loaded(game);
    with(|s| match s.page {
        Page::Main => {
            for c in CATEGORIES {
                let all: Vec<&Module> = s.modules.iter().filter(|m| m.category == c).collect();
                let on = all.iter().filter(|m| m.on).count();
                out.push(format!("{} ({on} of {} on)", c.label(), all.len()));
            }
            out.push(format!("Friends: {}", if s.friends.is_empty() { "none".into() } else { s.friends.join(", ") }));
            out.push("Type .help in chat for commands".into());
        }
        Page::Category(c) => {
            out.push("< Back".into());
            for m in s.modules.iter().filter(|m| m.category == c) {
                out.push(format!("{}: {}", m.name, on_off(m.on)));
            }
        }
        Page::Module(i) => {
            let m = &s.modules[i];
            out.push(format!("< Back to {}", m.category.label()));
            out.push(format!("{}: {}", m.name, on_off(m.on)));
            if s.binding == Some(i) {
                out.push("Keybind: > press a key (Backspace: none) <".into());
            } else {
                out.push(format!("Keybind: {}", key_label(m.key.as_deref())));
            }
            for setting in &m.settings {
                out.push(setting.shown());
            }
            let mut about = m.about.to_owned();
            if m.settings.iter().any(|s| matches!(s.value, Value::Num { .. })) {
                about.push_str(" Shift-click lowers a number.");
            }
            out.push(about);
        }
    });
}

pub(super) fn click(game: &mut Game<'_>, button: usize) {
    ensure_loaded(game);
    let shift = game.modifiers.shift_key();
    let mut toggled = None;
    let changed = with(|s| match s.page {
        Page::Main => {
            if let Some(&c) = CATEGORIES.get(button) {
                s.page = Page::Category(c);
            }
            false
        }
        Page::Category(c) => {
            if button == 0 {
                s.page = Page::Main;
                return false;
            }
            let idx: Vec<usize> = (0..s.modules.len()).filter(|&i| s.modules[i].category == c).collect();
            if let Some(&i) = idx.get(button - 1) {
                s.page = Page::Module(i);
            }
            false
        }
        Page::Module(i) => match button {
            0 => {
                s.binding = None;
                s.page = Page::Category(s.modules[i].category);
                false
            }
            1 => {
                toggled = Some(i);
                false
            }
            2 => {
                s.binding = if s.binding == Some(i) { None } else { Some(i) };
                false
            }
            b => match s.modules[i].settings.get_mut(b - 3) {
                Some(setting) => {
                    setting.click(shift);
                    true
                }
                None => false,
            },
        },
    });
    if let Some(i) = toggled {
        toggle(game, i, false);
    } else if changed {
        save(game);
    }
}

pub(super) fn toggle(game: &mut Game<'_>, i: usize, announce: bool) {
    let (name, on) = with(|s| {
        // Turned on or off by hand: a Hold bind no longer has it.
        s.inputs.take_where(|held| held.module == i);
        let m = &mut s.modules[i];
        m.on = !m.on;
        (m.name, m.on)
    });
    if !on {
        match name {
            "Scaffold" => {
                with(|s| { s.scaffold_wait = 0; s.scaffold_recent.clear(); });
                restore_scaffold_rotation(game);
            }
            "Speed" => {
                with(|s| { s.strafe = Strafe::default(); s.knockback_ticks = 0; });
                horizon_client::motion::sunset_motion::set_across(None);
                horizon_client::motion::sunset_motion::set_up(None);
            },
            "Velocity" => with(|s| { s.velocity = None; s.blast = [0.0; 3]; }),
            "No Slow" => horizon_client::input::sunset_flags::NO_SLOW.store(false, Ordering::Relaxed),
            _ => {},
        }
    }
    save(game);
    if announce {
        say(game, &format!("{name} {}", if on { "on" } else { "off" }));
    }
}

fn take_scaffold_rotation(saved: &mut Option<(f32, f32)>) -> Option<(f32, f32)> {
    saved.take()
}

fn restore_scaffold_rotation(game: &mut Game<'_>) {
    if let Some((yaw, pitch)) = with(|s| take_scaffold_rotation(&mut s.scaffold_rotation)) {
        game.yaw = yaw;
        game.pitch = pitch;
    }
}

// ---------------------------------------------------------------- keys

// Modules Hold binds had on, back as they were (left off if something turned them off meanwhile).
fn restore_holds(s: &mut State, held: Vec<Held>) {
    if held.is_empty() {
        return;
    }
    for h in held {
        if let Some(m) = s.modules.get_mut(h.module)
            && m.on
        {
            m.on = h.previous;
        }
    }
    sync_flags(s);
}

// Holds whose module went off, or whose bind or Bind mode changed, end.
fn release_changed_binds(s: &mut State) {
    let modules = &s.modules;
    let held = s.inputs.take_where(|h| {
        modules
            .get(h.module)
            .is_none_or(|m| !m.on || m.key.as_deref() != Some(h.key.as_str()) || m.p("Bind mode") != "Hold")
    });
    restore_holds(s, held);
}

// A screen, the menu or another window has the input: every hold ends.
fn release_binds() {
    with(|s| {
        let held = s.inputs.suspend();
        restore_holds(s, held);
    });
}

fn set_bind(s: &mut State, module: usize, key: Option<String>) {
    let held = s.inputs.take_where(|h| h.module == module);
    restore_holds(s, held);
    s.modules[module].key = key;
}

// Each tick, whatever the screen: keys let go of where no release came (from what is physically down), and no holds
// while the menu or a screen has the input (another window taking it is the window's Focused event, in bind_event).
fn sync_binds(game: &Game<'_>) {
    if !game.native_window() {
        return;
    }
    with(|s| {
        let held = s.inputs.missing(|name| {
            game.physical_keys.iter().any(|k| horizon_client::keybinds::key_name(*k) == Some(name))
                || game.physical_buttons.iter().any(|b| horizon_client::keybinds::mouse_name(*b) == name)
        });
        restore_holds(s, held);
        release_changed_binds(s);
        if !game.native_on(ID) || game.player.is_none() || !game.native_playing() || s.gui.open {
            let held = s.inputs.suspend();
            restore_holds(s, held);
        }
    });
}

// The window's presses and releases before any screen has them: a release a screen swallows still ends its hold, a
// key pressed in the menu or by the window coming back into focus is no bind press, and lost focus ends every hold.
pub(super) fn bind_event(event: &winit::event::WindowEvent) {
    use winit::event::{ElementState, WindowEvent};
    use winit::keyboard::{Key, NamedKey, PhysicalKey};
    if matches!(event, WindowEvent::Focused(false) | WindowEvent::CloseRequested | WindowEvent::Destroyed) {
        release_binds();
    }
    let (name, pressed, synthetic) = match event {
        WindowEvent::KeyboardInput { event, is_synthetic, .. } => {
            // Escape and Caps Lock by their meaning, as Horizon's keymap takes them.
            let code = match &event.logical_key {
                Key::Named(NamedKey::Escape) => KeyCode::Escape,
                Key::Named(NamedKey::CapsLock) => KeyCode::CapsLock,
                _ => match event.physical_key {
                    PhysicalKey::Code(code) => code,
                    PhysicalKey::Unidentified(_) => return,
                },
            };
            let Some(name) = horizon_client::keybinds::key_name(code) else {
                return;
            };
            (name.to_owned(), event.state == ElementState::Pressed, *is_synthetic)
        }
        WindowEvent::MouseInput { state, button, .. } => (horizon_client::keybinds::mouse_name(*button), *state == ElementState::Pressed, false),
        _ => return,
    };
    with(|s| {
        if !pressed {
            let held = s.inputs.release(&name);
            restore_holds(s, held);
            return;
        }
        if s.gui.open || synthetic {
            s.inputs.block(&name);
            let held = s.inputs.suspend();
            restore_holds(s, held);
        }
        if synthetic {
            s.inputs.synthetic(&name);
        }
    });
}

pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !game.native_window() {
        return;
    }
    if !pressed {
        with(|s| {
            let held = s.inputs.release(key);
            restore_holds(s, held);
        });
        return;
    }
    if !game.native_on(ID) {
        release_binds();
        return;
    }
    if with(|s| s.inputs.is_synthetic(key)) {
        return;
    }
    ensure_loaded(game);
    if gui_open() {
        gui_key(game, key);
        return;
    }
    if let Some(i) = with(|s| s.binding) {
        let chosen = match key {
            "key.keyboard.escape" => {
                with(|s| s.binding = None);
                refresh(game);
                return;
            }
            "key.keyboard.backspace" | "key.keyboard.delete" => None,
            k => Some(k.to_owned()),
        };
        with(|s| {
            set_bind(s, i, chosen);
            s.binding = None;
        });
        save(game);
        refresh(game);
        return;
    }
    if !game.native_playing() || game.player.is_none() {
        with(|s| s.inputs.block(key));
        release_binds();
        return;
    }
    // The chat key opens a text screen: a bind on the same key is not pressed by it.
    if !is_menu_key(game, key) && (key == game.settings.keys.get("key.chat") || key == game.settings.keys.get("key.command")) {
        with(|s| s.inputs.block(key));
        release_binds();
        return;
    }
    // A fresh press only: not one a screen or the menu had when it went down.
    if !with(|s| s.inputs.press(key)) {
        return;
    }
    if is_menu_key(game, key) {
        open_gui(game);
        return;
    }
    if key == "key.mouse.middle" && with(|s| s.is_on("Middle Click Extra")) {
        middle_click(game);
    }
    if key == "key.mouse.right" {
        super::sunset_player::right_click(game);
    }
    let bound: Vec<usize> = with(|s| {
        (0..s.modules.len())
            .filter(|&i| s.modules[i].key.as_deref() == Some(key))
            .collect()
    });
    for i in bound {
        if with(|s| s.modules[i].p("Bind mode") == "Hold") {
            // On while the key is down, then as it was.
            with(|s| {
                let previous = s.modules[i].on;
                s.inputs.hold(i, key, previous);
                s.modules[i].on = true;
                sync_flags(s);
            });
        } else {
            toggle(game, i, true);
        }
    }
    // Air Jump: the jump key in mid-air jumps again.
    if key == "key.keyboard.space" && with(|s| s.is_on("Air Jump")) {
        if let Some(p) = game.player.as_mut()
            && !p.ground.on_ground
            && !p.flying
        {
            p.velocity.y = 0.42;
            p.fall_distance = 0.0;
        }
        remember_velocity(game);
    }
}

// ---------------------------------------------------------------- ticks

// Before the game ticks: keys this mod holds, abilities, step height.
pub(super) fn pre_tick(game: &mut Game<'_>) {
    sync_binds(game);
    // Single player's own server ticks a game of its own (no window) through here too: it must leave the player's
    // state alone, or it clears Velocity's baseline and Speed's push between the client's ticks.
    if !game.native_window() {
        return;
    }
    if !game.native_on(ID) || game.player.is_none() {
        horizon_client::motion::sunset_motion::set_across(None);
        horizon_client::motion::sunset_motion::set_up(None);
        horizon_client::motion::sunset_motion::set_safe_walk(false);
        horizon_client::motion::sunset_motion::set_screen_sprint(None);
        restore_scaffold_rotation(game);
        with(|s| { s.velocity = None; s.blast = [0.0; 3]; s.knockback_ticks = 0; s.scaffold_recent.clear(); });
        return;
    }
    ensure_loaded(game);
    MOVED_THIS_TICK.store(false, Ordering::Relaxed);
    let mods = snapshot();
    super::sunset_render::pre_tick(game, &mods);
    super::sunset_player::speed_mine(game, mods.iter().find(|m| m.name == "Speed Mine" && m.on));
    super::sunset_player::no_break_reset(game, mods.iter().find(|m| m.name == "No Break Reset" && m.on));
    let on = |n: &str| mods.iter().any(|m| m.name == n && m.on);
    let get = |n: &str| mods.iter().find(|m| m.name == n).cloned();
    let playing = game.native_playing();
    // The click GUI holds no movement keys, as a screen would.
    if gui_open() {
        game.keys.clear();
    }
    let mut want = Vec::new();
    if on("Auto Walk") && playing {
        want.push(KeyCode::KeyW);
    }
    if on("Sprint") && playing {
        want.push(KeyCode::ControlLeft);
    }
    if on("Auto Jump") && playing && move_input(game) != (0.0, 0.0) {
        want.push(KeyCode::Space);
    }
    if on("Sneak") && playing {
        want.push(KeyCode::ShiftLeft);
    }
    // Criticals' Jump mode presses jump for this tick: the game's own jump.
    if CRIT_JUMP.swap(false, Ordering::Relaxed) && playing {
        want.push(KeyCode::Space);
    }
    // Parkour presses jump for this tick: the game's own jump (a sprint jump's push too).
    if let Some(m) = get("Parkour").filter(|m| m.on)
        && playing
        && parkour_jump(game, &m)
    {
        want.push(KeyCode::Space);
    }
    if let Some(m) = get("Anti AFK").filter(|m| m.on)
        && playing
    {
        want.extend(super::sunset_player::afk_keys(&m));
    }
    let scaffold_walk = get("Scaffold").is_some_and(|m| m.on && m.b("Safe walk"));
    horizon_client::motion::sunset_motion::set_safe_walk((on("Safe Walk") || scaffold_walk) && playing);
    // GUI Move walking you forward behind a screen: a sprint goes on (the game would stop it, a screen gives it no
    // forward key), and starts with Ctrl held or the Sprint module.
    let held = |k: KeyCode| game.keys.contains(&k) || game.physical_keys.contains(&k);
    let screen_sprint = get("GUI Move")
        .filter(|m| m.on && m.b("Sprint") && !gui_open() && gui_move_screen(game, m) && held(KeyCode::KeyW) && !held(KeyCode::KeyS))
        .map(|_| on("Sprint") || held(KeyCode::ControlLeft));
    horizon_client::motion::sunset_motion::set_screen_sprint(screen_sprint);
    let mut injected = Vec::new();
    for k in want {
        if game.keys.insert(k) {
            injected.push(k);
        }
    }
    velocity(game, get("Velocity").filter(|m| m.on).as_ref());
    let across = get("Speed").filter(|m| m.on).and_then(|m| speed(game, &m));
    horizon_client::motion::sunset_motion::set_across(across.map(|(xz, _)| xz));
    horizon_client::motion::sunset_motion::set_up(across.and_then(|(_, y)| y));
    let flight = get("Flight").filter(|m| m.on);
    flight_pre(game, flight.as_ref());
    let step = get("Step").filter(|m| m.on);
    let high_jump = get("High Jump").filter(|m| m.on);
    let (was_flying, was_stepping, was_high_jumping) = with(|s| (s.flying, s.stepping, s.high_jumping));
    let (creative, spectator_mode) = (game.creative, game.spectator());
    let step_to = step.as_ref().map_or(0.6, |st| step_height(game, st));
    if let Some(p) = game.player.as_mut() {
        if flight.is_none() && was_flying {
            p.flying = creative && p.flying || spectator_mode;
            p.may_fly = creative || spectator_mode;
            p.flying_speed = 0.05;
        }
        if step.is_some() {
            let _ = p.set_modifier("minecraft:step_height", "sunset:step", step_to - 0.6, "add_value", true);
        } else if was_stepping {
            let _ = p.set_modifier("minecraft:step_height", "sunset:step", 0.0, "add_value", false);
        }
        // High Jump: Meteor multiplies the jump's own speed, so the jump starts higher from its first tick.
        if let Some(h) = &high_jump {
            let _ = p.set_modifier("minecraft:jump_strength", "sunset:high_jump", h.n("Multiplier") - 1.0, "add_multiplied_total", true);
        } else if was_high_jumping {
            let _ = p.set_modifier("minecraft:jump_strength", "sunset:high_jump", 0.0, "add_multiplied_total", false);
        }
    }
    with(|s| {
        s.injected.extend(injected);
        s.flying = flight.as_ref().is_some_and(|m| m.p("Mode") == "Abilities");
        s.stepping = step.is_some();
        s.high_jumping = high_jump.is_some();
    });
}

// How high Step lets you climb this tick: the setting, or vanilla's 0.6 when
// its conditions say no (Meteor's Step: active when, safe step).
fn step_height(game: &Game<'_>, m: &Module) -> f64 {
    let sneaking = game.keys.contains(&KeyCode::ShiftLeft);
    let active = match m.p("Active when") {
        "Sneaking" => sneaking,
        "Not sneaking" => !sneaking,
        _ => true,
    };
    if !active {
        return 0.6;
    }
    if m.b("Safe step")
        && let Some(p) = game.player.as_ref()
    {
        let health = (p.health.health() + p.health.absorption()) as f64;
        let at = [p.position.x, p.position.y, p.position.z];
        let hurt = super::sunset_util::crystal_damage(game, at);
        // Safe means staying above the limit now and after the nearest crystal goes off.
        if health <= m.n("Step health") || health - hurt <= m.n("Step health") {
            return 0.6;
        }
    }
    m.n("Height")
}

// The screens GUI Move walks you in: the inventory and containers (not an anvil's rename box) and, with "All menus",
// the pause menu, the Options pages with no search box, Statistics, Advancements and the creative tabs but the search. Never
// a screen you type in. (A single-player game stops while the pause menu and the like are open: nothing moves there.)
fn gui_move_screen(game: &Game<'_>, m: &Module) -> bool {
    if game.screen == Screen::Inventory {
        return game.remote_menu().1 != "anvil";
    }
    if m.p("Screens") != "All menus" {
        return false;
    }
    match game.screen {
        Screen::Paused | Screen::Stats | Screen::Advancements => true,
        Screen::Options => {
            use horizon_client::options_ui::Page;
            let options = &game.ui.options;
            options.selected_key.is_none()
                && !matches!(options.page(), Page::Video | Page::Controls | Page::KeyBinds | Page::Language | Page::Packs)
        }
        Screen::Creative => game.creative_screen.tabs.get(game.creative_screen.selected).is_some_and(|t| !t.search),
        _ => false,
    }
}

// After the game ticks: let go of held keys, then each module's work.
// After the tick: first what waited for the look its move packet carried, then the modules, then (in a tick that
// sent no move packet) the one that carries the next look.
pub(super) fn post_tick(game: &mut Game<'_>) {
    if !game.native_window() || !game.native_on(ID) {
        return;
    }
    turned(game);
    module_tick(game);
    end_turns(game);
}

fn module_tick(game: &mut Game<'_>) {
    ensure_loaded(game);
    let injected = with(|s| std::mem::take(&mut s.injected));
    for k in injected {
        if !game.physical_keys.contains(&k) {
            game.keys.remove(&k);
        }
    }
    let mods = snapshot();
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    // Once a session, in a world: how to get to the menu.
    if game.remote.is_some() && game.player.is_some() && !WELCOMED.swap(true, Ordering::Relaxed) {
        say(game, &format!("{VERSION} is on. Press ` for the menu (or Right Shift), type .help for the commands."));
    }
    let ticks = with(|s| {
        s.ticks += 1;
        s.ticks
    });

    // Fullbright: the renderer's own fullbright, while on or while the
    // player's own extra is.
    let fullbright = get("Fullbright").is_some() || game.settings.extras.fullbright;
    if let Some(r) = game.renderer.as_mut() {
        r.fullbright = fullbright;
    }

    if game.player.is_none() {
        return;
    }

    if get("Auto Respawn").is_some() && game.screen == Screen::Dead {
        let wait = with(|s| {
            s.respawn_wait = s.respawn_wait.saturating_sub(1);
            let w = s.respawn_wait;
            if w == 0 {
                s.respawn_wait = 20;
            }
            w
        });
        if wait == 0 {
            let _ = game.respawn_player();
        }
        return;
    }
    if game.screen == Screen::Dead {
        return;
    }

    if get("Panic").is_some() {
        panic_off(game);
        return;
    }
    if let Some(m) = get("Break Delay") {
        let wait = m.n("Cooldown (ticks)") as u32;
        game.destroy_delay = game.destroy_delay.min(wait);
        super::remote::sunset_remote::clear_destroy_delay(game, wait);
    }

    // GUI Move: the game ignores the keys while a menu is open, so what is held steers by hand. Flight,
    // Speed and the rest read the same keys, so they work with it open too.
    let screen_move = get("GUI Move").filter(|m| !gui_open() && gui_move_screen(game, m));
    let held = |k: KeyCode| game.keys.contains(&k) || game.physical_keys.contains(&k);
    let screen_keys = screen_move.as_ref().map(|m| {
        (
            f64::from(i8::from(held(KeyCode::KeyW)) - i8::from(held(KeyCode::KeyS))),
            f64::from(i8::from(held(KeyCode::KeyA)) - i8::from(held(KeyCode::KeyD))),
            m.b("Jump") && held(KeyCode::Space),
            m.b("Sneak") && held(KeyCode::ShiftLeft),
            m.b("Arrows"),
            [held(KeyCode::ArrowLeft), held(KeyCode::ArrowRight), held(KeyCode::ArrowUp), held(KeyCode::ArrowDown)],
        )
    });
    if let Some((.., true, arrows)) = screen_keys {
        let turn = |neg: bool, pos: bool| f32::from(i8::from(pos) - i8::from(neg));
        game.yaw += turn(arrows[0], arrows[1]) * 6.0;
        game.pitch = (game.pitch + turn(arrows[2], arrows[3]) * 4.0).clamp(-90.0, 90.0);
    }
    let (yaw, pitch) = (game.yaw, game.pitch);
    let flat = |f: f64, s: f64| {
        let a = f64::from(yaw).to_radians();
        let (x, z) = (-a.sin() * f + a.cos() * s, a.cos() * f + a.sin() * s);
        let len = (x * x + z * z).sqrt();
        (len > 0.0).then(|| [x / len, z / len])
    };
    let dir = move_direction(game).or_else(|| screen_keys.and_then(|(f, s, ..)| flat(f, s)));
    let jump = game.native_playing() && game.keys.contains(&KeyCode::Space) || screen_keys.is_some_and(|k| k.2);
    let sneak = game.native_playing() && game.keys.contains(&KeyCode::ShiftLeft) || screen_keys.is_some_and(|k| k.3);
    let forward = move_input(game).0 > 0.0 || screen_keys.is_some_and(|k| k.0 > 0.0);
    let dim = game.dim.id();
    let flight_velocity = get("Flight").is_some_and(|f| f.p("Mode") == "Velocity");
    flight_post(game, get("Flight"), jump, sneak, screen_keys.and(dir));
    let speed_hack = get("Speed").is_some();

    if let Some(p) = game.player.as_mut() {
        // Without Flight or Speed to move you, the inventory screen walks you at the game's own speeds.
        if let Some((.., sneaking, _, _)) = screen_keys
            && !flight_velocity
            && !speed_hack
        {
            if p.flying {
                // Creative flight: about 11 blocks a second at the default flying speed.
                let fly = f64::from(p.flying_speed) / 0.05 * 10.9 / 20.0;
                if let Some([x, z]) = dir {
                    p.velocity.x = x * fly;
                    p.velocity.z = z * fly;
                }
                p.velocity.y = if jump { fly * 0.6 } else if sneak { -fly * 0.6 } else { 0.0 };
            } else {
                if let Some([x, z]) = dir {
                    let factor = screen_move.as_ref().map_or(1.0, |m| {
                        m.n(if p.ground.on_ground { "Ground speed %" } else { "Air speed %" }) / 100.0
                    });
                    let speed = (if sneaking { 0.0655 } else if p.sprinting { 0.2806 } else { 0.2159 }) * factor;
                    p.velocity.x = x * speed;
                    p.velocity.z = z * speed;
                }
                if jump && p.ground.on_ground {
                    p.velocity.y = 0.42;
                }
            }
        }
        if get("No Jump Delay").is_some() {
            p.no_jump_delay = 0;
        }
        if let Some(g) = get("Glide")
            && !p.ground.on_ground
            && !p.flying
            && !p.fall_flying
        {
            p.velocity.y = p.velocity.y.max(-g.n("Fall speed") / 20.0);
            p.fall_distance = 0.0;
        }
        if let Some(sp) = get("Spider")
            && p.collision.horizontal
            && dir.is_some()
        {
            p.velocity.y = sp.n("Climb speed");
            p.fall_distance = 0.0;
        }
        if let Some(e) = get("Elytra Boost")
            && p.fall_flying
            && forward
        {
            let look = horizon_client::gizmos::view_vector(pitch, yaw);
            let a = e.n("Boost");
            p.velocity.x += look[0] * a;
            p.velocity.y += look[1] * a;
            p.velocity.z += look[2] * a;
            let v = &mut p.velocity;
            let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
            let max = e.n("Max speed");
            if len > max {
                let k = max / len;
                (v.x, v.y, v.z) = (v.x * k, v.y * k, v.z * k);
            }
        }
        // Velocity's Sinking: in water or lava you stay level unless you jump or sneak.
        if get("Velocity").is_some_and(|m| m.b("Sinking"))
            && !jump
            && !sneak
            && (p.fluids.was_touching_water || p.is_in_lava())
            && p.velocity.y < 0.0
        {
            p.velocity.y = 0.0;
        }
        if get("Anti Void").is_some() {
            let floor = if dim == "minecraft:overworld" { -64.0 } else { 0.0 };
            if p.position.y < floor && p.velocity.y < 0.0 {
                p.velocity.y = 0.6;
                p.fall_distance = 0.0;
            }
        }
    }

    remember_velocity(game);
    let alive = game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    // Blink: let go of what it held when it turns off, or at its limit.
    {
        let max = get("Blink").map_or(0.0, |m| m.n("Max ticks")) as u32;
        let held = BLINK_HELD.load(Ordering::Relaxed);
        let ticks = with(|s| {
            s.blink_ticks = if get("Blink").is_some() && held { s.blink_ticks + 1 } else { 0 };
            s.blink_ticks
        });
        if held && (get("Blink").is_none() || (max > 0 && ticks >= max)) {
            // Moves go through again from the next tick; this one carries the player.
            let was_on = BLINK.swap(false, Ordering::Relaxed);
            flush_blink(game);
            BLINK.store(was_on && get("Blink").is_some(), Ordering::Relaxed);
            with(|s| s.blink_ticks = 0);
        }
    }
    let (jump_now, ground_now) = (
        game.native_playing() && game.keys.contains(&KeyCode::Space),
        game.player.as_ref().is_some_and(|p| p.ground.on_ground),
    );
    let was_ground = with(|s| std::mem::replace(&mut s.was_ground, ground_now));
    if let Some(p) = game.player.as_mut() {
        if let Some(m) = get("Long Jump")
            && was_ground
            && !ground_now
            && jump_now
            && p.velocity.y > 0.3
        {
            p.velocity.x *= m.n("Multiplier");
            p.velocity.z *= m.n("Multiplier");
        }
        if let Some(m) = get("Reverse Step")
            && was_ground
            && !ground_now
            && !jump_now
            && p.velocity.y <= 0.0
            && !p.flying
        {
            p.velocity.y = -m.n("Speed");
        }
        if let Some(m) = get("Fast Climb")
            && p.last_climbable.is_some()
            && (forward || jump_now)
            && !p.flying
        {
            p.velocity.y = m.n("Speed");
        }
    }
    if alive && let Some(m) = get("Auto Log") && game.remote.is_some()
        && game.player.as_ref().is_some_and(|p| f64::from(p.health.health()) <= m.n("Health"))
    {
        let health = game.player.as_ref().map_or(0.0, |p| p.health.health());
        if let Some(i) = with(|s| s.find("Auto Log")) {
            toggle(game, i, false);
        }
        let _ = game.leave_server(Some(format!("Sunset Auto Log: health was {health}")));
        return;
    }
    if let Some(m) = get("Scaffold").filter(|_| get("Kill Aura").is_none() && get("Blink").is_none()) {
        scaffold(game, m);
    }
    if let Some(m) = get("Auto Armor") {
        auto_armor(game, m);
    }
    if get("Chest Swap").is_some() {
        chest_swap(game);
        if let Some(i) = with(|s| s.find("Chest Swap")) {
            toggle(game, i, false);
        }
    }
    if let Some(m) = get("Auto Replenish") {
        auto_replenish(game, m);
    }
    if let Some(m) = get("Bow Spam") {
        bow_spam(game, m);
    }
    if let Some(m) = get("Notifier") {
        notify(game, m);
    } else {
        with(|s| s.notify = None);
    }
    if let Some(m) = get("Weather Changer") {
        let w = &mut game.world.weather;
        let (rain, thunder) = match m.p("Weather") {
            "Rain" => (true, false),
            "Thunder" => (true, true),
            _ => (false, false),
        };
        w.raining = rain;
        w.thundering = thunder;
        w.rain_level = if rain { (1.0, 1.0) } else { (0.0, 0.0) };
        w.thunder_level = if thunder { (1.0, 1.0) } else { (0.0, 0.0) };
    }
    if let Some(m) = get("Hole ESP") {
        scan_holes(game, m, ticks);
    } else {
        with(|s| s.holes = Sweep::new());
    }
    if let Some(m) = get("Nuker") {
        nuker(game, m);
    } else if let Some((pos, _)) = with(|s| s.nuke.take()) {
        // Turned off in the middle of a block: the server stops digging it too.
        let seq = super::sunset_util::sequence(game);
        game.send(S::PlayerAction(play::PlayerAction { action: 2, pos, direction: 1, sequence: seq }));
    }
    if let Some(m) = get("Auto Tool") {
        auto_tool(game, m);
    }
    if let Some((back, wait)) = with(|s| s.mc_back) {
        if wait <= 1 {
            hold(game, back);
            with(|s| s.mc_back = None);
        } else {
            with(|s| s.mc_back = Some((back, wait - 1)));
        }
    }
    if let Some(m) = get("Logout Spots") {
        track_logouts(game, m);
    } else {
        with(|s| {
            s.spots.clear();
            s.last_pos.clear();
        });
    }
    // Time Changer: the sky the player sees; the server's own time comes back
    // with its next update.
    if let Some(m) = get("Time Changer") {
        game.world.day_time = m.n("Time") as i64;
    }
    if let Some(m) = get("Auto Clicker") {
        auto_click(game, m);
    }
    if let Some(m) = get("Auto Eat") {
        auto_eat(game, m);
    } else if with(|s| s.eating) {
        eat_stop(game);
    }
    // Breadcrumbs: a point every few ticks once the player has moved.
    if let Some(m) = get("Breadcrumbs") {
        let max = m.n("Length") as usize;
        if let Some(p) = game.player.as_ref() {
            let here = [p.position.x, p.position.y + 0.1, p.position.z];
            with(|s| {
                let far = s
                    .crumbs
                    .last()
                    .is_none_or(|l| (l[0] - here[0]).powi(2) + (l[1] - here[1]).powi(2) + (l[2] - here[2]).powi(2) > 0.25);
                if far && ticks.saturating_sub(s.crumb_at) >= 2 {
                    s.crumbs.push(here);
                    s.crumb_at = ticks;
                }
                let over = s.crumbs.len().saturating_sub(max);
                s.crumbs.drain(..over);
            });
        }
    } else {
        with(|s| s.crumbs.clear());
    }
    if let Some(m) = get("Block ESP") {
        scan_blocks(game, m, ticks);
    } else {
        with(|s| s.blocks = Sweep::new());
    }
    super::sunset_combat::tick(game, &mods);
    super::sunset_render::tick(game, &mods, ticks);
    super::sunset_player::tick(game, &mods);
    super::sunset_world::tick(game, &mods);
    crit_tick(game);
    if let Some(m) = get("Kill Aura") {
        kill_aura(game, m, get("Criticals").is_some());
    }
    if let Some(m) = get("Trigger Bot") {
        trigger_bot(game, m, get("Criticals").is_some_and(|c| !c.b("Only Kill Aura")));
    }
}

pub(super) fn solid_at(game: &Game<'_>, x: i32, y: i32, z: i32) -> bool {
    !game.res.facts().is_air(game.world.block(x, y, z))
}

// The block column just ahead of the player's feet, in the direction the
// move keys point: its x and z, and the y of the block under the feet.
// Parkour (Meteor's), before the tick moves you: on the ground, moving, jump not held and not sneaking. Last tick:
// jump when your box where this tick's move takes you (as far as the last tick's), Edge distance narrower each side
// and half a block down, would touch no block, so you leave from the last spot on it. Early: when a point 0.35 blocks
// ahead of you is over air.
fn parkour_jump(game: &Game<'_>, m: &Module) -> bool {
    use horizon_core::geometry::Vec3;
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let (forward, side) = move_input(game);
    let held = |k: KeyCode| game.keys.contains(&k);
    if !p.ground.on_ground
        || held(KeyCode::Space)
        || (forward == 0.0 && side == 0.0)
        || (held(KeyCode::ShiftLeft) && !m.b("Ignore sneaking"))
        || (m.b("Only while sprinting") && !p.sprinting)
        || (m.b("Only when moving forward") && forward <= 0.0)
    {
        return false;
    }
    if m.p("Jump when") == "Early" {
        return ahead(game).is_some_and(|(x, y, z)| !solid_at(game, x, y, z));
    }
    let step = Vec3::new(p.position.x - game.previous_position.x, -0.5, p.position.z - game.previous_position.z);
    let edge = m.n("Edge distance");
    let under = p.bounds().move_by(step).deflate(Vec3::new(edge, 0.0, edge));
    horizon_client::collisions::SceneCollisions { world: &game.world, binding: game.collision, subject: None }
        .unobstructed(under)
        .unwrap_or(false)
}

pub(super) fn ahead(game: &Game<'_>) -> Option<(i32, i32, i32)> {
    let p = game.player.as_ref()?;
    let d = move_direction(game)?;
    let (ax, az) = (p.position.x + d[0] * 0.35, p.position.z + d[1] * 0.35);
    Some((ax.floor() as i32, (p.position.y - 0.5).floor() as i32, az.floor() as i32))
}

// Switches the held slot now: the server is told before anything is sent
// that depends on it.
pub(super) fn hold(game: &mut Game<'_>, slot: usize) {
    if game.selected != slot {
        game.selected = slot;
        game.send(S::SetCarriedItem(play::SetCarriedItem { slot: slot as i16 }));
    }
}

pub(super) fn item_info(game: &Game<'_>, slot: usize) -> Option<horizon_client::items::ItemInfo> {
    let stack = game.inventory.get(slot)?.as_ref()?;
    game.res.items().get(&stack.item).cloned()
}

pub(super) fn best_weapon(game: &Game<'_>, prefer_axes: bool) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for slot in 0..9 {
        let Some(stack) = game.inventory.get(slot).and_then(|s| s.as_ref()) else {
            continue;
        };
        let Some(info) = game.res.items().get(&stack.item) else {
            continue;
        };
        if info.attack_damage <= 0.0 {
            continue;
        }
        let kind = if stack.item.ends_with("_axe") && prefer_axes {
            100.0
        } else if stack.item.ends_with("_sword") && !prefer_axes {
            100.0
        } else {
            0.0
        };
        let score = info.attack_damage + kind;
        if best.is_none_or(|(_, b)| score > b) {
            best = Some((slot, score));
        }
    }
    best.map(|(slot, _)| slot)
}

// Scaffold's blocks: a block item in the list (or not in it, as a blacklist) that is a full block and does not fall.
fn scaffold_item(game: &Game<'_>, m: &Module, slot: usize) -> bool {
    let Some(state) = item_info(game, slot).and_then(|i| i.block) else {
        return false;
    };
    let f = game.res.facts().get(state);
    let name = super::sunset_util::short(&f.name);
    let listed = super::sunset_util::listed(m.t("Blocks"), name);
    let falls = ["sand", "gravel", "concrete_powder", "anvil"].iter().any(|e| name.ends_with(e)) || name == "dragon_egg";
    listed == (m.p("Blocks filter") == "Whitelist") && f.full_collision && !falls
}

fn scaffold_slot(game: &Game<'_>, m: &Module) -> Option<usize> {
    if scaffold_item(game, m, game.selected) {
        return Some(game.selected);
    }
    (0..9).find(|&i| scaffold_item(game, m, i)).filter(|_| m.b("Auto switch"))
}

fn scaffold_place(game: &mut Game<'_>, m: &Module, at: [i32; 3]) -> bool {
    let duplicate_cooldown = m.n("Duplicate cooldown") as u64;
    if duplicate_cooldown > 0 && with(|s| s.scaffold_recent.iter().any(|(pos, _)| *pos == at)) { return false; }
    let Some(eye) = game.eye_position() else { return false; };
    let side = super::sunset_util::place_side(game, at);
    if side.is_none() && !m.b("Air place") { return false; }
    let neighbor = side.map_or(at, |(pos, _)| pos);
    let cursor = super::sunset_movement::face_cursor(at, neighbor);
    let hit = if neighbor == at {
        [f64::from(at[0]) + 0.5, f64::from(at[1]) + 1.0, f64::from(at[2]) + 0.5]
    } else {
        [f64::from(neighbor[0]) + f64::from(cursor[0]), f64::from(neighbor[1]) + f64::from(cursor[1]), f64::from(neighbor[2]) + f64::from(cursor[2])]
    };
    if super::sunset_util::dist(eye, hit) > m.n("Max placement distance") { return false; }
    if m.b("Validate ray") && !super::sunset_movement::clear_voxel_ray(eye, hit, |cell| {
        super::sunset_util::facts(game, cell).full_collision
    }) { return false; }
    if m.b("Rotate") && m.n("Rotation speed") < 180.0 {
        let (yaw, pitch) = look_at(eye, hit);
        let rate = m.n("Rotation speed") as f32;
    let initial = (game.yaw, game.pitch);
    with(|s| { s.scaffold_rotation.get_or_insert(initial); });
        game.yaw = super::sunset_movement::turn_toward(game.yaw, yaw, rate);
        game.pitch = super::sunset_movement::turn_toward(game.pitch, pitch, rate).clamp(-90.0, 90.0);
        let turn = (yaw - game.yaw + 180.0).rem_euclid(360.0) - 180.0;
        if turn.abs() > 2.0 || (pitch - game.pitch).abs() > 2.0 { return false; }
    }
    let Some(slot) = scaffold_slot(game, m) else {
        return false;
    };
    let placed = super::sunset_util::place_block(game, at, slot, m.b("Rotate"), true);
    if placed {
        with(|s| {
            if duplicate_cooldown > 0 {
                s.scaffold_recent.push((at, s.ticks + duplicate_cooldown));
            }
            if m.b("Render") { s.scaffold_placed.push((at, 8)); }
        });
    }
    placed
}

// Scaffold (Meteor's): the block under where you will be next tick (or a step ahead, Ahead distance), down one more
// while you sneak; without a side to place it against, the nearest spot within Closest block range that has one.
// Air place puts it where you will be, and a Radius of blocks round it. Fast tower lifts you while you hold jump.
pub(super) fn scaffold(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    with(|s| {
        let ticks = s.ticks;
        s.scaffold_recent.retain(|(_, until)| *until > ticks);
        s.scaffold_placed.retain_mut(|(_, t)| {
            *t = t.saturating_sub(1);
            *t > 0
        });
    });
    if !game.native_playing() || game.remote.is_none() {
        return;
    }
    if m.b("Only on click") && !(game.physical_buttons.contains(&MouseButton::Right) || game.right) {
        return;
    }
    if with(|s| {
        s.scaffold_wait = s.scaffold_wait.saturating_sub(1);
        s.scaffold_wait > 0
    }) {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let held = |k: KeyCode| game.keys.contains(&k);
    let (pos, vel) = ([p.position.x, p.position.y, p.position.z], [p.velocity.x, p.velocity.y, p.velocity.z]);
    let next = [pos[0] + vel[0], pos[1] + vel[1] - 0.75, pos[2] + vel[2]];
    let feet = [pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32];
    let (jump, sneak) = (held(KeyCode::Space), held(KeyCode::ShiftLeft));
    let moving = move_input(game) != (0.0, 0.0);
    let item = scaffold_slot(game, m).is_some();
    let towering = m.b("Fast tower") && jump && !sneak && (m.b("While moving") || !moving) && item;
    let air = m.b("Air place");
    let mut at = if air {
        next
    } else {
        let mut here = [pos[0], pos[2]];
        let ahead = m.n("Ahead distance");
        let below = [feet[0], feet[1] - 1, feet[2]];
        if ahead != 0.0 && !towering && !super::sunset_util::is_air(game, below) {
            let yaw = f64::from(game.yaw).to_radians();
            let d = [-yaw.sin() * ahead, yaw.cos() * ahead];
            for (key, [x, z]) in [(KeyCode::KeyW, [d[0], d[1]]), (KeyCode::KeyS, [-d[0], -d[1]]), (KeyCode::KeyA, [d[1], -d[0]]), (KeyCode::KeyD, [-d[1], d[0]])] {
                if held(key) {
                    here = [here[0] + x, here[1] + z];
                }
            }
        }
        [here[0], next[1], here[1]]
    }
    .map(|v| v.floor() as i32);
    if sneak && !jump && pos[1] + next[1] > -1.0 {
        at[1] -= 1;
    }
    if at[1] >= feet[1] {
        at[1] = feet[1] - 1;
    }
    let target = at;
    if !air && super::sunset_util::place_side(game, at).is_none() {
        let Some(eye) = game.eye_position() else {
            return;
        };
        let r = m.n("Closest block range");
        let mut spots = Vec::new();
        for x in (pos[0] - r) as i32..(pos[0] + r).ceil() as i32 {
            for z in (pos[2] - r) as i32..(pos[2] + r).ceil() as i32 {
                for y in (pos[1] - r).max(-64.0) as i32..(pos[1] + r).min(320.0).ceil() as i32 {
                    let b = [x, y, z];
                    let Some((side, _)) = super::sunset_util::place_side(game, b) else {
                        continue;
                    };
                    if super::sunset_util::can_place(game, b, true) && super::sunset_util::dist(eye, super::sunset_util::center_of(side)) <= 6.0 {
                        spots.push(b);
                    }
                }
            }
        }
        let far = |b: &[i32; 3]| (0..3).map(|i| (b[i] - target[i]).pow(2)).sum::<i32>();
        let Some(best) = spots.into_iter().min_by_key(far) else {
            return;
        };
        at = best;
    }
    if air {
        let radius = m.n("Radius");
        let r = radius.floor() as i32;
        let mut blocks: Vec<[i32; 3]> = (at[0] - r..=at[0] + r)
            .flat_map(|x| (at[2] - r..=at[2] + r).map(move |z| [x, at[1], z]))
            .filter(|b| super::sunset_util::dist(pos, super::sunset_util::center_of(*b)) <= radius || (b[0] == at[0] && b[2] == at[2]))
            .collect();
        let near = |b: &[i32; 3]| super::sunset_util::dist(pos, super::sunset_util::center_of(*b));
        blocks.sort_by(|a, b| near(a).total_cmp(&near(b)));
        let mut placed = 0;
        for b in blocks {
            if placed >= m.n("Blocks per tick") as u32 {
                break;
            }
            if scaffold_place(game, m, b) {
                placed += 1;
            }
        }
    } else {
        scaffold_place(game, m, at);
    }
    with(|s| s.scaffold_wait = m.n("Delay (ticks)") as u32 + 1);
    if m.b("Fast tower") && jump && !sneak && item {
        let room = (0..2).all(|dy| super::sunset_util::is_air(game, [feet[0], feet[1] + 2 + dy, feet[2]]));
        let Some(p) = game.player.as_mut() else {
            return;
        };
        if room {
            if m.b("While moving") || !moving {
                p.velocity.y = m.n("Tower speed");
            }
        } else {
            p.velocity.y = p.position.y.ceil() - p.position.y;
            p.ground.on_ground = true;
        }
    }
}

pub(super) fn auto_click(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    if !game.native_playing() {
        return;
    }
    let (left, right) = (
        m.b("Left") && (game.physical_buttons.contains(&MouseButton::Left) || game.left),
        m.b("Right") && (game.physical_buttons.contains(&MouseButton::Right) || game.right),
    );
    if !left && !right {
        return;
    }
    if with(|s| {
        s.click_wait = s.click_wait.saturating_sub(1);
        s.click_wait > 0
    }) {
        return;
    }
    for (down, button) in [(left, MouseButton::Left), (right, MouseButton::Right)] {
        if down {
            let _ = game.play_mouse(button, true);
            let _ = game.play_mouse(button, false);
        }
    }
    with(|s| s.click_wait = m.n("Delay (ticks)") as u32);
}

// Auto Eat (Meteor's): once health or hunger is at its threshold, holds use on the best food (the off hand's first,
// then the hotbar's, then the inventory's moved into the hotbar), with Kill Aura off meanwhile, until neither is.
pub(super) fn auto_eat(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    if !game.native_playing() || super::sunset_player::gap_eating() {
        return;
    }
    if with(|s| s.eating) {
        if !eat_due(game, m) {
            eat_stop(game);
            return;
        }
        let slot = with(|s| s.eat_slot);
        let edible = match slot {
            Some(i) => edible(game, m, game.inventory.get(i).and_then(|s| s.as_ref())),
            None => edible(game, m, game.offhand.as_ref()),
        };
        if !edible {
            if !food_slot(game, m).is_some_and(|next| eat_from(game, next)) {
                eat_stop(game);
                return;
            }
        }
        if game.using.is_none() {
            let _ = game.play_mouse(MouseButton::Right, true);
        }
        return;
    }
    if !eat_due(game, m) {
        return;
    }
    let Some(slot) = food_slot(game, m) else {
        return;
    };
    let prev = game.selected;
    if !eat_from(game, slot) {
        return;
    }
    let paused = m.b("Pause auras") && with(|s| s.is_on("Kill Aura"));
    if paused {
        toggle_named(game, "Kill Aura", false);
    }
    with(|s| {
        s.eating = true;
        s.eat_prev = prev;
        s.eat_paused_aura = paused;
    });
    let _ = game.play_mouse(MouseButton::Right, true);
}

// Health and hunger against the thresholds, as Threshold mode joins them.
fn eat_due(game: &Game<'_>, m: &Module) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let health = f64::from(p.health.health()) <= m.n("Health threshold");
    let hunger = f64::from(p.health.food.food) <= m.n("Hunger threshold");
    match m.p("Threshold mode") {
        "Health" => health,
        "Hunger" => hunger,
        "Both" => health && hunger,
        _ => health || hunger,
    }
}

// Food that is not on the blacklist and that you can eat now (hungry, or food you can always eat).
fn edible(game: &Game<'_>, m: &Module, stack: Option<&horizon_client::items::ItemStack>) -> bool {
    let Some(stack) = stack else {
        return false;
    };
    let Some(info) = game.res.items().get(&stack.item) else {
        return false;
    };
    let hungry = game.player.as_ref().is_some_and(|p| p.health.food.food < 20);
    info.food.is_some() && !super::sunset_util::listed(m.t("Blacklist"), &stack.item) && (hungry || info.can_always_eat)
}

// Where to eat from: None for the off hand, else the inventory slot (0 to 8 the hotbar) of the best food.
fn food_slot(game: &Game<'_>, m: &Module) -> Option<Option<usize>> {
    if edible(game, m, game.offhand.as_ref()) {
        return Some(None);
    }
    let value = |i: usize| -> Option<f32> {
        let stack = game.inventory.get(i)?.as_ref();
        let (nutrition, saturation) = game.res.items().get(&stack?.item)?.food?;
        edible(game, m, stack).then(|| match m.p("Food priority") {
            "Combined" => nutrition as f32 + saturation,
            "Hunger" => nutrition as f32,
            _ => saturation,
        })
    };
    let best = |range: std::ops::Range<usize>| {
        range.filter_map(|i| value(i).map(|v| (i, v))).fold(None, |b: Option<(usize, f32)>, (i, v)| if b.is_none_or(|b| v > b.1) { Some((i, v)) } else { b })
    };
    let found = best(0..9).or_else(|| if m.b("Search inventory") { best(9..36) } else { None });
    found.map(|(i, _)| Some(i))
}

// Holds the food: the off hand as it is, a hotbar slot selected, or food from the inventory moved to an empty
// hotbar slot first. False when there is no room for it.
fn eat_from(game: &mut Game<'_>, slot: Option<usize>) -> bool {
    let at = match slot {
        None => {
            with(|s| s.eat_slot = None);
            return true;
        }
        Some(i) if i < 9 => i,
        Some(i) => {
            let Some(empty) = (0..9).find(|&h| game.inventory.get(h).is_some_and(Option::is_none)) else {
                return false;
            };
            menu_click(game, &[(i, horizon_client::menu::Input::Pickup), (empty, horizon_client::menu::Input::Pickup)]);
            empty
        }
    };
    hold(game, at);
    with(|s| s.eat_slot = Some(at));
    true
}

fn eat_stop(game: &mut Game<'_>) {
    use winit::event::MouseButton;
    let (prev, from_hand, aura) = with(|s| {
        s.eating = false;
        (s.eat_prev, s.eat_slot.is_none(), std::mem::take(&mut s.eat_paused_aura))
    });
    if !from_hand {
        hold(game, prev);
    }
    let _ = game.play_mouse(MouseButton::Right, false);
    if aura {
        toggle_named(game, "Kill Aura", true);
    }
}

fn toggle_named(game: &mut Game<'_>, name: &str, on: bool) {
    if let Some(i) = with(|s| s.find(name)) {
        toggle(game, i, on);
    }
}

pub(super) fn block_colour(preset: &str, name: &str) -> Option<u32> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    let ore = |n: &str| -> Option<u32> {
        let core = n.strip_prefix("deepslate_").unwrap_or(n);
        Some(match core {
            "diamond_ore" => 0xFF55_FFFF,
            "emerald_ore" => 0xFF55_FF55,
            "gold_ore" | "nether_gold_ore" => 0xFFFF_D040,
            "iron_ore" => 0xFFD8_AF93,
            "coal_ore" => 0xFF90_9090,
            "redstone_ore" => 0xFFFF_3030,
            "lapis_ore" => 0xFF30_50FF,
            "copper_ore" => 0xFFE0_7040,
            "nether_quartz_ore" => 0xFFEE_EEEE,
            _ => return None,
        })
    };
    match preset {
        "Ores" => ore(n).or((n == "ancient_debris").then_some(0xFFB0_6040)),
        "Diamond" => matches!(n, "diamond_ore" | "deepslate_diamond_ore").then_some(0xFF55_FFFF),
        "Ancient Debris" => (n == "ancient_debris").then_some(0xFFB0_6040),
        "Spawners" => matches!(n, "spawner" | "trial_spawner").then_some(0xFF80_40FF),
        "Portals" => matches!(n, "nether_portal" | "end_portal" | "end_portal_frame" | "end_gateway").then_some(0xFFB0_50FF),
        _ => None,
    }
}

// Block ESP's search of the loaded blocks around the player, spread over ticks: again when the player has moved, the
// settings changed, or it has been a while.
pub(super) fn scan_blocks(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(at) = block_pos(game) else {
        return;
    };
    let preset = m.p("Blocks");
    let range = m.n("Range") as i32;
    let kind = ["Ores", "Diamond", "Ancient Debris", "Spawners", "Portals"].iter().position(|p| *p == preset).unwrap_or(0);
    let key = (kind as u64) << 32 | range as u64;
    let facts = game.res.facts();
    with(|s| {
        s.blocks.tick(ticks, at, range, key, 100, 6, |c, x, out| {
            let ys = (c[1] - range).max(-64)..=(c[1] + range).min(320);
            for z in c[2] - range..=c[2] + range {
                for y in ys.clone() {
                    let state = game.world.block(x, y, z);
                    if facts.is_air(state) {
                        continue;
                    }
                    if let Some(col) = block_colour(preset, &facts.get(state).name) {
                        out.push(([x, y, z], col));
                    }
                }
            }
            (2 * range as usize + 1) * ys.count()
        })
    });
}

// A click or several in the player's own inventory (the menu with no
// container open), by the menu's slot numbers: 0 to 8 the hotbar, 9 to 35
// the rest, 500 to 503 the armor.
pub(super) fn menu_click(game: &mut Game<'_>, ops: &[(usize, horizon_client::menu::Input)]) {
    let ops: Vec<(Option<usize>, i32, horizon_client::menu::Input)> =
        ops.iter().map(|&(slot, input)| (Some(slot), 0, input)).collect();
    game.remote_menu_ops(&ops);
}

pub(super) fn inventory_idle(game: &Game<'_>) -> bool {
    game.native_playing() && game.remote.is_some() && game.remote_menu().1 == "inventory"
}

// A rough rank of an armor piece: its material, less what it has worn.
pub(super) fn armor_score(item: &str, damage: u32) -> f32 {
    let n = item.strip_prefix("minecraft:").unwrap_or(item);
    let tier = if n.starts_with("netherite") {
        6.0
    } else if n.starts_with("diamond") {
        5.0
    } else if n.starts_with("iron") {
        4.0
    } else if n.starts_with("chainmail") {
        3.0
    } else if n.starts_with("copper") {
        2.5
    } else if n.starts_with("golden") {
        2.0
    } else if n.starts_with("turtle") {
        2.0
    } else {
        1.0
    };
    tier - (damage as f32 / 2000.0).min(0.9)
}

// Puts the stack in inventory slot `i` on armor slot `a`: shift-click into an
// empty slot, else swap by picking both up.
pub(super) fn wear(game: &mut Game<'_>, i: usize, a: usize) {
    use horizon_client::menu::Input::{Pickup, QuickMove};
    if game.armor[a].is_none() {
        menu_click(game, &[(i, QuickMove)]);
    } else {
        menu_click(game, &[(i, Pickup), (500 + a, Pickup), (i, Pickup)]);
    }
}

pub(super) fn auto_armor(game: &mut Game<'_>, m: &Module) {
    if !inventory_idle(game) {
        return;
    }
    if with(|s| {
        s.armor_wait = s.armor_wait.saturating_sub(1);
        s.armor_wait > 0
    }) {
        return;
    }
    const SLOTS: [&str; 4] = ["head", "chest", "legs", "feet"];
    for (a, slot_name) in SLOTS.iter().enumerate() {
        let worn = game.armor[a].as_ref().map_or(0.0, |s| armor_score(&s.item, s.damage));
        let mut best: Option<(usize, f32)> = None;
        for i in 0..36 {
            let Some(st) = game.inventory.get(i).and_then(|s| s.as_ref()) else {
                continue;
            };
            if st.item.ends_with("elytra") {
                continue;
            }
            let fits = game
                .res
                .items()
                .get(&st.item)
                .is_some_and(|inf| inf.equip_slot.as_deref() == Some(*slot_name));
            let score = armor_score(&st.item, st.damage);
            if fits && score > worn && best.is_none_or(|(_, b)| score > b) {
                best = Some((i, score));
            }
        }
        if let Some((i, _)) = best {
            wear(game, i, a);
            with(|s| s.armor_wait = m.n("Delay (ticks)") as u32);
            return;
        }
    }
}

// Swaps the chestplate and the elytra, once.
pub(super) fn chest_swap(game: &mut Game<'_>) {
    if !inventory_idle(game) {
        return;
    }
    let wearing_elytra = game.armor[1].as_ref().is_some_and(|s| s.item.ends_with("elytra"));
    let found = (0..36).find(|&i| {
        game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| {
            if wearing_elytra {
                !s.item.ends_with("elytra")
                    && game.res.items().get(&s.item).is_some_and(|inf| inf.equip_slot.as_deref() == Some("chest"))
            } else {
                s.item.ends_with("elytra")
            }
        })
    });
    match found {
        Some(i) => wear(game, i, 1),
        None => say(game, if wearing_elytra { "No chestplate in your inventory." } else { "No elytra in your inventory." }),
    }
}

pub(super) fn auto_replenish(game: &mut Game<'_>, m: &Module) {
    use horizon_client::menu::Input::QuickMove;
    if !inventory_idle(game) {
        return;
    }
    if with(|s| {
        s.replenish_wait = s.replenish_wait.saturating_sub(1);
        s.replenish_wait > 0
    }) {
        return;
    }
    for h in 0..9 {
        let Some(st) = game.inventory.get(h).and_then(|s| s.as_ref()) else {
            continue;
        };
        let max = game.res.items().get(&st.item).map_or(64, |i| i.max_stack);
        if max <= 1 || st.count >= max || f64::from(st.count) > m.n("Threshold") {
            continue;
        }
        let item = st.item.clone();
        let source = (9..36).find(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| s.item == item));
        if let Some(src) = source {
            // Shift-click from the main inventory fills the hotbar stack first.
            menu_click(game, &[(src, QuickMove)]);
            with(|s| s.replenish_wait = m.n("Delay (ticks)") as u32);
            return;
        }
    }
}

// The hotbar tool that mines the block under the crosshair fastest, while
// the attack button is held; back to the old slot after.
// Auto Tool (Meteor's): while you break a block, the hotbar tool with the best score for it (Meteor's getScore:
// a tool that is right for the block's drops, by its speed and enchantments), after Switch delay; Switch back when you
// let go; Anti break lets go of a tool about to break.
pub(super) fn auto_tool(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    let down = game.native_playing() && (game.physical_buttons.contains(&MouseButton::Left) || game.left) && !gui_open();
    if !down {
        let back = with(|s| {
            s.tool_wait = None;
            s.tool_back.take()
        });
        if let Some(back) = back.filter(|_| m.b("Switch back")) {
            hold(game, back);
        }
        return;
    }
    if let Some((slot, left)) = with(|s| s.tool_wait) {
        if left == 0 {
            hold(game, slot);
            with(|s| s.tool_wait = None);
        } else {
            with(|s| s.tool_wait = Some((slot, left - 1)));
        }
    }
    if game.creative || game.remote.is_none() || game.entity_targeted() {
        return;
    }
    let Some(hit) = game.pick() else {
        return;
    };
    let name = game.res.facts().get(game.world.block(hit.pos[0], hit.pos[1], hit.pos[2])).name.clone();
    let list = if m.p("List mode") == "Whitelist" { m.t("Whitelist") } else { m.t("Blacklist") };
    let allowed = |stack: Option<&horizon_client::items::ItemStack>| {
        let listed = stack.is_some_and(|s| super::sunset_util::listed(list, &s.item));
        listed == (m.p("List mode") == "Whitelist")
    };
    let stack = |i: usize| game.inventory.get(i).and_then(|s| s.as_ref());
    let best = (0..9)
        .filter(|&i| allowed(stack(i)))
        .map(|i| (i, tool_score(game, m, stack(i), &name)))
        .filter(|(_, score)| *score >= 0.0)
        .fold(None, |b: Option<(usize, f64)>, (i, v)| if b.is_none_or(|b| v > b.1) { Some((i, v)) } else { b });
    let current = stack(game.selected);
    let switch = best
        .filter(|&(slot, score)| slot != game.selected && (score > tool_score(game, m, current, &name) || tool_spent(game, m, current) || !is_tool(current)))
        .map(|(slot, _)| slot);
    if let Some(slot) = switch {
        let selected = game.selected;
        with(|s| {
            s.tool_back.get_or_insert(selected);
        });
        match m.n("Switch delay") as u32 {
            0 => hold(game, slot),
            delay => with(|s| s.tool_wait = Some((slot, delay))),
        }
    }
    let held = game.inventory.get(game.selected).and_then(|s| s.as_ref());
    if tool_spent(game, m, held) && is_tool(held) {
        let _ = game.play_mouse(MouseButton::Left, false);
    }
}

fn is_tool(stack: Option<&horizon_client::items::ItemStack>) -> bool {
    stack.is_some_and(|s| ["_axe", "_hoe", "_pickaxe", "_shovel", ":shears"].iter().any(|t| s.item.ends_with(t)))
}

// Anti break: a tool worn down to its last Anti break percentage.
fn tool_spent(game: &Game<'_>, m: &Module, stack: Option<&horizon_client::items::ItemStack>) -> bool {
    let Some(s) = stack else {
        return false;
    };
    let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
    m.b("Anti break") && max > 0 && f64::from(max - s.damage.min(max)) < f64::from(max) * m.n("Anti break percentage") / 100.0
}

// Meteor's AutoTool.getScore: -1 for what is no tool for this block.
fn tool_score(game: &Game<'_>, m: &Module, stack: Option<&horizon_client::items::ItemStack>, block: &str) -> f64 {
    let Some(s) = stack.filter(|s| is_tool(Some(s)) && !tool_spent(game, m, Some(s))) else {
        return -1.0;
    };
    let items = game.res.items();
    let short = super::sunset_util::short(block);
    let shears = s.item.ends_with(":shears") && short.ends_with("_leaves");
    if !items.correct_for_drops(stack, block) && !shears && !short.ends_with("_wool") {
        return -1.0;
    }
    let level = |e: &str| f64::from(s.enchantment_level(e));
    if m.b("Silk touch for ender chest") && short == "ender_chest" && level("minecraft:silk_touch") == 0.0 {
        return -1.0;
    }
    let fortunable = (short.ends_with("_ore") && short != "ancient_debris") || matches!(short, "wheat" | "carrots" | "potatoes" | "beetroots" | "torchflower_crop");
    if m.b("Fortune for ores and crops") && fortunable && level("minecraft:fortune") == 0.0 {
        return -1.0;
    }
    let preferred = match m.p("Prefer") {
        "Fortune" => level("minecraft:fortune"),
        "Silk touch" => level("minecraft:silk_touch"),
        _ => 0.0,
    };
    f64::from(items.mining_speed(stack, block)) * 1000.0 + level("minecraft:unbreaking") + level("minecraft:efficiency") + level("minecraft:mending") + preferred
}

// Middle click: throws or uses the chosen item from the hotbar, and switches
// back a few ticks later.
pub(super) fn middle_click(game: &mut Game<'_>) {
    use winit::event::MouseButton;
    let Some(m) = with(|s| s.module("Middle Click Extra").filter(|m| m.on).cloned()) else {
        return;
    };
    let want = match m.p("Item") {
        "Pearl" => "ender_pearl",
        "Rocket" => "firework_rocket",
        _ => "experience_bottle",
    };
    let slot = (0..9).find(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| s.item.ends_with(want)));
    let Some(slot) = slot else {
        return;
    };
    let before = game.selected;
    hold(game, slot);
    let _ = game.play_mouse(MouseButton::Right, true);
    let _ = game.play_mouse(MouseButton::Right, false);
    with(|s| {
        if s.mc_back.is_none() {
            s.mc_back = Some((before, 3));
        }
    });
}

// Nuker's box for the Cube shape (Meteor's pos1 and pos2 by the way you face, with its offsets), block corners.
fn nuke_cube(game: &Game<'_>, m: &Module, at: [f64; 3]) -> [[f64; 3]; 2] {
    let n = |k: &str| m.n(k);
    let (x, y, z) = (at[0], at[1], at[2]);
    let (top, bottom) = ((y + n("Up") + 1.0).ceil(), y.ceil() - n("Down"));
    // Yaw 0 faces south (+z), 90 west, 180 north, -90 east.
    let facing = ((f64::from(game.yaw) / 90.0).round() as i32).rem_euclid(4);
    let (lo, hi) = match facing {
        0 => ([x + 1.0 - (n("Right") + 1.0), z + 1.0 - (n("Back") + 1.0)], [x + 1.0 + n("Left"), z + 1.0 + n("Forward")]),
        1 => ([x - n("Forward"), z - n("Right")], [x + n("Back") + 1.0, z + n("Left") + 1.0]),
        2 => ([x + 1.0 - (n("Left") + 1.0), z + 1.0 - (n("Forward") + 1.0)], [x + 1.0 + n("Right"), z + 1.0 + n("Back")]),
        _ => ([x + 1.0 - (n("Back") + 1.0), z - n("Left")], [x + 1.0 + n("Forward"), z + n("Right") + 1.0]),
    };
    let bottom = if m.p("Mode") == "Flatten" { (y + 0.5).floor() } else { bottom };
    // BlockPos.set floors each coordinate; the box runs between the two blocks' middles.
    let lo = [lo[0].floor() + 0.5, bottom.floor() + 0.5, lo[1].floor() + 0.5];
    let hi = [hi[0].floor() + 0.5, top.floor() + 0.5, hi[1].floor() + 0.5];
    [std::array::from_fn(|i| lo[i].min(hi[i])), std::array::from_fn(|i| lo[i].max(hi[i]))]
}

// Nuker (Meteor's): every block in its shape (a sphere of Range round your feet, a box by Up, Down and the sides,
// or a cube of whole blocks), above your feet in Flatten mode and only the instantly broken in Smash, within reach
// (behind a block, within Walls range) and the lists allow; sorted, then broken: as many as Max blocks per tick of
// those that break at once, else the first, mined over the ticks it takes (Packet mine starts and stops them all).
// Interact right-clicks them instead.
// How long a block Nuker sent waits for the server's update before it is tried again.
const NUKE_WAIT: u64 = 40;

pub(super) fn nuker(game: &mut Game<'_>, m: &Module) {
    with(|s| {
        s.nuke_broken.retain_mut(|(_, t)| {
            *t = t.saturating_sub(1);
            *t > 0
        })
    });
    if !game.native_playing() || game.remote.is_none() {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    // A block being mined the slow way: stop it when its time is up, and start the next one in the same tick.
    let now = game.ticks;
    if let Some((pos, left)) = with(|s| s.nuke) {
        if left <= 1 {
            let seq = super::sunset_util::sequence(game);
            game.send(S::PlayerAction(play::PlayerAction { action: 3, pos, direction: 1, sequence: seq }));
            // The block is still there until the server's update arrives: leave it alone meanwhile.
            with(|s| {
                s.nuke = None;
                s.nuke_skip.push((pos, now + NUKE_WAIT));
            });
        } else {
            with(|s| s.nuke = Some((pos, left - 1)));
            return;
        }
    }
    if with(|s| {
        let wait = s.nuke_timer > 0;
        s.nuke_timer = s.nuke_timer.saturating_sub(1);
        wait
    }) {
        return;
    }
    // Blocks sent wait for the server's update (the block gone here) or, should the server refuse, NUKE_WAIT ticks.
    let skip = with(|s| {
        let facts = game.res.facts();
        s.nuke_skip.retain(|&(p, until)| until > now && !facts.is_air(game.world.block(p[0], p[1], p[2])));
        s.nuke_skip.iter().map(|&(pos, _)| pos).collect::<Vec<_>>()
    });
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let at = [p.position.x, p.position.y, p.position.z];
    let feet = at.map(|v| v.floor() as i32);
    let (shape, mode, range) = (m.p("Shape"), m.p("Mode"), m.n("Range"));
    let r = range.round() as i32;
    let cube = (shape == "Cube").then(|| nuke_cube(game, m, at));
    with(|s| s.nuke_box = cube);
    let reach = match shape {
        "Uniform cube" => r + 1,
        "Cube" => 1 + ["Up", "Down", "Left", "Right", "Forward", "Back"].iter().map(|k| m.n(k) as i32).max().unwrap_or(0),
        _ => range.ceil() as i32 + 1,
    };
    let (interact, suitable) = (m.b("Interact"), m.b("Only suitable tools"));
    let list = if m.p("List mode") == "Whitelist" { m.t("Whitelist") } else { m.t("Blacklist") };
    let interacted = with(|s| s.nuke_interacted.clone());
    let facts = game.res.facts();
    let mut found: Vec<([i32; 3], bool, u32)> = Vec::new();
    for x in feet[0] - reach..=feet[0] + reach {
        for y in feet[1] - reach..=feet[1] + reach {
            for z in feet[2] - reach..=feet[2] + reach {
                let b = [x, y, z];
                let c = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
                let inside = match shape {
                    "Uniform cube" => (0..3).map(|i| (b[i] - feet[i]).abs()).max().unwrap_or(0) < r,
                    "Cube" => cube.is_some_and(|[lo, hi]| (0..3).all(|i| c[i] >= lo[i] && c[i] <= hi[i])),
                    _ => (0..3).map(|i| (c[i] - at[i]).powi(2)).sum::<f64>() <= range * range,
                };
                if !inside || (mode == "Flatten" && c[1] < at[1]) || skip.contains(&b) {
                    continue;
                }
                let state = game.world.block(x, y, z);
                if facts.is_air(state) {
                    continue;
                }
                let f = facts.get(state);
                if mode == "Smash" && f.hardness != 0.0 {
                    continue;
                }
                if suitable && !interact && !game.res.items().correct_for_drops(game.held(), &f.name) {
                    continue;
                }
                let fluid = f.name.ends_with("water") || f.name.ends_with("lava");
                if !interact && (f.hardness < 0.0 || fluid) {
                    continue;
                }
                // Behind a block (the line to its middle meets another first): only within Walls range.
                if !super::sunset_util::clear_line(game, c, b) && super::sunset_util::dist(eye, c) > m.n("Walls range") {
                    continue;
                }
                if super::sunset_util::listed(list, &f.name) != (m.p("List mode") == "Whitelist") {
                    continue;
                }
                if interact && interacted.contains(&b) {
                    continue;
                }
                let speed = game.res.items().mining_speed(game.held(), &f.name);
                let per_tick = if f.hardness <= 0.0 { 1.0 } else { speed / f.hardness / if speed > 1.0 { 30.0 } else { 100.0 } };
                let instant = game.creative || per_tick >= 1.0;
                if m.b("Instant only") && !instant {
                    continue;
                }
                // The server lets a stop through once the progress reaches 0.7, counting the tick it started in.
                let ticks = if instant { 0 } else { ((0.7 / per_tick).ceil() as u32).max(1) };
                found.push((b, instant, ticks));
            }
        }
    }
    let far = |b: &[i32; 3]| (0..3).map(|i| (f64::from(b[i]) + 0.5 - at[i]).powi(2)).sum::<f64>();
    match m.p("Sort mode") {
        "Top down" => found.sort_by_key(|f| -f.0[1]),
        "Closest" => found.sort_by(|a, b| far(&a.0).total_cmp(&far(&b.0))),
        "Furthest" => found.sort_by(|a, b| far(&b.0).total_cmp(&far(&a.0))),
        _ => {}
    }
    let delay = m.n("Delay") as u32;
    let Some(first) = found.first().map(|f| f.0) else {
        with(|s| {
            s.nuke_interacted.clear();
            s.nuke_idle += 1;
            if s.nuke_idle > delay {
                s.nuke_first = true;
            }
        });
        return;
    };
    let wait = with(|s| {
        s.nuke_idle = 0;
        let mut wait = false;
        if !s.nuke_first && s.nuke_last != Some(first) {
            s.nuke_timer = delay;
            s.nuke_last = Some(first);
            wait = delay > 0;
        }
        wait
    });
    if wait {
        return;
    }
    // No more blocks awaiting the server's answer than two ticks' worth: a server that falls behind is not flooded
    // with blocks sent again and again (Horizon's dropped a client 8000 packets behind).
    let (max, packet) = (m.n("Max blocks per tick") as usize, m.b("Packet mine"));
    let max = max.min((2 * max).saturating_sub(skip.len()));
    let mut done = 0;
    let mut sent: Vec<([i32; 3], u64)> = Vec::new();
    // What goes to the server, after the look when Rotate is on.
    let mut out: Vec<S> = Vec::new();
    for (pos, instant, ticks) in found {
        if done >= max {
            break;
        }
        if interact {
            let seq = super::sunset_util::sequence(game);
            let face = if eye[1] > f64::from(pos[1]) + 1.0 { 1 } else { 0 };
            out.push(S::UseItemOn(play::UseItemOn { hand: 0, pos, face, cursor: [0.5, 0.5, 0.5], inside: true, world_border: false, sequence: seq }));
            with(|s| s.nuke_interacted.push(pos));
        } else {
            let seq = super::sunset_util::sequence(game);
            out.push(S::PlayerAction(play::PlayerAction { action: 0, pos, direction: 1, sequence: seq }));
            if packet && !instant {
                let seq = super::sunset_util::sequence(game);
                // 26.x numbers the actions START 0, ABORT 2, STOP 3.
                out.push(S::PlayerAction(play::PlayerAction { action: 3, pos, direction: 1, sequence: seq }));
            } else if !instant {
                with(|s| s.nuke = Some((pos, ticks)));
            }
            // The block shows until the server's update comes back: next tick go on to new ones, not these again.
            sent.push((pos, now + NUKE_WAIT));
        }
        with(|s| {
            s.nuke_last = Some(pos);
            if m.b("Broken blocks") {
                s.nuke_broken.push((pos, 8));
            }
        });
        done += 1;
        if !instant && !packet && !interact {
            break;
        }
    }
    with(|s| {
        s.nuke_skip.extend(sent);
        s.nuke_first = false;
    });
    if done == 0 {
        return;
    }
    let swing = m.b("Swing");
    let send = move |game: &mut Game<'_>| {
        for p in out {
            game.send(p);
        }
        if swing {
            game.swing_arm();
        }
        game.send(S::Punch(Empty));
    };
    if m.b("Rotate") {
        let (yaw, pitch) = look_at(eye, super::sunset_util::center_of(first));
        turn_then("Nuker", yaw, pitch, send);
    } else {
        send(game);
    }
}

// Players who left the list since they were last seen: where they stood.
pub(super) fn track_logouts(game: &Game<'_>, m: &Module) {
    let Some(online) = game.remote_online_names() else {
        return;
    };
    let dim = game.dim.id().to_owned();
    let mut seen: Vec<(String, [f64; 3], String)> = Vec::new();
    for (id, name) in game.remote_player_names() {
        if let Some(mob) = game.mobs.iter().find(|mob| mob.id == id) {
            seen.push((name, [mob.position.x, mob.position.y, mob.position.z], dim.clone()));
        }
    }
    let now = game.ticks;
    let keep = (m.n("Minutes") * 1200.0) as u64;
    with(|s| {
        for (name, pos, d) in s.last_pos.clone() {
            if !online.iter().any(|n| *n == name) {
                s.spots.retain(|sp| sp.name != name);
                s.spots.push(Spot { name, pos, dim: d, at: now });
            }
        }
        s.last_pos.retain(|(n, _, _)| online.contains(n) && !seen.iter().any(|(sn, _, _)| sn == n));
        s.last_pos.extend(seen);
        s.spots.retain(|sp| now.saturating_sub(sp.at) < keep);
    });
}

pub(super) fn bow_spam(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    if !game.native_playing()
        || !(game.physical_buttons.contains(&MouseButton::Right) || game.right)
        || !game.held().is_some_and(|s| &*s.item == "minecraft:bow")
    {
        return;
    }
    if let Some((_, ticks)) = game.using
        && f64::from(ticks) >= m.n("Charge (ticks)")
    {
        let _ = game.play_mouse(MouseButton::Right, false);
        let _ = game.play_mouse(MouseButton::Right, true);
    }
}

// Join and leave messages, from the server's player list.
pub(super) fn notify(game: &mut Game<'_>, m: &Module) {
    let Some(mut now) = game.remote_online_names() else {
        return;
    };
    now.sort();
    let Some(before) = with(|s| s.notify.replace(now.clone())) else {
        return;
    };
    for name in now.iter().filter(|n| !before.contains(n)) {
        if m.b("Join") {
            say(game, &format!("{name} joined"));
        }
    }
    for name in before.iter().filter(|n| !now.contains(n)) {
        if m.b("Leave") {
            say(game, &format!("{name} left"));
        }
    }
}

// Hole ESP (Meteor's): open blocks Min height tall, walled below and on the four sides by bedrock (unbreakable) or
// blast-proof blocks (obsidian and the like), or with Doubles two such side by side. Searched again every second, a
// few slices a tick.
pub(super) fn scan_holes(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(at) = block_pos(game) else {
        return;
    };
    let (h, v) = (m.n("Horizontal radius") as i32, m.n("Vertical radius") as i32);
    let (height, doubles, own, webs) = (m.n("Min height") as i32, m.b("Doubles"), m.b("Ignore own"), m.b("Webs"));
    let facts = game.res.facts();
    let solid = |p: [i32; 3]| {
        let state = game.world.block(p[0], p[1], p[2]);
        !facts.is_air(state) && facts.get(state).full_collision
    };
    let open = |p: [i32; 3]| {
        let state = game.world.block(p[0], p[1], p[2]);
        !(own && p == at) && (webs || !facts.get(state).name.ends_with(":cobweb")) && (0..height).all(|i| !solid([p[0], p[1] + i, p[2]]))
    };
    // 0 bedrock, 1 blast-proof, None anything else.
    let wall = |p: [i32; 3]| {
        let state = game.world.block(p[0], p[1], p[2]);
        let f = facts.get(state);
        if facts.is_air(state) {
            None
        } else if f.full_collision && f.hardness < 0.0 {
            Some(0u8)
        } else if f.explosion_resistance >= 600.0 && f.hardness >= 0.0 {
            Some(1u8)
        } else {
            None
        }
    };
    const SIDES: [[i32; 3]; 5] = [[0, -1, 0], [1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
    let plus = |p: [i32; 3], d: [i32; 3]| [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
    let key = (h as u64) | (v as u64) << 8 | (height as u64) << 16 | u64::from(doubles) << 24 | u64::from(own) << 25 | u64::from(webs) << 26;
    with(|s| {
        s.holes.tick(ticks, at, h, key, 20, 4, |c, x, out| {
            for z in c[2] - h..=c[2] + h {
                'cell: for y in c[1] - v..=c[1] + v {
                    let p = [x, y, z];
                    if !open(p) {
                        continue;
                    }
                    let (mut walls, mut kinds, mut pair) = (0, [0u8; 2], false);
                    for d in SIDES {
                        let q = plus(p, d);
                        if let Some(k) = wall(q) {
                            walls += 1;
                            kinds[usize::from(k)] += 1;
                        } else if d[1] == -1 || !doubles || pair || !open(q) {
                            continue 'cell;
                        } else {
                            // The other half of a double: walled all round but towards this one.
                            for e in SIDES.iter().filter(|e| **e != [-d[0], 0, -d[2]]) {
                                match wall(plus(q, *e)) {
                                    Some(k) => {
                                        walls += 1;
                                        kinds[usize::from(k)] += 1;
                                    }
                                    None => continue 'cell,
                                }
                            }
                            pair = true;
                        }
                    }
                    if walls == if pair { 8 } else { 5 } {
                        let kind = if kinds[1] == 0 { 0 } else if kinds[0] == 0 { 1 } else { 2 };
                        out.push((p, kind));
                    }
                }
            }
            (2 * h as usize + 1) * (2 * v as usize + 1) * 6
        })
    });
}

pub(super) fn remember_velocity(game: &Game<'_>) {
    let v = game.player.as_ref().map(|p| [p.velocity.x, p.velocity.y, p.velocity.z]);
    with(|s| s.velocity = v);
}

// A correction replaces the player's position: Speed's strafe and Scaffold's waits start again. (Velocity's baseline
// stays: Horizon's server can bring a hit's knockback with a position packet, which Velocity has to see.)
pub(super) fn server_correction(game: &mut Game<'_>) {
    if !game.native_on(ID) { return; }
    with(|s| {
        s.knockback_ticks = 0;
        s.strafe = Strafe::default();
        s.scaffold_recent.clear();
        s.scaffold_wait = 0;
    });
    horizon_client::motion::sunset_motion::set_across(None);
    horizon_client::motion::sunset_motion::set_up(None);
}

fn velocity_share(m: &Module, name: &str) -> f64 {
    let blend = if m.p("Mode") == "Blend" { m.n("Blend %") } else { 100.0 };
    super::sunset_movement::velocity_factor(m.n(name) * 100.0, blend)
}

fn knockback_shares(m: &Module, ground: bool) -> [f64; 2] {
    let (h, v) = if !ground && m.b("Use air values") {
        ("Air knockback horizontal", "Air knockback vertical")
    } else {
        ("Knockback horizontal", "Knockback vertical")
    };
    [velocity_share(m, h), velocity_share(m, v)]
}

// Flight (Meteor's) before the tick: hovering after a fall of three blocks, nudge the yaw a degree either way so the
// look the server sees moves (put back after the tick); in Velocity mode set the speed the air gives you, and with
// No sneak let go of sneaking (its key still takes you down).
fn flight_pre(game: &mut Game<'_>, m: Option<&Module>) {
    use horizon_client::motion::sunset_motion as motion;
    let Some(m) = m else {
        motion::set_flying_speed(None);
        let (delay, off) = (20, 1);
        with(|s| {
            (s.fly_delay, s.fly_off) = (delay, off);
            if std::mem::take(&mut s.unsneaked) && game.physical_keys.contains(&KeyCode::ShiftLeft) {
                game.keys.insert(KeyCode::ShiftLeft);
            }
        });
        FLY_DOWN.store(false, Ordering::Relaxed);
        FLY_LAST_Y.store(u64::MAX, Ordering::Relaxed);
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let yaw = game.yaw;
    let still = p.velocity.x.hypot(p.velocity.y).hypot(p.velocity.z) < 0.003;
    let (fallen, sprinting) = (p.fall_distance, p.sprinting);
    let nudge = with(|s| {
        let nudge = (fallen >= 3.0 && yaw == s.fly_last_yaw && still).then(|| if s.fly_flip { 1.0 } else { -1.0 });
        if nudge.is_some() {
            s.fly_flip = !s.fly_flip;
        }
        s.fly_last_yaw = yaw;
        nudge
    });
    if let Some(d) = nudge {
        game.yaw = yaw + d;
    }
    let velocity = m.p("Mode") == "Velocity";
    let speed = m.n("Speed") as f32;
    motion::set_flying_speed(velocity.then(|| speed * if sprinting { 15.0 } else { 10.0 }));
    if velocity && m.b("No sneak") && game.keys.remove(&KeyCode::ShiftLeft) {
        with(|s| s.unsneaked = true);
    }
}

// Flight after the tick: Meteor's anti kick (Packet: every `Delay` ticks the next move packets sink a little for
// `Off time` ticks, Normal: the flying stops for those ticks), the yaw put back, then the mode: Abilities flies as
// creative does at Speed; Velocity holds you still but for the jump and sneak keys, and Speed sets the air's pull.
fn flight_post(game: &mut Game<'_>, m: Option<&Module>, jump: bool, sneak: bool, screen_dir: Option<[f64; 2]>) {
    let Some(m) = m else {
        return;
    };
    let (velocity, kick) = (m.p("Mode") == "Velocity", m.p("Anti kick mode"));
    let (delay, off_time) = (m.n("Delay") as i32, m.n("Off time") as i32);
    let (rest, remind, last_yaw) = with(|s| {
        let (mut rest, mut remind) = (false, false);
        if s.fly_delay > 0 {
            s.fly_delay -= 1;
        }
        if s.fly_off <= 0 && s.fly_delay <= 0 {
            (s.fly_delay, s.fly_off) = (delay, off_time);
            remind = kick == "Packet";
        } else if s.fly_delay <= 0 {
            rest = kick == "Normal" && !velocity;
            remind = kick == "Packet" && s.fly_off == off_time;
            s.fly_off -= 1;
        }
        FLY_DOWN.store(kick == "Packet" && s.fly_delay <= 0, Ordering::Relaxed);
        (rest, remind, s.fly_last_yaw)
    });
    if remind {
        horizon_client::player_sync::sunset_sync::remind();
    }
    let (creative, spectator) = (game.creative, game.spectator());
    let Some(p) = game.player.as_mut() else {
        return;
    };
    if rest {
        p.flying = false;
        p.flying_speed = 0.05;
        p.may_fly = creative || spectator;
        return;
    }
    game.yaw = last_yaw;
    let speed = m.n("Speed");
    if velocity {
        p.flying = false;
        let up = speed * if m.b("Vertical speed match") { 10.0 } else { 5.0 };
        let down = sneak || game.physical_keys.contains(&KeyCode::ShiftLeft);
        p.velocity = horizon_core::geometry::Vec3::new(0.0, f64::from(u8::from(jump)) * up - f64::from(u8::from(down)) * up, 0.0);
        // GUI Move: behind a menu the game takes no move keys, so the pull they would give is set by hand.
        if let Some([x, z]) = screen_dir {
            let pull = speed * if p.sprinting { 15.0 } else { 10.0 };
            (p.velocity.x, p.velocity.z) = (x * pull, z * pull);
        }
        if m.b("No sneak") {
            p.ground.on_ground = false;
        }
    } else if !spectator {
        p.flying_speed = speed as f32;
        p.flying = true;
        p.may_fly = true;
    }
}

// Speed's Strafe mode between moves: Meteor's stage, the last tick's distance, the speed, and the speed limit's clock.
#[derive(Default)]
pub(super) struct Strafe {
    stage: u8,
    distance: f64,
    speed: f64,
    since: Option<std::time::Instant>,
}

// Speed (Meteor's): this tick's own move across the ground, blocks a tick (and Strafe's hop up), or None to leave
// it to the game. Vanilla moves the setting's blocks a second the way the move keys point, and stands still without
// them; Strafe hops, faster at each jump, slowing in the air.
fn speed(game: &Game<'_>, m: &Module) -> Option<([f64; 2], Option<f64>)> {
    let p = game.player.as_ref()?;
    let held = |k: KeyCode| game.keys.contains(&k);
    let feet = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    let mode = m.p("Mode");
    let vanilla = mode == "Vanilla";
    let (flight, no_slow, pause) = with(|s| {
        let pause = s.knockback_ticks;
        s.knockback_ticks = pause.saturating_sub(1);
        let no_slow = s.module("No Slow").filter(|m| m.on).map(|m| [m.n("Ground %"), m.n("Air %")]);
        (s.is_on("Flight"), no_slow, pause)
    });
    let slow_factor = super::sunset_movement::item_use_factor(
        game.using.is_some(), no_slow, p.ground.on_ground,
    );
    if !super::sunset_movement::speed_can_write(
        flight || p.flying, p.fall_flying, game.using.is_some(), no_slow.is_some(),
        pause, p.collision.horizontal, m.b("Stop at wall"),
    ) { return None; }
    let scale_across = |v: [f64; 2]| [v[0] * slow_factor, v[1] * slow_factor];
    let effect = |id: &str| game.potions.effects.amplifier(id).map_or(0.0, |a| f64::from(a + 1));
    // Meteor's stopSpeed.
    if p.fall_flying
        || game.block_in_tag("minecraft:climbable", &game.block_name(feet))
        || game.riding.is_some()
        || (!m.b("When sneaking") && held(KeyCode::ShiftLeft))
        || (vanilla && m.b("Only on ground") && !p.ground.on_ground)
        || (!m.b("In liquids") && (p.fluids.was_touching_water || p.is_in_lava()))
    {
        return None;
    }
    if matches!(mode, "Smooth" | "Low Hop" | "Custom") {
        if m.b("Only on ground") && !p.ground.on_ground { return None; }
        let ground = p.ground.on_ground;
        let multiplier = if ground {
            if mode == "Custom" { m.n("Ground multiplier") } else { 1.0 }
        } else { m.n("Air multiplier") };
        let sprint = if p.sprinting { m.n("Sprint multiplier") } else { 1.0 };
        let target = m.n("Vanilla speed") / 20.0 * multiplier * sprint;
        let dir = move_direction(game);
        let up = if mode == "Low Hop" && ground && dir.is_some()
            && !held(KeyCode::Space) && !held(KeyCode::ShiftLeft)
        { Some(m.n("Jump height")) } else { None };
        let across = if mode == "Low Hop" {
            dir.map_or([0.0; 2], |d| {
                let speed = target.min(m.n("Max speed"));
                [d[0] * speed, d[1] * speed]
            })
        } else {
            super::sunset_movement::steer(
                [p.velocity.x, p.velocity.z], dir, target, m.n("Acceleration"),
                m.n("Deceleration"), m.n("Strafe strength"), m.n("Max speed"),
            )
        };
        return Some((scale_across(across), up));
    }
    let yaw = f64::from(game.yaw).to_radians();
    if vanilla {
        // PlayerUtils.getHorizontalVelocity, with the Speed effect's boost.
        let (forward, right) = ([-yaw.sin(), yaw.cos()], [-yaw.cos(), -yaw.sin()]);
        let f = f64::from(i8::from(held(KeyCode::KeyW)) - i8::from(held(KeyCode::KeyS)));
        let r = f64::from(i8::from(held(KeyCode::KeyD)) - i8::from(held(KeyCode::KeyA)));
        let diagonal = if (held(KeyCode::KeyW) || held(KeyCode::KeyS)) && (held(KeyCode::KeyA) || held(KeyCode::KeyD)) { std::f64::consts::FRAC_1_SQRT_2 } else { 1.0 };
        let k = m.n("Vanilla speed") / 20.0 * diagonal * (1.0 + effect("speed") * 0.205);
        return Some((scale_across([(forward[0] * f + right[0] * r) * k, (forward[1] * f + right[1] * r) * k]), None));
    }
    let base = 0.2873 * (1.0 + 0.2 * effect("speed")) / (1.0 + 0.2 * effect("slowness"));
    let forward = f64::from(i8::from(held(KeyCode::KeyW)) - i8::from(held(KeyCode::KeyS)));
    let side = f64::from(i8::from(held(KeyCode::KeyA)) - i8::from(held(KeyCode::KeyD)));
    let moving = forward != 0.0 || side != 0.0;
    let distance = (p.position.x - game.previous_position.x).hypot(p.position.z - game.previous_position.z);
    let (ground, landing) = (p.ground.on_ground, p.ground.on_ground || p.collision.vertical);
    let (strafe_speed, limit) = (m.n("Strafe speed"), m.b("Speed limit"));
    let (speed, up) = with(|s| {
        let st = &mut s.strafe;
        st.distance = distance;
        let mut up = None;
        match st.stage {
            0 => {
                if moving {
                    st.stage = 1;
                    st.speed = 1.18 * base - 0.01;
                }
            }
            1 => {
                if moving && ground {
                    up = Some(0.40123128 + effect("jump_boost") * 0.1);
                    st.speed *= strafe_speed;
                    st.stage = 2;
                }
            }
            2 => {
                st.speed = st.distance - 0.76 * (st.distance - base);
                st.stage = 3;
            }
            _ => {
                if landing {
                    st.stage = 0;
                }
                st.speed = st.distance - st.distance / 159.0;
            }
        }
        st.speed = st.speed.max(base);
        if limit {
            let now = std::time::Instant::now();
            let since = *st.since.get_or_insert(now);
            if now.duration_since(since).as_millis() > 2500 {
                st.since = Some(now);
            }
            let held_for = now.duration_since(st.since.unwrap_or(now)).as_millis();
            st.speed = st.speed.min(if held_for > 1250 { 0.44 } else { 0.43 });
        }
        (st.speed, up)
    });
    // Strafe.transformStrafe: the speed the way the move keys point.
    if !moving {
        return Some((scale_across([0.0; 2]), up));
    }
    let mut turn = 90.0 * side.signum();
    if forward != 0.0 {
        turn *= forward.signum() * 0.5;
    }
    let mut dir = f64::from(game.yaw) - turn;
    if forward < 0.0 {
        dir -= 180.0;
    }
    let dir = dir.to_radians();
    Some((scale_across([-dir.sin() * speed, dir.cos() * speed]), up))
}

// An explosion packet's push on the player, as it arrives (before the game adds it).
pub(super) fn explosion_push(game: &mut Game<'_>, push: Option<[f64; 3]>) {
    if let Some(push) = push {
        let shares = with(|s| {
            s.knockback_ticks = s.module("Speed").filter(|m| m.on).map_or(0, |m| m.n("Knockback pause") as u32);
            s.module("Velocity").filter(|m| m.on && m.b("Explosions"))
                .map(|m| [velocity_share(m, "Explosions horizontal"), velocity_share(m, "Explosions vertical")])
        });
        horizon_client::motion::sunset_motion::set_across(None);
        horizon_client::motion::sunset_motion::set_up(None);
        if let (Some([h, v]), Some(p)) = (shares, game.player.as_mut()) {
            // Horizon adds the original push after this hook; this leaves the chosen share.
            p.velocity.x += push[0] * (h - 1.0);
            p.velocity.y += push[1] * (v - 1.0);
            p.velocity.z += push[2] * (h - 1.0);
            let after = [p.velocity.x + push[0], p.velocity.y + push[1], p.velocity.z + push[2]];
            with(|s| { s.velocity = Some(after); s.blast = [0.0; 3]; });
        } else {
            with(|s| (0..3).for_each(|i| s.blast[i] += push[i]));
        }
    }
}

// Velocity (Meteor's), before the tick moves the player: of what changed the velocity since the last tick, the
// explosions' pushes keep their share and the rest, the server's knockback, its own; flowing liquids push by their
// share (the flow hook), and of the entities pushing you only the push amount is left.
fn velocity(game: &mut Game<'_>, m: Option<&Module>) {
    use horizon_client::motion::sunset_motion as motion;
    let (last, blast) = with(|s| (s.velocity.take(), std::mem::take(&mut s.blast)));
    if let (Some(last), Some(p)) = (last, game.player.as_ref()) {
        let impulse = [p.velocity.x - last[0] - blast[0], p.velocity.y - last[1] - blast[1], p.velocity.z - last[2] - blast[2]];
        if game.remote.is_some() && impulse[0].hypot(impulse[2]).hypot(impulse[1]) > 0.08 {
            with(|s| s.knockback_ticks = s.module("Speed").filter(|m| m.on).map_or(0, |m| m.n("Knockback pause") as u32));
        }
    }
    let share = |on: &str, h: &str, v: &str| m.filter(|m| m.b(on)).map_or([1.0; 2], |m| [velocity_share(m, h), velocity_share(m, v)]);
    let liquids = m.filter(|m| m.b("Liquids")).map(|m| [velocity_share(m, "Liquids horizontal"), velocity_share(m, "Liquids vertical")]);
    motion::set_liquid_flow(liquids);
    let Some(m) = m else {
        return;
    };
    let pushed = if m.b("Entity push") { entity_push(game) } else { [0.0; 2] };
    let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
    let knock = if m.b("Knockback") { knockback_shares(m, ground) } else { [1.0; 2] };
    let blasts = share("Explosions", "Explosions horizontal", "Explosions vertical");
    let Some(p) = game.player.as_mut() else {
        return;
    };
    let mut v = [p.velocity.x, p.velocity.y, p.velocity.z];
    if let Some(last) = last {
        for i in 0..3 {
            let (k, b) = if i == 1 { (knock[1], blasts[1]) } else { (knock[0], blasts[0]) };
            v[i] = last[i] + (v[i] - last[i] - blast[i]) * k + blast[i] * b;
        }
    }
    // The game adds the pushes later in the tick: take back what the push amount does not keep.
    let keep = velocity_share(m, "Entity push amount");
    v[0] -= pushed[0] * (1.0 - keep);
    v[2] -= pushed[1] * (1.0 - keep);
    (p.velocity.x, p.velocity.y, p.velocity.z) = (v[0], v[1], v[2]);
}

// The entities' push on the player this tick, x and z (the game's `LivingEntity.pushEntities` with
// `Entity.push(Entity)`): every living one but bats whose box overlaps yours, pushable by your team.
fn entity_push(game: &Game<'_>) -> [f64; 2] {
    let Some(p) = game.player.as_ref() else {
        return [0.0; 2];
    };
    let (bounds, own) = (p.bounds(), own_id(game));
    let team = game.score.board.players_team(&game.options.player_name());
    let mut push = [0.0; 2];
    for m in &game.mobs {
        if Some(m.id) == own
            || !m.alive()
            || m.species() == horizon_client::mobs::Species::Bat
            || game.riding.is_some_and(|v| v == m.id || m.extra.mount == Some(v))
            || !m.bounds().intersects(bounds)
            || !horizon_client::scoreboard::pushable_by(game.mob_team(m.id), team, true)
        {
            continue;
        }
        let (mut xa, mut za) = (m.position.x - p.position.x, m.position.z - p.position.z);
        let dd = xa.abs().max(za.abs());
        if dd < f64::from(0.01_f32) {
            continue;
        }
        let dd = dd.sqrt();
        let pow = (1.0 / dd).min(1.0) * f64::from(0.05_f32);
        xa = xa / dd * pow;
        za = za / dd * pow;
        push[0] -= xa;
        push[1] -= za;
    }
    push
}

pub(super) fn cooldown_ready(game: &Game<'_>, m: &Module, wait: &mut u32) -> bool {
    if m.b("Wait for cooldown") {
        // The ticker the hand counts on a server (attack_ticks is the
        // local world's).
        game.hand_strength.scale(0.5, game.main_hand_attack_speed()) >= 1.0
    } else if *wait > 0 {
        *wait -= 1;
        false
    } else {
        true
    }
}

// How Criticals makes a hit critical, when it can: a hop of so many blocks over two ticks (Packet, the mace's
// smash), or a wait for the fall (Jump, Mini jump, a hit while rising).
#[derive(Clone, Copy)]
enum Crit {
    Hop(f64),
    Jump,
    MiniJump,
    Fall,
}

// Criticals (Meteor's) for a hit now: never flying, gliding, riding, in water or lava or on a ladder. A mace (Mace
// smash) hops 1.501 blocks plus Additional height so the server counts a smash; on the ground the Mode; in the air,
// with Only on ground off, a hit while rising waits for the fall (one already falling is critical as it is). A hop
// needs the room above you, else Packet's goes back to 0.0625.
fn crit_plan(game: &Game<'_>, m: &Module) -> Option<Crit> {
    use horizon_core::geometry::Vec3;
    let p = game.player.as_ref()?;
    let item = game.held().map_or("", |s| &*s.item);
    let weapon = ["_sword", "_axe", "_spear", ":mace", ":trident"].iter().any(|w| item.ends_with(w));
    if (m.b("Only with a weapon") && !weapon)
        || p.flying
        || p.fall_flying
        || game.riding.is_some()
        || p.swimming
        || p.fluids.was_touching_water
        || p.is_in_lava()
        || p.last_climbable.is_some()
    {
        return None;
    }
    let extra = m.n("Additional height");
    let room = |h: f64| {
        horizon_client::collisions::SceneCollisions { world: &game.world, binding: game.collision, subject: None }
            .unobstructed(p.bounds().expand_towards(Vec3::new(0.0, h, 0.0)))
            .unwrap_or(false)
    };
    if m.b("Mace smash") && item.ends_with(":mace") && room(1.501 + extra) {
        return Some(Crit::Hop(1.501 + extra));
    }
    if !p.ground.on_ground {
        return (!m.b("Only on ground") && p.velocity.y > 0.0).then_some(Crit::Fall);
    }
    Some(match m.p("Mode") {
        "Jump" => Crit::Jump,
        "Mini jump" => Crit::MiniJump,
        _ => Crit::Hop(if room(0.0625 + extra) { 0.0625 + extra } else { 0.0625 }),
    })
}

pub(super) fn attack(game: &mut Game<'_>, id: u32, crits: bool, rotate: Option<(f32, f32)>) {
    let plan = with(|s| s.module("Criticals").filter(|m| m.on).cloned()).filter(|_| crits).and_then(|m| crit_plan(game, &m));
    let Some(plan) = plan else {
        match rotate {
            Some((yaw, pitch)) => turn_then("Attack", yaw, pitch, move |game| strike(game, id)),
            None => strike(game, id),
        }
        return;
    };
    with(|s| {
        s.crit_target = Some(id);
        s.crit_ticks = 0;
        s.crit_falls = 0;
        s.crit_look = rotate;
    });
    match plan {
        Crit::Hop(height) => {
            CRIT_HOP.store(height.to_bits(), Ordering::Relaxed);
            // The look rides on the hop's packets, and the hit follows them.
            if let Some((yaw, pitch)) = rotate {
                turn("Attack", yaw, pitch);
            }
            CRIT_STAGE.store(1, Ordering::Relaxed);
            return;
        }
        Crit::Jump => CRIT_JUMP.store(true, Ordering::Relaxed),
        Crit::MiniJump => {
            if let Some(p) = game.player.as_mut() {
                p.velocity.y = 0.25;
            }
        }
        Crit::Fall => {}
    }
    CRIT_STAGE.store(4, Ordering::Relaxed);
}

// Criticals on your own clicks (the attack hook): the hit the game would make on a mob, nearer than a block, a
// projectile it would turn or an object, waits as Kill Aura's does, and the click does nothing more now; while a hit
// waits, clicks do nothing. None: the game's own attack.
pub(super) fn crit_click(game: &mut Game<'_>) -> Option<bool> {
    let m = with(|s| s.module("Criticals").filter(|m| m.on && !m.b("Only Kill Aura")).cloned())?;
    if game.remote.is_none() || game.spectator() || game.clicks.miss_time > 0 {
        return None;
    }
    if CRIT_STAGE.load(Ordering::Relaxed) != 0 {
        return Some(false);
    }
    if game.held().is_some_and(|s| horizon_client::spear::Kinetic::of(&s.item).is_some()) {
        return None;
    }
    let (i, distance) = game.mob_hit()?;
    let projectile = game.redirectable_hit().is_some_and(|(_, d)| d <= distance);
    let object = game.remote_object_hit(true).is_some_and(|hit| hit.distance < distance);
    if projectile || object || !game.before_block(distance) {
        return None;
    }
    crit_plan(game, &m)?;
    let id = game.mobs[i].id;
    attack(game, id, true, None);
    Some(false)
}

pub(super) fn strike(game: &mut Game<'_>, id: u32) {
    if let Some(m) = with(|s| s.module("Auto Weapon").filter(|m| m.on).cloned())
        && let Some(slot) = best_weapon(game, m.b("Prefer axes"))
    {
        hold(game, slot);
    }
    game.send(S::Attack(play::Attack { id: id as i32 }));
    game.swing_arm();
    game.send(S::Punch(Empty));
    game.reset_attack_strength();
}

// The Criticals hop, a tick at a time. The regular position packet of a tick
// becomes the hop (see `outgoing`); when a tick has none, this sends it. Waiting
// for the fall, the hit goes on the second tick in a row that took you down off
// the ground (a server counts no fall on the first one after a jump's top), or
// anyway once you land again or a second passes.
pub(super) fn crit_tick(game: &mut Game<'_>) {
    let stage = CRIT_STAGE.load(Ordering::Relaxed);
    if stage == 0 {
        return;
    }
    let ticks = with(|s| {
        s.crit_ticks += 1;
        s.crit_ticks
    });
    if stage == 4 {
        let Some(p) = game.player.as_ref() else {
            return;
        };
        let (ground, falling) = (p.ground.on_ground, p.position.y < game.previous_position.y);
        let falls = with(|s| {
            s.crit_falls = if falling && !ground { s.crit_falls + 1 } else { 0 };
            s.crit_falls
        });
        if falls >= 2 || (ground && ticks > 3) || ticks > 20 {
            CRIT_STAGE.store(0, Ordering::Relaxed);
            if let Some((id, look)) = with(|s| s.crit_target.take().map(|id| (id, s.crit_look.take()))) {
                match look {
                    Some((yaw, pitch)) => turn_then("Attack", yaw, pitch, move |game| strike(game, id)),
                    None => strike(game, id),
                }
            }
        }
        return;
    }
    if ticks > 6 {
        CRIT_STAGE.store(0, Ordering::Relaxed);
        with(|s| s.crit_target = None);
        return;
    }
    if (stage == 1 || stage == 2) && !MOVED_THIS_TICK.load(Ordering::Relaxed) {
        let Some(p) = game.player.as_ref() else {
            return;
        };
        let (x, y, z) = (p.position.x, p.position.y, p.position.z);
        // outgoing makes it the hop and moves the stage on.
        game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags: 0 }));
    }
    if CRIT_STAGE.load(Ordering::Relaxed) == 3 {
        CRIT_STAGE.store(0, Ordering::Relaxed);
        if let Some(id) = with(|s| s.crit_target.take()) {
            strike(game, id);
        }
    }
}

pub(super) fn kill_aura(game: &mut Game<'_>, m: &Module, crits: bool) {
    super::sunset_aura::tick(game, m, crits);
}

pub(super) fn trigger_bot(game: &mut Game<'_>, m: &Module, crits: bool) {
    if !game.native_playing()
        || game.remote.is_none()
        || game.using.is_some()
        || CRIT_STAGE.load(Ordering::Relaxed) != 0
    {
        return;
    }
    let Some((i, _)) = game.mob_hit() else {
        return;
    };
    let mob = &game.mobs[i];
    let (id, kind) = (mob.id, kind_of(mob));
    let names = game.remote_player_names();
    let friends = with(|s| s.friends.clone());
    let player = kind == Kind::Player;
    if !wanted(m, kind) || (player && is_friend(&friends, &names, id)) || super::sunset_util::spared(game, m, id, player) {
        return;
    }
    let mut wait = with(|s| s.trigger_wait);
    let ready = cooldown_ready(game, m, &mut wait);
    with(|s| s.trigger_wait = wait);
    if !ready {
        return;
    }
    attack(game, id, crits, None);
    with(|s| s.trigger_wait = m.n("Delay (ticks)") as u32);
}

// A totem in the hotbar goes to the offhand by selecting it and pressing the
// swap key, as a player would; then the old slot comes back.

// Moves a totem from the hotbar to the offhand: select it, swap hands, select
// the old slot again, then give the server a few ticks to answer.
// ---------------------------------------------------------------- packets

// What the send hook does with an outgoing packet.
pub(super) enum Out {
    Pass,
    Drop,
    Replace(S),
}

// A move packet taken apart: where (if it says), the look (if it says), and its flags (bit 0 on the ground, bit 1
// against a wall).
#[derive(Clone, Copy, PartialEq)]
struct Move {
    at: Option<[f64; 3]>,
    look: Option<(f32, f32)>,
    flags: u8,
}

impl Move {
    fn of(packet: &S) -> Option<Self> {
        Some(match packet {
            S::MovePlayerPos(m) => Self { at: Some([m.x, m.y, m.z]), look: None, flags: m.flags },
            S::MovePlayerPosRot(m) => Self { at: Some([m.x, m.y, m.z]), look: Some((m.yaw, m.pitch)), flags: m.flags },
            S::MovePlayerRot(m) => Self { at: None, look: Some((m.yaw, m.pitch)), flags: m.flags },
            S::MovePlayerStatus(m) => Self { at: None, look: None, flags: m.flags },
            _ => return None,
        })
    }
    fn packet(self) -> S {
        let flags = self.flags;
        match (self.at, self.look) {
            (Some([x, y, z]), Some((yaw, pitch))) => S::MovePlayerPosRot(play::MovePlayerPosRot { x, y, z, yaw, pitch, flags }),
            (Some([x, y, z]), None) => S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }),
            (None, Some((yaw, pitch))) => S::MovePlayerRot(play::MovePlayerRot { yaw, pitch, flags }),
            (None, None) => S::MovePlayerStatus(play::MovePlayerStatus { flags }),
        }
    }
}

// Outgoing packets: Blink holds moves, Anti Hunger hides sprinting and the ground, No Fall claims the ground as you
// fall (or, in No ground, never), Criticals hops, Flight's anti kick sinks, and the look modules asked for (turn_then)
// rides along. Each is a change to the tick's own move packet, never another one.
pub(super) fn outgoing(game: &Game<'_>, packet: &S) -> Out {
    if ANTI_SPRINT.load(Ordering::Relaxed)
        && let S::PlayerCommand(c) = packet
        && (c.action == 1 || c.action == 2)
    {
        return Out::Drop;
    }
    let Some(sent) = Move::of(packet) else {
        return Out::Pass;
    };
    if BLINK.load(Ordering::Relaxed) {
        BLINK_HELD.store(true, Ordering::Relaxed);
        return Out::Drop;
    }
    MOVED_THIS_TICK.store(true, Ordering::Relaxed);
    let Some(p) = game.player.as_ref() else {
        return Out::Pass;
    };
    let mut m = sent;
    let here = [p.position.x, p.position.y, p.position.z];
    // Criticals: this tick's move is the hop (stage 1: up, stage 2: back down), both off the ground.
    let stage = CRIT_STAGE.load(Ordering::Relaxed);
    if stage == 1 || stage == 2 {
        let [x, y, z] = m.at.unwrap_or(here);
        m.at = Some([x, if stage == 1 { y + f64::from_bits(CRIT_HOP.load(Ordering::Relaxed)) } else { y }, z]);
        m.flags = 0;
        CRIT_STAGE.store(stage + 1, Ordering::Relaxed);
    } else {
        // Flight's Packet anti kick: the move carries the position, sunk 0.0313 below the last height sent while it
        // is due and you are in the air (Meteor's antiKickPacket).
        if FLY_PACKET.load(Ordering::Relaxed) {
            fly_down(game, &mut m, here);
        }
        // The server adds each move's drop to the fall, and a move that says you are on the ground costs what has
        // added up beyond three blocks. No Fall's Packet mode claims the ground on every move down, so the count
        // never passes one move's drop (Meteor's waits for 0.5 blocks a tick, which leaves a fall the server counted
        // before to hurt); past three blocks a tick (a fall of about 130 blocks) even that costs a little. No ground
        // never claims it. Flying that way claims it all the time.
        let grounded = !p.ground.on_ground && !p.flying && !p.fall_flying;
        let no_fall = NO_FALL.load(Ordering::Relaxed);
        let paused = game.creative || (NO_FALL_MACE.load(Ordering::Relaxed) && game.held().is_some_and(|s| s.item.ends_with(":mace")));
        let ground = if FLIGHT_GROUND.load(Ordering::Relaxed) || (GLIDE_GROUND.load(Ordering::Relaxed) && !p.flying) {
            Some(true)
        } else if no_fall == 1 && !paused && grounded && p.velocity.y < 0.0 {
            Some(true)
        } else if no_fall == 2 && !paused {
            Some(false)
        } else if ANTI_GROUND.load(Ordering::Relaxed) && (p.fall_distance <= 0.0 || p.flying) {
            Some(false)
        } else {
            None
        };
        if let Some(g) = ground {
            m.flags = (m.flags & !1) | u8::from(g);
        }
    }
    // The look a module asked for, else the player's own back after one (not during the Criticals hop, whose hit
    // follows it).
    let own = (game.yaw, game.pitch);
    let hopping = stage == 1 || stage == 2;
    let look = turns(|t| {
        if let Some(head) = t.queue.first().filter(|_| !t.carried) {
            t.carried = true;
            t.restore = head.look != own;
            Some(head.look)
        } else if t.restore && !hopping {
            t.restore = false;
            Some(own)
        } else {
            None
        }
    });
    if let Some(look) = look {
        m.look = Some(look);
    }
    if m == sent { Out::Pass } else { Out::Replace(m.packet()) }
}

// Flight's Packet anti kick on the move: while it is due and you are in the air, the height sinks 0.0313 below the
// last one sent (a move without a position gets the player's).
fn fly_down(game: &Game<'_>, m: &mut Move, here: [f64; 3]) {
    let mut at = m.at.unwrap_or(here);
    let bits = FLY_LAST_Y.load(Ordering::Relaxed);
    let last = f64::from_bits(bits);
    let sinking = at[1] >= last || last - at[1] < 0.03130;
    if FLY_DOWN.load(Ordering::Relaxed) && bits != u64::MAX && sinking && in_the_air(game) {
        at[1] = last - 0.03130;
        m.at = Some(at);
    } else {
        FLY_LAST_Y.store(at[1].to_bits(), Ordering::Relaxed);
    }
}

// ---- the look the server sees
//
// Rotate in Kill Aura, Nuker, Scaffold and the rest: a module asks for a look and hands over what to do once the server
// has it. The look rides on the tick's own move packet, or in a tick without one on the single one Sunset sends at
// its end, and what was asked runs after that packet has gone (Meteor's Rotations). Servers kick for a second move
// packet in a tick, and some check the look of a click.

type Deed = Box<dyn for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send>;

struct Ask {
    who: &'static str,
    look: (f32, f32),
    deeds: Vec<Deed>,
    // Asked this tick: another ask of the same module joins it instead of replacing it.
    fresh: bool,
}

struct Turns {
    // Oldest first, one a module; one look goes out a tick.
    queue: Vec<Ask>,
    // This tick's move packet carried the first ask's look.
    carried: bool,
    // A look went out that is not the player's own: the next move packet puts the player's back.
    restore: bool,
}

static TURNS: Mutex<Turns> = Mutex::new(Turns { queue: Vec::new(), carried: false, restore: false });

fn turns<R>(f: impl FnOnce(&mut Turns) -> R) -> R {
    f(&mut TURNS.lock().unwrap_or_else(|e| e.into_inner()))
}

// Has the server see the player look at (yaw, pitch), then does `then`. `who` names the module: what it asks again in
// the same tick goes along under its first look, and an ask from an earlier tick still waiting is replaced.
pub(super) fn turn_then(who: &'static str, yaw: f32, pitch: f32, then: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static) {
    if !yaw.is_finite() || !pitch.is_finite() {
        return;
    }
    let look = ((yaw + 180.0).rem_euclid(360.0) - 180.0, pitch.clamp(-90.0, 90.0));
    turns(|t| match t.queue.iter_mut().position(|a| a.who == who) {
        Some(i) if t.queue[i].fresh => t.queue[i].deeds.push(Box::new(then)),
        Some(i) => {
            if i == 0 && t.carried {
                // Its look is on the way already: the new ask waits behind it.
                t.queue.push(Ask { who, look, deeds: vec![Box::new(then)], fresh: true });
            } else {
                t.queue[i] = Ask { who, look, deeds: vec![Box::new(then)], fresh: true };
            }
        }
        None => t.queue.push(Ask { who, look, deeds: vec![Box::new(then)], fresh: true }),
    });
}

// A look alone (Anti AFK's spin).
pub(super) fn turn(who: &'static str, yaw: f32, pitch: f32) {
    turn_then(who, yaw, pitch, |_| {});
}

// Runs what waited for the look this tick's move packet carried.
fn turned(game: &mut Game<'_>) {
    let ask = turns(|t| std::mem::take(&mut t.carried).then(|| t.queue.remove(0)));
    for deed in ask.map_or_else(Vec::new, |a| a.deeds) {
        deed(game);
    }
}

// The end of the tick: with no move packet in it, one to carry the first look asked for (or the player's own back),
// and what waited for it.
fn end_turns(game: &mut Game<'_>) {
    if !MOVED_THIS_TICK.load(Ordering::Relaxed) && !BLINK.load(Ordering::Relaxed) && turns(|t| !t.queue.is_empty() || t.restore) {
        let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
        // outgoing puts the look in.
        game.send(S::MovePlayerRot(play::MovePlayerRot { yaw: game.yaw, pitch: game.pitch, flags: u8::from(ground) }));
        turned(game);
    }
    turns(|t| {
        for a in &mut t.queue {
            a.fresh = false;
        }
    });
}

fn in_the_air(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let b = p.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let f = |v: f64| v.floor() as i32;
    for x in f(lo.x - 0.0625)..=f(hi.x + 0.0625) {
        for y in f(lo.y - 0.0625 - 0.55)..=f(hi.y + 0.0625) {
            for z in f(lo.z - 0.0625)..=f(hi.z + 0.0625) {
                if !super::sunset_util::is_air(game, [x, y, z]) {
                    return false;
                }
            }
        }
    }
    true
}

// Where the player is, as one position packet: what Blink held back is a
// single move to here, as the server takes one position packet a tick.
pub(super) fn flush_blink(game: &mut Game<'_>) {
    if !BLINK_HELD.swap(false, Ordering::Relaxed) || MOVED_THIS_TICK.load(Ordering::Relaxed) {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let flags = u8::from(p.ground.on_ground) | u8::from(p.collision.horizontal) << 1;
    let (x, y, z) = (p.position.x, p.position.y, p.position.z);
    game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }));
}

// Zoom, through the zoom hook point.
pub(super) fn zoom(game: &Game<'_>, zoom: &mut Option<super::native_hooks::NativeZoom>) {
    if zoom.is_some() || !game.native_window() || !game.native_on(ID) {
        return;
    }
    let Some(m) = with(|s| s.module("Zoom").filter(|m| m.on).cloned()) else {
        return;
    };
    *zoom = Some(super::native_hooks::NativeZoom {
        factor: m.n("Factor") as f32,
        smooth: m.b("Smooth"),
        turn: 0.3,
    });
}

// ---------------------------------------------------------------- commands

pub(super) fn chat(game: &mut Game<'_>, message: &str) -> bool {
    if !game.native_window() || !game.native_on(ID) {
        return false;
    }
    let Some(rest) = message.strip_prefix('.') else {
        return false;
    };
    // `..text` sends `.text` as a chat message.
    if rest.starts_with('.') || rest.trim().is_empty() {
        return false;
    }
    ensure_loaded(game);
    game.ui.chat.add_recent(message);
    let args: Vec<&str> = rest.split_whitespace().collect();
    run(game, &args);
    true
}

const HELP: &[(&str, &str)] = &[
    (".menu", "open the click GUI (or press Right Shift)"),
    (".pages", "the same settings as pages in Options > Mods"),
    (".toggle <module>", "turn a module on or off (.t)"),
    (".modules", "list every module"),
    (".menu [search]", "open the click menu, searching for the text"),
    (".settings <module>", "list a module's settings"),
    (".set <module> <setting> <value>", "change a setting"),
    (".movementpreset <subtle|balanced|aggressive>", "apply explicit movement settings"),
    (".bind <module> <key|mouse4|none>", "bind a key or mouse button to a module"),
    (".binds", "list the binds"),
    (".reset <module>", "put a module back to its defaults"),
    (".panic", "turn every module off"),
    (".profile save|load|delete|export|import|list [name]", "keep the modules' settings under a name and bring them back (files in the mod's data folder; .profile load before-load undoes a load)"),
    (".friend add|remove|list [name]", "Kill Aura, Trigger Bot and Aim Assist skip friends"),
    (".waypoint add|remove|list [name]", "save a place (.wp)"),
    (".vclip <blocks>", "move up or down"),
    (".hclip <blocks>", "move forward"),
];

// Every module off: the Panic module, or .panic.
fn panic_off_state(s: &mut State) -> Option<(f32, f32)> {
    for m in &mut s.modules {
        m.on = false;
    }
    s.strafe = Strafe::default();
    s.knockback_ticks = 0;
    s.velocity = None;
    s.blast = [0.0; 3];
    s.scaffold_recent.clear();
    s.scaffold_wait = 0;
    take_scaffold_rotation(&mut s.scaffold_rotation)
}

pub(super) fn panic_off(game: &mut Game<'_>) {
    if let Some((yaw, pitch)) = with(panic_off_state) {
        game.yaw = yaw;
        game.pitch = pitch;
    }
    horizon_client::motion::sunset_motion::set_across(None);
    horizon_client::motion::sunset_motion::set_up(None);
    horizon_client::motion::sunset_motion::set_safe_walk(false);
    horizon_client::input::sunset_flags::NO_SLOW.store(false, Ordering::Relaxed);
    save(game);
    say(game, "Everything is off.");
}

// .profile save|load|delete|list <name>: the modules' settings kept under a name.
// Profiles (`.profile`): files in the mod's data folder (sunset_profiles), with a backup of the settings before every
// load. Profiles from before 0.9 (kept in the mod's settings) move into files the first time.
fn profile_command(game: &mut Game<'_>, args: &[&str]) {
    use super::sunset_profiles::{self as profiles, BACKUP, HEADER};
    let result = (|| -> Result<String, String> {
        let store = profiles::Store::new(game.native_data_dir(ID))?;
        let old: Vec<String> = game.native_setting(ID, "profiles").unwrap_or_default().split(',').filter(|n| !n.is_empty()).map(str::to_owned).collect();
        for n in &old {
            let key = format!("profile-{}", norm(n));
            if let (Ok(file), Some(text)) = (profiles::name(n), game.native_setting(ID, &key))
                && !store.exists(&file)
            {
                store.write(&file, &format!("{HEADER}\n{text}\n"))?;
            }
            game.native_set_setting(ID, &key, "");
        }
        if !old.is_empty() {
            game.native_set_setting(ID, "profiles", "");
        }
        let sub = args.first().map(|a| a.to_ascii_lowercase()).unwrap_or_default();
        if sub == "list" {
            let all = store.list()?;
            return Ok(if all.is_empty() { "No profiles yet. .profile save <name> keeps the current settings.".to_owned() } else { format!("Profiles: {}", all.join(", ")) });
        }
        let Some(name) = args.get(1..).filter(|rest| !rest.is_empty()).map(|rest| profiles::name(&rest.join(" "))).transpose()? else {
            return Err("use .profile save|load|delete|export|import <name>, or .profile list".into());
        };
        if name == BACKUP && sub != "load" {
            return Err(format!("{BACKUP} is the settings before the last load; .profile load {BACKUP} puts them back"));
        }
        match sub.as_str() {
            "save" => {
                let text = with(|s| s.profile_text());
                store.write(&name, &format!("{HEADER}\n{text}\n"))?;
                Ok(format!("Saved profile {name}."))
            }
            "load" => {
                let text = store.read(&name)?;
                let body = with(|s| s.check_profile(&text).map(str::to_owned))?;
                let before = with(|s| s.profile_text());
                store.write(BACKUP, &format!("{HEADER}\n{before}\n"))?;
                with(|s| s.load_profile(&body));
                save(game);
                Ok(format!("Loaded profile {name}. .profile load {BACKUP} puts the settings back as they were."))
            }
            "delete" => {
                store.delete(&name)?;
                Ok(format!("Deleted profile {name}."))
            }
            "export" => Ok(format!("Profile {name} is in {}", store.export(&name)?.display())),
            "import" => {
                let text = store.import(&name).map_err(|e| format!("{e} (put {name}.profile in {})", store.imports().display()))?;
                with(|s| s.check_profile(&text).map(|_| ()))?;
                store.write(&name, &text)?;
                Ok(format!("Imported profile {name}; .profile load {name} uses it."))
            }
            _ => Err("use .profile save|load|delete|export|import <name>, or .profile list".into()),
        }
    })();
    say(game, &result.unwrap_or_else(|e| format!("Profile: {e}.")));
}

fn apply_movement_preset(state: &mut State, style: &str) -> bool {
    let changes: &[(&str, &str, &str)] = match norm(style).as_str() {
        "subtle" => &[
            ("Speed", "Mode", "Smooth"), ("Speed", "Vanilla speed", "3"),
            ("Speed", "Only on ground", "true"), ("Speed", "Acceleration", "0.025"),
            ("Speed", "Deceleration", "0.04"), ("Speed", "Strafe strength", "0.5"),
            ("Velocity", "Mode", "Scale"), ("Velocity", "Knockback horizontal", "1"),
            ("Velocity", "Knockback vertical", "1"), ("Velocity", "Explosions horizontal", "1"),
            ("Velocity", "Explosions vertical", "1"),
            ("Scaffold", "Delay (ticks)", "2"), ("Scaffold", "Rotate", "true"),
            ("No Slow", "Ground %", "20"), ("No Slow", "Air %", "20"),
            ("GUI Move", "Ground speed %", "100"), ("GUI Move", "Air speed %", "100"),
        ],
        "balanced" => &[
            ("Speed", "Mode", "Strafe"), ("Speed", "Strafe speed", "1.3"),
            ("Speed", "Speed limit", "true"),
            ("Velocity", "Mode", "Scale"), ("Velocity", "Knockback horizontal", "0.7"),
            ("Velocity", "Knockback vertical", "0.85"),
            ("Scaffold", "Delay (ticks)", "1"), ("Scaffold", "Rotate", "true"),
            ("No Slow", "Ground %", "70"), ("No Slow", "Air %", "70"),
            ("GUI Move", "Ground speed %", "100"), ("GUI Move", "Air speed %", "100"),
        ],
        "aggressive" => &[
            ("Speed", "Mode", "Custom"), ("Speed", "Vanilla speed", "10"),
            ("Speed", "Only on ground", "false"), ("Speed", "Ground multiplier", "1.2"),
            ("Speed", "Air multiplier", "1.1"), ("Speed", "Acceleration", "0.15"),
            ("Speed", "Deceleration", "0.1"), ("Speed", "Strafe strength", "1"),
            ("Velocity", "Mode", "Scale"), ("Velocity", "Knockback horizontal", "0"),
            ("Velocity", "Knockback vertical", "0"),
            ("Scaffold", "Delay (ticks)", "0"), ("Scaffold", "Rotation speed", "180"),
            ("No Slow", "Ground %", "100"), ("No Slow", "Air %", "100"),
            ("GUI Move", "Ground speed %", "125"), ("GUI Move", "Air speed %", "125"),
        ],
        _ => return false,
    };
    for &(module, setting, value) in changes {
        if let Some(s) = state.modules.iter_mut().find(|m| m.name == module)
            .and_then(|m| m.settings.iter_mut().find(|s| s.name == setting)) {
            s.parse(value);
        }
    }
    true
}

pub(super) fn run(game: &mut Game<'_>, args: &[&str]) {
    let cmd = args[0].to_ascii_lowercase();
    let module_arg = |game: &mut Game<'_>, name: Option<&&str>| -> Option<usize> {
        let Some(name) = name else {
            say(game, "Name a module.");
            return None;
        };
        let found = with(|s| s.find(name));
        if found.is_none() {
            say(game, &format!("No module called {name}. Try .modules"));
        }
        found
    };
    match cmd.as_str() {
        "help" | "commands" => {
            say(game, "Commands:");
            for (c, d) in HELP {
                say(game, &format!("{c} - {d}"));
            }
        }
        "menu" | "gui" => {
            // `.menu esp` opens it with that search typed.
            let search: String = args[1..].join(" ").chars().take(20).collect();
            with(|s| s.gui.search = search);
            open_gui(game);
        }
        "pages" => open_menu(game),
        "toggle" | "t" => {
            if let Some(i) = module_arg(game, args.get(1)) {
                toggle(game, i, true);
            }
        }
        "modules" | "list" => {
            let lines: Vec<String> = with(|s| {
                CATEGORIES
                    .iter()
                    .map(|&c| {
                        let names: Vec<String> = s
                            .modules
                            .iter()
                            .filter(|m| m.category == c)
                            .map(|m| if m.on { format!("{}*", m.name) } else { m.name.to_owned() })
                            .collect();
                        format!("{}: {}", c.label(), names.join(", "))
                    })
                    .collect()
            });
            for l in lines {
                say(game, &l);
            }
            say(game, "* is on.");
        }
        "settings" | "get" => {
            if let Some(i) = module_arg(game, args.get(1)) {
                let lines: Vec<String> = with(|s| s.modules[i].settings.iter().map(Setting::shown).collect());
                if lines.is_empty() {
                    say(game, "It has no settings.");
                }
                for l in lines {
                    say(game, &l);
                }
            }
        }
        "set" => {
            let Some(i) = module_arg(game, args.get(1)) else {
                return;
            };
            if args.len() < 4 {
                say(game, "Use .set <module> <setting> <value>");
                return;
            }
            // The setting's name is the first words that name one (exactly, else
            // by its start); the rest is the value, spaces and all.
            let (found, value) = with(|s| {
                let m = &s.modules[i];
                for exact in [true, false] {
                    for k in (1..args.len() - 2).rev() {
                        let name = norm(&args[2..2 + k].join(""));
                        let at = m.settings.iter().position(|st| {
                            if exact { norm(st.name) == name } else { norm(st.name).starts_with(&name) }
                        });
                        if let Some(at) = at {
                            return (Some(at), args[2 + k..].join(" "));
                        }
                    }
                }
                (None, String::new())
            });
            let value = value.as_str();
            let result = found.and_then(|at| {
                with(|s| {
                    let setting = &mut s.modules[i].settings[at];
                    Some((setting.parse(value), setting.shown()))
                })
            });
            match result {
                None => say(game, "No such setting. Try .settings <module>"),
                Some((false, shown)) => say(game, &format!("Could not read {value}. Now {shown}")),
                Some((true, shown)) => {
                    save(game);
                    say(game, &shown);
                }
            }
        }
        "movementpreset" => {
            if let Some(style) = args.get(1) && with(|s| apply_movement_preset(s, style)) {
                save(game);
                say(game, &format!("{style} movement preset applied. Use .settings <module> to inspect values."));
            } else {
                say(game, "Use .movementpreset subtle, balanced or aggressive");
            }
        }
        "bind" => {
            let Some(i) = module_arg(game, args.get(1)) else {
                return;
            };
            let Some(k) = args.get(2) else {
                say(game, "Use .bind <module> <key or mouse1 to mouse5>, or none");
                return;
            };
            let key = if k.eq_ignore_ascii_case("none") {
                None
            } else {
                match key_from_text(k) {
                    Some(key) => Some(key),
                    None => {
                        say(game, &format!("Unknown key {k}."));
                        return;
                    }
                }
            };
            let name = with(|s| {
                set_bind(s, i, key.clone());
                s.modules[i].name
            });
            save(game);
            say(game, &format!("{name} bound to {}", key_label(key.as_deref())));
        }
        "binds" => {
            let lines: Vec<String> = with(|s| {
                s.modules
                    .iter()
                    .filter_map(|m| {
                        let hold = if m.p("Bind mode") == "Hold" { " (hold)" } else { "" };
                        m.key.as_deref().map(|k| format!("{}: {}{hold}", m.name, key_label(Some(k))))
                    })
                    .collect()
            });
            if lines.is_empty() {
                say(game, "No key binds yet.");
            }
            for l in lines {
                say(game, &l);
            }
        }
        "panic" => panic_off(game),
        "profile" | "profiles" | "config" | "cfg" => profile_command(game, &args[1..]),
        "reset" => {
            if let Some(i) = module_arg(game, args.get(1)) {
                let name = with(|s| {
                    s.inputs.take_where(|h| h.module == i);
                    let fresh = defaults().swap_remove(i);
                    s.modules[i] = fresh;
                    s.modules[i].name
                });
                save(game);
                say(game, &format!("{name} reset."));
            }
        }
        "friend" | "friends" | "f" => {
            let sub = args.get(1).map(|s| s.to_ascii_lowercase());
            match (sub.as_deref(), args.get(2)) {
                (Some("add"), Some(name)) => {
                    with(|s| {
                        if !s.friends.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                            s.friends.push((*name).to_owned());
                        }
                    });
                    save(game);
                    say(game, &format!("{name} is a friend."));
                }
                (Some("remove" | "del" | "rm"), Some(name)) => {
                    with(|s| s.friends.retain(|f| !f.eq_ignore_ascii_case(name)));
                    save(game);
                    say(game, &format!("{name} is no longer a friend."));
                }
                _ => {
                    let list = with(|s| s.friends.join(", "));
                    say(game, &format!("Friends: {}", if list.is_empty() { "none" } else { &list }));
                }
            }
        }
        "waypoint" | "waypoints" | "wp" => {
            let sub = args.get(1).map(|a| a.to_ascii_lowercase());
            match (sub.as_deref(), args.get(2)) {
                (Some("add" | "set"), Some(name)) => {
                    let Some(p) = game.native_player() else {
                        say(game, "Join a world first.");
                        return;
                    };
                    let w = Waypoint {
                        name: (*name).replace(['|', '='], ""),
                        pos: [p.x.floor() + 0.5, p.y.floor() + 1.0, p.z.floor() + 0.5],
                        dim: p.dimension.to_owned(),
                    };
                    with(|s| {
                        s.waypoints.retain(|o| !o.name.eq_ignore_ascii_case(&w.name) || o.dim != w.dim);
                        s.waypoints.push(w.clone());
                        if let Some(i) = s.find("Waypoints") {
                            s.modules[i].on = true;
                        }
                    });
                    save(game);
                    say(game, &format!("Saved {} at {} {} {}", w.name, w.pos[0].floor(), w.pos[1].floor(), w.pos[2].floor()));
                }
                (Some("remove" | "del" | "rm"), Some(name)) => {
                    with(|s| s.waypoints.retain(|o| !o.name.eq_ignore_ascii_case(name)));
                    save(game);
                    say(game, &format!("Removed {name}."));
                }
                _ => {
                    let lines: Vec<String> = with(|s| {
                        s.waypoints
                            .iter()
                            .map(|w| format!("{}: {} {} {} ({})", w.name, w.pos[0].floor(), w.pos[1].floor(), w.pos[2].floor(), w.dim.trim_start_matches("minecraft:")))
                            .collect()
                    });
                    if lines.is_empty() {
                        say(game, "No waypoints. Use .waypoint add <name>");
                    }
                    for l in lines {
                        say(game, &l);
                    }
                }
            }
        }
        "vclip" | "hclip" => {
            let Some(d) = args.get(1).and_then(|a| a.parse::<f64>().ok()) else {
                say(game, &format!("Use .{cmd} <blocks>"));
                return;
            };
            let yaw = f64::from(game.yaw).to_radians();
            if let Some(p) = game.player.as_mut() {
                if cmd == "vclip" {
                    p.position.y += d;
                } else {
                    p.position.x += -yaw.sin() * d;
                    p.position.z += yaw.cos() * d;
                }
                p.velocity = horizon_core::geometry::Vec3::ZERO;
            }
        }
        _ => say(game, &format!("Unknown command .{cmd}. Try .help")),
    }
}

// ---------------------------------------------------------------- HUD

pub(super) struct Camera {
    pos: [f64; 3],
    fwd: [f64; 3],
    up: [f64; 3],
    right: [f64; 3],
    focal: f64,
    aspect: f64,
    w: f64,
    h: f64,
    // The sine and cosine of half the view's width and of half its height: the sides of what it sees.
    cone: [f64; 4],
}

impl Camera {
    pub(super) fn of(game: &Game<'_>, w: f32, h: f32) -> Option<Self> {
        let (last_pos, last_yaw, last_pitch, fov) = game.locator_camera?;
        let p = game.player.as_ref()?;
        let alpha = game.clock.camera_fraction();
        let (pos, yaw, pitch) = if game.camera_type.first_person() {
            let lerp = |a: f64, b: f64| a + alpha * (b - a);
            let eye = f64::from(game.previous_eye) + alpha * f64::from(game.eye - game.previous_eye);
            (
                [
                    lerp(game.previous_position.x, p.position.x),
                    lerp(game.previous_position.y, p.position.y) + eye,
                    lerp(game.previous_position.z, p.position.z),
                ],
                game.yaw,
                game.pitch,
            )
        } else {
            (last_pos, last_yaw, last_pitch)
        };
        let (sy, cy) = f64::from(yaw.to_radians()).sin_cos();
        let (sp, cp) = f64::from(pitch.to_radians()).sin_cos();
        let focal = 1.0 / f64::from((fov / 2.0).to_radians()).tan();
        let aspect = f64::from(w) / f64::from(h);
        let (sh, ch) = (aspect / focal).atan().sin_cos();
        let (sv, cv) = (1.0 / focal).atan().sin_cos();
        Some(Self {
            pos,
            fwd: [-sy * cp, -sp, cy * cp],
            up: [-sy * sp, cp, cy * sp],
            right: [-cy, 0.0, -sy],
            focal,
            aspect,
            w: f64::from(w),
            h: f64::from(h),
            cone: [sh, ch, sv, cv],
        })
    }
    // Whether anything of a ball round `at` can be on screen: the test before any other work on a thing.
    pub(super) fn sees(&self, at: [f64; 3], radius: f64) -> bool {
        let o = [at[0] - self.pos[0], at[1] - self.pos[1], at[2] - self.pos[2]];
        let dot = |a: [f64; 3]| a[0] * o[0] + a[1] * o[1] + a[2] * o[2];
        let depth = dot(self.fwd);
        let [sh, ch, sv, cv] = self.cone;
        depth > -radius && depth * sh - dot(self.right).abs() * ch > -radius && depth * sv - dot(self.up).abs() * cv > -radius
    }
    // Whether a point on screen is within `margin` of it.
    pub(super) fn on_screen(&self, p: [f32; 2], margin: f32) -> bool {
        p[0] > -margin && p[1] > -margin && p[0] < self.w as f32 + margin && p[1] < self.h as f32 + margin
    }
    pub(super) fn project(&self, at: [f64; 3]) -> Option<[f32; 2]> {
        let o = [at[0] - self.pos[0], at[1] - self.pos[1], at[2] - self.pos[2]];
        let dot = |a: [f64; 3]| a[0] * o[0] + a[1] * o[1] + a[2] * o[2];
        let depth = dot(self.fwd);
        if depth < 0.05 {
            return None;
        }
        let x = dot(self.right) * self.focal / (depth * self.aspect);
        let y = dot(self.up) * self.focal / depth;
        Some([((1.0 + x) / 2.0 * self.w) as f32, ((1.0 - y) / 2.0 * self.h) as f32])
    }
    // Where the view is from.
    pub(super) fn eye(&self) -> [f64; 3] {
        self.pos
    }
    // A box's eight corners on screen (None behind the camera): corner i is at hi on x, y and z where bits 1, 2 and
    // 4 of i are set.
    pub(super) fn points(&self, lo: [f64; 3], hi: [f64; 3]) -> [Option<[f32; 2]>; 8] {
        std::array::from_fn(|i| {
            self.project([
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ])
        })
    }
    // The eight, when all of the box is in front.
    pub(super) fn corners(&self, lo: [f64; 3], hi: [f64; 3]) -> Option<[[f32; 2]; 8]> {
        let pts = self.points(lo, hi);
        pts.iter().all(Option::is_some).then(|| pts.map(Option::unwrap_or_default))
    }
    // The screen rectangle round those corners, unless it is far off screen.
    pub(super) fn bounds(&self, pts: &[[f32; 2]; 8]) -> Option<[f32; 4]> {
        let r = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, &[x, y]| [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)]);
        let margin = 4.0 * self.w as f32;
        (r[2] > -margin && r[0] < margin && r[3] > -margin && r[1] < margin).then_some(r)
    }
}

pub(super) enum Shape {
    Fill([f32; 4], u32),
    // A rectangle with rounded corners, and their radius.
    Round([f32; 4], f32, u32),
    // A rectangle with only some corners rounded: top left, top right, bottom right, bottom left.
    Corners([f32; 4], f32, [bool; 4], u32),
    // A slanted shape (a line): its four corners in order round it.
    Quad([[f32; 2]; 4], u32),
    Text(String, [f32; 2], u32),
    // A line of text in the font's bolder weight: a panel's header.
    Head(String, [f32; 2], u32),
}

// Where shapes go: a list drawn after the frame's other work (the menu, plugins), or straight into the frame's
// batch (what the world overlays draw, so a frame of them makes no list and no copies of their text).
pub(super) trait Canvas {
    fn fill(&mut self, r: [f32; 4], argb: u32);
    fn quad(&mut self, corners: [[f32; 2]; 4], argb: u32);
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32);
}

impl Canvas for Vec<Shape> {
    fn fill(&mut self, r: [f32; 4], argb: u32) {
        self.push(Shape::Fill(r, argb));
    }
    fn quad(&mut self, corners: [[f32; 2]; 4], argb: u32) {
        self.push(Shape::Quad(corners, argb));
    }
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.push(Shape::Text(text.to_owned(), at, rgb));
    }
}

pub(super) struct Painter<'a, 'g> {
    pub(super) game: &'a Game<'g>,
    pub(super) batch: &'a mut HudBatch,
}

impl Canvas for Painter<'_, '_> {
    fn fill(&mut self, r: [f32; 4], argb: u32) {
        self.game.native_fill(self.batch, r, argb);
    }
    fn quad(&mut self, corners: [[f32; 2]; 4], argb: u32) {
        let [a, r, g, b] = argb.to_be_bytes();
        self.batch.fill_quad(&self.game.gui.atlas, corners, horizon_client::hud::rgba(r, g, b, a));
    }
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.game.native_text(self.batch, text, at, rgb, true);
    }
}

pub(super) fn outline(out: &mut impl Canvas, r: [f32; 4], argb: u32, lw: f32, fill: bool) {
    let [x0, y0, x1, y1] = r;
    if fill {
        out.fill([x0, y0, x1 - x0, y1 - y0], (argb & 0x00FF_FFFF) | 0x2800_0000);
    }
    out.fill([x0, y0, x1 - x0, lw], argb);
    out.fill([x0, y1 - lw, x1 - x0, lw], argb);
    out.fill([x0, y0, lw, y1 - y0], argb);
    out.fill([x1 - lw, y0, lw, y1 - y0], argb);
}

pub(super) fn health_colour(fraction: f32) -> u32 {
    if fraction > 0.66 {
        0x55FF55
    } else if fraction > 0.33 {
        0xFFFF55
    } else {
        0xFF5555
    }
}

// The frame's list of shapes, kept from frame to frame so it is not grown again each time.
static SHAPES: Mutex<Vec<Shape>> = Mutex::new(Vec::new());

// A rounded rectangle: straight sides as plain fills, each corner a fan with one physical pixel of edge fade (`px`).
fn round_fill(game: &Game<'_>, batch: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, which: [bool; 4], argb: u32, px: f32) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    if r < px {
        game.native_fill(batch, [x, y, w, h], argb);
        return;
    }
    game.native_fill(batch, [x + r, y, w - 2.0 * r, h], argb);
    if h > 2.0 * r {
        game.native_fill(batch, [x, y + r, r, h - 2.0 * r], argb);
        game.native_fill(batch, [x + w - r, y + r, r, h - 2.0 * r], argb);
    }
    let [a, red, g, b] = argb.to_be_bytes();
    let colour = horizon_client::hud::rgba(red, g, b, a);
    let (inner, outer) = ((r - px * 0.5).max(0.0), r + px * 0.5);
    let segments = (((r / px).sqrt() * 2.0).ceil() as usize).clamp(3, 16);
    let at = |c: [f32; 2], radius: f32, angle: f32| [c[0] + radius * angle.cos(), c[1] + radius * angle.sin()];
    let quarter = std::f32::consts::FRAC_PI_2;
    let corners = [([x + r, y + r], 2.0), ([x + w - r, y + r], 3.0), ([x + w - r, y + h - r], 0.0), ([x + r, y + h - r], 1.0)];
    for ((c, turn), round) in corners.into_iter().zip(which) {
        if !round {
            // A square corner: the square the arc would have cut.
            game.native_fill(batch, [c[0] - r * f32::from(u8::from(c[0] < x + w / 2.0)), c[1] - r * f32::from(u8::from(c[1] < y + h / 2.0)), r, r], argb);
            continue;
        }
        for i in 0..segments {
            let a0 = (turn + i as f32 / segments as f32) * quarter;
            let a1 = (turn + (i + 1) as f32 / segments as f32) * quarter;
            let (p0, p1) = (at(c, inner, a0), at(c, inner, a1));
            batch.fill_quad(&game.gui.atlas, [c, p0, p1, p1], colour);
            let start = batch.vertices.len();
            batch.fill_quad(&game.gui.atlas, [p0, at(c, outer, a0), at(c, outer, a1), p1], colour);
            // fill_quad's two triangles use corners 0,1,2,0,2,3.
            for (vertex, corner) in batch.vertices[start..].iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                if corner == 1 || corner == 2 {
                    vertex.color &= 0x00FF_FFFF;
                }
            }
        }
    }
}

// Draws a list of shapes (and empties it, keeping its room).
fn paint(game: &Game<'_>, batch: &mut HudBatch, shapes: &mut Vec<Shape>, px: f32) {
    for s in shapes.drain(..) {
        match s {
            Shape::Fill(r, argb) => game.native_fill(batch, r, argb),
            Shape::Round(r, radius, argb) => round_fill(game, batch, r, radius, [true; 4], argb, px),
            Shape::Corners(r, radius, which, argb) => round_fill(game, batch, r, radius, which, argb, px),
            Shape::Quad(corners, argb) => {
                let [a, r, g, b] = argb.to_be_bytes();
                batch.fill_quad(&game.gui.atlas, corners, horizon_client::hud::rgba(r, g, b, a));
            }
            Shape::Text(t, at, rgb) => game.native_text(batch, &t, at, rgb, true),
            Shape::Head(t, at, rgb) => {
                horizon_client::font::sunset_font::BOLD.store(true, Ordering::Relaxed);
                game.native_text(batch, &t, at, rgb, true);
                horizon_client::font::sunset_font::BOLD.store(false, Ordering::Relaxed);
            }
        }
    }
}

pub(super) fn hud(game: &mut Game<'_>, batch: &mut HudBatch, corners: &mut Corners) {
    if !game.native_window() || !game.native_on(ID) {
        return;
    }
    ensure_loaded(game);
    let mods = snapshot();
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let (w, h) = (batch.width, batch.height);
    let lw = (1.5 / batch.scale).max(0.5);
    let mut shapes = std::mem::take(&mut *SHAPES.lock().unwrap_or_else(|e| e.into_inner()));

    if let Some(cam) = Camera::of(game, w, h)
        && game.player.is_some()
    {
        world_overlays(game, &cam, &mods, lw, &mut Painter { game, batch: &mut *batch });
    }

    if let Some(hud) = get("HUD") {
        if hud.b("Target HUD") && get("Kill Aura").is_some() {
            super::sunset_aura::draw_target(game, w, h, hud.p("Target HUD mode") == "Potato", &mut shapes);
        }
        if hud.b("Watermark") && game.native_corner_shown(Corner::TopLeft) {
            let text = format!("{NAME} {VERSION}");
            let tw = game.native_text_width(&text);
            let (x, y) = corners.place(Corner::TopLeft, tw + 4.0, 12.0);
            shapes.push(Shape::Text(text, [x + 2.0, y + 2.0], accent()));
        }
        if hud.b("Module list") && game.native_corner_shown(Corner::TopRight) {
            let mut lines: Vec<String> = mods
                .iter()
                .filter(|m| m.on && m.name != "HUD")
                .map(|m| match m.info() {
                    Some(i) => format!("{} [{i}]", m.name),
                    None => m.name.to_owned(),
                })
                .collect();
            let widths: Vec<f32> = lines.iter().map(|l| game.native_text_width(l)).collect();
            let mut order: Vec<usize> = (0..lines.len()).collect();
            order.sort_by(|&a, &b| widths[b].total_cmp(&widths[a]));
            let max = widths.iter().copied().fold(0.0, f32::max);
            if !lines.is_empty() {
                let (x, y) = corners.place(Corner::TopRight, max + 4.0, lines.len() as f32 * 10.0 + 2.0);
                for (row, &i) in order.iter().enumerate() {
                    let text = std::mem::take(&mut lines[i]);
                    shapes.push(Shape::Text(text, [x + max + 2.0 - widths[i], y + 2.0 + row as f32 * 10.0], 0xFFFFFF));
                }
            }
        }
        if game.native_corner_shown(Corner::TopLeft)
            && let Some(p) = game.native_player()
        {
            let mut lines: Vec<String> = Vec::new();
            if hud.b("Coordinates") {
                let (x, y, z) = (p.x.floor(), p.y.floor(), p.z.floor());
                let other = match p.dimension {
                    "minecraft:overworld" => format!(" (Nether {} {})", (p.x / 8.0).floor(), (p.z / 8.0).floor()),
                    "minecraft:the_nether" => format!(" (Overworld {} {})", (p.x * 8.0).floor(), (p.z * 8.0).floor()),
                    _ => String::new(),
                };
                lines.push(format!("XYZ {x} {y} {z}{other}"));
            }
            let mut info: Vec<String> = Vec::new();
            if hud.b("FPS") {
                info.push(format!("{} FPS", game.native_frame_rate().0));
            }
            if hud.b("Speed")
                && let Some(pl) = game.player.as_ref()
            {
                let (dx, dz) = (pl.position.x - game.previous_position.x, pl.position.z - game.previous_position.z);
                info.push(format!("{:.1} b/s", (dx * dx + dz * dz).sqrt() * 20.0));
            }
            if hud.b("Facing") {
                const NAMES: [&str; 8] = ["South +Z", "South-West", "West -X", "North-West", "North -Z", "North-East", "East +X", "South-East"];
                let yaw = f64::from(p.yaw).rem_euclid(360.0);
                info.push(NAMES[((yaw + 22.5) / 45.0).floor() as usize % 8].to_owned());
            }
            if hud.b("Totems") {
                let count = |s: &Option<horizon_client::items::ItemStack>| {
                    s.as_ref().filter(|s| s.item.ends_with("totem_of_undying")).map_or(0, |s| s.count)
                };
                let total: u32 = game.inventory.iter().map(count).sum::<u32>() + count(&game.offhand);
                info.push(format!("{total} totems"));
            }
            if !info.is_empty() {
                lines.push(info.join("  "));
            }
            let width = lines.iter().map(|l| game.native_text_width(l)).fold(0.0, f32::max);
            if !lines.is_empty() {
                let (bx, by) = corners.place(Corner::TopLeft, width + 4.0, lines.len() as f32 * 10.0 + 2.0);
                for (i, line) in lines.into_iter().enumerate() {
                    shapes.push(Shape::Text(line, [bx + 2.0, by + 2.0 + i as f32 * 10.0], 0xFFFFFF));
                }
            }
        }
    }

    if with(|s| s.gui.open && s.gui.closing) {
        close_gui(game);
    }
    let mut gui_shapes = Vec::new();
    // GUI scale, as far as a row of the narrowest panels still fits the window.
    let scale = f32::from_bits(LOOK[6].load(Ordering::Relaxed)).min((w / MIN_ROW).max(1.0));
    LOOK[7].store(scale.to_bits(), Ordering::Relaxed);
    if with(|s| s.gui.open) {
        if game.screen == Screen::Playing && game.player.is_some() {
            // The world dims behind the panels by Background dim (percent), the one fill behind them.
            let dim = LOOK[3].load(Ordering::Relaxed).min(100) * 255 / 100;
            if dim > 0 {
                shapes.push(Shape::Fill([0.0, 0.0, w, h], dim << 24));
            }
            draw_gui(game, w / scale, h / scale, 1.0 / (batch.scale * scale), &mut gui_shapes);
        } else {
            with(|s| {
                s.gui.open = false;
                s.gui.drag = None;
                s.gui.editor = None;
                s.gui.window = None;
            });
        }
    }
    let game = &*game;
    // Sunset's font is drawn for the scale it is told, and rebuilds its glyphs when that changes: while the GUI is
    // open all of the frame's text uses the GUI's, so it is sharp at any GUI scale and they rebuild only once.
    let real = batch.scale;
    if !gui_shapes.is_empty() {
        batch.scale = real * scale;
    }
    paint(game, batch, &mut shapes, 1.0 / batch.scale);
    *SHAPES.lock().unwrap_or_else(|e| e.into_inner()) = shapes;
    // The GUI goes on top, scaled about the corner its coordinates start from.
    let start = batch.vertices.len();
    paint(game, batch, &mut gui_shapes, 1.0 / batch.scale);
    batch.scale_since(start, 0.0, 0.0, scale, scale);
    batch.scale = real;
    if !with(|s| s.gui.open) {
        super::sunset_radar::draw(game, batch, corners, &mods);
    }
}

pub(super) fn world_overlays(game: &Game<'_>, cam: &Camera, mods: &[Module], lw: f32, out: &mut impl Canvas) {
    super::sunset_util::draw_shown(cam, out, lw);
    super::sunset_render::overlay(game, cam, mods, out, lw);
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    if let Some(m) = get("Nuker") {
        let (look, argb) = (Look::of(m), m.c("Broken color"));
        with(|s| {
            if let Some([lo, hi]) = s.nuke_box.filter(|_| m.b("Bounding box") && m.p("Mode") != "Smash") {
                draw_box(cam, out, &look, lo, hi, m.c("Box color"), lw);
            }
            for &(pos, left) in &s.nuke_broken {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                let a = ((argb >> 24) * u32::from(left) / 8) << 24;
                draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], a | (argb & 0x00FF_FFFF), lw);
            }
        });
    }
    if let Some(m) = get("Scaffold").filter(|m| m.b("Render")) {
        let (look, argb) = (Look::of(m), m.c("Color"));
        with(|s| {
            for &(pos, left) in &s.scaffold_placed {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                let a = ((argb >> 24) * u32::from(left) / 8) << 24;
                draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], a | (argb & 0x00FF_FFFF), lw);
            }
        });
    }
    let alpha = game.clock.camera_fraction();
    let lerp = |a: f64, b: f64| a + alpha * (b - a);
    let dist = |at: [f64; 3]| {
        let d = [at[0] - cam.pos[0], at[1] - cam.pos[1], at[2] - cam.pos[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    let (esp, tracers, tags) = (get("ESP"), get("Tracers"), get("Nametags"));
    if esp.is_some() || tracers.is_some() || tags.is_some() {
        entity_overlays(game, cam, [esp, tracers, tags], lw, out);
    }
    // Text drawn from one buffer, written over for each label.
    let mut text = String::new();

    if let Some(m) = esp.filter(|m| m.b("Items")) {
        let (look, colour, range, fade) = (Look::of(m), m.c("Items color"), m.n("Range"), m.n("Fade distance"));
        for item in &game.dropped {
            let at = [
                lerp(item.previous.x, item.position.x),
                lerp(item.previous.y, item.position.y),
                lerp(item.previous.z, item.position.z),
            ];
            let d = dist(at);
            if d > range {
                continue;
            }
            let lo = [at[0] - 0.125, at[1], at[2] - 0.125];
            let hi = [at[0] + 0.125, at[1] + 0.25, at[2] + 0.125];
            draw_box(cam, out, &look, lo, hi, faded(colour, d, fade), lw);
        }
    }

    if let Some(m) = get("Storage ESP") {
        let (look, range, fade, max) = (Look::of(m), m.n("Range"), m.n("Fade distance"), m.n("Max") as usize);
        let want = [m.b("Ender chests"), m.b("Chests"), m.b("Barrels"), m.b("Shulkers"), m.b("Other")];
        let facts = game.res.facts();
        let mut drawn = 0;
        for &(x, y, z) in game.world.block_entities.keys() {
            let at = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
            let d = dist(at);
            // Out of range or off screen: nothing more is looked up.
            if d > range || !cam.sees(at, 0.87) {
                continue;
            }
            let name = &facts.get(game.world.block(x, y, z)).name;
            let Some(col) = storage_colour(name) else {
                continue;
            };
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            let group = if short == "ender_chest" {
                0
            } else if short.ends_with("chest") {
                1
            } else if short == "barrel" {
                2
            } else if short.ends_with("shulker_box") {
                3
            } else {
                4
            };
            if !want[group] {
                continue;
            }
            let inset = if name.ends_with("chest") { 0.0625 } else { 0.0 };
            let lo = [f64::from(x) + inset, f64::from(y), f64::from(z) + inset];
            let hi = [f64::from(x + 1) - inset, f64::from(y + 1) - inset * 2.0, f64::from(z + 1) - inset];
            if draw_box(cam, out, &look, lo, hi, faded(col, d, fade), lw) {
                drawn += 1;
                if drawn >= max {
                    break;
                }
            }
        }
    }

    if let Some(m) = get("Breadcrumbs") {
        let (dots, colour) = (m.p("Mode") == "Potato", 0xFF00_0000 | accent());
        let now = game.player.as_ref().map(|p| [p.position.x, p.position.y + 0.1, p.position.z]);
        with(|s| {
            let mut last: Option<[f32; 2]> = None;
            // To where you are now at the end.
            for point in s.crumbs.iter().chain(now.as_ref()) {
                if dots {
                    potato(cam, out, *point, colour, lw);
                    continue;
                }
                let next = cam.project(*point);
                if let (Some(a), Some(b)) = (last, next) {
                    line(out, a, b, colour, lw);
                }
                last = next;
            }
        });
    }

    if let Some(m) = get("Block ESP") {
        let (look, range, max) = (Look::of(m), m.n("Range") * 1.5, m.n("Max") as usize);
        with(|s| {
            let mut drawn = 0;
            for &(pos, col) in &s.blocks.found {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                if dist([lo[0] + 0.5, lo[1] + 0.5, lo[2] + 0.5]) > range {
                    continue;
                }
                if draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], col, lw) {
                    drawn += 1;
                    if drawn >= max {
                        break;
                    }
                }
            }
        });
    }

    if let Some(m) = get("Logout Spots") {
        let look = Look::of(m);
        let here = game.native_player().map(|p| p.dimension);
        with(|s| {
            for sp in s.spots.iter().filter(|sp| Some(sp.dim.as_str()) == here) {
                let lo = [sp.pos[0] - 0.3, sp.pos[1], sp.pos[2] - 0.3];
                let hi = [sp.pos[0] + 0.3, sp.pos[1] + 1.8, sp.pos[2] + 0.3];
                if !draw_box(cam, out, &look, lo, hi, 0xFFFF_5050, lw) || look.draw == Draw::Potato {
                    continue;
                }
                if let Some([x, y]) = cam.project([sp.pos[0], sp.pos[1] + 2.2, sp.pos[2]]) {
                    text.clear();
                    let _ = write!(text, "{} logged out {}m ago", sp.name, game.ticks.saturating_sub(sp.at) / 1200);
                    let tw = game.native_text_width(&text);
                    out.fill([x - tw / 2.0 - 2.0, y - 2.0, tw + 4.0, 12.0], 0x7800_0000);
                    out.text(&text, [x - tw / 2.0, y], 0xFF5555);
                }
            }
        });
    }

    if let Some(m) = get("Trajectories") {
        trajectory(game, cam, m.p("Mode") == "Potato", out, lw);
    }

    if let Some(m) = get("Hole ESP") {
        let (look, height, max) = (Look::of(m), m.n("Height").max(0.02), m.n("Max") as usize);
        let colours = [m.c("Bedrock color"), m.c("Obsidian color"), m.c("Mixed color")];
        with(|s| {
            let mut drawn = 0;
            for &(pos, kind) in &s.holes.found {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                if draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + height, lo[2] + 1.0], colours[usize::from(kind)], lw) {
                    drawn += 1;
                    if drawn >= max {
                        break;
                    }
                }
            }
        });
    }

    if let Some(m) = get("Waypoints") {
        let (range, dots, colour) = (m.n("Range"), m.p("Mode") == "Potato", 0xFF00_0000 | accent());
        let here = game.native_player().map(|p| p.dimension);
        with(|s| {
            for w in s.waypoints.iter().filter(|w| Some(w.dim.as_str()) == here) {
                let d = dist(w.pos);
                if d > range {
                    continue;
                }
                if dots {
                    potato(cam, out, w.pos, colour, lw);
                    continue;
                }
                let Some([x, y]) = cam.project(w.pos).filter(|p| cam.on_screen(*p, 200.0)) else {
                    continue;
                };
                text.clear();
                let _ = write!(text, "{} {}m", w.name, d.round());
                let tw = game.native_text_width(&text);
                out.fill([x - tw / 2.0 - 2.0, y - 2.0, tw + 4.0, 12.0], 0x7800_0000);
                out.text(&text, [x - tw / 2.0, y], accent());
                out.fill([x - 1.0, y + 11.0, 2.0, 4.0], colour);
            }
        });
    }
}

// The settings ESP, Tracers and Nametags draw with, read once a frame.
struct EntityLook {
    kinds: Kinds,
    look: Look,
    range: f64,
    max: usize,
    fade: f64,
    potato: bool,
}

impl EntityLook {
    fn of(m: &Module) -> Self {
        let look = Look::of(m);
        Self {
            kinds: Kinds::of(m),
            potato: look.draw == Draw::Potato,
            look,
            range: m.n("Range"),
            max: m.n("Max") as usize,
            fade: m.n("Fade distance"),
        }
    }
}

// ESP, Tracers and Nametags in one pass over the mobs: those in range of any of them, nearest first when a Max
// leaves some out.
fn entity_overlays(game: &Game<'_>, cam: &Camera, mods: [Option<&Module>; 3], lw: f32, out: &mut impl Canvas) {
    let [esp, tracers, tags] = mods.map(|m| m.map(EntityLook::of));
    let tags_module = mods[2];
    let (tracer_target, tracer_bottom, tracer_by_distance) =
        mods[1].map_or(("Body", false, false), |m| (m.p("Target"), m.p("Start") == "Bottom", m.b("Distance colors")));
    let (tag_players, tag_mobs, tag_friends) =
        tags_module.map_or((false, false, false), |m| (m.b("Players"), m.b("Mobs"), !m.b("Ignore friends")));
    let (show_health, show_distance, tag_back, tag_colour) =
        tags_module.map_or((false, false, false, 0), |m| (m.b("Health"), m.b("Distance"), m.b("Background"), m.c("Name color") & 0x00FF_FFFF));
    let looks = [&esp, &tracers, &tags];
    let range = looks.iter().filter_map(|l| l.as_ref().map(|l| l.range)).fold(0.0, f64::max);
    let alpha = game.clock.camera_fraction();
    let lerp = |a: f64, b: f64| a + alpha * (b - a);
    let own = own_id(game);
    // The mobs in range: distance, index, and the box where the frame draws them.
    let mut near: Vec<(f64, usize, [f64; 3], [f64; 3])> = Vec::with_capacity(game.mobs.len());
    for (i, mob) in game.mobs.iter().enumerate() {
        if !mob.alive() || mob.removed || Some(mob.id) == own {
            continue;
        }
        let b = mob.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let off = [
            lerp(mob.previous.x, mob.position.x) - mob.position.x,
            lerp(mob.previous.y, mob.position.y) - mob.position.y,
            lerp(mob.previous.z, mob.position.z) - mob.position.z,
        ];
        let lo = [lo.x + off[0], lo.y + off[1], lo.z + off[2]];
        let hi = [hi.x + off[0], hi.y + off[1], hi.z + off[2]];
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let d = super::sunset_util::dist(mid, cam.pos);
        if d <= range {
            near.push((d, i, lo, hi));
        }
    }
    if looks.iter().any(|l| l.as_ref().is_some_and(|l| l.max < near.len())) {
        near.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    }
    // Friends only matter to the full modes.
    let friendly = esp.as_ref().is_some_and(|l| !l.potato) || tracers.as_ref().is_some_and(|l| !l.potato) || tags.is_some();
    let names = if friendly { game.remote_player_names() } else { Vec::new() };
    let friends = if friendly { with(|s| s.friends.clone()) } else { Vec::new() };
    let centre = [cam.w as f32 / 2.0, cam.h as f32 / 2.0];
    let from = if tracer_bottom { [centre[0], cam.h as f32] } else { centre };
    let mut drawn = [0usize; 3];
    let (mut name, mut hp, mut tail) = (String::new(), String::new(), String::new());
    for &(d, i, lo, hi) in &near {
        let mob = &game.mobs[i];
        let kind = kind_of(mob);
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let friend = || kind == Kind::Player && is_friend(&friends, &names, mob.id);
        if let Some(l) = &esp
            && drawn[0] < l.max
            && d <= l.range
            && l.kinds.wants(kind)
        {
            let col = if l.potato { l.kinds.colour(kind, false) } else { faded(l.kinds.colour(kind, friend()), d, l.fade) };
            if draw_box(cam, out, &l.look, lo, hi, col, lw) {
                drawn[0] += 1;
            }
        }
        if let Some(l) = &tracers
            && drawn[1] < l.max
            && d <= l.range
            && l.kinds.wants(kind)
        {
            // Potato: the line alone, to the middle, in the kind's colour.
            let (aim, col) = if l.potato {
                (mid, l.kinds.colour(kind, false))
            } else {
                let aim = match tracer_target {
                    "Head" => [mid[0], hi[1] - 0.1, mid[2]],
                    "Feet" => [mid[0], lo[1], mid[2]],
                    _ => mid,
                };
                (aim, if tracer_by_distance { distance_colour(d) } else { l.kinds.colour(kind, friend()) })
            };
            if let Some(to) = cam.project(aim) {
                line(out, from, to, col, lw);
                drawn[1] += 1;
            }
        }
        if let Some(l) = &tags
            && drawn[2] < l.max
            && d <= l.range
            && (if kind == Kind::Player { tag_players } else { tag_mobs })
        {
            let top = [mid[0], hi[1] + 0.6, mid[2]];
            if !cam.sees(top, 1.0) {
                continue;
            }
            let friend = friend();
            if friend && !tag_friends {
                continue;
            }
            let Some([x, y]) = cam.project(top) else {
                continue;
            };
            drawn[2] += 1;
            super::sunset_util::name_into(game, mob, &mut name);
            let name_colour = if friend { 0x55FFFF } else { tag_colour };
            let nw = game.native_text_width(&name);
            // Potato: the name alone.
            if l.potato {
                out.text(&name, [x - nw / 2.0, y], name_colour);
                continue;
            }
            hp.clear();
            tail.clear();
            if show_health {
                let _ = write!(hp, "{}", (mob.health * 10.0).round() / 10.0);
            }
            if show_distance {
                let _ = write!(tail, " {}m", d.round());
            }
            let (hw, tw) = (game.native_text_width(&hp), game.native_text_width(&tail));
            let gap = if hp.is_empty() { 0.0 } else { 4.0 };
            let total = nw + gap + hw + tw;
            let x0 = x - total / 2.0;
            if tag_back {
                out.fill([x0 - 2.0, y - 2.0, total + 4.0, 12.0], 0x7800_0000);
            }
            out.text(&name, [x0, y], name_colour);
            if !hp.is_empty() {
                out.text(&hp, [x0 + nw + gap, y], health_colour(mob.health / mob.kind.max_health.max(1.0)));
            }
            if !tail.is_empty() {
                out.text(&tail, [x0 + nw + gap + hw, y], 0xAAAAAA);
            }
        }
    }
}

// Where what the player holds would land: a bow being drawn, or a throwable. Potato: only a square where it lands.
pub(super) fn trajectory(game: &Game<'_>, cam: &Camera, potato_only: bool, out: &mut impl Canvas, lw: f32) {
    let Some(stack) = game.held() else {
        return;
    };
    let name = stack.item.strip_prefix("minecraft:").unwrap_or(&stack.item);
    let (speed, gravity) = match name {
        "bow" => match game.using {
            Some((_, ticks)) => {
                let t = f64::from(ticks) / 20.0;
                (((t * t + t * 2.0) / 3.0).min(1.0) * 3.0, 0.05)
            }
            None => return,
        },
        "trident" => (2.5, 0.05),
        "ender_pearl" | "snowball" | "egg" => (1.5, 0.03),
        "splash_potion" | "lingering_potion" => (0.5, 0.05),
        "experience_bottle" => (0.7, 0.07),
        _ => return,
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let colour = 0xFF00_0000 | accent();
    let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
    let dir = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let mut pos = [eye[0], eye[1] - 0.1, eye[2]];
    let mut vel = [dir[0] * speed, dir[1] * speed, dir[2] * speed];
    let mut last = if potato_only { None } else { cam.project(pos) };
    for _ in 0..200 {
        for i in 0..3 {
            pos[i] += vel[i];
            vel[i] *= 0.99;
        }
        vel[1] -= gravity;
        let (x, y, z) = (pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32);
        let hit = solid_at(game, x, y, z);
        if !potato_only {
            let now = cam.project(pos);
            if let (Some(a), Some(b)) = (last, now) {
                line(out, a, b, colour, lw);
            }
            last = now;
        }
        if hit {
            if potato_only {
                potato(cam, out, pos, colour, lw);
                break;
            }
            let lo = [pos[0] - 0.25, pos[1] - 0.25, pos[2] - 0.25];
            let hi = [pos[0] + 0.25, pos[1] + 0.25, pos[2] + 0.25];
            draw_box(cam, out, &Look::BOXED, lo, hi, colour, lw);
            break;
        }
        if pos[1] < -80.0 {
            break;
        }
    }
}

pub(super) fn storage_colour(name: &str) -> Option<u32> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    Some(if n == "ender_chest" {
        0xFFB0_50FF
    } else if n == "chest" || n == "trapped_chest" || n.ends_with("copper_chest") {
        0xFFFF_A040
    } else if n.ends_with("shulker_box") {
        0xFFFF_70C0
    } else if n == "barrel" {
        0xFFC0_8040
    } else if matches!(n, "hopper" | "dropper" | "dispenser" | "furnace" | "blast_furnace" | "smoker" | "crafter") {
        0xFFB0_B0B0
    } else {
        return None;
    })
}

// A line, as one slanted quad.
pub(super) fn line(out: &mut impl Canvas, from: [f32; 2], to: [f32; 2], argb: u32, lw: f32) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len < 1.0 {
        return;
    }
    let half = lw.max(1.0) / 2.0;
    let (nx, ny) = (-dy / len * half, dx / len * half);
    out.quad(
        [[from[0] + nx, from[1] + ny], [to[0] + nx, to[1] + ny], [to[0] - nx, to[1] - ny], [from[0] - nx, from[1] - ny]],
        argb,
    );
}

// ---------------------------------------------------------------- click GUI
//
// Meteor's style: every category as a panel at once, over the world. Left
// click turns a module on or off, right click opens its settings under it;
// headers drag the panels (right click folds one). Numbers are sliders. The
// game keeps running (single player does not pause), the pointer is free and
// the window's keys and clicks come here while it is open.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Panel {
    // Negative until the first frame places it.
    x: f32,
    y: f32,
    collapsed: bool,
    scroll: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Drag {
    Panel(usize, f32, f32),
    // The row and scale at the press: a setting that resizes the GUI must not move the slider under the pointer.
    Slider(usize, usize, [f32; 4], f32),
    // The settings window by its title, where in it the pointer took it.
    Window(f32, f32),
}

#[derive(Clone, Debug)]
pub(super) struct Gui {
    open: bool,
    // Esc or the menu key closes it at the next frame, so the same key
    // press can't also reach the game (Esc would open the pause menu).
    closing: bool,
    panels: Vec<Panel>,
    expanded: Vec<usize>,
    drag: Option<Drag>,
    // The height of the GUI the panels last drew in.
    view_h: f32,
    // Typed letters: only the modules whose names contain them show.
    search: String,
    // Ctrl+A: the next letter replaces the whole search, Backspace clears it.
    select_all: bool,
    // The text editor over the panels (a left click on a text setting), the GUI's width when it last drew, and
    // whether the key just pressed was the editor's already (from the window's own event, with its text).
    editor: Option<Editor>,
    // A module's settings in their own window (ClickGUI Settings: Window).
    window: Option<Win>,
    view_w: f32,
    typed: bool,
    // Ctrl or Cmd held, as the window last said.
    shortcut: bool,
}

// A text setting being edited: the setting changes only when Apply is clicked (or Enter pressed).
#[derive(Clone, Debug)]
pub(super) struct Editor {
    module: usize,
    setting: usize,
    // A value from .set or the config that the editor can't hold (too long, more than one line) is shown, not cut.
    draft: Result<Draft, &'static str>,
    // A number typed in its own slider row, instead of the text editor's box over the panels.
    inline: bool,
}

fn editor_rect(w: f32, h: f32) -> [f32; 4] {
    let width = (w - 8.0).clamp(1.0, 320.0);
    [(w - width) / 2.0, ((h - 76.0) / 2.0).max(0.0), width, 76.0]
}

// Apply and Cancel.
fn editor_buttons(r: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let width = ((r[2] - 18.0) / 2.0).clamp(1.0, 56.0);
    ([r[0] + 6.0, r[1] + 54.0, width, 15.0], [r[0] + 12.0 + width, r[1] + 54.0, width, 15.0])
}

// The editor's draft into its setting, and the editor closed.
// A number goes in rounded to its step and kept in its range; what is not a number keeps the field open, in red.
fn apply_editor(s: &mut State) {
    let Some(mut edit) = s.gui.editor.take() else {
        return;
    };
    let Ok(draft) = &mut edit.draft else {
        return;
    };
    match s.modules.get_mut(edit.module).and_then(|m| m.settings.get_mut(edit.setting)).map(|st| &mut st.value) {
        Some(Value::Text(t)) => *t = draft.text().to_owned(),
        Some(Value::Num { v, min, max, step }) => match draft.text().trim().parse::<f64>() {
            Ok(n) if n.is_finite() => *v = round_to(n, *step).clamp(*min, *max),
            _ => {
                draft.error = Some("not a number");
                s.gui.editor = Some(edit);
            }
        },
        _ => {}
    }
}

fn edit_key(name: &str) -> Option<EditKey> {
    Some(match name {
        "key.keyboard.left" => EditKey::Left,
        "key.keyboard.right" => EditKey::Right,
        "key.keyboard.home" => EditKey::Home,
        "key.keyboard.end" => EditKey::End,
        "key.keyboard.backspace" => EditKey::Backspace,
        "key.keyboard.delete" => EditKey::Delete,
        _ => return None,
    })
}

// The window's key events while the editor is open: the text the keyboard layout makes goes into the draft (a real
// keyboard's keys come here first, then to gui_key, which leaves them be).
pub(super) fn text_event(event: &winit::event::WindowEvent) {
    use winit::event::{ElementState, WindowEvent};
    use winit::keyboard::{Key, NamedKey};
    with(|s| {
        if let WindowEvent::ModifiersChanged(m) = event {
            s.gui.shortcut = m.state().control_key() && !m.state().alt_key() || m.state().super_key();
        }
        let WindowEvent::KeyboardInput { event, is_synthetic: false, .. } = event else {
            return;
        };
        if event.state != ElementState::Pressed || !s.gui.open || s.gui.editor.is_none() {
            return;
        }
        s.gui.typed = true;
        let key = match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                s.gui.editor = None;
                return;
            }
            Key::Named(NamedKey::Enter) => {
                apply_editor(s);
                return;
            }
            Key::Named(NamedKey::ArrowLeft) => Some(EditKey::Left),
            Key::Named(NamedKey::ArrowRight) => Some(EditKey::Right),
            Key::Named(NamedKey::Home) => Some(EditKey::Home),
            Key::Named(NamedKey::End) => Some(EditKey::End),
            Key::Named(NamedKey::Backspace) => Some(EditKey::Backspace),
            Key::Named(NamedKey::Delete) => Some(EditKey::Delete),
            Key::Character(c) if s.gui.shortcut && c.eq_ignore_ascii_case("a") => Some(EditKey::SelectAll),
            _ => None,
        };
        let shortcut = s.gui.shortcut;
        let Some(Editor { draft: Ok(draft), .. }) = s.gui.editor.as_mut() else {
            return;
        };
        match (key, event.text.as_deref()) {
            (Some(k), _) => draft.key(k),
            (None, Some(text)) if !shortcut && !text.chars().any(char::is_control) => draft.insert(text),
            _ => {}
        }
    });
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            open: false,
            closing: false,
            select_all: false,
            editor: None,
            window: None,
            view_w: 0.0,
            typed: false,
            shortcut: false,
            panels: (0..CATEGORIES.len())
                .map(|_| Panel { x: -1.0, y: 32.0, collapsed: false, scroll: 0.0 })
                .collect(),
            expanded: Vec::new(),
            drag: None,
            view_h: 240.0,
            search: String::new(),
        }
    }
}

// Row heights, Comfortable or Compact (ClickGUI's Spacing): a header, a row, and a slider, whose track is under its text.
fn spaced(comfortable: f32, compact: f32) -> f32 {
    if LOOK[10].load(Ordering::Relaxed) == 0 { comfortable } else { compact }
}
fn head_h() -> f32 {
    spaced(16.0, 13.0)
}
fn row_hh() -> f32 {
    spaced(14.0, 11.0)
}
fn slider_h() -> f32 {
    spaced(19.0, 15.0)
}
// To the right of what the rows draw (the scroll marker sits there).
const RIGHT_PAD: f32 = 7.0;
// The panel's own space under its last row: the last row's text is as far from the bottom as the header's from the top.
fn body_pad() -> f32 {
    (head_h() - row_hh()) / 2.0
}
// Text is this far down a row of its height.
const fn text_y(h: f32) -> f32 {
    (h - 8.0) / 2.0
}
// A row of panels still narrow enough to read, in GUI pixels at scale 1: GUI scale stops where this fills the window.
const MIN_ROW: f32 = 4.0 + 5.0 * 70.0 + 4.0 * 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Hit {
    Header(usize),
    Module(usize),
    Bind(usize),
    Setting(usize, usize),
    // The settings window: anywhere on it, its title (to drag it) and its close button.
    WinBody,
    WinTitle,
    WinClose,
}

// Other names a module is known by, so the search finds it under either.
fn aliases(name: &str) -> &'static str {
    match name {
        "Step" => "step up stepup stairs",
        "ClickGUI" => "gui menu color colour theme accent opacity width dim blur background indent scale size zoom bigger radius corner round",
        "Glide" => "slow fall slowfall",
        "Panic" => "disable all off hide",
        "Auto Steal" => "chest stealer loot",
        "Inventory Cleaner" => "inventory cleaner junk drop clean",
        "GUI Move" => "inventory move invmove gui walk",
        "Reverse Step" => "step down stepdown",
        "Flight" => "fly",
        "Kill Aura" => "aura killaura",
        "Break Delay" => "fast break nuker delay",
        "Fast Place" => "fastplace place speed delay",
        "Fullbright" => "gamma night vision",
        "Criticals" => "crits",
        "Auto Totem" => "totem",
        "Xray" => "x-ray",
        "No Render" => "no hide fog fog water darkness blindness overlay",
        _ => "",
    }
}

fn row_h(s: &State, hit: Hit) -> f32 {
    match hit {
        Hit::Setting(i, j) if matches!(s.modules[i].settings[j].value, Value::Num { .. }) => slider_h(),
        _ => row_hh(),
    }
}

// The rows of one panel before scrolling: their offset from the top of its
// body, what they are, and how tall the whole body is.
pub(super) fn panel_body(s: &State, c: usize) -> (Vec<(f32, Hit)>, f32) {
    let mut rows = Vec::new();
    let mut y = 0.0;
    let query = norm(&s.gui.search);
    for (i, m) in s
        .modules
        .iter()
        .enumerate()
        .filter(|(_, m)| m.category == CATEGORIES[c] && (norm(m.name).contains(&query) || norm(aliases(m.name)).contains(&query)))
    {
        rows.push((y, Hit::Module(i)));
        y += row_hh();
        if s.gui.expanded.contains(&i) {
            rows.push((y, Hit::Bind(i)));
            y += row_hh();
            for j in 0..m.settings.len() {
                if m.setting_visible(j) {
                    let hit = Hit::Setting(i, j);
                    rows.push((y, hit));
                    y += row_h(s, hit);
                }
            }
        }
    }
    (rows, y)
}

// How much of a panel's body shows: what is left of the screen under it.
pub(super) fn body_room(s: &State, c: usize) -> f32 {
    (s.gui.view_h - s.gui.panels[c].y - head_h() - 14.0).max(row_hh() * 3.0)
}

// Each row of every panel, top to bottom: its rectangle and what it is. A
// row only part way in view is left out.
fn panel_rows(s: &State) -> Vec<([f32; 4], Hit)> {
    let mut out = Vec::new();
    for c in 0..CATEGORIES.len() {
        let p = s.gui.panels[c];
        let (rows, _) = panel_body(s, c);
        // A search leaves out the panels with nothing that matches.
        if rows.is_empty() && !s.gui.search.is_empty() {
            continue;
        }
        out.push(([p.x, p.y, panel_w(), head_h()], Hit::Header(c)));
        if p.collapsed {
            continue;
        }
        let room = body_room(s, c);
        for (off, hit) in rows {
            let top = off - p.scroll;
            let rh = row_h(s, hit);
            if top < -0.01 || top + rh > room + 0.01 {
                continue;
            }
            out.push(([p.x, p.y + head_h() + top, panel_w(), rh], hit));
        }
    }
    out
}

// What the pointer can hit: the panels' rows, then the settings window over them.
pub(super) fn gui_rows(s: &State) -> Vec<([f32; 4], Hit)> {
    let mut rows = panel_rows(s);
    rows.extend(window_hits(s));
    rows
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Win {
    module: usize,
    x: f32,
    y: f32,
    scroll: f32,
    // As it last drew: its description's height, its rows' height and how much of them shows.
    about_h: f32,
    content: f32,
    room: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum WinRow {
    Group(&'static str),
    Row(Hit),
}

fn win_w(s: &State) -> f32 {
    (panel_w() * 1.6).max(150.0).min(s.gui.view_w - 8.0)
}

// The window's rows under its title and description, each group under its name: the settings, then the bind.
fn win_body(s: &State, i: usize) -> (Vec<(f32, WinRow)>, f32) {
    let m = &s.modules[i];
    let (bind, general): (Vec<usize>, Vec<usize>) =
        (0..m.settings.len()).filter(|&j| m.setting_visible(j)).partition(|&j| m.settings[j].name == "Bind mode");
    let groups = [
        ("General", general.into_iter().map(|j| Hit::Setting(i, j)).collect::<Vec<_>>()),
        ("Bind", std::iter::once(Hit::Bind(i)).chain(bind.into_iter().map(|j| Hit::Setting(i, j))).collect()),
    ];
    let mut rows = Vec::new();
    let mut y = 0.0;
    for (name, hits) in groups.into_iter().filter(|(_, hits)| !hits.is_empty()) {
        rows.push((y, WinRow::Group(name)));
        y += row_hh();
        for hit in hits {
            rows.push((y, WinRow::Row(hit)));
            y += row_h(s, hit);
        }
    }
    (rows, y)
}

fn win_frame(s: &State, w: &Win) -> [f32; 4] {
    [w.x, w.y, win_w(s), head_h() + w.about_h + w.content.min(w.room) + body_pad()]
}

fn win_close(frame: [f32; 4]) -> [f32; 4] {
    [frame[0] + frame[2] - head_h(), frame[1], head_h(), head_h()]
}

// The window's rows in view, and where.
fn win_rows(s: &State) -> Vec<([f32; 4], WinRow)> {
    let Some(w) = s.gui.window else {
        return Vec::new();
    };
    let [x, y, ww, _] = win_frame(s, &w);
    let top = y + head_h() + w.about_h;
    win_body(s, w.module)
        .0
        .into_iter()
        .filter_map(|(off, row)| {
            let rh = match row {
                WinRow::Row(hit) => row_h(s, hit),
                WinRow::Group(_) => row_hh(),
            };
            let at = off - w.scroll;
            (at >= -0.01 && at + rh <= w.room + 0.01).then_some(([x, top + at, ww, rh], row))
        })
        .collect()
}

// What a click on the window hits: before anything under it.
fn window_hits(s: &State) -> Vec<([f32; 4], Hit)> {
    let Some(w) = s.gui.window else {
        return Vec::new();
    };
    let frame = win_frame(s, &w);
    let mut out = vec![(frame, Hit::WinBody), ([frame[0], frame[1], frame[2], head_h()], Hit::WinTitle), (win_close(frame), Hit::WinClose)];
    out.extend(win_rows(s).into_iter().filter_map(|(r, row)| match row {
        WinRow::Row(hit) => Some((r, hit)),
        WinRow::Group(_) => None,
    }));
    out
}

// A number field's width, for its text.
fn field_width(game: &Game<'_>, text: &str) -> f32 {
    (game.native_text_width(text) + 6.0).max(24.0)
}

// A description in lines that fit `width`.
fn wrap(game: &Game<'_>, text: &str, width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(l) if game.native_text_width(&format!("{l} {word}")) <= width => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

// The pointer in the GUI's own coordinates, which GUI scale divides.
fn gui_cursor(game: &Game<'_>) -> (f32, f32) {
    let scale = gui_scale();
    (game.cursor.0 / scale, game.cursor.1 / scale)
}

// A panel's scroll moved by some rows: it always starts on a row, and stops at the first that shows the panel's end.
fn snap_scroll(rows: &[(f32, Hit)], content: f32, room: f32, scroll: f32, by: i32) -> f32 {
    let max = (content - room).max(0.0);
    let stops = rows.iter().position(|(off, _)| *off >= max - 0.01).map_or(rows.len(), |i| i + 1);
    let Some(last) = stops.checked_sub(1) else {
        return 0.0;
    };
    let at = rows[..stops].iter().position(|(off, _)| *off >= scroll - 0.01).unwrap_or(last);
    rows[(at as i32 + by).clamp(0, last as i32) as usize].0
}

// The wheel over a panel scrolls it: the top one, where panels overlap.
pub(super) fn gui_wheel(game: &Game<'_>, steps: f32) {
    let at = gui_cursor(game);
    with(|s| {
        // Over the settings window: it scrolls.
        if let Some(w) = s.gui.window
            && inside(win_frame(s, &w), at)
        {
            let max = (w.content - w.room).max(0.0);
            s.gui.window = Some(Win { scroll: (w.scroll - steps * row_hh() * 2.0).clamp(0.0, max), ..w });
            return;
        }
        for c in (0..CATEGORIES.len()).rev() {
            let p = s.gui.panels[c];
            let (rows, content) = panel_body(s, c);
            if rows.is_empty() && !s.gui.search.is_empty() {
                continue;
            }
            let room = body_room(s, c);
            let shown = if p.collapsed { 0.0 } else { content.min(room) + body_pad() };
            if inside([p.x, p.y, panel_w(), head_h() + shown], at) {
                let by = -steps * 2.0;
                let by = if by > 0.0 { by.ceil() } else { by.floor() } as i32;
                s.gui.panels[c].scroll = snap_scroll(&rows, content, room, p.scroll, by);
                return;
            }
        }
    });
}

pub(super) fn inside(r: [f32; 4], (x, y): (f32, f32)) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

pub(super) fn hit_at(s: &State, at: (f32, f32)) -> Option<([f32; 4], Hit)> {
    // The last drawn is the one on top, where panels overlap.
    gui_rows(s).into_iter().rfind(|(r, _)| inside(*r, at))
}

pub(super) fn open_gui(game: &mut Game<'_>) {
    release_binds();
    if !game.native_playing() {
        return;
    }
    ensure_loaded(game);
    game.keys.clear();
    game.capture(false);
    with(|s| {
        s.gui.open = true;
        s.gui.closing = false;
        s.gui.drag = None;
        s.binding = None;
    });
}

pub(super) fn close_gui(game: &mut Game<'_>) {
    with(|s| {
        s.gui.open = false;
        s.gui.closing = false;
        s.gui.drag = None;
        s.gui.editor = None;
        s.gui.window = None;
        s.binding = None;
    });
    save(game);
    if game.screen == Screen::Playing {
        game.capture(true);
    }
}

// Where a slider's track starts in its row, and how long it is.
fn slider_track(r: [f32; 4]) -> (f32, f32) {
    let x0 = r[0] + indent() + 4.0;
    (x0, (r[0] + r[2] - RIGHT_PAD - x0).max(1.0))
}

// A slider's value from the pointer's place along its row.
pub(super) fn slide(s: &mut State, i: usize, j: usize, r: [f32; 4], x: f32) -> bool {
    let Some(setting) = s.modules[i].settings.get_mut(j) else {
        return false;
    };
    if let Value::Num { v, min, max, step } = &mut setting.value {
        let (x0, track) = slider_track(r);
        let t = ((x - x0) / track).clamp(0.0, 1.0);
        let n = round_to(*min + f64::from(t) * (*max - *min), *step).clamp(*min, *max);
        let changed = (n - *v).abs() > 1e-9;
        *v = n;
        return changed;
    }
    false
}

pub(super) fn gui_open() -> bool {
    with(|s| s.gui.open)
}

// A key while the GUI is open: a bind being chosen, or Esc and the menu key
// closing it. Every key stops here.
pub(super) fn gui_key(game: &mut Game<'_>, name: &str) {
    // The editor takes every key: from the window's event already (text_event), or else here (a key without text of
    // its own, as horizon-ctl's are, types its letter).
    if with(|s| s.gui.editor.is_some() || std::mem::take(&mut s.gui.typed)) {
        let control = game.modifiers.control_key();
        with(|s| {
            if std::mem::take(&mut s.gui.typed) {
                return;
            }
            if name == "key.keyboard.escape" {
                s.gui.editor = None;
                return;
            }
            if name.ends_with("enter") {
                apply_editor(s);
                return;
            }
            let Some(Editor { draft: Ok(draft), .. }) = s.gui.editor.as_mut() else {
                return;
            };
            if control && name == "key.keyboard.a" {
                draft.key(EditKey::SelectAll);
            } else if let Some(k) = edit_key(name) {
                draft.key(k);
            } else if let Some(c) = key_char(name).filter(|_| !control) {
                draft.insert(&c.to_string());
            }
        });
        return;
    }
    if let Some(i) = with(|s| s.binding) {
        let chosen = match name {
            "key.keyboard.escape" => {
                with(|s| s.binding = None);
                return;
            }
            "key.keyboard.backspace" | "key.keyboard.delete" => None,
            k => Some(k.to_owned()),
        };
        with(|s| {
            set_bind(s, i, chosen);
            s.binding = None;
        });
        save(game);
        return;
    }
    if name == "key.keyboard.escape" && with(|s| s.gui.window.take().is_some()) {
        // Esc closes the settings window before the menu.
        return;
    }
    if name == "key.keyboard.escape" {
        // The first Esc clears a search; the next closes the GUI.
        with(|s| s.gui.select_all = false);
        if with(|s| std::mem::take(&mut s.gui.search)).is_empty() {
            with(|s| s.gui.closing = true);
        }
        return;
    }
    if is_menu_key(game, name) {
        with(|s| s.gui.closing = true);
        return;
    }
    let control = game.modifiers.control_key();
    if name.ends_with("enter") {
        // Enter in the search turns on (or off) the module it found.
        if let Some(i) = with(|s| first_match(s)) {
            toggle(game, i, true);
        }
    } else if control && name == "key.keyboard.a" {
        with(|s| s.gui.select_all = !s.gui.search.is_empty());
    } else if name == "key.keyboard.backspace" || name == "key.keyboard.delete" {
        with(|s| {
            if control || s.gui.select_all {
                s.gui.search.clear();
            } else {
                s.gui.search.pop();
            }
            s.gui.select_all = false;
        });
    } else if let Some(c) = key_char(name)
        && !control
    {
        with(|s| {
            if std::mem::take(&mut s.gui.select_all) {
                s.gui.search.clear();
            }
            if s.gui.search.chars().count() < 20 {
                s.gui.search.push(c);
            }
        });
    }
}

// The module a search points at: an exact name, then a name that starts with it, then one that contains it, then
// one known by another name.
fn first_match(s: &State) -> Option<usize> {
    let query = norm(&s.gui.search);
    if query.is_empty() {
        return None;
    }
    s.modules
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let name = norm(m.name);
            let rank = if name == query {
                0
            } else if name.starts_with(&query) {
                1
            } else if name.contains(&query) {
                2
            } else if norm(aliases(m.name)).contains(&query) {
                3
            } else {
                return None;
            };
            Some((rank, i))
        })
        .min()
        .map(|(_, i)| i)
}

// A letter, digit or space typed in the click GUI's search.
pub(super) fn key_char(name: &str) -> Option<char> {
    let k = name.strip_prefix("key.keyboard.")?;
    if k == "space" {
        return Some(' ');
    }
    let mut chars = k.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c.is_ascii_alphanumeric()).then_some(c)
}

// A mouse button pressed while the GUI is open (its release is read from
// the held buttons each frame).
pub(super) fn gui_press(game: &mut Game<'_>, button: winit::event::MouseButton) {
    use winit::event::MouseButton;
    let at = gui_cursor(game);
    // A bind being chosen takes any button but the left one (which clicks as ever).
    let captured = with(|s| {
        let i = s.binding.filter(|_| button != MouseButton::Left && s.gui.editor.is_none())?;
        set_bind(s, i, Some(horizon_client::keybinds::mouse_name(button)));
        s.binding = None;
        Some(())
    });
    if captured.is_some() {
        save(game);
        return;
    }
    let right = match button {
        MouseButton::Left => false,
        MouseButton::Right => true,
        _ => return,
    };
    let mut toggled = None;
    let mut changed = false;
    with(|s| {
        // The editor is in front of everything: its buttons, nothing else.
        if s.gui.editor.is_some() {
            let (apply, cancel) = editor_buttons(editor_rect(s.gui.view_w, s.gui.view_h));
            if !right && inside(apply, at) {
                apply_editor(s);
                changed = true;
            } else if !right && inside(cancel, at) {
                s.gui.editor = None;
            }
            return;
        }
        let found = hit_at(s, at);
        // A number being typed: a click in its field leaves it be; anywhere else, it is left as it was.
        if let Some(Editor { module, setting, draft: Ok(draft), inline: true }) = &s.gui.editor {
            let in_field = found.is_some_and(|(r, hit)| {
                hit == Hit::Setting(*module, *setting) && at.0 >= r[0] + r[2] - RIGHT_PAD - field_width(game, draft.text()) && at.1 < r[1] + row_hh()
            });
            if in_field {
                return;
            }
            s.gui.editor = None;
        }
        let Some((r, hit)) = found else {
            return;
        };
        match (hit, right) {
            (Hit::WinTitle, false) => {
                if let Some(w) = s.gui.window {
                    s.gui.drag = Some(Drag::Window(at.0 - w.x, at.1 - w.y));
                }
            }
            (Hit::WinClose, false) => s.gui.window = None,
            (Hit::WinBody | Hit::WinTitle | Hit::WinClose, _) => {}
            (Hit::Header(c), false) => {
                let p = s.gui.panels[c];
                s.gui.drag = Some(Drag::Panel(c, at.0 - p.x, at.1 - p.y));
            }
            (Hit::Header(c), true) => {
                s.gui.panels[c].collapsed = !s.gui.panels[c].collapsed;
                changed = true;
            }
            (Hit::Module(i), false) => toggled = Some(i),
            (Hit::Module(i), true) if s.module("ClickGUI").is_some_and(|m| m.p("Settings") == "Inline") => {
                if let Some(k) = s.gui.expanded.iter().position(|&e| e == i) {
                    s.gui.expanded.remove(k);
                } else {
                    s.gui.expanded.push(i);
                }
            }
            // Its own window, in the middle at first (the same module again closes it).
            (Hit::Module(i), true) => {
                s.gui.window = match s.gui.window {
                    Some(w) if w.module == i => None,
                    _ => Some(Win { module: i, x: (s.gui.view_w - win_w(s)) / 2.0, y: 40.0, scroll: 0.0, about_h: 0.0, content: 0.0, room: 0.0 }),
                };
                s.binding = None;
            }
            (Hit::Bind(i), false) => s.binding = if s.binding == Some(i) { None } else { Some(i) },
            (Hit::Bind(i), true) => {
                s.modules[i].key = None;
                s.binding = None;
                changed = true;
            }
            (Hit::Setting(i, j), right) => {
                let number = match s.modules[i].settings[j].value {
                    Value::Num { v, .. } => Some(v),
                    _ => None,
                };
                // A click on the number: it is typed instead (all of it selected, so typing replaces it).
                if let (Some(v), false) = (number, right)
                    && at.0 >= r[0] + r[2] - RIGHT_PAD - game.native_text_width(&num(v)) - 3.0
                    && at.1 < r[1] + row_hh()
                {
                    let mut draft = Draft::new(&num(v));
                    if let Ok(d) = &mut draft {
                        d.key(EditKey::SelectAll);
                    }
                    s.gui.editor = Some(Editor { module: i, setting: j, draft, inline: true });
                    s.gui.drag = None;
                    s.binding = None;
                } else if number.is_some() && !right {
                    s.gui.drag = Some(Drag::Slider(i, j, r, gui_scale()));
                    changed = slide(s, i, j, r, at.0);
                } else if number.is_some() {
                    // Right click on a slider: back to its default.
                    let fresh = defaults().swap_remove(i).settings.swap_remove(j);
                    s.modules[i].settings[j] = fresh;
                    changed = true;
                } else if let (Value::Text(t), false) = (&s.modules[i].settings[j].value, right) {
                    s.gui.editor = Some(Editor { module: i, setting: j, draft: Draft::new(t), inline: false });
                    s.gui.drag = None;
                    s.binding = None;
                } else {
                    s.modules[i].settings[j].click(right);
                    changed = true;
                }
            }
        }
    });
    if let Some(i) = toggled {
        toggle(game, i, false);
    } else if changed {
        save(game);
    }
}

// A drag follows the pointer each frame.
pub(super) fn follow_drag(game: &Game<'_>, w: f32, h: f32) {
    let at = gui_cursor(game);
    with(|s| match s.gui.drag {
        Some(Drag::Panel(c, dx, dy)) => {
            let p = &mut s.gui.panels[c];
            p.x = (at.0 - dx).clamp(0.0, (w - panel_w()).max(0.0));
            p.y = (at.1 - dy).clamp(0.0, (h - head_h()).max(0.0));
        }
        Some(Drag::Slider(i, j, r, scale)) => {
            // The look follows as it is dragged, but for what moves the panels: that waits for the release (save).
            let layout = s.modules[i].name == "ClickGUI" && matches!(s.modules[i].settings[j].name, "Panel width" | "GUI scale" | "Text indent");
            if slide(s, i, j, r, game.cursor.0 / scale) && !layout {
                sync_flags(s);
            }
        }
        Some(Drag::Window(dx, dy)) => {
            if let Some(win) = &mut s.gui.window {
                (win.x, win.y) = (at.0 - dx, at.1 - dy);
            }
        }
        None => {}
    });
    // The left button came up: the drag ends where the pointer is.
    if !game.physical_buttons.contains(&winit::event::MouseButton::Left)
        && with(|s| s.gui.drag.take().is_some())
    {
        save(game);
    }
}

pub(super) fn draw_gui(game: &Game<'_>, w: f32, h: f32, px: f32, out: &mut Vec<Shape>) {
    follow_drag(game, w, h);
    // The panels are as wide as asked, or as wide as lets all of them sit in a row.
    let fits = ((w - 4.0 - 3.0 * (CATEGORIES.len() as f32 - 1.0)) / CATEGORIES.len() as f32).floor();
    let width = f32::from_bits(LOOK[1].load(Ordering::Relaxed)).min(fits).max(50.0);
    let before = LOOK[4].swap(width.to_bits(), Ordering::Relaxed);
    let was = f32::from_bits(before);
    with(|s| {
        s.gui.view_h = h;
        s.gui.view_w = w;
        if before != 0 && was != width {
            // Another width: the panels spread or close up and a layout of your own keeps its shape.
            let k = (width + 3.0) / (was + 3.0);
            for p in s.gui.panels.iter_mut().filter(|p| p.x >= 0.0) {
                p.x = (2.0 + (p.x - 2.0) * k).max(0.0);
            }
        }
        // Panels with no place yet go left to right, in a second row when the
        // window is narrow.
        let mut x = 2.0;
        let mut y = 32.0;
        for p in s.gui.panels.iter_mut().filter(|p| p.x < 0.0) {
            if x + panel_w() > w {
                x = 2.0;
                y += head_h() + 3.0;
            }
            (p.x, p.y) = (x, y);
            x += panel_w() + 3.0;
        }
        for p in &mut s.gui.panels {
            p.x = p.x.clamp(0.0, (w - panel_w()).max(0.0));
            p.y = p.y.clamp(0.0, (h - head_h()).max(0.0));
        }
        // A scroll past the end (a shorter body, a taller window) comes back.
        for c in 0..CATEGORIES.len() {
            let (rows, content) = panel_body(s, c);
            let room = body_room(s, c);
            s.gui.panels[c].scroll = snap_scroll(&rows, content, room, s.gui.panels[c].scroll, 0);
        }
    });
    // The settings window: its description in lines, sized to what it holds (scrolling past the screen's height),
    // kept on the screen.
    let about = with(|s| s.gui.window.map(|w| (s.modules[w.module].about, win_w(s))))
        .map_or_else(Vec::new, |(text, ww)| wrap(game, text, ww - 2.0 * indent()));
    let window = with(|s| {
        let mut win = s.gui.window?;
        win.about_h = if about.is_empty() { 0.0 } else { about.len() as f32 * 10.0 + 4.0 };
        win.content = win_body(s, win.module).1;
        win.room = (h - 8.0 - head_h() - win.about_h - body_pad()).max(row_hh() * 3.0);
        win.scroll = win.scroll.clamp(0.0, (win.content - win.room).max(0.0));
        let [_, _, ww, wh] = win_frame(s, &win);
        win.x = win.x.clamp(0.0, (w - ww).max(0.0));
        win.y = win.y.clamp(0.0, (h - wh).max(0.0));
        s.gui.window = Some(win);
        Some((win, win_frame(s, &win), win_rows(s), about))
    });
    let at = gui_cursor(game);
    let (rows, modules, binding, hover, editing) =
        with(|s| (panel_rows(s), s.modules.clone(), s.binding, hit_at(s, at).map(|(_, h)| h), s.gui.editor.clone()));
    // Modules whose settings are open (inline or in the window): a mark by the name.
    let open: Vec<usize> = with(|s| s.gui.expanded.iter().copied().chain(s.gui.window.map(|w| w.module)).collect());
    // Each panel's height of rows, room, scroll and whether it is folded.
    let bodies: Vec<(f32, f32, f32, bool)> = with(|s| {
        (0..CATEGORIES.len())
            .map(|c| (panel_body(s, c).1, body_room(s, c), s.gui.panels[c].scroll, s.gui.panels[c].collapsed))
            .collect()
    });
    let fit = |text: &str, max: f32| -> String {
        if game.native_text_width(text) <= max {
            return text.to_owned();
        }
        let mut t = text.to_owned();
        while !t.is_empty() && game.native_text_width(&format!("{t}...")) > max {
            t.pop();
        }
        format!("{t}...")
    };
    let solid = |rgb: u32| 0xFF00_0000 | rgb;
    let control = control_radius();
    // The label on the left of a row, cut short of what sits to its right: whether it had to be.
    let label = |out: &mut Vec<Shape>, text: &str, r: [f32; 4], x: f32, right: f32, colour: u32| -> bool {
        let room = r[0] + r[2] - RIGHT_PAD - right - x - 3.0;
        let shown = fit(text, room);
        let cut = shown != text;
        out.push(Shape::Text(shown, [x, r[1] + text_y(row_hh())], colour));
        cut
    };
    // A value in its rounded box at the right of a row; how wide it is.
    let value_box = |out: &mut Vec<Shape>, text: &str, r: [f32; 4], colour: u32| -> f32 {
        let text = fit(text, r[2] * 0.5);
        let tw = game.native_text_width(&text);
        let bw = tw + 8.0;
        let bx = r[0] + r[2] - RIGHT_PAD - bw;
        out.push(Shape::Round([bx, r[1] + 2.0, bw, row_hh() - 4.0], control, 0xFF1E_1E1E));
        out.push(Shape::Text(text, [bx + 4.0, r[1] + text_y(row_hh())], colour));
        bw
    };
    // A number being typed in its slider row: the field's width (the draft and its caret are drawn in it).
    let typing = |i: usize, j: usize| editing.as_ref().filter(|e| e.inline && e.module == i && e.setting == j).and_then(|e| e.draft.as_ref().ok());
    // One row of a panel or of the settings window: what it shows, and its name when it had to be cut short.
    let draw_row = |out: &mut Vec<Shape>, r: [f32; 4], hit: Hit, hovered: bool| -> Option<&'static str> {
        let [x, y, rw, rh] = r;
        let setting_x = x + indent() + 4.0;
        match hit {
            Hit::Module(i) => {
                let m = &modules[i];
                // A bound key shows at the row's end.
                let tag = m.key.as_deref().map(|k| fit(&key_label(Some(k)), 30.0)).unwrap_or_default();
                let tag_w = if tag.is_empty() { 0.0 } else { game.native_text_width(&tag) };
                // The name clears the mark of open settings at any Text indent.
                let name_x = x + indent().max(6.0);
                if open.contains(&i) {
                    out.push(Shape::Round([name_x - 5.0, y + 3.0, 2.0, rh - 6.0], 1.0, 0xFFE4_E4E4));
                }
                if !tag.is_empty() {
                    out.push(Shape::Text(tag, [x + rw - RIGHT_PAD - tag_w, y + text_y(rh)], 0x707070));
                }
                (label(out, m.name, r, name_x, tag_w, if m.on { accent() } else { 0xE4E4E4 }) && hovered).then_some(m.name)
            }
            Hit::Bind(i) => {
                let waiting = binding == Some(i);
                let key = if waiting { "Press a key or button".to_owned() } else { key_label(modules[i].key.as_deref()) };
                let bw = value_box(out, &key, r, if waiting { accent() } else { 0xC8C8C8 });
                label(out, "Bind", r, setting_x, bw, 0xB4B4B4);
                None
            }
            Hit::Setting(i, j) => {
                let st = &modules[i].settings[j];
                let right = match st.value {
                    Value::Bool(b) => {
                        let (tw, th) = (14.0, 8.0);
                        let (tx, ty) = (x + rw - RIGHT_PAD - tw, y + (rh - th) / 2.0);
                        // Off is the same pill in grey; on follows the accent colour.
                        out.push(Shape::Round([tx, ty, tw, th], control, if b { solid(accent()) } else { 0xFF5A_5A5A }));
                        let knob = th - 2.0;
                        let kx = if b { tx + tw - knob - 1.0 } else { tx + 1.0 };
                        out.push(Shape::Round([kx, ty + 1.0, knob, knob], (control - 1.0).max(0.0), 0xFF10_1010));
                        tw
                    }
                    Value::Num { v, min, max, .. } => {
                        let vw = match typing(i, j) {
                            Some(draft) => {
                                let fw = field_width(game, draft.text());
                                let fx = x + rw - RIGHT_PAD - fw + 3.0;
                                let colour = match (draft.error, draft.selected()) {
                                    (Some(_), _) => 0xFF60_2020,
                                    (None, true) => 0xFF30_60B0,
                                    (None, false) => 0xFF08_0808,
                                };
                                out.push(Shape::Round([fx - 3.0, y + 1.0, fw, row_hh() - 2.0], control, colour));
                                out.push(Shape::Text(draft.text().to_owned(), [fx, y + text_y(row_hh())], 0xFFFFFF));
                                let caret = game.native_text_width(&draft.text()[..draft.caret()]);
                                out.push(Shape::Fill([fx + caret, y + 3.0, 1.0, row_hh() - 6.0], solid(accent())));
                                fw
                            }
                            None => {
                                let value = num(v);
                                let vw = game.native_text_width(&value);
                                out.push(Shape::Text(value, [x + rw - RIGHT_PAD - vw, y + text_y(row_hh())], 0xE4E4E4));
                                vw
                            }
                        };
                        let (tx, track) = slider_track(r);
                        let t = if max > min { ((v - min) / (max - min)) as f32 } else { 0.0 };
                        let ty = y + row_hh();
                        out.push(Shape::Round([tx, ty, track, 2.0], 1.0, 0xFF2A_2A2A));
                        out.push(Shape::Round([tx, ty, track * t, 2.0], 1.0, solid(accent())));
                        out.push(Shape::Round([tx + track * t - 2.5, ty - 1.5, 5.0, 5.0], control.min(2.5), 0xFFE4_E4E4));
                        vw
                    }
                    Value::Pick { .. } => value_box(out, &st.value_text(), r, 0xE4E4E4),
                    Value::Text(_) => {
                        // The text itself: the name keeps its width, the text gets what is left (a quarter at least).
                        let room = x + rw - RIGHT_PAD - setting_x - game.native_text_width(st.name) - 6.0;
                        let value = fit(&st.value_text(), room.max(rw * 0.25));
                        let vw = game.native_text_width(&value);
                        out.push(Shape::Text(value, [x + rw - RIGHT_PAD - vw, y + text_y(row_hh())], 0x808080));
                        vw
                    }
                };
                (label(out, st.name, r, setting_x, right, 0xB4B4B4) && hovered).then_some(st.name)
            }
            _ => None,
        }
    };
    // The pointer's row lit edge to edge, between the lines; at the bottom of a panel, down to its rounded corners.
    let lit = |out: &mut Vec<Shape>, [x, y, rw, rh]: [f32; 4], line: bool, last: bool| {
        let top = if line { px } else { 0.0 };
        let r = [x, y + top, rw, rh - top + if last { body_pad() } else { 0.0 }];
        out.push(Shape::Corners(r, panel_radius(), [false, false, last, last], 0x14FF_FFFF));
    };
    let mut whole: Option<&str> = None;
    let mut first_row = false;
    for (k, (r, hit)) in rows.iter().enumerate() {
        let [x, y, rw, rh] = *r;
        let hovered = hover == Some(*hit);
        let line = if let Hit::Header(_) = hit {
            first_row = true;
            false
        } else if std::mem::take(&mut first_row) {
            // The header's line is under it already.
            false
        } else {
            out.push(Shape::Fill([x, y, rw, px], 0x12FF_FFFF));
            true
        };
        if let Hit::Header(c) = *hit {
            let (content, room, scroll, folded) = bodies[c];
            let body = if folded || content <= 0.0 { 0.0 } else { content.min(room) + body_pad() };
            out.push(Shape::Round([x, y, rw, rh + body], panel_radius(), bg(0xC00E_0E0E)));
            if body > 0.0 {
                out.push(Shape::Fill([x, y + rh - px, rw, px], 0x24FF_FFFF));
            }
            let name = CATEGORIES[c].label();
            let on = modules.iter().filter(|m| m.category == CATEGORIES[c] && m.on).count();
            let count = format!("{on}");
            let cw = game.native_text_width(&count);
            out.push(Shape::Head(fit(name, rw - indent() - RIGHT_PAD - cw - 3.0), [x + indent(), y + text_y(rh)], 0xFFFFFF));
            out.push(Shape::Text(count, [x + rw - RIGHT_PAD - cw, y + text_y(rh)], 0x808080));
            // The scroll marker: how much of the panel shows, and where.
            if !folded && content > room {
                let thumb = (room * room / content).max(10.0);
                let t = (scroll / (content - room)).min(1.0);
                out.push(Shape::Round([x + rw - 4.0, y + rh + 1.0 + (room - 2.0 - thumb) * t, 2.0, thumb], 1.0, 0x70FF_FFFF));
            }
            continue;
        }
        if hovered {
            let last = rows.get(k + 1).is_none_or(|(_, next)| matches!(next, Hit::Header(_)));
            lit(out, *r, line, last);
        }
        if let Some(name) = draw_row(out, *r, *hit, hovered) {
            whole = Some(name);
        }
    }
    // The settings window, over the panels: its title, close button, description, groups and rows.
    if let Some((win, frame, list, about)) = &window {
        let [x, y, ww, wh] = *frame;
        out.push(Shape::Round(*frame, panel_radius(), 0xF812_1212));
        let hh = head_h();
        out.push(Shape::Head(fit(modules[win.module].name, ww - indent() - hh - 4.0), [x + indent(), y + text_y(hh)], accent()));
        // The close button: a cross, lit under the pointer.
        let close = win_close(*frame);
        if hover == Some(Hit::WinClose) {
            out.push(Shape::Corners(close, panel_radius(), [false, true, false, false], 0x24FF_FFFF));
        }
        let (cx, cy, d) = (close[0] + close[2] / 2.0, close[1] + close[3] / 2.0, 3.0);
        for (a, b) in [([cx - d, cy - d], [cx + d, cy + d]), ([cx - d, cy + d], [cx + d, cy - d])] {
            line(out, a, b, 0xFFE4_E4E4, 1.5);
        }
        out.push(Shape::Fill([x, y + hh - px, ww, px], 0x24FF_FFFF));
        for (n, l) in about.iter().enumerate() {
            out.push(Shape::Text(l.clone(), [x + indent(), y + hh + 3.0 + n as f32 * 10.0], 0xA8A8A8));
        }
        let mut line = false;
        for (n, (r, row)) in list.iter().enumerate() {
            match row {
                WinRow::Group(name) => {
                    out.push(Shape::Text((*name).to_owned(), [r[0] + indent(), r[1] + text_y(r[3])], accent()));
                    line = false;
                }
                WinRow::Row(hit) => {
                    if line {
                        out.push(Shape::Fill([r[0], r[1], r[2], px], 0x12FF_FFFF));
                    }
                    let hovered = hover == Some(*hit);
                    if hovered {
                        lit(out, *r, line, n + 1 == list.len() && r[1] + r[3] + body_pad() >= y + wh - 0.5);
                    }
                    if let Some(name) = draw_row(out, *r, *hit, hovered) {
                        whole = Some(name);
                    }
                    line = true;
                }
            }
        }
        // Its scroll marker.
        let (content, room) = (win.content, win.room);
        if content > room {
            let thumb = (room * room / content).max(10.0);
            let t = (win.scroll / (content - room)).min(1.0);
            out.push(Shape::Round([x + ww - 4.0, y + hh + win.about_h + 1.0 + (room - 2.0 - thumb) * t, 2.0, thumb], 1.0, 0x70FF_FFFF));
        }
    }
    // The hovered module's description, under the pointer.
    if let Some(Hit::Module(i)) = hover
        && with(|s| s.gui.drag.is_none())
    {
        let about = match whole {
            Some(name) => format!("{name}: {}", modules[i].about),
            None => modules[i].about.to_owned(),
        };
        let about = about.as_str();
        let tw = game.native_text_width(about);
        let bx = (at.0 + 8.0).min(w - tw - 8.0).max(0.0);
        let by = (at.1 + 10.0).min(h - 14.0);
        out.push(Shape::Round([bx, by, tw + 8.0, 14.0], control, 0xF80A_0A0A));
        out.push(Shape::Text(about.to_owned(), [bx + 4.0, by + text_y(14.0)], 0xE4E4E4));
    }
    // A setting's name cut short, whole under the pointer.
    if let (Some(Hit::Setting(..)), Some(name)) = (hover, whole) {
        let tw = game.native_text_width(name);
        let bx = (at.0 + 8.0).min(w - tw - 8.0).max(0.0);
        let by = (at.1 + 10.0).min(h - 14.0);
        out.push(Shape::Round([bx, by, tw + 8.0, 14.0], control, 0xF80A_0A0A));
        out.push(Shape::Text(name.to_owned(), [bx + 4.0, by + text_y(14.0)], 0xE4E4E4));
    }
    // The text editor, over everything.
    if let Some(edit) = editing.filter(|e| !e.inline) {
        let r = editor_rect(w, h);
        let [x, y, rw, _] = r;
        let room = (rw - 16.0).max(0.0);
        out.push(Shape::Fill([0.0, 0.0, w, h], 0x9000_0000));
        out.push(Shape::Round(r, panel_radius(), 0xF814_1414));
        let m = &modules[edit.module];
        out.push(Shape::Head(fit(&format!("{} / {}", m.name, m.settings[edit.setting].name), room), [x + 8.0, y + 6.0], accent()));
        let selected = edit.draft.as_ref().is_ok_and(Draft::selected);
        out.push(Shape::Round([x + 6.0, y + 22.0, rw - 12.0, 16.0], control, if selected { 0xFF30_60B0 } else { 0xFF08_0808 }));
        let note = match &edit.draft {
            Ok(draft) => {
                // The end of the text near the caret shows, with the caret at its place.
                let before = &draft.text()[..draft.caret()];
                let mut from = 0;
                while game.native_text_width(&before[from..]) > room - 4.0 {
                    from += before[from..].chars().next().map_or(1, char::len_utf8);
                }
                let shown = fit(&draft.text()[from..], room);
                let caret = game.native_text_width(&before[from..]);
                out.push(Shape::Text(shown, [x + 10.0, y + 22.0 + text_y(16.0)], 0xFFFFFF));
                out.push(Shape::Fill([x + 10.0 + caret, y + 25.0, 1.0, 10.0], solid(accent())));
                draft.error.unwrap_or("Enter or Apply keeps it, Esc or Cancel leaves it as it was")
            }
            Err(e) => e,
        };
        out.push(Shape::Text(fit(note, room), [x + 8.0, y + 41.0], if matches!(&edit.draft, Ok(d) if d.error.is_none()) { 0x909090 } else { 0xFF7070 }));
        let (apply, cancel) = editor_buttons(r);
        for (b, label, colour) in [(apply, "Apply", solid(accent())), (cancel, "Cancel", 0xFF3A_3A3A)] {
            out.push(Shape::Round(b, control, colour));
            let tw = game.native_text_width(label);
            out.push(Shape::Text(label.to_owned(), [b[0] + (b[2] - tw) / 2.0, b[1] + text_y(b[3])], 0xFFFFFF));
        }
    }
    // The search box, and over it a reminder of the clicks.
    let search = with(|s| s.gui.search.clone());
    let (text, colour) = if search.is_empty() {
        ("Type to search".to_owned(), 0x909090)
    } else {
        (format!("Search: {search}_"), accent())
    };
    let selected = with(|s| s.gui.select_all) && !search.is_empty();
    let lw_ = game.native_text_width(&text);
    out.push(Shape::Round([(w - lw_) / 2.0 - 6.0, 14.0, lw_ + 12.0, 14.0], control, if selected { 0xFF30_60B0 } else { bg(0xC00E_0E0E) }));
    out.push(Shape::Text(text, [(w - lw_) / 2.0, 14.0 + text_y(14.0)], if selected { 0xFFFFFF } else { colour }));
    let hint = "Right click: settings   Enter: toggle search result   Esc: close";
    let hw = game.native_text_width(hint);
    out.push(Shape::Text(hint.to_owned(), [(w - hw) / 2.0, 3.0], 0xA0A0A0));
}
#[cfg(test)]
mod movement_config_tests {
    use super::*;

    #[test]
    fn existing_movement_settings_still_load() {
        let mut state = State::new();
        state.load_text("speed.mode=Strafe\nspeed.vanillaspeed=7.2\nvelocity.knockbackhorizontal=0.4\nscaffold.delayticks=2\n");
        assert_eq!(state.module("Speed").unwrap().p("Mode"), "Strafe");
        assert_eq!(state.module("Speed").unwrap().n("Vanilla speed"), 7.2);
        assert_eq!(state.module("Velocity").unwrap().n("Knockback horizontal"), 0.4);
        assert_eq!(state.module("Scaffold").unwrap().n("Delay (ticks)"), 2.0);
    }

    #[test]
    fn local_pre_rename_movement_values_migrate() {
        let mut state = State::new();
        state.load_text("speed.mode=Ground\nspeed.speed=8\nvelocity.horizontal=70\nvelocity.vertical=85\nvelocity.airhorizontal=50\nscaffold.rotation=None\nscaffold.maxdistance=4\n");
        assert_eq!(state.module("Speed").unwrap().p("Mode"), "Vanilla");
        assert_eq!(state.module("Speed").unwrap().n("Vanilla speed"), 8.0);
        assert_eq!(state.module("Velocity").unwrap().n("Knockback horizontal"), 0.7);
        assert_eq!(state.module("Velocity").unwrap().n("Knockback vertical"), 0.85);
        assert_eq!(state.module("Velocity").unwrap().n("Air knockback horizontal"), 0.5);
        assert!(!state.module("Scaffold").unwrap().b("Rotate"));
        assert_eq!(state.module("Scaffold").unwrap().n("Max placement distance"), 4.0);
    }

    #[test]
    fn presets_round_trip_through_config_and_profile() {
        let mut state = State::new();
        assert!(apply_movement_preset(&mut state, "aggressive"));
        let config = state.save_text();
        let profile = state.profile_text();
        let mut restored = State::new();
        restored.load_text(&config);
        assert_eq!(restored.module("Speed").unwrap().p("Mode"), "Custom");
        assert_eq!(restored.module("Speed").unwrap().n("Vanilla speed"), 10.0);
        assert_eq!(restored.module("Velocity").unwrap().n("Knockback horizontal"), 0.0);
        let mut from_profile = State::new();
        from_profile.load_profile(&profile);
        assert_eq!(from_profile.module("No Slow").unwrap().n("Ground %"), 100.0);
        assert!(!apply_movement_preset(&mut state, "unknown"));
    }

    #[test]
    fn air_knockback_and_blend_have_distinct_factors() {
        let mut state = State::new();
        let velocity = state.modules.iter_mut().find(|m| m.name == "Velocity").unwrap();
        for (name, value) in [("Mode", "Blend"), ("Use air values", "true"),
            ("Knockback horizontal", "0"), ("Air knockback horizontal", "0.5"), ("Blend %", "50")] {
            velocity.settings.iter_mut().find(|s| s.name == name).unwrap().parse(value);
        }
        assert_eq!(knockback_shares(velocity, true)[0], 0.5);
        assert_eq!(knockback_shares(velocity, false)[0], 0.75);
    }

    #[test]
    fn advanced_click_rows_follow_mode() {
        let mut state = State::new();
        let speed = state.modules.iter_mut().find(|m| m.name == "Speed").unwrap();
        let index = speed.settings.iter().position(|s| s.name == "Acceleration").unwrap();
        assert!(!speed.setting_visible(index));
        speed.settings.iter_mut().find(|s| s.name == "Advanced").unwrap().parse("true");
        speed.settings.iter_mut().find(|s| s.name == "Mode").unwrap().parse("Smooth");
        assert!(speed.setting_visible(index));
    }

    #[test]
    fn fast_place_speed_survives_config_round_trip() {
        let mut state = State::new();
        state.load_text("fastplace.speed=3\n");
        assert_eq!(state.module("Fast Place").unwrap().n("Speed"), 3.0);

        let mut restored = State::new();
        restored.load_text(&state.save_text());
        assert_eq!(restored.module("Fast Place").unwrap().n("Speed"), 3.0);
    }

    #[test]
    fn scaffold_rotation_restore_consumes_the_saved_view_once() {
        let mut saved = Some((135.0, -20.0));
        assert_eq!(take_scaffold_rotation(&mut saved), Some((135.0, -20.0)));
        assert_eq!(take_scaffold_rotation(&mut saved), None);
    }

    #[test]
    fn panic_cleanup_disables_modules_and_returns_scaffold_rotation() {
        let mut state = State::new();
        state.modules.iter_mut().find(|m| m.name == "Scaffold").unwrap().on = true;
        state.scaffold_rotation = Some((135.0, -20.0));
        assert_eq!(panic_off_state(&mut state), Some((135.0, -20.0)));
        assert!(!state.module("Scaffold").unwrap().on);
    }
}
