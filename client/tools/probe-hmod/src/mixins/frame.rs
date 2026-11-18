// After the mouse turn (and Aim Assist), before the frame is drawn; the wire probe is installed on the first frame.
fn sunset_probe_frame(game: &crate::game::Game<'_>) {
    crate::game::sunset_probe_wire::install();
    crate::game::sunset_probe::frame(game);
}
