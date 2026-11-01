// Extras: the things that are not cheats (Unlock All Recipes works in recipes.rs). Client Sort sorts with Horizon's
// own sort (its button, key or mouse button) in the order ClientSort's modifiers pick: plain, Shift, Ctrl and Alt
// each choose one, with items pinned to the start or the end, the hotbar sorted too, and the clicks paced over ticks.

use super::sunset::{Category, Module};
use super::sunset_util::{listed, short};
use super::Game;
use std::sync::Mutex;
use winit::keyboard::KeyCode;

// A key past any real one (the group goes in the bits above it).
const LAST: usize = 1 << 40;

const ORDERS: &[&str] = &["Creative", "Quantity", "Alphabet", "Raw id"];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Unlock All Recipes", Category::Extras,
            "The recipe book shows every recipe as unlocked (from Horizon's own recipe data; on servers, which send only yours). A recipe the server has given you is placed as usual; any other one is laid in the crafting grid with ordinary clicks from your inventory. A server with doLimitedCrafting on still gives no result for those."),
        Module::new("Client Sort", Category::Extras,
            "Horizon's sort buttons, key and mouse button (it turns the buttons on) sort in the order you pick: plain, with Shift, Ctrl or Alt held. Start and End items always go first and last.")
            .pick("Order", ORDERS)
            .pick("Shift order", &["Quantity", "Creative", "Alphabet", "Raw id"])
            .pick("Ctrl order", &["Alphabet", "Creative", "Quantity", "Raw id"])
            .pick("Alt order", &["Raw id", "Creative", "Quantity", "Alphabet"])
            .bool("Hotbar", true)
            .text("Start items", "")
            .text("End items", "")
            .num("Clicks per tick", 8.0, 1.0, 64.0, 1.0),
    ]
}

// The clicks still to send (slot ids), so a sort is spread over ticks.
static QUEUE: Mutex<Vec<usize>> = Mutex::new(Vec::new());

fn queue<R>(f: impl FnOnce(&mut Vec<usize>) -> R) -> R {
    f(&mut QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
}

// Horizon's sort of `slots` (its button, key or mouse button), done Client Sort's way. True when it took it.
pub(super) fn sort(game: &mut Game<'_>, slots: &[usize]) -> bool {
    if !game.native_on(super::sunset::ID) {
        return false;
    }
    let Some(m) = super::sunset::with(|s| s.module("Client Sort").filter(|m| m.on).cloned()) else {
        return false;
    };
    if game.carried.is_some() || !queue(|q| q.is_empty()) {
        return true;
    }
    let alt = game.physical_keys.contains(&KeyCode::AltLeft) || game.physical_keys.contains(&KeyCode::AltRight);
    let order = m.p(if alt {
        "Alt order"
    } else if game.has_control_down() {
        "Ctrl order"
    } else if game.has_shift_down() {
        "Shift order"
    } else {
        "Order"
    });
    let mut clicks = plan(game, &m, order, slots);
    // The main inventory's sort takes the hotbar too, as a run of its own.
    if m.b("Hotbar") && slots.first() == Some(&9) {
        clicks.extend(plan(game, &m, order, &(0..9).collect::<Vec<_>>()));
    }
    queue(|q| *q = clicks);
    true
}

// The slot ids to left-click to sort `slots` in `order`.
fn plan(game: &mut Game<'_>, m: &Module, order: &str, slots: &[usize]) -> Vec<usize> {
    let views: Vec<Option<horizon_client::items::ItemStack>> = slots.iter().map(|&s| game.slot_view(s)).collect();
    let (start, end) = (m.t("Start items"), m.t("End items"));
    let pinned = |list: &str, item: &str| list.split(',').position(|x| short(x.trim()) == short(item));
    let totals = |item: &str| views.iter().flatten().filter(|s| &*s.item == item).map(|s| s.count as usize).sum::<usize>();
    let registry = &game.res.items().registry_order;
    let rank = |s: &horizon_client::items::ItemStack| -> horizon_client::inv_sort::Rank {
        let name = s.custom_name().unwrap_or_else(|| game.pack.names.display_item(&s.item));
        let creative = game.creative_screen.order_of(s).unwrap_or(LAST);
        // Start items, then the rest, then End items; within the rest the order's own key.
        let (group, key, text) = if let Some(i) = pinned(start, &s.item).filter(|_| listed(start, &s.item)) {
            (0, i, name)
        } else if let Some(i) = pinned(end, &s.item).filter(|_| listed(end, &s.item)) {
            (2, i, name)
        } else {
            match order {
                "Quantity" => (1, LAST - totals(&s.item).min(LAST), format!("{creative:012}")),
                "Alphabet" => (1, 0, name.to_lowercase()),
                "Raw id" => (1, registry.iter().position(|r| *r == s.item).unwrap_or(LAST), name),
                _ => (1, creative, name),
            }
        };
        (group << 56 | key, text)
    };
    let max = |s: &horizon_client::items::ItemStack| game.max_stack(s);
    horizon_client::inv_sort::plan(&views, &rank, &max).into_iter().map(|i| slots[i]).collect()
}

// Each tick: Horizon's sort buttons on while Client Sort is, and the next clicks of a sort; none once the screen it
// was for has closed.
pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    super::recipe_book::sunset_recipes::sync(game, mods.iter().any(|m| m.name == "Unlock All Recipes" && m.on));
    let on = mods.iter().find(|m| m.name == "Client Sort" && m.on);
    if on.is_some() {
        game.settings.extras.sort = true;
    }
    if game.screen != super::Screen::Inventory {
        queue(Vec::clear);
        return;
    }
    let per_tick = on.map_or(8, |m| m.n("Clicks per tick") as usize);
    let now: Vec<usize> = queue(|q| q.drain(..per_tick.min(q.len())).collect());
    for slot in now {
        game.tweak_click(slot, 0, horizon_client::menu::Input::Pickup);
    }
}
