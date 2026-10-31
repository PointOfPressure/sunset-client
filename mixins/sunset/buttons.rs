// While the click GUI is open the mouse buttons bound to attack and use do nothing.
fn sunset_buttons(game: &crate::game::Game<'_>, button: winit::event::MouseButton, pressed: bool) -> std::ops::ControlFlow<Result<()>> {
    crate::game::sunset_city_trap::user_mouse(game, button, pressed);
    crate::game::sunset_combat_macros::user_mouse(game, button, pressed);
    if crate::game::sunset::gui_open() {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
