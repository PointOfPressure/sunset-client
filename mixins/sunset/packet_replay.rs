// Register as a module inside crates/horizon-client/src/game/remote.rs.
// Reuses the native packet handler, including every current protocol field.
pub(crate) fn apply(
    game: &mut crate::game::Game<'_>,
    packet: horizon_protocol::packets::play::Clientbound,
) -> horizon_client::Result<()> {
    game.handle_packet(packet)
}
