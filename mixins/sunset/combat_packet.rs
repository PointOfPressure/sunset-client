// Captured impulses reach observers only when the native handler replays them.
fn sunset_combat_packet(
    game: &mut crate::game::Game<'_>,
    packet: &horizon_protocol::packets::play::Clientbound,
) -> std::ops::ControlFlow<Result<()>> {
    if !game.native_window() { return std::ops::ControlFlow::Continue(()); }
    if crate::game::sunset_packet_delay::incoming(game, packet) {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    crate::game::sunset_combat_inventory::packet(game, packet);
    if let horizon_protocol::packets::play::Clientbound::SetEntityMotion(p) = packet {
        crate::game::sunset_melee_assist::note_velocity(game, p.id, p.velocity);
        crate::game::sunset_sword_bot::note_velocity(game, p.id, p.velocity);
    }
    std::ops::ControlFlow::Continue(())
}
