// Sunset's text, in Sunset's font.
fn sunset_text_hook(game: &Game<'_>, batch: &mut HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::game::sunset_font::ensure(&game.gui.atlas.font);
    if !crate::game::sunset_font::enabled() {
        return std::ops::ControlFlow::Continue(());
    }
    crate::game::sunset_font::sync(&game.gui.atlas.font, batch.scale);
    // Each glyph on whole screen pixels, and a soft shadow a screen pixel away in place of Minecraft's hard one.
    let snap = |v: f32| (v * batch.scale).round() / batch.scale;
    let (x, y) = (snap(at[0]), snap(at[1]));
    let [a, r, g, b] = argb.to_be_bytes();
    if a == 0 {
        return std::ops::ControlFlow::Break(());
    }
    let color = rgba(r, g, b, a);
    let styled = crate::game::sunset_font::text(text);
    if shadow {
        batch.text_styled(&game.gui.atlas, &styled, x + 1.0 / batch.scale, y + 1.0 / batch.scale, rgba(0, 0, 0, (u32::from(a) * 0x60 / 0xFF) as u8), false);
    }
    batch.text_styled(&game.gui.atlas, &styled, x, y, color, false);
    std::ops::ControlFlow::Break(())
}
