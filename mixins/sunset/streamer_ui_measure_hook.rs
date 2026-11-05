// Streamer Mode: Horizon's own screens measure text as it will be drawn, names replaced.
fn sunset_streamer_ui_measure(ui: &Ui<'_>, face: Face, size: f32, text: &str) -> std::ops::ControlFlow<f32> {
    match horizon_client_render::hud::sunset_streamer::protect_str(text) {
        Some(t) => std::ops::ControlFlow::Break(horizon_client_render::hud::sunset_streamer::guarded(|| {
            ui.measure(face, size, &t)
        })),
        None => std::ops::ControlFlow::Continue(()),
    }
}
