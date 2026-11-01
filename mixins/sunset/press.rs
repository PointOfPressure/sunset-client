// A mouse press goes to the click GUI while it is open, before the game sees it; then to Shulker Peek's view.
fn sunset_press(game: &mut crate::game::Game<'_>, button: winit::event::MouseButton) -> std::ops::ControlFlow<bool> {
    if crate::game::sunset::gui_open() {
        crate::game::sunset::gui_press(game, button);
        return std::ops::ControlFlow::Break(true);
    }
    if crate::game::sunset_render::peek_press(game, button == winit::event::MouseButton::Middle) {
        return std::ops::ControlFlow::Break(true);
    }
    std::ops::ControlFlow::Continue(())
}
