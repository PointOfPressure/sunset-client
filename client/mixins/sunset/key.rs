// Keys: the menu key, module binds, a bind being chosen, Air Jump.
fn sunset_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::sunset::key(game, key, pressed);
}
