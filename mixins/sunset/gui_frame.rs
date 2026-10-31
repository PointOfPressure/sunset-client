// GUI input ownership must not depend on whether the HUD was drawn.
fn sunset_gui_frame(game: &mut crate::game::Game<'_>) {
    crate::game::sunset::gui_frame(game);
}
