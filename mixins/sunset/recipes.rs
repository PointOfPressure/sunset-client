// Unlock All Recipes (Extras): the recipe book shows every recipe of Horizon's own recipe data as unlocked; it lives
// next to the book in recipe_book.rs. A server takes the book's place-recipe click only for recipes it put in your
// book, so a click on any other one lays it in the grid with ordinary slot clicks from your inventory instead.
use super::super::Game;
use horizon_client::menu::Input::{Pickup, QuickMove};
use horizon_protocol::packets::play::Clientbound as C;
use std::collections::HashSet;
use std::sync::Mutex;

#[derive(Default)]
struct Book {
    // The displays the server put in your book, as its packets said.
    server: HashSet<i32>,
    // The client book's version once every display was in it (None: not shown, or shown no more).
    shown: Option<u64>,
}

static BOOK: Mutex<Option<Book>> = Mutex::new(None);

fn book<R>(f: impl FnOnce(&mut Book) -> R) -> R {
    f(BOOK.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Book::default))
}

pub(crate) fn packet(packet: &C) {
    book(|b| match packet {
        C::Login(_) => b.server.clear(),
        C::RecipeBookAdd(p) => {
            if p.replace {
                b.server.clear();
            }
            b.server.extend(p.entries.iter().map(|e| e.id));
        }
        C::RecipeBookRemove(p) => {
            for id in &p.ids {
                b.server.remove(id);
            }
        }
        _ => {}
    });
}

// Each tick on a server: every display in the client's book while on; back to the server's own once off.
pub(crate) fn sync(game: &mut Game<'_>, on: bool) {
    if game.remote.is_none() {
        return;
    }
    let version = game.recipe_ui.client_version;
    if on {
        if book(|b| b.shown) == Some(version) {
            return;
        }
        game.ensure_book();
        let known: HashSet<i32> = game.recipe_ui.client.displays().into_iter().collect();
        let all: Vec<i32> = game.recipe_ui.book.as_ref().map_or_else(Vec::new, |b| b.entries.iter().map(|e| e.display_id).collect());
        let missing: Vec<i32> = all.into_iter().filter(|d| *d >= 0 && !known.contains(d)).collect();
        let ui = &mut game.recipe_ui;
        if !missing.is_empty() {
            for d in missing {
                ui.client.add(d);
            }
            ui.client_version += 1;
        }
        book(|b| b.shown = Some(ui.client_version));
    } else if book(|b| b.shown.take()).is_some() {
        let server = book(|b| b.server.clone());
        let ui = &mut game.recipe_ui;
        for d in ui.client.displays().into_iter().filter(|d| !server.contains(d)) {
            ui.client.remove(d);
        }
        ui.client_version += 1;
    }
}

// The book's click on entry `index`: true when it was not the server's to place, and the grid was filled here (the
// grid's items first go back to the inventory; each stack is picked up, one dropped in each cell that takes it, and
// put back).
pub(crate) fn place(game: &mut Game<'_>, index: usize) -> bool {
    if game.remote.is_none() || book(|b| b.shown.is_none()) {
        return false;
    }
    let Some(entry) = game.recipe_ui.book.as_ref().and_then(|b| b.entries.get(index)).cloned() else {
        return false;
    };
    if book(|b| b.server.contains(&entry.display_id)) {
        return false;
    }
    let size = match game.remote_menu().1.as_str() {
        "inventory" => 2,
        "minecraft:crafting" => 3,
        _ => return true,
    };
    if !entry.fits(size, size) {
        return true;
    }
    let mut ops = Vec::new();
    for cell in 0..size * size {
        if game.slot_mut(100 + cell).is_some() {
            ops.push((Some(100 + cell), 0, QuickMove));
        }
    }
    let needs = entry.requirements();
    let mut left: Vec<(usize, String, u32)> =
        (0..36).filter_map(|i| game.inventory[i].as_ref().map(|s| (i, s.item.to_string(), s.count))).collect();
    let mut from: Vec<(usize, Vec<usize>)> = Vec::new();
    for (need, cell) in entry.grid_cells(size, size) {
        let Some(need) = need.and_then(|n| needs.get(n)) else {
            continue;
        };
        // A stack already being used for this kind first, so fewer pick-ups.
        let pick = left
            .iter()
            .enumerate()
            .filter(|(_, (_, item, count))| *count > 0 && need.0.contains(item))
            .min_by_key(|(_, (slot, _, _))| !from.iter().any(|f| f.0 == *slot))
            .map(|(i, _)| i);
        let Some(i) = pick else {
            super::super::sunset::say(game, "Unlock All Recipes: not enough items for that recipe.");
            return true;
        };
        left[i].2 -= 1;
        let slot = left[i].0;
        match from.iter_mut().find(|f| f.0 == slot) {
            Some(f) => f.1.push(cell),
            None => from.push((slot, vec![cell])),
        }
    }
    for (slot, cells) in from {
        ops.push((Some(slot), 0, Pickup));
        ops.extend(cells.into_iter().map(|cell| (Some(100 + cell), 1, Pickup)));
        ops.push((Some(slot), 0, Pickup));
    }
    game.remote_menu_ops(&ops);
    true
}
