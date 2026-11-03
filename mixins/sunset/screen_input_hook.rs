// GUI Move: the PlayerInput a tick sends behind a screen says the keys its move took, so the server predicts the move
// that was made (the call again with them passes through here).
fn sunset_screen_input(game: &mut Game<'_>, input: u8) -> std::ops::ControlFlow<()> {
    match horizon_client::motion::sunset_motion::screen_input(input) {
        Some(input) => std::ops::ControlFlow::Break(game.remote_send_state(input)),
        None => std::ops::ControlFlow::Continue(()),
    }
}
