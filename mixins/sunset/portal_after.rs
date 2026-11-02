// Portals: after the portal's effect, the screen put aside comes back.
fn sunset_portal_after(value: bool, game: &mut crate::game::Game<'_>) -> bool {
    crate::game::sunset_parity::portal_after(game);
    value
}
