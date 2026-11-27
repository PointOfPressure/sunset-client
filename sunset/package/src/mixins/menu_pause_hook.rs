// No Render: the shade behind the pause menu in a world is left out.
fn sunset_pause_dark(in_world: bool) -> std::ops::ControlFlow<()> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if in_world && ::sunset_core::overlay::NO_MENU.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
