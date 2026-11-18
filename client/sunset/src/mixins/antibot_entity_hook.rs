fn sunset_antibot_entity(game: &crate::game::Game<'_>, id: i32) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_antibot::invalidate(game, id);
}
