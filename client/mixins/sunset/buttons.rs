// The mouse buttons the game is pressed with: the combat macros see your right clicks. While the click GUI is open
// the buttons bound to attack and use do nothing; a release still lets go (a shield held up as the menu opened comes
// down when you let go in it).
fn sunset_buttons(game: &crate::game::Game<'_>, button: winit::event::MouseButton, pressed: bool) -> std::ops::ControlFlow<Result<()>> {
    crate::game::remote::sunset_combat_remote::native_mouse(game, button, pressed);
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::game::sunset_autoclicker::button(button);
    crate::game::sunset_combat_macros::user_mouse(game, button, pressed);
    if pressed && crate::game::sunset::gui_open() {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
