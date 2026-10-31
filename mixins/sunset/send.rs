// Outgoing packets: Blink holds moves, Anti Hunger and No Fall change them.
fn sunset_send(
    game: &crate::game::Game<'_>,
    packet: &horizon_protocol::packets::play::Serverbound,
) -> std::ops::ControlFlow<()> {
    use crate::game::sunset::Out;
    if !game.native_window() {
        return std::ops::ControlFlow::Continue(());
    }
    if !game.native_on(crate::game::sunset::ID) {
        // Release original buffered frames before an unmodified native move.
        // Stale module flags must not alter packets after master disable.
        if crate::game::sunset_packet_delay::outgoing(game, packet) {
            return std::ops::ControlFlow::Break(());
        }
        crate::game::sunset_combat_wire::forwarded(game, packet);
        return std::ops::ControlFlow::Continue(());
    }
    if !crate::game::sunset_combat_wire::allowed(game, packet) {
        return std::ops::ControlFlow::Break(());
    }
    match crate::game::sunset::outgoing(game, packet) {
        Out::Pass => {
            if crate::game::sunset_packet_delay::outgoing(game, packet) {
                std::ops::ControlFlow::Break(())
            } else {
                crate::game::sunset_combat_wire::forwarded(game, packet);
                std::ops::ControlFlow::Continue(())
            }
        },
        Out::Drop => std::ops::ControlFlow::Break(()),
        Out::Replace(changed) => {
            if !crate::game::sunset_packet_delay::outgoing(game, &changed)
                && let Some(r) = &game.remote {
                crate::game::sunset_combat_wire::forwarded(game, &changed);
                r.session.send(changed);
            }
            std::ops::ControlFlow::Break(())
        }
    }
}
