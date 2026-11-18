// Mount HUD: the XP bar while riding, over the jump bar while no jump charges.
fn sunset_mount_xp(
    number: Option<u32>,
    outline: Option<u32>,
    w: i32,
    h: i32,
    progress: f32,
    level: i32,
    bar: bool,
    width: &dyn Fn(&str) -> i32,
) -> std::ops::ControlFlow<Vec<GuiDraw>> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    crate::hud_layout::sunset_hunger::experience([number, outline], w, h, progress, level, bar, width)
}
