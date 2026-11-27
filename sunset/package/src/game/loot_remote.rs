// Whether an entity event 3 is a player dying, and where.
pub(crate) enum Death {
    Other,
    Player(Option<[f64; 3]>),
}
pub(crate) fn player_death(game: &crate::game::Game<'_>, id: i32) -> Death {
    let Some(remote) = game.remote.as_ref() else {
        return Death::Other;
    };
    if remote.entity_id == id {
        return Death::Player(game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]));
    }
    if !remote.level_players.contains_key(&id) {
        return Death::Other;
    }
    match remote.entities.get(&id) {
        Some(entity) if entity.kind == "minecraft:player" => Death::Player(Some(entity.position)),
        Some(_) => Death::Other,
        None => Death::Player(None),
    }
}
