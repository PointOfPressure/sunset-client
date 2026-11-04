// A server position correction invalidates the previous movement baseline; Debug Report notes it.
fn sunset_server_correction(game: &mut crate::game::Game<'_>, position: &horizon_protocol::packets::play::PlayerPosition) {
    crate::game::sunset_debug::correction(game, position);
    crate::game::sunset::server_correction(game);
}
