// No Render: the shade behind the pause menu in a world is left out.
fn sunset_pause_dark(in_world: bool) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if in_world && crate::screen_effects::sunset_overlay::NO_MENU.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
