// The control socket calls key_pressed directly; window input already stops
// in mapped_key. Keep base-game actions out of the overlay on either route.
fn sunset_gui_key_capture(game: &crate::game::Game<'_>) -> std::ops::ControlFlow<Result<()>> {
    if game.native_window()
        && game.native_on(crate::game::sunset::ID)
        && crate::game::sunset::gui_open()
    {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
