// Sunset's text, in Sunset's font. Sunset gives its colours as 0xRRGGBB (alpha 0 is opaque, as the API it was written
// for took them); the game's native_text now reads that as fully transparent and draws nothing, so Sunset's text is
// drawn here in either font.
fn sunset_text_hook(game: &Game<'_>, batch: &mut HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    let [a, r, g, b] = argb.to_be_bytes();
    let a = if a == 0 { 0xFF } else { a };
    crate::game::sunset_font::ensure(&game.gui.atlas.font);
    if !crate::game::sunset_font::enabled() {
        batch.text_styled(&game.gui.atlas, &horizon_client::text::Text::literal(text), at[0], at[1], rgba(r, g, b, a), shadow);
        return std::ops::ControlFlow::Break(());
    }
    crate::game::sunset_font::sync(&game.gui.atlas.font, batch.scale);
    // Each glyph on whole screen pixels, and a soft shadow a screen pixel away in place of Minecraft's hard one.
    let snap = |v: f32| (v * batch.scale).round() / batch.scale;
    let (x, y) = (snap(at[0]), snap(at[1]));
    let color = rgba(r, g, b, a);
    let styled = crate::game::sunset_font::text(text);
    if shadow {
        batch.text_styled(&game.gui.atlas, &styled, x + 1.0 / batch.scale, y + 1.0 / batch.scale, rgba(0, 0, 0, (u32::from(a) * 0x60 / 0xFF) as u8), false);
    }
    batch.text_styled(&game.gui.atlas, &styled, x, y, color, false);
    std::ops::ControlFlow::Break(())
}
