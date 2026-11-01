// Criticals: your own hit on a mob waits for the hop or the fall that makes it critical.
fn sunset_crit_click(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<bool> {
    if crate::game::sunset_combat_macros::active() {
        return std::ops::ControlFlow::Break(false);
    }
    if let Some(instant) = crate::game::sunset_melee_assist::before_click(game) {
        return std::ops::ControlFlow::Break(instant);
    }
    match crate::game::sunset::crit_click(game) {
        Some(instant) => std::ops::ControlFlow::Break(instant),
        None => std::ops::ControlFlow::Continue(()),
    }
}
