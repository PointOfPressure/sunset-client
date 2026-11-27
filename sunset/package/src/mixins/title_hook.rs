// No Render's Hide titles: the title and subtitle are not drawn at all. A title lands between ticks, so the per-tick
// clear alone (render.rs no_render) let it show for the frames until the next tick.
fn sunset_title() -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if crate::game::sunset_render::NO_TITLES.load(std::sync::atomic::Ordering::Relaxed) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
