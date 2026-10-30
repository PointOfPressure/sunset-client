// Velocity: an explosion's push on the player, noted before the game adds it.
fn sunset_explode(p: &play::Explode) {
    crate::game::sunset::explosion_push(p.knockback);
}
