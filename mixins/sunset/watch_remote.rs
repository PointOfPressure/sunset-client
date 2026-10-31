// Read-only adapter for the pinned connection's private player-list fields.
use crate::game::{
    Game,
    sunset_watch_state::{MAX_ROSTER, NearbyPlayer, TabPlayer},
};

pub(in crate::game) struct Snapshot {
    pub roster: Vec<TabPlayer>,
    pub nearby: Vec<NearbyPlayer>,
    pub local: [f64; 3],
}

pub(in crate::game) fn snapshot(game: &Game<'_>) -> Option<Snapshot> {
    let remote = game.remote.as_ref().filter(|r| r.logged_in)?;
    let local = game.player.as_ref()?.position;
    if remote.tab_players.len() > MAX_ROSTER || remote.level_players.len() > MAX_ROSTER {
        return None; // Never convert a truncated roster into disappearance evidence.
    }
    let roster = remote
        .tab_players
        .values()
        .filter(|p| p.uuid != remote.profile)
        .map(|p| TabPlayer {
            uuid: p.uuid,
            name: p.name.clone(),
            listed: remote.listed.contains(&p.uuid),
            spectator: p.game_mode == horizon_client::tab_list::SPECTATOR,
        })
        .collect();
    let nearby = game
        .mobs
        .iter()
        .filter(|m| m.kind.id == "minecraft:player" && m.alive() && !m.removed)
        .filter_map(|m| {
            remote
                .level_players
                .get(&(m.id as i32))
                .filter(|id| **id != remote.profile)
                .map(|id| NearbyPlayer {
                    uuid: *id,
                    position: [m.position.x, m.position.y, m.position.z],
                })
        })
        .take(MAX_ROSTER)
        .collect();
    Some(Snapshot {
        roster,
        nearby,
        local: [local.x, local.y, local.z],
    })
}
