// SPDX-License-Identifier: MIT
// Read-only Horizon player-info lookup for Radar. AI-assisted contribution.

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
