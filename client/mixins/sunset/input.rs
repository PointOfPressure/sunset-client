// While the click GUI is open the game's key binds do nothing; a release still lets go of what it held.
fn sunset_keys(pressed: bool) -> std::ops::ControlFlow<Result<()>> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    if pressed && crate::game::sunset::gui_open() {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
