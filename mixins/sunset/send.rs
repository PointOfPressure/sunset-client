// Outgoing packets: Friend Guard drops hits on friends, Blink holds moves, Anti Hunger and No Fall change them, Fake Lag
// holds what goes out for a while; what goes on the wire is noted (sunset_combat_wire::delivered, the same for a packet
// Fake Lag lets go later).
fn sunset_send(
    game: &crate::game::Game<'_>,
    packet: &horizon_protocol::packets::play::Serverbound,
) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
        if horizon_protocol::connection::sunset_lifecycle::RUNTIME.stopping() {
            crate::game::sunset_lag::drain(game);
        }
        crate::game::sunset_combat_wire::observe(game, packet);
        return std::ops::ControlFlow::Continue(());
    }

    use crate::game::sunset::Out;
    // Friend Guard: no hit on a friend, whoever sends it.
    if game.native_window()
        && game.native_on(crate::game::sunset::ID)
        && !crate::game::sunset_melee::allowed_out(game, packet)
    {
        return std::ops::ControlFlow::Break(());
    }
    let replay = crate::game::sunset_gui_move::replaying();
    let changed = match if replay { Out::Pass } else { crate::game::sunset::outgoing(game, packet) } {
        Out::Pass => None,
        Out::Drop => return std::ops::ControlFlow::Break(()),
        Out::Replace(changed) => Some(changed),
    };
    // Plugins' packet handlers (the plugin API), after Sunset's own changes.
    let changed = match if replay { Out::Pass } else { crate::game::sunset_api::packet_out(game, changed.as_ref().unwrap_or(packet)) } {
        Out::Pass => changed,
        Out::Drop => return std::ops::ControlFlow::Break(()),
        Out::Replace(replaced) => Some(replaced),
    };
    let last = changed.as_ref().unwrap_or(packet);
    if !replay && crate::game::sunset_gui_move::outgoing(game, last) {
        return std::ops::ControlFlow::Break(());
    }
    if let Some(release) = crate::game::sunset_hands::before_outgoing(game, last)
        && let Some(remote) = &game.remote
    {
        // Owned cleanup precedes the accepted attack, including native clicks and deferred Criticals. It bypasses
        // No Slow's intentional item-release suppression; it must not suppress AutoBlock's own release.
        crate::game::sunset_combat_wire::delivered(game, &release);
        remote.session.send(release);
    }
    // Fake Lag: held (or sent by it, behind what it held) means it is no longer this hook's to send.
    if crate::game::sunset_lag::held(game, last) {
        return std::ops::ControlFlow::Break(());
    }
    crate::game::sunset_combat_wire::delivered(game, last);
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
