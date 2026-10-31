// Captured impulses reach observers only when the native handler replays them.
fn sunset_combat_packet(
    game: &mut crate::game::Game<'_>,
    packet: &horizon_protocol::packets::play::Clientbound,
) -> std::ops::ControlFlow<Result<()>> {
    if !game.native_window() { return std::ops::ControlFlow::Continue(()); }
    if crate::game::sunset_packet_delay::incoming(game, packet) {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    crate::game::sunset_loot_guard::packet(game, packet);
    crate::game::sunset_crystal_aura::packet(game, packet);
    crate::game::sunset_combat_inventory::packet(game, packet);
    crate::game::sunset_city_trap::packet(game, packet);
    crate::game::sunset_crystal_optimizer::packet(game, packet);
    match packet {
        horizon_protocol::packets::play::Clientbound::Login(_) | horizon_protocol::packets::play::Clientbound::Respawn(_) | horizon_protocol::packets::play::Clientbound::PlayerPosition(_) => crate::game::sunset_combat_wire::reset_look(),
        _ => {},
    }
    match packet {
        horizon_protocol::packets::play::Clientbound::EntityEvent(p) if p.event == 35 => crate::game::sunset_crystal_bot::note_totem(game, p.id),
        horizon_protocol::packets::play::Clientbound::PlayerPosition(_) => crate::game::sunset_crystal_bot::correction(),
        _ => {},
    }
    if let horizon_protocol::packets::play::Clientbound::SetEntityMotion(p) = packet {
        crate::game::sunset_melee_assist::note_velocity(game, p.id, p.velocity);
        crate::game::sunset_sword_bot::note_velocity(game, p.id, p.velocity);
    }
    std::ops::ControlFlow::Continue(())
}
