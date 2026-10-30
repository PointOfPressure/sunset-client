// Solstice's text, in Solstice's font.
fn solstice_text_hook(game: &Game<'_>, batch: &mut HudBatch, text: &str, at: [f32; 2], rgb: u32, shadow: bool) -> std::ops::ControlFlow<()> {
    if !horizon_client::font::solstice_font::enabled() {
        return std::ops::ControlFlow::Continue(());
    }
    horizon_client::font::solstice_font::sync(&game.gui.atlas.font, batch.scale);
    let [x, y] = at;
    let [_, r, g, b] = rgb.to_be_bytes();
    let color = rgba(r, g, b, 0xFF);
    let styled = horizon_client::font::solstice_font::text(text);
    batch.text_styled(&game.gui.atlas, &styled, x, y, color, shadow);
    std::ops::ControlFlow::Break(())
}
