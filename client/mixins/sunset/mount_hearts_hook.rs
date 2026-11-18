// Mount HUD: the mount's hearts above the food row.
fn sunset_mount_hearts(value: Vec<GuiDraw>) -> Vec<GuiDraw> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    crate::hud_layout::sunset_hunger::raise(value)
}
