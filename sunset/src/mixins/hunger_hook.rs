// HUD's Mount HUD and food overlays: the health and food rows laid out by Sunset when no other mod answered.
fn sunset_hunger_bars(
    game: &mut Game<'_>,
    draws: &mut Option<Vec<hud_layout::GuiDraw>>,
    inputs: &hud_layout::HealthInputs,
    batch: &HudBatch,
) {
    if !::sunset_core::lifecycle::RUNTIME.running() || draws.is_some() {
        return;
    }
    let (w, h) = (batch.width as i32, batch.height as i32);
    if let std::ops::ControlFlow::Break(rows) = crate::game::sunset_hunger::layout(&mut game.health_hud, inputs, w, h) {
        *draws = Some(rows);
    }
}
