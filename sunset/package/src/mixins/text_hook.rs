// Sunset's text, in Sunset's font. Sunset gives its colours as 0xRRGGBB (alpha 0 is opaque, as the API it was written
// for took them); the game's native_text now reads that as fully transparent and draws nothing, so Sunset's text is
// drawn here in either font.
fn sunset_text_hook(game: &Game<'_>, batch: &mut HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::game::sunset_font::draw(game, batch, text, at, argb, shadow);
    std::ops::ControlFlow::Break(())
}
