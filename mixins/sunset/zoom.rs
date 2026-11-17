// Zoom: the field of view divided by the zoom the view is at.
fn sunset_zoom_fov(game: &crate::game::Game<'_>, fov: f32) -> std::ops::ControlFlow<f32> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    let custom = crate::game::sunset_cyanite_render::fov(game);
    let fov = custom.unwrap_or(fov);
    match crate::game::sunset_render::zoom_now(game) {
        Some(factor) => std::ops::ControlFlow::Break(fov / factor),
        None if custom.is_some() => std::ops::ControlFlow::Break(fov),
        None => std::ops::ControlFlow::Continue(()),
    }
}
