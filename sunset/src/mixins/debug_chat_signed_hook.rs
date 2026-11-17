// Debug Report's chat log (and plugins' on_chat): a player's message, which comes into the chat by its own path.
fn sunset_debug_chat_signed(content: &crate::text::Text) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    ::sunset_core::chat_log::added(content, crate::chat::ChatSource::Player);
}
