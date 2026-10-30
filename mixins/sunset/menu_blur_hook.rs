// No Render: the blur behind the pause menu and other screens is left out with the dark sheet.
fn sunset_menu_blur() -> std::ops::ControlFlow<()> {
    if crate::screen_effects::sunset_overlay::NO_MENU.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
