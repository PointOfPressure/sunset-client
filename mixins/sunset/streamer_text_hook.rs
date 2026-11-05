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
) -> std::ops::ControlFlow<f32> {
    match crate::hud::sunset_streamer::protect(text) {
        Some(t) => std::ops::ControlFlow::Break(crate::hud::sunset_streamer::guarded(|| {
            batch.styled_text(atlas, &t, x, y, color, shadow, world)
        })),
        None => std::ops::ControlFlow::Continue(()),
    }
}
