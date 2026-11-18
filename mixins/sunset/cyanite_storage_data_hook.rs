fn sunset_storage_data(game: &crate::game::Game<'_>, pos: [i32; 3], kind: i32) {
    if horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
        crate::game::sunset_cyanite_storage::record(game, pos, kind);
    }
}
