// The chat's suggestions for . commands (chat_suggest.rs), over the chat screen's box.
fn sunset_chat_suggest_draw(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    phase: horizon_client::screen_draw::RenderPhase,
) {
    if !::sunset_core::lifecycle::RUNTIME.running() || phase != horizon_client::screen_draw::RenderPhase::Foreground {
        return;
    }
    crate::game::sunset_chat_suggest::draw(game, batch);
}
