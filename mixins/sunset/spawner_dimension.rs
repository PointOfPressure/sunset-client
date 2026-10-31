fn sunset_spawner_dimension(game: &mut crate::game::Game<'_>, spawn: &play::SpawnInfo) {
    if game.native_window() { crate::game::sunset_spawners::dimension(&spawn.dimension); }
}
