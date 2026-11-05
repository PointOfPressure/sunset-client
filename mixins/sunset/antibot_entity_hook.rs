fn sunset_antibot_entity(game: &crate::game::Game<'_>, id: i32) {
    crate::game::sunset_antibot::invalidate(game, id);
}
