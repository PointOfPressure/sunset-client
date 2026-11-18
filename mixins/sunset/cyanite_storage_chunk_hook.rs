fn sunset_storage_chunk(game: &crate::game::Game<'_>, p: &horizon_protocol::packets::play::LevelChunkWithLight) {
    if game.native_window() && horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
        crate::game::sunset_cyanite_storage::chunk(game, p.x, p.z);
    }
}
