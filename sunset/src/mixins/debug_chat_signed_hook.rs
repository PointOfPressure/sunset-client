// Debug Report's chat log (and plugins' on_chat): a player's message, which comes into the chat by its own path.
fn sunset_debug_chat_signed(value: horizon_client::text::Text) -> horizon_client::text::Text {
    if ::sunset_core::lifecycle::RUNTIME.running() {
        ::sunset_core::chat_log::added(&value.strip_legacy().plain(), horizon_client::chat::ChatSource::Player as u8);
    }
    value
}
