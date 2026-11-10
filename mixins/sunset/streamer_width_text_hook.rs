// Streamer Mode: a styled text is measured as it will be drawn, names replaced.
fn sunset_streamer_width_text(font: &Font, t: &Text) -> std::ops::ControlFlow<f32> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::hud::sunset_streamer::protect(t) {
        Some(t) => std::ops::ControlFlow::Break(crate::hud::sunset_streamer::guarded(|| font.width_text(&t))),
        None => std::ops::ControlFlow::Continue(()),
    }
}
