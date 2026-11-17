// Debug Report's chat log: each line as it comes into the chat.
fn sunset_debug_chat(content: &crate::text::Text, source: crate::chat::ChatSource) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    ::sunset_core::chat_log::added(&content.strip_legacy().plain(), source as u8);
}
