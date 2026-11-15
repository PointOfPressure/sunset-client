// The scoreboard sidebar's draws: measured for the chat layout editor, and moved to where the sidebar was dragged
// (scoreboard_pos.rs).
fn sunset_sidebar(mut value: Vec<GuiDraw>) -> Vec<GuiDraw> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
        return value;
    }
    crate::hud_overlays::sunset_scoreboard_pos::apply(&mut value);
    value
}
