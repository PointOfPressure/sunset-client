// Entities the server removed (a RemoveEntities packet's, or a single one): Antibot forgets each.
fn sunset_antibot_despawn(game: &crate::game::Game<'_>, ids: &[i32]) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    for &id in ids {
        crate::game::sunset_antibot::invalidate(game, id);
    }
}
