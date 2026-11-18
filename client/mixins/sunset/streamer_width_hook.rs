// Streamer Mode: text is measured as it will be drawn, names replaced, so what is centred or wrapped by its width
// still lines up.
fn sunset_streamer_width(font: &Font, s: &str) -> std::ops::ControlFlow<f32> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::hud::sunset_streamer::protect_str(s) {
        Some(t) => std::ops::ControlFlow::Break(crate::hud::sunset_streamer::guarded(|| font.width(&t))),
        None => std::ops::ControlFlow::Continue(()),
    }
}
