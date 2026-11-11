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

// Watchdog differs deliberately: a launch impulse followed by native air physics, not NCP's forced air recurrence.
// Optional damage steering can only redirect the actual post-Velocity server impulse; it never manufactures speed.
#[derive(Debug, Default)]
pub(super) struct Watchdog {
    phase: Phase,
    previous: Option<(u64, [f64; 2])>,
    knock: Option<(u64, bool)>,
    boosted: bool,
    owns_momentum: bool,
}

const WATCHDOG_LAUNCH: f64 = 1.68;
const WATCHDOG_SPEED_ONE: f64 = 1.8;
const WATCHDOG_SPEED_HIGH: f64 = 1.6;
const WATCHDOG_EFFECT_STEP: f64 = 0.07;
const WATCHDOG_TURN: f64 = 25.0 * std::f64::consts::PI / 180.0;
const KNOCK_WINDOW: u64 = 6;

impl Watchdog {
    pub(super) fn reset(&mut self) -> bool {
        let owned = self.owns_momentum;
        *self = Self::default();
        owned
    }

    pub(super) fn suspend(&mut self) -> bool {
        let owned = self.owns_momentum;
        self.owns_momentum = false;
        self.previous = None;
        self.phase = Phase::Recovery;
        owned
    }

    pub(super) fn knocked(&mut self, tick: u64, received: [f64; 3]) {
        // Even zero/vertical-only server motion invalidates ownership of an earlier launch.
        self.owns_momentum = false;
        self.boosted = false;
        self.knock = received.iter().all(|v| v.is_finite()).then_some((tick, received[0].hypot(received[2]) > 0.1));
        self.phase = Phase::Recovery;
    }

    pub(super) fn step(&mut self, f: Frame, velocity: [f64; 2], speed_level: Option<u32>, boost: bool) -> Option<Move> {
        let Some(dir) = f.direction.filter(|d| d.iter().all(|n| n.is_finite()) && d[0].hypot(d[1]) > 1e-9) else {
            self.suspend(); return None;
        };
        if f.blocked || !f.position.iter().chain(velocity.iter()).all(|n| n.is_finite())
            || !f.base.is_finite() || f.base <= 0.0
        { self.suspend(); return None; }
        let norm = dir[0].hypot(dir[1]);
        let dir = [dir[0] / norm, dir[1] / norm];
        let previous = self.previous.replace((f.tick, f.position));
        if previous.is_some_and(|(tick, pos)| tick.checked_add(1) != Some(f.tick)
            || (f.position[0] - pos[0]).hypot(f.position[1] - pos[1]) > MAX_SAMPLE_DISTANCE)
        { self.suspend(); return None; }
        let magnitude = velocity[0].hypot(velocity[1]);
        if let Some((tick, eligible)) = self.knock {
            let age = f.tick.saturating_sub(tick);
            if age <= KNOCK_WINDOW {
                if boost && eligible && age > 0 && !self.boosted && magnitude > f.base {
                    self.boosted = true;
                    let from = velocity[1].atan2(velocity[0]);
                    let to = dir[1].atan2(dir[0]);
                    let delta = (to - from + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
                    let angle = from + delta.clamp(-WATCHDOG_TURN, WATCHDOG_TURN);
                    return Some(Move { across: [angle.cos() * magnitude, angle.sin() * magnitude], jump_scale: None });
                }
                if age <= 1 || magnitude > f.base { return None; }
            } else { self.knock = None; }
        }
        // Do not overwrite residual external momentum when the received event's steering window has expired.
        if !self.owns_momentum && magnitude > f.base { return None; }
        if !f.grounded {
            self.phase = Phase::Airborne;
            return None;
        }
        let multiplier = match speed_level {
            None => WATCHDOG_LAUNCH,
            Some(0) => WATCHDOG_SPEED_ONE,
            Some(amplifier) => WATCHDOG_SPEED_HIGH * (1.0 + WATCHDOG_EFFECT_STEP * f64::from(amplifier)),
        };
        self.phase = Phase::Launch;
        self.owns_momentum = true;
        Some(Move { across: [dir[0] * f.base * multiplier, dir[1] * f.base * multiplier], jump_scale: Some(1.0) })
    }
}

// Shared bounded recovery for the experimental modes. These are safety limits, not historical NCP constants.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Recovery {
    until: u64,
    last: Option<u64>,
    count: u8,
}

impl Recovery {
    pub(super) fn waiting(&self, now: u64) -> bool { now < self.until }
    pub(super) fn correction(&mut self, now: u64) -> bool {
        self.count = if self.last.is_some_and(|last| now.saturating_sub(last) < 100) { self.count.saturating_add(1) } else { 1 };
        self.last = Some(now);
        self.until = now.saturating_add(10);
        self.count >= 3
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
    fn watchdog_uses_a_native_air_cycle_and_distinct_effect_launches() {
        for (level, multiplier) in [(None, 1.68), (Some(0), 1.8), (Some(1), 1.6 * 1.07)] {
            let mut w = Watchdog::default();
            let launch = w.step(frame(0, 0.0, true), [0.0; 2], level, false).unwrap();
            assert!((launch.across[0] - 0.2873 * multiplier).abs() < 1e-9);
            assert_eq!(launch.jump_scale, Some(1.0));
            assert_eq!(w.step(frame(1, 0.4, false), [0.3, 0.0], level, false), None, "native air physics owns the move");
            assert!(w.step(frame(2, 0.7, true), [0.2, 0.0], level, false).is_some(), "no competing global jump-delay controller");
            assert!(w.reset());
        }
    }

    #[test]
    fn watchdog_never_recreates_knockback_removed_by_velocity_and_wraps_turns() {
        let mut w = Watchdog::default();
        w.knocked(0, [-0.7, 0.4, -0.01]);
        assert_eq!(w.step(frame(1, 0.0, false), [0.0; 2], None, true), None, "Velocity's zero share stays zero");
        let mut f = frame(2, 0.0, false);
        f.direction = Some([-1.0, 0.01]); // Crosses the +/-pi seam.
        let boosted = w.step(f, [-0.5, -0.01], None, true).unwrap();
        assert!((boosted.across[0].hypot(boosted.across[1]) - 0.5f64.hypot(0.01)).abs() < 1e-9);
        assert!(boosted.across[0] < -0.49 && boosted.across[1] > 0.0, "take the short turn across the seam");
        assert_eq!(w.step(frame(3, 0.0, false), [0.5, 0.0], None, true), None, "one steering action per genuine impulse");
        assert!(!w.reset(), "external knockback never becomes owned launch momentum");
    }

    #[test]
    fn watchdog_yields_to_collisions_input_loss_and_server_momentum() {
        let mut w = Watchdog::default();
        w.step(frame(0, 0.0, true), [0.0; 2], None, false);
        w.knocked(1, [1.0, 0.0, 0.0]);
        assert!(!w.suspend());
        assert_eq!(w.step(frame(2, 0.0, true), [0.8, 0.0], None, false), None);
        assert_eq!(w.step(frame(20, 0.0, true), [0.8, 0.0], None, false), None);
        let mut f = frame(21, 0.0, true);
        f.blocked = true;
        assert_eq!(w.step(f, [0.0; 2], None, false), None);
        f.blocked = false;
        f.direction = None;
        assert_eq!(w.step(f, [0.0; 2], None, false), None);
    }

    #[test]
    fn repeated_setbacks_pause_then_stop_instead_of_looping() {
        let mut r = Recovery::default();
        assert!(!r.correction(10));
        assert!(r.waiting(19));
        assert!(!r.waiting(20));
        assert!(!r.correction(30));
        assert!(r.correction(50));
        assert!(!r.correction(200), "isolated legitimate teleports do not accumulate forever");
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
