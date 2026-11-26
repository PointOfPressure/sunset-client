// Runtime part (see runtime.rs): the client's state, its lifecycle (start, shutdown) and saving.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset_bind_state::Inputs;
use super::sunset_util::Sweep;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::KeyCode;
use ::sunset_core::lifecycle::RUNTIME;
use super::sunset::*;

// ---------------------------------------------------------------- state

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Page {
    Main,
    Category(Category),
    Module(usize),
}

pub(super) struct State {
    pub(crate) modules: Vec<Module>,
    // The modules as last handed out by snapshot(): shared again while nothing changed.
    pub(super) shared: Arc<Vec<Module>>,
    pub(super) page: Page,
    // A module waiting for its key bind (its index).
    pub(super) binding: Option<usize>,
    // Keys and buttons down, and the modules Hold binds have on.
    pub(super) inputs: Inputs,
    // Each preset choice as last seen, by module name: fill_picked_presets.
    pub(super) presets_seen: Vec<(&'static str, &'static str)>,
    pub(super) friends: Vec<String>,
    // The config file as read: modules a plugin registers later take their saved settings from it.
    raw: String,
    // Plugins' modules as they registered them: what .reset and a slider's right click go back to.
    plugin_defaults: Vec<Module>,
    pub(super) selection: super::sunset_profiles::Selection,
    // Keys this mod held down for the tick (Auto Walk, Sprint).
    pub(super) injected: Vec<KeyCode>,
    pub(super) sprint_owned: bool,
    pub(super) loaded: bool,
    pub(super) ticks: u64,
    pub(super) trigger_wait: u32,
    // Trigger Bot: the share of the attack cooldown the next hit waits for (rolled between Shortest and Longest
    // cooldown after each), the target Miss chance passed over until the crosshair leaves it, and the Sticky
    // target.
    pub(super) trigger_need: f32,
    pub(super) trigger_missed: Option<super::sunset_antibot::Token>,
    pub(super) trigger_sticky: Option<super::sunset_antibot::Token>,
    pub(super) respawn_wait: u32,
    // Death Location: the death screen was up last tick.
    pub(super) dead: bool,
    // Flight's Abilities mode is holding the player up.
    pub(super) flying: bool,
    // The Step modifier is on the player.
    pub(super) stepping: bool,
    pub(super) high_jumping: bool,
    // The player's velocity when the last tick ended: what changes it before
    // the next one is the server's push (Velocity).
    pub(super) velocity: Option<[f64; 3]>,
    pub(super) strafe: Strafe,
    pub(super) bhop: Bhop,
    pub(super) ncp: super::sunset_legacy_speed::Ncp,
    // NCP Hop: its speed, the ticks since you left the ground, and the last move it made.
    pub(super) ncp_hop: super::sunset_legacy_speed::NcpHop,
    pub(super) hop_air: u32,
    pub(super) hop_last: [f64; 2],
    pub(super) watchdog: super::sunset_legacy_speed::Watchdog,
    pub(super) legacy_recovery: super::sunset_legacy_speed::Recovery,
    pub(super) knockback_ticks: u32,
    // Flight: Sunset's anti kick counters, the yaw it keeps, which way it nudged last, and whether it let go of sneak.
    pub(super) fly_delay: i32,
    // Flight's ticks still to hold after a server setback.
    pub(super) fly_setback: u32,
    pub(super) fly_off: i32,
    pub(super) fly_last_yaw: f32,
    pub(super) fly_flip: bool,
    pub(super) unsneaked: bool,
    // What explosions pushed the player by since the last tick.
    pub(super) blast: [f64; 3],
    pub(super) gui: Gui,
    pub(super) crumbs: Vec<[f64; 3]>,
    pub(super) waypoints: Vec<Waypoint>,
    // Loadouts, as saved (sunset_loadouts reads them).
    pub(super) loadouts: Vec<String>,
    // Block ESP's finds: where, and which colour.
    pub(super) blocks: Sweep<u32>,
    // Your view before Scaffold turned it (Normal's Look Visible), put back when Scaffold goes off.
    pub(super) scaffold_rotation: Option<(f32, f32)>,
    // The hotbar slot Scaffold took as the biggest stack, kept until it runs out.
    pub(super) scaffold_stack: Option<usize>,
    // Scaffold's placed blocks, drawn for 8 ticks as they fade.
    pub(super) scaffold_placed: Vec<([i32; 3], u8)>,
    // Legit Scaffold holds sneak (not you).
    pub(super) scaffold_sneak: bool,
    pub(super) blink_ticks: u32,
    pub(super) eating: bool,
    // Auto Eat: the slot it eats from (None: the off hand), the one held before, and Kill Aura turned off meanwhile.
    pub(super) eat_slot: Option<usize>,
    pub(super) eat_prev: usize,
    pub(super) eat_paused_aura: bool,
    // Auto Tool's switch waiting out Switch delay: the slot and the ticks left.
    pub(super) tool_wait: Option<(usize, u32)>,
    pub(super) was_ground: bool,
    pub(super) crumb_at: u64,
    // Criticals: who the waiting hit is for, the ticks it has waited, the ticks you have fallen meanwhile, and the
    // look it is made with.
    pub(super) crit_target: Option<AttackGuard>,
    pub(super) crit_ticks: u32,
    pub(super) crit_falls: u32,
    // The hit started again from the ground once already (it does so once).
    pub(super) crit_again: bool,
    pub(super) crit_look: Option<(f32, f32)>,
    pub(super) notify: Option<Vec<String>>,
    // Hole ESP's finds: where, and what walls them (0 bedrock, 1 obsidian, 2 both).
    pub(super) holes: Sweep<u8>,
    pub(super) tool_back: Option<usize>,
    // Auto Tool's From inventory: the main inventory slot a tool was swapped in from, and the hotbar slot it went to.
    pub(super) tool_pulled: Option<(usize, usize)>,
    // Middle Click Extra: the hotbar slot and item a click asked for (used at the start of the next tick), and the
    // slot to go back to with the ticks left.
    pub(super) mc_use: Option<(usize, &'static str)>,
    pub(super) mc_back: Option<(usize, u32)>,
    pub(super) nuke: Option<([i32; 3], u32)>,
    // Blocks Nuker just stopped on and the tick to forget them at.
    pub(super) nuke_skip: Vec<([i32; 3], u64)>,
    // Nuker (Sunset's): its delay, the first-block flag, the last block, the ticks with nothing to break, the blocks
    // it right-clicked (Interact), its Cube box and the blocks it broke (drawn for 8 ticks).
    pub(super) nuke_timer: u32,
    pub(super) nuke_first: bool,
    pub(super) nuke_last: Option<[i32; 3]>,
    pub(super) nuke_idle: u32,
    pub(super) nuke_interacted: Vec<[i32; 3]>,
    pub(super) nuke_box: Option<[[f64; 3]; 2]>,
    pub(super) nuke_broken: Vec<([i32; 3], u8)>,
    pub(super) armor_wait: u32,
    // Auto Armor's Switch key: the set it went to (worn until the key goes to the next).
    pub(super) armor_set: Option<String>,
    pub(super) replenish_wait: u32,
    pub(super) last_pos: Vec<(String, [f64; 3], String)>,
    pub(super) spots: Vec<Spot>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Spot {
    pub(super) name: String,
    pub(super) pos: [f64; 3],
    pub(super) dim: String,
    pub(super) at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Waypoint {
    pub(super) name: String,
    pub(super) pos: [f64; 3],
    pub(super) dim: String,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            modules: defaults(),
            shared: Arc::default(),
            page: Page::Main,
            binding: None,
            inputs: Inputs::default(),
            presets_seen: Vec::new(),
            friends: Vec::new(),
            raw: String::new(),
            plugin_defaults: Vec::new(),
            selection: super::sunset_profiles::Selection::default(),
            injected: Vec::new(),
            sprint_owned: false,
            loaded: false,
            ticks: 0,
                    trigger_wait: 0,
                    trigger_need: 1.0,
                    trigger_missed: None,
                    trigger_sticky: None,
            respawn_wait: 0,
            dead: false,
            flying: false,
            stepping: false,
            high_jumping: false,
                    velocity: None,
                    strafe: Strafe::default(),
                    bhop: Bhop::default(),
                    ncp: super::sunset_legacy_speed::Ncp::default(),
                    ncp_hop: super::sunset_legacy_speed::NcpHop::default(),
                    hop_air: 0,
                    hop_last: [0.0; 2],
                    watchdog: super::sunset_legacy_speed::Watchdog::default(),
                    legacy_recovery: super::sunset_legacy_speed::Recovery::default(),
            knockback_ticks: 0,
                    fly_delay: 0,
                    fly_setback: 0,
                    fly_off: 0,
                    fly_last_yaw: 0.0,
                    fly_flip: false,
                    unsneaked: false,
                    blast: [0.0; 3],
            gui: Gui::default(),
            crumbs: Vec::new(),
            waypoints: Vec::new(),
            loadouts: Vec::new(),
            blocks: Sweep::new(),
            scaffold_rotation: None,
            scaffold_stack: None,
                    scaffold_placed: Vec::new(),
            scaffold_sneak: false,
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
            crit_again: false,
            crit_look: None,
            notify: None,
            holes: Sweep::new(),
            tool_back: None,
            tool_pulled: None,
            mc_use: None,
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
            armor_set: None,
            replenish_wait: 0,
            last_pos: Vec::new(),
            spots: Vec::new(),
        }
    }
    // The friends list, kept in ClickGUI's Friends setting too (where the menu's picker edits it).
    pub(super) fn set_friends(&mut self, list: Vec<String>) {
        if let Some(i) = self.find("ClickGUI")
            && let Some(set) = self.modules[i].settings.iter_mut().find(|s| s.name == "Friends")
        {
            set.value = Value::Text(list.join(","));
        }
        self.friends = list;
    }
    // A player in or out of the friends list; whether they are a friend now.
    pub(super) fn flip_friend(&mut self, name: &str) -> bool {
        let mut list = self.friends.clone();
        let was = list.iter().any(|f| f.eq_ignore_ascii_case(name));
        list.retain(|f| !f.eq_ignore_ascii_case(name));
        if !was {
            list.push(name.to_owned());
        }
        self.set_friends(list);
        !was
    }
    // A module by its name, or by a name it had before (`.t autosteal` is Chest Stealer).
    pub(super) fn find(&self, name: &str) -> Option<usize> {
        let n = norm(name);
        let n = RENAMED.iter().find(|(from, _)| *from == n).map_or(n.as_str(), |(_, to)| to);
        // Blink is Fake Lag's Hold mode (.t blink sets the mode too: runtime_commands.rs).
        let n = if n == "blink" { "fakelag" } else { n };
        self.modules.iter().position(|m| norm(m.name) == n)
    }
    pub(super) fn save_text(&self) -> String {
        let mut out = self.module_text();
        out.push_str(&format!("{MENU_HIDDEN}={MENU_HIDDEN_LIST}\n"));
        out.push_str(&format!("profile-selection={}\n", self.selection.stored()));
        out.push_str(&format!("friends={}\n", self.friends.join(",")));
        for w in &self.waypoints {
            out.push_str(&format!("waypoint={}|{}|{}|{}|{}\n", w.name, w.pos[0], w.pos[1], w.pos[2], w.dim));
        }
        for (c, p) in CATEGORIES.iter().zip(&self.gui.panels) {
            // Its place of your own, not where a small window happens to draw it.
            out.push_str(&format!("panel.{}={},{},{}\n", norm(c.label()), p.home[0], p.home[1], p.collapsed));
        }
        out
    }
    pub(super) fn module_text(&self) -> String {
        let mut out = String::new();
        for (i, m) in self.modules.iter().enumerate() {
            let id = norm(m.name);
            let on = super::sunset_pvp_bot::saved_on(m.name, self.inputs.saved_on(i, m.on));
            out.push_str(&format!("{id}.on={on}\n"));
            out.push_str(&format!("{id}.key={}\n", m.key.as_deref().unwrap_or("none")));
            for s in &m.settings {
                out.push_str(&format!("{id}.{}={}\n", norm(s.name), kept_as(&id, s)));
            }
        }
        out
    }
    pub(super) fn load_text(&mut self, text: &str) {
        self.absent_baritone_defaults(text);
        self.waypoints.clear();
        self.loadouts.clear();
        self.selection = super::sunset_profiles::Selection::default();
        let merged = merged_modules(text);
        let text = hidden_since_1_3(&merged);
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            if k == MENU_HIDDEN {
                continue;
            }
            if k == "profile-selection" {
                self.selection.restore(v);
                continue;
            }
            if let Some(cat) = k.strip_prefix("panel.") {
                // The Client panel was Extras before 0.9.
                let cat = if cat == "extras" { "client" } else { cat };
                let parts: Vec<&str> = v.split(',').collect();
                if let (Some(i), [x, y, c]) = (CATEGORIES.iter().position(|c| norm(c.label()) == cat), parts.as_slice())
                    && let (Ok(x), Ok(y)) = (x.parse(), y.parse())
                {
                    self.gui.panels[i] = Panel { x, y, home: [x, y], collapsed: *c == "true", scroll: 0.0 };
                }
                continue;
            }
            if k == "loadout" {
                self.loadouts.push(v.to_owned());
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
                let list = v.split(',').map(str::trim).filter(|f| !f.is_empty()).map(str::to_owned).collect();
                self.set_friends(list);
                continue;
            }
            Self::set_field(&mut self.modules, k, v);
        }
        // A config saved with the menu open (as every close saves, or a crash) doesn't open it: ClickGUI follows it.
        mirror_gui(self);
        self.see_presets();
        let snapshot = self.profile_text();
        self.selection.refresh(&snapshot);
        self.selection.fresh = fresh_settings(&snapshot);
    }
    // One "module.field=value" line of a config or a profile, on `modules`.
    pub(super) fn set_field(modules: &mut [Module], k: &str, v: &str) {
        let Some((id, field)) = k.split_once('.') else {
            return;
        };
        let Some(m) = modules.iter_mut().find(|m| norm(m.name) == id) else {
            return;
        };
        match field {
            "on" => m.on = v == "true",
            "key" => m.key = key_from_text(v),
            _ => {
                if id == "killaura" && field == "shield" {
                    let parsed = m.settings.iter_mut().find(|s| s.name == "Shield").is_some_and(|s| s.parse(v));
                    if parsed {
                        let on = m.b("Shield").to_string();
                        if let Some(s) = m.settings.iter_mut().find(|s| s.name == "Shield block") { s.parse(&on); }
                    }
                    return;
                }
                // Kill Aura's AutoBlock before the Version split: Legacy was the sword block (now Packet), Modern the
                // shield (now Shield block). Such a config decides the shield by AutoBlock alone; a newer one's own
                // Shield block comes after this line and has the last word.
                if id == "killaura" && field == "autoblock" {
                    let (block, shield) = match v {
                        "Legacy" => ("Packet", Some(false)),
                        "Modern" => ("Off", Some(true)),
                        "Off" => ("Off", Some(false)),
                        other => (other, None),
                    };
                    if let Some(s) = m.settings.iter_mut().find(|s| s.name == "AutoBlock") { s.parse(block); }
                    if let Some(on) = shield
                        && let Some(s) = m.settings.iter_mut().find(|s| s.name == "Shield block")
                    {
                        s.parse(&on.to_string());
                    }
                    return;
                }
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
                    // Kill Aura before its Attack timing, and when Switch delay was its only one.
                    ("killaura", "customdelay") => Some(("attacktiming", if v == "true" { "Custom delay" } else { "Vanilla cooldown" }.to_owned())),
                    ("killaura", "switchdelay") => Some(("weaponswitchdelay", v.to_owned())),
                    // Kill Aura's Combat version before Version: Auto (which mostly ran 1.9+) becomes 1.9+.
                    ("killaura", "combatversion") => Some(("version", if v == "Legacy (1.8)" { "1.8" } else { "1.9+" }.to_owned())),
                    // Range was 64 by default before All (0): an unchanged one becomes All.
                    ("esp" | "tracers" | "nametags", "range") if v == "64" => Some(("range", "0".to_owned())),
                    // Offhand before Sunset's Auto Totem: Refill Inventory opened the inventory for the totem; Off and
                    // Hotbar Swap moved it with no screen.
                    ("offhand", "refill") => Some(("silentopen", (v != "Inventory").to_string())),
                    // Base Finder's one list before its tiers: a list of your own becomes List 7.
                    ("basefinder", "blocks") if v != OLD_BASE_BLOCKS => Some(("list7", v.to_owned())),
                    ("basefinder", "threshold") => Some(("list7count", v.to_owned())),
                    _ => None,
                };
                // ...its Swap delay (ticks) is the Totem delay, and Item None (a totem only at Health or below) a totem
                // always: Health is gone, and Item comes before it in a saved config.
                if id == "offhand" && field == "swapdelay" && let Ok(ticks) = v.parse::<f64>() {
                    let ms = (ticks * 50.0).clamp(50.0, 200.0).to_string();
                    for target in ["shortesttotemdelayms", "longesttotemdelayms"] {
                        if let Some(s) = m.settings.iter_mut().find(|s| norm(s.name) == target) { s.parse(&ms); }
                    }
                    return;
                }
                if id == "offhand" && field == "health" && m.p("Item") == "None"
                    && let Some(s) = m.settings.iter_mut().find(|s| s.name == "Item")
                {
                    s.parse("Totem");
                }
                // Before Attack mode, more than one target meant hitting them all. A newer config's own Attack mode
                // comes after this line and has the last word.
                if id == "killaura" && field == "maxtargets" && v.parse::<f64>().is_ok_and(|n| n > 1.0)
                    && let Some(s) = m.settings.iter_mut().find(|s| norm(s.name) == "attackmode")
                {
                    s.parse("Multi");
                }
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
    pub(super) fn profile_text(&self) -> String {
        let text = self.module_text();
        let kept: Vec<&str> = text.lines().filter(|l| l.split_once('=').is_none_or(|(k, _)| !not_in_profiles(k))).collect();
        kept.join("\n")
    }
    // The profile in use is `name`, as the modules are now.
    pub(super) fn select(&mut self, name: &str) {
        let snapshot = self.profile_text();
        self.selection.choose(name, &snapshot);
    }
    // Migration for configs saved before selection metadata existed. Only a
    // unique, valid match may claim a name; a partial profile compares its fields.
    fn matches_profile(&self, body: &str) -> bool {
        !body.trim().is_empty() && merged_modules(body).lines().filter(|l| !l.trim().is_empty()).all(|line| {
            let Some((key, value)) = line.split_once('=') else {
                return false;
            };
            let Some((id, field)) = key.split_once('.') else {
                return false;
            };
            let Some((i, module)) = self.modules.iter().enumerate().find(|(_, m)| norm(m.name) == id) else {
                return false;
            };
            match field {
                "on" => self.inputs.saved_on(i, module.on) == (value.trim() == "true"),
                "key" => module.key == key_from_text(value.trim()),
                _ => module.settings.iter().find(|s| norm(s.name) == field).is_some_and(|s| {
                    let mut parsed = s.clone();
                    parsed.parse(value.trim()) && parsed.stored() == s.stored()
                }),
            }
        })
    }
    pub(super) fn load_profile(&mut self, text: &str) {
        self.absent_baritone_defaults(text);
        for line in merged_modules(text).lines() {
            if let Some((k, v)) = line.split_once('=')
                && !not_in_profiles(k)
            {
                Self::set_field(&mut self.modules, k, v.trim());
            }
        }
        mirror_gui(self);
        self.see_presets();
    }
    // A load (the config, a profile or an imported one, a Sunset preset) brings each preset choice with the settings
    // saved beside it: what it loaded counts as seen, so nothing is filled in over the loaded settings.
    pub(super) fn see_presets(&mut self) {
        self.presets_seen = PRESET_CHOICES.iter().filter_map(|&(name, choice)| Some((name, self.module(name)?.p(choice)))).collect();
    }
    // A preset picked since the last look (in the menu, with .set, by a plugin), the module on or off: its settings
    // filled in, once; a change made after that stays. Whether anything was filled in.
    pub(super) fn fill_picked_presets(&mut self) -> bool {
        let mut filled = false;
        for &(name, choice) in &PRESET_CHOICES {
            let Some(now) = self.module(name).map(|m| m.p(choice)) else {
                continue;
            };
            let before = match self.presets_seen.iter_mut().find(|seen| seen.0 == name) {
                Some(seen) => std::mem::replace(&mut seen.1, now),
                None => {
                    self.presets_seen.push((name, now));
                    continue;
                }
            };
            if before == now {
                continue;
            }
            let id = norm(name);
            for (field, value) in preset_fields(&id, now) {
                Self::set_field(&mut self.modules, &format!("{id}.{field}"), &value);
                filled = true;
            }
        }
        filled
    }
    // Profiles predating Baritone cannot arm it or carry new preferences across a legacy load.
    fn absent_baritone_defaults(&mut self, text: &str) {
        if !text.lines().filter_map(|line| line.split_once('=')).any(|(key, _)| key.starts_with("baritone.")) {
            if let Some(module) = self.modules.iter_mut().find(|m| m.name == "Baritone") {
                *module = super::sunset_baritone::defaults();
            }
        }
    }
    // A preset's lines as it is taken: every module it neither turns on nor leaves alone goes off first.
    pub(super) fn preset_text(&self, preset: &super::sunset_presets::Preset) -> Result<String, String> {
        let checked = self.check_profile(preset.text)?;
        let mut out = String::new();
        for m in self.modules.iter().filter(|m| m.on && !super::sunset_presets::untouched(m)) {
            out.push_str(&format!("{}.on=false\n", norm(m.name)));
        }
        out.push_str(&checked.body);
        Ok(out)
    }
    // What taking a preset does: the modules it turns on, those it turns off, and how many settings it changes.
    pub(super) fn preview(&self, preset: &super::sunset_presets::Preset) -> Result<Preview, String> {
        let text = self.preset_text(preset)?;
        let mut after = self.modules.clone();
        for line in merged_modules(&text).lines() {
            if let Some((k, v)) = line.split_once('=') {
                Self::set_field(&mut after, k, v.trim());
            }
        }
        let mut preview = Preview::default();
        for (was, now) in self.modules.iter().zip(&after) {
            match (was.on, now.on) {
                (false, true) => preview.on.push(now.name),
                (true, false) => preview.off.push(now.name),
                _ => {}
            }
            preview.settings += was.settings.iter().zip(&now.settings).filter(|(a, b)| a.stored() != b.stored()).count();
        }
        Ok(preview)
    }
    // A profile file read whole before anything changes: its header, then its lines. Lines this version doesn't know
    // (a module or setting since removed or renamed, a value that doesn't fit, a hand edit gone wrong) are left out
    // and named rather than refusing the profile: the rest of it still loads. Your friends and Panic are never a
    // profile's. The lines kept (the header off, as this version reads them) and the ones left out.
    pub(super) fn check_profile(&self, text: &str) -> Result<Checked, String> {
        // A file saved on Windows by hand has \r\n lines and may start with a byte order mark.
        let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
        let body = text.strip_prefix(super::sunset_profiles::HEADER).and_then(|t| t.strip_prefix('\n')).ok_or("not a Sunset profile")?;
        let mut checked = Checked::default();
        for line in merged_modules(body).lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let mut skip = |why: String| checked.skipped.push(why);
            let Some((k, v)) = line.split_once('=') else {
                skip(format!("{} (no =)", line.chars().take(40).collect::<String>()));
                continue;
            };
            if not_in_profiles(k) {
                continue;
            }
            let Some((id, field)) = k.split_once('.') else {
                skip(format!("{k} (no module)"));
                continue;
            };
            let Some(m) = self.modules.iter().find(|m| norm(m.name) == id) else {
                skip(format!("{k} (no module called {id})"));
                continue;
            };
            let fits = match field {
                "on" => matches!(v.trim(), "true" | "false"),
                "key" => !v.chars().any(char::is_control),
                _ => (id == "offhand" && OFFHAND_RETIRED.contains(&field))
                    || m.settings.iter().find(|s| norm(s.name) == field).is_some_and(|s| s.clone().parse(v)),
            };
            if fits {
                checked.body.push_str(line);
                checked.body.push('\n');
            } else {
                skip(format!("{k}={}", v.chars().take(40).collect::<String>()));
            }
        }
        if checked.body.is_empty() && !checked.skipped.is_empty() {
            // The line (or the first of them) is named, as a load that leaves lines out names it: which one to mend.
            return Err(match checked.skipped.as_slice() {
                [one] => format!("nothing in it this version of Sunset knows: {one}"),
                all => format!("nothing in it this version of Sunset knows ({} lines left out, the first {})", all.len(), all[0]),
            });
        }
        Ok(checked)
    }
    pub(super) fn module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }
    // The menus' modules: all but the ones switched off in Modules.
    pub(super) fn listed(&self) -> impl Fn(&Module) -> bool + use<> {
        let hidden = super::sunset_extra_modules::hidden(&self.modules);
        move |m| !hidden.contains(&m.name)
    }
    // The ClickGUI's modules: the menus' less the menu-only ones.
    pub(super) fn in_gui(&self) -> impl Fn(&Module) -> bool + use<> {
        let listed = self.listed();
        move |m| m.in_gui && listed(m)
    }
    // Blink is Fake Lag's Hold mode now: the modules that step aside for it still ask by its old name.
    pub(super) fn is_on(&self, name: &str) -> bool {
        if name == "Blink" {
            return super::sunset_lag::hold_of(&self.modules).is_some();
        }
        self.module(name).is_some_and(|m| m.on)
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

// The packet hooks read these: they run inside the game's own sends, which
// may happen while a module holds the state.
// No Fall's move packets: 0 leaves them, 1 claims the ground while falling (Packet), 2 never does (No ground); and
// whether holding a mace pauses it.
pub(super) static NO_FALL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub(super) static NO_FALL_MACE: AtomicBool = AtomicBool::new(true);
pub(super) static BLINK: AtomicBool = AtomicBool::new(false);
// Flight's position packets claim the ground, so nothing counts as a fall; Glide's too while you are not flying.
pub(super) static FLIGHT_GROUND: AtomicBool = AtomicBool::new(false);
pub(super) static GLIDE_GROUND: AtomicBool = AtomicBool::new(false);
// Flight's Packet anti kick: every move packet carries the position, and while FLY_DOWN the height sent sinks a
// little below the last one sent (FLY_LAST_Y, f64 bits).
pub(super) static FLY_PACKET: AtomicBool = AtomicBool::new(false);
pub(super) static FLY_DOWN: AtomicBool = AtomicBool::new(false);
pub(super) static FLY_LAST_Y: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
pub(super) static ANTI_SPRINT: AtomicBool = AtomicBool::new(false);
pub(super) static ANTI_GROUND: AtomicBool = AtomicBool::new(false);
pub(super) static WELCOMED: AtomicBool = AtomicBool::new(false);
// Hard Panic: Sunset stops for the rest of this game process (lifecycle.rs: Running, Stopping,
// Dormant). Nothing about it is saved; the next launch starts Running, with the settings as they
// were saved before Panic.
static STOP_REASON: Mutex<Option<String>> = Mutex::new(None);

pub(super) fn running() -> bool { RUNTIME.running() }

// Panic. The first call closes every Sunset entry point at once (each hook checks `running()`)
// and takes Sunset out of Options > Mods and the launcher's list; the cleanup itself waits until
// no plugin callback is still executing (a callback may have asked for the stop), then
// `finish_shutdown` runs it once from whichever entry point comes next on the game thread.
pub(super) fn shutdown(game: &mut Game<'_>, why: &str) {
    super::sunset_baritone::stop(game, super::sunset_baritone::StopReason::Panic);
    if RUNTIME.request_stop() {
        // The input-only crate has no protocol dependency; its hook uses this
        // existing switch directly instead of dispatching through the lifecycle.
        ::sunset_core::input::NO_SLOW.store(false, Ordering::Relaxed);
        *STOP_REASON.lock().unwrap_or_else(|e| e.into_inner()) = Some(why.to_owned());
        super::sunset_api::cancel_tasks();
        // The menu closes now (and gives the game back its cursor); the HUD stops drawing with
        // `running()`, its state is cleared with the rest.
        if gui_open() { close_gui(game); }
        unlist(game);
        super::sunset_session::hide(ID);
    }
    finish_shutdown(game);
}

pub(super) fn finish_shutdown(game: &mut Game<'_>) {
    if !RUNTIME.stopping() || !RUNTIME.idle() { return; }
    let why = STOP_REASON.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(why) = why else { return; };
    all_off(game);
    super::sunset_api::stopped(game, &why);
    // Stop handlers may use the API for cleanup, but cannot leave modules on.
    with(|s| { for m in &mut s.modules { m.on = false; } sync_flags(s); });
    // Stop handlers ran after the first unlist; a row they could have caused goes too.
    unlist(game);
    RUNTIME.finish_stop();
}

// Sunset's row leaves Options > Mods for this session: the row list (`ModList::installed`, which
// is never saved), and the options screen's pages and waiting key that pointed at it. The mod
// stays built in and on (`horizon-mods.txt` is untouched); `listed` adds no row again.
fn unlist(game: &mut Game<'_>) {
    let Some(i) = mod_index(game) else { return; };
    game.settings.mods.installed.remove(i);
    crate::game::sunset_mod_rows::removed(&mut game.ui.options, i);
}
// The server takes one position packet per tick and disconnects a client that
// sends more ("Invalid move player packet received"). Criticals hops over three
// ticks instead: stages 1 to 3 turn the next position packets into the hop up
// (CRIT_HOP blocks, f64 bits), half way down and back down (all off the ground),
// stage 4 lets the hit follow. Vanilla servers take each packet as it comes, so
// two would do; Horizon's counts a tick's moves only after its packets, the hit
// included, so the hit needs a move down counted in an earlier tick. A mace's
// smash hop (CRIT_DROP) comes all the way down on its second move: the whole
// drop is counted a tick before the hit, so a hit that arrives in the same
// server tick as the last move still smashes. The hit
// goes at the start of the next tick, and that tick's move (CRIT_LAND) stays off
// the ground where the hop ended: Horizon's server judges a tick's hit with the
// ground its move says. Stage CRIT_FALL waits for you to fall (Jump, Mini jump,
// a hit in the air), and CRIT_JUMP presses jump for the next tick.
pub(super) static CRIT_STAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub(super) const CRIT_HIT: u8 = 4;
pub(super) const CRIT_FALL: u8 = 5;
pub(super) const CRIT_LAND: u8 = 6;
pub(super) static CRIT_HOP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(super) static CRIT_DROP: AtomicBool = AtomicBool::new(false);
pub(super) static CRIT_JUMP: AtomicBool = AtomicBool::new(false);
// A server takes no critical hit from a sprinting player, and Horizon's applies all the sprint commands of a tick
// before its hits: so a critical hit's sprint stop goes out when the hit is planned, ticks before it, and the sprint
// comes back the tick after it. CRIT_SPRINT_STOPPED: the server was told the sprint stopped; CRIT_SPRINT_DUE: the hit
// went, the sprint goes back on next tick.
pub(super) static CRIT_SPRINT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub(super) const CRIT_SPRINT_STOPPED: u8 = 1;
pub(super) const CRIT_SPRINT_DUE: u8 = 2;
// A move packet went out this tick (the game's or Sunset's): Sunset never sends a second.
pub(super) static MOVED_THIS_TICK: AtomicBool = AtomicBool::new(false);
// Criticals sent a hop move of its own after the game's ClientTickEnd (crit_tick runs then), so it is that wire tick's
// position packet: until the next ClientTickEnd the game's own move goes without its position (outgoing).
pub(super) static CRIT_LOOSE: AtomicBool = AtomicBool::new(false);
// Blink held at least one move packet.
pub(super) static BLINK_HELD: AtomicBool = AtomicBool::new(false);
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
        s.plugin_defaults.push(m.clone());
        s.modules.push(m);
        let raw = s.raw.clone();
        s.load_text(&raw);
        if !running() { for m in &mut s.modules { m.on = false; } }
        sync_flags(s);
    });
}

// The module at `i` as it starts: Sunset's own defaults, or a plugin's module as the plugin registered it.
pub(super) fn fresh_module(s: &State, i: usize) -> Module {
    let mut built_in = defaults();
    if i < built_in.len() {
        return built_in.swap_remove(i);
    }
    let name = s.modules[i].name;
    s.plugin_defaults.iter().find(|m| m.name == name).cloned().unwrap_or_else(|| s.modules[i].clone())
}

// The modules for one tick or frame. Comparing costs no allocation, so they are copied only when they changed.
pub(super) fn snapshot() -> Arc<Vec<Module>> {
    with(|s| {
        if *s.shared != s.modules {
            s.shared = Arc::new(s.modules.clone());
        }
        s.shared.clone()
    })
}

pub(super) fn sync_flags(s: &mut State) {
    if let Some(list) = s.module("ClickGUI").map(|m| super::sunset_picker::chosen(m.t("Friends")).into_iter().map(str::to_owned).collect()) {
        s.friends = list;
    }
    let g = s.module("ClickGUI");
    OLED_THEME.store(g.is_some_and(|m| m.p("Theme") == "OLED"), Ordering::Relaxed);
    super::sunset_dashboard::set_on(g.is_some_and(|m| m.p("Style") == "Dashboard"));
    super::sunset_sidebar::set_on(g.is_some_and(|m| m.p("Style") == "Sidebar"));
    super::sunset_smooth::set_enabled(g.is_none_or(|m| m.p("Animations") != "Off"));
    LOOK[0].store(g.map_or(0xFFB020, |m| m.c("Accent color") & 0x00FF_FFFF), Ordering::Relaxed);
    LOOK[1].store(g.map_or(120.0, |m| m.n("Panel width") as f32).to_bits(), Ordering::Relaxed);
    LOOK[2].store(g.map_or(1.0, |m| m.n("Background opacity") as f32).to_bits(), Ordering::Relaxed);
    LOOK[3].store(g.map_or(35.0, |m| m.n("Background dim")) as u32, Ordering::Relaxed);
    LOOK[5].store(g.map_or(2.0, |m| m.n("Text indent") as f32).to_bits(), Ordering::Relaxed);
    LOOK[6].store(g.map_or(1.0, |m| m.n("GUI scale") as f32).to_bits(), Ordering::Relaxed);
    FIT_SCREEN.store(g.is_none_or(|m| m.b("Fit to screen")), Ordering::Relaxed);
    LOOK[8].store(g.map_or(4.0, |m| m.n("Panel radius") as f32).to_bits(), Ordering::Relaxed);
    LOOK[9].store(g.map_or(3.0, |m| m.n("Control radius") as f32).to_bits(), Ordering::Relaxed);
    LOOK[10].store(u32::from(g.is_some_and(|m| m.p("Spacing") == "Compact")), Ordering::Relaxed);
    let no_fall = s.module("No Fall").filter(|m| m.on);
    NO_FALL.store(no_fall.map_or(0, |m| match m.p("Mode") {
        "Packet" => 1,
        "No ground" => 2,
        _ => 0,
    }), Ordering::Relaxed);
    NO_FALL_MACE.store(no_fall.is_some_and(|m| m.b("Pause on mace")), Ordering::Relaxed);
    let flight = s.module("Flight").filter(|m| m.on);
    let legacy_flight = flight.filter(|m| !super::sunset_flight::experimental(m.p("Mode")));
    FLIGHT_GROUND.store(legacy_flight.is_some(), Ordering::Relaxed);
    GLIDE_GROUND.store(s.is_on("Glide"), Ordering::Relaxed);
    FLY_PACKET.store(legacy_flight.is_some_and(|m| m.p("Anti kick mode") == "Packet"), Ordering::Relaxed);
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
    let data = game.native_data_dir(ID);
    super::sunset_profiles::sweep(&data);
    // Sunset saves its config in its data directory as a single file.
    let mut text = std::fs::read_to_string(data.join("config")).unwrap_or_default();
    let mut recovered = false;
    let read = match (text.is_empty(), super::sunset_profiles::whole_config(&text)) {
        (_, true) => format!("read {} lines", text.lines().count()),
        (true, false) => "no settings file".to_owned(),
        (false, false) => format!("a settings file cut short ({} bytes)", text.len()),
    };
    // A config cut short (a crash while an older version wrote it, a disk that filled up): the copy kept at the last
    // launch that loaded a whole one. Without either, the defaults (a first launch).
    if !super::sunset_profiles::whole_config(&text)
        && let Some(copy) = std::fs::read_to_string(data.join("config.bak")).ok().filter(|t| super::sunset_profiles::whole_config(t))
    {
        notice(format!(
            "Sunset's settings file was {}: loaded the copy kept at the last launch.",
            if text.is_empty() { "missing or empty" } else { "cut short" }
        ));
        text = copy;
        recovered = true;
    } else if super::sunset_profiles::whole_config(&text)
        && let Err(e) = super::sunset_profiles::write_config(&data, "config.bak", &text)
    {
        notice(format!("Couldn't keep a copy of the settings ({e})."));
    }
    *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Kept {
        folder: data.clone(),
        loaded: match (recovered, super::sunset_profiles::whole_config(&text)) {
            (true, _) => format!("{read}: loaded the copy kept at the last launch ({} lines)", text.lines().count()),
            (false, true) => read,
            (false, false) => format!("{read}: the defaults"),
        },
        ..Kept::default()
    });
    with(|s| {
        s.raw = text.clone();
        s.load_text(&text);
        s.loaded = true;
        sync_flags(s);
    });
    // Loadouts live in their own file now (the config kept them before).
    super::sunset_loadouts::load(game);
    if !text.lines().any(|l| l.starts_with("profile-selection=")) {
        if let Ok(store) = super::sunset_profiles::Store::new(game.native_data_dir(ID))
            && let Ok(names) = store.list()
        {
            let matches: Vec<String> = names.into_iter().filter(|n| n != super::sunset_profiles::BACKUP).filter(|n| {
                store.read(n).is_ok_and(|text| with(|s| s.check_profile(&text).is_ok_and(|c| s.matches_profile(&c.body))))
            }).collect();
            if let [name] = matches.as_slice() {
                with(|s| s.select(name));
            }
        }
        save(game);
    } else if recovered {
        save(game);
    }
}

// A setting set from code (a preset filling in its values), as .set would, and saved.
pub(super) fn set_setting(game: &Game<'_>, key: &str, value: &str) {
    with(|s| State::set_field(&mut s.modules, key, value));
    save(game);
}

pub(super) fn save(game: &Game<'_>) {
    // Panic writes nothing: the next launch starts from the settings saved before it.
    if !running() { return; }
    // Before the settings were read, the state is the defaults: writing it would replace the player's.
    if !with(|s| s.loaded) { return; }
    let text = with(|s| {
        release_changed_binds(s);
        sync_flags(s);
        let snapshot = s.profile_text();
        s.selection.refresh(&snapshot);
        s.selection.fresh = fresh_settings(&snapshot);
        s.save_text()
    });
    // Unchanged since the last write: nothing to do (a click that changed nothing, the same value dragged back).
    let hash = super::sunset_profiles::fingerprint(&text);
    let mut last = LAST_SAVED.lock().unwrap_or_else(|e| e.into_inner());
    if *last == Some(hash) {
        return;
    }
    let written = super::sunset_profiles::write_config(&game.native_data_dir(ID), "config", &text);
    if let Some(kept) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        match &written {
            Ok(()) => kept.saves += 1,
            Err(e) => {
                kept.failed += 1;
                kept.last_error.clone_from(e);
            }
        }
    }
    match written {
        Ok(()) => *last = Some(hash),
        Err(e) => {
            // A virus scanner holding the old file can stop the rename: written in place instead, and tried again
            // at the next save. Said once a launch for each outcome, not at every change.
            *last = None;
            let plain = super::sunset_profiles::write_in_place(&game.native_data_dir(ID), "config", &text);
            if let Some(kept) = KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                let first = if plain.is_ok() { !std::mem::replace(&mut kept.said_plain, true) } else { !std::mem::replace(&mut kept.said_lost, true) };
                if !first {
                    return;
                }
            }
            match plain {
                Ok(()) => notice(format!("Couldn't save the settings safely ({e}); saved them the plain way.")),
                Err(second) => notice(format!("Couldn't save the settings: {e}; {second}. Changes made now are lost when the game closes.")),
            }
        }
    }
}

// Whether these settings (a profile's text of them) are a fresh install's: with no profile in use the HUD then names
// them Default, as the Profiles menu's preset is named, not Custom. Never with a plugin's modules among them.
fn fresh_settings(snapshot: &str) -> bool {
    static FRESH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    super::sunset_profiles::fingerprint(snapshot) == *FRESH.get_or_init(|| super::sunset_profiles::fingerprint(&State::new().profile_text()))
}

// What the config was last written as, so the same text isn't written again.
static LAST_SAVED: Mutex<Option<u64>> = Mutex::new(None);

// How the settings came in at this launch and how the saves since went, for the Debug Report: settings that "reset"
// were not read, not written, or kept in another folder than the one the next launch reads.
#[derive(Default)]
struct Kept {
    folder: std::path::PathBuf,
    loaded: String,
    saves: u32,
    failed: u32,
    last_error: String,
    // The notice for a save that went the plain way, and for one that did not go at all, was shown this launch.
    said_plain: bool,
    said_lost: bool,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

// A path as a report shows it: the home folder (which names the computer's user) as ~.
fn home_as_tilde(path: &std::path::Path) -> String {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).filter(|h| !h.is_empty());
    match home.and_then(|h| path.strip_prefix(h).ok().map(|rest| std::path::Path::new("~").join(rest))) {
        Some(short) => short.display().to_string(),
        None => path.display().to_string(),
    }
}

// The Debug Report's "Settings file" section.
pub(super) fn kept_report(game: &Game<'_>, out: &mut String) {
    use std::fmt::Write as _;
    let now = game.native_data_dir(ID);
    let _ = writeln!(out, "\n## Settings file");
    let kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(kept) = kept.as_ref() else {
        let _ = writeln!(out, "- Not read yet. Folder: {}", home_as_tilde(&now));
        return;
    };
    let _ = writeln!(out, "- Folder: {}", home_as_tilde(&now));
    if kept.folder != now {
        // The game's folder is its working folder: one that changed since the launch splits the settings in two.
        let _ = writeln!(out, "- CHANGED since launch, when it was: {}", home_as_tilde(&kept.folder));
    }
    let _ = writeln!(out, "- At launch: {}", kept.loaded);
    let failed = if kept.failed == 0 { String::new() } else { format!(", {} failed safely and went the plain way (last: {})", kept.failed, kept.last_error) };
    let _ = writeln!(out, "- Saved since: {} times{failed}{}", kept.saves, if running() { "" } else { " (Panic: nothing is saved)" });
    let on_disk = std::fs::metadata(now.join("config")).ok().map(|m| {
        let age = m.modified().ok().and_then(|t| t.elapsed().ok()).map_or("at an unknown time".to_owned(), |d| format!("{} s ago", d.as_secs()));
        format!("{} bytes, written {age}", m.len())
    });
    let _ = writeln!(out, "- On disk now: {}", on_disk.unwrap_or_else(|| "no config file".to_owned()));
}

// Lines for the chat from where there is none to write to (loading the settings): said at the next tick in a world,
// each once.
static NOTICES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn notice(line: String) {
    let mut n = NOTICES.lock().unwrap_or_else(|e| e.into_inner());
    if !n.contains(&line) && n.len() < 8 {
        n.push(line);
    }
}

pub(super) fn say_notices(game: &mut Game<'_>) {
    let lines = std::mem::take(&mut *NOTICES.lock().unwrap_or_else(|e| e.into_inner()));
    let _ = game;
    for line in lines {
        super::sunset_toast::push(super::sunset_toast::Kind::Warning, &line);
    }
}

// A setting as the config and profiles write it. ESP's, Tracers' and Nametags' Range of 64 is written 64.0: a bare 64
// is the default from before All (0), which set_field turns into All, so a 64 you chose would come back as All.
fn kept_as(id: &str, s: &Setting) -> String {
    let stored = s.stored();
    if stored == "64" && s.name == "Range" && matches!(id, "esp" | "tracers" | "nametags") { "64.0".to_owned() } else { stored }
}

// The modules whose choice fills in other settings (a preset), and that choice.
const PRESET_CHOICES: [(&str, &str); 4] =
    [("Scaffold", "Preset"), ("Kill Aura", "Preset"), ("Inventory Manager", "Preset"), ("Auto Sign", "Anticheat")];

// What picking `preset` in module `id` fills in, as config fields and values.
pub(super) fn preset_fields(id: &str, preset: &str) -> Vec<(String, String)> {
    fn listed(presets: &[(&str, &[(&str, &str)])], preset: &str) -> Vec<(String, String)> {
        presets.iter().filter(|p| p.0 == preset).flat_map(|p| p.1.iter().map(|(f, v)| (norm(f), (*v).to_owned()))).collect()
    }
    match id {
        "scaffold" => listed(&SCAFFOLD_PRESETS, preset),
        "killaura" => listed(&super::sunset_aura::PRESETS, preset),
        "autosign" => listed(super::sunset_auto_sign::PRESETS, preset),
        "inventorymanager" => super::sunset_player::PRESETS.iter().filter(|p| p.0 == preset)
            .flat_map(|&(_, blocks, food, arrows, pearls, junk)| {
                [("blocks", blocks.to_string()), ("food", food.to_string()), ("arrows", arrows.to_string()),
                    ("pearls", pearls.to_string()), ("junk", junk.to_owned())]
            })
            .map(|(f, v)| (f.to_owned(), v))
            .collect(),
        _ => Vec::new(),
    }
}
