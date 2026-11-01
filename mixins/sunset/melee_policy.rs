// Original, world-independent policies for native melee assistance.

pub(super) const POPULATION_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SlotLease {
    pub owner: &'static str,
    pub previous: usize,
    pub selected: usize,
    pub until: u64,
    pub return_enabled: bool,
}

impl SlotLease {
    // A user changing slots takes ownership immediately. Never switch them back
    // to a stale slot merely because an old timer elapsed.
    pub(super) fn restore(self, selected: usize, now: u64, still_owner: bool) -> Option<usize> {
        (still_owner
            && self.return_enabled
            && selected == self.selected
            && self.previous < 9
            && now >= self.until)
            .then_some(self.previous)
    }
    pub(super) fn expired(self, selected: usize, now: u64, still_owner: bool) -> bool {
        !still_owner || selected != self.selected || now >= self.until
    }
}

pub(super) fn usable(count: u32, damage: u32, max_damage: u32) -> bool {
    count > 0 && (max_damage == 0 || damage < max_damage)
}

pub(super) fn box_distance(eye: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> Option<f64> {
    if !eye
        .iter()
        .chain(lo.iter())
        .chain(hi.iter())
        .all(|n| n.is_finite())
        || (0..3).any(|i| lo[i] > hi[i])
    {
        return None;
    }
    Some(
        (0..3)
            .map(|i| (eye[i] - eye[i].clamp(lo[i], hi[i])).powi(2))
            .sum::<f64>()
            .sqrt(),
    )
}

// This is a current geometric test, not a promise that a server will keep a
// shield raised or accept a future hit. Custom shield angles are provided by
// the native item's blocks_attacks component.
pub(super) fn facing_attack(defender: [f64; 3], yaw: f32, attacker: [f64; 3], angle: f32) -> bool {
    if !defender
        .iter()
        .chain(attacker.iter())
        .all(|n| n.is_finite())
        || !yaw.is_finite()
        || !angle.is_finite()
        || angle < 0.0
    {
        return false;
    }
    let dx = attacker[0] - defender[0];
    let dz = attacker[2] - defender[2];
    let length = dx.hypot(dz);
    if length < 1.0e-4 {
        return angle >= 90.0;
    }
    let y = f64::from(yaw).to_radians();
    let dot = (-y.sin() * dx + y.cos() * dz) / length;
    dot.clamp(-1.0, 1.0).acos().to_degrees() <= f64::from(angle.min(180.0))
}

// The fall tiers are the native mace mechanic; no player state is changed to
// evaluate the candidates in the hotbar.
pub(super) fn smash_bonus(fall: f64) -> Option<f64> {
    if !fall.is_finite() || fall < 0.0 {
        return None;
    }
    let first = fall.min(3.0) * 4.0;
    let middle = (fall - 3.0).clamp(0.0, 5.0) * 2.0;
    let tail = (fall - 8.0).max(0.0);
    Some(first + middle + tail)
}



pub(super) fn should_tap(
    sprinting: bool,
    forward: bool,
    strength: f32,
    living_target: bool,
) -> bool {
    sprinting && forward && living_target && strength.is_finite() && strength > 0.9
}

pub(super) fn jump_knockback(velocity: [f64; 3]) -> bool {
    velocity.iter().all(|n| n.is_finite())
        && velocity[1] > 0.0
        && velocity[0].hypot(velocity[2]) >= 0.1
}





// Mirrors the native attack-speed attribute kernel for a proposed hand item:
// base4 plus the item's additive modifier, then active total multipliers.
pub(super) fn attack_speed(
    item_modifier: f64,
    total_modifiers: impl IntoIterator<Item = f64>,
) -> Option<f64> {
    if !item_modifier.is_finite() {
        return None;
    }
    let mut value = 4.0 + item_modifier;
    for modifier in total_modifiers {
        if !modifier.is_finite() {
            return None;
        }
        value *= 1.0 + modifier;
        if !value.is_finite() {
            return None;
        }
    }
    Some(value.clamp(0.0, 1024.0))
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Request {
    deadline: Option<u64>,
}
impl Request {
    pub(super) fn start(&mut self, now: u64, ticks: u64) {
        self.deadline = Some(now.saturating_add(ticks));
    }
    pub(super) fn active(&mut self, now: u64) -> bool {
        if self.deadline.is_some_and(|end| now <= end) {
            true
        } else {
            self.clear();
            false
        }
    }
    pub(super) fn clear(&mut self) {
        self.deadline = None;
    }
}
