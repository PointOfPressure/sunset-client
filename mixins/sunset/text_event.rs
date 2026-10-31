// The window's key events as they come, with the text the keyboard layout makes: the GUI's text editor types it.
fn sunset_text_event(event: &winit::event::WindowEvent) {
    crate::game::sunset::text_event(event);
}
