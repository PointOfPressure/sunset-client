// Streamer Mode: sign text in the world is drawn with the names replaced, centred again on its new width.
fn sunset_streamer_sign(scene: &Scene, call: &TextCall, out: &mut Vec<Vertex>) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    let Some(text) = crate::game::sunset_streamer::protect_str(&call.text) else {
        return std::ops::ControlFlow::Continue(());
    };
    let width = text.chars().map(|c| (scene.glyph)(c).advance).sum::<f32>().ceil();
    let shown = TextCall { x: -((width as i32) / 2) as f32, text, ..call.clone() };
    crate::game::sunset_streamer::guarded(|| draw_text(scene, &shown, out));
    std::ops::ControlFlow::Break(())
}
