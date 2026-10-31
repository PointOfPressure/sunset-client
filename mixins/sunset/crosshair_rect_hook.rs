fn sunset_crosshair_rect(
    batch: &crate::hud::HudBatch,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rect: crate::hud::Rect,
    color: u32,
) -> std::ops::ControlFlow<()> {
    if crate::hud::sunset_crosshair::suppress_mark(batch, x, y, w, h, rect, color) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
