// Outgoing packets: Friend Guard drops hits on friends, Blink holds moves, Anti Hunger and No Fall change them, Fake Lag
// holds what goes out for a while; what goes on the wire is noted (the look and the sprint the server has, for Crystal
// Aura and Anchor Aura).
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
    let changed = match crate::game::sunset::outgoing(game, packet) {
        Out::Pass => None,
        Out::Drop => return std::ops::ControlFlow::Break(()),
        Out::Replace(changed) => Some(changed),
    };
    let last = changed.as_ref().unwrap_or(packet);
    if crate::game::sunset_lag::held(game, last) {
        return std::ops::ControlFlow::Break(());
    }
    crate::game::sunset_combat_wire::forwarded(game, last);
    crate::game::sunset_render::sent(game, last);
    match changed {
        None => std::ops::ControlFlow::Continue(()),
        Some(changed) => {
            if let Some(r) = &game.remote {
                r.session.send(changed);
            }
            std::ops::ControlFlow::Break(())
        }
    }
}
