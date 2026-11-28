// A key or a mouse button while playing: the click screen of `#click` acts when a mouse button is let go.
fn baritone_click_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    crate::game::baritone::click_key(game, key, pressed);
}
