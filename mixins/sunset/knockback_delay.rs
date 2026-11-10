// Velocity's Knockback delay: what the network module holds by for the next pushes on you
// (horizon_client::net::sunset_packets::hold). Its own state and its own switch: Fake Lag's Knockback mode only hears of
// a push once the game takes it, after this has let it go.
use super::Game;
use super::sunset::Module;
use horizon_client::net::sunset_packets as net;

pub(super) fn tick(game: &Game<'_>, mods: &[Module]) {
    let live = game.remote.is_some()
        && game.screen == super::Screen::Playing
        && !super::sunset::gui_open()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    let kb = mods.iter().find(|m| m.name == "Velocity" && m.on).filter(|m| {
        m.b("Knockback delay") && live && (!m.b("Delay only near enemies") || super::sunset_lag::enemy(game, 6.0, false))
            && !(m.b("Delay not in water") && game.player.as_ref().is_some_and(|p| p.fluids.was_touching_water))
    });
    match (kb, game.remote.as_ref()) {
        (Some(m), Some(r)) => {
            let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
            let (a, b) = if ground {
                (m.n("Ground delay min (ms)"), m.n("Ground delay max (ms)"))
            } else {
                (m.n("Air delay min (ms)"), m.n("Air delay max (ms)"))
            };
            net::delay(r.entity_id, m.n("Delay chance (%)"), [a.min(b) as u64, a.max(b) as u64]);
        }
        _ => net::delay_off(),
    }
}

pub(super) fn shutdown() {
    net::delay_off();
}
