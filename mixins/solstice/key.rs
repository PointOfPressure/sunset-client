// Keys: the menu key, module binds, a bind being chosen, Air Jump.
fn solstice_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::solstice::key(game, key, pressed);
}
