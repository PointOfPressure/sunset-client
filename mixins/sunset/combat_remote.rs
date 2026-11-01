// Bits of the connection the combat modules read: the ping, the server's sprint, a player's game mode.
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
