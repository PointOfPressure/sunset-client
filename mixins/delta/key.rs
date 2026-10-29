// Keys: the menu key, module binds, a bind being chosen, Air Jump.
fn delta_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::delta::key(game, key, pressed);
}
