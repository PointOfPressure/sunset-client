// Ghost Hand's Hits: the mob under the crosshair is before the block you aim at.
fn sunset_wall_hit(game: &crate::game::Game<'_>, distance: f64) -> std::ops::ControlFlow<bool> {
    if crate::game::sunset_player::hit_through_walls(game, distance) {
        return std::ops::ControlFlow::Break(true);
    }
    std::ops::ControlFlow::Continue(())
}
