// The window's events as they come, before any screen: the text the keyboard layout makes (the GUI's text editor types
// it), and presses, releases and focus for the binds (a Hold bind lets go even when a screen took the release).
fn sunset_window_event(event: &winit::event::WindowEvent) {
    crate::game::sunset::text_event(event);
    crate::game::sunset::bind_event(event);
    if matches!(event, winit::event::WindowEvent::Focused(false)) {
        crate::game::sunset_combat_macros::focus_lost();
    }
}
