// The scoreboard sidebar's draws: measured for the chat layout editor, and moved to where the sidebar was dragged
// (scoreboard_pos.rs).
fn sunset_sidebar(
    w: i32,
    h: i32,
    board: &horizon_client::scoreboard::Scoreboard,
    objective: &horizon_client::scoreboard::Objective,
    font: &horizon_client::font::Font,
    background: u32,
    header: u32,
    operation: impl FnOnce(i32, i32, &horizon_client::scoreboard::Scoreboard, &horizon_client::scoreboard::Objective, &horizon_client::font::Font, u32, u32) -> Vec<horizon_client::hud_layout::GuiDraw>,
) -> Vec<horizon_client::hud_layout::GuiDraw> {
    let mut draws = operation(w, h, board, objective, font, background, header);
    if ::sunset_core::lifecycle::RUNTIME.running() {
        // Streamer Mode: the names it hides, replaced.
        draws = crate::game::sunset_streamer::protect_draws(draws, font, false);
        crate::game::sunset_scoreboard_pos::apply(&mut draws);
    }
    draws
}
