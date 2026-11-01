// F5 does nothing while Freecam flies: its view stays the one it draws from.
fn sunset_perspective() -> std::ops::ControlFlow<()> {
    if crate::game::sunset_render::freecam_flying() {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
