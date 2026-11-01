// Zoom: the field of view divided by the zoom the view is at.
fn sunset_zoom_fov(game: &crate::game::Game<'_>, fov: f32) -> std::ops::ControlFlow<f32> {
    match crate::game::sunset_render::zoom_now(game) {
        Some(factor) => std::ops::ControlFlow::Break(fov / factor),
        None => std::ops::ControlFlow::Continue(()),
    }
}
