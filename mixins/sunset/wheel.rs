// The wheel scrolls a click GUI panel, and stays off the hotbar while it is open.
fn sunset_wheel(game: &crate::game::Game<'_>, steps: f32, claimed: &mut bool) {
    if crate::game::sunset::gui_open() {
        crate::game::sunset::gui_wheel(game, steps);
        *claimed = true;
    }
}
