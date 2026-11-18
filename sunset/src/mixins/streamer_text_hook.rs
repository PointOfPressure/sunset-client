// Streamer Mode: a line coming into the chat, its names replaced (after Debug Report's log and plugins' on_chat have
// it as it came).
fn sunset_streamer_chat(value: Text) -> Text {
    let mut value = value;
    if ::sunset_core::lifecycle::RUNTIME.running()
        && let Some(protect) = ::sunset_core::bridge::STREAMER_TEXT.get()
    {
        protect(&mut value as *mut Text as ::sunset_core::bridge::Ptr);
    }
    value
}
