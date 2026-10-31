// Same-module bridge: native item use retains cooldown and prediction rules.
pub(crate) fn use_main(game: &mut crate::game::Game<'_>) -> bool {
    game.remote_use_item(horizon_client::first_person::Hand::Main)
}

// Native unpredicted menu input: leave local slots untouched and report no
// predicted changes. A matching predicted click is silently accepted by Java;
// an unpredicted swap produces the real source/destination slot updates needed
// by the inventory transaction's two-slot acknowledgment policy.
pub(crate) fn swap_unpredicted(
    game: &mut crate::game::Game<'_>, menu: &(i32, String), from: usize, to: usize,
) -> bool {
    if game.carried.is_some() || from == to || &game.remote_menu() != menu
        || !(to < 9 || to == 504) { return false; }
    let Some(state) = game.remote.as_ref().map(|r| r.state_id) else { return false; };
    let Some(slot) = crate::game::Game::menu_slot_index(&menu.1, from)
        .and_then(|slot| i16::try_from(slot).ok()) else { return false; };
    let button = if to == 504 { 40 } else { to as i8 };
    game.send(horizon_protocol::packets::play::Serverbound::ContainerClick(
        horizon_protocol::packets::play::ContainerClick {
            container: menu.0, state, slot, button,
            input: super::input_ordinal(horizon_client::menu::Input::Swap),
            changed: Vec::new(), carried: None,
        },
    ));
    true
}
