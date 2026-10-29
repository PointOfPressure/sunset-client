// A mouse press goes to the click GUI while it is open, before the game sees it.
fn solstice_press(game: &mut crate::game::Game<'_>, button: winit::event::MouseButton) -> std::ops::ControlFlow<bool> {
    if crate::game::solstice::gui_open() {
        crate::game::solstice::gui_press(game, button);
        return std::ops::ControlFlow::Break(true);
    }
    std::ops::ControlFlow::Continue(())
}
