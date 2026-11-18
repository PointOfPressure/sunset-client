// A mouse button pressed, on any screen: the click screen of `#click` remembers the block it went down on.
fn baritone_click_press(game: &mut crate::game::Game<'_>) {
    if !crate::game::sunset_baritone_full::present(game) { return; }

    crate::game::baritone::click_press(game);
}
