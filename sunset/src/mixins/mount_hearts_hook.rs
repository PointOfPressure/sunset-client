// Mount HUD: the mount's hearts above the food row.
fn sunset_mount_hearts(value: Vec<GuiDraw>) -> Vec<GuiDraw> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    crate::game::sunset_hunger::raise(value)
}
