// After the frame's packets, before its ticks: back to the time and weather asked for.
fn sunset_sky_shown(game: &mut crate::game::Game<'_>) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_sky::shown(game);
}
