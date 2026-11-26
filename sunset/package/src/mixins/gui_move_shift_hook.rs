fn sunset_gui_move_shift(game: &crate::game::Game<'_>, value: bool) -> bool {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    value && !crate::game::sunset_gui_move::suppress_shift(game)
}
