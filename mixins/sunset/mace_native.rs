//! Native geometry adapter draft; integration/build is owned by the parent.
//! Uses Horizon geometry/collision, never a synthetic box-distance proxy.
use super::Game;
use horizon_client::collisions::SceneCollisions;
use horizon_core::geometry::Vec3;

/// Whether waiting one further movement step preserves a real hit.
/// `yaw/pitch` must be the accepted server look, not an unsent camera wish.
/// `target_margin` must come from the native target pick radius (normally 0
/// for player melee); caller must not pass the spear's inflated attack margin.
/// None means unavailable/invalid native facts and must be treated as false.
pub(super) fn continuation(
    game: &mut Game<'_>,
    target: u32,
    yaw: f32,
    pitch: f32,
    target_margin: f64,
) -> Option<bool> {
    if !yaw.is_finite()
        || !pitch.is_finite()
        || !target_margin.is_finite()
        || !(0.0..=1.0).contains(&target_margin)
    {
        return None;
    }
    let reach = game
        .player
        .as_mut()?
        .attribute("minecraft:entity_interaction_range")
        .ok()?
        - 0.35;
    if !reach.is_finite() || !(0.0..=64.0).contains(&reach) || reach == 0.0 {
        return None;
    }
    let p = game.player.as_ref()?;
    let velocity = p.velocity;
    let current = game.eye_position()?;
    let from = Vec3::new(current[0], current[1], current[2]).add_vector(velocity);
    let look = super::view_vector(yaw, pitch);
    let next_bounds = p.bounds().move_by(velocity);
    let mob = game
        .mobs
        .iter()
        .find(|m| m.id == target && m.alive() && !m.removed)?;
    let drift = mob.position.subtract(mob.previous);
    let next_target =
        mob.bounds()
            .move_by(drift)
            .inflate(Vec3::new(target_margin, target_margin, target_margin));
    let lo = next_bounds.min_position();
    let hi = next_bounds.max_position();
    let tlo = next_target.min_position();
    let thi = next_target.max_position();
    if [
        from.x, from.y, from.z, velocity.x, velocity.y, velocity.z, look[0], look[1], look[2],
        lo.x, lo.y, lo.z, hi.x, hi.y, hi.z, tlo.x, tlo.y, tlo.z, thi.x, thi.y, thi.z,
    ]
    .iter()
    .any(|n| !n.is_finite())
    {
        return None;
    }
    let clear_next_body = SceneCollisions {
        world: &game.world,
        binding: game.collision,
        subject: None,
    }
    .unobstructed(next_bounds)
    .ok()?;
    if !clear_next_body {
        return Some(false);
    }
    if next_target.contains(from) {
        return Some(true);
    }
    let to = from.add_vector(Vec3::new(look[0], look[1], look[2]).scale(reach));
    let Some(contact) = next_target.clip(from, to) else {
        return Some(false);
    };
    let collider = super::spear::sunset_combat_collision::clip(game, from, contact);
    Some(
        collider.is_none_or(|point| {
            from.distance_squared(point) >= from.distance_squared(contact) - 0.001
        }),
    )
}
