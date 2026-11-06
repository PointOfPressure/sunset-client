fn sunset_gui_move_shift(game: &Game<'_>, value: bool) -> bool {
    value && !crate::game::sunset_gui_move::suppress_shift(game)
}
