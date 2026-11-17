fn baritone_chat(game: &mut crate::game::Game<'_>, message: &str) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_baritone_full::chat(game, message) { std::ops::ControlFlow::Break(()) } else { std::ops::ControlFlow::Continue(()) }
}
