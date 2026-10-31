fn sunset_watch_local(game: &crate::game::Game<'_>) {
    crate::game::sunset_watch_runtime::connecting(game, "local", true);
}
