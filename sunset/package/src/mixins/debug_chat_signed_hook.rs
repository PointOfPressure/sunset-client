// Debug Report's chat log (and plugins' on_chat): a player's message, which comes into the chat by its own path.
// Streamer Mode: then its names are replaced.
fn sunset_debug_chat_signed(value: horizon_client::text::Text) -> horizon_client::text::Text {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return value;
    }
    ::sunset_core::chat_log::added(&value.strip_legacy().plain(), horizon_client::chat::ChatSource::Player as u8);
    crate::game::sunset_streamer::protect(&value).unwrap_or(value)
}
