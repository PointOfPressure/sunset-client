// The melee modules' rules that need no world (Rust's PR #16): when a borrowed hotbar slot goes back, distances to a
// box, whether a shield faces an attacker, the mace's fall bonus, attack speed, W Tap and Jump Reset's triggers.

// More living things than this nearby and the melee modules sit out the tick.
pub(super) const POPULATION_LIMIT: usize = 512;

// A hotbar slot a melee module took for a hit, and the one it goes back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SlotLease {
    pub owner: &'static str,
    pub previous: usize,
    pub selected: usize,
    pub until: u64,
    pub return_enabled: bool,
}

impl SlotLease {
    // The slot to go back to: never once you changed slot yourself.
    pub(super) fn restore(self, selected: usize, now: u64, still_owner: bool) -> Option<usize> {
        (still_owner && self.return_enabled && selected == self.selected && self.previous < 9 && now >= self.until)
            .then_some(self.previous)
    }
    pub(super) fn expired(self, selected: usize, now: u64, still_owner: bool) -> bool {
        !still_owner || selected != self.selected || now >= self.until
    }
}

// An item that is there and not broken.
pub(super) fn usable(count: u32, damage: u32, max_damage: u32) -> bool {
    count > 0 && (max_damage == 0 || damage < max_damage)
}

// From the eyes to the nearest point of a box.
pub(super) fn box_distance(eye: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> Option<f64> {
    if !eye.iter().chain(lo.iter()).chain(hi.iter()).all(|n| n.is_finite()) || (0..3).any(|i| lo[i] > hi[i]) {
        return None;
    }
    Some((0..3).map(|i| (eye[i] - eye[i].clamp(lo[i], hi[i])).powi(2)).sum::<f64>().sqrt())
}

// A raised shield facing the attacker within the shield's own angle (its blocks_attacks component).
pub(super) fn facing_attack(defender: [f64; 3], yaw: f32, attacker: [f64; 3], angle: f32) -> bool {
    if !defender.iter().chain(attacker.iter()).all(|n| n.is_finite()) || !yaw.is_finite() || !angle.is_finite() || angle < 0.0 {
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

// The mace's smash bonus for a fall: 4 a block for the first 3, 2 for the next 5, 1 after.
pub(super) fn smash_bonus(fall: f64) -> Option<f64> {
    if !fall.is_finite() || fall < 0.0 {
        return None;
    }
    Some(fall.min(3.0) * 4.0 + (fall - 3.0).clamp(0.0, 5.0) * 2.0 + (fall - 8.0).max(0.0))
}

// W Tap: a charged hit while you sprint forward at something alive.
pub(super) fn should_tap(sprinting: bool, forward: bool, strength: f32, living_target: bool) -> bool {
    sprinting && forward && living_target && strength.is_finite() && strength > 0.9
}

// Jump Reset: knockback that throws you up and sideways.
pub(super) fn jump_knockback(velocity: [f64; 3]) -> bool {
    velocity.iter().all(|n| n.is_finite()) && velocity[1] > 0.0 && velocity[0].hypot(velocity[2]) >= 0.1
}

// The attack speed a hand item would give: 4 plus the item's modifier, times the effects' multipliers.
pub(super) fn attack_speed(item_modifier: f64, total_modifiers: impl IntoIterator<Item = f64>) -> Option<f64> {
    if !item_modifier.is_finite() {
        return None;
    }
    let mut value = 4.0 + item_modifier;
    for modifier in total_modifiers {
        value *= 1.0 + modifier;
        if !value.is_finite() {
            return None;
        }
    }
    Some(value.clamp(0.0, 1024.0))
}

// A key press kept for a few ticks (Spear Swap's Lunge key).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Request {
    deadline: Option<u64>,
}

impl Request {
    pub(super) fn start(&mut self, now: u64, ticks: u64) {
        self.deadline = Some(now.saturating_add(ticks));
    }
    pub(super) fn active(&mut self, now: u64) -> bool {
        let active = self.deadline.is_some_and(|end| now <= end);
        if !active {
            self.clear();
        }
        active
    }
    pub(super) fn clear(&mut self) {
        self.deadline = None;
    }
}
