// Debug Report's chat log: each line as it comes into the chat.
fn sunset_debug_chat(content: &crate::text::Text, source: crate::chat::ChatSource) {
    crate::chat::sunset_chat_log::added(content, source);
}
