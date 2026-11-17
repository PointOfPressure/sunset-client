// Streamer Mode: Horizon's own screens measure text as it will be drawn, names replaced.
fn sunset_streamer_ui_measure(ui: &Ui<'_>, face: Face, size: f32, text: &str) -> std::ops::ControlFlow<f32> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::game::sunset_streamer::protect_str(text) {
        Some(t) => std::ops::ControlFlow::Break(crate::game::sunset_streamer::guarded(|| {
            ui.measure(face, size, &t)
        })),
        None => std::ops::ControlFlow::Continue(()),
    }
}
