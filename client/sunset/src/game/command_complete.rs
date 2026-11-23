// The . commands' grammar for the chat's suggestions (chat_suggest.rs): at the cursor, what can come next (a list to
// pick from), the arguments still to come (a usage line) or a word before it that can't be read (an error line).
// Nothing here touches the game: what it needs to know (modules, profiles, players...) comes through `Source`.
//
// It follows what runtime_commands.rs `run` reads: keep the two together when a command or its arguments change.

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Suggestion {
    // What Tab puts in.
    pub(crate) text: String,
    // What the list shows beside it: what it is or does.
    pub(crate) tip: String,
    // Other words it is found by (a module's name with its spaces: `aura` finds Kill Aura).
    pub(crate) also: String,
}

impl Suggestion {
    fn new(text: impl Into<String>, tip: impl Into<String>) -> Self {
        Self { text: text.into(), tip: tip.into(), also: String::new() }
    }
    fn also(mut self, also: impl Into<String>) -> Self {
        self.also = also.into();
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Completion {
    // The input's bytes the list replaces: from the start of the word (or words) being typed to the cursor.
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) list: Vec<Suggestion>,
    // The arguments still to come (`<x> <y> <z>`), drawn from `usage_at` when no list shows.
    pub(crate) usage: Option<String>,
    pub(crate) usage_at: usize,
    // A word that can't be read, drawn in red in place of the usage.
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    Bool(bool),
    Number { now: f64, min: f64, max: f64, step: f64 },
    Pick { now: usize, of: Vec<String> },
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModuleInfo {
    pub(crate) name: String,
    pub(crate) about: String,
    pub(crate) on: bool,
    pub(crate) key: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingInfo {
    pub(crate) name: String,
    pub(crate) kind: Kind,
}

// What the grammar asks of the game. Each is read once per edit of the chat box, not per frame.
pub(crate) trait Source {
    fn modules(&self) -> Vec<ModuleInfo>;
    // The module a typed word names, as the commands find it (renamed modules too).
    fn find_module(&self, word: &str) -> Option<ModuleInfo>;
    fn settings(&self, module: &str) -> Vec<SettingInfo>;
    fn profiles(&self) -> Vec<String>;
    // Files in the mod's imports folder with this extension (`profile`, `loadout`), without it.
    fn imports(&self, extension: &str) -> Vec<String>;
    fn presets(&self) -> Vec<(String, String)>;
    fn friends(&self) -> Vec<String>;
    fn players(&self) -> Vec<String>;
    fn waypoints(&self) -> Vec<String>;
    fn loadouts(&self) -> Vec<String>;
    // Plugins' commands: name and their line in .help.
    fn plugin_commands(&self) -> Vec<(String, String)>;
    // The block you stand in.
    fn position(&self) -> Option<[i32; 3]>;
}

// Lowercase letters and digits only: how commands and the config name things (runtime_helpers.rs `norm`).
pub(crate) fn norm(s: &str) -> String {
    s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

// ---------------------------------------------------------------- the commands

struct Command {
    name: &'static str,
    aliases: &'static [&'static str],
    usage: &'static str,
    about: &'static str,
    args: fn(&dyn Source, &[&str], &str) -> Step,
}

// What may come at the word being typed, given the words before it (after the command's name).
enum Step {
    // One of these. `strict`: nothing else is read, so a word none of them starts is an error (this text).
    // `back`: how many of the words before belong to this one argument too (a multi-word name or value).
    Pick { list: Vec<Suggestion>, usage: String, strict: Option<String>, back: usize, ordered: bool },
    // Anything; `list` holds a few likely ones.
    Free { list: Vec<Suggestion>, usage: String, back: usize },
    // A word before this one can't be read.
    Bad(String),
    // Nothing more is read.
    Done,
}

fn pick(list: Vec<Suggestion>, usage: impl Into<String>, strict: impl Into<String>) -> Step {
    Step::Pick { list, usage: usage.into(), strict: Some(strict.into()), back: 0, ordered: false }
}

fn ordered(list: Vec<Suggestion>, usage: impl Into<String>, strict: impl Into<String>) -> Step {
    Step::Pick { list, usage: usage.into(), strict: Some(strict.into()), back: 0, ordered: true }
}

fn free(usage: impl Into<String>) -> Step {
    Step::Free { list: Vec::new(), usage: usage.into(), back: 0 }
}

fn none(_: &dyn Source, _: &[&str], _: &str) -> Step {
    Step::Done
}

fn words(list: &[(&str, &str)]) -> Vec<Suggestion> {
    list.iter().map(|(t, tip)| Suggestion::new(*t, *tip)).collect()
}

fn lower(word: Option<&&str>) -> String {
    word.map(|w| w.to_ascii_lowercase()).unwrap_or_default()
}

const COMMANDS: &[Command] = &[
    Command { name: "help", aliases: &["commands"], usage: "", about: "list the commands", args: none },
    Command { name: "menu", aliases: &["gui"], usage: "[search]", about: "open the click GUI, searching for the text", args: menu },
    Command { name: "pages", aliases: &[], usage: "", about: "the same settings as pages in Options > Mods", args: none },
    Command { name: "toggle", aliases: &["t"], usage: "<module>", about: "turn a module on or off", args: one_module },
    Command { name: "modules", aliases: &["list"], usage: "", about: "list every module", args: none },
    Command { name: "settings", aliases: &["get"], usage: "<module>", about: "list a module's settings", args: one_module },
    Command { name: "set", aliases: &[], usage: "<module> <setting> <value>", about: "change a setting", args: set },
    Command { name: "bind", aliases: &[], usage: "<module> <key|mouse4|none>", about: "bind a key or mouse button to a module", args: bind },
    Command { name: "binds", aliases: &[], usage: "", about: "list the binds", args: none },
    Command { name: "reset", aliases: &[], usage: "<module>", about: "put a module back to its defaults", args: one_module },
    Command { name: "profile", aliases: &["profiles", "config", "cfg"], usage: "save|load|delete|export|import|list [name]", about: "keep the modules' settings under a name and bring them back", args: profile },
    Command { name: "preset", aliases: &["presets"], usage: "[name] [apply]", about: "the pre-made profiles: what a preset turns on and off, apply takes it", args: preset },
    Command { name: "movementpreset", aliases: &[], usage: "<subtle|balanced|aggressive>", about: "apply explicit movement settings", args: movement_preset },
    Command { name: "friend", aliases: &["friends", "f"], usage: "add|remove|list [name]", about: "Kill Aura, Trigger Bot and Aim Assist skip friends", args: friend },
    Command { name: "waypoint", aliases: &["waypoints", "wp"], usage: "add|remove|list [name]", about: "save a place", args: waypoint },
    Command { name: "loadout", aliases: &["loadouts"], usage: "save|delete|export|import|list|<name>", about: "keep and put on your hotbar, offhand and armour", args: loadout },
    Command { name: "goto", aliases: &[], usage: "<x> <y> <z>|stop", about: "walk there through loaded terrain, up to 128 blocks", args: goto },
    Command { name: "baritone", aliases: &["b"], usage: "status|goto|mine|stop|off|targets|help", about: "Full Baritone and bounded visible/Ore Sim mining", args: baritone },
    Command { name: "vclip", aliases: &[], usage: "<blocks>", about: "move up or down", args: blocks },
    Command { name: "hclip", aliases: &[], usage: "<blocks>", about: "move forward", args: blocks },
    Command { name: "seed", aliases: &[], usage: "[status|auto|clear|restart|<seed>]", about: "Ore Sim's seed, automatic discovery and progress", args: seed },
    Command { name: "scan", aliases: &[], usage: "[plugins|details|channels|commands|status|run] [page]", about: "show what Server Scan found", args: scan },
    Command { name: "relog", aliases: &[], usage: "[delay seconds]", about: "leave and rejoin this multiplayer server once", args: relog },
    Command { name: "debug", aliases: &[], usage: "[chat lines]", about: "copy a debug report to the clipboard and its Save folder", args: debug },
    Command { name: "vanish", aliases: &[], usage: "debug", about: "Anti Vanish's HorizonSMP evidence", args: vanish },
    Command { name: "staffcheck", aliases: &[], usage: "[test [x y z]]", about: "what explained the pushes Anti Vanish's Staff check saw", args: staffcheck },
    Command { name: "auradebug", aliases: &[], usage: "", about: "why Kill Aura did what it did last, and who has the hand", args: none },
    Command { name: "elytratarget", aliases: &[], usage: "", about: "Elytra Target's state", args: none },
    Command { name: "antibotdebug", aliases: &[], usage: "[entity id]", about: "Anti Bot's confidence and what it saw", args: antibot },
    Command { name: "plugins", aliases: &[], usage: "", about: "the plugins loaded, their modules and commands", args: none },
    Command { name: "panic", aliases: &[], usage: "", about: "stop Sunset until the game restarts", args: none },
];

// The command a typed name runs, as `run` matches it.
fn command(name: &str) -> Option<&'static Command> {
    let name = name.to_ascii_lowercase();
    COMMANDS.iter().find(|c| c.name == name || c.aliases.contains(&name.as_str()))
}

fn modules(src: &dyn Source) -> Vec<Suggestion> {
    src.modules()
        .into_iter()
        .map(|m| {
            let state = if m.on { "on" } else { "off" };
            Suggestion::new(norm(&m.name), format!("{} ({state}): {}", m.name, m.about)).also(m.name)
        })
        .collect()
}

fn module_word(src: &dyn Source, word: &str) -> Result<ModuleInfo, Step> {
    src.find_module(word).ok_or_else(|| Step::Bad(format!("No module called {word}")))
}

fn one_module(src: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => pick(modules(src), "<module>", "No module called"),
        _ => Step::Done,
    }
}

fn menu(src: &dyn Source, before: &[&str], _: &str) -> Step {
    // The search is any text; the modules are what it usually is.
    Step::Free { list: modules(src), usage: "[search]".into(), back: before.len() }
}

fn number_usage(now: f64, min: f64, max: f64, step: f64) -> String {
    let show = |v: f64| {
        let s = format!("{v:.2}");
        let s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
        if s == "-0" { "0".to_owned() } else { s }
    };
    let whole = step >= 1.0 && min.fract() == 0.0 && max.fract() == 0.0;
    format!("<{} {} to {}> (now {})", if whole { "whole number" } else { "number" }, show(min), show(max), show(now))
}

fn value_text(kind: &Kind) -> String {
    match kind {
        Kind::Bool(b) => if *b { "ON" } else { "OFF" }.to_owned(),
        Kind::Number { now, .. } => {
            let s = format!("{now:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
        Kind::Pick { now, of } => of.get(*now).cloned().unwrap_or_default(),
        Kind::Text(t) if t.is_empty() => "(empty)".to_owned(),
        Kind::Text(t) => t.chars().take(40).collect(),
    }
}

fn set(src: &dyn Source, before: &[&str], partial: &str) -> Step {
    let Some((module, words)) = before.split_first() else {
        return pick(modules(src), "<module> <setting> <value>", "No module called");
    };
    let module = match module_word(src, module) {
        Ok(m) => m,
        Err(bad) => return bad,
    };
    let settings = src.settings(&module.name);
    if settings.is_empty() {
        return Step::Bad(format!("{} has no settings", module.name));
    }
    let names = || {
        settings
            .iter()
            .map(|s| Suggestion::new(norm(&s.name), format!("{}: {}", s.name, value_text(&s.kind))).also(s.name.clone()))
            .collect::<Vec<_>>()
    };
    if words.is_empty() {
        return ordered(names(), "<setting> <value>", format!("{} has no setting called", module.name));
    }
    // As `.set` reads it: the setting is the first words that name one exactly, else by its start; the rest is the
    // value. Here the value may not be typed yet, so every count of words is tried.
    let find = |exact: bool| {
        (1..=words.len()).rev().find_map(|k| {
            let name = norm(&words[..k].join(""));
            settings
                .iter()
                .find(|s| if exact { norm(&s.name) == name } else { norm(&s.name).starts_with(&name) })
                .map(|s| (k, s))
        })
    };
    let found = find(true);
    // Still typing a setting of more than one word (`attack ra`, `range wa` for Range walls beside Range): its name
    // again, from its first word. A word after a whole name (`range `) is its value.
    let all = norm(&format!("{}{partial}", words.join("")));
    let longer = settings.iter().any(|s| {
        let n = norm(&s.name);
        n.starts_with(&all) && found.is_none_or(|(k, f)| n != norm(&f.name) && k == words.len())
    });
    if longer && (found.is_none() || !partial.is_empty()) {
        return Step::Pick {
            list: names(),
            usage: "<setting> <value>".into(),
            strict: Some(format!("{} has no setting called", module.name)),
            back: words.len(),
            ordered: true,
        };
    }
    let Some((k, setting)) = found.or_else(|| find(false)) else {
        return Step::Bad(format!("{} has no setting called {}", module.name, words.join(" ")));
    };
    // The value: the rest of the line.
    let back = words.len() - k;
    let now = value_text(&setting.kind);
    match &setting.kind {
        Kind::Bool(b) => Step::Pick {
            list: vec![
                Suggestion::new("on", if *b { "now on" } else { "turn it on" }),
                Suggestion::new("off", if *b { "turn it off" } else { "now off" }),
                Suggestion::new("toggle", "the other way"),
            ],
            usage: format!("<on|off|toggle> (now {now})"),
            strict: Some(format!("{} is on or off, not", setting.name)),
            back,
            ordered: true,
        },
        Kind::Number { now, min, max, step } => {
            Step::Free { list: Vec::new(), usage: number_usage(*now, *min, *max, *step), back }
        }
        Kind::Pick { now, of } => Step::Pick {
            list: of
                .iter()
                .enumerate()
                .map(|(i, o)| Suggestion::new(o.clone(), if i == *now { "now" } else { "" }))
                .collect(),
            usage: format!("<{}> (now {})", of.join("|"), value_text(&setting.kind)),
            strict: Some(format!("{} is one of {}, not", setting.name, of.join(", "))),
            back,
            ordered: true,
        },
        Kind::Text(_) => Step::Free { list: Vec::new(), usage: format!("<text> (now {now})"), back },
    }
}

// The keys `.bind` reads by a short name (bind_state.rs `canonical` reads more: f13, key.keyboard.*, mouse6...).
const KEYS: &[&str] = &[
    "none", "mouse3", "mouse4", "mouse5", "rshift", "lshift", "rctrl", "lctrl", "ralt", "lalt", "tab", "capslock",
    "space", "enter", "backspace", "insert", "delete", "home", "end", "pageup", "pagedown", "up", "down", "left",
    "right", "grave", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "a", "b", "c", "d",
    "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "kp0", "kp1", "kp2", "kp3", "kp4", "kp5", "kp6", "kp7", "kp8",
    "kp9",
];

fn bind(src: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => pick(modules(src), "<module> <key|mouse4|none>", "No module called"),
        [module] => {
            let module = match module_word(src, module) {
                Ok(m) => m,
                Err(bad) => return bad,
            };
            let now = module.key.as_deref().map_or("none".to_owned(), |k| {
                k.trim_start_matches("key.keyboard.").trim_start_matches("key.").to_owned()
            });
            let list = KEYS
                .iter()
                .map(|k| Suggestion::new(*k, if *k == "none" { "no key" } else { "" }))
                .collect();
            Step::Pick { list, usage: format!("<key|mouse4|none> (now {now})"), strict: None, back: 0, ordered: true }
        }
        _ => Step::Done,
    }
}

fn profile(src: &dyn Source, before: &[&str], _: &str) -> Step {
    const SUBS: &[(&str, &str)] = &[
        ("save", "keep the current settings under a name"),
        ("load", "bring a profile's settings back"),
        ("delete", "delete a profile"),
        ("export", "copy a profile to the exports folder, to share"),
        ("import", "keep a profile from the imports folder"),
        ("list", "the profiles, and the one in use"),
    ];
    let Some((sub, name)) = before.split_first() else {
        return ordered(words(SUBS), "save|load|delete|export|import|list [name]", "Profile: use save, load, delete, export, import or list, not");
    };
    let back = name.len();
    let names = |src: &dyn Source| src.profiles().into_iter().map(|p| Suggestion::new(p, "profile")).collect::<Vec<_>>();
    match sub.to_ascii_lowercase().as_str() {
        "list" => Step::Done,
        "save" => Step::Free { list: names(src), usage: "<name>".into(), back },
        "load" => {
            let mut list = names(src);
            list.push(Suggestion::new("before-load", "the settings before the last load"));
            Step::Pick { list, usage: "<name>".into(), strict: Some("No profile called".into()), back, ordered: false }
        }
        "delete" | "export" => {
            Step::Pick { list: names(src), usage: "<name>".into(), strict: Some("No profile called".into()), back, ordered: false }
        }
        "import" => {
            let list = src.imports("profile").into_iter().map(|p| Suggestion::new(p, "in the imports folder")).collect();
            Step::Pick { list, usage: "<name>".into(), strict: Some("No profile in the imports folder called".into()), back, ordered: false }
        }
        other => Step::Bad(format!("Profile: use save, load, delete, export, import or list, not {other}")),
    }
}

fn preset(src: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => {
            let list = src.presets().into_iter().map(|(id, about)| Suggestion::new(id, about)).collect();
            pick(list, "[name] [apply]", "No preset called")
        }
        [name] => {
            if !src.presets().iter().any(|(id, _)| id.eq_ignore_ascii_case(name)) {
                return Step::Bad(format!("No preset called {name}"));
            }
            pick(words(&[("apply", "take it (.profile load before-load undoes it)")]), "[apply]", "Use apply, not")
        }
        _ => Step::Done,
    }
}

fn movement_preset(_: &dyn Source, before: &[&str], _: &str) -> Step {
    if !before.is_empty() {
        return Step::Done;
    }
    ordered(
        words(&[("subtle", "small changes"), ("balanced", "between the two"), ("aggressive", "the most movement")]),
        "<subtle|balanced|aggressive>",
        "Use subtle, balanced or aggressive, not",
    )
}

fn friend(src: &dyn Source, before: &[&str], _: &str) -> Step {
    const SUBS: &[(&str, &str)] = &[("add", "make a player a friend"), ("remove", "no longer a friend"), ("list", "your friends")];
    match (lower(before.first()).as_str(), before.len()) {
        (_, 0) => ordered(words(SUBS), "add|remove|list [name]", "Friend: use add, remove or list, not"),
        ("add", 1) => {
            let friends = src.friends();
            let list = src
                .players()
                .into_iter()
                .filter(|p| !friends.iter().any(|f| f.eq_ignore_ascii_case(p)))
                .map(|p| Suggestion::new(p, "online"))
                .collect();
            Step::Free { list, usage: "<name>".into(), back: 0 }
        }
        ("remove" | "del" | "rm", 1) => {
            let list = src.friends().into_iter().map(|f| Suggestion::new(f, "friend")).collect();
            pick(list, "<name>", "No friend called")
        }
        ("add" | "remove" | "del" | "rm" | "list", _) => Step::Done,
        (other, _) => Step::Bad(format!("Friend: use add, remove or list, not {other}")),
    }
}

fn waypoint(src: &dyn Source, before: &[&str], _: &str) -> Step {
    const SUBS: &[(&str, &str)] = &[("add", "save the place you stand"), ("remove", "delete a waypoint"), ("list", "your waypoints")];
    match (lower(before.first()).as_str(), before.len()) {
        (_, 0) => ordered(words(SUBS), "add|remove|list [name]", "Waypoint: use add, remove or list, not"),
        ("add" | "set", 1) => free("<name>"),
        ("remove" | "del" | "rm", 1) => {
            let list = src.waypoints().into_iter().map(|w| Suggestion::new(w, "waypoint")).collect();
            pick(list, "<name>", "No waypoint called")
        }
        ("add" | "set" | "remove" | "del" | "rm" | "list", _) => Step::Done,
        (other, _) => Step::Bad(format!("Waypoint: use add, remove or list, not {other}")),
    }
}

fn loadout(src: &dyn Source, before: &[&str], _: &str) -> Step {
    const SUBS: &[(&str, &str)] = &[
        ("save", "keep what you wear and hold under a name"),
        ("delete", "delete a loadout"),
        ("export", "copy a loadout to the exports folder"),
        ("import", "keep a loadout from the imports folder"),
        ("list", "your loadouts"),
    ];
    let names = |src: &dyn Source, tip: &str| src.loadouts().into_iter().map(|l| Suggestion::new(l, tip)).collect::<Vec<_>>();
    let Some((sub, name)) = before.split_first() else {
        let mut list = words(SUBS);
        list.extend(names(src, "put it on"));
        return Step::Free { list, usage: "save|delete|export|import|list|<name>".into(), back: 0 };
    };
    let back = name.len();
    match sub.to_ascii_lowercase().as_str() {
        "list" => Step::Done,
        "save" | "add" => Step::Free { list: Vec::new(), usage: "[name]".into(), back },
        "delete" | "del" | "remove" | "rm" | "export" => Step::Pick {
            list: names(src, "loadout"),
            usage: "<name>".into(),
            strict: Some("No loadout called".into()),
            back,
            ordered: false,
        },
        "import" => {
            let list = src.imports("loadout").into_iter().map(|p| Suggestion::new(p, "in the imports folder")).collect();
            Step::Pick { list, usage: "<name>".into(), strict: None, back, ordered: false }
        }
        // `.loadout <name>` puts it on: its name may have spaces.
        _ => Step::Free { list: names(src, "put it on"), usage: "<name>".into(), back: before.len() },
    }
}

// Whole block coordinates, from where you stand: `x`, `x y`, `x y z` as vanilla offers a block position.
fn coordinates(src: &dyn Source, done: usize) -> Vec<Suggestion> {
    let Some(at) = src.position() else {
        return Vec::new();
    };
    let rest: Vec<String> = at[done.min(3)..].iter().map(i32::to_string).collect();
    (1..=rest.len()).rev().map(|n| Suggestion::new(rest[..n].join(" "), "where you stand")).collect()
}

fn coordinate_step(src: &dyn Source, coords: &[&str]) -> Step {
    if let Some(bad) = coords.iter().find(|c| c.parse::<i32>().is_err()) {
        return Step::Bad(format!("{bad} is not a whole block coordinate"));
    }
    if coords.len() >= 3 {
        return Step::Done;
    }
    let usage = ["<x>", "<y>", "<z>"][coords.len()..].join(" ");
    Step::Free { list: coordinates(src, coords.len()), usage, back: 0 }
}

fn goto(src: &dyn Source, before: &[&str], _: &str) -> Step {
    if before.first().is_some_and(|w| w.eq_ignore_ascii_case("stop")) {
        return if before.len() == 1 { Step::Done } else { Step::Bad("Use .goto <x> <y> <z> or .goto stop".into()) };
    }
    if before.is_empty() {
        let mut list = coordinates(src, 0);
        list.push(Suggestion::new("stop", "stop walking"));
        return Step::Free { list, usage: "<x> <y> <z>|stop".into(), back: 0 };
    }
    coordinate_step(src, before)
}

fn baritone(src: &dyn Source, before: &[&str], _: &str) -> Step {
    const SUBS: &[(&str, &str)] = &[
        ("status", "what Baritone is doing"),
        ("goto", "walk to a block"),
        ("mine", "mine blocks you can see, or Ore Sim's ores"),
        ("stop", "stop and let go of the controls"),
        ("off", "turn Baritone off"),
        ("targets", "the mining targets it knows"),
        ("help", "all of Baritone's commands"),
    ];
    let Some((sub, rest)) = before.split_first() else {
        return ordered(words(SUBS), "status|goto|mine|stop|off|targets|help", "Baritone: no command");
    };
    match (sub.to_ascii_lowercase().as_str(), rest) {
        ("goto", coords) => coordinate_step(src, coords),
        ("mine", []) => Step::Free {
            list: words(&[("oresim", "the ores Ore Sim predicts")]),
            usage: "[block[,block...]] | oresim [ore[,ore...]]".into(),
            back: 0,
        },
        ("mine", [w]) if w.eq_ignore_ascii_case("oresim") => free("[ore[,ore...]]"),
        ("targets", []) => free("[1..64]"),
        ("status" | "stop" | "off" | "help" | "mine" | "targets", _) => Step::Done,
        (other, _) => Step::Bad(format!("Baritone: no command {other}")),
    }
}

fn blocks(_: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => free("<blocks>"),
        [b] if b.parse::<f64>().is_err() => Step::Bad(format!("{b} is not a number of blocks")),
        _ => Step::Done,
    }
}

fn seed(_: &dyn Source, before: &[&str], _: &str) -> Step {
    if !before.is_empty() {
        // A seed's text may have spaces.
        return Step::Free { list: Vec::new(), usage: "<seed>".into(), back: before.len() };
    }
    Step::Free {
        list: words(&[
            ("status", "the seed and the search's progress"),
            ("auto", "find the seed by itself"),
            ("clear", "forget the seed"),
            ("restart", "search again"),
        ]),
        usage: "[status|auto|clear|restart|<seed>]".into(),
        back: 0,
    }
}

fn scan(_: &dyn Source, before: &[&str], _: &str) -> Step {
    const VIEWS: &[(&str, &str)] = &[
        ("summary", "what was found"),
        ("plugins", "the server's plugins"),
        ("details", "every finding"),
        ("channels", "the plugin channels"),
        ("commands", "the server's commands"),
        ("status", "what the scan is doing"),
        ("run", "run the Extended checks now"),
    ];
    match (lower(before.first()).as_str(), before.len()) {
        (_, 0) => ordered(words(VIEWS), "[plugins|details|channels|commands|status|run] [page]", "Server Scan has no view"),
        ("plugins" | "details" | "channels", 1) => free("[page]"),
        ("commands", 1) => free("[command] [page]"),
        ("commands", 2) => free("[page]"),
        _ => Step::Done,
    }
}

fn relog(_: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => free("[delay seconds]"),
        [s] if s.parse::<f64>().is_err() => Step::Bad(format!("{s} is not a number of seconds")),
        _ => Step::Done,
    }
}

fn debug(_: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => free("[chat lines]"),
        [n] if n.parse::<usize>().is_err() => Step::Bad(format!("{n} is not a number of chat lines")),
        _ => Step::Done,
    }
}

fn vanish(_: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => pick(words(&[("debug", "Anti Vanish's HorizonSMP evidence")]), "debug", "Use .vanish debug, not"),
        _ => Step::Done,
    }
}

fn staffcheck(_: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => Step::Free { list: words(&[("test", "send it a fake push")]), usage: "[test [x y z]]".into(), back: 0 },
        [t, push @ ..] if t.eq_ignore_ascii_case("test") && push.len() < 3 => free(["[x]", "[y]", "[z]"][push.len()..].join(" ")),
        _ => Step::Done,
    }
}

fn antibot(_: &dyn Source, before: &[&str], _: &str) -> Step {
    match before {
        [] => free("[entity id]"),
        _ => Step::Done,
    }
}

// ---------------------------------------------------------------- matching

// The words of `s` with the byte each starts at.
fn tokens(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in s.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(at)) => {
                out.push((at, &s[at..i]));
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(at) = start {
        out.push((at, &s[at..]));
    }
    out
}

// How well a suggestion fits what is typed: 0 starts with it as typed, 1 starts with it ignoring case and spaces,
// 2 a word of its name starts with it. None: it doesn't fit.
fn fit(s: &Suggestion, typed: &str) -> Option<u8> {
    if typed.is_empty() || s.text.to_lowercase().starts_with(&typed.to_lowercase()) {
        return Some(0);
    }
    let n = norm(typed);
    if n.is_empty() {
        return None;
    }
    if norm(&s.text).starts_with(&n) || norm(&s.also).starts_with(&n) {
        return Some(1);
    }
    let words = s.also.split(|c: char| !c.is_ascii_alphanumeric()).chain(s.text.split(|c: char| !c.is_ascii_alphanumeric()));
    words.filter(|w| !w.is_empty()).any(|w| norm(w).starts_with(&n)).then_some(2)
}

// The fitting ones, best first; alphabetical within each fit unless `ordered`.
fn matching(mut list: Vec<Suggestion>, typed: &str, ordered: bool) -> Vec<Suggestion> {
    if !ordered {
        list.sort_by(|a, b| a.text.to_lowercase().cmp(&b.text.to_lowercase()));
    }
    let mut seen = std::collections::HashSet::new();
    list.retain(|s| seen.insert(s.text.to_lowercase()));
    let mut fits: Vec<(u8, Suggestion)> = list.into_iter().filter_map(|s| fit(&s, typed).map(|f| (f, s))).collect();
    fits.sort_by_key(|(f, _)| *f);
    let out: Vec<Suggestion> = fits.into_iter().map(|(_, s)| s).collect();
    // What is typed already is no suggestion.
    if out.len() == 1 && out[0].text.eq_ignore_ascii_case(typed) {
        return Vec::new();
    }
    out
}

// The command names, plugins' too. An alias is offered only when the typed text fits it and not its command.
fn command_names(src: &dyn Source, typed: &str) -> Vec<Suggestion> {
    let mut list = Vec::new();
    for c in COMMANDS {
        let usage = if c.usage.is_empty() { String::new() } else { format!(" {}", c.usage) };
        let main = Suggestion::new(c.name, format!(".{}{usage}: {}", c.name, c.about));
        if fit(&main, typed) == Some(0) || typed.is_empty() {
            list.push(main);
        } else if let Some(alias) = c.aliases.iter().find(|a| a.starts_with(&typed.to_lowercase())) {
            list.push(Suggestion::new(*alias, format!(".{}{usage}: {}", c.name, c.about)));
        } else {
            list.push(main);
        }
    }
    for (name, about) in src.plugin_commands() {
        list.push(Suggestion::new(name.clone(), format!(".{name}: {about}")));
    }
    list
}

// What the chat box `input` offers at byte `cursor`, or None when it is not a Sunset command (no `.` first; `..`
// sends a chat line starting with `.`).
pub(crate) fn complete(input: &str, cursor: usize, src: &dyn Source) -> Option<Completion> {
    if !input.starts_with('.') || input.starts_with("..") || !input.is_char_boundary(cursor) || cursor == 0 {
        return None;
    }
    let before = &input[1..cursor];
    let toks: Vec<(usize, &str)> = tokens(before).into_iter().map(|(at, w)| (at + 1, w)).collect();
    let typing = !before.is_empty() && !before.ends_with(char::is_whitespace);
    let (done, current) = if typing { (&toks[..toks.len() - 1], toks.last().copied()) } else { (&toks[..], None) };
    let word_start = current.map_or(cursor, |(at, _)| at);
    let mut out = Completion { start: word_start, end: cursor, usage_at: word_start, ..Completion::default() };

    let Some(((_, name), args)) = done.split_first() else {
        let typed = current.map_or("", |(_, w)| w);
        out.list = matching(command_names(src, typed), typed, false);
        match command(typed) {
            // A whole name: what it takes.
            Some(c) if out.list.is_empty() && !c.usage.is_empty() => out.usage = Some(format!("{typed} {}", c.usage)),
            None if out.list.is_empty() && !typed.is_empty() && !plugin(src, typed) => {
                out.error = Some(format!("Unknown command .{typed}"));
            }
            _ => {}
        }
        return Some(out);
    };
    let step = match command(name) {
        Some(c) => (c.args)(src, &args.iter().map(|(_, w)| *w).collect::<Vec<_>>(), current.map_or("", |(_, w)| w)),
        None if plugin(src, name) => {
            let about = src.plugin_commands().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, a)| a);
            Step::Free { list: Vec::new(), usage: about.unwrap_or_default(), back: args.len() }
        }
        None => {
            out.error = Some(format!("Unknown command .{name}"));
            out.usage_at = 1;
            return Some(out);
        }
    };
    let from = |back: usize| if back == 0 || back > args.len() { word_start } else { args[args.len() - back].0 };
    match step {
        Step::Done => {}
        Step::Bad(e) => {
            out.error = Some(e);
            out.usage_at = args.first().map_or(word_start, |(at, _)| *at);
        }
        Step::Pick { list, usage, strict, back, ordered } => {
            out.start = from(back);
            out.usage_at = out.start;
            let typed = &input[out.start..cursor];
            // A whole word that names one is read even though nothing more is offered.
            let whole = !typed.trim().is_empty() && list.iter().any(|s| norm(&s.text) == norm(typed));
            out.list = matching(list, typed, ordered);
            if out.list.is_empty()
                && !typed.trim().is_empty()
                && !whole
                && let Some(why) = strict
            {
                out.error = Some(format!("{why} {}", typed.trim()));
            }
            if out.error.is_none() && !whole && !usage.is_empty() {
                out.usage = Some(usage);
            }
        }
        Step::Free { list, usage, back } => {
            out.start = from(back);
            out.usage_at = out.start;
            let typed = &input[out.start..cursor];
            out.list = matching(list, typed, true);
            if !usage.is_empty() {
                out.usage = Some(usage);
            }
        }
    }
    Some(out)
}

fn plugin(src: &dyn Source, name: &str) -> bool {
    src.plugin_commands().iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl Source for Fake {
        fn modules(&self) -> Vec<ModuleInfo> {
            ["Kill Aura", "Crystal Aura", "Auto Totem", "Flight"]
                .iter()
                .map(|n| ModuleInfo { name: (*n).into(), about: "about".into(), on: *n == "Flight", key: None })
                .collect()
        }
        fn find_module(&self, word: &str) -> Option<ModuleInfo> {
            self.modules().into_iter().find(|m| norm(&m.name) == norm(word))
        }
        fn settings(&self, module: &str) -> Vec<SettingInfo> {
            if module != "Kill Aura" {
                return Vec::new();
            }
            vec![
                SettingInfo { name: "Range".into(), kind: Kind::Number { now: 3.0, min: 1.0, max: 6.0, step: 0.1 } },
                SettingInfo { name: "Attack range".into(), kind: Kind::Number { now: 3.0, min: 1.0, max: 6.0, step: 0.1 } },
                SettingInfo { name: "Mode".into(), kind: Kind::Pick { now: 0, of: vec!["Single".into(), "Swap attack".into()] } },
                SettingInfo { name: "Players".into(), kind: Kind::Bool(true) },
            ]
        }
        fn profiles(&self) -> Vec<String> {
            vec!["pvp".into(), "pvp two".into()]
        }
        fn imports(&self, _: &str) -> Vec<String> {
            Vec::new()
        }
        fn presets(&self) -> Vec<(String, String)> {
            vec![("default".into(), "defaults".into())]
        }
        fn friends(&self) -> Vec<String> {
            vec!["Alex".into()]
        }
        fn players(&self) -> Vec<String> {
            vec!["Alex".into(), "Steve".into()]
        }
        fn waypoints(&self) -> Vec<String> {
            vec!["home".into()]
        }
        fn loadouts(&self) -> Vec<String> {
            vec!["Loadout 1".into()]
        }
        fn plugin_commands(&self) -> Vec<(String, String)> {
            vec![("testplugin".into(), "the test plugin".into())]
        }
        fn position(&self) -> Option<[i32; 3]> {
            Some([10, 64, -3])
        }
    }

    fn at_end(input: &str) -> Completion {
        complete(input, input.len(), &Fake).expect("a Sunset command")
    }

    fn texts(c: &Completion) -> Vec<&str> {
        c.list.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn a_dot_offers_every_command() {
        let c = at_end(".");
        assert_eq!(c.start, 1);
        assert_eq!(c.list.len(), COMMANDS.len() + 1);
        assert!(texts(&c).contains(&"toggle") && texts(&c).contains(&"testplugin"));
        assert!(c.error.is_none());
    }

    #[test]
    fn not_a_command() {
        assert!(complete("hello", 5, &Fake).is_none());
        assert!(complete("..t", 3, &Fake).is_none());
    }

    #[test]
    fn command_names_by_start_and_alias() {
        assert_eq!(texts(&at_end(".tog")), ["toggle"]);
        assert_eq!(texts(&at_end(".wa")), ["waypoint"]);
        assert_eq!(at_end(".waypoints").usage.as_deref(), Some("waypoints add|remove|list [name]"));
        let wp = at_end(".wp");
        assert!(wp.list.is_empty());
        assert_eq!(wp.usage.as_deref(), Some("wp add|remove|list [name]"));
        assert!(texts(&at_end(".s")).starts_with(&["scan", "seed", "set", "settings"]));
        let bad = at_end(".zzz");
        assert!(bad.list.is_empty());
        assert_eq!(bad.error.as_deref(), Some("Unknown command .zzz"));
    }

    #[test]
    fn modules_by_start_or_by_a_word() {
        let c = at_end(".t ");
        assert_eq!(texts(&c), ["autototem", "crystalaura", "flight", "killaura"]);
        assert_eq!(c.start, 3);
        assert_eq!(texts(&at_end(".t aura")), ["crystalaura", "killaura"]);
        assert_eq!(texts(&at_end(".toggle Kill")), ["killaura"]);
        let bad = at_end(".t nothing");
        assert_eq!(bad.error.as_deref(), Some("No module called nothing"));
        // A whole name is read: no list, no error.
        let whole = at_end(".t killaura");
        assert!(whole.list.is_empty() && whole.error.is_none());
    }

    #[test]
    fn set_offers_settings_then_values() {
        assert_eq!(texts(&at_end(".set killaura ")), ["range", "attackrange", "mode", "players"]);
        assert_eq!(texts(&at_end(".set killaura att")), ["attackrange"]);
        // A two-word setting typed with its space: offered again from its first word.
        let c = at_end(".set killaura attack ra");
        assert_eq!(texts(&c), ["attackrange"]);
        assert_eq!(c.start, ".set killaura ".len());
        assert_eq!(texts(&at_end(".set killaura mode ")), ["Single", "Swap attack"]);
        assert_eq!(texts(&at_end(".set killaura mode swap")), ["Swap attack"]);
        let value = at_end(".set killaura mode swap at");
        assert_eq!(texts(&value), ["Swap attack"]);
        assert_eq!(value.start, ".set killaura mode ".len());
        assert_eq!(texts(&at_end(".set killaura players ")), ["on", "off", "toggle"]);
        let number = at_end(".set killaura range ");
        assert!(number.list.is_empty());
        assert_eq!(number.usage.as_deref(), Some("<number 1 to 6> (now 3)"));
        let two = at_end(".set killaura attack range ");
        assert_eq!(two.usage.as_deref(), Some("<number 1 to 6> (now 3)"));
        assert_eq!(at_end(".set nothing ").error.as_deref(), Some("No module called nothing"));
        assert_eq!(at_end(".set killaura mode zzz").error.as_deref(), Some("Mode is one of Single, Swap attack, not zzz"));
    }

    #[test]
    fn profiles_friends_and_coordinates() {
        assert_eq!(texts(&at_end(".profile load ")), ["before-load", "pvp", "pvp two"]);
        let two = at_end(".profile load pvp t");
        assert_eq!(texts(&two), ["pvp two"]);
        assert_eq!(two.start, ".profile load ".len());
        assert_eq!(texts(&at_end(".friend add ")), ["Steve"]);
        assert_eq!(texts(&at_end(".friend remove ")), ["Alex"]);
        assert_eq!(texts(&at_end(".goto ")), ["10 64 -3", "10 64", "10", "stop"]);
        assert_eq!(texts(&at_end(".goto 10 ")), ["64 -3", "64"]);
        assert_eq!(at_end(".goto 10 x ").error.as_deref(), Some("x is not a whole block coordinate"));
        assert_eq!(at_end(".vclip ").usage.as_deref(), Some("<blocks>"));
    }

    #[test]
    fn plugin_commands_show_their_line() {
        let c = at_end(".testplugin ");
        assert_eq!(c.usage.as_deref(), Some("the test plugin"));
        assert!(c.error.is_none());
    }

    // Every name runtime_commands.rs `run` matches has its grammar here.
    #[test]
    fn every_command_run_reads() {
        let run = "relog seed auradebug elytratarget antibotdebug scan vanish staffcheck goto baritone b help commands \
                   menu gui pages toggle t modules list settings get set movementpreset bind binds panic debug profile \
                   profiles config cfg preset presets reset friend friends f loadout loadouts waypoint waypoints wp \
                   vclip hclip plugins";
        for name in run.split_whitespace() {
            assert!(command(name).is_some(), ".{name} has no grammar");
        }
        assert_eq!(run.split_whitespace().count(), COMMANDS.iter().map(|c| 1 + c.aliases.len()).sum::<usize>());
    }

    #[test]
    fn a_longer_setting_beside_a_shorter_one() {
        struct Walls;
        impl Source for Walls {
            fn modules(&self) -> Vec<ModuleInfo> {
                vec![ModuleInfo { name: "ESP".into(), about: String::new(), on: false, key: None }]
            }
            fn find_module(&self, word: &str) -> Option<ModuleInfo> {
                self.modules().into_iter().find(|m| norm(&m.name) == norm(word))
            }
            fn settings(&self, _: &str) -> Vec<SettingInfo> {
                let n = Kind::Number { now: 64.0, min: 0.0, max: 256.0, step: 1.0 };
                vec![
                    SettingInfo { name: "Range".into(), kind: n },
                    SettingInfo { name: "Range walls".into(), kind: Kind::Bool(false) },
                ]
            }
            fn profiles(&self) -> Vec<String> { Vec::new() }
            fn imports(&self, _: &str) -> Vec<String> { Vec::new() }
            fn presets(&self) -> Vec<(String, String)> { Vec::new() }
            fn friends(&self) -> Vec<String> { Vec::new() }
            fn players(&self) -> Vec<String> { Vec::new() }
            fn waypoints(&self) -> Vec<String> { Vec::new() }
            fn loadouts(&self) -> Vec<String> { Vec::new() }
            fn plugin_commands(&self) -> Vec<(String, String)> { Vec::new() }
            fn position(&self) -> Option<[i32; 3]> { None }
        }
        let c = |input: &str| complete(input, input.len(), &Walls).unwrap();
        assert_eq!(c(".set esp range ").usage.as_deref(), Some("<whole number 0 to 256> (now 64)"));
        let walls = c(".set esp range wa");
        assert_eq!(walls.list.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["rangewalls"]);
        assert_eq!(walls.start, ".set esp ".len());
        assert_eq!(c(".set esp range walls ").list.len(), 3);
        assert!(c(".set esp range 12").error.is_none());
    }

    #[test]
    fn cursor_inside_the_input() {
        // Only the text before the cursor counts.
        let input = ".t kill more";
        let c = complete(input, ".t kill".len(), &Fake).unwrap();
        assert_eq!(texts(&c), ["killaura"]);
        assert_eq!((c.start, c.end), (3, 7));
    }
}
