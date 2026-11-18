fn sunset_gui_move_incoming(
    game: &mut Game<'_>,
    packet: &horizon_protocol::packets::play::Clientbound,
) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_gui_move::incoming_before(game, packet);
}
