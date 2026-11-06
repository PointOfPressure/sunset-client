fn sunset_gui_move_click(game: &mut Game<'_>) -> std::ops::ControlFlow<bool> {
    if crate::game::sunset_gui_move::before_click(game) {
        std::ops::ControlFlow::Continue(())
    } else {
        std::ops::ControlFlow::Break(false)
    }
}
