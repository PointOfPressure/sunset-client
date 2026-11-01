// Genuine device counts, before native sensitivity/cinematic smoothing. A
// bot's own turn_player calls never enter this input observer.
fn sunset_combat_mouse_motion(game:&crate::game::Game<'_>,event:&winit::event::DeviceEvent) {
    if game.pending_save.is_none() && game.captured && game.focused
        && game.screen==crate::game::Screen::Playing
        && let winit::event::DeviceEvent::MouseMotion{delta:(x,y)}=event {
        crate::game::sunset_crystal_bot::mouse_delta(game,[*x,*y]);
    }
}
