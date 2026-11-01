// Same-module bridge to the pinned engine's ordinary COLLIDER ray.
pub(crate) fn clip(game: &crate::game::Game<'_>, from: horizon_core::geometry::Vec3, to: horizon_core::geometry::Vec3) -> Option<horizon_core::geometry::Vec3> {
    game.clip_collider(from, to)
}
