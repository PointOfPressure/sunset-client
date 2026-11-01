// A right click: a combat macro takes it when you click with a crystal or an anchor; FastXP holds one back.
fn sunset_combat_use(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<()> {
    if crate::game::sunset_combat_macros::intercept_use(game) || crate::game::sunset_combat_inventory::intercept_use(game) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
