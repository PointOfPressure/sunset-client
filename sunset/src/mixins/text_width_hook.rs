fn sunset_text_width_hook(game: &Game<'_>, text: &str) -> std::ops::ControlFlow<f32> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if !crate::game::sunset_font::enabled() {
        return std::ops::ControlFlow::Continue(());
    }
    std::ops::ControlFlow::Break(game.gui.atlas.font.width_text(&crate::game::sunset_font::text(text)))
}
