// Debug Report's chat log (and plugins' on_chat): a player's message, which comes into the chat by its own path.
fn sunset_debug_chat_signed(content: &crate::text::Text) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::chat::sunset_chat_log::added(content, crate::chat::ChatSource::Player);
}
