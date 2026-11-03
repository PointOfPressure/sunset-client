// Liquid Interact: when the crosshair meets no block, the water or lava it meets counts as one.
fn sunset_liquid_pick(game: &crate::game::Game<'_>) -> std::ops::ControlFlow<Option<Hit>> {
    match crate::game::sunset_parity::liquid_pick(game) {
        Some(hit) => std::ops::ControlFlow::Break(hit),
        None => std::ops::ControlFlow::Continue(()),
    }
}
