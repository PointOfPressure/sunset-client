// Pure movement calculations shared by the game hooks. Values are blocks per tick.

pub(super) fn steer(
    current: [f64; 2],
    direction: Option<[f64; 2]>,
    target_speed: f64,
    acceleration: f64,
    deceleration: f64,
    strafe_strength: f64,
    limit: f64,
) -> [f64; 2] {
    if !current.iter().all(|v| v.is_finite()) || !target_speed.is_finite() || !limit.is_finite() {
        return [0.0, 0.0];
    }
    let limit = limit.clamp(0.0, 2.0);
    let current_len = current[0].hypot(current[1]);
    let current = if current_len > limit && current_len > 0.0 {
        [
            current[0] * limit / current_len,
            current[1] * limit / current_len,
        ]
    } else {
        current
    };
    let Some(direction) = direction.filter(|d| d.iter().all(|v| v.is_finite())) else {
        let deceleration = if deceleration.is_finite() {
            deceleration.clamp(0.0, 2.0)
        } else {
            0.0
        };
        let keep = if current_len > 0.0 {
            (current_len - deceleration).max(0.0) / current_len
        } else {
            0.0
        };
        return [current[0] * keep, current[1] * keep];
    };
    let dir_len = direction[0].hypot(direction[1]);
    if dir_len <= 1e-12 {
        return current;
    }
    let target = target_speed.clamp(0.0, limit);
    let desired = [
        direction[0] / dir_len * target,
        direction[1] / dir_len * target,
    ];
    let turn = if strafe_strength.is_finite() {
        strafe_strength.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let aimed = [
        current[0] + (desired[0] - current[0]) * turn,
        current[1] + (desired[1] - current[1]) * turn,
    ];
    let delta = [aimed[0] - current[0], aimed[1] - current[1]];
    let delta_len = delta[0].hypot(delta[1]);
    let accel = if acceleration.is_finite() {
        acceleration.clamp(0.0, 2.0)
    } else {
        0.0
    };
    let fraction = if delta_len > accel && delta_len > 0.0 {
        accel / delta_len
    } else {
        1.0
    };
    let next = [
        current[0] + delta[0] * fraction,
        current[1] + delta[1] * fraction,
    ];
    let len = next[0].hypot(next[1]);
    if len > limit && len > 0.0 {
        [next[0] * limit / len, next[1] * limit / len]
    } else {
        next
    }
}

pub(super) fn velocity_factor(scale_percent: f64, blend_percent: f64) -> f64 {
    if !scale_percent.is_finite() || !blend_percent.is_finite() {
        return 1.0;
    }
    let scale = scale_percent.clamp(0.0, 100.0) / 100.0;
    let blend = blend_percent.clamp(0.0, 100.0) / 100.0;
    1.0 + (scale - 1.0) * blend
}

pub(super) fn item_use_factor(
    using_item: bool,
    no_slow_percent: Option<[f64; 2]>,
    on_ground: bool,
) -> f64 {
    if !using_item {
        return 1.0;
    }
    let Some(percent) = no_slow_percent.map(|v| if on_ground { v[0] } else { v[1] }) else {
        return 1.0;
    };
    if percent.is_finite() {
        percent.clamp(20.0, 100.0) / 100.0
    } else {
        1.0
    }
}

pub(super) fn turn_toward(from: f32, to: f32, max_degrees: f32) -> f32 {
    if !from.is_finite() || !to.is_finite() || !max_degrees.is_finite() {
        return from;
    }
    let delta = (to - from + 180.0).rem_euclid(360.0) - 180.0;
    from + delta.clamp(-max_degrees.max(0.0), max_degrees.max(0.0))
}

pub(super) fn face_cursor(target: [i32; 3], neighbor: [i32; 3]) -> [f32; 3] {
    let mut cursor = [0.5; 3];
    for axis in 0..3 {
        cursor[axis] = match target[axis] - neighbor[axis] {
            1 => 1.0,
            -1 => 0.0,
            _ => 0.5,
        };
    }
    cursor
}

pub(super) fn speed_can_write(
    flight: bool,
    fall_flying: bool,
    using_item: bool,
    no_slow: bool,
    knockback_ticks: u32,
    wall: bool,
    stop_at_wall: bool,
) -> bool {
    !flight
        && !fall_flying
        && (!using_item || no_slow)
        && knockback_ticks == 0
        && (!wall || !stop_at_wall)
}

// Walk the exact voxel cells crossed by a placement ray, excluding the endpoint face.
pub(super) fn clear_voxel_ray(
    start: [f64; 3],
    end: [f64; 3],
    mut blocked: impl FnMut([i32; 3]) -> bool,
) -> bool {
    if !start.iter().chain(end.iter()).all(|v| v.is_finite()) {
        return false;
    }
    let mut cell = start.map(|v| v.floor() as i32);
    let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let mut step = [0; 3];
    let mut next = [f64::INFINITY; 3];
    let mut stride = [f64::INFINITY; 3];
    for axis in 0..3 {
        if delta[axis] > 0.0 {
            step[axis] = 1;
            next[axis] = (f64::from(cell[axis] + 1) - start[axis]) / delta[axis];
            stride[axis] = 1.0 / delta[axis];
        } else if delta[axis] < 0.0 {
            step[axis] = -1;
            next[axis] = (f64::from(cell[axis]) - start[axis]) / delta[axis];
            stride[axis] = -1.0 / delta[axis];
        }
    }
    for _ in 0..32 {
        let axis = if next[0] <= next[1] && next[0] <= next[2] {
            0
        } else if next[1] <= next[2] {
            1
        } else {
            2
        };
        if next[axis] >= 1.0 - 1e-9 {
            return true;
        }
        cell[axis] += step[axis];
        if blocked(cell) {
            return false;
        }
        next[axis] += stride[axis];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steering_accelerates_and_caps_without_diagonal_gain() {
        let v = steer([0.0, 0.0], Some([1.0, 1.0]), 0.4, 0.1, 0.1, 1.0, 0.4);
        assert!((v[0].hypot(v[1]) - 0.1).abs() < 1e-10);
        let v = steer([10.0, 0.0], Some([1.0, 0.0]), 0.4, 0.1, 0.1, 1.0, 0.4);
        assert!(v[0].hypot(v[1]) <= 0.4 + 1e-10);
    }

    #[test]
    fn steering_decelerates_and_handles_invalid_state() {
        assert!((steer([0.3, 0.0], None, 0.4, 0.1, 0.1, 1.0, 0.5)[0] - 0.2).abs() < 1e-10);
        assert_eq!(
            steer(
                [f64::NAN, 1.0],
                Some([f64::INFINITY, 0.0]),
                0.4,
                0.1,
                0.1,
                1.0,
                0.5
            ),
            [0.0, 0.0]
        );
    }

    #[test]
    fn velocity_blend_keeps_a_bounded_share_of_server_motion() {
        assert_eq!(velocity_factor(0.0, 100.0), 0.0);
        assert_eq!(velocity_factor(0.0, 50.0), 0.5);
        assert_eq!(velocity_factor(100.0, 100.0), 1.0);
        assert_eq!(velocity_factor(f64::NAN, 100.0), 1.0);
    }

    #[test]
    fn rotation_uses_shortest_wrapped_turn() {
        assert!((turn_toward(179.0, -179.0, 1.0) - 180.0).abs() < 1e-5);
        assert_eq!(turn_toward(0.0, 90.0, 20.0), 20.0);
    }

    #[test]
    fn placement_cursor_is_on_the_selected_face() {
        assert_eq!(face_cursor([4, 10, 6], [4, 9, 6]), [0.5, 1.0, 0.5]);
        assert_eq!(face_cursor([4, 10, 6], [3, 10, 6]), [1.0, 0.5, 0.5]);
    }

    #[test]
    fn speed_yields_to_flight_item_use_knockback_and_walls() {
        assert!(speed_can_write(false, false, false, false, 0, false, true));
        assert!(!speed_can_write(true, false, false, false, 0, false, true));
        assert!(!speed_can_write(false, false, true, false, 0, false, true));
        assert!(!speed_can_write(false, false, false, false, 2, false, true));
        assert!(!speed_can_write(false, false, false, false, 0, true, true));
        assert!(speed_can_write(false, false, true, true, 0, false, true));
    }

    #[test]
    fn placement_ray_rejects_obstruction_without_blocking_the_hit_face() {
        let eye = [0.5, 2.5, 0.5];
        let hit = [3.0, 2.5, 0.5];
        assert!(!clear_voxel_ray(eye, hit, |p| p == [1, 2, 0]));
        assert!(clear_voxel_ray(eye, hit, |p| p == [3, 2, 0]));
    }

    #[test]
    fn item_use_factor_preserves_configured_ground_and_air_slowdown() {
        assert_eq!(item_use_factor(true, Some([50.0, 80.0]), true), 0.5);
        assert_eq!(item_use_factor(true, Some([50.0, 80.0]), false), 0.8);
        assert_eq!(item_use_factor(false, Some([50.0, 80.0]), true), 1.0);
        assert_eq!(item_use_factor(true, None, true), 1.0);
    }
}
