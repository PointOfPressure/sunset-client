// Keys while playing: the menu key, module binds, a bind being chosen, Air Jump. A repeat is not a new press.
fn sunset_key(game: &mut crate::game::Game<'_>, key: winit::keyboard::KeyCode, pressed: bool, repeat: bool) {
    if repeat {
        return;
    }
    if let Some(name) = horizon_client::keybinds::key_name(key) {
        crate::game::sunset::key(game, name, pressed);
    }
}
