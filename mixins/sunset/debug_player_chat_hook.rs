// Signed and disguised player chat bypass Chat::add.
fn sunset_debug_player_chat(content: &crate::text::Text) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }
    crate::chat::sunset_chat_log::added(content, crate::chat::ChatSource::Player);
}
