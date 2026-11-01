// Original shared, fail-closed policy for explosive modules. No world mutation.

pub(super) fn in_range(from: [f64; 3], to: [f64; 3], range: f64) -> bool {
    if !range.is_finite() || range < 0.0 || !from.iter().chain(to.iter()).all(|x| x.is_finite()) {
        return false;
    }
    let squared = (0..3).map(|i| (from[i] - to[i]).powi(2)).sum::<f64>();
    squared.is_finite() && squared <= range * range
}

// Same sample locations as the pinned engine's ServerExplosion exposure grid.
// Refuse giant/invalid boxes rather than allocating or casting unbounded rays.
pub(super) fn samples(min: [f64; 3], max: [f64; 3]) -> Option<Vec<[f64; 3]>> {
    if !(0..3).all(|i| min[i].is_finite() && max[i].is_finite()
        && min[i].abs() <= 30_000_000.0 && max[i].abs() <= 30_000_000.0
        && max[i] >= min[i] && max[i] - min[i] <= 16.0) {
        return None;
    }
    let step: [f64; 3] = std::array::from_fn(|i| 1.0 / ((max[i] - min[i]) * 2.0 + 1.0));
    let offset = [(1.0 - (1.0 / step[0]).floor() * step[0]) / 2.0, 0.0,
        (1.0 - (1.0 / step[2]).floor() * step[2]) / 2.0];
    let mut result = Vec::with_capacity(64);
    let mut x = 0.0;
    while x <= 1.0 {
        let mut y = 0.0;
        while y <= 1.0 {
            let mut z = 0.0;
            while z <= 1.0 {
                if result.len() >= 256 { return None; }
                result.push([min[0] + (max[0] - min[0]) * x + offset[0],
                    min[1] + (max[1] - min[1]) * y,
                    min[2] + (max[2] - min[2]) * z + offset[2]]);
                z += step[2];
            }
            y += step[1];
        }
        x += step[0];
    }
    Some(result)
}

pub(super) fn raw_damage(distance: f64, power: f32, exposure: f32) -> Option<f32> {
    if !distance.is_finite() || distance < 0.0 || !power.is_finite() || power <= 0.0 || power > 8.0
        || !exposure.is_finite() || !(0.0..=1.0).contains(&exposure) { return None; }
    let diameter = f64::from(power * 2.0);
    let normalized = distance / diameter;
    if normalized > 1.0 { return Some(0.0); }
    let impact = (1.0 - normalized) * f64::from(exposure);
    // The pinned engine converts to f32, without integer rounding.
    Some(((impact * impact + impact) / 2.0 * 7.0 * diameter + 1.0) as f32)
}

// Player.hurtArmor wears every eligible piece by floor(damage / 4), at least
// one. Use the complete pre-mitigation bound and give no Unbreaking credit.
pub(super) fn armor_survives(damage: f32, current: u32, maximum: u32, wears: bool,
    known_components: bool, nonincreasing_wear: bool) -> bool {
    if !damage.is_finite() || damage < 0.0 || !known_components || !nonincreasing_wear { return false; }
    if damage == 0.0 || !wears || maximum == 0 { return true; }
    let wear = ((damage / 4.0) as u32).max(1);
    current.checked_add(wear).is_some_and(|after| after < maximum)
}

// A prediction reserves all exposure rays before any are cast. Both limits are
// absolute even if malformed dimensions/settings arrive from a server/config.
// Create once per aura tick and reuse for all candidates AND all friend checks.
#[derive(Clone, Debug)]
pub(super) struct Budget {
    limit: usize,
    pub predictions: usize,
    pub rays: usize,
    pub rejected: usize,
}

impl Budget {
    pub(super) fn new(max_predictions: usize) -> Self {
        Self { limit: max_predictions.min(64), predictions: 0, rays: 0, rejected: 0 }
    }

    pub(super) fn prediction(&mut self, rays: usize) -> bool {
        if self.predictions >= self.limit || rays > 256 || self.rays.saturating_add(rays) > 4096 {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        self.predictions += 1;
        self.rays += rays;
        true
    }

    pub(super) fn line(&mut self) -> bool {
        if self.rays >= 4096 {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        self.rays += 1;
        true
    }
}
