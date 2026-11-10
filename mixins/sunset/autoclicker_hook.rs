fn sunset_autoclicker_input(game: &mut crate::game::Game<'_>) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_autoclicker::tick(game);
}
