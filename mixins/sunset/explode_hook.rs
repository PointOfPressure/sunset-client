// Velocity: an explosion's push on the player, noted before the game adds it.
fn sunset_explode(game: &mut crate::game::Game<'_>, p: &play::Explode) {
    crate::game::sunset::explosion_push(game, p.knockback);
}
