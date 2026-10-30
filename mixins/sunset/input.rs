// While the click GUI is open the game's key binds do nothing.
fn sunset_keys() -> std::ops::ControlFlow<Result<()>> {
    if crate::game::sunset::gui_open() {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
