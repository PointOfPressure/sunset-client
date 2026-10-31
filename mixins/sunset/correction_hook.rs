// A server position correction invalidates the previous movement baseline.
fn sunset_server_correction(game: &mut crate::game::Game<'_>) {
    crate::game::sunset::server_correction(game);
}
