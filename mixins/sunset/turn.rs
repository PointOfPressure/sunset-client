// Mouse movement turns the free camera instead of the player.
fn sunset_turn(game: &mut crate::game::Game<'_>, xo: f64, yo: f64) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_render::turn(game, xo, yo) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
