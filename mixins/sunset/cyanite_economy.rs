// Optional auction automation through the normal menu and command entry points.
use super::Game;
use super::sunset::{Category, Module, hold, say};
use super::sunset_util::listed;
use std::collections::HashSet;
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Auto Sell", Category::Player, "Sells selected stacks through /shop or lists them on /ah.")
            .pick("Mode", &["Shop", "Auction"])
            .group("Shop")
            .text("Shop command", "shop").text("Shop title", "shop")
            .num("Stacks per visit", 2.0, 1.0, 36.0, 1.0)
            .group("Auction")
            .num("Price", 1000.0, 1.0, 1_000_000_000.0, 1.0)
            .bool("Skip tools and weapons", true).bool("Skip food", false).bool("Switch back", true)
            .group("General")
            .num("Delay (s)", 1.5, 0.5, 10.0, 0.25)
            .text("Items", "diamond"),
        Module::new("Auction Sniper", Category::Player, "Buys matching /ah listings within your price and total budget.")
            .group("Listings")
            .text("Items", "").text("Max price", "").text("Budget", "")
            .bool("Price per item", false)
            .group("Auction menu")
            .bool("Open auction", true).text("Auction command", "ah").text("Menu title", "auction")
            .group("Confirmation")
            .bool("Confirm screen", true).text("Confirm title", "confirm")
            .text("Confirm button text", "confirm").text("Confirm item", "")
            .group("Timing")
            .num("Click delay (s)", 0.5, 0.25, 5.0, 0.25)
            .num("Refresh delay (s)", 2.0, 0.5, 10.0, 0.5),
    ]
}

#[derive(Default)]
struct State {
    world: Option<u64>,
    sell: Option<Sale>,
    shop: Option<Shop>,
    sniper_on: bool,
    spent: f64,
    clicked: HashSet<String>,
    next_click: u64,
    next_refresh: u64,
    pending: Option<Purchase>,
    open_deadline: Option<u64>,
    needs_open: bool,
    attempts: u32,
}

struct Purchase {
    listing_window: i32,
    deadline: u64,
    price: f64,
}

struct Sale {
    slots: Vec<(usize, horizon_client::items::ItemStack)>,
    back: usize,
    expected: usize,
    next: u64,
}

struct Shop {
    slots: Vec<(usize, horizon_client::items::ItemStack)>,
    window: Option<i32>,
    due: u64,
    deadline: u64,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

pub(super) fn stopped(game: &mut Game<'_>) {
    let sale = state(|s| { let sale = s.sell.take(); *s = State::default(); sale });
    if let Some(sale) = sale && game.selected == sale.expected {
        hold(game, sale.back);
    }
}

fn shop(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        state(|s| s.shop = None);
        return;
    };
    if m.t("Shop title").trim().is_empty() || m.t("Shop command").trim().is_empty() { return; }
    if state(|s| s.shop.is_none()) {
        if !game.native_playing() || super::sunset::gui_open() || game.carried.is_some() { return; }
        let slots: Vec<_> = (0..36).filter_map(|i| {
            let stack = game.inventory.get(i)?.as_ref()?;
            listed(m.t("Items"), &stack.item).then(|| (i, stack.clone()))
        }).take(m.n("Stacks per visit") as usize).collect();
        if slots.is_empty() {
            let _ = super::sunset_api::toggle(game, "Auto Sell", false);
            return;
        }
        let command = format!("/{}", m.t("Shop command").trim().trim_start_matches('/'));
        if super::sunset_api::say_server(game, &command).is_err() { return; }
        state(|s| s.shop = Some(Shop { slots, window: None, due: game.ticks + 4, deadline: game.ticks + 100 }));
        return;
    }
    let (window, due, deadline) = state(|s| {
        let shop = s.shop.as_ref().unwrap();
        (shop.window, shop.due, shop.deadline)
    });
    let (open, _) = game.remote_menu();
    if window.is_some_and(|id| id != open) || game.carried.is_some() || game.ticks >= deadline {
        state(|s| s.shop = None);
        let _ = super::sunset_api::toggle(game, "Auto Sell", false);
        say(game, "Auto Sell stopped because the shop changed, timed out, or the cursor holds an item.");
        return;
    }
    let title = game.remote_menu_title().map(|t| t.plain().to_lowercase()).unwrap_or_default();
    if open == 0 || !title.contains(&m.t("Shop title").to_lowercase()) {
        return;
    }
    if window.is_none() {
        state(|s| if let Some(shop) = &mut s.shop {
            shop.window = Some(open);
            shop.due = game.ticks + 4;
            shop.deadline = game.ticks + 100 + (m.n("Delay (s)") * 20.0) as u64;
        });
        return;
    }
    if game.ticks < due { return; }
    let next = state(|s| s.shop.as_mut().and_then(|shop| {
        if shop.slots.is_empty() { None } else { Some(shop.slots.remove(0)) }
    }));
    let Some((slot, snapshot)) = next else {
        // A partial sale stays in its inventory slot for the next visit.
        super::sunset_combat_inventory::close_inventory(game);
        state(|s| s.shop = None);
        let _ = super::sunset_api::toggle(game, "Auto Sell", false);
        return;
    };
    if game.inventory.get(slot).and_then(Option::as_ref) == Some(&snapshot) {
        super::remote::sunset_remote::server_menu_click(game, open, slot);
    }
    state(|s| if let Some(shop) = &mut s.shop {
        shop.due = game.ticks + (m.n("Delay (s)") * 20.0) as u64;
        shop.deadline = shop.due + 100;
    });
}

fn sell(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        if let Some(sale) = state(|s| s.sell.take()) && game.selected == sale.expected {
            hold(game, sale.back);
        }
        return;
    };
    if !game.native_playing() || super::sunset::gui_open() || game.using.is_some() {
        return;
    }
    if state(|s| s.sell.is_none()) {
        let slots: Vec<_> = (0..9).filter_map(|i| {
            let stack = game.inventory.get(i)?.as_ref()?;
            let facts = game.items().get(&stack.item);
            let tool = facts.as_ref().is_some_and(|f| f.max_damage > 0);
            let food = facts.as_ref().is_some_and(|f| f.food.is_some());
            (listed(m.t("Items"), &stack.item) && !(m.b("Skip tools and weapons") && tool)
                && !(m.b("Skip food") && food)).then(|| (i, stack.clone()))
        }).collect();
        state(|s| s.sell = Some(Sale { slots, back: game.selected, expected: game.selected, next: game.ticks }));
    }
    if state(|s| s.sell.as_ref().is_some_and(|sale| game.selected != sale.expected)) {
        state(|s| s.sell = None);
        let _ = super::sunset_api::toggle(game, "Auto Sell", false);
        say(game, "Auto Sell stopped after you changed slots.");
        return;
    }
    if state(|s| s.sell.as_ref().is_some_and(|sale| game.ticks < sale.next)) { return; }
    let item = state(|s| s.sell.as_mut().and_then(|sale| {
        if sale.slots.is_empty() { None } else { Some(sale.slots.remove(0)) }
    }));
    let Some((slot, snapshot)) = item else {
        let sale = state(|s| s.sell.take()).unwrap();
        if m.b("Switch back") && game.selected == sale.expected { hold(game, sale.back); }
        let _ = super::sunset_api::toggle(game, "Auto Sell", false);
        return;
    };
    if game.inventory.get(slot).and_then(Option::as_ref) == Some(&snapshot) {
        hold(game, slot);
        game.send_chat(&format!("/ah sell {:.0}", m.n("Price")));
        state(|s| if let Some(sale) = &mut s.sell {
            sale.expected = slot;
            sale.next = game.ticks + (m.n("Delay (s)") * 20.0) as u64;
        });
    }
}

fn price(stack: &horizon_client::items::ItemStack) -> Option<f64> {
    use horizon_world::nbt::Tag;
    let extra = stack.extra.as_ref()?;
    let (_, Tag::List(lines)) = extra.other.iter().find(|(name, _)| name == "minecraft:lore")? else { return None };
    lines.iter().find_map(|line| {
        let json = horizon_client::scoreboard::tag_to_json(line);
        let text = horizon_client::scoreboard::comp_text(&json);
        super::sunset_cyanite_logic::listing_price(&text.strip_legacy().plain())
    })
}

fn wanted(m: &Module, stack: &horizon_client::items::ItemStack) -> bool {
    let filter = m.t("Items").trim();
    if filter.is_empty() { return true; }
    let label = stack.custom_name().unwrap_or_else(|| stack.item.replace('_', " ")).to_lowercase();
    filter.split(',').map(str::trim).filter(|word| !word.is_empty()).any(|word| {
        listed(word, &stack.item) || label.contains(&word.to_lowercase())
    })
}

fn listing_key(stack: &horizon_client::items::ItemStack) -> String {
    use horizon_world::nbt::Tag;
    let mut stable = stack.clone();
    if let Some(extra) = &mut stable.extra {
        for (name, value) in &mut extra.other {
            if name != "minecraft:lore" { continue; }
            if let Tag::List(lines) = value {
                lines.retain(|line| {
                    let json = horizon_client::scoreboard::tag_to_json(line);
                    let text = horizon_client::scoreboard::comp_text(&json);
                    super::sunset_cyanite_logic::stable_listing_line(&text.strip_legacy().plain())
                });
            }
        }
    }
    format!("{stable:?}")
}

fn sniper(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        state(|s| { s.sniper_on = false; s.clicked.clear(); s.spent = 0.0; s.pending = None;
            s.needs_open = false; s.open_deadline = None; s.attempts = 0; });
        return;
    };
    if !state(|s| std::mem::replace(&mut s.sniper_on, true)) {
        state(|s| { s.clicked.clear(); s.spent = 0.0; s.next_click = 0; s.next_refresh = 0; s.pending = None;
            s.needs_open = true; s.open_deadline = None; s.attempts = 0; });
    }
    let Some(max_price) = super::sunset_cyanite_logic::money(m.t("Max price")) else { return };
    let budget = if m.t("Budget").trim().is_empty() || m.t("Budget").trim() == "0" {
        None
    } else {
        let Some(budget) = super::sunset_cyanite_logic::money(m.t("Budget")) else { return };
        Some(budget)
    };
    if m.t("Menu title").trim().is_empty() || state(|s| game.ticks < s.next_click)
    {
        return;
    }
    if game.carried.is_some() || super::sunset::gui_open() || !game.physical_buttons.is_empty() {
        if state(|s| s.pending.take().is_some()) {
            let _ = super::sunset_api::toggle(game, "Auction Sniper", false);
            say(game, "Auction Sniper stopped after inventory input interrupted confirmation.");
        }
        return;
    }
    let title = game.remote_menu_title().map(|t| t.plain().to_lowercase()).unwrap_or_default();
    let open = game.remote_menu().0;
    if let Some((window, deadline, expected)) = state(|s| s.pending.as_ref().map(|p| (p.listing_window, p.deadline, p.price))) {
        if game.ticks >= deadline {
            state(|s| s.pending = None);
            let _ = super::sunset_api::toggle(game, "Auction Sniper", false);
            say(game, "Auction Sniper stopped because its confirmation menu did not arrive.");
            return;
        }
        if game.remote_menu().0 == window || m.t("Confirm title").trim().is_empty()
            || !title.contains(&m.t("Confirm title").to_lowercase())
        {
            return;
        }
        let Some((rows, _)) = game.remote_chest_menu() else { return };
        for slot in 0..rows.min(6) * 9 {
            let Some(stack) = game.slot_mut(300 + slot).clone() else { continue };
            let label = stack.custom_name().unwrap_or_default();
            if (!m.t("Confirm item").trim().is_empty() && !listed(m.t("Confirm item"), &stack.item))
                || !super::sunset_cyanite_logic::confirms(&label, m.t("Confirm button text"))
                || price(&stack).is_some_and(|price| price > expected)
            { continue; }
            if super::remote::sunset_remote::server_menu_click(game, open, 300 + slot) {
                state(|s| { s.pending = None; s.next_click = game.ticks + 10; });
            }
            return;
        }
        return;
    }
    if !title.contains(&m.t("Menu title").to_lowercase()) {
        if state(|s| s.open_deadline.is_some_and(|due| game.ticks >= due)) {
            let _ = super::sunset_api::toggle(game, "Auction Sniper", false);
            say(game, "Auction Sniper stopped because its auction menu did not arrive.");
        } else if m.b("Open auction") && game.native_playing() && game.remote_menu().0 == 0
            && !m.t("Auction command").trim().is_empty() && state(|s| s.needs_open)
        {
            let command = format!("/{}", m.t("Auction command").trim().trim_start_matches('/'));
            if super::sunset_api::say_server(game, &command).is_ok() {
                state(|s| { s.needs_open = false; s.open_deadline = Some(game.ticks + 100); });
            }
        }
        return;
    }
    state(|s| { s.needs_open = false; s.open_deadline = None; });
    let Some((rows, _)) = game.remote_chest_menu() else { return };
    if !(2..=6).contains(&rows) { return; }
    // Auction controls occupy the last row. Listings are examined only in the earlier rows.
    let listing_slots = (rows - 1) * 9;
    for slot in 0..listing_slots {
        let Some(stack) = game.slot_mut(300 + slot).clone() else { continue };
        if !wanted(m, &stack) || stack.count == 0 { continue; }
        let Some(total) = price(&stack) else { continue };
        let compared = if m.b("Price per item") { total / f64::from(stack.count) } else { total };
        if compared > max_price || budget.is_some_and(|budget| state(|s| s.spent + total > budget)) { continue; }
        let key = listing_key(&stack);
        if state(|s| s.clicked.contains(&key) || s.clicked.len() >= 1024) { continue; }
        if super::remote::sunset_remote::server_menu_click(game, open, 300 + slot) {
            state(|s| {
                s.clicked.insert(key);
                s.spent += total;
                s.attempts += 1;
                s.next_click = game.ticks + (m.n("Click delay (s)") * 20.0) as u64;
                if m.b("Confirm screen") {
                    s.pending = Some(Purchase { listing_window: game.remote_menu().0,
                        deadline: game.ticks + 100, price: total });
                }
            });
        }
        return;
    }
    if state(|s| game.ticks < s.next_refresh) { return; }
    for slot in listing_slots..rows * 9 {
        if game.slot_mut(300 + slot).as_ref().is_some_and(|s| s.item.as_ref() == "minecraft:anvil") {
            super::remote::sunset_remote::server_menu_click(game, open, 300 + slot);
            state(|s| {
                s.next_refresh = game.ticks + (m.n("Refresh delay (s)") * 20.0) as u64;
                s.next_click = game.ticks + 5;
            });
            break;
        }
    }
}

// The existing HUD's module list owns presentation, color and placement.
pub(super) fn info(m: &Module) -> Option<String> {
    if m.name == "Auto Sell" { return Some(m.p("Mode").to_owned()); }
    if super::sunset_cyanite_logic::money(m.t("Max price")).is_none() {
        return Some("Set Max price".into());
    }
    state(|s| Some(if s.pending.is_some() { "Confirming".into() }
        else { format!("{} attempts / ${:.0} reserved", s.attempts, s.spent) }))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let world = super::sunset_cyanite_utilities::world_key(game);
    if state(|s| s.world != world) {
        state(|s| *s = State { world, ..State::default() });
    }
    if game.remote.is_none() || game.player.is_none() { return; }
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    shop(game, get("Auto Sell").filter(|m| m.p("Mode") == "Shop"));
    sell(game, get("Auto Sell").filter(|m| m.p("Mode") == "Auction"));
    sniper(game, get("Auction Sniper"));
}
