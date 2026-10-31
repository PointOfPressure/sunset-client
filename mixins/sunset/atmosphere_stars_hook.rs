// The procedural field owns stars only after a custom sky pipeline succeeded.
// This restores vanilla stars automatically on every disabled or unsupported frame.
fn sunset_atmosphere_stars() -> std::ops::ControlFlow<()> {
    if super::sunset_atmosphere_renderer::owns_sky() {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
