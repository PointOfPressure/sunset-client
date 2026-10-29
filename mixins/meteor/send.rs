// Outgoing packets: No Fall sends its own version of a move.
fn meteor_send(
    game: &crate::game::Game<'_>,
    packet: &horizon_protocol::packets::play::Serverbound,
) -> std::ops::ControlFlow<()> {
    if let Some(changed) = crate::game::meteor::outgoing(game, packet) {
        if let Some(r) = &game.remote {
            r.session.send(changed);
        }
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
