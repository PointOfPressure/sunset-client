// Streamer Mode: text is measured as it will be drawn, names replaced, so what is centred or wrapped by its width
// still lines up.
fn sunset_streamer_width(font: &Font, s: &str) -> std::ops::ControlFlow<f32> {
    match crate::hud::sunset_streamer::protect_str(s) {
        Some(t) => std::ops::ControlFlow::Break(crate::hud::sunset_streamer::guarded(|| font.width(&t))),
        None => std::ops::ControlFlow::Continue(()),
    }
}
