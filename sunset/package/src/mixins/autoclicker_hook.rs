fn sunset_autoclicker_input(game: &mut crate::game::Game<'_>) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_autoclicker::tick(game);
}
