// Debug Report's chat log: each line as it comes into the chat.
fn sunset_debug_chat(content: &crate::text::Text, source: crate::chat::ChatSource) {
    // Names the crate root's re-export the game's copy of Chat::add reaches through its module's imports.
    #[allow(unused_imports)]
    use crate::command as _;
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    ::sunset_core::chat_log::added(&content.strip_legacy().plain(), source as u8);
}
