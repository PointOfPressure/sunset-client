// Kill Aura's aiming, apart from the game (aura.rs feeds it positions and line-of-sight answers):
// - `Track`: a bounded, latency-aware predictor of where the target will be, that checks its own one-tick forecasts
//   against where the target turned up and trusts itself less as they miss (knockback, reversals), down to none.
// - `plan_point`: where in the target's box to aim. Dynamic weighs points across the box by the turn they need, how
//   central (robust to movement) they are, reach and sight, and keeps the last point while it stays good.
// - `adaptive_step`: a turn that is fast when far off, settles near the point, holds still inside a deadzone, and lands
//   exactly when a hit is due.
// - `smart_score` and `Recent`: the composite target priority and Switch's memory of whom it hit.

pub(super) type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn len(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

// ---- prediction

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Prediction {
    Off,
    Basic,
    Adaptive,
}

impl Prediction {
    pub(super) fn named(name: &str) -> Self {
        match name {
            "Off" => Self::Off,
            "Adaptive" => Self::Adaptive,
            _ => Self::Basic,
        }
    }
}

// A one-tick forecast missing by this much (blocks, smoothed) leaves no trust in the prediction.
const UNTRUSTED: f64 = 0.6;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Track {
    id: Option<u32>,
    last: V3,
    vel: V3,
    acc: V3,
    forecast: Option<V3>,
    // Smoothed miss of the one-tick forecasts, blocks.
    pub(super) error: f64,
    samples: u32,
    reversed: bool,
}

impl Track {
    // Where target `id` is this tick. A new target starts the track afresh.
    pub(super) fn observe(&mut self, id: u32, pos: V3) {
        if self.id != Some(id) {
            *self = Self { id: Some(id), last: pos, ..Self::default() };
            return;
        }
        if let Some(f) = self.forecast {
            self.error = 0.7 * self.error + 0.3 * len(sub(f, pos));
        }
        let vel = sub(pos, self.last);
        // A teleport or respawn (over 4 blocks a tick): nothing to carry over.
        if len(vel) > 4.0 {
            *self = Self { id: Some(id), last: pos, ..Self::default() };
            return;
        }
        let horizontal_dot = vel[0] * self.vel[0] + vel[2] * self.vel[2];
        self.reversed = self.samples > 0 && horizontal_dot < 0.0;
        let acc = sub(vel, self.vel);
        // Acceleration counts only once there is a velocity before it, and never more than a jump's worth a tick.
        self.acc = if self.samples > 0 { acc.map(|a| a.clamp(-0.5, 0.5)) } else { [0.0; 3] };
        self.vel = vel;
        self.last = pos;
        self.samples = self.samples.saturating_add(1);
        self.forecast = Some(add(add(pos, vel), scale(self.acc, 0.5)));
    }

    // How much the prediction is trusted now, 0 to 1.
    pub(super) fn trust(&self) -> f64 {
        if self.samples < 2 {
            return 0.5;
        }
        (1.0 - self.error / UNTRUSTED).clamp(0.0, 1.0)
    }

    // Where the target is expected `ticks` ahead (at most 3), at `strength` (0 to 2). Off: where it is. Basic: the last
    // tick's movement carried on. Adaptive: velocity and (unless it just reversed) half its acceleration, scaled by trust.
    pub(super) fn predict(&self, mode: Prediction, ticks: f64, strength: f64) -> V3 {
        let strength = strength.clamp(0.0, 2.0);
        match mode {
            Prediction::Off => self.last,
            Prediction::Basic => add(self.last, scale(self.vel, strength)),
            Prediction::Adaptive => {
                let h = if self.reversed { 1.0 } else { ticks.clamp(1.0, 3.0) };
                let acc = if self.reversed { [0.0; 3] } else { scale(self.acc, 0.5 * h * h) };
                let moved = add(scale(self.vel, h), acc);
                add(self.last, scale(moved, self.trust() * strength))
            }
        }
    }
}

// The ticks ahead to aim for at a ping: the attack reaches the server half a round trip later, at least a tick.
pub(super) fn horizon_ticks(latency_ms: Option<i32>) -> f64 {
    1.0 + f64::from(latency_ms.unwrap_or(50).clamp(0, 400)) / 100.0
}

// ---- the aim point

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum AimPoint {
    Dynamic,
    Center,
    Nearest,
    // The box's middle at this share of its height.
    Custom(f64),
}

impl AimPoint {
    pub(super) fn named(name: &str, height: f64) -> Self {
        match name {
            "Center" => Self::Center,
            "Nearest" => Self::Nearest,
            "Custom" => Self::Custom((height / 100.0).clamp(0.0, 1.0)),
            _ => Self::Dynamic,
        }
    }
}

// A point of a box as shares of its size (0 to 1 each way), so it moves with the box.
pub(super) type Share = [f64; 3];

fn at(lo: V3, hi: V3, s: Share) -> V3 {
    [lo[0] + (hi[0] - lo[0]) * s[0], lo[1] + (hi[1] - lo[1]) * s[1], lo[2] + (hi[2] - lo[2]) * s[2]]
}

// The yaw and pitch from `eye` to `to` (yaw 0 faces +z).
pub(super) fn look_at(eye: V3, to: V3) -> (f32, f32) {
    let d = sub(to, eye);
    let yaw = (-d[0]).atan2(d[2]).to_degrees();
    let pitch = -d[1].atan2(d[0].hypot(d[2])).to_degrees();
    (yaw as f32, pitch as f32)
}

// The angle between two looks, degrees.
pub(super) fn angle(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dy = (b.0 - a.0 + 180.0).rem_euclid(360.0) - 180.0;
    dy.hypot(b.1 - a.1)
}

const XZ: [f64; 3] = [0.2, 0.5, 0.8];
const Y: [f64; 4] = [0.15, 0.4, 0.65, 0.9];
// A kept point stays unless another is this much better (degrees of score).
const KEEP: f64 = 2.0;
// Sight checks per plan, at most: the cheapest candidates first.
const SIGHT_CHECKS: usize = 6;

// Where to aim in the box `lo`..`hi` from `eye` looking `look`: a point within `reach` that `visible` says has a clear
// line, for the mode. Dynamic scores each candidate by the turn it needs plus a little for being off the middle (an
// edge point is lost first when the target moves), keeps `kept` while it is within KEEP of the best, and falls back to
// the middle. Returns the point and its share of the box (for keeping).
pub(super) fn plan_point(eye: V3, lo: V3, hi: V3, look: (f32, f32), mode: AimPoint, reach: f64, kept: Option<Share>,
    visible: &mut dyn FnMut(V3) -> bool) -> (V3, Share) {
    let centre: Share = [0.5, 0.5, 0.5];
    let nearest = || -> Share {
        let s = |i: usize| if hi[i] > lo[i] { ((eye[i].clamp(lo[i], hi[i]) - lo[i]) / (hi[i] - lo[i])).clamp(0.05, 0.95) } else { 0.5 };
        [s(0), s(1), s(2)]
    };
    let fixed = match mode {
        AimPoint::Center => Some(centre),
        AimPoint::Nearest => Some(nearest()),
        AimPoint::Custom(h) => Some([0.5, h.clamp(0.05, 0.95), 0.5]),
        AimPoint::Dynamic => None,
    };
    if let Some(s) = fixed {
        return (at(lo, hi, s), s);
    }
    let cost = |s: Share| -> f64 {
        let p = at(lo, hi, s);
        let turn = f64::from(angle(look, look_at(eye, p)));
        let off_middle = (s[0] - 0.5).abs() + (s[2] - 0.5).abs() + 0.5 * (s[1] - 0.5).abs();
        let far = (len(sub(p, eye)) - reach).max(0.0);
        turn + 4.0 * off_middle + 100.0 * far
    };
    let mut candidates: Vec<(f64, Share)> = Vec::with_capacity(XZ.len() * XZ.len() * Y.len() + 2);
    for &x in &XZ {
        for &z in &XZ {
            for &y in &Y {
                candidates.push((0.0, [x, y, z]));
            }
        }
    }
    // Level with your eyes (robust at close range), and the box's nearest point to you (best reach).
    let eye_level = if hi[1] > lo[1] { ((eye[1] - lo[1]) / (hi[1] - lo[1])).clamp(0.1, 0.9) } else { 0.5 };
    candidates.push((0.0, [0.5, eye_level, 0.5]));
    candidates.push((0.0, nearest()));
    for c in &mut candidates {
        c.0 = cost(c.1);
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let reachable = |s: Share| len(sub(at(lo, hi, s), eye)) <= reach;
    let mut best: Option<(f64, Share)> = None;
    for &(c, s) in candidates.iter().filter(|(_, s)| reachable(*s)).take(SIGHT_CHECKS) {
        if visible(at(lo, hi, s)) {
            best = Some((c, s));
            break;
        }
    }
    if let (Some(k), Some((best_cost, _))) = (kept, best)
        && reachable(k)
        && cost(k) <= best_cost + KEEP
        && visible(at(lo, hi, k))
    {
        return (at(lo, hi, k), k);
    }
    let s = best.map_or(centre, |b| b.1);
    (at(lo, hi, s), s)
}

// ---- turning

// Inside this many degrees of the point the look holds still (no shaking on the spot).
pub(super) const DEADZONE: f32 = 0.4;

// The next look from `from` toward `to`: Adaptive covers most of a large turn at once and less as it closes in (a
// steady approach, no overshoot), holds inside the deadzone, and lands exactly when `hit_due`, never past `limit`
// degrees a tick each way.
pub(super) fn adaptive_step(from: (f32, f32), to: (f32, f32), limit: (f32, f32), hit_due: bool) -> (f32, f32) {
    let dy = (to.0 - from.0 + 180.0).rem_euclid(360.0) - 180.0;
    let dp = to.1 - from.1;
    let error = dy.hypot(dp);
    if error <= DEADZONE && !hit_due {
        return from;
    }
    let share = if hit_due { 1.0 } else if error > 30.0 { 0.85 } else { 0.45 + 0.4 * (error / 30.0) };
    let yaw = (dy * share).clamp(-limit.0.max(1.0), limit.0.max(1.0));
    let pitch = (dp * share).clamp(-limit.1.max(1.0), limit.1.max(1.0));
    (from.0 + yaw, (from.1 + pitch).clamp(-90.0, 90.0))
}

// ---- targets

// Smart priority: the lower the better. Distance and the turn needed weigh most; a target still in hurt time (it can't
// take a full hit yet) or out of sight costs extra; the target already held is preferred, so it is not dropped for a
// near tie.
pub(super) fn smart_score(distance: f64, range: f64, turn: f32, hurt: bool, visible: bool, held: bool) -> f64 {
    distance / range.max(0.1) + f64::from(turn) / 90.0 + if hurt { 0.3 } else { 0.0 } + if visible { 0.0 } else { 0.5 }
        - if held { 0.35 } else { 0.0 }
}

// Switch's memory: the last few targets it sent a hit at, with the tick, so a fresh one is preferred over one just hit.
#[derive(Clone, Debug, Default)]
pub(super) struct Recent {
    hits: [(u32, u64); 4],
    len: usize,
}

impl Recent {
    pub(super) fn hit(&mut self, id: u32, tick: u64) {
        if let Some(i) = self.hits[..self.len].iter().position(|h| h.0 == id) {
            self.hits[i].1 = tick;
            return;
        }
        if self.len < self.hits.len() {
            self.len += 1;
        } else {
            self.hits.rotate_left(1);
        }
        self.hits[self.len - 1] = (id, tick);
    }

    // When `id` was last hit (None: not among the last few).
    pub(super) fn last(&self, id: u32) -> Option<u64> {
        self.hits[..self.len].iter().find(|h| h.0 == id).map(|h| h.1)
    }

    // Of `ids` (best first), the first not hit within `window` ticks; when all were, the one hit longest ago.
    pub(super) fn next(&self, ids: &[u32], now: u64, window: u64) -> Option<usize> {
        ids.iter().position(|&id| self.last(id).is_none_or(|t| now.saturating_sub(t) >= window))
            .or_else(|| (0..ids.len()).min_by_key(|&i| self.last(ids[i]).unwrap_or(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(track: &mut Track, path: &[V3]) {
        for &p in path {
            track.observe(1, p);
        }
    }

    #[test]
    fn a_still_target_is_predicted_where_it_stands() {
        let mut t = Track::default();
        run(&mut t, &[[0.0, 64.0, 0.0]; 5]);
        assert_eq!(t.predict(Prediction::Adaptive, 2.0, 1.0), [0.0, 64.0, 0.0]);
    }

    #[test]
    fn constant_movement_is_followed_ahead() {
        let mut t = Track::default();
        let path: Vec<V3> = (0..8).map(|i| [0.28 * f64::from(i), 64.0, 0.0]).collect();
        run(&mut t, &path);
        assert!(t.trust() > 0.9, "{}", t.trust());
        let p = t.predict(Prediction::Adaptive, 2.0, 1.0);
        assert!((p[0] - (0.28 * 7.0 + 0.56)).abs() < 0.05, "{p:?}");
    }

    #[test]
    fn a_strafe_reversal_drops_to_one_tick_without_acceleration() {
        let mut t = Track::default();
        run(&mut t, &[[0.0, 64.0, 0.0], [0.25, 64.0, 0.0], [0.5, 64.0, 0.0], [0.3, 64.0, 0.0]]);
        let p = t.predict(Prediction::Adaptive, 3.0, 1.0);
        // One tick of the new velocity (-0.2), not three, scaled by the (now lower) trust.
        assert!(p[0] < 0.3 && p[0] >= 0.1, "{p:?}");
    }

    #[test]
    fn knockback_and_erratic_movement_lower_trust_toward_the_current_position() {
        let mut t = Track::default();
        let jumpy = [[0.0, 64.0, 0.0], [0.6, 64.4, 0.0], [-0.4, 64.1, 0.5], [0.9, 64.8, -0.6], [-0.7, 64.0, 0.9], [1.0, 65.0, -1.0]];
        run(&mut t, &jumpy);
        assert!(t.trust() < 0.2, "{}", t.trust());
        let p = t.predict(Prediction::Adaptive, 3.0, 1.0);
        assert!(len(sub(p, [1.0, 65.0, -1.0])) < 0.5, "close to where it is: {p:?}");
        // A teleport starts afresh.
        t.observe(1, [50.0, 64.0, 50.0]);
        assert_eq!(t.predict(Prediction::Adaptive, 3.0, 1.0), [50.0, 64.0, 50.0]);
    }

    #[test]
    fn a_jump_keeps_rising_in_the_forecast() {
        let mut t = Track::default();
        run(&mut t, &[[0.0, 64.0, 0.0], [0.0, 64.0, 0.0], [0.0, 64.42, 0.0], [0.0, 64.75, 0.0]]);
        let p = t.predict(Prediction::Adaptive, 1.0, 1.0);
        assert!(p[1] > 64.75 && p[1] < 65.2, "{p:?}");
    }

    #[test]
    fn dynamic_aim_takes_the_reachable_visible_point_needing_least_turn_and_keeps_it() {
        let (lo, hi) = ([-0.3, 64.0, 2.7], [0.3, 65.8, 3.3]);
        let eye = [0.0, 65.62, 0.0];
        let look = look_at(eye, [0.0, 64.9, 3.0]);
        let mut all = |_: V3| true;
        let (p, s) = plan_point(eye, lo, hi, look, AimPoint::Dynamic, 3.0, None, &mut all);
        assert!(len(sub(p, eye)) <= 3.0, "within reach: {p:?}");
        // Kept next time when nothing better is far ahead.
        let (_, s2) = plan_point(eye, lo, hi, look_at(eye, p), AimPoint::Dynamic, 3.0, Some(s), &mut all);
        assert_eq!(s, s2);
        // The upper body hidden (a block in front of the head): a lower point is chosen.
        let mut low_only = |q: V3| q[1] < 64.8;
        let (p3, _) = plan_point(eye, lo, hi, look, AimPoint::Dynamic, 3.0, None, &mut low_only);
        assert!(p3[1] < 64.8, "{p3:?}");
        // Fixed modes.
        assert_eq!(plan_point(eye, lo, hi, look, AimPoint::Center, 3.0, None, &mut all).1, [0.5, 0.5, 0.5]);
        assert_eq!(plan_point(eye, lo, hi, look, AimPoint::Custom(0.8), 3.0, None, &mut all).1, [0.5, 0.8, 0.5]);
    }

    #[test]
    fn adaptive_turns_fast_settles_and_lands_when_due() {
        let limit = (180.0, 180.0);
        let mut look = (0.0f32, 0.0f32);
        let target = (90.0, 10.0);
        let mut steps = 0;
        while angle(look, target) > DEADZONE {
            look = adaptive_step(look, target, limit, false);
            steps += 1;
            assert!(steps < 30, "it converges");
        }
        assert!(steps < 15, "{steps}");
        // Inside the deadzone it holds; a due hit lands exactly.
        let near = (target.0 - 0.2, target.1);
        assert_eq!(adaptive_step(near, target, limit, false), near);
        assert_eq!(adaptive_step(near, target, limit, true), target);
        // Speed limits hold.
        let capped = adaptive_step((0.0, 0.0), (170.0, 0.0), (20.0, 20.0), true);
        assert_eq!(capped.0, 20.0);
    }

    #[test]
    fn switch_prefers_whom_it_has_not_just_hit() {
        let mut r = Recent::default();
        r.hit(1, 100);
        r.hit(2, 102);
        assert_eq!(r.next(&[1, 2, 3], 103, 10), Some(2), "3 is fresh");
        assert_eq!(r.next(&[1, 2], 103, 10), Some(0), "both hit: the one hit longest ago");
        for id in 3..10 {
            r.hit(id, 110 + u64::from(id));
        }
        assert_eq!(r.last(1), None, "bounded: the oldest are forgotten");
        // Smart priority keeps the held target over a near tie.
        assert!(smart_score(3.0, 6.0, 10.0, false, true, true) < smart_score(2.8, 6.0, 8.0, false, true, false));
    }
}
