// Reach: the block reach, plus Reach's extra.
fn sunset_reach(game: &crate::game::Game<'_>) -> std::ops::ControlFlow<f64> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::game::sunset_player::reach_extra() {
        Some(extra) => std::ops::ControlFlow::Break(if game.creative { 5.0 } else { 4.5 } + extra),
        None => std::ops::ControlFlow::Continue(()),
    }
}
