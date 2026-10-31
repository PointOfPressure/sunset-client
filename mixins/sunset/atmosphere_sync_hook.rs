// HUD construction runs before native world submission, even with F1 active.
fn sunset_atmosphere_sync(game: &crate::game::Game<'_>) {
    crate::game::sunset_atmosphere::sync(game);
}
