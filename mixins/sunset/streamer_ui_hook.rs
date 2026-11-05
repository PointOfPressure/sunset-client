// Streamer Mode: Horizon's own screens (its title screen and menus, in their own font) draw and measure text with
// the names replaced.
fn sunset_streamer_ui(
    ui: &mut Ui<'_>,
    face: Face,
    size: f32,
    line: f32,
    x: f32,
    y: f32,
    color: u32,
    text: &str,
) -> std::ops::ControlFlow<f32> {
    match horizon_client_render::hud::sunset_streamer::protect_str(text) {
        Some(t) => std::ops::ControlFlow::Break(horizon_client_render::hud::sunset_streamer::guarded(|| {
            ui.text_line(face, size, line, x, y, color, &t)
        })),
        None => std::ops::ControlFlow::Continue(()),
    }
}
