fn sunset_text_width_hook(game: &Game<'_>, text: &str) -> std::ops::ControlFlow<f32> {
    if !horizon_client::font::sunset_font::enabled() {
        return std::ops::ControlFlow::Continue(());
    }
    std::ops::ControlFlow::Break(game.gui.atlas.font.width_text(&horizon_client::font::sunset_font::text(text)))
}
