// Crystal Optimizer: an attack on an end crystal is done on your screen at once.
fn sunset_crystal(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<bool> {
    match crate::game::sunset_combat::crystal_attack(game) {
        Some(instant) => std::ops::ControlFlow::Break(instant),
        None => std::ops::ControlFlow::Continue(()),
    }
}
