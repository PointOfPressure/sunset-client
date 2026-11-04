// Criticals: your own hit on a mob waits for the hop or the fall that makes it critical. While a combat macro runs,
// clicks do nothing, nor those No Interact stops (its lists, Hit fix, Sword on shield); Aim Assist's Silent aim and Hit
// flick take a miss or a hit next, then No miss delay drops a swing at nothing; Packet Mine takes a click on a block;
// Friend Guard and Auto Weapon's Reach poke see the click first.
fn sunset_crit_click(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<bool> {
    if crate::game::sunset_combat_macros::active()
        || crate::game::sunset_parity::hit_blocked(game)
        || crate::game::sunset_click_fix::hit_blocked(game)
    {
        return std::ops::ControlFlow::Break(false);
    }
    if let Some(instant) = crate::game::sunset_click_fix::aim_click(game) {
        return std::ops::ControlFlow::Break(instant);
    }
    if crate::game::sunset_click_fix::miss_blocked(game) {
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
