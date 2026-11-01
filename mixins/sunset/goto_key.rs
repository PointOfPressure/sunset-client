fn sunset_goto_key(game: &mut crate::game::Game<'_>, key: winit::keyboard::KeyCode) {
    crate::game::agent::sunset_goto::key(game, key);
}
