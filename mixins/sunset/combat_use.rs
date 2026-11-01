// A right click: a combat macro takes it when you click with a crystal or an anchor; Fast Use's Throwables hold one
// back, and with Burst add more of the same use before it (each re-enters here and goes through).
fn sunset_combat_use(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<()> {
    use crate::game::sunset_combat_inventory as inventory;
    if inventory::bursting() {
        return std::ops::ControlFlow::Continue(());
    }
    if crate::game::sunset_combat_macros::intercept_use(game) || inventory::intercept_use(game) {
        return std::ops::ControlFlow::Break(());
    }
    while inventory::burst_more(game) {
        game.remote_use();
    }
    std::ops::ControlFlow::Continue(())
}
