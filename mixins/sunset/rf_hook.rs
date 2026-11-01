// RF Click: R and F become the mouse buttons before the game sees the key.
fn sunset_rf_click(event: &mut winit::event::WindowEvent) {
    crate::game::sunset_hands::rf_event(event);
}
