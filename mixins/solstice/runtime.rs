// Solstice: Horizon's first cheat client, a Meteor Client style utility mod, rebuilt in Rust as a
// Horizon native mod. Modules in categories, each with settings and a key
// bind, a menu on vanilla's own Options > Mods page, `.` chat commands, and a
// HUD with the active modules. The bundle adds this file to the game as
// `crate::game::solstice`, so it reads and changes the game's own state.
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

pub(crate) const ID: &str = "solstice";
pub(crate) const NAME: &str = "Solstice";
pub(crate) const VERSION: &str = "0.5.0";
pub(crate) const DESCRIPTION: &str =
    "Utility client: combat, movement and render modules, a menu, key binds and . commands.";
// The menu key's name on the mod's page and its default (Right Shift).
pub(crate) const MENU_KEY: &str = "menu";
pub(crate) const MENU_DEFAULT: &str = "key.keyboard.right.shift";
// Solstice's sun gold.
const ACCENT: u32 = 0xFFB020;

// ---------------------------------------------------------------- modules

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
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
        Module::new("HUD", Render, "The watermark, the active modules, your coordinates and more.")
            .on()
            .bool("Watermark", true)
            .bool("Module list", true)
            .bool("Coordinates", true)
            .bool("FPS", true)
            .bool("Speed", false)
            .bool("Facing", true)
            .bool("Totems", false),
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
        Module::new("Parkour", Movement, "Jumps for you at the edge of a block."),
        Module::new("Blink", Movement, "Holds back your movement packets, then sends them all at once when off.")
            .num("Max ticks", 0.0, 0.0, 200.0, 10.0),
        Module::new("Scaffold", Movement, "Places blocks under your feet as you walk.")
            .bool("Auto switch", true)
            .num("Delay (ticks)", 0.0, 0.0, 10.0, 1.0),
        Module::new("Anti Hunger", Player, "Spends less food by hiding that you are sprinting and on the ground.")
            .bool("Sprint", true)
            .bool("Ground", true),
        Module::new("Anti AFK", Player, "Moves a little every few seconds so you aren't kicked for idling.")
            .num("Seconds", 10.0, 3.0, 60.0, 1.0),
        Module::new("Auto Clicker", Player, "Clicks for you while you hold a mouse button.")
            .bool("Left", true)
            .bool("Right", false)
            .num("Delay (ticks)", 4.0, 1.0, 20.0, 1.0),
        Module::new("Auto Eat", Player, "Eats food from your hotbar when you get hungry.")
            .num("Hunger", 14.0, 1.0, 19.0, 1.0),
        Module::new("Zoom", Render, "Zooms in.")
            .num("Factor", 4.0, 1.5, 10.0, 0.5)
            .bool("Smooth", true),
        Module::new("Breadcrumbs", Render, "Draws a line along where you have been.")
            .num("Length", 300.0, 20.0, 1000.0, 20.0),
        Module::new("Block ESP", Render, "Draws boxes around ores and other blocks, through walls.")
            .pick("Blocks", &["Ores", "Diamond", "Ancient Debris", "Spawners", "Portals"])
            .num("Range", 32.0, 8.0, 48.0, 4.0)
            .bool("Fill", true),
        Module::new("Waypoints", Render, "Shows the places saved with .waypoint, on screen.")
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
        Module::new("Auto Tool", Player, "Picks the fastest tool in your hotbar for the block you mine.")
            .bool("Switch back", true),
        Module::new("Middle Click Extra", Player, "Middle click uses an item from your hotbar, then switches back.")
            .pick("Item", &["Pearl", "Rocket", "XP bottle"]),
        Module::new("Trajectories", Render, "Draws where a bow shot or a throw will land."),
        Module::new("Logout Spots", Render, "Marks where players were when they logged out.")
            .num("Minutes", 10.0, 1.0, 60.0, 1.0),
        Module::new("Bow Spam", Combat, "Fires arrows as fast as you can draw them while you hold use.")
            .num("Charge (ticks)", 5.0, 3.0, 20.0, 1.0),
        Module::new("Notifier", Player, "Says in chat when a player joins or leaves the server.")
            .bool("Join", true)
            .bool("Leave", true),
        Module::new("Weather Changer", World, "Sets the weather you see.")
            .pick("Weather", &["Clear", "Rain", "Thunder"]),
        Module::new("Hole ESP", Render, "Marks holes you can stand in: green all obsidian or bedrock, yellow other solid blocks.")
            .num("Range", 8.0, 3.0, 16.0, 1.0)
            .bool("Fill", true),
        Module::new("Nuker", World, "Breaks the blocks around you.")
            .num("Range", 4.0, 1.0, 6.0, 0.5)
            .num("Blocks per tick", 2.0, 1.0, 10.0, 1.0)
            .bool("Instant only", true)
            .bool("Keep ground", true),
        Module::new("Time Changer", World, "Sets the time of day you see.")
            .num("Time", 6000.0, 0.0, 24000.0, 1000.0),
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
    gui: Gui,
    crumbs: Vec<[f64; 3]>,
    waypoints: Vec<Waypoint>,
    // Block ESP's finds: where, and which colour; and when and for what it was made.
    found: Vec<([i32; 3], u32)>,
    found_for: (usize, i32, [i32; 3]),
    found_at: u64,
    afk_wait: u32,
    scaffold_wait: u32,
    click_wait: u32,
    blink_ticks: u32,
    eating: bool,
    was_ground: bool,
    eat_ticks: u32,
    crumb_at: u64,
    crit_target: Option<(u32, Option<(f32, f32)>)>,
    crit_ticks: u32,
    notify: Option<Vec<String>>,
    holes: Vec<([i32; 3], u32)>,
    holes_at: u64,
    tool_back: Option<usize>,
    mc_back: Option<(usize, u32)>,
    nuke: Option<([i32; 3], u32)>,
    armor_wait: u32,
    replenish_wait: u32,
    last_pos: Vec<(String, [f64; 3], String)>,
    spots: Vec<Spot>,
}

#[derive(Clone, Debug, PartialEq)]
struct Spot {
    name: String,
    pos: [f64; 3],
    dim: String,
    at: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct Waypoint {
    name: String,
    pos: [f64; 3],
    dim: String,
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
            gui: Gui::default(),
            crumbs: Vec::new(),
            waypoints: Vec::new(),
            found: Vec::new(),
            found_for: (usize::MAX, 0, [0; 3]),
            found_at: 0,
            afk_wait: 0,
            scaffold_wait: 0,
            click_wait: 0,
            blink_ticks: 0,
            eating: false,
            was_ground: false,
            eat_ticks: 0,
            crumb_at: 0,
            crit_target: None,
            crit_ticks: 0,
            notify: None,
            holes: Vec::new(),
            holes_at: 0,
            tool_back: None,
            mc_back: None,
            nuke: None,
            armor_wait: 0,
            replenish_wait: 0,
            last_pos: Vec::new(),
            spots: Vec::new(),
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
static BLINK: AtomicBool = AtomicBool::new(false);
static ANTI_SPRINT: AtomicBool = AtomicBool::new(false);
static ANTI_GROUND: AtomicBool = AtomicBool::new(false);
// The server takes one position packet per tick and disconnects a client that
// sends more ("Invalid move player packet received"). Criticals hops over two
// ticks instead: stage 1 and 2 turn the next position packets into the hop up
// and the hop back (both off the ground), stage 3 lets the hit follow.
static CRIT_STAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static POS_THIS_TICK: AtomicBool = AtomicBool::new(false);
// Blink held at least one move packet.
static BLINK_HELD: AtomicBool = AtomicBool::new(false);
// Scaffold's block placements count up from here (the game's own are small).
static SEQUENCE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1_000_000_000);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::new))
}

fn snapshot() -> Vec<Module> {
    with(|s| s.modules.clone())
}

fn sync_flags(s: &State) {
    NO_FALL.store(s.is_on("No Fall"), Ordering::Relaxed);
    BLINK.store(s.is_on("Blink"), Ordering::Relaxed);
    let hunger = s.module("Anti Hunger").filter(|m| m.on);
    ANTI_SPRINT.store(hunger.is_some_and(|m| m.b("Sprint")), Ordering::Relaxed);
    ANTI_GROUND.store(hunger.is_some_and(|m| m.b("Ground")), Ordering::Relaxed);
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

// A line in chat: Solstice's tag, then the message.
fn say(game: &mut Game<'_>, msg: &str) {
    let line = Text::literal("[Solstice] ")
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
        keys: vec![[MENU_KEY.into(), "Open Solstice menu".into(), MENU_DEFAULT.into()]],
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
        say(game, "Turn Solstice on in Options > Mods first.");
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
        open_gui(game);
        return;
    }
    if key == "key.mouse.middle" && with(|s| s.is_on("Middle Click Extra")) {
        middle_click(game);
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
    POS_THIS_TICK.store(false, Ordering::Relaxed);
    let mods = snapshot();
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
    if on("Safe Walk") && playing && at_edge(game) {
        want.push(KeyCode::ShiftLeft);
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
            let _ = p.set_modifier("minecraft:step_height", "solstice:step", st.n("Height") - 0.6, "add_value", true);
        } else if was_stepping {
            let _ = p.set_modifier("minecraft:step_height", "solstice:step", 0.0, "add_value", false);
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
        if let Some(m) = get("High Jump")
            && was_ground
            && !ground_now
            && jump_now
            && p.velocity.y > 0.3
        {
            // The jump's own speed (0.42), scaled, after this tick's gravity.
            p.velocity.y = (0.42 * m.n("Multiplier") - 0.08) * 0.98;
        }
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
    if get("Parkour").is_some()
        && ground_now
        && !sneak
        && let Some((x, y, z)) = ahead(game)
        && !solid_at(game, x, y, z)
        && let Some(p) = game.player.as_mut()
    {
        p.velocity.y = 0.42;
    }
    if alive && let Some(m) = get("Auto Log") && game.remote.is_some()
        && game.player.as_ref().is_some_and(|p| f64::from(p.health.health()) <= m.n("Health"))
    {
        let health = game.player.as_ref().map_or(0.0, |p| p.health.health());
        if let Some(i) = with(|s| s.find("Auto Log")) {
            toggle(game, i, false);
        }
        let _ = game.leave_server(Some(format!("Solstice Auto Log: health was {health}")));
        return;
    }
    if let Some(m) = get("Scaffold") {
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
        with(|s| {
            s.holes.clear();
            s.holes_at = 0;
        });
    }
    if let Some(m) = get("Nuker") {
        nuker(game, m);
    } else {
        with(|s| s.nuke = None);
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
    }
    if let Some(m) = get("Anti AFK") {
        anti_afk(game, m);
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
        with(|s| {
            s.found.clear();
            s.found_at = 0;
        });
    }
    if let Some(m) = get("Auto Totem") {
        auto_totem(game, m.b("Swap back"));
    }
    crit_tick(game);
    if let Some(m) = get("Kill Aura") {
        kill_aura(game, m, get("Criticals").is_some());
    }
    if let Some(m) = get("Trigger Bot") {
        trigger_bot(game, m, get("Criticals").is_some());
    }
}

fn solid_at(game: &Game<'_>, x: i32, y: i32, z: i32) -> bool {
    !game.res.facts().is_air(game.world.block(x, y, z))
}

// The block column just ahead of the player's feet, in the direction the
// move keys point: its x and z, and the y of the block under the feet.
fn ahead(game: &Game<'_>) -> Option<(i32, i32, i32)> {
    let p = game.player.as_ref()?;
    let d = move_direction(game)?;
    let (ax, az) = (p.position.x + d[0] * 0.35, p.position.z + d[1] * 0.35);
    Some((ax.floor() as i32, (p.position.y - 0.5).floor() as i32, az.floor() as i32))
}

// Standing on the ground and walking toward a drop of two blocks or more.
fn at_edge(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    if !p.ground.on_ground || p.flying || p.fall_flying {
        return false;
    }
    match ahead(game) {
        Some((x, y, z)) => !solid_at(game, x, y, z) && !solid_at(game, x, y - 1, z),
        None => false,
    }
}

// Switches the held slot now: the server is told before anything is sent
// that depends on it.
fn hold(game: &mut Game<'_>, slot: usize) {
    if game.selected != slot {
        game.selected = slot;
        game.send(S::SetCarriedItem(play::SetCarriedItem { slot: slot as i16 }));
    }
}

fn item_info(game: &Game<'_>, slot: usize) -> Option<horizon_client::items::ItemInfo> {
    let stack = game.inventory.get(slot)?.as_ref()?;
    game.res.items().get(&stack.item).cloned()
}

fn best_weapon(game: &Game<'_>, prefer_axes: bool) -> Option<usize> {
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

fn scaffold(game: &mut Game<'_>, m: &Module) {
    if !game.native_playing() || game.remote.is_none() {
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
    let t = [p.position.x.floor() as i32, (p.position.y - 0.5).floor() as i32, p.position.z.floor() as i32];
    if solid_at(game, t[0], t[1], t[2]) {
        return;
    }
    // A block to put it against, and the face of that block that points at it.
    let sides: [([i32; 3], i32); 5] = [
        ([0, -1, 0], 1),
        ([-1, 0, 0], 5),
        ([1, 0, 0], 4),
        ([0, 0, -1], 3),
        ([0, 0, 1], 2),
    ];
    let Some((against, face)) = sides
        .iter()
        .map(|(d, f)| ([t[0] + d[0], t[1] + d[1], t[2] + d[2]], *f))
        .find(|(n, _)| solid_at(game, n[0], n[1], n[2]))
    else {
        return;
    };
    let is_block = |game: &Game<'_>, slot: usize| item_info(game, slot).is_some_and(|i| i.block.is_some());
    if !is_block(game, game.selected) {
        if !m.b("Auto switch") {
            return;
        }
        let Some(slot) = (0..9).find(|&i| is_block(game, i)) else {
            return;
        };
        hold(game, slot);
        // The server takes the slot first; the block goes next tick.
        return;
    }
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    game.send(S::UseItemOn(play::UseItemOn {
        hand: 0,
        pos: against,
        face,
        cursor: [0.5, 0.5, 0.5],
        inside: false,
        world_border: false,
        sequence,
    }));
    game.swing_arm();
    game.send(S::Punch(Empty));
    with(|s| s.scaffold_wait = m.n("Delay (ticks)") as u32 + 1);
}

fn auto_click(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    if !game.native_playing() {
        return;
    }
    let (left, right) = (
        m.b("Left") && game.physical_buttons.contains(&MouseButton::Left),
        m.b("Right") && game.physical_buttons.contains(&MouseButton::Right),
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

fn auto_eat(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    if !game.native_playing() {
        return;
    }
    let Some(food) = game.player.as_ref().map(|p| p.health.food.food) else {
        return;
    };
    if with(|s| s.eating) {
        let ticks = with(|s| {
            s.eat_ticks += 1;
            s.eat_ticks
        });
        // Done: the item is used up, or it never started.
        if game.using.is_none() && ticks > 3 || ticks > 100 {
            let _ = game.play_mouse(MouseButton::Right, false);
            with(|s| {
                s.eating = false;
                s.eat_ticks = 0;
            });
        }
        return;
    }
    if f64::from(food) > m.n("Hunger") || game.using.is_some() {
        return;
    }
    const BAD: [&str; 6] = ["rotten_flesh", "spider_eye", "pufferfish", "poisonous_potato", "chorus_fruit", "suspicious_stew"];
    let mut best: Option<(usize, u32)> = None;
    for slot in 0..9 {
        let Some(stack) = game.inventory.get(slot).and_then(|s| s.as_ref()) else {
            continue;
        };
        if BAD.iter().any(|b| stack.item.ends_with(b)) {
            continue;
        }
        if let Some((nutrition, _)) = game.res.items().get(&stack.item).and_then(|i| i.food)
            && best.is_none_or(|(_, n)| nutrition > n)
        {
            best = Some((slot, nutrition));
        }
    }
    let Some((slot, _)) = best else {
        return;
    };
    hold(game, slot);
    let _ = game.play_mouse(MouseButton::Right, true);
    with(|s| {
        s.eating = true;
        s.eat_ticks = 0;
    });
}

fn anti_afk(game: &mut Game<'_>, m: &Module) {
    if !game.native_playing() || game.remote.is_none() {
        return;
    }
    let ticks = with(|s| {
        s.afk_wait += 1;
        s.afk_wait
    });
    if f64::from(ticks) < m.n("Seconds") * 20.0 {
        return;
    }
    with(|s| s.afk_wait = 0);
    game.yaw += if game.ticks % 40 < 20 { 4.0 } else { -4.0 };
    game.swing_arm();
    game.send(S::Punch(Empty));
}

fn block_colour(preset: &str, name: &str) -> Option<u32> {
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

// Block ESP's search of the loaded blocks around the player: again when the
// player has moved, the settings changed, or it has been a while.
fn scan_blocks(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let preset = m.p("Blocks");
    let key = match preset {
        "Ores" => 0,
        "Diamond" => 1,
        "Ancient Debris" => 2,
        "Spawners" => 3,
        _ => 4,
    };
    let range = m.n("Range") as i32;
    let at = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    let (last, last_at, last_ticks) = with(|s| (s.found_for, s.found_at, s.found_at));
    let moved = (at[0] - last.2[0]).abs().max((at[1] - last.2[1]).abs()).max((at[2] - last.2[2]).abs());
    if (key, range) == (last.0, last.1) && moved < 6 && ticks.saturating_sub(last_ticks) < 100 && last_at != 0 {
        return;
    }
    let facts = game.res.facts();
    let mut found = Vec::new();
    'scan: for x in at[0] - range..=at[0] + range {
        for z in at[2] - range..=at[2] + range {
            for y in (at[1] - range).max(-64)..=(at[1] + range).min(320) {
                let state = game.world.block(x, y, z);
                if facts.is_air(state) {
                    continue;
                }
                if let Some(c) = block_colour(preset, &facts.get(state).name) {
                    found.push(([x, y, z], c));
                    if found.len() >= 2000 {
                        break 'scan;
                    }
                }
            }
        }
    }
    with(|s| {
        s.found = found;
        s.found_for = (key, range, at);
        s.found_at = ticks.max(1);
    });
}

// A click or several in the player's own inventory (the menu with no
// container open), by the menu's slot numbers: 0 to 8 the hotbar, 9 to 35
// the rest, 500 to 503 the armor.
fn menu_click(game: &mut Game<'_>, ops: &[(usize, horizon_client::menu::Input)]) {
    let ops: Vec<(Option<usize>, i32, horizon_client::menu::Input)> =
        ops.iter().map(|&(slot, input)| (Some(slot), 0, input)).collect();
    game.remote_menu_ops(&ops);
}

fn inventory_idle(game: &Game<'_>) -> bool {
    game.native_playing() && game.remote.is_some() && game.remote_menu().1 == "inventory"
}

// A rough rank of an armor piece: its material, less what it has worn.
fn armor_score(item: &str, damage: u32) -> f32 {
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
fn wear(game: &mut Game<'_>, i: usize, a: usize) {
    use horizon_client::menu::Input::{Pickup, QuickMove};
    if game.armor[a].is_none() {
        menu_click(game, &[(i, QuickMove)]);
    } else {
        menu_click(game, &[(i, Pickup), (500 + a, Pickup), (i, Pickup)]);
    }
}

fn auto_armor(game: &mut Game<'_>, m: &Module) {
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
fn chest_swap(game: &mut Game<'_>) {
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

fn auto_replenish(game: &mut Game<'_>, m: &Module) {
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
fn auto_tool(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    let down = game.native_playing() && game.physical_buttons.contains(&MouseButton::Left) && !gui_open();
    if !down {
        if let Some(back) = with(|s| s.tool_back.take())
            && m.b("Switch back")
        {
            hold(game, back);
        }
        return;
    }
    if game.remote.is_none() || game.entity_targeted() {
        return;
    }
    let Some(hit) = game.pick() else {
        return;
    };
    let name = game.res.facts().get(game.world.block(hit.pos[0], hit.pos[1], hit.pos[2])).name.clone();
    let speed = |game: &Game<'_>, slot: usize| {
        game.res.items().mining_speed(game.inventory.get(slot).and_then(|s| s.as_ref()), &name)
    };
    let current = game.selected;
    let mut best = (current, speed(game, current));
    for slot in 0..9 {
        let sp = speed(game, slot);
        if sp > best.1 + 0.01 {
            best = (slot, sp);
        }
    }
    if best.0 != current {
        with(|s| {
            if s.tool_back.is_none() {
                s.tool_back = Some(current);
            }
        });
        hold(game, best.0);
    }
}

// Middle click: throws or uses the chosen item from the hotbar, and switches
// back a few ticks later.
fn middle_click(game: &mut Game<'_>) {
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

fn nuker(game: &mut Game<'_>, m: &Module) {
    if !game.native_playing() || game.remote.is_none() {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    // A block being mined the slow way: stop it when its time is up.
    if let Some((pos, left)) = with(|s| s.nuke) {
        if left <= 1 {
            game.send(S::PlayerAction(play::PlayerAction { action: 3, pos, direction: 1, sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed) }));
            with(|s| s.nuke = None);
        } else {
            with(|s| s.nuke = Some((pos, left - 1)));
        }
        return;
    }
    let range = m.n("Range");
    let floor_y = game.player.as_ref().map_or(0, |p| p.position.y.floor() as i32);
    let facts = game.res.facts();
    let mut found: Vec<(f64, [i32; 3], bool, u32)> = Vec::new();
    let r = range.ceil() as i32;
    let (ex, ey, ez) = (eye[0].floor() as i32, eye[1].floor() as i32, eye[2].floor() as i32);
    for x in ex - r..=ex + r {
        for y in ey - r..=ey + r {
            for z in ez - r..=ez + r {
                if m.b("Keep ground") && y < floor_y {
                    continue;
                }
                let state = game.world.block(x, y, z);
                if facts.is_air(state) {
                    continue;
                }
                let f = facts.get(state);
                if f.hardness < 0.0 || f.name.ends_with("water") || f.name.ends_with("lava") {
                    continue;
                }
                let d = ((f64::from(x) + 0.5 - eye[0]).powi(2) + (f64::from(y) + 0.5 - eye[1]).powi(2) + (f64::from(z) + 0.5 - eye[2]).powi(2)).sqrt();
                if d > range {
                    continue;
                }
                let speed = game.res.items().mining_speed(game.held(), &f.name);
                let per_tick = if f.hardness <= 0.0 { 1.0 } else { speed / f.hardness / if speed > 1.0 { 30.0 } else { 100.0 } };
                let instant = game.creative || per_tick >= 1.0;
                if m.b("Instant only") && !instant {
                    continue;
                }
                let ticks = if instant { 0 } else { (1.0 / per_tick).ceil() as u32 + 2 };
                found.push((d, [x, y, z], instant, ticks));
            }
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut broke = 0;
    for (_, pos, instant, ticks) in found {
        if instant {
            if broke >= m.n("Blocks per tick") as usize {
                break;
            }
            game.send(S::PlayerAction(play::PlayerAction { action: 0, pos, direction: 1, sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed) }));
            broke += 1;
        } else if broke == 0 {
            game.send(S::PlayerAction(play::PlayerAction { action: 0, pos, direction: 1, sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed) }));
            with(|s| s.nuke = Some((pos, ticks)));
            break;
        }
    }
    if broke > 0 {
        game.swing_arm();
        game.send(S::Punch(Empty));
    }
}

// Players who left the list since they were last seen: where they stood.
fn track_logouts(game: &Game<'_>, m: &Module) {
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

fn bow_spam(game: &mut Game<'_>, m: &Module) {
    use winit::event::MouseButton;
    if !game.native_playing()
        || !game.physical_buttons.contains(&MouseButton::Right)
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
fn notify(game: &mut Game<'_>, m: &Module) {
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

// Holes to stand in around the player: air two high, solid below and on the
// four sides. Found again every second.
fn scan_holes(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(p) = game.player.as_ref() else {
        return;
    };
    if with(|s| s.holes_at != 0 && ticks.saturating_sub(s.holes_at) < 20) {
        return;
    }
    let r = m.n("Range") as i32;
    let (px, py, pz) = (p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32);
    let facts = game.res.facts();
    let hard = |x: i32, y: i32, z: i32| -> Option<bool> {
        let state = game.world.block(x, y, z);
        if facts.is_air(state) {
            return None;
        }
        let n = &facts.get(state).name;
        Some(n.ends_with("obsidian") || n.ends_with("bedrock"))
    };
    let mut found = Vec::new();
    for x in px - r..=px + r {
        for z in pz - r..=pz + r {
            for y in (py - r).max(-63)..=py + r {
                if hard(x, y, z).is_some() || hard(x, y + 1, z).is_some() {
                    continue;
                }
                let around = [hard(x, y - 1, z), hard(x + 1, y, z), hard(x - 1, y, z), hard(x, y, z + 1), hard(x, y, z - 1)];
                if around.iter().any(Option::is_none) {
                    continue;
                }
                let safe = around.iter().all(|a| *a == Some(true));
                found.push(([x, y, z], if safe { 0xFF55_FF55 } else { 0xFFFF_FF55 }));
                if found.len() >= 400 {
                    break;
                }
            }
        }
    }
    with(|s| {
        s.holes = found;
        s.holes_at = ticks.max(1);
    });
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
    let in_water = p.swimming || p.fluids.was_touching_water;
    // Criticals: the hit waits for the two-tick hop (see CRIT_STAGE).
    if crits && p.ground.on_ground && !in_water && !p.flying {
        with(|s| {
            s.crit_target = Some((id, rotate));
            s.crit_ticks = 0;
        });
        CRIT_STAGE.store(1, Ordering::Relaxed);
        return;
    }
    strike(game, id, rotate);
}

fn strike(game: &mut Game<'_>, id: u32, rotate: Option<(f32, f32)>) {
    let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
    if let Some((yaw, pitch)) = rotate {
        game.send(S::MovePlayerRot(play::MovePlayerRot {
            yaw,
            pitch,
            flags: u8::from(ground),
        }));
    }
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
// becomes the hop (see `outgoing`); when a tick has none, this sends it.
fn crit_tick(game: &mut Game<'_>) {
    let stage = CRIT_STAGE.load(Ordering::Relaxed);
    if stage == 0 {
        return;
    }
    let grounded_or_hopping = game.player.as_ref().is_some_and(|p| p.ground.on_ground || stage > 1);
    let ticks = with(|s| {
        s.crit_ticks += 1;
        s.crit_ticks
    });
    if !grounded_or_hopping || ticks > 6 {
        CRIT_STAGE.store(0, Ordering::Relaxed);
        with(|s| s.crit_target = None);
        return;
    }
    let mut stage = stage;
    if (stage == 1 || stage == 2) && !POS_THIS_TICK.load(Ordering::Relaxed) {
        let Some(p) = game.player.as_ref() else {
            return;
        };
        let (x, y, z) = (p.position.x, p.position.y, p.position.z);
        let y = if stage == 1 { y + 0.0625 } else { y };
        game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags: 0 }));
        stage += 1;
        CRIT_STAGE.store(stage, Ordering::Relaxed);
        POS_THIS_TICK.store(true, Ordering::Relaxed);
    }
    if CRIT_STAGE.load(Ordering::Relaxed) == 3 {
        CRIT_STAGE.store(0, Ordering::Relaxed);
        if let Some((id, rotate)) = with(|s| s.crit_target.take()) {
            strike(game, id, rotate);
        }
    }
}

fn kill_aura(game: &mut Game<'_>, m: &Module, crits: bool) {
    if !game.native_playing() || game.remote.is_none() || CRIT_STAGE.load(Ordering::Relaxed) != 0 {
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

// What the send hook does with an outgoing packet.
pub(super) enum Out {
    Pass,
    Drop,
    Replace(S),
}

fn with_ground(packet: &S, ground: bool) -> Option<S> {
    let f = |flags: u8| (flags & !1) | u8::from(ground);
    Some(match packet {
        S::MovePlayerPos(m) => S::MovePlayerPos(play::MovePlayerPos { x: m.x, y: m.y, z: m.z, flags: f(m.flags) }),
        S::MovePlayerPosRot(m) => S::MovePlayerPosRot(play::MovePlayerPosRot {
            x: m.x,
            y: m.y,
            z: m.z,
            yaw: m.yaw,
            pitch: m.pitch,
            flags: f(m.flags),
        }),
        S::MovePlayerRot(m) => S::MovePlayerRot(play::MovePlayerRot { yaw: m.yaw, pitch: m.pitch, flags: f(m.flags) }),
        S::MovePlayerStatus(m) => S::MovePlayerStatus(play::MovePlayerStatus { flags: f(m.flags) }),
        _ => return None,
    })
}

// Outgoing packets: Blink holds moves, Anti Hunger hides sprinting and the
// ground, No Fall claims the ground after a long fall.
pub(super) fn outgoing(game: &Game<'_>, packet: &S) -> Out {
    let is_move = matches!(
        packet,
        S::MovePlayerPos(_) | S::MovePlayerPosRot(_) | S::MovePlayerRot(_) | S::MovePlayerStatus(_)
    );
    if is_move && BLINK.load(Ordering::Relaxed) {
        BLINK_HELD.store(true, Ordering::Relaxed);
        return Out::Drop;
    }
    if ANTI_SPRINT.load(Ordering::Relaxed)
        && let S::PlayerCommand(c) = packet
        && (c.action == 1 || c.action == 2)
    {
        return Out::Drop;
    }
    if !is_move {
        return Out::Pass;
    }
    let has_position = matches!(packet, S::MovePlayerPos(_) | S::MovePlayerPosRot(_));
    // Criticals: this tick's regular position packet is the hop (stage 1: up,
    // stage 2: back down), both off the ground.
    let stage = CRIT_STAGE.load(Ordering::Relaxed);
    if has_position && (stage == 1 || stage == 2) {
        let hop = |y: f64| if stage == 1 { y + 0.0625 } else { y };
        let changed = match packet {
            S::MovePlayerPos(m) => S::MovePlayerPos(play::MovePlayerPos { x: m.x, y: hop(m.y), z: m.z, flags: 0 }),
            S::MovePlayerPosRot(m) => S::MovePlayerPosRot(play::MovePlayerPosRot {
                x: m.x,
                y: hop(m.y),
                z: m.z,
                yaw: m.yaw,
                pitch: m.pitch,
                flags: 0,
            }),
            _ => unreachable!(),
        };
        CRIT_STAGE.store(stage + 1, Ordering::Relaxed);
        POS_THIS_TICK.store(true, Ordering::Relaxed);
        return Out::Replace(changed);
    }
    if has_position {
        POS_THIS_TICK.store(true, Ordering::Relaxed);
    }
    let Some(p) = game.player.as_ref() else {
        return Out::Pass;
    };
    let ground = if NO_FALL.load(Ordering::Relaxed) && p.fall_distance > 2.0 && !p.flying && !p.fall_flying {
        Some(true)
    } else if ANTI_GROUND.load(Ordering::Relaxed) && (p.fall_distance <= 0.0 || p.flying) {
        Some(false)
    } else {
        None
    };
    match ground.and_then(|g| with_ground(packet, g)) {
        Some(changed) => Out::Replace(changed),
        None => Out::Pass,
    }
}

// Where the player is, as one position packet: what Blink held back is a
// single move to here, as the server takes one position packet a tick.
fn flush_blink(game: &mut Game<'_>) {
    if !BLINK_HELD.swap(false, Ordering::Relaxed) || POS_THIS_TICK.load(Ordering::Relaxed) {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let flags = u8::from(p.ground.on_ground) | u8::from(p.collision.horizontal) << 1;
    let (x, y, z) = (p.position.x, p.position.y, p.position.z);
    game.send(S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }));
    POS_THIS_TICK.store(true, Ordering::Relaxed);
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
    (".settings <module>", "list a module's settings"),
    (".set <module> <setting> <value>", "change a setting"),
    (".bind <module> <key|none>", "bind a key to a module"),
    (".binds", "list key binds"),
    (".reset <module>", "put a module back to its defaults"),
    (".friend add|remove|list [name]", "Kill Aura and Trigger Bot skip friends"),
    (".waypoint add|remove|list [name]", "save a place (.wp)"),
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
        "menu" | "gui" => open_gui(game),
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
    if with(|s| s.gui.open) {
        if game.screen == Screen::Playing && game.player.is_some() {
            draw_gui(game, w, h, &mut shapes);
        } else {
            with(|s| {
                s.gui.open = false;
                s.gui.drag = None;
            });
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
    let (crumbs, blocks, points) = (get("Breadcrumbs"), get("Block ESP"), get("Waypoints"));
    let (spots, flight_path, holes) = (get("Logout Spots"), get("Trajectories"), get("Hole ESP"));
    if esp.is_none()
        && tracers.is_none()
        && tags.is_none()
        && storage.is_none()
        && crumbs.is_none()
        && blocks.is_none()
        && points.is_none()
        && spots.is_none()
        && flight_path.is_none()
        && holes.is_none()
    {
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

    if crumbs.is_some() {
        let trail = with(|s| s.crumbs.clone());
        let mut last: Option<[f32; 2]> = None;
        for point in &trail {
            let now = cam.project(*point);
            if let (Some(a), Some(b)) = (last, now) {
                line(out, a, b, 0xFF00_0000 | ACCENT, lw);
            }
            last = now;
        }
        // To where you are now.
        if let (Some(a), Some(p)) = (last, game.player.as_ref())
            && let Some(b) = cam.project([p.position.x, p.position.y + 0.1, p.position.z])
        {
            line(out, a, b, 0xFF00_0000 | ACCENT, lw);
        }
    }

    if let Some(m) = blocks {
        let found = with(|s| s.found.clone());
        let range = m.n("Range") * 1.5;
        for (pos, col) in found {
            let at = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5];
            if dist(at) > range {
                continue;
            }
            let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
            let hi = [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0];
            if let Some(r) = cam.rect(lo, hi) {
                outline(out, r, col, lw, m.b("Fill"));
            }
        }
    }

    if spots.is_some() {
        let here = game.native_player().map(|p| p.dimension);
        let spots = with(|s| s.spots.clone());
        for sp in spots.iter().filter(|sp| Some(sp.dim.as_str()) == here) {
            let lo = [sp.pos[0] - 0.3, sp.pos[1], sp.pos[2] - 0.3];
            let hi = [sp.pos[0] + 0.3, sp.pos[1] + 1.8, sp.pos[2] + 0.3];
            if let Some(r) = cam.rect(lo, hi) {
                outline(out, r, 0xFFFF_5050, lw, true);
            }
            if let Some([x, y]) = cam.project([sp.pos[0], sp.pos[1] + 2.2, sp.pos[2]]) {
                let mins = game.ticks.saturating_sub(sp.at) / 1200;
                let text = format!("{} logged out {}m ago", sp.name, mins);
                let tw = game.native_text_width(&text);
                out.push(Shape::Fill([x - tw / 2.0 - 2.0, y - 2.0, tw + 4.0, 12.0], 0x7800_0000));
                out.push(Shape::Text(text, [x - tw / 2.0, y], 0xFF5555));
            }
        }
    }

    if flight_path.is_some() {
        trajectory(game, cam, out, lw);
    }

    if let Some(m) = holes {
        let found = with(|s| s.holes.clone());
        for (pos, col) in found {
            let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
            if dist([lo[0] + 0.5, lo[1], lo[2] + 0.5]) > m.n("Range") + 2.0 {
                continue;
            }
            if let Some(r) = cam.rect(lo, [lo[0] + 1.0, lo[1] + 0.02, lo[2] + 1.0]) {
                outline(out, r, col, lw, m.b("Fill"));
            }
        }
    }

    if let Some(m) = points {
        let here = game.native_player().map(|p| p.dimension);
        let waypoints = with(|s| s.waypoints.clone());
        for w in waypoints.iter().filter(|w| Some(w.dim.as_str()) == here) {
            let d = dist(w.pos);
            if d > m.n("Range") {
                continue;
            }
            let Some([x, y]) = cam.project(w.pos) else {
                continue;
            };
            let text = format!("{} {}m", w.name, d.round());
            let tw = game.native_text_width(&text);
            out.push(Shape::Fill([x - tw / 2.0 - 2.0, y - 2.0, tw + 4.0, 12.0], 0x7800_0000));
            out.push(Shape::Text(text, [x - tw / 2.0, y], ACCENT));
            out.push(Shape::Fill([x - 1.0, y + 11.0, 2.0, 4.0], 0xFF00_0000 | ACCENT));
        }
    }
}

// Where what the player holds would land: a bow being drawn, or a throwable.
fn trajectory(game: &Game<'_>, cam: &Camera, out: &mut Vec<Shape>, lw: f32) {
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
    let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
    let dir = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let mut pos = [eye[0], eye[1] - 0.1, eye[2]];
    let mut vel = [dir[0] * speed, dir[1] * speed, dir[2] * speed];
    let mut last = cam.project(pos);
    for _ in 0..200 {
        for i in 0..3 {
            pos[i] += vel[i];
            vel[i] *= 0.99;
        }
        vel[1] -= gravity;
        let (x, y, z) = (pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32);
        let hit = solid_at(game, x, y, z);
        let now = cam.project(pos);
        if let (Some(a), Some(b)) = (last, now) {
            line(out, a, b, 0xFF00_0000 | ACCENT, lw);
        }
        last = now;
        if hit {
            let lo = [pos[0] - 0.25, pos[1] - 0.25, pos[2] - 0.25];
            let hi = [pos[0] + 0.25, pos[1] + 0.25, pos[2] + 0.25];
            if let Some(r) = cam.rect(lo, hi) {
                outline(out, r, 0xFF00_0000 | ACCENT, lw, true);
            }
            break;
        }
        if pos[1] < -80.0 {
            break;
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

// ---------------------------------------------------------------- click GUI
//
// Meteor's style: every category as a panel at once, over the world. Left
// click turns a module on or off, right click opens its settings under it;
// headers drag the panels (right click folds one). Numbers are sliders. The
// game keeps running (single player does not pause), the pointer is free and
// the window's keys and clicks come here while it is open.

#[derive(Clone, Copy, Debug, PartialEq)]
struct Panel {
    // Negative until the first frame places it.
    x: f32,
    y: f32,
    collapsed: bool,
    scroll: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Panel(usize, f32, f32),
    Slider(usize, usize),
}

#[derive(Clone, Debug)]
struct Gui {
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
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            open: false,
            closing: false,
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

// Five panels fit a 427-pixel-wide GUI (1280 by 720 at scale 3).
const PANEL_W: f32 = 82.0;
const HEAD_H: f32 = 13.0;
const ROW_H: f32 = 11.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Header(usize),
    Module(usize),
    Bind(usize),
    Setting(usize, usize),
}

// The rows of one panel before scrolling: their offset from the top of its
// body, what they are, and how tall the whole body is.
fn panel_body(s: &State, c: usize) -> (Vec<(f32, Hit)>, f32) {
    let mut rows = Vec::new();
    let mut y = 0.0;
    let query = norm(&s.gui.search);
    for (i, m) in s.modules.iter().enumerate().filter(|(_, m)| m.category == CATEGORIES[c] && norm(m.name).contains(&query)) {
        rows.push((y, Hit::Module(i)));
        y += ROW_H;
        if s.gui.expanded.contains(&i) {
            rows.push((y, Hit::Bind(i)));
            y += ROW_H;
            for j in 0..m.settings.len() {
                rows.push((y, Hit::Setting(i, j)));
                y += ROW_H;
            }
        }
    }
    (rows, y)
}

// How much of a panel's body shows: what is left of the screen under it.
fn body_room(s: &State, c: usize) -> f32 {
    (s.gui.view_h - s.gui.panels[c].y - HEAD_H - 14.0).max(ROW_H * 3.0)
}

// Each row of every panel, top to bottom: its rectangle and what it is. A
// row only part way in view is left out.
fn gui_rows(s: &State) -> Vec<([f32; 4], Hit)> {
    let mut out = Vec::new();
    for c in 0..CATEGORIES.len() {
        let p = s.gui.panels[c];
        let (rows, _) = panel_body(s, c);
        // A search leaves out the panels with nothing that matches.
        if rows.is_empty() && !s.gui.search.is_empty() {
            continue;
        }
        out.push(([p.x, p.y, PANEL_W, HEAD_H], Hit::Header(c)));
        if p.collapsed {
            continue;
        }
        let room = body_room(s, c);
        for (off, hit) in rows {
            let top = off - p.scroll;
            if top < -0.01 || top + ROW_H > room + 0.01 {
                continue;
            }
            out.push(([p.x, p.y + HEAD_H + top, PANEL_W, ROW_H], hit));
        }
    }
    out
}

// The wheel over a panel scrolls it.
pub(super) fn gui_wheel(game: &Game<'_>, steps: f32) {
    let at = game.cursor;
    with(|s| {
        for c in 0..CATEGORIES.len() {
            let p = s.gui.panels[c];
            let (_, content) = panel_body(s, c);
            let room = body_room(s, c);
            let shown = if p.collapsed { 0.0 } else { content.min(room) };
            if inside([p.x, p.y, PANEL_W, HEAD_H + shown], at) {
                let max = (content - room).max(0.0);
                s.gui.panels[c].scroll = (p.scroll - steps * ROW_H * 2.0).clamp(0.0, max);
                return;
            }
        }
    });
}

fn inside(r: [f32; 4], (x, y): (f32, f32)) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

fn hit_at(s: &State, at: (f32, f32)) -> Option<([f32; 4], Hit)> {
    gui_rows(s).into_iter().find(|(r, _)| inside(*r, at))
}

pub(super) fn open_gui(game: &mut Game<'_>) {
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

fn close_gui(game: &mut Game<'_>) {
    with(|s| {
        s.gui.open = false;
        s.gui.closing = false;
        s.gui.drag = None;
        s.binding = None;
    });
    save(game);
    if game.screen == Screen::Playing {
        game.capture(true);
    }
}

// A slider's value from the pointer's place along its row.
fn slide(s: &mut State, i: usize, j: usize, r: [f32; 4], x: f32) -> bool {
    let Some(setting) = s.modules[i].settings.get_mut(j) else {
        return false;
    };
    if let Value::Num { v, min, max, step } = &mut setting.value {
        let t = ((x - r[0] - 2.0) / (r[2] - 4.0)).clamp(0.0, 1.0);
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
fn gui_key(game: &mut Game<'_>, name: &str) {
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
            s.modules[i].key = chosen;
            s.binding = None;
        });
        save(game);
        return;
    }
    if name == "key.keyboard.escape" {
        // The first Esc clears a search; the next closes the GUI.
        if with(|s| std::mem::take(&mut s.gui.search)).is_empty() {
            with(|s| s.gui.closing = true);
        }
        return;
    }
    if name == game.native_mod_key(ID, MENU_KEY, MENU_DEFAULT) {
        with(|s| s.gui.closing = true);
        return;
    }
    if name == "key.keyboard.backspace" {
        with(|s| {
            s.gui.search.pop();
        });
    } else if let Some(c) = key_char(name)
        && !game.modifiers.control_key()
    {
        with(|s| {
            if s.gui.search.chars().count() < 20 {
                s.gui.search.push(c);
            }
        });
    }
}

// A letter, digit or space typed in the click GUI's search.
fn key_char(name: &str) -> Option<char> {
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
    let at = game.cursor;
    let right = match button {
        MouseButton::Left => false,
        MouseButton::Right => true,
        _ => return,
    };
    let mut toggled = None;
    let mut changed = false;
    with(|s| {
        let Some((r, hit)) = hit_at(s, at) else {
            return;
        };
        match (hit, right) {
            (Hit::Header(c), false) => {
                let p = s.gui.panels[c];
                s.gui.drag = Some(Drag::Panel(c, at.0 - p.x, at.1 - p.y));
            }
            (Hit::Header(c), true) => {
                s.gui.panels[c].collapsed = !s.gui.panels[c].collapsed;
                changed = true;
            }
            (Hit::Module(i), false) => toggled = Some(i),
            (Hit::Module(i), true) => {
                if let Some(k) = s.gui.expanded.iter().position(|&e| e == i) {
                    s.gui.expanded.remove(k);
                } else {
                    s.gui.expanded.push(i);
                }
            }
            (Hit::Bind(i), false) => s.binding = if s.binding == Some(i) { None } else { Some(i) },
            (Hit::Bind(i), true) => {
                s.modules[i].key = None;
                s.binding = None;
                changed = true;
            }
            (Hit::Setting(i, j), right) => {
                let num = matches!(s.modules[i].settings[j].value, Value::Num { .. });
                if num && !right {
                    s.gui.drag = Some(Drag::Slider(i, j));
                    changed = slide(s, i, j, r, at.0);
                } else if num {
                    // Right click on a slider: back to its default.
                    let fresh = defaults().swap_remove(i).settings.swap_remove(j);
                    s.modules[i].settings[j] = fresh;
                    changed = true;
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
fn follow_drag(game: &Game<'_>, w: f32, h: f32) {
    let at = game.cursor;
    with(|s| match s.gui.drag {
        Some(Drag::Panel(c, dx, dy)) => {
            let p = &mut s.gui.panels[c];
            p.x = (at.0 - dx).clamp(0.0, (w - PANEL_W).max(0.0));
            p.y = (at.1 - dy).clamp(0.0, (h - HEAD_H).max(0.0));
        }
        Some(Drag::Slider(i, j)) => {
            let row = gui_rows(s).into_iter().find(|(_, hit)| *hit == Hit::Setting(i, j));
            if let Some((r, _)) = row {
                slide(s, i, j, r, at.0);
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

fn draw_gui(game: &Game<'_>, w: f32, h: f32, out: &mut Vec<Shape>) {
    follow_drag(game, w, h);
    with(|s| {
        s.gui.view_h = h;
        // Panels with no place yet go left to right, in a second row when the
        // window is narrow.
        let mut x = 2.0;
        let mut y = 32.0;
        for p in s.gui.panels.iter_mut().filter(|p| p.x < 0.0) {
            if x + PANEL_W > w {
                x = 2.0;
                y += 16.0;
            }
            (p.x, p.y) = (x, y);
            x += PANEL_W + 3.0;
        }
        for p in &mut s.gui.panels {
            p.x = p.x.clamp(0.0, (w - PANEL_W).max(0.0));
            p.y = p.y.clamp(0.0, (h - HEAD_H).max(0.0));
        }
        // A scroll past the end (a shorter body, a taller window) comes back.
        for c in 0..CATEGORIES.len() {
            let (_, content) = panel_body(s, c);
            let max = (content - body_room(s, c)).max(0.0);
            s.gui.panels[c].scroll = s.gui.panels[c].scroll.clamp(0.0, max);
        }
    });
    let at = game.cursor;
    let (rows, modules, binding, hover) = with(|s| (gui_rows(s), s.modules.clone(), s.binding, hit_at(s, at).map(|(_, h)| h)));
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
    // The world dims a little behind the panels, as a screen would.
    out.push(Shape::Fill([0.0, 0.0, w, h], 0x3000_0000));
    for (r, hit) in &rows {
        let [x, y, rw, rh] = *r;
        let hovered = hover == Some(*hit);
        match *hit {
            Hit::Header(c) => {
                out.push(Shape::Fill([x, y, rw, rh], 0xF014_1414));
                out.push(Shape::Fill([x, y + rh - 1.0, rw, 1.0], 0xFF00_0000 | ACCENT));
                let label = CATEGORIES[c].label();
                let on = modules.iter().filter(|m| m.category == CATEGORIES[c] && m.on).count();
                let count = format!("{on}");
                out.push(Shape::Text(label.to_owned(), [x + 4.0, y + 2.5], 0xFFFFFF));
                let cw = game.native_text_width(&count);
                out.push(Shape::Text(count, [x + rw - 4.0 - cw, y + 2.5], 0xAAAAAA));
            }
            Hit::Module(i) => {
                let m = &modules[i];
                out.push(Shape::Fill([x, y, rw, rh], if hovered { 0xD02A_2A2A } else { 0xC010_1010 }));
                if m.on {
                    out.push(Shape::Fill([x, y, 2.0, rh], 0xFF00_0000 | ACCENT));
                }
                let colour = if m.on { ACCENT } else { 0xE0E0E0 };
                out.push(Shape::Text(fit(m.name, rw - 16.0), [x + 5.0, y + 1.5], colour));
                if !m.settings.is_empty() || m.key.is_some() {
                    out.push(Shape::Text("...".into(), [x + rw - 12.0, y + 0.5], 0x808080));
                }
            }
            Hit::Bind(i) => {
                out.push(Shape::Fill([x, y, rw, rh], 0xD008_0808));
                let text = if binding == Some(i) {
                    "Bind: press a key".to_owned()
                } else {
                    format!("Bind: {}", key_label(modules[i].key.as_deref()))
                };
                out.push(Shape::Text(fit(&text, rw - 12.0), [x + 9.0, y + 1.5], if binding == Some(i) { 0xFFFF55 } else { 0xB0B0B0 }));
            }
            Hit::Setting(i, j) => {
                let st = &modules[i].settings[j];
                out.push(Shape::Fill([x, y, rw, rh], if hovered { 0xD01C_1C1C } else { 0xD008_0808 }));
                if let Value::Num { v, min, max, .. } = st.value {
                    let t = if max > min { ((v - min) / (max - min)) as f32 } else { 0.0 };
                    out.push(Shape::Fill([x + 2.0, y + rh - 2.0, (rw - 4.0) * t, 1.0], 0xFF00_0000 | ACCENT));
                }
                let colour = match st.value {
                    Value::Bool(true) => ACCENT,
                    _ => 0xD0D0D0,
                };
                out.push(Shape::Text(fit(&st.shown(), rw - 12.0), [x + 9.0, y + 1.5], colour));
            }
        }
    }
    // A panel with more below its last row shows it.
    let more: Vec<(f32, f32)> = with(|s| {
        (0..CATEGORIES.len())
            .filter(|&c| !s.gui.panels[c].collapsed)
            .filter_map(|c| {
                let p = s.gui.panels[c];
                let (_, content) = panel_body(s, c);
                let room = body_room(s, c);
                (content - p.scroll > room + 0.5).then_some((p.x, p.y + HEAD_H + room))
            })
            .collect()
    });
    for (x, y) in more {
        out.push(Shape::Fill([x, y, PANEL_W, 8.0], 0xE014_1414));
        out.push(Shape::Text("v".into(), [x + PANEL_W / 2.0 - 2.0, y - 0.5], 0xB0B0B0));
    }
    // The hovered module's description, under the pointer.
    if let Some(Hit::Module(i)) = hover
        && with(|s| s.gui.drag.is_none())
    {
        let about = modules[i].about;
        let tw = game.native_text_width(about);
        let bx = (at.0 + 8.0).min(w - tw - 6.0).max(0.0);
        let by = (at.1 + 10.0).min(h - 12.0);
        out.push(Shape::Fill([bx, by, tw + 6.0, 12.0], 0xF000_0000));
        out.push(Shape::Text(about.to_owned(), [bx + 3.0, by + 2.0], 0xFFFFFF));
    }
    // The search box, and above the hotbar a reminder of the clicks.
    let search = with(|s| s.gui.search.clone());
    let (label, colour) = if search.is_empty() {
        ("Type to search".to_owned(), 0x909090)
    } else {
        (format!("Search: {search}_"), ACCENT)
    };
    let lw_ = game.native_text_width(&label);
    out.push(Shape::Fill([(w - lw_) / 2.0 - 4.0, 14.0, lw_ + 8.0, 12.0], 0xC010_1010));
    out.push(Shape::Text(label, [(w - lw_) / 2.0, 16.0], colour));
    let hint = "Left click: on/off   Right click: settings   Drag headers   Esc: close";
    let hw = game.native_text_width(hint);
    out.push(Shape::Text(hint.to_owned(), [(w - hw) / 2.0, h - 54.0], 0xA0A0A0));
}
