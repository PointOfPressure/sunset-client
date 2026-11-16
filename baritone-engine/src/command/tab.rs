// baritone.api.command.helpers.TabCompleteHelper, each command's tabComplete and the datatypes': what the
// chat box offers when TAB is pressed on a `#line`. The game's chat screen asks `suggest` through the
// bridge, with no access to the game: what the completions read is published by the tick (see `publish`).
use super::baritone_cached_world::BLOCKS_TO_KEEP_TRACK_OF;
use super::baritone_command::{registry, relative_coordinate};
use super::baritone_settings::{self as settings, Kind, DEFS};
use super::baritone_waypoints::Tag;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A block's properties and each one's values, for `oak_stairs[facing=`.
pub type BlockProperties = HashMap<String, Vec<(String, Vec<String>)>>;

#[derive(Default)]
pub struct Data {
    pub prefix: String,
    /// `minecraft:stone`, sorted.
    pub blocks: Vec<String>,
    pub items: Vec<String>,
    pub mobs: Vec<String>,
    pub properties: BlockProperties,
    /// Newest first, the ones with a name.
    pub waypoints: Vec<String>,
    pub players: Vec<String>,
    /// The folder of the settings files.
    pub settings_dir: PathBuf,
    /// The folder `#build` and `#explorefilter` read from.
    pub schematics_dir: PathBuf,
    pub settings: settings::Settings,
}

thread_local! {
    static DATA: RefCell<Data> = RefCell::new(Data::default());
}

/// Refresh what the completions read.
pub fn publish(fill: impl FnOnce(&mut Data)) {
    DATA.with_borrow_mut(fill);
}

pub fn needs_names() -> bool {
    DATA.with_borrow(|d| d.blocks.is_empty())
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// `filterPrefix`: ignoring case.
fn filter_prefix(list: Vec<String>, word: &str) -> Vec<String> {
    let word = word.to_lowercase();
    list.into_iter().filter(|s| s.to_lowercase().starts_with(&word)).collect()
}

/// `filterPrefixNamespaced`: `dia` finds `minecraft:diamond_ore`, and so does `minecraft:dia`; what no id can start with finds nothing.
fn filter_namespaced(list: &[String], word: &str) -> Vec<String> {
    let valid = word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:".contains(c)) && word.matches(':').count() <= 1;
    if !valid {
        return Vec::new();
    }
    let mut found: Vec<String> = list
        .iter()
        .filter(|id| id.starts_with(word) || (!word.contains(':') && id.split_once(':').is_some_and(|(_, path)| path.starts_with(word))))
        .cloned()
        .collect();
    found.sort();
    found
}

/// `RelativeCoordinate.tabComplete`.
fn coordinate(word: &str) -> Vec<String> {
    if word.is_empty() || word == "~" { strings(&["~"]) } else { Vec::new() }
}

/// `RelativeBlockPos.tabComplete` and its kin: only while every word before the last is a coordinate, up to `max` words.
fn coordinates(words: &[&str], max: usize) -> Vec<String> {
    let Some((last, before)) = words.split_last() else {
        return Vec::new();
    };
    if words.len() > max || !before.iter().all(|w| relative_coordinate(w, 0.0).is_some()) {
        return Vec::new();
    }
    coordinate(last)
}

/// `RelativeFile.tabComplete`: the files and folders where `word` points, under `base`; none with a space in the name.
pub fn relative_file(word: &str, base: &Path) -> Vec<String> {
    let given = Path::new(word);
    let absolute = given.is_absolute();
    let at = if absolute { given.to_path_buf() } else { base.join(word) };
    let ends_in_separator = word.ends_with(std::path::MAIN_SEPARATOR) || word.ends_with('/');
    let folder = if word.is_empty() || ends_in_separator { at.as_path() } else { at.parent().unwrap_or(base) };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let lower = word.to_lowercase();
    let mut found: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let shown = if absolute { path.clone() } else { path.strip_prefix(base).ok()?.to_path_buf() };
            let mut text = shown.to_str()?.to_owned();
            if path.is_dir() {
                text.push(std::path::MAIN_SEPARATOR);
            }
            Some(text)
        })
        .filter(|s| s.to_lowercase().starts_with(&lower) && !s.contains(' '))
        .collect();
    found.sort();
    found
}

/// `ForBlockOptionalMeta.tabComplete`: the id, then the property names not used yet, then a property's values.
fn block_optional_meta(word: &str, d: &Data) -> Vec<String> {
    if word.ends_with(']') {
        return Vec::new();
    }
    let Some((id, properties)) = word.split_once('[') else {
        return filter_namespaced(&d.blocks, word);
    };
    let full = if id.contains(':') { id.to_owned() } else { format!("minecraft:{id}") };
    let Some(known) = d.properties.get(&full) else {
        return Vec::new();
    };
    let (leading, last) = properties.rsplit_once(',').unwrap_or(("", properties));
    let mut found: Vec<String> = match last.split_once('=') {
        None => {
            let used: Vec<&str> = leading.split(',').map(|pair| pair.split('=').next().unwrap_or("")).collect();
            let prefix = &word[..word.len() - last.len()];
            let names = known.iter().map(|(name, _)| name.clone()).filter(|name| !used.contains(&name.as_str())).collect();
            filter_prefix(names, last).into_iter().map(|name| format!("{prefix}{name}")).collect()
        }
        Some((name, value)) => {
            let prefix = &word[..word.len() - value.len()];
            let Some((_, values)) = known.iter().find(|(n, _)| n == name) else {
                return Vec::new();
            };
            filter_prefix(values.clone(), value).into_iter().map(|v| format!("{prefix}{v}")).collect()
        }
    };
    found.sort();
    found
}

/// `ForWaypoints.tabComplete`: the tags first.
fn for_waypoints(word: &str, d: &Data) -> Vec<String> {
    let mut names: Vec<String> = Tag::ALL.iter().flat_map(|t| t.names().iter().map(|s| (*s).to_owned())).collect();
    let mut waypoints = d.waypoints.clone();
    waypoints.sort();
    names.extend(waypoints);
    filter_prefix(names, word)
}

/// The value a setting completes to: `true` and `false` with the current one first, otherwise what it holds now.
fn setting_value(def: &settings::Def, word: &str, d: &Data) -> Vec<String> {
    if def.kind == Kind::Bool {
        let list = if d.settings.flag(def.name) { ["true", "false"] } else { ["false", "true"] };
        return filter_prefix(strings(&list), word);
    }
    vec![d.settings.get(def.name).to_owned()]
}

/// `SetCommand.tabComplete`.
fn set_candidates(args: &[&str], d: &Data) -> Vec<String> {
    let word = args.last().copied().unwrap_or("");
    if args.len() == 1 {
        let mut names: Vec<String> = DEFS.iter().map(|def| def.name.to_owned()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        let mut all = strings(&["list", "modified", "reset", "toggle", "save", "load"]);
        all.extend(names);
        return filter_prefix(all, word);
    }
    if args.len() != 2 || word.eq_ignore_ascii_case("s") || word.eq_ignore_ascii_case("save") {
        return Vec::new();
    }
    let first = args[0].to_lowercase();
    match first.as_str() {
        "reset" => {
            let mut all = strings(&["all"]);
            all.extend(DEFS.iter().filter(|def| d.settings.modified(def)).map(|def| def.name.to_owned()));
            filter_prefix(all, word)
        }
        "toggle" => filter_prefix(DEFS.iter().filter(|def| def.kind == Kind::Bool).map(|def| def.name.to_owned()).collect(), word),
        "ld" | "load" => relative_file(word, &d.settings_dir),
        _ => settings::find(&first).map_or_else(Vec::new, |def| setting_value(def, word, d)),
    }
}

const FILL_ACTIONS: &[&str] = &["set", "fill", "s", "f", "walls", "w", "shell", "shl", "sphere", "sph", "hsphere", "hsph", "cylinder", "cyl", "hcylinder", "hcyl", "replace", "r"];
const AXES: &[&str] = &["x", "y", "z"];
const DIRECTIONS: &[&str] = &["down", "up", "north", "south", "west", "east"];

/// `SelCommand.tabComplete`.
fn sel_candidates(args: &[&str], d: &Data) -> Vec<String> {
    let word = args.last().copied().unwrap_or("");
    let action = args[0].to_lowercase();
    let action = action.as_str();
    if args.len() == 1 {
        let mut names = strings(&[
            "pos1", "p1", "1", "pos2", "p2", "2", "clear", "c", "undo", "u", "set", "fill", "s", "f", "walls", "w", "shell", "shl", "sphere", "sph",
            "hsphere", "hsph", "cylinder", "cyl", "hcylinder", "hcyl", "cleararea", "ca", "replace", "r", "expand", "ex", "copy", "cp", "paste", "p",
            "contract", "ct", "shift", "sh",
        ]);
        names.sort();
        return filter_prefix(names, word);
    }
    let rest = &args[1..];
    match action {
        "pos1" | "p1" | "1" | "pos2" | "p2" | "2" => coordinates(rest, 3),
        a if FILL_ACTIONS.contains(&a) => {
            if rest.len() == 1 || matches!(a, "replace" | "r") {
                block_optional_meta(word, d)
            } else if rest.len() == 2 && matches!(a, "cylinder" | "cyl" | "hcylinder" | "hcyl") {
                filter_prefix(strings(AXES), word)
            } else {
                Vec::new()
            }
        }
        "expand" | "ex" | "contract" | "ct" | "shift" | "sh" => match rest.len() {
            1 => {
                let mut names = strings(&["all", "a", "newest", "n", "oldest", "o"]);
                names.sort();
                filter_prefix(names, word)
            }
            2 => filter_prefix(strings(DIRECTIONS), word),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// `WaypointsCommand.tabComplete`.
fn waypoint_candidates(args: &[&str], d: &Data) -> Vec<String> {
    let word = args.last().copied().unwrap_or("");
    if args.len() == 1 {
        let mut names = strings(&["list", "get", "l", "clear", "c", "save", "s", "info", "show", "i", "delete", "d", "restore", "goal", "goto", "g"]);
        names.sort();
        return filter_prefix(names, word);
    }
    let action = args[0].to_lowercase();
    let saving = matches!(action.as_str(), "save" | "s");
    if args.len() == 2 {
        return match action.as_str() {
            "list" | "get" | "l" | "clear" | "c" | "save" | "s" => {
                let mut tags: Vec<String> = Tag::ALL.iter().flat_map(|t| t.names().iter().map(|s| (*s).to_owned())).collect();
                tags.sort();
                filter_prefix(tags, word)
            }
            "restore" => Vec::new(),
            _ => for_waypoints(word, d),
        };
    }
    if saving && args.len() >= 3 {
        return coordinates(&args[2..], 3);
    }
    Vec::new()
}

/// `FollowCommand.tabComplete`.
fn follow_candidates(args: &[&str], d: &Data) -> Vec<String> {
    let word = args.last().copied().unwrap_or("");
    if args.len() == 1 {
        return filter_prefix(strings(&["entities", "players", "entity", "player"]), word);
    }
    match args[0].to_lowercase().as_str() {
        "entity" => filter_namespaced(&d.mobs, word),
        "player" => {
            let mut names = filter_prefix(d.players.clone(), word);
            names.sort();
            names
        }
        _ => Vec::new(),
    }
}

/// The line up to the cursor in; the byte where its last word starts and the candidates for that word out.
/// None when the line is not for Baritone.
pub fn suggest(line: &str) -> Option<(usize, Vec<String>)> {
    DATA.with_borrow(|d| {
        if d.prefix.is_empty() || !d.settings.flag("prefixControl") {
            return None;
        }
        let rest = line.strip_prefix(d.prefix.as_str())?;
        // CommandArguments.from(msg, true): spaces only separate, and a line that ends in one has an empty last word.
        let mut words: Vec<&str> = rest.split(' ').filter(|w| !w.is_empty()).collect();
        if rest.is_empty() || rest.ends_with(' ') {
            words.push("");
        }
        let word = *words.last()?;
        Some((line.len() - word.len(), candidates(&words, d)))
    })
}

/// `ExampleBaritoneControl.tabComplete`, then the command's own.
fn candidates(words: &[&str], d: &Data) -> Vec<String> {
    let word = *words.last().unwrap_or(&"");
    if words.len() == 1 {
        let mut names: Vec<String> = registry().iter().flat_map(|c| c.names.iter().map(|n| (*n).to_owned())).collect();
        names.extend(DEFS.iter().map(|def| def.name.to_owned()));
        names.sort();
        names.dedup();
        return filter_prefix(names, word);
    }
    if words.len() == 2 {
        if let Some(def) = settings::find(words[0]) {
            return setting_value(def, word, d);
        }
    }
    let first = words[0].to_ascii_lowercase();
    let commands = registry();
    let Some(command) = commands.iter().find(|c| c.names.contains(&first.as_str())) else {
        return Vec::new();
    };
    let args = &words[1..];
    if !command.alias.is_empty() {
        // CommandAlias.tabComplete: what it stands for, with these words after it.
        let expanded: Vec<&str> = command.alias.split(' ').chain(args.iter().copied()).collect();
        return candidates(&expanded, d);
    }
    let n = args.len();
    match command.names[0] {
        "help" if n == 1 => {
            let mut names: Vec<String> = registry().iter().flat_map(|c| c.names.iter().map(|s| (*s).to_owned())).collect();
            names.dedup();
            filter_prefix(names, word)
        }
        "set" => set_candidates(args, d),
        "mine" => block_optional_meta(word, d),
        "find" => {
            let tracked: Vec<String> = BLOCKS_TO_KEEP_TRACK_OF.iter().map(|b| format!("minecraft:{b}")).collect();
            filter_namespaced(&tracked, word)
        }
        "goto" if n == 1 => filter_namespaced(&d.blocks, word),
        "pickup" => filter_namespaced(&d.items, word),
        "sel" => sel_candidates(args, d),
        "waypoints" => waypoint_candidates(args, d),
        "follow" => follow_candidates(args, d),
        "elytra" if n == 1 => filter_prefix(strings(&["reset", "repack", "supported"]), word),
        "goal" => {
            if n == 1 {
                filter_prefix(strings(&["reset", "clear", "none", "~"]), word)
            } else {
                coordinates(args, 3)
            }
        }
        "explore" => coordinates(args, 2),
        "explorefilter" if n == 1 => relative_file(word, &d.schematics_dir),
        "build" => {
            if n == 1 {
                relative_file(word, &d.schematics_dir)
            } else {
                coordinates(&args[1..], 3)
            }
        }
        _ => Vec::new(),
    }
}
