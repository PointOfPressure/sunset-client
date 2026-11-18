// Streamer Mode: every text drawn in the game's font (HUD, chat, tab list, scoreboard, titles, boss bars, tooltips,
// screens, name tags in the world) is drawn with the names it hides replaced.
fn sunset_streamer_text(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    text: &crate::text::Text,
    x: f32,
    y: f32,
    color: u32,
    shadow: bool,
    world: bool,
    outline: Option<(u32, [f32; 2])>,
) -> std::ops::ControlFlow<f32> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::hud::sunset_streamer::protect(text) {
        Some(t) => std::ops::ControlFlow::Break(crate::hud::sunset_streamer::guarded(|| {
            batch.styled_text(atlas, &t, x, y, color, shadow, world, outline)
        })),
        None => std::ops::ControlFlow::Continue(()),
    }
}
