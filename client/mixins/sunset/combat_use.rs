// A right click: No Interact stops it on what it lists and by Anti action, or puts its block behind the mob clicked
// (Ghost place); a combat macro takes it when you click with a crystal or an
// anchor; Fast Use's Throwables hold one back, and with Burst add more of the same use before it (each re-enters here
// and goes through).
fn sunset_combat_use(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<()> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    use crate::game::sunset_combat_inventory as inventory;
    if inventory::bursting() {
        return std::ops::ControlFlow::Continue(());
    }
    if crate::game::sunset_parity::use_blocked(game)
        || crate::game::sunset_click_fix::use_blocked(game)
        || crate::game::sunset_combat_macros::intercept_use(game)
        || inventory::intercept_use(game)
    {
        return std::ops::ControlFlow::Break(());
    }
    if let Some(hand) = crate::game::sunset_click_fix::ghost_place(game) {
        game.use_delay = 4;
        game.remote_use_block(hand);
        return std::ops::ControlFlow::Break(());
    }
    while inventory::burst_more(game) {
        game.remote_use();
    }
    std::ops::ControlFlow::Continue(())
}
