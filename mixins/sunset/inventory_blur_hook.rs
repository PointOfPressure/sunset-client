// ClickGUI's Inventory blur (vape-rs's InventoryBlur): your inventory blurs the world behind it as the pause menu
// does, at the game's Menu blur strength.
fn sunset_inventory_blur(screen: crate::game::Screen) -> std::ops::ControlFlow<bool> {
    use horizon_client::screen_effects::sunset_overlay::INVENTORY_BLUR;
    let inventory = screen == crate::game::Screen::Inventory || screen == crate::game::Screen::Creative;
    if inventory && INVENTORY_BLUR.load(std::sync::atomic::Ordering::Relaxed) {
        return std::ops::ControlFlow::Break(true);
    }
    std::ops::ControlFlow::Continue(())
}
