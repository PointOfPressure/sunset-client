// Streamer Mode: a centred layout (a title, the action bar, the boss bars) drawn with the names replaced.
fn sunset_streamer_centred(value: Vec<hud_layout::GuiDraw>, game: &Game<'_>) -> Vec<hud_layout::GuiDraw> {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return value;
    }
    crate::game::sunset_streamer::protect_draws(value, &game.gui.atlas.font, true)
}
