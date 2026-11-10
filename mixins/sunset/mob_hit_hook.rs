// Reach's Entity reach and Auto Spear's Spear reach: the crosshair picks mobs further out.
fn sunset_mob_hit(game: &crate::game::Game<'_>) -> std::ops::ControlFlow<Option<(usize, f64)>> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match crate::game::sunset_player::mob_hit(game) {
        Some(hit) => std::ops::ControlFlow::Break(hit),
        None => std::ops::ControlFlow::Continue(()),
    }
}
