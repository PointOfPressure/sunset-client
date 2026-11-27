fn sunset_text_width_hook(game: &Game<'_>, text: &str) -> std::ops::ControlFlow<f32> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::game::sunset_font::ensure(&game.gui.atlas.font);
    if !crate::game::sunset_font::enabled() {
        return std::ops::ControlFlow::Continue(());
    }
    std::ops::ControlFlow::Break(crate::game::sunset_font::width(&game.gui.atlas.font, text))
}
