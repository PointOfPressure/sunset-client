// How a look moves toward where it is going, a tick at a time, shared by the modules that aim. Degrees.

const NORMAL_SPEED: f32 = 45.0;
const SMOOTH_SHARE: f32 = 0.35;
const SMOOTH_LEAST: f32 = 1.0;

fn toward(from: f32, delta: f32, share: f32, least: f32, limit: f32) -> f32 {
    let wanted = (delta.abs() * share).max(least).min(delta.abs()).min(limit);
    from + wanted.copysign(delta)
}

// The next (yaw, pitch). Snap arrives at once; Normal turns at a steady speed; Smooth covers a share of what is left;
// Custom covers (1 - smoothing) of what is left, at most its speeds.
pub(super) fn step(mode: &str, from: (f32, f32), to: (f32, f32), yaw_speed: f32, pitch_speed: f32, smoothing: f32) -> (f32, f32) {
    if !from.0.is_finite() || !from.1.is_finite() || !to.0.is_finite() || !to.1.is_finite() {
        return from;
    }
    let dy = (to.0 - from.0 + 180.0).rem_euclid(360.0) - 180.0;
    let dp = to.1 - from.1;
    let (yaw, pitch) = match mode {
        "Snap" => (from.0 + dy, to.1),
        "Normal" => (toward(from.0, dy, 1.0, 0.0, NORMAL_SPEED), toward(from.1, dp, 1.0, 0.0, NORMAL_SPEED)),
        "Smooth" => (toward(from.0, dy, SMOOTH_SHARE, SMOOTH_LEAST, f32::MAX), toward(from.1, dp, SMOOTH_SHARE, SMOOTH_LEAST, f32::MAX)),
        _ => {
            let share = 1.0 - smoothing.clamp(0.0, 0.9);
            (toward(from.0, dy, share, SMOOTH_LEAST, yaw_speed.max(0.0)), toward(from.1, dp, share, SMOOTH_LEAST, pitch_speed.max(0.0)))
        }
    };
    (yaw, pitch.clamp(-90.0, 90.0))
}

// A step toward a look that is itself moving: `was` is where it was last tick, and the look first moves as far as it
// did, so a mode's smoothing eats into how far behind you are and not into keeping up. Normal and Custom still turn no
// faster than their speeds.
pub(super) fn track(mode: &str, from: (f32, f32), to: (f32, f32), was: Option<(f32, f32)>, yaw_speed: f32, pitch_speed: f32, smoothing: f32) -> (f32, f32) {
    let wrap = |a: f32| (a + 180.0).rem_euclid(360.0) - 180.0;
    let carried = was.map_or(from, |w| (from.0 + wrap(to.0 - w.0), from.1 + (to.1 - w.1)));
    let next = step(mode, carried, to, yaw_speed, pitch_speed, smoothing);
    let (most_yaw, most_pitch) = match mode {
        "Normal" => (NORMAL_SPEED, NORMAL_SPEED),
        "Custom" => (yaw_speed.max(0.0), pitch_speed.max(0.0)),
        _ => return next,
    };
    (from.0 + wrap(next.0 - from.0).clamp(-most_yaw, most_yaw), (from.1 + (next.1 - from.1).clamp(-most_pitch, most_pitch)).clamp(-90.0, 90.0))
}

// The way a (yaw, pitch) looks, length 1. Yaw 0 faces south (+z).
pub(super) fn direction(look: (f32, f32)) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(look.0).to_radians(), f64::from(look.1).to_radians());
    [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
}

// ---- the silent look the game's own move packet carries (Kill Aura's Silent aura mode)
//
// Ported from vape-rs (github.com/rdbtCVS/vape-rs, MIT, commit d6e48b1): its SilentAura's managed rotation
// (`modules/combat/rotation.rs` Managed, `silent_aura.rs` updateAim and getSpeed). The look moves in whole mouse
// counts of your sensitivity, as a mouse would turn it, 50 small steps a tick toward a target that a PID controller
// with a little jitter keeps on the aim point. It goes to the server as the rotation the game's position sender is
// given (a hook on `position_sender.update`), so the game sends a look only when it changed, in its own move packet.

use std::sync::Mutex;

// The look the position sender is given instead of yours, while a module holds one.
static SENT: Mutex<Option<(f32, f32)>> = Mutex::new(None);

// The hook on the position sender's rotation.
pub(crate) fn sent(value: (f32, f32)) -> (f32, f32) {
    SENT.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or(value)
}

fn set_sent(look: Option<(f32, f32)>) {
    *SENT.lock().unwrap_or_else(|e| e.into_inner()) = look;
}

// Degrees one mouse count turns the view at this sensitivity (Minecraft's `(s * 0.6 + 0.2)^3 * 8 * 0.15`).
pub(super) fn degrees_per_count(sensitivity: f64) -> f32 {
    let base = sensitivity as f32 * 0.6 + 0.2;
    (base * base * base * 8.0 * 0.15).max(1e-4)
}

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

// A small xorshift generator: the jitter and the speeds only need to vary.
#[derive(Clone, Copy, Debug)]
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub(super) fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next() * (hi - lo)
    }
    fn gaussian(&mut self) -> f64 {
        let u = self.next().max(1e-12);
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * self.next()).cos()
    }
}

// The managed look: where it is, where it is going, the counts not yet turned, and the aim's PID state.
#[derive(Clone, Copy, Debug)]
pub(super) struct Managed {
    pub(super) look: (f32, f32),
    previous: (f32, f32),
    pending: (f32, f32),
    target: Option<(f32, f32)>,
    // Going back to your own look, after which it lets go.
    returning: bool,
    gains: [f64; 4],
    integrals: [f64; 2],
    jitter: [[f64; 3]; 2],
    jitter_next: u32,
    rng: Rng,
}

impl Managed {
    // From your look.
    pub(super) fn start(view: (f32, f32), seed: u64) -> Self {
        let mut m = Self {
            look: view,
            previous: view,
            pending: (0.0, 0.0),
            target: None,
            returning: false,
            gains: [1.0; 4],
            integrals: [0.0; 2],
            jitter: [[0.0; 3]; 2],
            jitter_next: 0,
            rng: Rng::new(seed),
        };
        m.new_target();
        m
    }

    // A new target: fresh gains, and the controller starts again.
    pub(super) fn new_target(&mut self) {
        let r = &mut self.rng;
        self.gains = [0.85 + r.next() * 0.3, 0.85 + r.next() * 0.3, 0.8 + r.next() * 0.4, 0.85 + r.next() * 0.3];
        self.integrals = [0.0; 2];
        self.returning = false;
    }

    // Back to `view`, then let go.
    pub(super) fn release(&mut self) {
        self.returning = true;
        self.integrals = [0.0; 2];
    }

    pub(super) fn returning(&self) -> bool {
        self.returning
    }

    // SilentAura's `updateAim` (PID mode): the next target from the aim point seen from the eye. `motion` is your own
    // move this tick, `target_motion` the target's; `close` is how far the aim point is (inside 0.8 it slows).
    pub(super) fn aim(&mut self, eye: [f64; 3], point: [f64; 3], height: f64, motion: [f64; 3], target_motion: f64, distance: f64, ticks: u32) {
        // SilentAuraAimJitter: offsets that drift toward targets rolled again every 5 to 50 ticks.
        const RANGES: [(f64, f64); 3] = [(-0.15, 0.15), (-0.3, 0.25), (-0.15, 0.15)];
        if ticks >= self.jitter_next {
            for (t, (lo, hi)) in self.jitter[1].iter_mut().zip(RANGES) {
                *t = self.rng.range(lo, hi);
            }
            self.jitter_next = ticks + 2 + (self.rng.next() * 18.0) as u32;
        }
        for i in 0..3 {
            let step = 0.01 + self.rng.next() * 0.05;
            self.jitter[0][i] += (self.jitter[1][i] - self.jitter[0][i]).clamp(-step, step);
        }
        let j = self.jitter[0];
        let jx = point[0] + j[0] * (1.0 + target_motion);
        let jz = point[2] + j[2] * (1.0 + target_motion);
        let jy = if eye[1] < point[1] { point[1] + j[1] * 0.5 } else { eye[1].min(point[1] + height) - 0.275 + j[1] };
        let desired = super::sunset::look_at(eye, [jx, jy, jz]);
        let target_pitch = super::sunset::look_at(eye, [point[0], jy, point[2]]).1;
        let mut yaw_err = f64::from(wrap(desired.0 - self.look.0));
        let mut pitch_err = f64::from(wrap(target_pitch - self.look.1));
        let prev_pitch_step = f64::from(self.look.1 - self.previous.1);
        let mut prev_yaw_step = f64::from(wrap(self.look.0 - self.previous.0));
        let ready = yaw_err.hypot(pitch_err) < 3.0;
        let [gp, gip, gy, giy] = self.gains;
        let (pitch_p, pitch_i, yaw_i) = (0.45 * gp, 0.91 * gip, 0.33 * giy);
        let mut yaw_p = if ready { 0.05 } else { 0.1 } * gy;
        if motion[1].abs() > 0.1 {
            pitch_err *= 1.0 + self.rng.next() * 0.32;
        }
        if yaw_err.signum() == prev_yaw_step.signum() && yaw_err.abs() < 20.0 {
            yaw_p *= 2.5;
            prev_yaw_step *= 1.0 + (target_motion + motion[0].hypot(motion[2])).min(0.25);
        }
        if distance < 0.8 {
            let f = distance / 0.8;
            pitch_err *= f * f;
            yaw_err *= f;
        }
        let sign = if self.rng.next() < 0.5 { -1.0 } else { 1.0 };
        let pitch_ctl = pitch_err - prev_pitch_step + prev_yaw_step * 0.05 * sign;
        let yaw_ctl = yaw_err - prev_yaw_step;
        self.integrals[0] += pitch_ctl * 0.05;
        self.integrals[1] += yaw_ctl * 0.05;
        let pitch_adj = pitch_p * pitch_ctl + pitch_i * self.integrals[0];
        let mut yaw_adj = yaw_p * yaw_ctl + yaw_i * self.integrals[1];
        if yaw_err.abs() > 120.0 {
            self.integrals[1] = 0.0;
            yaw_adj = 0.0;
        }
        self.target = Some(((f64::from(self.look.0) + yaw_err + yaw_adj / 3.0) as f32, (f64::from(self.look.1) + pitch_adj) as f32));
    }

    // One tick's 50 controller updates toward the target (or back to `view`), each a whole number of counts of
    // `aim_speed` (getSpeed: aim speed times a random 1.4 to 3, a quarter of that a step). True once it is back.
    pub(super) fn tick(&mut self, view: (f32, f32), sensitivity: f64, aim_speed: f64, distance: f64) -> bool {
        self.previous = self.look;
        let degrees = degrees_per_count(sensitivity);
        let Some(target) = (if self.returning { Some(view) } else { self.target }) else {
            return false;
        };
        for _ in 0..50 {
            let mut speed = aim_speed * (0.65 + 0.25 * self.rng.gaussian()).exp().clamp(1.4, 3.0);
            if distance < 0.8 && !self.returning {
                speed *= distance / 0.8;
            }
            let counts = speed * 0.25;
            let predicted = (self.look.0 + self.pending.0.trunc() * degrees, self.look.1 + self.pending.1.trunc() * degrees);
            let err = (f64::from(wrap(target.0 - predicted.0)), f64::from(wrap(target.1 - predicted.1)));
            let outside = |e: f64| (e.abs() / f64::from(degrees)).round() > 0.0;
            if outside(err.0) {
                self.pending.0 += (counts.min((err.0 / f64::from(degrees)).abs()) * err.0.signum()) as f32;
            }
            if outside(err.1) {
                self.pending.1 += (counts.min((err.1 / f64::from(degrees)).abs()) * err.1.signum()) as f32;
            }
            // Whole counts turn the look; what is left waits for the next step.
            let whole = (self.pending.0.trunc(), self.pending.1.trunc());
            self.pending = (self.pending.0 - whole.0, self.pending.1 - whole.1);
            self.look = (self.look.0 + whole.0 * degrees, (self.look.1 + whole.1 * degrees).clamp(-90.0, 90.0));
        }
        self.returning && wrap(view.0 - self.look.0).abs() <= degrees && (view.1 - self.look.1).abs() <= degrees
    }

    // The look goes out in the game's next move packet (only if it changed).
    pub(super) fn send(&self) {
        set_sent(Some(self.look));
    }
}

// Nothing held: the game sends your own look again.
pub(super) fn let_go() {
    set_sent(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_arrives_at_once() {
        assert_eq!(step("Snap", (0.0, 0.0), (90.0, 30.0), 1.0, 1.0, 0.5), (90.0, 30.0));
    }

    #[test]
    fn normal_turns_at_a_steady_speed_and_stops_on_the_target() {
        assert_eq!(step("Normal", (0.0, 0.0), (90.0, 10.0), 1.0, 1.0, 0.0), (45.0, 10.0));
    }

    #[test]
    fn turns_take_the_short_way_round() {
        let (yaw, _) = step("Custom", (179.0, 0.0), (-179.0, 0.0), 180.0, 180.0, 0.0);
        assert!((yaw - 181.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_reaches_the_target_without_passing_it() {
        let mut look = (0.0, 0.0);
        for _ in 0..60 {
            look = step("Smooth", look, (120.0, -40.0), 0.0, 0.0, 0.0);
            assert!(look.0 <= 120.001 && look.1 >= -40.001);
        }
        assert!((look.0 - 120.0).abs() < 1e-3 && (look.1 + 40.0).abs() < 1e-3);
    }

    #[test]
    fn custom_keeps_its_separate_speeds() {
        assert_eq!(step("Custom", (0.0, 0.0), (90.0, 60.0), 20.0, 5.0, 0.0), (20.0, 5.0));
    }

    #[test]
    fn tracking_stays_on_a_moving_look() {
        for mode in ["Snap", "Normal", "Smooth", "Custom"] {
            let (mut look, mut was) = ((0.0, 0.0), None);
            for n in 0..60 {
                let to = (n as f32 * 6.0, 10.0);
                look = track(mode, look, to, was, 60.0, 60.0, 0.5);
                was = Some(to);
            }
            assert!((look.0 - 354.0).abs() < 1e-2 && (look.1 - 10.0).abs() < 1e-2, "{mode}: {look:?}");
        }
    }

    #[test]
    fn tracking_only_smooths_what_is_behind() {
        // The target look moves 6 degrees and you are 90 behind: 6 kept up, then Smooth's share of the rest.
        let look = track("Smooth", (0.0, 0.0), (96.0, 0.0), Some((90.0, 0.0)), 0.0, 0.0, 0.0);
        assert!((look.0 - (6.0 + 90.0 * SMOOTH_SHARE)).abs() < 1e-3);
    }

    #[test]
    fn tracking_keeps_to_the_speed_limit() {
        assert_eq!(track("Custom", (0.0, 0.0), (90.0, 60.0), Some((80.0, 50.0)), 20.0, 5.0, 0.0), (20.0, 5.0));
        assert_eq!(track("Normal", (0.0, 0.0), (170.0, 0.0), Some((100.0, 0.0)), 0.0, 0.0, 0.0), (45.0, 0.0));
    }

    #[test]
    fn tracking_a_new_target_is_a_plain_step() {
        assert_eq!(track("Smooth", (0.0, 0.0), (40.0, 0.0), None, 0.0, 0.0, 0.0), step("Smooth", (0.0, 0.0), (40.0, 0.0), 0.0, 0.0, 0.0));
    }


    #[test]
    fn the_managed_look_turns_in_whole_counts_and_reaches_the_target() {
        let degrees = degrees_per_count(0.5);
        let mut m = Managed::start((0.0, 0.0), 7);
        for n in 0..40 {
            m.aim([0.0, 1.62, 0.0], [-2.0, 0.0, 2.0], 1.8, [0.0; 3], 0.0, 3.0, n);
            m.tick((0.0, 0.0), 0.5, 7.0, 3.0);
            let counts = m.look.0 / degrees;
            assert!((counts - counts.round()).abs() < 1e-2, "{counts}");
        }
        // The point is at yaw 45 (-x, +z).
        assert!((m.look.0 - 45.0).abs() < 4.0, "{:?}", m.look);
    }

    #[test]
    fn the_managed_look_goes_back_and_lets_go() {
        let mut m = Managed::start((0.0, 0.0), 3);
        m.look = (60.0, 20.0);
        m.release();
        let mut back = false;
        for _ in 0..20 {
            back = m.tick((0.0, 0.0), 0.5, 7.0, 3.0);
            if back {
                break;
            }
        }
        assert!(back, "{:?}", m.look);
    }

    #[test]
    fn direction_matches_the_game_axes() {
        let south = direction((0.0, 0.0));
        assert!((south[2] - 1.0).abs() < 1e-9);
        assert!((direction((0.0, 90.0))[1] + 1.0).abs() < 1e-9);
    }
}
