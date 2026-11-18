// Sunset's crosshair takes the vanilla mark's place (the attack indicator stays).
fn sunset_crosshair_mark(
    w: i32,
    h: i32,
    attack: Option<f32>,
    full: bool,
    operation: impl FnOnce(i32, i32, Option<f32>, bool) -> Vec<hud_layout::GuiDraw>,
) -> Vec<hud_layout::GuiDraw> {
    let mut draws = operation(w, h, attack, full);
    if ::sunset_core::lifecycle::RUNTIME.running() {
        crate::game::sunset_crosshair_flags::hide_mark(&mut draws);
    }
    draws
}
