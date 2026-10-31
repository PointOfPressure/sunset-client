fn sunset_watch_connect(game: &crate::game::Game<'_>, address: &str) {
    crate::game::sunset_watch_runtime::connecting(game, address, false);
}
