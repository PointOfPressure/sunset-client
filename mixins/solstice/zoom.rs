// Zoom, through the zoom hook point.
fn solstice_zoom(game: &crate::game::Game<'_>, zoom: &mut Option<crate::game::native_hooks::NativeZoom>) {
    crate::game::solstice::zoom(game, zoom);
}
