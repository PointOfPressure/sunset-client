fn sunset_gui_move_click(game: &mut Game<'_>) -> std::ops::ControlFlow<bool> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if crate::game::sunset_gui_move::before_click(game) {
        std::ops::ControlFlow::Continue(())
    } else {
        std::ops::ControlFlow::Break(false)
    }
}
