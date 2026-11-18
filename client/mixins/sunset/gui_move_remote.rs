// The own-inventory transaction uses the same menu prediction as an ordinary click.
use super::*;
use horizon_protocol::packets::play::{Clientbound as C, Serverbound as S};

#[derive(Clone)]
pub(crate) struct Snapshot {
    inventory: Vec<Option<ItemStack>>,
    armor: [Option<ItemStack>; 4],
    offhand: Option<ItemStack>,
    grid: Vec<Option<ItemStack>>,
    carried: Option<ItemStack>,
    result: Option<ItemStack>,
    state: i32,
}

impl Snapshot {
    pub(crate) fn capture(game: &Game<'_>) -> Self {
        Self {
            inventory: game.inventory.clone(),
            armor: game.armor.clone(),
            offhand: game.offhand.clone(),
            grid: game.craft_grid.clone(),
            carried: game.carried.clone(),
            result: game.remote.as_ref().and_then(|r| r.craft_result.clone()),
            state: game.remote.as_ref().map_or(0, |r| r.state_id),
        }
    }

    pub(crate) fn restore(&self, game: &mut Game<'_>) {
        game.inventory.clone_from(&self.inventory);
        game.armor.clone_from(&self.armor);
        game.offhand.clone_from(&self.offhand);
        game.craft_grid.clone_from(&self.grid);
        game.carried.clone_from(&self.carried);
        if let Some(r) = game.remote.as_mut() {
            r.craft_result.clone_from(&self.result);
            r.state_id = self.state;
        }
    }
}

fn apply(game: &mut Game<'_>, packet: &C) {
    match packet {
        C::ContainerSetContent(p) if p.container == 0 => {
            game.remote_menu_state(0, p.state);
            for (slot, item) in p.items.iter().enumerate() {
                let stack = game.to_stack(item);
                game.set_menu_slot(0, slot as i32, stack);
            }
            game.carried = game.to_stack(&p.carried);
            game.remote_synced();
        }
        C::ContainerSetSlot(p) if p.container == 0 => {
            game.remote_menu_state(0, p.state);
            let stack = game.to_stack(&p.item);
            game.set_menu_slot(0, i32::from(p.slot), stack);
            game.remote_synced();
        }
        C::SetCursorItem(p) => game.carried = game.to_stack(&p.item),
        C::SetPlayerInventory(p) => {
            let stack = game.to_stack(&p.item);
            match p.slot {
                0..=35 => game.inventory[p.slot as usize] = stack,
                36..=39 => game.armor[3 - (p.slot - 36) as usize] = stack,
                40 => game.offhand = stack,
                _ => {}
            }
            game.remote_synced();
        }
        _ => {}
    }
}

fn input(value: i32) -> Option<Input> {
    Some(match value {
        0 => Input::Pickup,
        1 => Input::QuickMove,
        2 => Input::Swap,
        3 => Input::Clone,
        4 => Input::Throw,
        5 => Input::QuickCraft,
        6 => Input::PickupAll,
        _ => return None,
    })
}

// An update invalidates the hashes, not the user's sequence of clicks. Re-run that sequence on the new authority.
// `Err` leaves the authoritative inventory in place, rather than sending old hashes or inventing a resync click.
pub(crate) fn rebase(
    game: &mut Game<'_>,
    base: &mut Snapshot,
    packets: &mut Vec<S>,
    updates: &[C],
) -> bool {
    base.restore(game);
    for packet in updates {
        apply(game, packet);
    }
    *base = Snapshot::capture(game);
    let mut clicks = Vec::new();
    for packet in packets.iter() {
        if let S::ContainerClick(p) = packet {
            let Some(kind) = input(p.input) else {
                return false;
            };
            clicks.push((p.slot, i32::from(p.button), kind));
        }
    }
    let mut prediction = None;
    let handled = game.menu_session(|menu, env, _| {
        prediction = predict_clicks(menu, env, &clicks);
        menu.player_sounds.clear();
        menu.sounds.clear();
        menu.equipped.clear();
        menu.item_crafted.clear();
        Some(prediction.is_some())
    });
    let Some((prediction, _)) = prediction.filter(|_| handled) else {
        base.restore(game);
        return false;
    };
    let mut prediction = prediction.into_iter();
    for packet in packets.iter_mut() {
        if let S::ContainerClick(p) = packet {
            let Some((slot, button, kind, changed, carried)) = prediction.next() else {
                return false;
            };
            if let Some((_, result)) = changed.iter().find(|(slot, _)| *slot == 0) {
                if let Some(remote) = game.remote.as_mut() {
                    remote.craft_result = result.clone();
                }
            }
            *p = play::ContainerClick {
                container: 0,
                state: base.state,
                slot,
                button: button as i8,
                input: input_ordinal(kind),
                changed: changed
                    .iter()
                    .map(|(slot, stack)| (*slot, game.hashed_stack(stack)))
                    .collect(),
                carried: game.hashed_stack(&carried),
            };
        }
    }
    if game.screen != Screen::Inventory {
        game.carried = None;
        game.craft_grid.fill(None);
    }
    true
}

pub(crate) fn restore_authority(game: &mut Game<'_>, base: &Snapshot, updates: &[C]) {
    base.restore(game);
    for packet in updates {
        apply(game, packet);
    }
    if game.screen != Screen::Inventory {
        game.carried = None;
        game.craft_grid.fill(None);
    }
}

// A dropped/delayed input changes the engine's scheduled cache before it changes the server. A stopping tick
// starts that cache at the actual wire state, so native sendChanges retries an idle input/sprint stop if needed.
pub(crate) fn retry_idle(game: &mut Game<'_>) {
    let input = crate::game::sunset_combat_wire::sent_input(game);
    let sprinting = crate::game::sunset_combat_wire::sent_sprinting(game);
    if let Some(remote) = game.remote.as_mut() {
        remote.last_input = input;
        remote.was_sprinting = sprinting;
    }
}
