// HUD's Mount HUD and food overlays on the game's health and food layout.
fn sunset_hunger_bars(hud: &mut HealthHud, i: &HealthInputs, w: i32, h: i32) -> std::ops::ControlFlow<Vec<GuiDraw>> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::hud_layout::sunset_hunger::layout(hud, i, w, h)
}
