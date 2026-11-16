// Runtime part (see runtime.rs): armour, Chest Swap, Auto Replenish, Auto Tool and middle click.

use super::{Game, Screen};
use winit::keyboard::KeyCode;
use super::sunset::*;

// Puts the stack in inventory slot `i` on armor slot `a`: shift-click into an empty slot; else from the hotbar one
// hotbar-key swap with the armor slot, and from the main inventory three (into a hotbar slot, onto the armor slot, the
// worn piece back to `i`). Never by picking pieces up: a server that refuses one of the clicks would leave a piece on
// the cursor, and a refused middle swap puts both back.
// Ungated: Chest Swap and Take off check the input themselves, and a restore must go out.
pub(super) fn wear(game: &mut Game<'_>, i: usize, a: usize) {
    use horizon_client::menu::Input::{QuickMove, Swap};
    let armor = 500 + a;
    if super::sunset_loadouts::locked(i) || super::sunset_loadouts::locked(armor) {
        return;
    }
    let ops: Vec<(Option<usize>, i32, horizon_client::menu::Input)> = if game.armor[a].is_none() {
        vec![(Some(i), 0, QuickMove)]
    } else if i < 9 {
        vec![(Some(armor), i as i32, Swap)]
    } else {
        let empty: [bool; 9] = std::array::from_fn(|h| game.inventory.get(h).is_none_or(|s| s.is_none()));
        let locked: [bool; 9] = std::array::from_fn(super::sunset_loadouts::locked);
        let Some(h) = super::sunset_inventory_plan::pivot(empty, locked, game.selected) else {
            return;
        };
        vec![(Some(i), h as i32, Swap), (Some(armor), h as i32, Swap), (Some(i), h as i32, Swap)]
    };
    game.remote_menu_ops(&ops);
}

// An armour piece's material: `diamond` of diamond_helmet.
fn armor_material(item: &str) -> &str {
    let name = super::sunset_util::short(item);
    name.rsplit_once('_').map_or(name, |(material, _)| material)
}

// Switch key (Sunset's ArmorSwitch), on a press: the next set Switch sets lists after the one you wear, once all four
// of its pieces are on you or in your inventory; Auto Armor then wears that set instead of the best.
pub(super) fn armor_switch(game: &mut Game<'_>, key: &str) {
    auto_armor_key(game, key);
    let Some(m) = with(|s| s.module("Inventory Manager").filter(|m| m.b("Auto Armor")).cloned()) else {
        return;
    };
    if key_from_text(m.t("Switch key")).as_deref() != Some(key) || !game.native_playing() || !inventory_idle(game) {
        return;
    }
    let sets: Vec<&str> = m.t("Switch sets").split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
    let worn = game.armor.iter().flatten().next().map(|s| armor_material(&s.item).to_owned());
    let at = worn.as_deref().and_then(|w| sets.iter().position(|s| *s == w));
    let next = at.map_or(0, |i| i + 1);
    let complete = |set: &str| {
        ["head", "chest", "legs", "feet"].iter().all(|slot| {
            game.armor.iter().chain(game.inventory.iter().take(36)).flatten().any(|st| {
                armor_material(&st.item) == set && game.res.items().get(&st.item).is_some_and(|i| i.equip_slot.as_deref() == Some(*slot))
            })
        })
    };
    match (0..sets.len()).map(|k| sets[(next + k) % sets.len()]).find(|set| Some(*set) != worn.as_deref() && complete(set)) {
        Some(set) => with(|s| s.armor_set = Some(set.to_owned())),
        None => say(game, "No other full armour set from Switch sets."),
    }
}

// Auto Armor key: turns Auto Armor on or off, as its module's key did before it joined Inventory Manager.
fn auto_armor_key(game: &mut Game<'_>, key: &str) {
    let bound = with(|s| s.module("Inventory Manager").map(|m| (key_from_text(m.t("Auto Armor key")), m.b("Auto Armor"))));
    let Some((Some(bound), on)) = bound else {
        return;
    };
    if bound != key || gui_open() || !game.native_playing() {
        return;
    }
    set_setting(game, "inventorymanager.autoarmor", if on { "false" } else { "true" });
    announce_toggle("Auto Armor", !on);
}

pub(super) fn auto_armor(game: &mut Game<'_>, m: &Module) {
    // Inventory Manager's Slots mode wears your armour itself (inventory_slots.rs).
    if m.on && m.p("Mode") == "Slots" {
        return;
    }
    // One inventory step at a time: Inventory Manager's last one settles first.
    if super::sunset_player::inventory_stepping(game.ticks) {
        return;
    }
    let set = with(|s| s.armor_set.clone());
    // Inventory open: only with your inventory screen up; Key: only while Armor key is held. A set from Switch key
    // goes on anyway.
    let allowed = set.is_some() || match m.p("Armor mode") {
        "Inventory open" => game.screen == Screen::Inventory,
        "Key" => super::sunset_util::setting_key_down(game, m, "Armor key"),
        _ => true,
    };
    // Nothing while the cursor holds an item (yours, or one Inventory Manager is putting back): a click would move it.
    if !allowed || !inventory_idle(game) || game.carried.is_some() {
        return;
    }
    // Avoid cursed: never a piece with Curse of Binding or Vanishing.
    let cursed = |st: &horizon_client::items::ItemStack| {
        m.b("Avoid cursed") && st.enchantments().iter().any(|(id, _)| id.ends_with("binding_curse") || id.ends_with("vanishing_curse"))
    };
    if with(|s| {
        s.armor_wait = s.armor_wait.saturating_sub(1);
        s.armor_wait > 0
    }) {
        return;
    }
    const SLOTS: [&str; 4] = ["head", "chest", "legs", "feet"];
    // Pieces are ranked by inventory_plan's armour evaluator (vanilla's damage reduction with Protection), the same one
    // Inventory Manager keeps armour by, so the two never disagree.
    let value = |game: &Game<'_>, s: &horizon_client::items::ItemStack| super::sunset_inventory_plan::armor_score(&super::sunset_player::plan_item(game, s));
    for (a, slot_name) in SLOTS.iter().enumerate() {
        // A worn elytra is Chest Swap's to change, not Auto Armor's.
        if a == 1 && game.armor[a].as_ref().is_some_and(|s| s.item.ends_with("elytra")) && set.is_none() {
            continue;
        }
        let worn = game.armor[a].as_ref().map_or(0.0, |s| value(game, s));
        let mut best: Option<(usize, f32)> = None;
        for i in 0..36 {
            let Some(st) = game.inventory.get(i).and_then(|s| s.as_ref()) else {
                continue;
            };
            if st.item.ends_with("elytra") || cursed(st) {
                continue;
            }
            let fits = game
                .res
                .items()
                .get(&st.item)
                .is_some_and(|inf| inf.equip_slot.as_deref() == Some(*slot_name));
            let score = value(game, st);
            let wanted = match set.as_deref() {
                Some(set) => armor_material(&st.item) == set && game.armor[a].as_ref().is_none_or(|w| armor_material(&w.item) != set),
                // Better by more than a hair: no swapping back and forth between near-equal pieces.
                None => score > worn + 0.005,
            };
            if fits && wanted && best.is_none_or(|(_, b)| score > b) {
                best = Some((i, score));
            }
        }
        if let Some((i, _)) = best {
            // A click while moving is what Grim's MultiActionsC flags: the piece waits until you stand still.
            if !super::remote::sunset_combat_remote::quiet(game) {
                return;
            }
            // Past GUI Move's GrimAC queue (it would hold the click until the inventory closes); you stand still here.
            super::sunset_gui_move::direct(|| wear(game, i, a));
            with(|s| s.armor_wait = m.n("Armor delay (ticks)") as u32);
            // Inventory Manager waits for the server's answer to this too.
            let settle = super::sunset_inventory_exec::settle_ticks(super::remote::sunset_combat_remote::latency(game), 0);
            super::sunset_player::inventory_hold(game.ticks + settle);
            return;
        }
    }
}

// Swaps the chestplate and the elytra once the server has received idle input.
// Grim checks the last PlayerInput and sprint state on each inventory click.
pub(super) fn chest_swap(game: &mut Game<'_>) -> bool {
    let moving_now = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD, KeyCode::Space, KeyCode::ShiftLeft]
        .into_iter()
        .any(|key| game.keys.contains(&key) || game.physical_keys.contains(&key));
    if !inventory_idle(game)
        || moving_now
        || super::sunset_combat_wire::sent_input(game) & 0x3f != 0
        || super::sunset_combat_wire::sent_sprinting(game)
        || game.player.as_ref().is_some_and(|p| p.sprinting || p.fall_flying)
    {
        return false;
    }
    let wearing_elytra = game.armor[1].as_ref().is_some_and(|s| s.item.ends_with("elytra"));
    let found = (0..36).find(|&i| {
        game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| {
            if wearing_elytra {
                !s.item.ends_with("elytra")
                    && game.res.items().get(&s.item).is_some_and(|inf| inf.equip_slot.as_deref() == Some("chest"))
            } else {
                s.item.ends_with("elytra")
            }
        })
    });
    match found {
        Some(i) => wear(game, i, 1),
        None => say(game, if wearing_elytra { "No chestplate in your inventory." } else { "No elytra in your inventory." }),
    }
    true
}

pub(super) fn auto_replenish(game: &mut Game<'_>, m: &Module) {
    use horizon_client::menu::Input::QuickMove;
    if !inventory_idle(game) {
        return;
    }
    if with(|s| {
        s.replenish_wait = s.replenish_wait.saturating_sub(1);
        s.replenish_wait > 0
    }) {
        return;
    }
    let locked = super::sunset_loadouts::locked_slots();
    for h in (0..9).filter(|&h| !locked[h]) {
        let Some(st) = game.inventory.get(h).and_then(|s| s.as_ref()) else {
            continue;
        };
        let max = game.res.items().get(&st.item).map_or(64, |i| i.max_stack);
        if max <= 1 || st.count >= max || f64::from(st.count) > m.n("Threshold") {
            continue;
        }
        let item = st.item.clone();
        let source = (9..36).find(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| s.item == item));
        if let Some(src) = source {
            // Shift-click from the main inventory fills the hotbar stack first.
            if menu_click(game, &[(src, QuickMove)]) {
                with(|s| s.replenish_wait = m.n("Delay (ticks)") as u32);
            }
            return;
        }
    }
}

// Auto Tool (Sunset's): while you break a block, the hotbar tool with the best score for it (Sunset's getScore:
// a tool that is right for the block's drops, by its speed and enchantments), after Switch delay; Switch back when you
// let go; Anti break lets go of a tool about to break. Only below you: only for the block under you. From inventory:
// with no better tool in the hotbar, the best in the main inventory swapped into your hand (one swap click), and
// swapped back when you let go or look off the block.
// It runs at the start of a tick, before the game's dig packets and the move packet, as a hotbar key does: a slot
// change after the move packet is what Grim's Post check flags, and one after a hit in the same tick its PacketOrderE
// (`busy`: a hit or a slot change already went this tick; the change waits a tick). The swap clicks go only while the
// server knows you stand still (no movement keys, sneak or sprint sent: Grim's MultiActionsC), each with the
// inventory's close after it, as the game sends them.
pub(super) fn auto_tool(game: &mut Game<'_>, m: &Module, busy: bool) {
    use winit::event::MouseButton;
    let below = super::sunset_util::block_pos(game).map(|[x, y, z]| [x, y - 1, z]);
    let on_block = !m.b("Only below you") || game.pick().is_some_and(|hit| Some(hit.pos) == below);
    let down = game.native_playing() && (game.physical_buttons.contains(&MouseButton::Left) || game.left) && !gui_open() && on_block;
    if !down || game.selected != with(|s| s.tool_pulled.map_or(game.selected, |p| p.1)) {
        tool_unpull(game);
    }
    if !down {
        let back = with(|s| {
            s.tool_wait = None;
            if busy && m.b("Switch back") { None } else { s.tool_back.take() }
        });
        if let Some(back) = back.filter(|_| m.b("Switch back")) {
            hold(game, back);
        }
        return;
    }
    if let Some((slot, left)) = with(|s| s.tool_wait) {
        if left > 1 {
            with(|s| s.tool_wait = Some((slot, left - 1)));
        } else if !busy {
            hold(game, slot);
            with(|s| s.tool_wait = None);
        }
    }
    if game.creative || game.remote.is_none() || game.entity_targeted() {
        return;
    }
    let Some(hit) = game.pick() else {
        return;
    };
    let facts = game.res.facts().get(game.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
    let block = (facts.name.clone(), facts.requires_tool);
    let list = if m.p("List mode") == "Whitelist" { m.t("Whitelist") } else { m.t("Blacklist") };
    let allowed = |stack: Option<&horizon_client::items::ItemStack>| {
        let listed = stack.is_some_and(|s| super::sunset_util::listed(list, &s.item));
        listed == (m.p("List mode") == "Whitelist")
    };
    let stack = |i: usize| game.inventory.get(i).and_then(|s| s.as_ref());
    let best = (0..9)
        .filter(|&i| allowed(stack(i)))
        .map(|i| (i, tool_score(game, m, stack(i), &block)))
        .filter(|(_, score)| *score >= 0.0)
        .fold(None, |b: Option<(usize, f64)>, (i, v)| if b.is_none_or(|b| v > b.1) { Some((i, v)) } else { b });
    let current = stack(game.selected);
    let locked = super::sunset_loadouts::locked;
    let still = super::remote::sunset_combat_remote::quiet(game);
    let pull = (m.b("From inventory") && still && with(|s| s.tool_pulled.is_none()) && inventory_idle(game) && !locked(game.selected))
        .then(|| {
            let held = tool_score(game, m, current, &block).max(best.map_or(-1.0, |b| b.1));
            (9..36)
                .filter(|&i| !locked(i) && allowed(stack(i)))
                .map(|i| (i, tool_score(game, m, stack(i), &block)))
                .filter(|&(_, score)| score > held)
                .fold(None, |b: Option<(usize, f64)>, (i, v)| if b.is_none_or(|b| v > b.1) { Some((i, v)) } else { b })
        })
        .flatten();
    if let Some((from, _)) = pull {
        let to = game.selected;
        if game.remote_menu_ops(&[(Some(from), to as i32, horizon_client::menu::Input::Swap)]) {
            game.close_container();
            with(|s| s.tool_pulled = Some((from, to)));
        }
        return;
    }
    let switch = best
        .filter(|&(slot, score)| slot != game.selected && (score > tool_score(game, m, current, &block) || tool_spent(game, m, current)))
        .map(|(slot, _)| slot);
    if let Some(slot) = switch {
        let selected = game.selected;
        with(|s| {
            s.tool_back.get_or_insert(selected);
        });
        // Switch delay counts from when the tool is first wanted: the wait is set once, not again each tick.
        match m.n("Switch delay") as u32 {
            0 if !busy => hold(game, slot),
            0 => {}
            delay => with(|s| {
                if s.tool_wait.is_none_or(|(waiting, _)| waiting != slot) {
                    s.tool_wait = Some((slot, delay));
                }
            }),
        }
    }
    let held = game.inventory.get(game.selected).and_then(|s| s.as_ref());
    if tool_spent(game, m, held) {
        let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Left, false);
    }
}

// From inventory's tool goes back where it came from: one swap click and the inventory's close, once the server knows
// you stand still (else it waits).
fn tool_unpull(game: &mut Game<'_>) {
    let Some((from, to)) = with(|s| s.tool_pulled) else {
        return;
    };
    if !inventory_idle(game) || !super::remote::sunset_combat_remote::quiet(game) {
        return;
    }
    if game.remote_menu_ops(&[(Some(from), to as i32, horizon_client::menu::Input::Swap)]) {
        game.close_container();
    }
    with(|s| s.tool_pulled = None);
}

// Auto Tool turned off: a tool it pulled from the inventory still goes back, nothing else waits.
pub(super) fn auto_tool_off(game: &mut Game<'_>) {
    with(|s| {
        s.tool_wait = None;
        s.tool_back = None;
    });
    tool_unpull(game);
}

// Anti break: a tool worn down to its last Anti break percentage.
fn tool_spent(game: &Game<'_>, m: &Module, stack: Option<&horizon_client::items::ItemStack>) -> bool {
    let Some(s) = stack else {
        return false;
    };
    let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
    m.b("Anti break") && max > 0 && f64::from(max - s.damage.min(max)) < f64::from(max) * m.n("Anti break percentage") / 100.0
}

// Sunset's AutoTool.getScore, on the game's own destroy speed: any item (a sword or shears on a cobweb, a sword on
// bamboo, a pickaxe on stone) scores by how fast it breaks the block (its name, and whether it needs the right tool to
// drop: a wrong tool then breaks it at a third of its speed), the right tool first on a tie; -1 for one no faster
// than the bare hand.
fn tool_score(game: &Game<'_>, m: &Module, stack: Option<&horizon_client::items::ItemStack>, (block, requires_tool): &(std::sync::Arc<str>, bool)) -> f64 {
    let Some(s) = stack.filter(|s| !tool_spent(game, m, Some(s))) else {
        return -1.0;
    };
    let items = game.res.items();
    let short = super::sunset_util::short(block);
    let correct = items.correct_for_drops(stack, block);
    let speed = f64::from(items.mining_speed(stack, block)) * if *requires_tool && !correct { 0.3 } else { 1.0 };
    if speed <= 1.0 && !correct {
        return -1.0;
    }
    let level = |e: &str| f64::from(s.enchantment_level(e));
    if m.b("Silk touch for ender chest") && short == "ender_chest" && level("minecraft:silk_touch") == 0.0 {
        return -1.0;
    }
    let fortunable = (short.ends_with("_ore") && short != "ancient_debris") || matches!(short, "wheat" | "carrots" | "potatoes" | "beetroots" | "torchflower_crop");
    if m.b("Fortune for ores and crops") && fortunable && level("minecraft:fortune") == 0.0 {
        return -1.0;
    }
    let preferred = match m.p("Prefer") {
        "Fortune" => level("minecraft:fortune"),
        "Silk touch" => level("minecraft:silk_touch"),
        _ => 0.0,
    };
    speed * 1000.0 + f64::from(u8::from(correct)) * 100.0 + level("minecraft:unbreaking") + level("minecraft:efficiency") + level("minecraft:mending") + preferred
}

// Middle click: throws or uses the chosen item from the hotbar, and switches
// back a few ticks later.
pub(super) fn middle_click(game: &mut Game<'_>) {
    let Some(m) = with(|s| s.module("Middle Click Extra").filter(|m| m.on).cloned()) else {
        return;
    };
    if m.p("Item") == "Friend" {
        let player = game.mob_hit().map(|(i, ..)| &game.mobs[i]).filter(|mob| kind_of(mob) == Kind::Player).map(|mob| mob.id);
        if let Some(name) = player.and_then(|id| game.remote_player_name(id)).map(str::to_owned) {
            let now = with(|s| s.flip_friend(&name));
            save(game);
            super::sunset_util::report(game, &format!("{name} {}", if now { "is a friend." } else { "is no longer a friend." }));
        }
        return;
    }
    let want = match m.p("Item") {
        "Pearl" => "ender_pearl",
        "Rocket" => "firework_rocket",
        _ => "experience_bottle",
    };
    let slot = (0..9).find(|&i| game.inventory.get(i).and_then(|s| s.as_ref()).is_some_and(|s| s.item.ends_with(want)));
    if let Some(slot) = slot {
        with(|s| s.mc_use = Some((slot, want)));
    }
}

// Middle Click Extra's use and its slot back, at the start of a tick as a hotbar key and a right click are: the slot,
// then the use, before the tick's move packet (a slot change or use after it is what Grim's Post check flags); the slot
// goes back three ticks later, never in the tick of the use (Grim's PacketOrderE). `busy`: a hit or a slot change went
// already this tick; both wait.
pub(super) fn middle_click_tick(game: &mut Game<'_>, busy: bool) -> bool {
    use winit::event::MouseButton;
    if busy {
        return false;
    }
    if let Some((slot, want)) = with(|s| s.mc_use.take()) {
        if !game.inventory.get(slot).and_then(|s| s.as_ref()).is_some_and(|s| s.item.ends_with(want)) {
            return false;
        }
        let before = game.selected;
        hold(game, slot);
        let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, true);
        let _ = super::remote::sunset_combat_remote::mouse(game, MouseButton::Right, false);
        with(|s| {
            if s.mc_back.is_none() {
                s.mc_back = Some((before, 3));
            }
        });
        return true;
    }
    match with(|s| s.mc_back) {
        Some((back, wait)) if wait <= 1 => {
            with(|s| s.mc_back = None);
            let moved = game.selected != back;
            hold(game, back);
            moved
        }
        Some((back, wait)) => {
            with(|s| s.mc_back = Some((back, wait - 1)));
            false
        }
        None => false,
    }
}
