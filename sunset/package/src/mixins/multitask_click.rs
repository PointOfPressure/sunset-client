// Multitask: a left click while you use an item, which the game will drop, is counted for a hit.
fn sunset_multitask_click(game: &crate::game::Game<'_>, button: winit::event::MouseButton, pressed: bool) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    crate::game::sunset_parity::clicked(game, button == winit::event::MouseButton::Left && pressed);
}
