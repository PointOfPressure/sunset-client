// F1 leaves the HUD shown while the menu is open: the menu is drawn with the HUD, and hidden it would still take the
// mouse and keys.
fn sunset_hide_hud() -> std::ops::ControlFlow<()> {
    if crate::game::sunset::gui_open() {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
