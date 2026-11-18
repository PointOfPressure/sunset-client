// Streamer Mode: a name tag over a player or a named mob drawn with the names replaced (cached as drawn).
fn sunset_streamer_name_tag(
    this: &mut NameTagLayouts,
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    text: &Text,
    y: f32,
    alpha: u32,
    background: bool,
) -> ::core::ops::ControlFlow<usize> {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return ::core::ops::ControlFlow::Continue(());
    }
    match crate::game::sunset_streamer::protect(text) {
        Some(shown) => ::core::ops::ControlFlow::Break(crate::game::sunset_streamer::guarded(|| {
            this.name_tag(batch, atlas, &shown, y, alpha, background)
        })),
        None => ::core::ops::ControlFlow::Continue(()),
    }
}
