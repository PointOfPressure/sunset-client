// Client: the things that are not cheats (Unlock All Recipes, in World, works in recipes.rs). Client Sort, like
// Client Sort: three small buttons down the right of each part of an inventory screen (sort_buttons.rs),
// Sort, Fill Stacks and Restock. Sort goes in creative order, by count with Shift, by name with Ctrl and by id with Alt; Horizon's
// sort key does the same, on the hotbar only while the pointer is on it. The clicks go out a few a tick, an operation
// at least two ticks after the one before. Shift fill shift-clicks the slot under the pointer
// while you hold Shift and the left button.

use super::sunset::{Category, Module};
use super::Game;
use horizon_client::items::ItemStack;
use horizon_client::menu::Input;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::KeyCode;

// A key past any real one.
const LAST: usize = 1 << 40;
const CLICKS_PER_TICK: usize = 4;
const GAP_TICKS: u32 = 2;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Unlock All Recipes", Category::World,
            "Shows every recipe as unlocked."),
        Module::new("Profiles", Category::Client, "Saved settings to load."),
        Module::new("Client Sort", Category::Client,
            "Sorts inventories in your order.")
            .bool("Shift fill", false)
            .num("Fill CPS", 15.0, 1.0, 20.0, 1.0),
    ]
}

// Client Sort is on (read by the buttons' hooks each frame).
static ON: AtomicBool = AtomicBool::new(false);

// Shift fill: the slot under the pointer this frame, the last slot it clicked, and when the
// next click may go.
struct Fill {
    hovered: Option<usize>,
    last: Option<usize>,
    next: Option<std::time::Instant>,
    roll: u64,
}

static FILL: Mutex<Fill> = Mutex::new(Fill { hovered: None, last: None, next: None, roll: 0x5EED });

fn fill<R>(f: impl FnOnce(&mut Fill) -> R) -> R {
    f(&mut FILL.lock().unwrap_or_else(|e| e.into_inner()))
}

// The inventory screen's draw: the slot under the pointer.
pub(super) fn note_hover(slot: Option<usize>) {
    fill(|f| f.hovered = slot);
}

// Shift fill, each tick with Shift and the left button held over an inventory screen and nothing carried: the slot
// under the pointer is shift-clicked, Fill CPS (one either side) a second; the slot just clicked again only one time
// in five.
fn shift_fill(game: &mut Game<'_>, m: &Module) {
    let held = (game.physical_buttons.contains(&winit::event::MouseButton::Left) || game.left)
        && [KeyCode::ShiftLeft, KeyCode::ShiftRight].iter().any(|k| game.physical_keys.contains(k) || game.keys.contains(k));
    let now = std::time::Instant::now();
    let Some(slot) = fill(|f| f.hovered).filter(|_| held && game.carried.is_none()) else {
        fill(|f| f.last = None);
        return;
    };
    let roll = fill(|f| {
        f.roll ^= f.roll << 13;
        f.roll ^= f.roll >> 7;
        f.roll ^= f.roll << 17;
        (f.roll >> 11) as f64 / (1u64 << 53) as f64
    });
    if fill(|f| f.next.is_some_and(|due| now < due) || (f.last == Some(slot) && roll < 0.8)) {
        return;
    }
    game.tweak_click(slot, 0, Input::QuickMove);
    let cps = (m.n("Fill CPS") - 1.0 + roll * 2.0).clamp(1.0, 20.0);
    fill(|f| {
        f.last = Some(slot);
        f.next = Some(now + std::time::Duration::from_secs_f64(1.0 / cps));
    });
}

pub(super) fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

// The operations still to do, each its clicks (slot id, quick move or not), and the ticks to wait before the next.
struct Queue {
    ops: VecDeque<VecDeque<(usize, bool)>>,
    wait: u32,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue { ops: VecDeque::new(), wait: 0 });

pub(super) fn shutdown(game: &mut Game<'_>) {
    ON.store(false, Ordering::Relaxed);
    queue(|q| { q.ops.clear(); q.wait = 0; });
    fill(|f| { f.hovered = None; f.last = None; f.next = None; });
    super::recipe_book::sunset_recipes::sync(game, false);
}

fn queue<R>(f: impl FnOnce(&mut Queue) -> R) -> R {
    f(&mut QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
}

// Horizon's sort of `slots` (its sort key; the buttons are Client Sort's own), done Client Sort's way. True when it
// took it.
pub(super) fn sort(game: &mut Game<'_>, slots: &[usize]) -> bool {
    if !on() || !game.native_on(super::sunset::ID) {
        return false;
    }
    let hotbar: Vec<usize> = (0..9).collect();
    let slots = if game.inventory_target().0.is_some_and(|s| s < 9) { &hotbar } else { slots };
    operate(game, 0, slots);
    true
}

// One of the buttons' operations on the part `slots`: 0 sort, 1 fill stacks, 2 restock. Queued after any still going.
pub(super) fn operate(game: &mut Game<'_>, op: usize, slots: &[usize]) {
    if game.carried.is_some() {
        return;
    }
    // Slots Loadouts locks take no part.
    let slots: Vec<usize> = slots.iter().copied().filter(|&s| !super::sunset_loadouts::locked(s)).collect();
    let slots = &slots[..];
    let clicks: VecDeque<(usize, bool)> = match op {
        0 => sort_clicks(game, slots).into_iter().map(|s| (s, false)).collect(),
        1 => fill_clicks(game, slots).into(),
        _ => restock_clicks(game, slots).into(),
    };
    queue(|q| q.ops.push_back(clicks));
}

// What faces this part: a container's slots face your inventory, your inventory faces the container (or, with none,
// the hotbar).
fn other_side(game: &Game<'_>, slots: &[usize]) -> Vec<usize> {
    if slots.first().is_some_and(|&s| s >= 300) {
        return (0..36).filter(|&s| !super::sunset_loadouts::locked(s)).collect();
    }
    let container: Vec<usize> = game.slots().into_iter().filter(|s| (300..500).contains(s)).collect();
    if container.is_empty() { (0..9).collect() } else { container }
}

// Left clicks that sort `slots`: creative order, Shift by count, Ctrl by name, Alt by id.
fn sort_clicks(game: &mut Game<'_>, slots: &[usize]) -> Vec<usize> {
    let alt = game.physical_keys.contains(&KeyCode::AltLeft) || game.physical_keys.contains(&KeyCode::AltRight);
    let (ctrl, shift) = (game.has_control_down(), game.has_shift_down());
    let views: Vec<Option<ItemStack>> = slots.iter().map(|&s| game.slot_view(s)).collect();
    let totals = |item: &str| views.iter().flatten().filter(|s| &*s.item == item).map(|s| s.count as usize).sum::<usize>();
    let registry = &game.res.items().registry_order;
    let rank = |s: &ItemStack| -> horizon_client::inv_sort::Rank {
        let name = s.custom_name().unwrap_or_else(|| game.pack.names.display_item(&s.item));
        let creative = game.creative_screen.order_of(s).unwrap_or(LAST);
        if alt {
            (registry.iter().position(|r| *r == s.item).unwrap_or(LAST), name)
        } else if ctrl {
            (0, name.to_lowercase())
        } else if shift {
            (LAST - totals(&s.item).min(LAST), format!("{creative:012}"))
        } else {
            (creative, name)
        }
    };
    let max = |s: &ItemStack| game.max_stack(s);
    horizon_client::inv_sort::plan(&views, &rank, &max).into_iter().map(|i| slots[i]).collect()
}

// Fill Stacks: each stack here short of full takes the same item from the other side (pick it up, click the stack
// here, put back what is left), as the counts will be after each move.
fn fill_clicks(game: &mut Game<'_>, slots: &[usize]) -> Vec<(usize, bool)> {
    let side = other_side(game, slots);
    let mut here: Vec<(usize, Option<ItemStack>)> = slots.iter().map(|&s| (s, game.slot_view(s))).collect();
    let mut there: Vec<(usize, Option<ItemStack>)> = side.iter().map(|&s| (s, game.slot_view(s))).collect();
    let mut clicks = Vec::new();
    for (to, stack) in &mut here {
        let Some(stack) = stack else {
            continue;
        };
        let max = game.max_stack(stack);
        for (from, source) in &mut there {
            if stack.count >= max {
                break;
            }
            let Some(src) = source.as_mut().filter(|v| v.same_item_same_components(stack)) else {
                continue;
            };
            let moved = src.count.min(max - stack.count);
            stack.count += moved;
            src.count -= moved;
            clicks.extend([(*from, false), (*to, false)]);
            if src.count > 0 {
                clicks.push((*from, false));
            } else {
                *source = None;
            }
        }
    }
    clicks
}

// Restock: each stack here whose item the other side already has is quick-moved across.
fn restock_clicks(game: &mut Game<'_>, slots: &[usize]) -> Vec<(usize, bool)> {
    let side = other_side(game, slots);
    let there: Vec<ItemStack> = side.iter().filter_map(|&s| game.slot_view(s)).collect();
    slots
        .iter()
        .filter(|&&s| game.slot_view(s).is_some_and(|v| there.iter().any(|t| t.same_item_same_components(&v))))
        .map(|&s| (s, true))
        .collect()
}

// Each tick: Horizon's sort (its key) on while Client Sort is, and the next clicks of an operation; none once the
// screen they were for has closed.
pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    super::recipe_book::sunset_recipes::sync(game, mods.iter().any(|m| m.name == "Unlock All Recipes" && m.on));
    let on = mods.iter().any(|m| m.name == "Client Sort" && m.on);
    ON.store(on, Ordering::Relaxed);
    if on {
        game.settings.extras.sort = true;
    }
    if game.screen != super::Screen::Inventory {
        queue(|q| q.ops.clear());
        fill(|f| f.hovered = None);
        return;
    }
    if let Some(m) = mods.iter().find(|m| m.name == "Client Sort" && m.on && m.b("Shift fill")) {
        shift_fill(game, m);
    }
    let now: Vec<(usize, bool)> = queue(|q| {
        if q.wait > 0 {
            q.wait -= 1;
            return Vec::new();
        }
        let Some(op) = q.ops.front_mut() else {
            return Vec::new();
        };
        let now = op.drain(..CLICKS_PER_TICK.min(op.len())).collect();
        if op.is_empty() {
            q.ops.pop_front();
            q.wait = GAP_TICKS;
        }
        now
    });
    for (slot, quick) in now {
        game.tweak_click(slot, 0, if quick { Input::QuickMove } else { Input::Pickup });
    }
}
