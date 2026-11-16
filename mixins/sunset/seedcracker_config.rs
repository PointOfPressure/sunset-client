// kaptainwutax.seedcrackerX.config.Config and util.FeatureToggle. The file is `seedcracker.json` in the config folder and
// is written as Gson writes it (two spaces, the fields in the order they are declared, an enum by the name of its
// constant, a FeatureToggle as `{"enabled": true}`), so the Java mod and this one read each other's.
use super::sunset_scx_version::MCVersion;
use std::sync::atomic::{AtomicBool, Ordering};

/// `FeatureToggle`.
#[derive(Debug)]
pub struct FeatureToggle {
    enabled: AtomicBool,
}

impl FeatureToggle {
    pub fn new(flag: bool) -> FeatureToggle {
        FeatureToggle { enabled: AtomicBool::new(flag) }
    }

    pub fn set(&self, flag: bool) {
        self.enabled.store(flag, Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

/// `Config.RenderType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderType {
    Off,
    On,
    Xray,
}

impl RenderType {
    pub const ALL: [RenderType; 3] = [RenderType::Off, RenderType::On, RenderType::Xray];

    /// The name of the Java constant (`toString`).
    pub fn name(self) -> &'static str {
        match self {
            RenderType::Off => "OFF",
            RenderType::On => "ON",
            RenderType::Xray => "XRAY",
        }
    }

    pub fn from_name(name: &str) -> Option<RenderType> {
        Self::ALL.into_iter().find(|r| r.name() == name)
    }
}

/// The toggles of the finders, in the order the Java declares them.
pub const TOGGLE_FIELDS: [&str; 17] = [
    "buriedTreasure",
    "desertTemple",
    "endCity",
    "jungleTemple",
    "monument",
    "swampHut",
    "shipwreck",
    "outpost",
    "igloo",
    "trialChambers",
    "endPillars",
    "endGateway",
    "dungeon",
    "emeraldOre",
    "desertWell",
    "warpedFungus",
    "biome",
];

#[derive(Debug)]
pub struct Config {
    pub buried_treasure: FeatureToggle,
    pub desert_temple: FeatureToggle,
    pub end_city: FeatureToggle,
    pub jungle_temple: FeatureToggle,
    pub monument: FeatureToggle,
    pub swamp_hut: FeatureToggle,
    pub shipwreck: FeatureToggle,
    pub outpost: FeatureToggle,
    pub igloo: FeatureToggle,
    pub trial_chambers: FeatureToggle,
    pub end_pillars: FeatureToggle,
    pub end_gateway: FeatureToggle,
    pub dungeon: FeatureToggle,
    pub emerald_ore: FeatureToggle,
    pub desert_well: FeatureToggle,
    pub warped_fungus: FeatureToggle,
    pub biome: FeatureToggle,
    pub render: RenderType,
    pub active: bool,
    pub debug: bool,
    pub anti_xray_bypass: bool,
    version: MCVersion,
    pub database_submits: bool,
    pub anonymus_submits: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            buried_treasure: FeatureToggle::new(true),
            desert_temple: FeatureToggle::new(true),
            end_city: FeatureToggle::new(true),
            jungle_temple: FeatureToggle::new(true),
            monument: FeatureToggle::new(true),
            swamp_hut: FeatureToggle::new(true),
            shipwreck: FeatureToggle::new(true),
            outpost: FeatureToggle::new(true),
            igloo: FeatureToggle::new(true),
            trial_chambers: FeatureToggle::new(true),
            end_pillars: FeatureToggle::new(true),
            end_gateway: FeatureToggle::new(false),
            dungeon: FeatureToggle::new(true),
            emerald_ore: FeatureToggle::new(false),
            desert_well: FeatureToggle::new(false),
            warped_fungus: FeatureToggle::new(false),
            biome: FeatureToggle::new(false),
            render: RenderType::Xray,
            active: true,
            debug: false,
            anti_xray_bypass: true,
            version: MCVersion::latest(),
            database_submits: false,
            anonymus_submits: false,
        }
    }
}

impl Config {
    pub fn get_version(&self) -> MCVersion {
        self.version
    }

    /// `setVersion`: the caller rebuilds the features when it answers true.
    pub fn set_version(&mut self, version: MCVersion) -> bool {
        if self.version == version {
            return false;
        }
        self.version = version;
        true
    }

    fn toggles(&self) -> [&FeatureToggle; 17] {
        [
            &self.buried_treasure,
            &self.desert_temple,
            &self.end_city,
            &self.jungle_temple,
            &self.monument,
            &self.swamp_hut,
            &self.shipwreck,
            &self.outpost,
            &self.igloo,
            &self.trial_chambers,
            &self.end_pillars,
            &self.end_gateway,
            &self.dungeon,
            &self.emerald_ore,
            &self.desert_well,
            &self.warped_fungus,
            &self.biome,
        ]
    }

    /// Gson's output for the config.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        for (name, toggle) in TOGGLE_FIELDS.iter().zip(self.toggles()) {
            out.push_str(&format!("  \"{name}\": {{\n    \"enabled\": {}\n  }},\n", toggle.get()));
        }
        out.push_str(&format!("  \"render\": \"{}\",\n", self.render.name()));
        out.push_str(&format!("  \"active\": {},\n", self.active));
        out.push_str(&format!("  \"debug\": {},\n", self.debug));
        out.push_str(&format!("  \"antiXrayBypass\": {},\n", self.anti_xray_bypass));
        out.push_str(&format!("  \"version\": \"{}\",\n", version_constant_name(self.version)));
        out.push_str(&format!("  \"databaseSubmits\": {},\n", self.database_submits));
        out.push_str(&format!("  \"anonymusSubmits\": {}\n}}", self.anonymus_submits));
        out
    }

    /// `Gson.fromJson`: a field that is not in the file keeps its default. `None` where the text is not a config.
    pub fn from_json(text: &str) -> Option<Config> {
        let mut parser = Json { chars: text.chars().collect(), at: 0 };
        let value = parser.value()?;
        let JsonValue::Object(fields) = value else { return None };
        let mut config = Config::default();
        for (key, value) in fields {
            match (key.as_str(), value) {
                (name, JsonValue::Object(inner)) if TOGGLE_FIELDS.contains(&name) => {
                    if let Some((_, JsonValue::Bool(b))) = inner.iter().find(|(k, _)| k == "enabled") {
                        let idx = TOGGLE_FIELDS.iter().position(|f| *f == name)?;
                        config.toggles()[idx].set(*b);
                    }
                }
                ("render", JsonValue::String(s)) => config.render = RenderType::from_name(&s)?,
                ("active", JsonValue::Bool(b)) => config.active = b,
                ("debug", JsonValue::Bool(b)) => config.debug = b,
                ("antiXrayBypass", JsonValue::Bool(b)) => config.anti_xray_bypass = b,
                ("version", JsonValue::String(s)) => config.version = version_from_constant_name(&s)?,
                ("databaseSubmits", JsonValue::Bool(b)) => config.database_submits = b,
                ("anonymusSubmits", JsonValue::Bool(b)) => config.anonymus_submits = b,
                _ => {}
            }
        }
        Some(config)
    }
}

/// The name of the Java enum constant, which is what Gson writes: `v1_21_3`.
pub fn version_constant_name(version: MCVersion) -> String {
    let name = version.name();
    let mut out = String::from("v");
    for ch in name.chars() {
        out.push(if ch == '.' { '_' } else { ch });
    }
    out
}

fn version_from_constant_name(name: &str) -> Option<MCVersion> {
    MCVersion::values().iter().copied().find(|v| version_constant_name(*v) == name)
}

enum JsonValue {
    Object(Vec<(String, JsonValue)>),
    String(String),
    Bool(bool),
    Other,
}

struct Json {
    chars: Vec<char>,
    at: usize,
}

impl Json {
    fn skip(&mut self) {
        while self.at < self.chars.len() && self.chars[self.at].is_whitespace() {
            self.at += 1;
        }
    }

    fn eat(&mut self, ch: char) -> Option<()> {
        self.skip();
        if self.chars.get(self.at) == Some(&ch) {
            self.at += 1;
            Some(())
        } else {
            None
        }
    }

    fn string(&mut self) -> Option<String> {
        self.eat('"')?;
        let mut out = String::new();
        loop {
            let ch = *self.chars.get(self.at)?;
            self.at += 1;
            match ch {
                '"' => return Some(out),
                '\\' => {
                    let esc = *self.chars.get(self.at)?;
                    self.at += 1;
                    out.push(match esc {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                }
                other => out.push(other),
            }
        }
    }

    fn value(&mut self) -> Option<JsonValue> {
        self.skip();
        match self.chars.get(self.at)? {
            '{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.skip();
                if self.chars.get(self.at) == Some(&'}') {
                    self.at += 1;
                    return Some(JsonValue::Object(fields));
                }
                loop {
                    let key = self.string()?;
                    self.eat(':')?;
                    fields.push((key, self.value()?));
                    self.skip();
                    match self.chars.get(self.at)? {
                        ',' => self.at += 1,
                        '}' => {
                            self.at += 1;
                            return Some(JsonValue::Object(fields));
                        }
                        _ => return None,
                    }
                }
            }
            '"' => Some(JsonValue::String(self.string()?)),
            _ => {
                let start = self.at;
                while self.at < self.chars.len() && !matches!(self.chars[self.at], ',' | '}' | ']') && !self.chars[self.at].is_whitespace() {
                    self.at += 1;
                }
                let word: String = self.chars[start..self.at].iter().collect();
                Some(match word.as_str() {
                    "true" => JsonValue::Bool(true),
                    "false" => JsonValue::Bool(false),
                    _ => JsonValue::Other,
                })
            }
        }
    }
}
