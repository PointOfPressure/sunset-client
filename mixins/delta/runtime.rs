// Delta Client: a Meteor Client style utility mod, rebuilt in Rust as a
// Horizon native mod. Modules in categories, each with settings and a key
// bind, a menu on vanilla's own Options > Mods page, `.` chat commands, and a
// HUD with the active modules. The bundle adds this file to the game as
// `crate::game::delta`, so it reads and changes the game's own state.
//
// Ordinary comments only: module payloads cannot hold inner attributes.

use super::{Game, Screen};
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

pub(crate) const ID: &str = "delta";
pub(crate) const NAME: &str = "Delta Client";
pub(crate) const VERSION: &str = "0.1.1";
pub(crate) const DESCRIPTION: &str =
    "Utility client: combat, movement and render modules, a menu, key binds and . commands.";
// The menu key's name on the mod's page and its default (Right Shift).
pub(crate) const MENU_KEY: &str = "menu";
pub(crate) const MENU_DEFAULT: &str = "key.keyboard.right.shift";
// Delta's blue.
const ACCENT: u32 = 0x4FA3FF;

// ---------------------------------------------------------------- modules

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Combat,
    Movement,
    Player,
    Render,
}

const CATEGORIES: [Category; 4] = [
    Category::Combat,
    Category::Movement,
    Category::Player,
    Category::Render,
];

impl Category {
    fn label(self) -> &'static str {
        match self {
            Category::Combat => "Combat",
            Category::Movement => "Movement",
            Category::Player => "Player",
            Category::Render => "Render",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Bool(bool),
    Num { v: f64, min: f64, max: f64, step: f64 },
    Pick { i: usize, of: &'static [&'static str] },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Setting {
    pub(crate) name: &'static str,
    pub(crate) value: Value,
}

impl Setting {
    fn shown(&self) -> String {
        match &self.value {
            Value::Bool(b) => format!("{}: {}", self.name, on_off(*b)),
            Value::Num { v, .. } => format!("{}: {}", self.name, num(*v)),
            Value::Pick { i, of } => format!("{}: {}", self.name, of[*i]),
        }
    }
    fn stored(&self) -> String {
        match &self.value {
            Value::Bool(b) => b.to_string(),
            Value::Num { v, .. } => num(*v),
            Value::Pick { i, of } => of[*i].to_owned(),
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
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Module {
    pub(crate) name: &'static str,
    pub(crate) category: Category,
    pub(crate) about: &'static str,
    pub(crate) on: bool,
    pub(crate) key: Option<String>,
    pub(crate) settings: Vec<Setting>,
}

impl Module {
    fn new(name: &'static str, category: Category, about: &'static str) -> Self {
        Self {
            name,
            category,
            about,
            on: false,
            key: None,
            settings: Vec::new(),
        }
    }
    fn on(mut self) -> Self {
        self.on = true;
        self
    }
    fn bool(mut self, name: &'static str, v: bool) -> Self {
        self.settings.push(Setting {
            name,
            value: Value::Bool(v),
        });
        self
    }
    fn num(mut self, name: &'static str, v: f64, min: f64, max: f64, step: f64) -> Self {
        self.settings.push(Setting {
            name,
            value: Value::Num { v, min, max, step },
        });
        self
    }
    fn pick(mut self, name: &'static str, of: &'static [&'static str]) -> Self {
        self.settings.push(Setting {
            name,
            value: Value::Pick { i: 0, of },
        });
        self
    }
    fn setting(&self, name: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.name == name)
    }
    pub(crate) fn b(&self, name: &str) -> bool {
        matches!(self.setting(name), Some(Setting { value: Value::Bool(true), .. }))
    }
    pub(crate) fn n(&self, name: &str) -> f64 {
        match self.setting(name) {
            Some(Setting { value: Value::Num { v, .. }, .. }) => *v,
            _ => 0.0,
        }
    }
    pub(crate) fn p(&self, name: &str) -> &'static str {
        match self.setting(name) {
            Some(Setting { value: Value::Pick { i, of }, .. }) => of[*i],
            _ => "",
        }
    }
    // What the module list shows beside the name (as Meteor's info string).
    fn info(&self) -> Option<String> {
        match self.name {
            "Flight" => Some(self.p("Mode").to_owned()),
            "Kill Aura" => Some(num(self.n("Range"))),
            "Velocity" => Some(format!("{}% {}%", num(self.n("Horizontal %")), num(self.n("Vertical %")))),
            "Speed" => Some(num(self.n("Speed"))),
            _ => None,
        }
    }
}

// Every module with its defaults, in menu order.
pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        // Combat
        Module::new("Kill Aura", Combat, "Attacks the nearest living thing in range.")
            .num("Range", 3.5, 1.0, 6.0, 0.5)
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", false)
            .bool("Wait for cooldown", true)
            .num("Delay (ticks)", 10.0, 0.0, 20.0, 1.0)
            .bool("Rotate", true)
            .bool("Pause while using", true),
        Module::new("Trigger Bot", Combat, "Attacks what your crosshair is on.")
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", false)
            .bool("Wait for cooldown", true)
            .num("Delay (ticks)", 10.0, 0.0, 20.0, 1.0),
        Module::new("Criticals", Combat, "Makes your hits from the ground critical hits."),
        Module::new("Auto Totem", Combat, "Moves a totem from your hotbar to your offhand.")
            .bool("Swap back", true),
        // Movement
        Module::new("Sprint", Movement, "Sprints for you whenever you walk forward."),
        Module::new("Auto Walk", Movement, "Walks forward for you."),
        Module::new("Flight", Movement, "Lets you fly.")
            .pick("Mode", &["Velocity", "Abilities"])
            .num("Speed", 10.0, 1.0, 50.0, 1.0)
            .bool("Anti kick", true),
        Module::new("Speed", Movement, "Moves you faster on the ground.")
            .num("Speed", 8.0, 1.0, 30.0, 0.5)
            .bool("Only on ground", true),
        Module::new("No Fall", Movement, "Stops fall damage on servers."),
        Module::new("Velocity", Movement, "Takes less knockback from hits and explosions.")
            .num("Horizontal %", 0.0, 0.0, 100.0, 10.0)
            .num("Vertical %", 0.0, 0.0, 100.0, 10.0),
        Module::new("Spider", Movement, "Climbs walls.")
            .num("Climb speed", 0.2, 0.1, 1.0, 0.1),
        Module::new("Step", Movement, "Steps up full blocks.")
            .num("Height", 1.0, 0.5, 3.0, 0.5),
        Module::new("Air Jump", Movement, "Jumps again in mid-air."),
        Module::new("Elytra Boost", Movement, "Speeds up gliding while you hold forward.")
            .num("Boost", 0.05, 0.01, 0.2, 0.01)
            .num("Max speed", 2.0, 0.5, 5.0, 0.5),
        Module::new("Anti Void", Movement, "Bounces you out of the void."),
        // Player
        Module::new("Fast Place", Player, "Removes the delay between placing blocks."),
        Module::new("Fast Break", Player, "Removes the delay between breaking blocks."),
        Module::new("Auto Respawn", Player, "Respawns as soon as you die."),
        // Render
        Module::new("HUD", Render, "The watermark, the active modules and your coordinates.")
            .on()
            .bool("Watermark", true)
            .bool("Module list", true)
            .bool("Coordinates", true),
        Module::new("Fullbright", Render, "Lights everything up."),
        Module::new("ESP", Render, "Draws boxes around entities, through walls.")
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", true)
            .bool("Items", false)
            .bool("Fill", false)
            .num("Range", 64.0, 8.0, 256.0, 8.0),
        Module::new("Tracers", Render, "Draws lines from your crosshair to entities.")
            .bool("Players", true)
            .bool("Hostiles", false)
            .bool("Passive", false)
            .num("Range", 64.0, 8.0, 256.0, 8.0),
        Module::new("Nametags", Render, "Big name tags with health and distance, through walls.")
            .bool("Players", true)
            .bool("Mobs", false)
            .num("Range", 64.0, 8.0, 256.0, 8.0),
        Module::new("Storage ESP", Render, "Draws boxes around chests and other storage.")
            .num("Range", 64.0, 8.0, 128.0, 8.0),
    ]
}

// ---------------------------------------------------------------- state

#[derive(Clone, Copy, Debug, PartialEq)]
enum Page {
    Main,
    Category(Category),
    Module(usize),
}

pub(crate) struct State {
    pub(crate) modules: Vec<Module>,
    page: Page,
    // A module waiting for its key bind (its index).
    binding: Option<usize>,
    friends: Vec<String>,
    // Keys this mod held down for the tick (Auto Walk, Sprint).
    injected: Vec<KeyCode>,
    loaded: bool,
    ticks: u64,
    aura_wait: u32,
    trigger_wait: u32,
    respawn_wait: u32,
    // Flight's Abilities mode is holding the player up.
    flying: bool,
    // The Step modifier is on the player.
    stepping: bool,
    // Auto Totem moved a totem from this hotbar slot and should swap back.
    totem_from: Option<usize>,
    // The player's velocity when the last tick ended: what changes it before
    // the next one is the server's push (Velocity).
    velocity: Option<[f64; 3]>,
}

impl State {
    fn new() -> Self {
        Self {
            modules: defaults(),
            page: Page::Main,
            binding: None,
            friends: Vec::new(),
            injected: Vec::new(),
            loaded: false,
            ticks: 0,
            aura_wait: 0,
            trigger_wait: 0,
            respawn_wait: 0,
            flying: false,
            stepping: false,
            totem_from: None,
            velocity: None,
        }
    }
    fn find(&self, name: &str) -> Option<usize> {
        let n = norm(name);
        self.modules.iter().position(|m| norm(m.name) == n)
    }
    fn save_text(&self) -> String {
        let mut out = String::new();
        for m in &self.modules {
            let id = norm(m.name);
            out.push_str(&format!("{id}.on={}\n", m.on));
            out.push_str(&format!("{id}.key={}\n", m.key.as_deref().unwrap_or("none")));
            for s in &m.settings {
                out.push_str(&format!("{id}.{}={}\n", norm(s.name), s.stored()));
            }
        }
        out.push_str(&format!("friends={}\n", self.friends.join(",")));
        out
    }
    fn load_text(&mut self, text: &str) {
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            if k == "friends" {
                self.friends = v
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(str::to_owned)
                    .collect();
                continue;
            }
            let Some((id, field)) = k.split_once('.') else {
                continue;
            };
            let Some(m) = self.modules.iter_mut().find(|m| norm(m.name) == id) else {
                continue;
            };
            match field {
                "on" => m.on = v == "true",
                "key" => m.key = (v != "none" && !v.is_empty()).then(|| v.to_owned()),
                _ => {
                    if let Some(s) = m.settings.iter_mut().find(|s| norm(s.name) == field) {
                        s.parse(v);
                    }
                }
            }
        }
    }
    fn module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }
    fn is_on(&self, name: &str) -> bool {
        self.module(name).is_some_and(|m| m.on)
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

// The packet hooks read these: they run inside the game's own sends, which
// may happen while a module holds the state.
static NO_FALL: AtomicBool = AtomicBool::new(false);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::new))
}

fn snapshot() -> Vec<Module> {
    with(|s| s.modules.clone())
}

fn sync_flags(s: &State) {
    NO_FALL.store(s.is_on("No Fall"), Ordering::Relaxed);
}

fn ensure_loaded(game: &Game<'_>) {
    if with(|s| s.loaded) {
        return;
    }
    let text = game.native_setting(ID, "config").unwrap_or_default();
    with(|s| {
        s.load_text(&text);
        s.loaded = true;
        sync_flags(s);
    });
}

fn save(game: &Game<'_>) {
    let text = with(|s| {
        sync_flags(s);
        s.save_text()
    });
    game.native_set_setting(ID, "config", &text);
}

// ---------------------------------------------------------------- helpers

fn on_off(b: bool) -> &'static str {
    if b { "ON" } else { "OFF" }
}

fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn round_to(v: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return v;
    }
    (v / step).round() * step
}

// Lowercase letters and digits only: how commands and the config name things.
fn norm(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

// A key typed in a command (`r`, `rshift`, `f6`, `key.keyboard.r`) as the
// game names it.
fn key_from_text(t: &str) -> Option<String> {
    let t = t.trim().to_ascii_lowercase();
    if t.starts_with("key.") {
        return Some(t);
    }
    let name = match t.as_str() {
        "rshift" | "rightshift" => "right.shift",
        "lshift" | "leftshift" | "shift" => "left.shift",
        "rctrl" | "rightctrl" | "rightcontrol" => "right.control",
        "lctrl" | "leftctrl" | "ctrl" | "control" => "left.control",
        "ralt" | "rightalt" => "right.alt",
        "lalt" | "leftalt" | "alt" => "left.alt",
        "space" => "space",
        "tab" => "tab",
        "enter" | "return" => "enter",
        "caps" | "capslock" => "caps.lock",
        "up" | "down" | "left" | "right" => t.as_str(),
        "insert" | "home" | "end" | "delete" => t.as_str(),
        "pageup" => "page.up",
        "pagedown" => "page.down",
        "grave" | "`" => "grave.accent",
        _ if t.chars().count() == 1 => t.as_str(),
        _ if t.starts_with('f') && t[1..].parse::<u32>().is_ok() => t.as_str(),
        _ if t.starts_with("kp") => return Some(format!("key.keyboard.keypad.{}", &t[2..])),
        _ => return None,
    };
    Some(format!("key.keyboard.{name}"))
}

fn key_label(key: Option<&str>) -> String {
    match key {
        None => "None".into(),
        Some(k) => {
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

// A line in chat: Delta's tag, then the message.
fn say(game: &mut Game<'_>, msg: &str) {
    let line = Text::literal("[Delta] ")
        .with_color(ACCENT)
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

fn own_id(game: &Game<'_>) -> Option<u32> {
    game.remote.as_ref().map(|r| r.entity_id as u32)
}

// The move keys held now: forward minus back, left minus right.
fn move_input(game: &Game<'_>) -> (f64, f64) {
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
fn move_direction(game: &Game<'_>) -> Option<[f64; 2]> {
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
enum Kind {
    Player,
    Hostile,
    Passive,
}

fn kind_of(mob: &horizon_client::mobs::Mob) -> Kind {
    if mob.kind.id == "minecraft:player" {
        Kind::Player
    } else if mob.hostile() || mob.kind.category == "monster" {
        Kind::Hostile
    } else {
        Kind::Passive
    }
}

fn wanted(m: &Module, kind: Kind) -> bool {
    match kind {
        Kind::Player => m.b("Players"),
        Kind::Hostile => m.b("Hostiles"),
        Kind::Passive => m.b("Passive"),
    }
}

fn is_friend(friends: &[String], names: &[(u32, String)], id: u32) -> bool {
    names
        .iter()
        .find(|(i, _)| *i == id)
        .is_some_and(|(_, n)| friends.iter().any(|f| f.eq_ignore_ascii_case(n)))
}

fn box_distance(eye: [f64; 3], b: &horizon_core::geometry::Aabb) -> f64 {
    let (lo, hi) = (b.min_position(), b.max_position());
    let c = [
        eye[0].clamp(lo.x, hi.x),
        eye[1].clamp(lo.y, hi.y),
        eye[2].clamp(lo.z, hi.z),
    ];
    let d = [eye[0] - c[0], eye[1] - c[1], eye[2] - c[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn look_at(eye: [f64; 3], at: [f64; 3]) -> (f32, f32) {
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
        keys: vec![[MENU_KEY.into(), "Open Delta menu".into(), MENU_DEFAULT.into()]],
    });
}

fn mod_index(game: &Game<'_>) -> Option<usize> {
    game.settings.mods.installed.iter().position(|m| m.id == ID)
}

fn refresh(game: &mut Game<'_>) {
    if let Some(i) = mod_index(game) {
        game.native_settings_refresh(i);
    }
}

pub(super) fn open_menu(game: &mut Game<'_>) {
    let Some(i) = mod_index(game) else {
        say(game, "Turn Delta Client on in Options > Mods first.");
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
        let m = &mut s.modules[i];
        m.on = !m.on;
        (m.name, m.on)
    });
    save(game);
    if announce {
        say(game, &format!("{name} {}", if on { "on" } else { "off" }));
    }
}

// ---------------------------------------------------------------- keys

pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_window() || !game.native_on(ID) {
        return;
    }
    ensure_loaded(game);
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
            s.modules[i].key = chosen;
            s.binding = None;
        });
        save(game);
        refresh(game);
        return;
    }
    if !game.native_playing() {
        return;
    }
    if key == game.native_mod_key(ID, MENU_KEY, MENU_DEFAULT) {
        open_menu(game);
        return;
    }
    let bound: Vec<usize> = with(|s| {
        (0..s.modules.len())
            .filter(|&i| s.modules[i].key.as_deref() == Some(key))
            .collect()
    });
    for i in bound {
        toggle(game, i, true);
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
    if !game.native_window() || !game.native_on(ID) || game.player.is_none() {
        return;
    }
    ensure_loaded(game);
    let mods = snapshot();
    let on = |n: &str| mods.iter().any(|m| m.name == n && m.on);
    let get = |n: &str| mods.iter().find(|m| m.name == n).cloned();
    let playing = game.native_playing();
    let mut want = Vec::new();
    if on("Auto Walk") && playing {
        want.push(KeyCode::KeyW);
    }
    if on("Sprint") && playing {
        want.push(KeyCode::ControlLeft);
    }
    let mut injected = Vec::new();
    for k in want {
        if game.keys.insert(k) {
            injected.push(k);
        }
    }
    // Velocity: the server's knockback (its SetEntityMotion and an
    // explosion's push) arrived since the last tick; keep only its share.
    if let Some(m) = get("Velocity").filter(|m| m.on)
        && let Some(last) = with(|s| s.velocity.take())
        && let Some(p) = game.player.as_mut()
    {
        let (h, v) = (m.n("Horizontal %") / 100.0, m.n("Vertical %") / 100.0);
        let now = [p.velocity.x, p.velocity.y, p.velocity.z];
        if now != last {
            p.velocity.x = last[0] + (now[0] - last[0]) * h;
            p.velocity.y = last[1] + (now[1] - last[1]) * v;
            p.velocity.z = last[2] + (now[2] - last[2]) * h;
        }
    }
    let flight = get("Flight").filter(|m| m.on && m.p("Mode") == "Abilities");
    let step = get("Step").filter(|m| m.on);
    let (was_flying, was_stepping) = with(|s| (s.flying, s.stepping));
    let (creative, spectator_mode) = (game.creative, game.spectator());
    if let Some(p) = game.player.as_mut() {
        if let Some(f) = &flight {
            p.may_fly = true;
            p.flying = true;
            // Vanilla's 0.05 flies about 11 blocks a second.
            p.flying_speed = (f.n("Speed") * 0.05 / 10.9) as f32;
            p.fall_distance = 0.0;
        } else if was_flying {
            p.flying = creative && p.flying || spectator_mode;
            p.may_fly = creative || spectator_mode;
            p.flying_speed = 0.05;
        }
        if let Some(st) = &step {
            let _ = p.set_modifier("minecraft:step_height", "delta:step", st.n("Height") - 0.6, "add_value", true);
        } else if was_stepping {
            let _ = p.set_modifier("minecraft:step_height", "delta:step", 0.0, "add_value", false);
        }
    }
    with(|s| {
        s.injected.extend(injected);
        s.flying = flight.is_some();
        s.stepping = step.is_some();
    });
}

// After the game ticks: let go of held keys, then each module's work.
pub(super) fn post_tick(game: &mut Game<'_>) {
    if !game.native_window() || !game.native_on(ID) {
        return;
    }
    ensure_loaded(game);
    let injected = with(|s| std::mem::take(&mut s.injected));
    for k in injected {
        if !game.physical_keys.contains(&k) {
            game.keys.remove(&k);
        }
    }
    let mods = snapshot();
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
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

    if get("Fast Place").is_some() {
        game.use_delay = 0;
    }
    if get("Fast Break").is_some() {
        game.destroy_delay = 0;
    }

    let dir = move_direction(game);
    let jump = game.native_playing() && game.keys.contains(&KeyCode::Space);
    let sneak = game.native_playing() && game.keys.contains(&KeyCode::ShiftLeft);
    let forward = move_input(game).0 > 0.0;
    let dim = game.dim.id();
    let (yaw, pitch) = (game.yaw, game.pitch);

    if let Some(p) = game.player.as_mut() {
        if let Some(f) = get("Flight")
            && f.p("Mode") == "Velocity"
        {
            let speed = f.n("Speed") / 20.0;
            let [x, z] = dir.unwrap_or([0.0, 0.0]);
            p.velocity.x = x * speed;
            p.velocity.z = z * speed;
            p.velocity.y = if jump {
                speed
            } else if sneak {
                -speed
            } else {
                0.0
            };
            // Vanilla kicks a player floating for 80 ticks: drop a little
            // every 40.
            if f.b("Anti kick") && ticks % 40 == 0 && p.velocity.y >= 0.0 {
                p.velocity.y = -0.04;
            }
            p.fall_distance = 0.0;
        }
        if let Some(sp) = get("Speed")
            && let Some([x, z]) = dir
            && (!sp.b("Only on ground") || p.ground.on_ground)
            && !p.flying
            && !p.fall_flying
            && get("Flight").is_none()
        {
            let speed = sp.n("Speed") / 20.0;
            p.velocity.x = x * speed;
            p.velocity.z = z * speed;
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
        if get("Anti Void").is_some() {
            let floor = if dim == "minecraft:overworld" { -64.0 } else { 0.0 };
            if p.position.y < floor && p.velocity.y < 0.0 {
                p.velocity.y = 0.6;
                p.fall_distance = 0.0;
            }
        }
    }

    remember_velocity(game);
    if let Some(m) = get("Auto Totem") {
        auto_totem(game, m.b("Swap back"));
    }
    if let Some(m) = get("Kill Aura") {
        kill_aura(game, m, get("Criticals").is_some());
    }
    if let Some(m) = get("Trigger Bot") {
        trigger_bot(game, m, get("Criticals").is_some());
    }
}

fn remember_velocity(game: &Game<'_>) {
    let v = game.player.as_ref().map(|p| [p.velocity.x, p.velocity.y, p.velocity.z]);
    with(|s| s.velocity = v);
}

fn cooldown_ready(game: &Game<'_>, m: &Module, wait: &mut u32) -> bool {
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

fn attack(game: &mut Game<'_>, id: u32, crits: bool, rotate: Option<(f32, f32)>) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (x, y, z) = (p.position.x, p.position.y, p.position.z);
    let ground = p.ground.on_ground;
    let in_water = p.swimming || p.fluids.was_touching_water;
    if let Some((yaw, pitch)) = rotate {
        game.send(S::MovePlayerRot(play::MovePlayerRot {
            yaw,
            pitch,
            flags: u8::from(ground),
        }));
    }
    // Packet Criticals (as Meteor's): a tiny hop the server sees, so the hit
    // lands while falling.
    if crits && ground && !in_water {
        game.send(S::MovePlayerPos(play::MovePlayerPos { x, y: y + 0.0625, z, flags: 0 }));
        game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags: 0 }));
    }
    game.send(S::Attack(play::Attack { id: id as i32 }));
    game.swing_arm();
    game.send(S::Punch(Empty));
    game.reset_attack_strength();
}

fn kill_aura(game: &mut Game<'_>, m: &Module, crits: bool) {
    if !game.native_playing() || game.remote.is_none() {
        return;
    }
    if m.b("Pause while using") && game.using.is_some() {
        return;
    }
    let mut wait = with(|s| s.aura_wait);
    let ready = cooldown_ready(game, m, &mut wait);
    with(|s| s.aura_wait = wait);
    if !ready {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let own = own_id(game);
    let names = game.remote_player_names();
    let friends = with(|s| s.friends.clone());
    let range = m.n("Range");
    let mut best: Option<(u32, [f64; 3], f64)> = None;
    for mob in &game.mobs {
        if !mob.alive() || mob.removed || Some(mob.id) == own || Some(mob.id) == game.riding {
            continue;
        }
        let kind = kind_of(mob);
        if !wanted(m, kind) || (kind == Kind::Player && is_friend(&friends, &names, mob.id)) {
            continue;
        }
        let b = mob.bounds();
        let d = box_distance(eye, &b);
        if d <= range && best.is_none_or(|(_, _, bd)| d < bd) {
            let (lo, hi) = (b.min_position(), b.max_position());
            let centre = [(lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0];
            best = Some((mob.id, centre, d));
        }
    }
    let Some((id, centre, _)) = best else {
        return;
    };
    let rotate = m.b("Rotate").then(|| look_at(eye, centre));
    attack(game, id, crits, rotate);
    with(|s| s.aura_wait = m.n("Delay (ticks)") as u32);
}

fn trigger_bot(game: &mut Game<'_>, m: &Module, crits: bool) {
    if !game.native_playing() || game.remote.is_none() || game.using.is_some() {
        return;
    }
    let Some((i, _)) = game.mob_hit() else {
        return;
    };
    let mob = &game.mobs[i];
    let (id, kind) = (mob.id, kind_of(mob));
    let names = game.remote_player_names();
    let friends = with(|s| s.friends.clone());
    if !wanted(m, kind) || (kind == Kind::Player && is_friend(&friends, &names, id)) {
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
fn auto_totem(game: &mut Game<'_>, swap_back: bool) {
    if game.remote.is_none() || !game.native_playing() {
        return;
    }
    let is_totem = |s: &Option<horizon_client::items::ItemStack>| {
        s.as_ref().is_some_and(|s| &*s.item == "minecraft:totem_of_undying")
    };
    if is_totem(&game.offhand) {
        if let Some(back) = with(|s| s.totem_from.take())
            && swap_back
            && back < 9
        {
            game.selected = back;
        }
        return;
    }
    let Some(slot) = (0..9).find(|&i| game.inventory.get(i).is_some_and(is_totem)) else {
        return;
    };
    let before = game.selected;
    game.selected = slot;
    // The server swaps the slot it last heard about: tell it first.
    game.send(S::SetCarriedItem(play::SetCarriedItem { slot: slot as i16 }));
    game.remote_swap_hands();
    with(|s| {
        if s.totem_from.is_none() && before != slot {
            s.totem_from = Some(before);
        }
    });
}

// ---------------------------------------------------------------- packets

// An outgoing packet to send instead (No Fall), or none to send it as it is.
pub(super) fn outgoing(game: &Game<'_>, packet: &S) -> Option<S> {
    if !NO_FALL.load(Ordering::Relaxed) {
        return None;
    }
    let p = game.player.as_ref()?;
    if p.fall_distance <= 2.0 || p.flying || p.fall_flying {
        return None;
    }
    // On the ground, as the server reads it: its fall distance starts over.
    Some(match packet {
        S::MovePlayerPos(m) => S::MovePlayerPos(play::MovePlayerPos {
            x: m.x,
            y: m.y,
            z: m.z,
            flags: m.flags | 1,
        }),
        S::MovePlayerPosRot(m) => S::MovePlayerPosRot(play::MovePlayerPosRot {
            x: m.x,
            y: m.y,
            z: m.z,
            yaw: m.yaw,
            pitch: m.pitch,
            flags: m.flags | 1,
        }),
        S::MovePlayerRot(m) => S::MovePlayerRot(play::MovePlayerRot {
            yaw: m.yaw,
            pitch: m.pitch,
            flags: m.flags | 1,
        }),
        S::MovePlayerStatus(m) => S::MovePlayerStatus(play::MovePlayerStatus { flags: m.flags | 1 }),
        _ => return None,
    })
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
    (".menu", "open the menu (or press Right Shift)"),
    (".toggle <module>", "turn a module on or off (.t)"),
    (".modules", "list every module"),
    (".settings <module>", "list a module's settings"),
    (".set <module> <setting> <value>", "change a setting"),
    (".bind <module> <key|none>", "bind a key to a module"),
    (".binds", "list key binds"),
    (".reset <module>", "put a module back to its defaults"),
    (".friend add|remove|list [name]", "Kill Aura and Trigger Bot skip friends"),
    (".vclip <blocks>", "move up or down"),
    (".hclip <blocks>", "move forward"),
];

fn run(game: &mut Game<'_>, args: &[&str]) {
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
        "menu" | "gui" => open_menu(game),
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
            let value = args[args.len() - 1];
            let name = norm(&args[2..args.len() - 1].join(""));
            let result = with(|s| {
                let m = &mut s.modules[i];
                let setting = m.settings.iter_mut().find(|st| norm(st.name) == name)?;
                Some((setting.parse(value), setting.shown()))
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
        "bind" => {
            let Some(i) = module_arg(game, args.get(1)) else {
                return;
            };
            let Some(k) = args.get(2) else {
                say(game, "Use .bind <module> <key>, or none");
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
                s.modules[i].key = key.clone();
                s.modules[i].name
            });
            save(game);
            say(game, &format!("{name} bound to {}", key_label(key.as_deref())));
        }
        "binds" => {
            let lines: Vec<String> = with(|s| {
                s.modules
                    .iter()
                    .filter_map(|m| m.key.as_deref().map(|k| format!("{}: {}", m.name, key_label(Some(k)))))
                    .collect()
            });
            if lines.is_empty() {
                say(game, "No key binds yet.");
            }
            for l in lines {
                say(game, &l);
            }
        }
        "reset" => {
            if let Some(i) = module_arg(game, args.get(1)) {
                let name = with(|s| {
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

struct Camera {
    pos: [f64; 3],
    fwd: [f64; 3],
    up: [f64; 3],
    right: [f64; 3],
    focal: f64,
    aspect: f64,
    w: f64,
    h: f64,
}

impl Camera {
    fn of(game: &Game<'_>, w: f32, h: f32) -> Option<Self> {
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
        Some(Self {
            pos,
            fwd: [-sy * cp, -sp, cy * cp],
            up: [-sy * sp, cp, cy * sp],
            right: [-cy, 0.0, -sy],
            focal: 1.0 / f64::from((fov / 2.0).to_radians()).tan(),
            aspect: f64::from(w) / f64::from(h),
            w: f64::from(w),
            h: f64::from(h),
        })
    }
    fn project(&self, at: [f64; 3]) -> Option<[f32; 2]> {
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
    // The screen rectangle around a box, when all of it is in front.
    fn rect(&self, lo: [f64; 3], hi: [f64; 3]) -> Option<[f32; 4]> {
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for i in 0..8 {
            let c = [
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ];
            let [x, y] = self.project(c)?;
            r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
        }
        let margin = 4.0 * self.w as f32;
        (r[2] > -margin && r[0] < margin && r[3] > -margin && r[1] < margin).then_some(r)
    }
}

enum Shape {
    Fill([f32; 4], u32),
    Text(String, [f32; 2], u32),
}

fn outline(out: &mut Vec<Shape>, r: [f32; 4], argb: u32, lw: f32, fill: bool) {
    let [x0, y0, x1, y1] = r;
    if fill {
        out.push(Shape::Fill([x0, y0, x1 - x0, y1 - y0], (argb & 0x00FF_FFFF) | 0x2800_0000));
    }
    out.push(Shape::Fill([x0, y0, x1 - x0, lw], argb));
    out.push(Shape::Fill([x0, y1 - lw, x1 - x0, lw], argb));
    out.push(Shape::Fill([x0, y0, lw, y1 - y0], argb));
    out.push(Shape::Fill([x1 - lw, y0, lw, y1 - y0], argb));
}

fn colour(kind: Kind) -> u32 {
    match kind {
        Kind::Player => 0xFFFF_FFFF,
        Kind::Hostile => 0xFFFF_5050,
        Kind::Passive => 0xFF50_FF50,
    }
}

fn health_colour(fraction: f32) -> u32 {
    if fraction > 0.66 {
        0x55FF55
    } else if fraction > 0.33 {
        0xFFFF55
    } else {
        0xFF5555
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
    let mut shapes = Vec::new();

    if let Some(cam) = Camera::of(game, w, h)
        && game.player.is_some()
    {
        world_overlays(game, &cam, &mods, lw, &mut shapes);
    }

    if let Some(hud) = get("HUD") {
        if hud.b("Watermark") && game.native_corner_shown(Corner::TopLeft) {
            let text = format!("{NAME} {VERSION}");
            let tw = game.native_text_width(&text);
            let (x, y) = corners.place(Corner::TopLeft, tw + 4.0, 12.0);
            shapes.push(Shape::Text(text, [x + 2.0, y + 2.0], ACCENT));
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
        if hud.b("Coordinates")
            && game.native_corner_shown(Corner::TopLeft)
            && let Some(p) = game.native_player()
        {
            let (x, y, z) = (p.x.floor(), p.y.floor(), p.z.floor());
            let other = match p.dimension {
                "minecraft:overworld" => format!(" (Nether {} {})", (p.x / 8.0).floor(), (p.z / 8.0).floor()),
                "minecraft:the_nether" => format!(" (Overworld {} {})", (p.x * 8.0).floor(), (p.z * 8.0).floor()),
                _ => String::new(),
            };
            let text = format!("XYZ {x} {y} {z}{other}");
            let tw = game.native_text_width(&text);
            let (bx, by) = corners.place(Corner::TopLeft, tw + 4.0, 12.0);
            shapes.push(Shape::Text(text, [bx + 2.0, by + 2.0], 0xFFFFFF));
        }
    }

    let game = &*game;
    for s in shapes {
        match s {
            Shape::Fill(r, argb) => game.native_fill(batch, r, argb),
            Shape::Text(t, at, rgb) => game.native_text(batch, &t, at, rgb, true),
        }
    }
}

fn world_overlays(game: &Game<'_>, cam: &Camera, mods: &[Module], lw: f32, out: &mut Vec<Shape>) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let (esp, tracers, tags, storage) = (get("ESP"), get("Tracers"), get("Nametags"), get("Storage ESP"));
    if esp.is_none() && tracers.is_none() && tags.is_none() && storage.is_none() {
        return;
    }
    let alpha = game.clock.camera_fraction();
    let lerp = |a: f64, b: f64| a + alpha * (b - a);
    let own = own_id(game);
    let names = game.remote_player_names();
    let friends = with(|s| s.friends.clone());
    let centre = [cam.w as f32 / 2.0, cam.h as f32 / 2.0];
    let dist = |at: [f64; 3]| {
        let d = [at[0] - cam.pos[0], at[1] - cam.pos[1], at[2] - cam.pos[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };

    for mob in &game.mobs {
        if !mob.alive() || mob.removed || Some(mob.id) == own {
            continue;
        }
        let kind = kind_of(mob);
        let b = mob.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        // The box where the frame draws the mob.
        let off = [
            lerp(mob.previous.x, mob.position.x) - mob.position.x,
            lerp(mob.previous.y, mob.position.y) - mob.position.y,
            lerp(mob.previous.z, mob.position.z) - mob.position.z,
        ];
        let lo = [lo.x + off[0], lo.y + off[1], lo.z + off[2]];
        let hi = [hi.x + off[0], hi.y + off[1], hi.z + off[2]];
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let d = dist(mid);
        let friend = kind == Kind::Player && is_friend(&friends, &names, mob.id);
        let col = if friend { 0xFF55_FFFF } else { colour(kind) };
        if let Some(m) = esp
            && wanted(m, kind)
            && d <= m.n("Range")
            && let Some(r) = cam.rect(lo, hi)
        {
            outline(out, r, col, lw, m.b("Fill"));
        }
        if let Some(m) = tracers
            && wanted(m, kind)
            && d <= m.n("Range")
            && let Some(to) = cam.project(mid)
        {
            line(out, centre, to, col, lw);
        }
        if let Some(m) = tags
            && d <= m.n("Range")
            && (if kind == Kind::Player { m.b("Players") } else { m.b("Mobs") })
            && let Some([x, y]) = cam.project([mid[0], hi[1] + 0.6, mid[2]])
        {
            let name = names
                .iter()
                .find(|(i, _)| *i == mob.id)
                .map(|(_, n)| n.clone())
                .or_else(|| mob.custom_name.clone())
                .unwrap_or_else(|| pretty_id(&mob.kind.id));
            let max = mob.kind.max_health.max(1.0);
            let hp = format!("{}", (mob.health * 10.0).round() / 10.0);
            let tail = format!(" {}m", d.round());
            let (nw, hw, tw) = (game.native_text_width(&name), game.native_text_width(&hp), game.native_text_width(&tail));
            let total = nw + 4.0 + hw + tw;
            let x0 = x - total / 2.0;
            out.push(Shape::Fill([x0 - 2.0, y - 2.0, total + 4.0, 12.0], 0x7800_0000));
            out.push(Shape::Text(name, [x0, y], if friend { 0x55FFFF } else { 0xFFFFFF }));
            out.push(Shape::Text(hp, [x0 + nw + 4.0, y], health_colour(mob.health / max)));
            out.push(Shape::Text(tail, [x0 + nw + 4.0 + hw, y], 0xAAAAAA));
        }
    }

    if let Some(m) = esp
        && m.b("Items")
    {
        for item in &game.dropped {
            let at = [
                lerp(item.previous.x, item.position.x),
                lerp(item.previous.y, item.position.y),
                lerp(item.previous.z, item.position.z),
            ];
            if dist(at) > m.n("Range") {
                continue;
            }
            let lo = [at[0] - 0.125, at[1], at[2] - 0.125];
            let hi = [at[0] + 0.125, at[1] + 0.25, at[2] + 0.125];
            if let Some(r) = cam.rect(lo, hi) {
                outline(out, r, 0xFFFF_AA00, lw, m.b("Fill"));
            }
        }
    }

    if let Some(m) = storage {
        let range = m.n("Range");
        let facts = game.res.facts();
        for (&(x, y, z), _) in game.world.block_entities.iter() {
            let at = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
            if dist(at) > range {
                continue;
            }
            let name = &facts.get(game.world.block(x, y, z)).name;
            let Some(col) = storage_colour(name) else {
                continue;
            };
            let inset = if name.ends_with("chest") { 0.0625 } else { 0.0 };
            let lo = [f64::from(x) + inset, f64::from(y), f64::from(z) + inset];
            let hi = [f64::from(x + 1) - inset, f64::from(y + 1) - inset * 2.0, f64::from(z + 1) - inset];
            if let Some(r) = cam.rect(lo, hi) {
                outline(out, r, col, lw, true);
            }
        }
    }
}

fn storage_colour(name: &str) -> Option<u32> {
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

fn pretty_id(id: &str) -> String {
    let n = id.strip_prefix("minecraft:").unwrap_or(id);
    key_label(Some(&n.replace('_', ".")))
}

// A line as small squares, which is all the HUD draws.
fn line(out: &mut Vec<Shape>, from: [f32; 2], to: [f32; 2], argb: u32, lw: f32) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len < 1.0 {
        return;
    }
    let step = lw.max(0.75);
    let n = ((len / step) as usize).min(4000);
    for i in 0..=n {
        let t = i as f32 / n.max(1) as f32;
        out.push(Shape::Fill([from[0] + dx * t - lw / 2.0, from[1] + dy * t - lw / 2.0, lw, lw], argb));
    }
}
