// Shulker Peek: the grid a middle click pinned, in place of the hovered slot's tooltip.
fn sunset_peek_pinned(game: &crate::game::Game<'_>, batch: &mut HudBatch) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_render::pinned(game, batch) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
