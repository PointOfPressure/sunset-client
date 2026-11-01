fn sunset_city_continue(game: &mut crate::game::Game<'_>, down: bool) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_city_trap::suppress_idle_abort(game, down)
        || crate::game::sunset_crystal_bot::suppress_idle_abort(game, down)
    {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
