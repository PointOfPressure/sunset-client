// GUI Move: the PlayerInput a tick sends behind a screen says the keys its move took, so the server predicts the move
// that was made (the call again with them passes through here). Debug Report notes the change.
fn sunset_screen_input(game: &mut Game<'_>, input: u8) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match horizon_client::motion::sunset_motion::screen_input(input) {
        Some(changed) => {
            crate::game::sunset_debug::gui_move_input(input, changed);
            std::ops::ControlFlow::Break(game.remote_send_state(changed))
        }
        None => std::ops::ControlFlow::Continue(()),
    }
}
