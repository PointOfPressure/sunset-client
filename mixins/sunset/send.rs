// Outgoing packets: Friend Guard drops hits on friends, Blink holds moves, Anti Hunger and No Fall change them; what goes on the wire is noted (the look
// and the sprint the server has, for Crystal Aura and Anchor Aura).
fn sunset_send(
    game: &crate::game::Game<'_>,
    packet: &horizon_protocol::packets::play::Serverbound,
) -> std::ops::ControlFlow<()> {
    use crate::game::sunset::Out;
    // Friend Guard: no hit on a friend, whoever sends it.
    if game.native_window()
        && game.native_on(crate::game::sunset::ID)
        && !crate::game::sunset_melee::allowed_out(game, packet)
    {
        return std::ops::ControlFlow::Break(());
    }
    match crate::game::sunset::outgoing(game, packet) {
        Out::Pass => {
            crate::game::sunset_combat_wire::forwarded(game, packet);
            std::ops::ControlFlow::Continue(())
        }
        Out::Drop => std::ops::ControlFlow::Break(()),
        Out::Replace(changed) => {
            if let Some(r) = &game.remote {
                crate::game::sunset_combat_wire::forwarded(game, &changed);
                r.session.send(changed);
            }
            std::ops::ControlFlow::Break(())
        }
    }
}
