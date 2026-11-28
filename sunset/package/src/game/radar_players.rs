// Read-only Horizon player-info lookup for Radar.

pub(in crate::game) fn spectator(game: &crate::game::Game<'_>, entity_id: u32) -> bool {
    game.remote
        .as_ref()
        .and_then(|remote| {
            remote
                .level_players
                .get(&(entity_id as i32))
                .and_then(|uuid| remote.tab_players.get(uuid))
        })
        .is_some_and(|player| player.game_mode == 3)
}

// Locator radar: whether a player (by UUID) is tracked as an entity here, and their name from the player list.
pub(in crate::game) fn tracked(game: &crate::game::Game<'_>, uuid: u128) -> bool {
    game.remote.as_ref().is_some_and(|remote| remote.level_players.values().any(|u| *u == uuid))
}

pub(in crate::game) fn name(game: &crate::game::Game<'_>, uuid: u128) -> Option<String> {
    game.remote.as_ref()?.tab_players.get(&uuid).map(|p| p.name.clone())
}

pub(in crate::game) fn own_uuid(game: &crate::game::Game<'_>) -> Option<u128> {
    game.remote.as_ref().map(|remote| remote.profile)
}
