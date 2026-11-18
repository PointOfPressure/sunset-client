fn sunset_gui_move_state(game: &mut Game<'_>, container: i32) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_gui_move::menu_state(game, container);
}
