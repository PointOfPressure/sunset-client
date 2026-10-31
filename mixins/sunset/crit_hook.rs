// Criticals: your own hit on a mob waits for the hop or the fall that makes it critical.
fn sunset_crit_click(game: &mut crate::game::Game<'_>) -> std::ops::ControlFlow<bool> {
    match crate::game::sunset::crit_click(game) {
        Some(instant) => std::ops::ControlFlow::Break(instant),
        None => std::ops::ControlFlow::Continue(()),
    }
}
