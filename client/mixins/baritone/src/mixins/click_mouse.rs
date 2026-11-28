// A mouse button while playing: the click screen of `#click` acts when a mouse button is let go.
fn baritone_click_mouse(game: &mut crate::game::Game<'_>, button: winit::event::MouseButton, pressed: bool) {
    if !crate::game::sunset_baritone_full::present(game) {
        return;
    }
    let name = horizon_client::keybinds::mouse_name(button);
    crate::game::baritone::click_key(game, &*name, pressed);
}
