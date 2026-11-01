// Same-module bridge: native item use retains cooldown and prediction rules.
pub(crate) fn latency(game: &crate::game::Game<'_>) -> Option<i32> {
    let remote = game.remote.as_ref()?;
    let milliseconds = remote.tab_players.get(&remote.profile)?.latency;
    (milliseconds >= 0).then_some(milliseconds)
}
pub(crate) fn container_state(game: &crate::game::Game<'_>) -> Option<i32> {
    Some(game.remote.as_ref()?.state_id)
}

// Use the client's most recently submitted input, including sprint state.
pub(crate) fn quiet(game: &crate::game::Game<'_>) -> bool {
    game.remote.as_ref().is_some_and(|remote| {
        !remote.was_sprinting && remote.last_input & 0b0011_1111 == 0
    })
}
pub(crate) fn sprint_synced(game: &crate::game::Game<'_>) -> bool {
    game.player.as_ref().is_some_and(|player|
        player.sprinting == crate::game::sunset_combat_wire::sent_sprinting(game))
}

// Kept profile names outlive tab-list removal, so they cannot establish that
// a player is still listed or that their current mode permits targeting.
pub(crate) fn player_mode(game: &crate::game::Game<'_>, id: u32) -> Option<i32> {
    let remote = game.remote.as_ref()?;
    let uuid = remote.level_players.get(&(id as i32))?;
    Some(remote.tab_players.get(uuid)?.game_mode)
}

pub(crate) fn use_main(game: &mut crate::game::Game<'_>) -> bool {
    game.remote_use_item(horizon_client::first_person::Hand::Main)
}

// The native UseItem packet and local prediction read Game's look, not the
// rotation already carried by a movement packet. Scope that look to this
// synchronous native call; no frame or camera interpolation is advanced.
pub(crate) fn use_main_at(game:&mut crate::game::Game<'_>,yaw:f32,pitch:f32)->bool {
    if !yaw.is_finite() || !pitch.is_finite() || !(-90.0..=90.0).contains(&pitch) {return false;}
    let camera=(game.yaw,game.pitch);
    game.yaw=(yaw+180.0).rem_euclid(360.0)-180.0;
    game.pitch=pitch;
    let used=game.remote_use_item(horizon_client::first_person::Hand::Main);
    game.yaw=camera.0;
    game.pitch=camera.1;
    used
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
