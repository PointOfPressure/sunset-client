// The open click GUI goes over all the frame's HUD, after the chat, titles, tab list and toasts, and under its prompts.
fn sunset_gui_top(game: &crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset::gui_top(game, batch);
}
