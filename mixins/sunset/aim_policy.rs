// Original native aim controller. The reference's observable controls and
// timing are recorded in docs/combat-parity.md; mouse granularity comes from
// Horizon, not a second sensitivity implementation.

#[derive(Clone, Copy, Debug)]
pub(super) struct Candidate {
    pub id: u32,
    pub distance: f64,
    pub angle: f64,
}

pub(super) fn choose(candidates: &[Candidate], range: f64, fov: f64,
    recent: Option<u32>, kept: Option<u32>) -> Option<u32>
{
    if !range.is_finite() || !fov.is_finite() || range <= 0.0 || fov <= 0.0 {
        return None;
    }
    let eligible = |c: &&Candidate, extra: f64, cone: f64| c.distance.is_finite()
        && c.distance >= 0.0 && c.distance <= range + extra
        && c.angle.is_finite() && c.angle >= 0.0 && c.angle <= fov * 0.5 * cone;
    if let Some(c) = candidates.iter().find(|c| Some(c.id) == recent && eligible(c, 0.0, 1.0)) {
        return Some(c.id);
    }
    if let Some(c) = candidates.iter().find(|c| Some(c.id) == kept && eligible(c, 0.5, 1.3)) {
        return Some(c.id);
    }
    let mut best = None;
    for c in candidates.iter().filter(|c| eligible(c, 0.0, 1.0)) {
        if best.is_none_or(|old: &Candidate| c.angle <= old.angle) { best = Some(c); }
    }
    best.map(|c| c.id)
}

#[derive(Debug)]
pub(super) struct Easing {
    target: Option<u32>,
    pace: f64,
    random: u64,
    input_at: Option<u64>,
    direction: [f64; 2],
    remainder: [f64; 2],
}
impl Default for Easing {
    fn default() -> Self {
        Self { target: None, pace: 1.0, random: 0x4d595df4d0f33173,
            input_at: None, direction: [0.0; 2], remainder: [0.0; 2] }
    }
}
impl Easing {
    pub(super) fn follow(&mut self, target: Option<u32>, entropy: u64) {
        if self.target == target { return; }
        self.target = target;
        self.remainder = [0.0; 2];
        self.random ^= entropy.rotate_left(17);
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.pace = 0.85 + ((self.random >> 11) as f64 / ((1u64 << 53) as f64)) * 0.3;
    }
    pub(super) fn reset(&mut self) {
        self.target = None;
        self.input_at = None;
        self.direction = [0.0; 2];
        self.remainder = [0.0; 2];
    }
    /// Returns added degrees, quantized to whole native mouse counts.
    pub(super) fn nudge(&mut self, now_ms: u64, user: [f64; 2], error: Option<[f64; 2]>,
        seconds: f64, speed: f64, vertical: bool, quantum: f64) -> [f64; 2]
    {
        if self.input_at.is_some_and(|at| now_ms < at) { self.reset(); }
        if !user.iter().all(|v| v.is_finite()) || !seconds.is_finite() || seconds < 0.0
            || !speed.is_finite() || speed < 0.0 || !quantum.is_finite() || quantum <= 1e-9 {
            self.remainder = [0.0; 2];
            return [0.0; 2];
        }
        for axis in 0..2 {
            if user[axis] != 0.0 {
                self.input_at = Some(now_ms);
                self.direction[axis] = user[axis].signum();
            }
        }
        let active = self.target.is_some()
            && self.input_at.is_some_and(|at| now_ms.saturating_sub(at) <= 150);
        let Some(error) = error.filter(|e| active && e.iter().all(|v| v.is_finite())) else {
            self.direction = [0.0; 2];
            self.remainder = [0.0; 2];
            return [0.0; 2];
        };
        let dt = seconds.min(0.05);
        let gain = -(-12.0 * dt).exp_m1();
        let limit = speed * self.pace * dt;
        let mut output = [0.0; 2];
        for axis in 0..2 {
            let allowed = self.direction[axis] * error[axis] >= 0.0 && (axis == 0 || vertical);
            let change = if allowed { error[axis] * gain * if axis == 1 { 0.6 } else { 1.0 } } else { 0.0 };
            self.remainder[axis] += change.clamp(-limit, limit) / quantum;
            let whole = self.remainder[axis].trunc();
            self.remainder[axis] -= whole;
            output[axis] = whole * quantum;
        }
        output
    }
}
