// Mount HUD: the mount's hearts above the food row.
fn sunset_mount_hearts(
    w: i32,
    h: i32,
    hearts: i32,
    health: f32,
    operation: impl FnOnce(i32, i32, i32, f32) -> Vec<horizon_client::hud_layout::GuiDraw>,
) -> Vec<horizon_client::hud_layout::GuiDraw> {
    let draws = operation(w, h, hearts, health);
    if !::sunset_core::lifecycle::RUNTIME.running() { return draws; }

    crate::game::sunset_hunger::raise(draws)
}
