// Velocity: an explosion's push on the player, noted before the game adds it; Wind Jump times its jump by the blast.
fn sunset_explode(game: &mut crate::game::Game<'_>, p: &play::Explode) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_duel::blast(game, p.knockback);
    crate::game::sunset::explosion_push(game, p.knockback);
}
