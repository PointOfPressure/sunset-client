// Before the dark sheet behind an in-game screen. ClickGUI's Inventory blur: your inventory blurs the world behind
// it as the pause menu does, at the game's Menu blur strength (the request a mod's apply_screen_blur makes).
// No Render: the dark sheet behind an inventory, a book, a trade and the like is left out.
fn sunset_menu_dark(game: &Game<'_>) -> ::core::ops::ControlFlow<()> {
    use std::sync::atomic::Ordering::Relaxed;
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return ::core::ops::ControlFlow::Continue(());
    }
    if ::sunset_core::overlay::INVENTORY_BLUR.load(Relaxed)
        && (game.screen == Screen::Inventory || game.screen == Screen::Creative)
    {
        game.screen_blur_requested.set(true);
    }
    if ::sunset_core::overlay::NO_MENU.load(Relaxed) {
        return ::core::ops::ControlFlow::Break(());
    }
    ::core::ops::ControlFlow::Continue(())
}
