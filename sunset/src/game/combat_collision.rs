// The game's collider ray (a module of game/spear.rs, where it is private): where a line from `from` to `to` first
// meets a block, for the melee modules and Kill Aura.
pub(crate) fn clip(
    game: &crate::game::Game<'_>,
    from: horizon_core::geometry::Vec3,
    to: horizon_core::geometry::Vec3,
) -> Option<horizon_core::geometry::Vec3> {
    game.clip_collider(from, to)
}
