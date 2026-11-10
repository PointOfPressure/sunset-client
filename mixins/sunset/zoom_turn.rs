// Zoom: the mouse turns slower the further in the view is.
fn sunset_zoom_turn(game: &crate::game::Game<'_>) -> std::ops::ControlFlow<f64> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::game::sunset_render::zoom_turn(game) {
        Some(scale) => std::ops::ControlFlow::Break(scale),
        None => std::ops::ControlFlow::Continue(()),
    }
}
