// The list picker: a list setting (blocks, items or entities, comma separated) chosen from the game's registries in
// the settings window instead of typed. The chosen ones come first, then every other one the search matches; a click
// adds or takes one away. Names show readable (Diamond Ore), the setting keeps ids (diamond_ore).

use super::Game;
use super::sunset::Shape;
use super::sunset_util::short;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ListKind {
    Blocks,
    Items,
    Entities,
    Particles,
    Players,
}

// What a text setting lists, by its name (and module, for the white and black lists); None: free text.
pub(super) fn list_kind(module: &str, setting: &str) -> Option<ListKind> {
    match setting {
        "Blocks" => Some(ListKind::Blocks),
        "Entities" => Some(ListKind::Entities),
        "Particles" => Some(ListKind::Particles),
        "Friends" => Some(ListKind::Players),
        "Items" | "Item list" | "Selected weapon types" => Some(ListKind::Items),
        "Whitelist" | "Blacklist" if matches!(module, "Auto Eat" | "Auto Tool") => Some(ListKind::Items),
        "Whitelist" | "Blacklist" => Some(ListKind::Blocks),
        _ => None,
    }
}

// The open picker: every name it offers, sorted by readable name, and how far its list is scrolled; or, for a
// short list, the groups of names it ticks on and off together.
#[derive(Clone, Debug)]
pub(super) struct Picker {
    pub(super) kind: ListKind,
    pub(super) all: Arc<[Box<str>]>,
    pub(super) groups: Option<&'static [&'static [&'static str]]>,
    pub(super) scroll: f32,
}

// Xray's Simple list: the ores and valuables, each row the blocks it stands for (the first names the row).
pub(super) const VALUABLES: &[&[&str]] = &[
    &["diamond_ore", "deepslate_diamond_ore"],
    &["ancient_debris"],
    &["emerald_ore", "deepslate_emerald_ore"],
    &["gold_ore", "deepslate_gold_ore"],
    &["nether_gold_ore"],
    &["iron_ore", "deepslate_iron_ore"],
    &["copper_ore", "deepslate_copper_ore"],
    &["redstone_ore", "deepslate_redstone_ore"],
    &["lapis_ore", "deepslate_lapis_ore"],
    &["coal_ore", "deepslate_coal_ore"],
    &["nether_quartz_ore"],
    &["spawner", "trial_spawner"],
    &["chest", "trapped_chest", "barrel"],
    &["ender_chest"],
    &["obsidian", "crying_obsidian"],
    &["budding_amethyst", "amethyst_cluster"],
];

// The entity groups the entity lists take (see the aura's list matching).
const GROUPS: [&str; 4] = ["@all", "@player", "@hostile", "@passive"];

// The short list a picker of groups offers.
pub(super) fn short_list(groups: &'static [&'static [&'static str]]) -> Picker {
    Picker { kind: ListKind::Blocks, all: Arc::new([]), groups: Some(groups), scroll: 0.0 }
}

pub(super) fn open(game: &Game<'_>, kind: ListKind) -> Picker {
    let mut names: Vec<Box<str>> = match kind {
        ListKind::Blocks => game.res.facts().states.iter().map(|f| short(&f.name).into()).collect(),
        ListKind::Items => game.res.items().registry_order.iter().map(|n| short(n).into()).collect(),
        ListKind::Entities => std::iter::once("player")
            .chain(horizon_client::mobs::species::ALL.iter().map(|d| short(d.name)))
            .map(Into::into)
            .collect(),
        ListKind::Particles => horizon_client::particles::sunset_particles::names().into_iter().map(Into::into).collect(),
        ListKind::Players => online().into_iter().map(|(name, _)| name.into()).collect(),
    };
    names.retain(|n| !matches!(&**n, "air" | "cave_air" | "void_air"));
    names.sort();
    names.dedup();
    if kind != ListKind::Players {
        names.sort_by_cached_key(|n| readable(n));
    }
    if kind == ListKind::Entities {
        names.splice(0..0, GROUPS.iter().map(|g| (*g).into()));
    }
    Picker { kind, all: names.into(), groups: None, scroll: 0.0 }
}

const STEVE: &str = "entity/player/wide/steve";

// The players online and the skin each one's face is drawn from, as the menu last saw them (it looks while a
// player picker is open).
static ONLINE: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

fn online() -> Vec<(String, String)> {
    ONLINE.lock().map(|o| o.clone()).unwrap_or_default()
}

// The players in the server's list, their skins loaded for their faces (as the tab list does). The picker's names
// follow them.
pub(super) fn see_players(game: &mut Game<'_>, picker: &mut Option<Picker>) {
    let Some(p) = picker.as_mut().filter(|p| p.kind == ListKind::Players) else {
        return;
    };
    let players = game.remote_tab_players().map(|(list, _, _)| list).unwrap_or_default();
    let seen: Vec<(String, String)> = players.into_iter().map(|t| (t.name.clone(), game.face_skin(t.uuid, &t.skin))).collect();
    p.all = seen.iter().map(|(n, _)| n.as_str().into()).collect();
    if let Ok(mut o) = ONLINE.lock() {
        *o = seen;
    }
}

// A row's icon: a block or an item as the game draws it in its menus (a block with no item of its own as the item
// that places it: a wall torch as a torch, water as a bucket of it), a mob as its spawn egg, a player as a face.
pub(super) fn icon(game: &Game<'_>, picker: &Picker, name: &str, at: [f32; 2], size: f32) -> Option<Shape> {
    let items = game.res.items();
    let item = |id: &str| items.items.get_key_value(format!("minecraft:{id}").as_str()).map(|(k, _)| k.clone());
    let found = match picker.kind {
        _ if name.starts_with('@') => None,
        ListKind::Entities if name == "player" => {
            return Some(Shape::Face(STEVE.into(), at, size));
        }
        ListKind::Entities => item(&format!("{name}_spawn_egg")),
        ListKind::Items => item(name),
        ListKind::Particles => None,
        ListKind::Players => {
            let skin = online().into_iter().find(|(n, _)| n == name).map_or_else(|| STEVE.to_owned(), |(_, skin)| skin);
            return Some(Shape::Face(skin, at, size));
        }
        ListKind::Blocks => item(name)
            .or_else(|| item(&name.replace("_wall_", "_").replace("wall_", "")))
            .or_else(|| name.strip_prefix("potted_").and_then(item))
            .or_else(|| matches!(name, "water" | "lava" | "powder_snow").then(|| item(&format!("{name}_bucket"))).flatten()),
    };
    found.map(|id| Shape::Item(id, at, size))
}

// diamond_ore as Diamond Ore; a group as it is.
pub(super) fn readable(id: &str) -> String {
    if id.starts_with('@') {
        return id.to_owned();
    }
    short(id)
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// The names in a list setting's text.
pub(super) fn chosen(text: &str) -> Vec<&str> {
    text.split(',').map(|n| short(n.trim())).filter(|n| !n.is_empty()).collect()
}

// The list with `name` added, or taken away if it was in.
pub(super) fn toggled(text: &str, name: &str) -> String {
    let mut list = chosen(text);
    match list.iter().position(|n| *n == name) {
        Some(i) => {
            list.remove(i);
        }
        None => list.push(name),
    }
    list.join(",")
}

// The list after a click on a row: the name, or its whole group, added or taken away.
pub(super) fn clicked(picker: &Picker, text: &str, name: &str) -> String {
    let Some(group) = picker.groups.and_then(|g| g.iter().find(|g| g[0] == name)) else {
        return toggled(text, name);
    };
    let mut list = chosen(text);
    if group.iter().all(|n| list.contains(n)) {
        list.retain(|n| !group.contains(n));
    } else {
        list.extend(group.iter().filter(|n| !chosen(text).contains(n)));
    }
    list.join(",")
}

// The rows the picker shows: the chosen ones (true), then the rest that the search finds in the id or the readable
// name; a list of groups keeps its order, a group chosen when all of it is.
pub(super) fn rows<'a>(picker: &'a Picker, text: &'a str, search: &'a str) -> Vec<(&'a str, bool)> {
    let have = chosen(text);
    let typed = search.trim();
    let lower = typed.to_lowercase();
    let found = |n: &str| lower.is_empty() || n.to_lowercase().contains(&lower) || readable(n).to_lowercase().contains(&lower);
    if let Some(groups) = picker.groups {
        return groups.iter().filter(|g| found(g[0])).map(|g| (g[0], g.iter().all(|n| have.contains(n)))).collect();
    }
    // A player's name typed in the search can be added when no row is that name.
    let other = |n: &str| !n.eq_ignore_ascii_case(typed);
    let named = picker.kind == ListKind::Players
        && !typed.is_empty()
        && !typed.contains(',')
        && have.iter().all(|n| other(n))
        && picker.all.iter().all(|n| other(n));
    have.iter()
        .map(|n| (*n, true))
        .chain(named.then_some((typed, false)))
        .chain(picker.all.iter().map(|n| &**n).filter(|n| !have.contains(n) && found(n)).map(|n| (n, false)))
        .collect()
}

pub(super) const ROW: f32 = 12.0;

// The picker's box, its list's area and its Done button.
pub(super) fn rect(w: f32, h: f32) -> [f32; 4] {
    let width = (w - 8.0).clamp(1.0, 300.0);
    let height = (h - 16.0).clamp(1.0, 280.0);
    [(w - width) / 2.0, ((h - height) / 2.0).max(0.0), width, height]
}

pub(super) fn list_area(r: [f32; 4]) -> [f32; 4] {
    [r[0] + 6.0, r[1] + 42.0, r[2] - 12.0, (r[3] - 64.0).max(0.0)]
}

pub(super) fn done_button(r: [f32; 4]) -> [f32; 4] {
    [r[0] + r[2] - 62.0, r[1] + r[3] - 19.0, 56.0, 15.0]
}

// How far the list can scroll.
pub(super) fn max_scroll(count: usize, r: [f32; 4]) -> f32 {
    (count as f32 * ROW - list_area(r)[3]).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_and_lists_toggle() {
        assert_eq!(readable("minecraft:diamond_ore"), "Diamond Ore");
        assert_eq!(readable("@hostile"), "@hostile");
        assert_eq!(toggled("obsidian, minecraft:anvil", "anvil"), "obsidian");
        assert_eq!(toggled("obsidian", "cobweb"), "obsidian,cobweb");
        assert_eq!(toggled("", "cobweb"), "cobweb");
    }

    #[test]
    fn chosen_first_then_what_the_search_finds() {
        let picker = Picker { kind: ListKind::Blocks, all: ["coal_ore", "diamond_ore", "dirt"].map(Into::into).into(), groups: None, scroll: 0.0 };
        assert_eq!(rows(&picker, "dirt", "ore"), [("dirt", true), ("coal_ore", false), ("diamond_ore", false)]);
        assert_eq!(rows(&picker, "", "Diamond O"), [("diamond_ore", false)]);
    }

    #[test]
    fn a_group_ticks_on_and_off_together() {
        let picker = short_list(&[&["coal_ore", "deepslate_coal_ore"], &["spawner"]]);
        assert_eq!(rows(&picker, "coal_ore,spawner", ""), [("coal_ore", false), ("spawner", true)]);
        assert_eq!(clicked(&picker, "coal_ore,spawner", "coal_ore"), "coal_ore,spawner,deepslate_coal_ore");
        assert_eq!(clicked(&picker, "coal_ore,deepslate_coal_ore,dirt", "coal_ore"), "dirt");
    }
}
