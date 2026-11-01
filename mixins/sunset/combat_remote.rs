// Bits of the connection the combat modules read and send: the ping, the server's sprint, a player's game mode, a
// menu's state, a swap click.
pub(crate) fn latency(game: &crate::game::Game<'_>) -> Option<i32> {
    let remote = game.remote.as_ref()?;
    let milliseconds = remote.tab_players.get(&remote.profile)?.latency;
    (milliseconds >= 0).then_some(milliseconds)
}
pub(crate) fn sprint_synced(game: &crate::game::Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|player| player.sprinting == crate::game::sunset_combat_wire::sent_sprinting(game))
}

// Kept profile names outlive tab-list removal, so they cannot establish that
// a player is still listed or that their current mode permits targeting.
pub(crate) fn player_mode(game: &crate::game::Game<'_>, id: u32) -> Option<i32> {
    let remote = game.remote.as_ref()?;
    let uuid = remote.level_players.get(&(id as i32))?;
    Some(remote.tab_players.get(uuid)?.game_mode)
}

// The menu's state id, as the last server packet left it.
pub(crate) fn container_state(game: &crate::game::Game<'_>) -> Option<i32> {
    Some(game.remote.as_ref()?.state_id)
}

// The last input sent had no movement keys and no sprint: a menu click now is one a standing player makes.
pub(crate) fn quiet(game: &crate::game::Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|remote| !remote.was_sprinting && remote.last_input & 0b0011_1111 == 0)
}

// A swap click (inventory slot `from` with hotbar slot `to`, or the offhand as 504) in `menu`, sent without changing
// the slots here first: the server's slot packets then say what really happened.
pub(crate) fn swap_unpredicted(game: &mut crate::game::Game<'_>, menu: &(i32, String), from: usize, to: usize) -> bool {
    if game.carried.is_some() || from == to || &game.remote_menu() != menu || !(to < 9 || to == 504) {
        return false;
    }
    let Some(state) = game.remote.as_ref().map(|r| r.state_id) else {
        return false;
    };
    let Some(slot) = crate::game::Game::menu_slot_index(&menu.1, from).and_then(|slot| i16::try_from(slot).ok()) else {
        return false;
    };
    game.send(horizon_protocol::packets::play::Serverbound::ContainerClick(horizon_protocol::packets::play::ContainerClick {
        container: menu.0,
        state,
        slot,
        button: if to == 504 { 40 } else { to as i8 },
        input: super::input_ordinal(horizon_client::menu::Input::Swap),
        changed: Vec::new(),
        carried: None,
    }));
    true
}
