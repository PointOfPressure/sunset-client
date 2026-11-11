// Experimental legacy movement controllers. Independently implemented from behavioural comparisons; historical
// coefficients describe a client strategy, not a promise about any server's movement checks. Distances: blocks/tick.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    Initializing,
    Launch,
    Transition,
    Airborne,
    Recovery,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub tick: u64,
    pub position: [f64; 2],
    pub grounded: bool,
    pub blocked: bool,
    pub direction: Option<[f64; 2]>,
    pub base: f64,
    pub jump_scale: f64,
    pub speed_effect: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Move {
    pub across: [f64; 2],
    pub jump_scale: Option<f64>,
}

// Historical strategy parameters (not wire/protocol constants).
const NCP_LAUNCH: [f64; 2] = [2.14, 2.08];
const NCP_TRANSITION: [f64; 2] = [0.78, 0.597];
const AIR_DRAG: f64 = 0.91;
const AIR_ACCELERATION: f64 = 0.026;
// Defensive discontinuity limit for this experimental controller, not an anticheat threshold.
const MAX_SAMPLE_DISTANCE: f64 = 4.0;

#[derive(Debug, Default)]
pub(super) struct Ncp {
    phase: Phase,
    speed: f64,
    previous: Option<(u64, [f64; 2])>,
}

impl Ncp {
    pub(super) fn reset(&mut self) { *self = Self::default(); }

    pub(super) fn recover(&mut self) {
        self.reset();
        self.phase = Phase::Recovery;
    }

    pub(super) fn step(&mut self, f: Frame) -> Option<Move> {
        let Some(dir) = f.direction.filter(|d| d.iter().all(|n| n.is_finite()) && d[0].hypot(d[1]) > 1e-9) else {
            self.reset();
            return None;
        };
        if f.blocked || !f.position.iter().all(|n| n.is_finite()) || !f.base.is_finite() || f.base <= 0.0
            || !f.jump_scale.is_finite() || f.jump_scale <= 0.0
        {
            self.recover();
            return None;
        }
        let previous = self.previous.replace((f.tick, f.position));
        let distance = previous.map_or(0.0, |(_, at)| (f.position[0] - at[0]).hypot(f.position[1] - at[1]));
        if previous.is_some_and(|(tick, _)| tick.checked_add(1) != Some(f.tick)) || distance > MAX_SAMPLE_DISTANCE {
            self.recover();
            return None;
        }
        let effect = usize::from(f.speed_effect);
        let jump = if f.grounded {
            if self.phase == Phase::Launch {
                self.recover(); // The previous launch did not leave the ground (e.g. a blocked jump).
                return None;
            }
            self.speed = f.base * NCP_LAUNCH[effect];
            self.phase = Phase::Launch;
            Some(f.jump_scale)
        } else {
            match self.phase {
                Phase::Launch => {
                    self.speed = distance - NCP_TRANSITION[effect] * (distance - f.base);
                    self.phase = Phase::Transition;
                }
                Phase::Transition | Phase::Airborne => {
                    self.speed = self.speed * AIR_DRAG + AIR_ACCELERATION;
                    self.phase = Phase::Airborne;
                }
                Phase::Initializing | Phase::Recovery => {
                    self.recover();
                    return None;
                }
            }
            None
        };
        if !self.speed.is_finite() || self.speed < 0.0 { self.recover(); return None; }
        let norm = dir[0].hypot(dir[1]);
        Some(Move { across: [dir[0] / norm * self.speed, dir[1] / norm * self.speed], jump_scale: jump })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(tick: u64, x: f64, ground: bool) -> Frame {
        Frame { tick, position: [x, 0.0], grounded: ground, blocked: false, direction: Some([1.0, 0.0]),
            base: 0.2873, jump_scale: 1.0, speed_effect: false }
    }

    #[test]
    fn ncp_launch_transition_uses_actual_displacement_then_air_recurrence() {
        let mut ncp = Ncp::default();
        let launch = ncp.step(frame(0, 10.0, true)).unwrap();
        assert!((launch.across[0] - 0.2873 * 2.14).abs() < 1e-9);
        assert_eq!(launch.jump_scale, Some(1.0));
        assert_eq!(ncp.phase, Phase::Launch);
        // Collision-free actual movement can differ from the requested launch speed.
        let transition = ncp.step(frame(1, 10.45, false)).unwrap();
        let expected = 0.45 - 0.78 * (0.45 - 0.2873);
        assert!((transition.across[0] - expected).abs() < 1e-9);
        assert_eq!(transition.jump_scale, None);
        assert_eq!(ncp.phase, Phase::Transition);
        let air = ncp.step(frame(2, 10.8, false)).unwrap();
        assert!((air.across[0] - (expected * 0.91 + 0.026)).abs() < 1e-9);
        assert_eq!(ncp.phase, Phase::Airborne);
    }

    #[test]
    fn ncp_effect_branch_and_current_direction_are_separate_from_old_momentum() {
        let mut ncp = Ncp::default();
        let mut f = frame(0, 0.0, true);
        f.base *= 1.2; // Actual Speed I attribute, not zero-based amplifier math.
        f.jump_scale = 0.4 / 0.42; // Native block factor and Jump Boost are applied by the movement hook.
        f.speed_effect = true;
        let launch = ncp.step(f).unwrap();
        assert!((launch.across[0] - f.base * 2.08).abs() < 1e-9);
        assert_eq!(launch.jump_scale, Some(0.4 / 0.42));
        f.tick = 1;
        f.position = [0.6, 0.0];
        f.grounded = false;
        f.direction = Some([0.0, -1.0]);
        let turned = ncp.step(f).unwrap();
        assert_eq!(turned.across[0], 0.0);
        assert!((turned.across[1] + (0.6 - 0.597 * (0.6 - f.base))).abs() < 1e-9);
    }

    #[test]
    fn ncp_yields_and_recovers_without_midair_relaunch_or_stale_samples() {
        let mut ncp = Ncp::default();
        assert_eq!(ncp.step(frame(0, 0.0, false)), None);
        assert!(ncp.step(frame(1, 0.0, true)).is_some());
        let mut f = frame(2, 0.3, false);
        f.blocked = true;
        assert_eq!(ncp.step(f), None);
        assert_eq!(ncp.phase, Phase::Recovery);
        assert_eq!(ncp.step(frame(3, 0.4, false)), None);
        assert!(ncp.step(frame(4, 0.4, true)).is_some());
        ncp.recover(); // Teleport/world/mode/ownership interruption.
        assert_eq!(ncp.step(frame(5, 1000.0, false)), None);
        assert!(ncp.step(frame(6, 1000.0, true)).is_some());
        f = frame(7, 1000.4, false);
        f.direction = None;
        assert_eq!(ncp.step(f), None);
        assert_eq!(ncp.speed, 0.0);
        ncp.reset();
        assert_eq!(ncp.previous, None);
    }

    #[test]
    fn ncp_rejects_invalid_and_discontinuous_samples() {
        let mut ncp = Ncp::default();
        ncp.step(frame(0, 0.0, true));
        assert_eq!(ncp.step(frame(1, f64::NAN, false)), None);
        ncp.step(frame(2, 0.0, true));
        assert_eq!(ncp.step(frame(3, 100.0, false)), None);
        assert_eq!(ncp.phase, Phase::Recovery);
        assert_eq!(ncp.step(frame(50, 100.1, false)), None);
    }
}
