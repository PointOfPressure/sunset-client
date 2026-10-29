// Keys: the menu key, module binds, a bind being chosen, Air Jump.
fn meteor_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::meteor::key(game, key, pressed);
}
