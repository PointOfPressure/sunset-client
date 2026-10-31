fn sunset_optimizer_skip(culled: bool, game: &crate::game::Game<'_>, feet: &glam::DVec3) -> bool {
    culled || crate::game::sunset_crystal_optimizer::hidden_position(game, [feet.x, feet.y, feet.z])
}
