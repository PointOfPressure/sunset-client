// Freecam and Free Look decide where the frame is drawn from.
fn sunset_camera(game: &mut crate::game::Game<'_>, partial: f32) -> std::ops::ControlFlow<Option<(glam::DVec3, f32, f32)>> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::game::sunset_render::camera(game, partial) {
        Some(view) => std::ops::ControlFlow::Break(view),
        None => std::ops::ControlFlow::Continue(()),
    }
}
