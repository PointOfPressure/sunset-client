//! Original native rotation and critical-window policies.
pub fn rotation_step(
    base: [f32; 2],
    desired: [f32; 2],
    quantum: f64,
    speed: f64,
    random: f64,
) -> Option<[f32; 2]> {
    if !base.iter().chain(desired.iter()).all(|n| n.is_finite())
        || !quantum.is_finite()
        || quantum <= 0.
        || !speed.is_finite()
        || speed < 0.
        || !random.is_finite()
    {
        return None;
    }
    let yaw = f64::from((desired[0] - base[0] + 180.).rem_euclid(360.) - 180.);
    let pitch = f64::from(desired[1] - base[1]);
    let distance = yaw.hypot(pitch);
    let mut scale = 1.;
    if distance >= 1e-4 && speed > 0. {
        let mut stride = (0.313 * distance + 1.56 * speed).min(distance);
        if stride < distance {
            stride *= 0.85 + 0.15 * random.clamp(0., 1.);
        }
        scale = stride / distance;
    }
    let axis = yaw.abs().max(pitch.abs()) * scale;
    if axis > 25. {
        scale *= 25. / axis;
    }
    let cap = (25. / quantum).floor().max(1.);
    let counts = |delta: f64| ((delta * scale / quantum) + 0.5).floor().clamp(-cap, cap);
    Some([
        (f64::from(base[0]) + counts(yaw) * quantum) as f32,
        (f64::from(base[1]) + counts(pitch) * quantum).clamp(-90., 90.) as f32,
    ])
}
#[derive(Default, Debug)]
pub struct CriticalWindow {
    waits: u8,
    tick: Option<u64>,
    cached: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wait {
    pub hold: bool,
    pub drop_sprint: bool,
}
impl CriticalWindow {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn plan(
        &mut self,
        tick: u64,
        airborne: bool,
        sprinting: bool,
        falling: bool,
        ground_jump: bool,
    ) -> Wait {
        if airborne && sprinting {
            return Wait {
                hold: true,
                drop_sprint: true,
            };
        }
        if airborne && falling {
            return Wait {
                hold: false,
                drop_sprint: false,
            };
        }
        if !airborne && !ground_jump {
            return Wait {
                hold: false,
                drop_sprint: false,
            };
        }
        if self.tick != Some(tick) {
            self.cached = self.waits < 8;
            self.waits = self.waits.saturating_add(1);
            self.tick = Some(tick);
        }
        Wait {
            hold: self.cached,
            drop_sprint: airborne && self.cached,
        }
    }
}
pub fn stride(observed: [f64; 3]) -> [f64; 3] {
    if observed.iter().all(|v| v.is_finite())
        && observed[0] * observed[0] + observed[2] * observed[2] <= 1.
    {
        [observed[0], 0., observed[2]]
    } else {
        [0.; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_turn_is_bounded_per_axis_and_retains_direction_ratio() {
        let step = rotation_step([0., 0.], [100., 50.], 0.125, 0., 0.5).unwrap();
        assert_eq!(step, [25., 12.5]);
    }
    #[test]
    fn native_quantum_uses_java_tie_direction_and_wraps_shortest_yaw() {
        assert_eq!(
            rotation_step([0., 0.], [-0.5, 0.5], 1., 0., 0.5),
            Some([0., 1.])
        );
        assert_eq!(
            rotation_step([179., 0.], [-179., 0.], 0.125, 0., 0.5),
            Some([181., 0.])
        );
    }
    #[test]
    fn positive_speed_eases_small_turn_and_all_axes_stay_bounded() {
        let a = rotation_step([0., 0.], [20., 10.], 0.125, 1., 0.).unwrap();
        let b = rotation_step([0., 0.], [20., 10.], 0.125, 1., 1.).unwrap();
        assert!(a[0] > 0. && a[0] < b[0] && b[0] < 20.);
        assert!(rotation_step([0., 0.], [180., 90.], 0.125, 5., 1.).unwrap()[0].abs() <= 25.);
    }
    #[test]
    fn invalid_rotation_facts_do_not_create_a_turn() {
        assert!(rotation_step([0., 0.], [f32::NAN, 0.], 0.1, 0., 0.).is_none());
        assert!(rotation_step([0., 0.], [1., 0.], 0., 0., 0.).is_none());
    }
    #[test]
    fn wait_allows_normal_ground_hit_and_falling_hit_without_eight_tick_delay() {
        let mut w = CriticalWindow::default();
        assert!(!w.plan(1, false, false, false, false).hold);
        assert!(!w.plan(2, true, false, true, false).hold);
    }
    #[test]
    fn rising_or_ground_jump_wait_is_eight_actual_calls_not_target_age() {
        let mut w = CriticalWindow::default();
        for tick in 1000..1008 {
            assert!(w.plan(tick, true, false, false, false).hold);
        }
        assert!(!w.plan(1008, true, false, false, false).hold);
        w.reset();
        for tick in 2000..2008 {
            assert!(w.plan(tick, false, false, false, true).hold);
        }
        assert!(!w.plan(2008, false, false, false, true).hold);
    }
    #[test]
    fn duplicate_same_tick_does_not_consume_window_and_sprint_wait_does_not_count() {
        let mut w = CriticalWindow::default();
        for tick in 1..20 {
            assert!(w.plan(tick, true, true, true, false).drop_sprint);
        }
        for _ in 0..20 {
            assert!(w.plan(20, true, false, false, false).hold);
        }
        assert!(!w.plan(21, true, false, true, false).hold);
    }
    #[test]
    fn stride_prediction_discards_vertical_and_teleport_steps() {
        assert_eq!(stride([0.6, 4., 0.8]), [0.6, 0., 0.8]);
        assert_eq!(stride([1.01, 0., 0.]), [0.; 3]);
        assert_eq!(stride([f64::NAN, 0., 0.]), [0.; 3]);
    }
}
