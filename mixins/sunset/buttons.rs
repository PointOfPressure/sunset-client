// While the click GUI is open the mouse buttons bound to attack and use do nothing.
fn sunset_buttons() -> std::ops::ControlFlow<Result<()>> {
    if crate::game::sunset::gui_open() {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
