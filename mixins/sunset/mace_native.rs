// Whether Mace may hold a smash for one more tick (Rust's PR #16): a tick from now, with your body and the target
// moved on as they move now, the look (yaw, pitch) the server has still meets the target within reach, nothing blocks
// the way, and the next step is clear. None: the facts are not there, which counts as no.
use super::Game;
use horizon_client::collisions::SceneCollisions;
use horizon_core::geometry::Vec3;

pub(super) fn continuation(game: &mut Game<'_>, target: u32, yaw: f32, pitch: f32) -> Option<bool> {
    if !yaw.is_finite() || !pitch.is_finite() {
        return None;
    }
    let reach = game.player.as_mut()?.attribute("minecraft:entity_interaction_range").ok()? - 0.35;
    if !reach.is_finite() || reach <= 0.0 || reach > 64.0 {
        return None;
    }
    let p = game.player.as_ref()?;
    let velocity = p.velocity;
    let current = game.eye_position()?;
    let from = Vec3::new(current[0], current[1], current[2]).add_vector(velocity);
    let look = super::view_vector(yaw, pitch);
    let next_bounds = p.bounds().move_by(velocity);
    let mob = game.mobs.iter().find(|m| m.id == target && m.alive() && !m.removed)?;
    let next_target = mob.bounds().move_by(mob.position.subtract(mob.previous));
    let (lo, hi) = (next_bounds.min_position(), next_bounds.max_position());
    let (tlo, thi) = (next_target.min_position(), next_target.max_position());
    if [from.x, from.y, from.z, look[0], look[1], look[2], lo.x, lo.y, lo.z, hi.x, hi.y, hi.z, tlo.x, tlo.y, tlo.z, thi.x, thi.y, thi.z]
        .iter()
        .any(|n| !n.is_finite())
    {
        return None;
    }
    let clear_step = SceneCollisions { world: &game.world, binding: game.collision, subject: None }.unobstructed(next_bounds).ok()?;
    if !clear_step {
        return Some(false);
    }
    if next_target.contains(from) {
        return Some(true);
    }
    let to = from.add_vector(Vec3::new(look[0], look[1], look[2]).scale(reach));
    let Some(contact) = next_target.clip(from, to) else {
        return Some(false);
    };
    let wall = super::spear::sunset_combat_collision::clip(game, from, contact);
    Some(wall.is_none_or(|point| from.distance_squared(point) >= from.distance_squared(contact) - 0.001))
}
