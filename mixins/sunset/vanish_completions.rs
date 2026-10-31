// Anti Vanish's completion requests are answered to it, not to the chat box.
fn sunset_vanish_completions(game: &mut crate::game::Game<'_>, data: &[u8]) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_vanish::completions(game, data) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
