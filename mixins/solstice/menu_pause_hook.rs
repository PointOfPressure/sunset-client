// No Render: the shade behind the pause menu in a world is left out.
fn solstice_pause_dark(in_world: bool) -> std::ops::ControlFlow<()> {
    if in_world && crate::screen_effects::solstice_overlay::NO_MENU.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
