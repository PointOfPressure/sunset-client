// Sunset's text, in Sunset's font.
fn sunset_text_hook(game: &Game<'_>, batch: &mut HudBatch, text: &str, at: [f32; 2], rgb: u32, shadow: bool) -> std::ops::ControlFlow<()> {
    if !horizon_client::font::sunset_font::enabled() {
        return std::ops::ControlFlow::Continue(());
    }
    horizon_client::font::sunset_font::sync(&game.gui.atlas.font, batch.scale);
    // Each glyph on whole screen pixels, and a soft shadow a screen pixel away in place of Minecraft's hard one.
    let snap = |v: f32| (v * batch.scale).round() / batch.scale;
    let (x, y) = (snap(at[0]), snap(at[1]));
    let [_, r, g, b] = rgb.to_be_bytes();
    let color = rgba(r, g, b, 0xFF);
    let styled = horizon_client::font::sunset_font::text(text);
    if shadow {
        batch.text_styled(&game.gui.atlas, &styled, x + 1.0 / batch.scale, y + 1.0 / batch.scale, rgba(0, 0, 0, 0x60), false);
    }
    batch.text_styled(&game.gui.atlas, &styled, x, y, color, false);
    std::ops::ControlFlow::Break(())
}
