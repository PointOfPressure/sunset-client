// Portals (Sunset's): before a nether portal's effect closes the open screen, the screen is put aside.
fn sunset_portal_before(game: &mut crate::game::Game<'_>) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_parity::portal_before(game);
}
