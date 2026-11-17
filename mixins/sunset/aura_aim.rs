// Kill Aura's aiming, apart from the game (aura.rs feeds it positions and line-of-sight answers):
// - `Track`: a bounded, latency-aware predictor of where the target will be, that checks its own one-tick forecasts
//   against where the target turned up and trusts itself less as they miss (knockback, reversals), down to none.
// - `plan_point`: where in the target's box to aim. Dynamic weighs points across the box by the turn they need, how
//   central (robust to movement) they are, reach and sight, and keeps the last point while it stays good.
// - `adaptive_step`: a turn that is fast when far off, settles near the point, holds still inside a deadzone, and lands
//   exactly when a hit is due.
// - `smart_score` and `Recent`: the composite target priority and Switch's memory of whom it hit.
// - `rotation_step`: the explosive auras' turn toward a target (Rust's rotation step).
// - Elytra Target's steering and firework timing (the section at the end).

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

// ---- the explosive auras' turn

// The look a tick turns toward a target (Rust's rotation step): a stride that grows with the distance and the speed, a
// little shorter at random, at most 25 degrees, in whole steps of the mouse.
pub fn rotation_step(base: [f32; 2], desired: [f32; 2], quantum: f64, speed: f64, random: f64) -> Option<[f32; 2]> {
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

// ---- Elytra Target
//
// Elytra Target's steering and firework timing, apart from the game (elytra_target.rs feeds it positions and answers):
// - `predicted`: where a gliding target will be a couple of ticks on (Kill Aura's `Track`, its last move carried on),
//   with an optional gravity drop; a target that is not gliding is aimed at where it is.
// - `aim_point`: the eyes or the middle of its box.
// - `hold_pitch`: inside Ideal distance and with no hit due, the look tips up so the glide bleeds speed (a climb slows
//   an elytra) and keeps its distance; with the hit due it goes straight at the target.
// - `step`: one tick's turn, never past Yaw/Pitch speed, cut to whole mouse counts, and no turn at all inside the
//   deadzone (an unchanged look is never sent again).
// - `firework_due`: a rocket when the target is beyond Firework distance and Firework delay has passed since the last.
// All steering is the look alone: the game's own glide physics turn it into motion, as a mouse would.

// Gravity's pull over the ticks ahead for Gravity drop, blocks a tick squared (an entity's 0.08, as a glide that is
// not lifting feels it).
const GRAVITY: f64 = 0.08;
// The most Ideal distance tips the look up, degrees, reached when the target is right beside you.
const HOLD_MAX: f32 = 30.0;
// Safe's climb: the look at least this far up while the next tick's box would hit a block.
pub(super) const SAFE_PITCH: f32 = -35.0;

#[derive(Clone, Copy, Debug)]
pub(super) struct Ahead {
    pub on: bool,
    pub ticks: f64,
    pub gliding_only: bool,
    pub gravity: bool,
}

// Where the target is expected `cfg.ticks` ahead, as a shift from where it is now.
pub(super) fn predicted(track: &Track, now: V3, gliding: bool, cfg: Ahead) -> V3 {
    if !cfg.on || (cfg.gliding_only && !gliding) {
        return [0.0; 3];
    }
    let ticks = cfg.ticks.clamp(0.0, 2.0);
    let to = track.predict(Prediction::Basic, ticks, ticks);
    let drop = if cfg.gravity { 0.5 * GRAVITY * ticks * ticks } else { 0.0 };
    [to[0] - now[0], to[1] - now[1] - drop, to[2] - now[2]]
}

// The eyes (high in the box, so a gliding body's low box too) or the middle of the box.
pub(super) fn aim_point(lo: V3, hi: V3, eyes: bool) -> V3 {
    let share = if eyes { 0.85 } else { 0.5 };
    [(lo[0] + hi[0]) / 2.0, lo[1] + (hi[1] - lo[1]) * share, (lo[2] + hi[2]) / 2.0]
}

// How far up (negative pitch) to tip the look this tick: none from Ideal distance out or with the hit due.
pub(super) fn hold_pitch(distance: f64, ideal: f64, close_in: bool) -> f32 {
    if close_in || ideal <= 0.0 || distance >= ideal {
        return 0.0;
    }
    -HOLD_MAX * ((ideal - distance) / ideal) as f32
}

// The look to aim for: at `point` from `eye`, tipped by `hold`, and up to Safe's climb when the next tick would collide.
pub(super) fn wanted(eye: V3, point: V3, hold: f32, colliding: bool) -> (f32, f32) {
    let (yaw, pitch) = look_at(eye, point);
    let pitch = (pitch + hold).clamp(-90.0, 90.0);
    (yaw, if colliding { pitch.min(SAFE_PITCH) } else { pitch })
}

fn wrap(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

// One tick's turn from `from` toward `to`: at most `cap` (yaw, pitch) degrees, in whole counts of `count` degrees
// (rounded toward `from`, so never past the cap), yaw continuous with `from` (no fold into ±180, no 360 snap), and
// `from` itself inside the deadzone.
pub(super) fn step(from: (f32, f32), to: (f32, f32), cap: (f32, f32), count: f32) -> (f32, f32) {
    let dy = wrap(to.0 - from.0);
    let dp = to.1 - from.1;
    if dy.hypot(dp) <= DEADZONE {
        return from;
    }
    let count = count.max(1e-4);
    let whole = |d: f32, cap: f32| (d.clamp(-cap.max(0.0), cap.max(0.0)) / count).trunc() * count;
    (from.0 + whole(dy, cap.0), (from.1 + whole(dp, cap.1)).clamp(-90.0, 90.0))
}

// A rocket now: the target beyond `distance`, `delay` ticks since the last one (or none yet), and nothing holding it
// back (`wait`: Kill Aura's hit about to go, with Sync).
pub(super) fn firework_due(target_distance: f64, distance: f64, now: u64, last: Option<u64>, delay: u64, wait: bool) -> bool {
    target_distance > distance && !wait && last.is_none_or(|t| now >= t + delay)
}

// With Sync with Kill Aura: while Kill Aura is engaged, a rocket goes only in the ticks just after its hit went (the
// tick it was planned in, the next one that sends it, then two free ones), never in a tick a hit could go in.
pub(super) fn sync_wait(engaged: bool, now: u64, last_hit: Option<u64>) -> bool {
    engaged && !last_hit.is_some_and(|t| now >= t + 2 && now <= t + 3)
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

#[cfg(test)]
mod elytra_tests {
    use super::*;

    fn glide(track: &mut Track, from: V3, vel: V3, ticks: u32) -> V3 {
        let mut p = from;
        for _ in 0..ticks {
            track.observe(7, p);
            p = [p[0] + vel[0], p[1] + vel[1], p[2] + vel[2]];
        }
        [p[0] - vel[0], p[1] - vel[1], p[2] - vel[2]]
    }

    const ON: Ahead = Ahead { on: true, ticks: 1.9, gliding_only: true, gravity: false };

    #[test]
    fn a_gliding_target_is_led_by_its_velocity_times_the_ticks() {
        let mut t = Track::default();
        let now = glide(&mut t, [0.0, 100.0, 0.0], [1.5, -0.2, 0.0], 5);
        let s = predicted(&t, now, true, ON);
        assert!((s[0] - 2.85).abs() < 1e-6 && (s[1] + 0.38).abs() < 1e-6 && s[2].abs() < 1e-9, "{s:?}");
    }

    #[test]
    fn a_target_not_gliding_or_prediction_off_is_aimed_where_it_is() {
        let mut t = Track::default();
        let now = glide(&mut t, [0.0, 100.0, 0.0], [0.3, 0.0, 0.0], 5);
        assert_eq!(predicted(&t, now, false, ON), [0.0; 3]);
        assert_eq!(predicted(&t, now, true, Ahead { on: false, ..ON }), [0.0; 3]);
        // Gliding targets only off: a walker is led too.
        let s = predicted(&t, now, false, Ahead { gliding_only: false, ..ON });
        assert!((s[0] - 0.57).abs() < 1e-6, "{s:?}");
    }

    #[test]
    fn gravity_drop_lowers_the_lead_and_ticks_are_capped_at_two() {
        let mut t = Track::default();
        let now = glide(&mut t, [0.0, 100.0, 0.0], [1.0, 0.0, 0.0], 5);
        let s = predicted(&t, now, true, Ahead { gravity: true, ticks: 2.0, ..ON });
        assert!((s[1] + 0.16).abs() < 1e-9 && (s[0] - 2.0).abs() < 1e-9, "{s:?}");
        let capped = predicted(&t, now, true, Ahead { ticks: 9.0, ..ON });
        assert!((capped[0] - 2.0).abs() < 1e-9, "{capped:?}");
    }

    #[test]
    fn eyes_sit_high_in_the_box_and_body_in_the_middle() {
        let (lo, hi) = ([0.0, 10.0, 0.0], [0.6, 11.8, 0.6]);
        let e = aim_point(lo, hi, true);
        let b = aim_point(lo, hi, false);
        assert!((e[1] - 11.53).abs() < 1e-9 && (b[1] - 10.9).abs() < 1e-9 && e[0] == 0.3 && b[2] == 0.3);
    }

    #[test]
    fn inside_ideal_distance_the_look_tips_up_unless_the_hit_is_due() {
        assert_eq!(hold_pitch(15.0, 10.0, false), 0.0);
        assert_eq!(hold_pitch(10.0, 10.0, false), 0.0);
        assert!((hold_pitch(5.0, 10.0, false) + 15.0).abs() < 1e-6);
        assert!((hold_pitch(0.0, 10.0, false) + HOLD_MAX).abs() < 1e-6);
        assert_eq!(hold_pitch(2.0, 10.0, true), 0.0);
        assert_eq!(hold_pitch(2.0, 0.0, false), 0.0);
    }

    #[test]
    fn safe_climbs_only_when_the_next_tick_collides() {
        let eye = [0.0, 100.0, 0.0];
        let level = wanted(eye, [0.0, 90.0, 10.0], 0.0, false);
        assert!(level.1 > 40.0 && level.0.abs() < 1e-4);
        assert_eq!(wanted(eye, [0.0, 90.0, 10.0], 0.0, true).1, SAFE_PITCH);
        // Already climbing harder than Safe's: kept.
        assert!(wanted(eye, [0.0, 120.0, 5.0], 0.0, true).1 < SAFE_PITCH);
        // The hold tips it up from the straight line.
        assert!((wanted(eye, [0.0, 100.0, 10.0], -12.0, false).1 + 12.0).abs() < 1e-4);
    }

    #[test]
    fn a_step_never_passes_the_caps_and_takes_the_short_way() {
        let count = 0.15;
        let s = step((170.0, 0.0), (-170.0, 80.0), (45.0, 35.0), count);
        // -170 is 20 degrees on from 170, the short way: yaw goes up past 180 (continuous), not back through 0.
        assert!(s.0 > 170.0 && s.0 - 170.0 <= 20.0 + 1e-3, "{s:?}");
        assert!(s.1 > 0.0 && s.1 <= 35.0 + 1e-4, "{s:?}");
        let big = step((0.0, 0.0), (179.0, -90.0), (45.0, 35.0), count);
        assert!(big.0 <= 45.0 + 1e-4 && big.0 > 44.0 && big.1 >= -35.0 - 1e-4, "{big:?}");
        // Whole mouse counts: each move is a multiple of the count.
        for d in [s.0 - 170.0, s.1, big.0, big.1] {
            let n = d / count;
            assert!((n - n.round()).abs() < 1e-2, "{d} is not whole counts");
        }
    }

    #[test]
    fn a_settled_look_does_not_move_so_no_duplicate_is_sent() {
        assert_eq!(step((10.0, 5.0), (10.2, 5.1), (45.0, 35.0), 0.15), (10.0, 5.0));
        assert_eq!(step((10.0, 5.0), (10.0, 5.0), (45.0, 35.0), 0.15), (10.0, 5.0));
        // Just outside the deadzone but under one count: no change either.
        let s = step((0.0, 0.0), (0.5, 0.0), (45.0, 35.0), 1.0);
        assert_eq!(s, (0.0, 0.0));
    }

    #[test]
    fn steering_converges_on_a_fixed_look_without_overshoot() {
        let mut look = (0.0_f32, 0.0_f32);
        let to = (135.0_f32, -40.0_f32);
        let mut moved = Vec::new();
        for _ in 0..12 {
            let next = step(look, to, (45.0, 35.0), 0.15);
            moved.push(wrap(next.0 - look.0).abs().max((next.1 - look.1).abs()));
            assert!(wrap(to.0 - next.0) >= -0.15 && next.1 >= to.1 - 0.15, "{next:?} passed {to:?}");
            look = next;
        }
        assert!(wrap(to.0 - look.0).abs() < 0.5 && (to.1 - look.1).abs() < 0.5, "{look:?}");
        // AimModulo360: never a yaw step anywhere near 320 degrees.
        assert!(moved.iter().all(|&d| d <= 45.0 + 1e-3));
    }

    #[test]
    fn fireworks_go_beyond_the_distance_after_the_delay() {
        assert!(firework_due(20.0, 16.0, 100, None, 10, false));
        assert!(!firework_due(12.0, 16.0, 100, None, 10, false));
        assert!(!firework_due(20.0, 16.0, 105, Some(100), 10, false));
        assert!(firework_due(20.0, 16.0, 110, Some(100), 10, false));
        assert!(!firework_due(20.0, 16.0, 110, Some(100), 10, true));
    }

    #[test]
    fn sync_lets_a_rocket_go_only_just_after_kill_auras_hit() {
        assert!(!sync_wait(false, 50, None));
        assert!(sync_wait(true, 50, None));
        assert!(sync_wait(true, 50, Some(49)));
        assert!(!sync_wait(true, 50, Some(48)));
        assert!(!sync_wait(true, 50, Some(47)));
        assert!(sync_wait(true, 50, Some(46)));
    }
}
