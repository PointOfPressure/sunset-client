// A mouse press goes to the click GUI while it is open, before the game sees it; then to the Loadouts panel and
// Shulker Peek's view.
fn sunset_press(game: &mut crate::game::Game<'_>, button: winit::event::MouseButton) -> std::ops::ControlFlow<bool> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if crate::game::sunset::gui_open() {
        crate::game::sunset::gui_press(game, button);
        return std::ops::ControlFlow::Break(true);
    }
    if crate::game::sunset_loadouts::press(game, button == winit::event::MouseButton::Right) {
        return std::ops::ControlFlow::Break(true);
    }
    if crate::game::sunset_render::peek_press(game, button == winit::event::MouseButton::Middle) {
        return std::ops::ControlFlow::Break(true);
    }
    std::ops::ControlFlow::Continue(())
}
