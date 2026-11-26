// Mouse buttons while playing, as keys: binds on the side buttons and the wheel click.
fn sunset_mouse_key(game: &mut crate::game::Game<'_>, button: winit::event::MouseButton, pressed: bool) {
    let name = horizon_client::keybinds::mouse_name(button);
    crate::game::sunset::key(game, &name, pressed);
}
