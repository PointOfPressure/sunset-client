// After the mouse turn (and Aim Assist), before the frame is drawn.
fn sunset_probe_frame(game: &crate::game::Game<'_>) {
    crate::game::sunset_probe::frame(game);
}
