// Outgoing packets: Blink holds moves, Anti Hunger and No Fall change them.
fn solstice_send(
    game: &crate::game::Game<'_>,
    packet: &horizon_protocol::packets::play::Serverbound,
) -> std::ops::ControlFlow<()> {
    use crate::game::solstice::Out;
    match crate::game::solstice::outgoing(game, packet) {
        Out::Pass => std::ops::ControlFlow::Continue(()),
        Out::Drop => std::ops::ControlFlow::Break(()),
        Out::Replace(changed) => {
            if let Some(r) = &game.remote {
                r.session.send(changed);
            }
            std::ops::ControlFlow::Break(())
        }
    }
}
