fn sunset_seedcracker_incoming(
    _game: &mut Game<'_>,
    packet: &horizon_protocol::packets::play::Clientbound,
) {
    if crate::game::sunset_seedcracker::incoming_before(packet) {
        crate::game::sunset_ore_sim::stop();
    }
}
