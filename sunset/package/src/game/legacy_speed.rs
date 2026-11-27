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
    pub(super) fn recovering(&self) -> bool { self.phase == Phase::Recovery }

    fn recover_state(&mut self) {
        // Keep the ownership bit until the game adapter actually restores the owned surplus momentum.
        self.previous = None;
        self.phase = Phase::Recovery;
    }
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

    pub(super) fn filtered_knockback(&mut self, tick: u64, horizontal_share: f64) {
        if let Some((at, eligible)) = self.knock.as_mut()
            && *at == tick && (!horizontal_share.is_finite() || horizontal_share <= 0.0)
        { *eligible = false; }
    }

    pub(super) fn step(&mut self, f: Frame, velocity: [f64; 2], speed_level: Option<u32>, boost: bool) -> Option<Move> {
        let Some(dir) = f.direction.filter(|d| d.iter().all(|n| n.is_finite()) && d[0].hypot(d[1]) > 1e-9) else {
            self.recover_state(); return None;
        };
        if f.blocked || !f.position.iter().chain(velocity.iter()).all(|n| n.is_finite())
            || !f.base.is_finite() || f.base <= 0.0
        { self.recover_state(); return None; }
        let norm = dir[0].hypot(dir[1]);
        let dir = [dir[0] / norm, dir[1] / norm];
        let previous = self.previous.replace((f.tick, f.position));
        if previous.is_some_and(|(tick, pos)| tick.checked_add(1) != Some(f.tick)
            || (f.position[0] - pos[0]).hypot(f.position[1] - pos[1]) > MAX_SAMPLE_DISTANCE)
        { self.recover_state(); return None; }
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

// ---- the 1.8 strafe: Speed's NCP Hop and Flight's Old NCP set the tick's move the way a 1.8 client's moveFlying does
// when its friction is replaced (the speed given, along the move keys at the yaw, what was left of the old motion kept
// at a random 0 to 1/2000). Floats are the 1.8 client's (f32 constants widened).

// The move keys as a 1.8 client has them: each 0.98, sneaking a 0.3 share, an item in use its slowdown share.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Input {
    pub forward: f64,
    pub strafe: f64,
    pub yaw: f32,
}

impl Input {
    // `forward`/`left`: -1, 0 or 1 from the keys; `use_factor`: 1 with no item in use.
    pub(super) fn of(forward: f64, left: f64, sneaking: bool, use_factor: f64, yaw: f32) -> Self {
        let k = f64::from(0.98f32) * if sneaking { f64::from(0.3f32) } else { 1.0 } * use_factor;
        Self { forward: forward * k, strafe: left * k, yaw }
    }

    pub(super) fn moving(&self) -> bool {
        self.forward != 0.0 || self.strafe != 0.0
    }

    // `enoughMovementForSprinting`: a key's share of 0.8 or more.
    fn sprint_input(&self) -> bool {
        self.forward.abs() >= f64::from(0.8f32) || self.strafe.abs() >= f64::from(0.8f32)
    }
}

// Where you are and what slows you, for the allowed distance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Allowance {
    pub web: bool,
    pub liquid: bool,
    // Depth Strider's level, 0 to 3.
    pub depth_strider: u32,
    pub sneaking: bool,
    // The Speed effect's level (amplifier + 1), if any.
    pub speed: Option<u32>,
    pub slowness: bool,
}

const DEPTH_STRIDER: [f64; 4] = [1.0, 1.4304347400741908, 1.7347825295420372, 1.9217390955733897];

// The horizontal distance a tick a 1.8 strategy allows itself: walking's 0.221 (a sprint's 1.3 times it whenever the
// keys could sprint), sneaking's, a liquid's (with Depth Strider) or a web's; the Speed effect's 20% a level; Slowness
// sets it to 0.29.
pub(super) fn allowed_distance(a: Allowance, input: Input) -> f64 {
    let (mut d, scaled) = if a.web {
        (0.105, false)
    } else if a.liquid {
        let d = 0.11500000208616258;
        if a.depth_strider > 0 { (d * DEPTH_STRIDER[a.depth_strider.min(3) as usize], true) } else { (d, false) }
    } else if a.sneaking {
        (0.0663000026345253, false)
    } else {
        (0.221, true)
    };
    if scaled {
        if input.sprint_input() {
            d *= f64::from(1.3f32);
        }
        if let Some(level) = a.speed {
            d *= 1.0 + 0.2 * f64::from(level);
        }
        if a.slowness {
            d = 0.29;
        }
    }
    d
}

// The tick's move across: `speed` along the keys at the yaw (a diagonal's 0.98 of it, as the strafe it replaces has),
// plus `residual` (the motion left from the last move) times `keep`.
pub(super) fn strafe_move(speed: f64, input: Input, residual: [f64; 2], keep: f64) -> [f64; 2] {
    let diagonal = input.forward != 0.0 && input.strafe != 0.0;
    let friction = f64::from((if diagonal { speed * f64::from(0.98f32) } else { speed }) as f32);
    let mut out = [residual[0] * keep, residual[1] * keep];
    let f = input.strafe * input.strafe + input.forward * input.forward;
    if f >= 1.0e-4 {
        let f = friction / f.sqrt().max(1.0);
        let (strafe, forward) = (input.strafe * f, input.forward * f);
        let yaw = f64::from(input.yaw).to_radians() as f32;
        let (sin, cos) = (f64::from(yaw.sin()), f64::from(yaw.cos()));
        out[0] += strafe * cos - forward * sin;
        out[1] += forward * cos + strafe * sin;
    }
    out
}

// Speed's NCP Hop: a jump each landing at Ground speed times the allowed distance, the first tick after it drawn back
// toward the allowed distance by Slope, then a 1/159.9 loss a tick, never under the allowed distance; a wall or no keys
// bring it back to that. Damage boost takes a knockback's speed across; Custom boost sets Boost speed for the first
// ticks of a hurt; Low hop and Y-port pull the hop down early.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HopCfg {
    pub jump: f64,
    pub ground_speed: f64,
    pub slope: f64,
    // Boost speed and the hurt ticks it lasts.
    pub custom_boost: Option<(f64, u32)>,
    pub low_hop: bool,
    pub y_port: bool,
    // Jump Boost's level (amplifier + 1), 0 without.
    pub jump_boost: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HopFrame {
    // Ticks since you left the ground (0 on it).
    pub air: u32,
    pub moving: bool,
    pub blocked: bool,
    pub allowed: f64,
    // Vertical speed before this tick's move.
    pub vy: f64,
    // Ticks since you were hurt, while the hurt lasts.
    pub hurt: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HopOut {
    pub speed: f64,
    pub vy: Option<f64>,
}

const LOW_HOP_FALL: f64 = -0.09800000190734864;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct NcpHop {
    speed: f64,
    reset: bool,
}

impl NcpHop {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    // A teleport: nothing carried.
    pub(super) fn teleported(&mut self) {
        self.speed = 0.0;
    }

    // Damage boost: the knockback's speed across.
    pub(super) fn knocked(&mut self, across: f64) {
        if across.is_finite() {
            self.speed = across;
        }
    }

    pub(super) fn step(&mut self, f: HopFrame, c: HopCfg) -> HopOut {
        let mut vy = None;
        if c.low_hop && f.air == 4 {
            vy = Some(LOW_HOP_FALL);
        }
        if c.y_port && f.air == 5 && (f.vy - LOW_HOP_FALL.abs()).abs() < 0.12 {
            vy = Some(LOW_HOP_FALL);
        }
        if let Some((boost, ticks)) = c.custom_boost
            && f.hurt.is_some_and(|t| t <= ticks)
        {
            self.speed = boost;
        }
        let d0 = f.allowed;
        if f.moving {
            match f.air {
                0 => {
                    let jump = if !f.blocked && (c.jump as f32) == 0.4f32 { 0.4f32 } else { 0.42f32 };
                    vy = Some(f64::from(jump) + f64::from(c.jump_boost) * f64::from(0.1f32));
                    self.speed = d0 * c.ground_speed;
                }
                1 => self.speed -= c.slope * (self.speed - d0),
                _ => self.speed -= self.speed / f64::from(159.9f32),
            }
            self.reset = false;
        } else if !self.reset {
            self.speed = d0;
            self.reset = true;
        }
        if f.blocked {
            self.speed = d0;
        }
        HopOut { speed: self.speed.max(d0), vy }
    }
}

#[cfg(test)]
mod hop_tests {
    use super::*;

    const CFG: HopCfg = HopCfg { jump: 0.4, ground_speed: 1.75, slope: 0.66, custom_boost: None, low_hop: false, y_port: false, jump_boost: 0 };

    fn frame(air: u32, allowed: f64) -> HopFrame {
        HopFrame { air, moving: true, blocked: false, allowed, vy: 0.0, hurt: None }
    }

    #[test]
    fn a_hop_launches_then_slopes_then_decays_but_never_under_the_allowed_distance() {
        let walk = Input::of(1.0, 0.0, false, 1.0, 0.0);
        let d0 = allowed_distance(Allowance::default(), walk);
        assert!((d0 - 0.221 * f64::from(1.3f32)).abs() < 1e-12, "{d0}");
        let mut hop = NcpHop::default();
        let launch = hop.step(frame(0, d0), CFG);
        assert_eq!(launch.vy, Some(f64::from(0.4f32)));
        assert!((launch.speed - d0 * 1.75).abs() < 1e-12);
        let first = hop.step(frame(1, d0), CFG).speed;
        assert!((first - (d0 * 1.75 - 0.66 * (d0 * 1.75 - d0))).abs() < 1e-12);
        let mut last = first;
        for air in 2..40 {
            let s = hop.step(frame(air, d0), CFG).speed;
            assert!(s <= last && s >= d0);
            last = s;
        }
        // A wall: back to the allowed distance; no keys: the same, once.
        assert_eq!(hop.step(HopFrame { blocked: true, ..frame(3, d0) }, CFG).speed, d0);
        assert_eq!(hop.step(HopFrame { moving: false, ..frame(0, d0) }, CFG), HopOut { speed: d0, vy: None });
    }

    #[test]
    fn a_jump_against_a_wall_or_off_the_setting_is_the_full_jump_and_low_hop_pulls_down() {
        let mut hop = NcpHop::default();
        assert_eq!(hop.step(HopFrame { blocked: true, ..frame(0, 0.28) }, CFG).vy, Some(f64::from(0.42f32)));
        assert_eq!(hop.step(frame(0, 0.28), HopCfg { jump: 0.41, ..CFG }).vy, Some(f64::from(0.42f32)));
        let boosted = hop.step(frame(0, 0.28), HopCfg { jump_boost: 2, ..CFG }).vy.unwrap();
        assert!((boosted - (f64::from(0.4f32) + 2.0 * f64::from(0.1f32))).abs() < 1e-12);
        assert_eq!(hop.step(frame(4, 0.28), HopCfg { low_hop: true, ..CFG }).vy, Some(LOW_HOP_FALL));
        assert_eq!(hop.step(HopFrame { vy: 0.1, ..frame(5, 0.28) }, HopCfg { y_port: true, ..CFG }).vy, Some(LOW_HOP_FALL));
    }

    #[test]
    fn knockback_and_custom_boost_set_the_speed() {
        let mut hop = NcpHop::default();
        hop.step(frame(0, 0.28), CFG);
        hop.knocked(0.9);
        assert!((hop.step(frame(3, 0.28), CFG).speed - (0.9 - 0.9 / f64::from(159.9f32))).abs() < 1e-12);
        let boost = HopCfg { custom_boost: Some((1.2, 6)), ..CFG };
        let s = hop.step(HopFrame { hurt: Some(2), ..frame(3, 0.28) }, boost).speed;
        assert!((s - (1.2 - 1.2 / f64::from(159.9f32))).abs() < 1e-12);
    }

    #[test]
    fn the_strafe_moves_a_diagonal_as_far_as_a_straight_line() {
        let straight = strafe_move(0.3, Input::of(1.0, 0.0, false, 1.0, 0.0), [0.0; 2], 0.0);
        let diagonal = strafe_move(0.3, Input::of(1.0, 1.0, false, 1.0, 0.0), [0.0; 2], 0.0);
        let len = |v: [f64; 2]| v[0].hypot(v[1]);
        assert!((len(straight) - 0.3 * f64::from(0.98f32)).abs() < 1e-6, "{straight:?}");
        assert!((len(straight) - len(diagonal)).abs() < 1e-6);
        // Yaw 0 faces +z; left (A) is +x.
        assert!(straight[1] > 0.29 && straight[0].abs() < 1e-9);
        let left = strafe_move(0.3, Input::of(0.0, 1.0, false, 1.0, 0.0), [0.0; 2], 0.0);
        assert!(left[0] > 0.29);
        // No keys: only what was left, kept by its share.
        assert_eq!(strafe_move(0.3, Input::default(), [1.0, 2.0], 0.5), [0.5, 1.0]);
    }

    #[test]
    fn the_allowed_distance_follows_the_1_8_cases() {
        let walk = Input::of(1.0, 0.0, false, 1.0, 0.0);
        assert_eq!(allowed_distance(Allowance { web: true, ..Allowance::default() }, walk), 0.105);
        assert_eq!(allowed_distance(Allowance { sneaking: true, ..Allowance::default() }, walk), 0.0663000026345253);
        assert_eq!(allowed_distance(Allowance { liquid: true, ..Allowance::default() }, walk), 0.11500000208616258);
        let strider = allowed_distance(Allowance { liquid: true, depth_strider: 3, ..Allowance::default() }, walk);
        assert!((strider - 0.11500000208616258 * 1.9217390955733897 * f64::from(1.3f32)).abs() < 1e-12);
        let fast = allowed_distance(Allowance { speed: Some(2), ..Allowance::default() }, walk);
        assert!((fast - 0.221 * f64::from(1.3f32) * 1.4).abs() < 1e-12);
        assert_eq!(allowed_distance(Allowance { slowness: true, speed: Some(1), ..Allowance::default() }, walk), 0.29);
        // An item in use takes the sprint's share away.
        let using = Input::of(1.0, 0.0, false, 0.2, 0.0);
        assert_eq!(allowed_distance(Allowance::default(), using), 0.221);
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
        w.filtered_knockback(0, 0.0);
        assert_eq!(w.step(frame(1, 0.0, false), [0.7, 0.0], None, true), None,
            "zero Velocity share restores old momentum; it is not a new damage impulse to steer");
        w.knocked(1, [-0.7, 0.4, -0.01]);
        w.filtered_knockback(1, 0.5);
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
    fn watchdog_interruption_keeps_cleanup_ownership_until_adapter_restores_it() {
        let mut w = Watchdog::default();
        w.step(frame(0, 0.0, true), [0.0; 2], None, false);
        let mut stopped = frame(1, 0.4, false);
        stopped.direction = None;
        assert_eq!(w.step(stopped, [0.45, 0.0], None, false), None);
        assert!(w.recovering());
        assert!(w.suspend(), "the game adapter still owes restoration of the launch surplus");
        assert!(!w.reset());
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
