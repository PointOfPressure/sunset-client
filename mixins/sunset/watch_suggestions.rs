fn sunset_watch_suggestions(
    game: &mut crate::game::Game<'_>,
    data: &[u8],
) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_watch_runtime::suggestions(game, data) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
