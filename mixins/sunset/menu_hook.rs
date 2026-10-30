// No Render: the dark sheet drawn behind an inventory, a book, a trade and the like is left out.
fn sunset_menu_dark(this: &HudBatch, x: f32, y: f32, w: f32, h: f32, color: u32) -> std::ops::ControlFlow<()> {
    use std::sync::atomic::Ordering::Relaxed;
    if crate::screen_effects::sunset_overlay::NO_MENU.load(Relaxed)
        && x == 0.0
        && y == 0.0
        && w == this.width
        && h == this.height
        && color == rgba(16, 16, 16, 160)
    {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
