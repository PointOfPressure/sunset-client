fn sunset_gui_move_shift(game: &Game<'_>, value: bool) -> bool {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    value && !crate::game::sunset_gui_move::suppress_shift(game)
}
