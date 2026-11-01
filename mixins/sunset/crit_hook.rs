// Criticals: your own hit on a mob waits for the hop or the fall that makes it critical. While a combat macro runs,
// clicks do nothing.
fn sunset_crit_click(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<bool> {
    if crate::game::sunset_combat_macros::active() {
        return std::ops::ControlFlow::Break(false);
    }
    match crate::game::sunset::crit_click(game) {
        Some(instant) => std::ops::ControlFlow::Break(instant),
        None => std::ops::ControlFlow::Continue(()),
    }
}
