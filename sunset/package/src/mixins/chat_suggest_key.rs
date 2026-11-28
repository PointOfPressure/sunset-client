// The chat's suggestions for . commands (chat_suggest.rs): Tab, and Up, Down and Escape while its list shows.
fn sunset_chat_suggest_key(game: &mut crate::game::Game<'_>, key: winit::keyboard::KeyCode) -> std::ops::ControlFlow<()> {
    if ::sunset_core::lifecycle::RUNTIME.running() && crate::game::sunset_chat_suggest::key(game, key) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
