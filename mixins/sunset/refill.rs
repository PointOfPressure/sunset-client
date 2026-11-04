// Inventory routines on modules that had a silent version already: Auto Replenish's Inventory open and Key modes and
// its Potion refill, and Inventory Cleaner's Loot key. Each is one click step at a time with the module's delay
// between, from the player's own inventory menu (no container open), never from a slot Loadouts locks.
use super::sunset::{Module, key_from_text, menu_click};
use super::sunset_util::short;
use super::{Game, Screen};
use horizon_client::menu::Input::{Pickup, QuickMove, Throw};
use std::sync::Mutex;

#[derive(Default)]
struct Refill {
    // Keys held now, as the game names them.
    down: Vec<String>,
    replenish_wait: u32,
    potion_wait: u32,
    loot_wait: u32,
}

static REFILL: Mutex<Option<Refill>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut Refill) -> R) -> R {
    f(REFILL.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Refill::default))
}

pub(super) fn key(key: &str, pressed: bool) {
    st(|s| {
        s.down.retain(|k| k != key);
        if pressed {
            s.down.push(key.to_owned());
        }
    });
}

// Whether the key a setting names is held.
fn held(m: &Module, setting: &str) -> bool {
    key_from_text(m.t(setting)).is_some_and(|k| st(|s| s.down.contains(&k)))
}

// The player's inventory screen is open (not a container's).
fn inventory_screen(game: &Game<'_>) -> bool {
    game.screen == Screen::Inventory && game.remote_menu().1 == "inventory" && game.remote.is_some() && game.player.is_some()
}

// A step's wait: counts down a tick, true while some is left.
fn waiting(wait: impl FnOnce(&mut Refill) -> &mut u32) -> bool {
    st(|s| {
        let w = wait(s);
        *w = w.saturating_sub(1);
        *w > 0
    })
}

fn splash(game: &Game<'_>, slot: usize, healing_only: bool) -> bool {
    game.inventory.get(slot).and_then(|s| s.as_ref()).is_some_and(|s| {
        short(&s.item) == "splash_potion" && (!healing_only || s.potion().is_some_and(|p| short(p).ends_with("healing")))
    })
}

// Auto Replenish's modes beside Silent. Inventory open: with your inventory screen open, a hotbar stack at or under
// Threshold (one that stacks and isn't full) is topped up from the main inventory: pick that stack up, put it on the
// hotbar one (it fills to full), put what is left back. Key: the same while Refill key is held, the screen open or
// not. Potion refill: with the screen open, a splash potion (only healing ones, with Healing only) shift-clicked from
// the main inventory into an empty hotbar slot.
pub(super) fn replenish(game: &mut Game<'_>, m: &Module) {
    let screen = inventory_screen(game);
    if m.b("Potion refill") && screen && !waiting(|s| &mut s.potion_wait) {
        let locked = super::sunset_loadouts::locked_slots();
        let room = (0..9).any(|h| !locked[h] && game.inventory.get(h).is_some_and(Option::is_none));
        let healing = m.b("Healing only");
        if let Some(src) = (9..36).find(|&i| !locked[i] && splash(game, i, healing)).filter(|_| room) {
            menu_click(game, &[(src, QuickMove)]);
            st(|s| s.potion_wait = m.n("Potion delay (ticks)") as u32 + 1);
            return;
        }
    }
    let ready = match m.p("Mode") {
        "Inventory open" => screen,
        "Key" => held(m, "Refill key") && (screen || super::sunset::inventory_idle(game)),
        _ => false,
    };
    if !ready || waiting(|s| &mut s.replenish_wait) {
        return;
    }
    let locked = super::sunset_loadouts::locked_slots();
    for h in (0..9).filter(|&h| !locked[h]) {
        let Some(st_h) = game.inventory.get(h).and_then(|s| s.as_ref()) else {
            continue;
        };
        let max = game.res.items().get(&st_h.item).map_or(64, |i| i.max_stack);
        if max <= 1 || st_h.count >= max || f64::from(st_h.count) > m.n("Threshold") {
            continue;
        }
        let item = st_h.item.clone();
        let source = (9..36).find(|&i| !locked[i] && game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| s.item == item));
        if let Some(src) = source {
            menu_click(game, &[(src, Pickup), (h, Pickup), (src, Pickup)]);
            st(|s| s.replenish_wait = m.n("Delay (ticks)") as u32);
            return;
        }
    }
}

// Inventory Cleaner's Loot key: held with your inventory screen open, it makes room for loot by dropping whole stacks
// of totems while more slots than Keep totems hold one, then ender pearl stacks past Keep pearl stacks; one stack a
// step, Delay apart. True while the key is held (the junk list waits).
pub(super) fn loot(game: &mut Game<'_>, m: &Module) -> bool {
    if !held(m, "Loot key") || !inventory_screen(game) {
        return false;
    }
    if waiting(|s| &mut s.loot_wait) {
        return true;
    }
    let locked = super::sunset_loadouts::locked_slots();
    let slots = |item: &str| -> Vec<usize> {
        (0..36).filter(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| short(&s.item) == item)).collect()
    };
    let extra = |found: Vec<usize>, keep: f64| (found.len() as f64 > keep).then(|| found.into_iter().find(|&i| !locked[i])).flatten();
    let drop = extra(slots("totem_of_undying"), m.n("Keep totems")).or_else(|| extra(slots("ender_pearl"), m.n("Keep pearl stacks")));
    if let Some(i) = drop {
        // Button 1: the whole stack.
        game.remote_menu_ops(&[(Some(i), 1, Throw)]);
        st(|s| s.loot_wait = m.n("Delay (ticks)") as u32 + 1);
    }
    true
}
