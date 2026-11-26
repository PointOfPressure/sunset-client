// The wheel scrolls a click GUI panel, and stays off the hotbar while it is open; held Zoom and a flying Freecam
// take it too (zoom level, camera speed).
fn sunset_wheel(game: &crate::game::Game<'_>, accumulated: [i32; 2], claimed: &mut bool) {
    let steps = accumulated[1] as f32;
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    if crate::game::sunset::gui_open() {
        crate::game::sunset::gui_wheel(game, steps);
        *claimed = true;
    } else if crate::game::sunset_render::camera_wheel(game, steps) {
        *claimed = true;
    }
}
