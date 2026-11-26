// The chat's suggestions for . commands (chat_suggest.rs): a click on its list picks one.
fn sunset_chat_suggest_click(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<()> {
    if ::sunset_core::lifecycle::RUNTIME.running() && crate::game::sunset_chat_suggest::click(game) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
