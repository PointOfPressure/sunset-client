// Client Sort: Horizon's sort, in Client Sort's order.
fn sunset_sort(game: &mut crate::game::Game<'_>, slots: &[usize]) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_extras::sort(game, slots) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
