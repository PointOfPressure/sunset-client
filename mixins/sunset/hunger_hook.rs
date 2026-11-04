// HUD's Mount HUD and food overlays on the game's health and food layout.
fn sunset_hunger_bars(hud: &mut HealthHud, i: &HealthInputs, w: i32, h: i32) -> std::ops::ControlFlow<Vec<GuiDraw>> {
    crate::hud_layout::sunset_hunger::layout(hud, i, w, h)
}
