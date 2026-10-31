// The list picker: a list setting (blocks, items or entities, comma separated) chosen from the game's registries in
// the settings window instead of typed. The chosen ones come first, then every other one the search matches; a click
// adds or takes one away. Names show readable (Diamond Ore), the setting keeps ids (diamond_ore).

use super::Game;
use super::sunset_util::short;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ListKind {
    Blocks,
    Items,
    Entities,
}

// What a text setting lists, by its name (and module, for the white and black lists); None: free text.
pub(super) fn list_kind(module: &str, setting: &str) -> Option<ListKind> {
    match setting {
        "Blocks" => Some(ListKind::Blocks),
        "Entities" => Some(ListKind::Entities),
        "Items" | "Item list" | "Selected weapon types" => Some(ListKind::Items),
        "Whitelist" | "Blacklist" if matches!(module, "Auto Eat" | "Auto Tool") => Some(ListKind::Items),
        "Whitelist" | "Blacklist" => Some(ListKind::Blocks),
        _ => None,
    }
}

// The open picker: every name it offers, sorted by readable name, and how far its list is scrolled.
#[derive(Clone, Debug)]
pub(super) struct Picker {
    pub(super) all: Arc<[Box<str>]>,
    searchable: Arc<[Box<str>]>,
    pub(super) scroll: f32,
}

// The entity groups the entity lists take (see the aura's list matching).
const GROUPS: [&str; 4] = ["@all", "@player", "@hostile", "@passive"];

pub(super) fn open(game: &Game<'_>, kind: ListKind) -> Picker {
    let names: Vec<&str> = match kind {
        ListKind::Blocks => game.res.facts().states.iter().map(|f| short(&f.name)).collect(),
        ListKind::Items => game.res.items().registry_order.iter().map(|n| short(n)).collect(),
        ListKind::Entities => std::iter::once("player")
            .chain(horizon_client::mobs::species::ALL.iter().map(|d| short(d.name)))
            .collect(),
    };
    from_names(names, kind)
}

fn from_names(mut names: Vec<&str>, kind: ListKind) -> Picker {
    // Many block states share one name. Deduplicate borrowed names before
    // allocating an owned ID or its cached readable search key.
    names.retain(|n| !matches!(*n, "air" | "cave_air" | "void_air"));
    names.sort();
    names.dedup();
    names.sort_by_cached_key(|n| readable(n));
    if kind == ListKind::Entities {
        names.splice(0..0, GROUPS);
    }
    let searchable = names.iter().map(|n| readable(n).to_lowercase().into_boxed_str()).collect();
    let all = names.into_iter().map(Into::into).collect();
    Picker { all, searchable, scroll: 0.0 }
}

// diamond_ore as Diamond Ore; a group as it is.
pub(super) fn readable(id: &str) -> String {
    if id.starts_with('@') {
        return id.to_owned();
    }
    let mut name = String::with_capacity(id.len());
    for word in short(id).split('_').filter(|w| !w.is_empty()) {
        if !name.is_empty() { name.push(' '); }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            name.extend(first.to_uppercase());
            name.push_str(chars.as_str());
        }
    }
    name
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

// The rows the picker shows: the chosen ones (true), then the rest that the search finds in the id or the readable
// name.
pub(super) fn rows<'a>(picker: &'a Picker, text: &'a str, search: &str) -> Vec<(&'a str, bool)> {
    let have = chosen(text);
    let search = search.trim().to_lowercase();
    have.iter()
        .map(|n| (*n, true))
        .chain(picker.all.iter().zip(picker.searchable.iter())
            .filter(|(id, readable)| !have.contains(&id.as_ref()) && (search.is_empty() || id.contains(&search) || readable.contains(&search)))
            .map(|(id, _)| (id.as_ref(), false)))
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
        let picker = from_names(vec!["coal_ore", "diamond_ore", "dirt"], ListKind::Blocks);
        assert_eq!(rows(&picker, "dirt", "ore"), [("dirt", true), ("coal_ore", false), ("diamond_ore", false)]);
        assert_eq!(rows(&picker, "", "Diamond O"), [("diamond_ore", false)]);
    }
}
