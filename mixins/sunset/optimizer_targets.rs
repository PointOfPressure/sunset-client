// Keep the native list and stable special-id mapping; suppress only the local
// ghost's hit box. Actual server removal and aura ACKs still use real entities.
fn sunset_optimizer_targets(mut targets:Vec<(u32,horizon_core::geometry::Aabb)>,
    game:&crate::game::Game<'_>)->Vec<(u32,horizon_core::geometry::Aabb)> {
    targets.retain(|(id,_)| id.checked_sub(CRYSTAL_IDS)
        .and_then(|index|game.crystals.get(index as usize))
        .is_none_or(|crystal| !crate::game::sunset_crystal_optimizer::hidden_position(
            game,[crystal.position.x,crystal.position.y,crystal.position.z])));
    targets
}
