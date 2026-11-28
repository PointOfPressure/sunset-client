// Portals: after the portal's effect, the screen put aside comes back.
fn sunset_portal_after(value: bool, game: &mut crate::game::Game<'_>) -> bool {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    crate::game::sunset_parity::portal_after(game);
    value
}
