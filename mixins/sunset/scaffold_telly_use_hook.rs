fn sunset_telly_use(game: &crate::game::Game<'_>) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_scaffold_telly::manual_use_blocked(game) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
