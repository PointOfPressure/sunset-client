// Zoom, through the zoom hook point.
fn sunset_zoom(game: &crate::game::Game<'_>, zoom: &mut Option<crate::game::native_hooks::NativeZoom>) {
    crate::game::sunset::zoom(game, zoom);
}
