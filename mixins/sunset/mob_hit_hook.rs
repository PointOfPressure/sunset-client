// Reach's Entity reach and Spear Swap's Spear reach: the crosshair picks mobs further out.
fn sunset_mob_hit(game: &crate::game::Game<'_>) -> std::ops::ControlFlow<Option<(usize, f64)>> {
    match crate::game::sunset_player::mob_hit(game) {
        Some(hit) => std::ops::ControlFlow::Break(hit),
        None => std::ops::ControlFlow::Continue(()),
    }
}
