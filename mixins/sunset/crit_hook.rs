// Criticals: your own hit on a mob waits for the hop or the fall that makes it critical. While a combat macro runs,
// clicks do nothing; Packet Mine takes a click on a block; Friend Guard, Shield Breaker, Mace and Spear Swap's Reach
// poke see the click first.
fn sunset_crit_click(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<bool> {
    if crate::game::sunset_combat_macros::active() {
        return std::ops::ControlFlow::Break(false);
    }
    if crate::game::sunset_packet_mine::click(game, true) {
        return std::ops::ControlFlow::Break(false);
    }
    if let Some(instant) = crate::game::sunset_melee::before_click(game) {
        return std::ops::ControlFlow::Break(instant);
    }
    match crate::game::sunset::crit_click(game) {
        Some(instant) => std::ops::ControlFlow::Break(instant),
        None => std::ops::ControlFlow::Continue(()),
    }
}
