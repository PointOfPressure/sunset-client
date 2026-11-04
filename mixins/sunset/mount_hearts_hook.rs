// Mount HUD: the mount's hearts above the food row.
fn sunset_mount_hearts(value: Vec<GuiDraw>) -> Vec<GuiDraw> {
    crate::hud_layout::sunset_hunger::raise(value)
}
