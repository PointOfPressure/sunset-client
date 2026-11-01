// Velocity: an explosion's push on the player, noted before the game adds it; Wind Jump jumps first.
fn sunset_explode(game: &mut crate::game::Game<'_>, p: &play::Explode) {
    crate::game::sunset_duel::blast(game, p.knockback);
    crate::game::sunset::explosion_push(game, p.knockback);
}
