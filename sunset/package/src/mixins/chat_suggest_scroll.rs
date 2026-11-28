// The chat's suggestions for . commands (chat_suggest.rs): the wheel over its list scrolls the list, not the chat.
// HUD's layout editor: the wheel over an element's frame sets its size (runtime_gui_input.rs chat_layout_wheel).
fn sunset_chat_suggest_scroll(game: &crate::game::Game<'_>, amount: f32) -> std::ops::ControlFlow<()> {
    if ::sunset_core::lifecycle::RUNTIME.running()
        && (crate::game::sunset_chat_suggest::scroll(game, amount) || crate::game::sunset::chat_layout_wheel(game, amount))
    {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
