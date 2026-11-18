// Runtime part (see runtime.rs): modules, their categories and settings, and what each setting shows.

use super::sunset::*;

// ---------------------------------------------------------------- modules

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Combat,
    Movement,
    Player,
    Render,
    World,
    // Not cheats: the client's own (profiles, sorting, which extra modules the menu lists).
    Client,
}

pub(super) const CATEGORIES: [Category; 6] = [
    Category::Combat,
    Category::Movement,
    Category::Player,
    Category::Render,
    Category::World,
    Category::Client,
];

// The menus (both ClickGUI styles, the Options pages, .modules, Modules' switches) list modules A to Z, as Rise,
// Meteor and LiquidBounce do, whatever order the catalog and plugins add them in. Case is ignored; equal names keep
// their order. A search still ranks what it finds best first (search.rs).
pub(super) fn alphabetical(modules: &[Module], ids: impl IntoIterator<Item = usize>) -> Vec<usize> {
    let mut ids: Vec<usize> = ids.into_iter().collect();
    ids.sort_by_cached_key(|&i| modules[i].name.to_ascii_lowercase());
    ids
}

impl Category {
    pub(super) fn label(self) -> &'static str {
        match self {
            Category::Combat => "Combat",
            Category::Movement => "Movement",
            Category::Player => "Player",
            Category::Render => "Render",
            Category::World => "World",
            Category::Client => "Client",
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
    pub(super) fn shown(&self) -> String {
        format!("{}: {}", self.name, self.value_text())
    }
    pub(super) fn value_text(&self) -> String {
        match &self.value {
            Value::Bool(b) => on_off(*b).to_owned(),
            Value::Num { v, .. } if self.all_loaded(*v) => "All".to_owned(),
            Value::Num { v, .. } => num(*v),
            Value::Pick { i, of } => of[*i].to_owned(),
            Value::Text(t) if t.is_empty() => "(empty)".to_owned(),
            Value::Text(t) => t.chars().map(|c| if c.is_control() { '?' } else { c }).collect(),
        }
    }
    // ESP's, Tracers' and Nametags' Range at 0: everything the server sent, however far.
    fn all_loaded(&self, v: f64) -> bool {
        self.name == "Range" && v == 0.0 && matches!(self.value, Value::Num { min, max, .. } if min == 0.0 && max == ALL_RANGE_MAX)
    }
    // As the config and profiles keep it: numbers to six places (HUD positions step by 0.001, frictions by 0.005;
    // two places moved them a little at every save), text on one line.
    pub(super) fn stored(&self) -> String {
        match &self.value {
            Value::Bool(b) => b.to_string(),
            Value::Num { v, .. } => exact(*v),
            Value::Pick { i, of } => of[*i].to_owned(),
            Value::Text(t) => one_line(t),
        }
    }
    // A click: a switch flips, a number goes up a step (down with Shift) and
    // wraps at its ends, a choice moves to the next.
    pub(super) fn click(&mut self, down: bool) {
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
    pub(super) fn parse(&mut self, text: &str) -> bool {
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
            Value::Text(v) => *v = one_line(t),
        }
        true
    }
}

// A number as the config keeps it: up to six places, no trailing zeros.
pub(super) fn exact(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

// Text with its line breaks and other control characters made spaces: a value is one line of the config.
fn one_line(t: &str) -> String {
    if t.chars().any(char::is_control) { t.chars().map(|c| if c.is_control() { ' ' } else { c }).collect() } else { t.to_owned() }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Module {
    pub(crate) name: &'static str,
    pub(crate) category: Category,
    pub(crate) about: &'static str,
    pub(crate) on: bool,
    pub(crate) key: Option<String>,
    pub(crate) settings: Vec<Setting>,
    // Its settings' sections, for the menu: where each starts (a setting's index) and its title. None for most modules.
    pub(crate) groups: Vec<(usize, &'static str)>,
    // In the ClickGUI (every style, its search and its counts). False (`menu_only`): only in Options > Mods > Sunset
    // and `.pages`; still toggled by `.t` and its key.
    pub(crate) in_gui: bool,
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
            groups: Vec::new(),
            in_gui: true,
        }
    }
    // The settings after this, up to the next group, are a section of the menu under `title`. Only the menu reads it:
    // saved settings go by name.
    pub(super) fn group(mut self, title: &'static str) -> Self {
        self.groups.push((self.settings.len(), title));
        self
    }
    // The section a setting is in, if the module has sections.
    pub(super) fn group_of(&self, j: usize) -> Option<&'static str> {
        self.groups.iter().rev().find(|(start, _)| *start <= j).map(|(_, title)| *title)
    }
    pub(super) fn menu_only(mut self) -> Self {
        self.in_gui = false;
        self
    }
    pub(super) fn on(mut self) -> Self {
        self.on = true;
        self
    }
    // Kill Aura's and Trigger Bot's Shield: the offhand shield kept up, lowered for each hit (sunset_hands).
    pub(super) fn shield(self) -> Self {
        self.bool("Shield", false)
            .bool("Shield only with a weapon", true)
            .bool("Shield only near a target", true)
            .num("Shield range", 6.0, 2.0, 16.0, 0.5)
            .num("Shield re-raise delay", 0.0, 0.0, 10.0, 1.0)
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
    // Auto Weapon: whether its Mode runs this section.
    pub(super) fn weapon(&self, section: &str) -> bool {
        super::sunset_melee::weapon_on(self, section)
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
    // What the module list shows beside the name (as Sunset's info string).
    pub(super) fn info(&self) -> Option<String> {
        match self.name {
            "Flight" => Some(self.p("Mode").to_owned()),
            "Kill Aura" => Some(num(self.n("Range"))),
            "Fast Place" => Some(num(self.n("Speed"))),
            "Speed" | "Criticals" | "Fake Lag" => Some(self.p("Mode").to_owned()),
            "Client Spoofer" => Some(self.p("Client").to_owned()),
            "Ore Sim" => super::sunset_ore_sim::info(self),
            "Baritone" => super::sunset_baritone::info(self),
            "Auto Sell" | "Auction Sniper" => super::sunset_cyanite_economy::info(self),
            "Offhand" if self.b("Show totem count") => Some(super::sunset_combat_inventory::totem_count().to_string()),
            _ => None,
        }
    }

    pub(super) fn setting_visible(&self, j: usize) -> bool {
        let name = self.settings[j].name;
        let advanced = self.b("Advanced");
        match self.name {
            "Ore Sim" if matches!(name, "Database fetch" | "Cracker threads") => self.p("Seed source") == "Auto-crack",
            // Kill Aura's and Trigger Bot's Shield: its settings with it on.
            "Kill Aura" if name == "Shield" => false, // Retained compatibility alias, superseded by AutoBlock.
            "Kill Aura" if name.starts_with("Shield ") => self.p("AutoBlock") == "Modern",
            "Kill Aura" if name == "Legacy reblock delay" => self.p("AutoBlock") == "Legacy",
            "Trigger Bot" if name.starts_with("Shield ") => self.b("Shield"),
            // Streamer Mode: what shows in place of a name, with that name hidden.
            "Streamer Mode" => match name {
                "Your name" => self.b("Hide your name"),
                "Others as" | "Others' name" => self.b("Hide others"),
                _ => true,
            },
            // Client Spoofer: a profile's own settings with it picked (all of them with Per server set).
            "Client Spoofer" => {
                let picked = |profile: &str| self.p("Client") == profile || !self.t("Per server").trim().is_empty();
                match name {
                    "Custom brand" => picked("Custom"),
                    "Lunar version" | "Lunar Fabric tag" => picked("Lunar Client"),
                    "LabyMod version" => picked("LabyMod"),
                    _ => true,
                }
            }
            "Auto Sign" => super::sunset_auto_sign::visible(self, name),
            "Auto Sell" => match name {
                "Shop command" | "Shop title" | "Stacks per visit" => self.p("Mode") == "Shop",
                "Price" | "Skip tools and weapons" | "Skip food" | "Switch back" => self.p("Mode") == "Auction",
                _ => true,
            },
            "Auction Sniper" => match name {
                "Auction command" => self.b("Open auction"),
                "Confirm title" | "Confirm button text" | "Confirm item" => self.b("Confirm screen"),
                _ => true,
            },
            "HUD" => match name {
                "Second color" => self.p("List color mode") != "Static" || self.p("Watermark color mode") != "Static",
                "Color speed" => [self.p("List color mode"), self.p("Watermark color mode")].iter().any(|m| *m != "Static"),
                "Watermark text" | "Watermark profile" | "Watermark color mode" | "Watermark position" => self.b("Watermark"),
                "Watermark x" | "Watermark y" => self.b("Watermark") && self.p("Watermark position") == "Custom",
                "Module list side" | "Modules shown" | "Module details" | "Remove spaces" | "List sort" | "List color mode"
                | "List background" | "List bar" | "List spacing" | "List animations" | "Module list position" => self.b("Module list"),
                "Module list x" | "Module list y" => self.b("Module list") && self.p("Module list position") == "Custom",
                "Target HUD position" => self.b("Target HUD"),
                "Target HUD x" | "Target HUD y" => self.b("Target HUD") && self.p("Target HUD position") == "Custom",
                "Scoreboard x" | "Scoreboard y" => self.p("Scoreboard position") == "Custom",
                "Coordinates position" => self.b("Coordinates"),
                "Coordinates x" | "Coordinates y" => self.b("Coordinates") && self.p("Coordinates position") == "Custom",
                "Compass position" => self.b("Compass"),
                "Compass x" | "Compass y" => self.b("Compass") && self.p("Compass position") == "Custom",
                "Armor HUD x" | "Armor HUD y" => self.b("Armor HUD") && self.p("Armor HUD position") == "Custom",
                // The Info line's parts and Shield status, each placed on its own once dragged.
                "FPS position" | "Speed position" | "Facing position" | "Totems position" | "Shield status position" => {
                    self.b(name.trim_end_matches(" position"))
                }
                "FPS x" | "FPS y" => self.b("FPS") && self.p("FPS position") == "Custom",
                "Speed x" | "Speed y" => self.b("Speed") && self.p("Speed position") == "Custom",
                "Facing x" | "Facing y" => self.b("Facing") && self.p("Facing position") == "Custom",
                "Totems x" | "Totems y" => self.b("Totems") && self.p("Totems position") == "Custom",
                "Shield status x" | "Shield status y" => self.b("Shield status") && self.p("Shield status position") == "Custom",
                _ if name.starts_with("Armor HUD ") => self.b("Armor HUD"),
                "Durability format" | "Durability tooltip" => self.b("Item durability"),
                "Target HUD mode" => self.b("Target HUD"),
                "Low health below" | "Low health style" | "Low health strength" => self.b("Low health warning"),
                _ => true,
            },
            "Client Sort" => name != "Fill CPS" || self.b("Shift fill"),
            "Loadouts" => match name {
                "Hotbar mode" | "Hotbar rules" | "Hotbar delay (ms)" => self.b("Auto hotbar"),
                _ => true,
            },
            "No Slow" => match name {
                "Release items" => self.b("No item release"),
                _ => true,
            },
            "Auto Clicker" => match name {
                "Delay (ticks)" => self.p("Mode") == "Delay",
                "Min CPS" | "Max CPS" => self.b("Left") && self.p("Mode") == "CPS" || self.b("Right") && !self.b("Right own CPS") && self.p("Mode") == "CPS",
                "CPS pattern" => self.p("Mode") == "CPS" || self.b("Right") && self.b("Right own CPS"),
                "Respect miss cooldown" => self.b("Left"),
                "Right items" | "Right sneak only" | "Right start delay (ms)" | "Right own CPS" => self.b("Right"),
                "Right min CPS" | "Right max CPS" => self.b("Right") && self.b("Right own CPS"),
                _ => true,
            },
            "Trigger Bot" => match name {
                "Shortest cooldown (%)" | "Longest cooldown (%)" => self.b("Wait for cooldown"),
                "Sticky range" => self.b("Sticky target"),
                _ => true,
            },
            "Auto Gap" => match name {
                "Pot mode" | "Pot pitch" => self.b("Auto pot"),
                "Pot health" => self.b("Auto pot") && self.p("Pot mode") == "Auto",
                "Soup health" | "Soup delay (ms)" | "Drop bowls" => self.b("Soup"),
                _ => true,
            },
            "Auto Replenish" => match name {
                "Refill key" => self.p("Mode") == "Key",
                "Healing only" | "Potion delay (ticks)" => self.b("Potion refill"),
                _ => true,
            },
            "Flight" => {
                let mode = self.p("Mode");
                let old = matches!(mode, "Abilities" | "Velocity");
                match name {
                    "Speed" | "Anti kick mode" => old,
                    "Delay" | "Off time" => old && self.p("Anti kick mode") != "None",
                    "Vertical speed match" | "No sneak" => mode == "Velocity",
                    "Burst speed" | "Burst friction" | "Burst repeat" => mode == "NCP Burst",
                    "Bounce period (ticks)" | "Bounce boost" => mode == "NCP Bounce",
                    "Damage time (ticks)" | "Knockback" | "Damage boost" | "After damage" => self.p("Damage fly") != "Off",
                    "Self damage health" | "Self damage method" => self.p("Damage fly") == "Self damage",
                    _ => true,
                }
            }
            "Inventory Manager" => {
                let slots = self.p("Mode") == "Slots";
                match name {
                    "Armor key" => self.p("Armor mode") == "Key",
                    "Switch sets" => key_from_text(self.t("Switch key")).is_some(),
                    "Mode" | "Only in inventory" | "Protected items" | "Auto Armor" | "Auto Armor key"
                    | "Armor delay (ticks)" | "Armor mode" | "Avoid cursed" | "Switch key" => true,
                    "Delay min (ms)" | "Delay max (ms)" | "Drop custom items" | "Use custom items" | "Prioritize splash potions"
                    | "Block limit" | "Arrow limit" | "Bucket limit" | "Snowball and egg limit" | "Ender pearl limit"
                    | "Sword slot" | "Second sword slot" | "Pickaxe slot" | "Axe slot" | "Shovel slot" | "Block slots from"
                    | "Block slots to" | "Potion slots from" | "Potion slots to" | "Bow slot" | "Rod slot" | "Food slots from"
                    | "Food slots to" => slots,
                    _ if slots => false,
                    "Keep totems" | "Keep pearl stacks" => self.t("Loot key") != "None",
                    "List mode" | "Items" => self.p("Preset") == "Custom",
                    "Blocks" | "Food" | "Arrows" | "Pearls" | "Junk" => self.p("Preset") != "Custom",
                    _ => true,
                }
            }
            // Crystal Aura and Anchor Aura: each section's settings with it on, the shared ones with either.
            "Crystal Aura" => match name {
                "Aura" | "Macro" => true,
                "Hold key" => self.b("Aura") && self.b("Require hold"),
                "Render color" => self.b("Aura") && self.b("Render"),
                "Require hold" | "Min damage" | "Face place" | "Force face place" | "Turn speed" | "View angle" | "Render" => self.b("Aura"),
                "Obsidian delay" => self.b("Macro") && self.b("Auto obsidian"),
                "Perfect timing" => self.b("Macro"),
                _ => self.b("Aura") || self.b("Macro"),
            },
            "Anchor Aura" => match name {
                "Aura" | "Macro" | "Pearl catch" => true,
                "Scan range" | "Catch min damage" | "Catch max self damage" | "Anti suicide" | "Damage tick" | "Safe cover"
                | "Silent aim FOV" | "Catch swap back" | "Shortest place delay (ms)" | "Longest place delay (ms)"
                | "Shortest charge delay (ms)" | "Longest charge delay (ms)" | "Shortest detonate delay (ms)"
                | "Longest detonate delay (ms)" => self.b("Pearl catch"),
                "Safe block" => self.b("Pearl catch") && self.b("Safe cover"),
                "Render color" => self.b("Aura") && self.b("Render"),
                "Min damage" | "Max self damage" | "Render" | "Turn speed" => self.b("Aura"),
                "Mode" | "Delay" | "Safe anchor" | "Switch back" => self.b("Macro"),
                "Wait for hurt time" => self.b("Macro") && self.p("Mode") == "Double",
                "If unguarded" => self.b("Macro") && self.b("Safe anchor"),
                _ => self.b("Aura") || self.b("Macro"),
            },
            "Anvil" => match name {
                "Target range" | "Height" | "Delay" | "Place at feet" | "Multi place" => self.b("Auto Anvil"),
                "Rotate" => self.b("Anti Anvil") || self.b("Self Anvil") || self.b("Auto Anvil"),
                _ => true,
            },
            "Hand View" => {
                let legacy = self.p("Animations") == "1.8";
                match name {
                    "Swing animation" | "Block animation" => !legacy,
                    "1.8 swing" => legacy,
                    "1.8 block" => legacy || self.p("Block animation") == "1.8 style",
                    _ => true,
                }
            }
            "Speed" => match name {
                "Vanilla speed" => !matches!(self.p("Mode"), "Strafe" | "Bhop" | "NCP" | "NCP Hop" | "Watchdog"),
                "NCP reduced jump" => self.p("Mode") == "NCP",
                "NCP hop jump" | "NCP hop ground speed" | "NCP hop slope" | "NCP hop custom boost" | "NCP hop low hop"
                | "NCP hop Y-port" => self.p("Mode") == "NCP Hop",
                "NCP hop boost speed" | "NCP hop hurt ticks" => self.p("Mode") == "NCP Hop" && self.b("NCP hop custom boost"),
                "Watchdog knockback boost" => self.p("Mode") == "Watchdog",
                "Strafe speed" | "Speed limit" => self.p("Mode") == "Strafe",
                "Hop speed" | "Hop gain (%)" | "Hop height" | "Hop slope" | "Air friction (%)" | "Air control" | "Fast fall"
                | "Hop when" | "Stop on release" => self.p("Mode") == "Bhop",
                "Damage boost" => matches!(self.p("Mode"), "Bhop" | "NCP Hop"),
                "Hop max speed" => self.p("Mode") == "Bhop" && self.n("Hop gain (%)") > 0.0,
                "Only on ground" => !matches!(self.p("Mode"), "Bhop" | "NCP" | "NCP Hop" | "Watchdog"),
                "Ground multiplier" => advanced && self.p("Mode") == "Custom",
                "Air multiplier" => advanced && matches!(self.p("Mode"), "Smooth" | "Low Hop" | "Custom") && !self.b("Only on ground"),
                "Acceleration" | "Deceleration" | "Strafe strength" => advanced && matches!(self.p("Mode"), "Smooth" | "Custom"),
                "Jump height" => advanced && self.p("Mode") == "Low Hop",
                "Max speed" | "Sprint multiplier" => advanced && matches!(self.p("Mode"), "Smooth" | "Low Hop" | "Custom"),
                "Knockback pause" | "Stop at wall" => advanced,
                _ => true,
            },
            "Velocity" => match name {
                "Delay chance (%)" | "Air delay min (ms)" | "Air delay max (ms)" | "Ground delay min (ms)"
                | "Ground delay max (ms)" | "Delay not in water" | "Delay only near enemies" => self.b("Knockback delay"),
                "Use air values" => advanced,
                "Air knockback horizontal" | "Air knockback vertical" => advanced && self.b("Use air values"),
                "Blend %" => self.p("Mode") == "Blend",
                _ => true,
            },
            "Kill Aura" => {
                let always = self.p("Rotate") == "Always";
                match name {
                    "Rotation mode" | "Rotation view" => always,
                    "Yaw speed" | "Pitch speed" => always && matches!(self.p("Rotation mode"), "Custom" | "Adaptive"),
                    "Smoothing" => always && self.p("Rotation mode") == "Custom",
                    "Aim speed" => always && self.p("Rotation mode") == "Silent aura",
                    "Rotation range" => self.p("Rotate") != "None",
                    "Aim point" | "Prediction" => always && self.p("Rotation mode") != "Silent aura",
                    "Aim height (%)" => always && self.p("Rotation mode") != "Silent aura" && self.p("Aim point") == "Custom",
                    "Prediction strength" => always && self.p("Rotation mode") != "Silent aura" && self.p("Prediction") != "Off",
                    "Max targets" => self.p("Attack mode") == "Multi",
                    "Target lock" => self.p("Attack mode") == "Single",
                    "Target switch delay" => self.p("Attack mode") != "Multi",
                    // Modern's timing and Legacy's clicks; Auto may run either, so it shows both.
                    "Attack timing" => self.p("Combat version") != "Legacy (1.8)",
                    "Cooldown" => self.p("Combat version") != "Legacy (1.8)" && self.p("Attack timing") != "Custom delay",
                    "Hit delay" => self.p("Combat version") != "Legacy (1.8)" && self.p("Attack timing") == "Custom delay",
                    "Min CPS" | "Max CPS" | "CPS pattern" => self.p("Combat version") != "Modern (1.9+)",
                    "Strafe radius" | "Strafe when" | "Avoid edges" => self.b("Target strafe"),
                    "Selected weapon types" => self.p("Attack when holding") == "Weapons",
                    "Swap back" => self.b("Auto switch"),
                    "Anti bot sensitivity" => self.b("Anti bot"),
                    _ => true,
                }
            }
            "Entity Control" => match name {
                "Horizontal speed" | "Only on ground" | "In water" => self.b("Speed"),
                "Vertical speed" | "Fall speed" | "Anti fly kick" => self.b("Fly"),
                "Delay" => self.b("Fly") && self.b("Anti fly kick"),
                _ => true,
            },
            "Pearl Catch" => match name {
                "Wind charge delay (ms)" => matches!(self.p("Key mode"), "Up" | "Diagonal macro"),
                "Moving delay (ms)" => self.p("Key mode") == "Up",
                "Pearl angle" | "Wind charge angle" => self.p("Key mode") == "Fast",
                "Offhand pearl angle" | "Offhand wind charge angle" => self.p("Key mode") == "Offhand",
                "Max yaw delta" => self.p("Key mode") == "Diagonal",
                _ => true,
            },
            "Tap Use" => match name {
                "Shortest step delay (ms)" | "Longest step delay (ms)" | "Smoothing" => self.p("Throw mode") == "Legit",
                "Wind charge pitch" => !self.b("Wind charge along look"),
                "Shortest hit delay (ms)" | "Longest hit delay (ms)" => self.b("Auto short pearl"),
                "Thrower range" | "Min landing distance" | "Landing range" | "Max error" | "Shortest follow delay (ms)"
                | "Longest follow delay (ms)" | "Follow only on ground" => self.b("Pearl follow"),
                "Pot type" | "Pot amount" | "Random order" | "Throw bowls" => key_from_text(self.t("Pot key")).is_some(),
                "Debuff mode" | "Debuffs" => key_from_text(self.t("Debuff key")).is_some(),
                "Shortest pot delay (ms)" | "Longest pot delay (ms)" => {
                    key_from_text(self.t("Pot key")).is_some() || key_from_text(self.t("Debuff key")).is_some()
                }
                _ => true,
            },
            "Wind Jump" => match name {
                "Hit reach" | "Max yaw" | "In air only" | "Humanized aim" => self.b("Hit charges"),
                _ => true,
            },
            "Fast Use" => match name {
                "Throw mode" | "Experience bottles" | "Ender pearls" | "Wind charges" | "Snowballs and eggs" | "Splash potions"
                | "Off hand" => self.b("Throwables"),
                "Throw delay" => self.b("Throwables") && self.p("Throw mode") == "Delay",
                "Burst" => self.b("Throwables") && self.p("Throw mode") == "Burst",
                _ => true,
            },
            "Offhand" => {
                let totem = self.p("Item") == "Totem" || self.b("Totem on low health");
                let (open, silent) = (self.b("Open inventory"), self.b("Silent open"));
                match name {
                    "Totem health" | "Wait while shielding" => self.b("Totem on low health"),
                    "Sword gapple" => self.b("Right gapple"),
                    "Open inventory" | "Inventory only" | "Random slot" | "Shortest totem delay (ms)" | "Longest totem delay (ms)"
                    | "Extra randomization" | "Show totem count" => totem,
                    "Silent open" => totem && open,
                    "Shortest silent move delay (ms)" | "Longest silent move delay (ms)" => totem && open && silent,
                    "Close inventory" => totem && open && !silent,
                    "Hotbar refill" => self.b("Hover"),
                    "Double hand on" | "Double hand sensitivity" | "Reswap delay (ticks)" => self.b("Double hand"),
                    "Hotbar slot" => self.b("Hover") && self.b("Hotbar refill"),
                    _ => true,
                }
            }
            "Hitboxes" if name == "Weapons" => self.b("Only on weapon"),
            "Auto Web" => match name {
                "Max webs per fall" => self.b("Only when falling"),
                "Ticks to predict" => self.b("Predict movement") && !self.b("Only when falling"),
                _ => true,
            },
            "Shield" => match name {
                "Target distance" | "FOV" | "Movement threshold" | "Sword cooldown correction (%)" | "Disable tracker" => self.b("Auto block"),
                "Swap to sword" => self.b("Auto block") && self.b("Disable tracker"),
                "Block hit min chance (%)" | "Block hit max chance (%)" | "Block hit ticks" => self.p("Block hit") == "Manual",
                "Max hurt time (ms)" | "Include ping" | "Hold after" => self.p("Block hit") == "Predict",
                "Require mouse down" => self.p("Block hit") != "Off",
                _ => true,
            },
            "Fake Lag" => super::sunset_lag::setting_visible(self, name),
            "Sprint" if name.ends_with("restore delay") => self.b("Keep sprint"),
            "Aim Assist" => match name {
                "Silent players only" | "Silent weapon only" | "Silent max yaw" | "Silent max pitch" => self.b("Silent aim"),
                "Aim points" => self.b("Silent aim") || self.b("Hit flick"),
                "Flick players only" | "Flick needs sprint" | "Flick min angle" | "Flick max angle" | "Flick direction" => self.b("Hit flick"),
                _ => true,
            },
            "Liquid Filler" => match name {
                "Drain aim" | "Only by cobwebs" | "Drain swap" | "Swap back" | "Fallback to cobweb" => self.p("Mode") == "Drain",
                "Silent aim FOV" => self.p("Mode") == "Drain" && self.p("Drain aim") == "Silent aim",
                "Shape" | "Place range" | "Walls range" | "Max blocks per tick" | "Sort" | "List mode" | "Whitelist" | "Blacklist" => {
                    self.p("Mode") == "Fill"
                }
                _ => true,
            },
            "No Interact" => match name {
                "Hit fix weapons only" | "Hit fix hurt time" | "Hit fix on ground" => self.b("Hit fix"),
                "Hit select chance (%)" => self.p("Hit select") != "Off",
                "Hit select favours" => self.p("Hit select") == "Sprint reset",
                "Humanize (%)" | "In combat only" => self.b("No miss delay"),
                "Combat range" => self.b("No miss delay") && self.b("In combat only"),
                "Double glowstone" | "Open ender chest" | "Place glowstone" | "Ground firework" => self.b("Anti action"),
                "Cobweb only" | "Use offhand" => self.b("Ghost place"),
                "Hide armor stands" | "Hide minecarts" | "Hide item frames" | "Hide boats" | "Hide end crystals" | "Trap range"
                | "Drop spawns" | "Remove loaded" | "Anti sign" => self.b("Anti trap"),
                _ => true,
            },
            "Chest Swap" => match name {
                "Mode" => true,
                "Fly time (ms)" => self.p("Mode") == "Take off" && self.b("Auto disable"),
                "Shortest space cooldown (ms)" | "Longest space cooldown (ms)" => self.p("Mode") == "Take off" && self.b("Firework on space"),
                _ => self.p("Mode") == "Take off",
            },
            "Safe Walk" => match name {
                "Min pitch" | "Only holding blocks" => self.p("Mode") == "Sneak",
                _ => true,
            },
            "Rotations" => match name {
                "Yaw lock" | "Pitch lock" => self.b("Lock"),
                "Yaw angle" => self.b("Lock") && self.p("Yaw lock") == "Simple",
                "Pitch angle" => self.b("Lock") && self.p("Pitch lock") == "Simple",
                _ => true,
            },
            "Step" => match name {
                "Height" | "Active when" | "Safe step" => self.b("Step up"),
                "Step health" => self.b("Step up") && self.b("Safe step"),
                "Fall speed" => self.b("Reverse step"),
                _ => true,
            },
            "Base Finder" => super::sunset_base_finder::visible(self, name),
            super::sunset_light_scan::GEODE | super::sunset_light_scan::LIGHT => super::sunset_light_scan::visible(self, name),
            super::sunset_sound_detector::NAME => super::sunset_sound_detector::visible(self, name),
            "Anti Vanish" => super::sunset_vanish::visible(self, name),
            "Effects" => super::sunset_effects::visible(self, name),
            super::sunset_pvp_bot::NAME => super::sunset_pvp_bot::visible(self, name),
            "Atmosphere" => super::sunset_atmosphere::visible(self, name),
            "ESP" => match name {
                "Villager filter" | "Villager babies" | "Villager profession colors" | "Villager level label" =>
                    self.b("Passive") || super::sunset_util::listed(self.t("Entities"), "villager"),
                "2D style" | "Health bar" | "2D names" | "2D distance" | "Glow" => self.p("Mode") == "2D box",
                "Health side" | "Health text" => self.p("Mode") == "2D box" && self.b("Health bar"),
                "Shield mode" | "Shield color" | "Broken shield color" => self.b("Shield colors"),
                "Murderer items" | "Murderer color" | "Murderer list" | "Callout" => self.b("Murderer finder"),
                "Murderer list position" => self.b("Murderer finder") && self.b("Murderer list"),
                "Murderer list x" | "Murderer list y" => {
                    self.b("Murderer finder") && self.b("Murderer list") && self.p("Murderer list position") == "Custom"
                }
                "Callout delay (ms)" | "Callout messages" => self.b("Murderer finder") && self.b("Callout"),
                _ => true,
            },
            "Nametags" => match name {
                "Health colors" => self.b("Health"),
                _ => true,
            },
            "Cart" => super::sunset_cart::visible(self, name),
            super::sunset_gear::NAME => super::sunset_gear::visible(self, name),
            "Auto Mace" => super::sunset_auto_mace::visible(self, name),
            "ClickGUI" => match name {
                "Inventory blur" => self.b("Menu dim"),
                _ => true,
            },
            // Elytra Fly: each mode's own settings.
            "Elytra Fly" => match name {
                "Horizontal speed" | "Vertical speed" | "Fall multiplier" => self.p("Mode") == "Control",
                "Boost" | "Max speed" | "Boost only on firework" => self.p("Mode") == "Boost",
                _ => true,
            },
            "Auto Weapon" => match name {
                "Best weapon" | "Axe on shields" => self.p("Mode") == "Custom",
                "Prefer axes" => self.p("Mode") == "Custom" && self.b("Best weapon"),
                "Totem hit" => self.weapon("Best weapon"),
                "Shield mode" | "Shield held" => self.weapon("Axe on shields"),
                "Back after" => self.b("Swap back"),
                "Shortest delay" | "Longest delay" => self.b("Swap back") && self.p("Back after") == "A delay",
                _ => true,
            },
            "Radar" => !matches!(name, "Radar x" | "Radar y") || self.p("Radar position") == "Custom",
            "Auto Spear" => match name {
                "Reach chance (%)" | "Reach weapons" | "Works with Trigger Bot" => self.b("Extended reach"),
                "Damage indicator position" => self.b("Damage indicator"),
                "Damage indicator x" | "Damage indicator y" => self.b("Damage indicator") && self.p("Damage indicator position") == "Custom",
                "Back after" => self.b("Swap back"),
                "Shortest delay" | "Longest delay" => self.b("Swap back") && self.p("Back after") == "A delay",
                _ => true,
            },
            "GUI Move" => match name {
                "Rotate speed" => self.b("Arrows"),
                _ => true,
            },
            // W Tap's S Tap: its own settings in that mode.
            "W Tap" => match name {
                "Adaptive" => self.p("Mode") == "S Tap",
                "Spacing" => self.p("Mode") == "S Tap" && self.b("Adaptive"),
                _ => true,
            },
            "Scaffold" => match name {
                "Block counter position" => self.b("Block counter"),
                "Block counter x" | "Block counter y" => self.b("Block counter") && self.p("Block counter position") == "Custom",
                "Telly rotations" | "Start speed min" | "Start speed max" | "Normal speed min" | "Normal speed max"
                | "Keep Y" | "Tower" | "Telly sprint" | "Ground motion (%)" | "Air motion (%)" | "Speed motion (%)"
                | "Multi place" | "Swing" | "Reject wait (ticks)" => self.p("Mode") == "Telly",
                "Keep Y on press" => self.p("Mode") == "Telly" && self.p("Keep Y") == "Telly",
                "Shortest sneak delay (ms)" | "Longest sneak delay (ms)" => self.p("Mode") == "Legit",
                "Activation blocks" => self.p("Mode") == "Godbridge",
                "Auto switch" => self.p("Mode") != "Legit",
                "Swap back" => matches!(self.p("Mode"), "Normal" | "Telly") && self.b("Auto switch"),
                "Delay (ticks)" => matches!(self.p("Mode"), "Normal" | "Telly"),
                "Rotations" | "Look" | "Rotation speed" | "Min rotation speed" | "Ray cast" => self.p("Mode") == "Normal" && self.b("Rotate"),
                "Telly ticks" => self.p("Mode") == "Normal" && self.b("Rotate") && self.p("Rotations") == "Telly",
                "Movement correction" => self.p("Mode") == "Normal" && self.b("Rotate") && self.p("Look") == "Silent",
                "Late click" => self.p("Mode") == "Normal" && self.b("Rotate") && self.p("Ray cast") == "Strict",
                "Air place" => self.p("Mode") == "Normal" && (!self.b("Rotate") || self.p("Ray cast") == "Off"),
                "Render" => matches!(self.p("Mode"), "Normal" | "Telly"),
                "Fast tower" | "Only on click" | "Rotate" | "Sprint" | "Same Y" | "Expand" | "Ledge sneak"
                | "Advanced" => self.p("Mode") == "Normal",
                "Tower speed" | "While moving" => self.p("Mode") == "Normal" && self.b("Fast tower"),
                "Shape mode" | "Fill opacity" | "Color" => matches!(self.p("Mode"), "Normal" | "Telly") && self.b("Render"),
                "Safe walk" | "Max placement distance" | "Closest block range" | "Max per second" => {
                    self.p("Mode") == "Normal" && advanced
                }
                _ => true,
            },
            "Server Scan" => match name {
                "Query" | "Command suggestions" => self.p("Mode") == "Extended",
                _ => true,
            },
            _ => true,
        }
    }
}

// Base Finder's block list before its tiers (0.8.1), told apart from a list of your own.
pub(super) const OLD_BASE_BLOCKS: &str = "chest,trapped_chest,barrel,ender_chest,shulker_box,*_shulker_box,crafting_table,furnace,blast_furnace,smoker,*_bed,*_sign,anvil,enchanting_table,brewing_stand,hopper,dropper,dispenser,beacon,respawn_anchor,lodestone,*_concrete,nether_portal";
