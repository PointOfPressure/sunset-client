fn sunset_spawner_disconnect(game: &mut crate::game::Game<'_>) {
    if game.native_window() { crate::game::sunset_spawners::reset(); }
}
