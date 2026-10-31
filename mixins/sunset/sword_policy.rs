// Original deterministic policy for a native controller; no world mutation.
pub const POPULATION_LIMIT: usize = 512;

#[derive(Default)]
pub struct Intent {
    pub yielded: bool,
}
impl Intent {
    // Explicit intent cannot be inferred from value equality: the user may
    // deliberately select the controller's current temporary value.
    pub fn editing(&mut self, own_module: bool, controlled_context: bool) -> bool {
        if own_module {
            self.yielded = false;
            true
        } else if controlled_context {
            self.yielded = true;
            true
        } else {
            false
        }
    }
    pub fn disabled(&mut self) {
        self.yielded = false;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Owned<T> {
    pub before: T,
    pub applied: T,
    pub yielded: bool,
}
impl<T: Clone + PartialEq> Owned<T> {
    pub fn new(before: T, applied: T) -> Self {
        Self {
            before,
            applied,
            yielded: false,
        }
    }
    pub fn update(&mut self, current: &mut T, next: T) -> bool {
        if self.yielded || *current != self.applied {
            self.yielded = true;
            return false;
        }
        *current = next.clone();
        self.applied = next;
        true
    }
    pub fn restore(self, current: &mut T) -> bool {
        if !self.yielded && *current == self.applied {
            *current = self.before;
            true
        } else {
            false
        }
    }
    // Project a copy for persistence without consuming or changing the live
    // lease. Values explicitly changed by the user must be serialized as-is.
    pub fn baseline(&self, current: &mut T) -> bool {
        if !self.yielded && *current == self.applied {
            *current = self.before.clone();
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub distance: f64,
    pub enemy_distance: f64,
    pub closing: f64,
    pub enemy_facing: f64,
    pub charge: f32,
    pub health: f32,
    pub eating: bool,
    pub hurt: bool,
    pub immune: bool,
    pub shielded: bool,
    pub grounded: bool,
    pub can_jump: bool,
    pub aggressive: bool,
    pub strafe: bool,
    pub jump_crits: bool,
    pub jump_reset: bool,
    pub eat_below: f64,
    pub side: i8,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decision {
    pub forward: i8,
    pub side: i8,
    pub sprint: bool,
    pub jump: bool,
    pub eat: bool,
    pub attack: bool,
    pub desired_gap: f64,
}

pub fn decide(v: View) -> Option<Decision> {
    if ![
        v.distance,
        v.enemy_distance,
        v.closing,
        v.enemy_facing,
        v.eat_below,
    ]
    .iter()
    .all(|x| x.is_finite())
        || !v.charge.is_finite()
        || !v.health.is_finite()
        || v.distance < 0.0
        || v.enemy_distance < 0.0
        || v.health <= 0.0
    {
        return None;
    }
    let mut d = Decision::default();
    d.eat = v.eating || v.health as f64 <= v.eat_below;
    let ready = v.charge >= 1.0;
    let risk = (v.closing.max(0.0) * 2.0).clamp(0.0, 0.6);
    d.desired_gap = if d.eat {
        5.0
    } else if ready && !v.shielded && !v.immune {
        2.75
    } else if v.aggressive {
        3.25 + risk
    } else {
        3.85 + risk
    };
    d.forward = if v.distance > d.desired_gap + 0.2 {
        1
    } else if v.enemy_distance < d.desired_gap - 0.2 {
        -1
    } else {
        0
    };
    // While retreating for food, never walk back into an opponent merely to
    // preserve a narrow distance band.
    if d.eat {
        d.forward = if v.enemy_distance < 5.5 { -1 } else { 0 };
    }
    if v.distance < 1.65 {
        d.forward = -1;
    }
    let threatened = v.enemy_distance < 3.25 && v.enemy_facing < 50.0;
    d.side = if v.strafe && v.distance < 6.0 && !d.eat {
        v.side.signum()
    } else {
        0
    };
    d.sprint = d.forward > 0 && !d.eat;
    d.jump = v.can_jump
        && v.grounded
        && !d.eat
        && ((v.jump_reset && v.hurt)
            || (v.jump_crits
                && !threatened
                && v.distance > 2.3
                && v.distance < 4.8
                && v.charge >= 0.3
                && v.charge < 0.85
                && (v.aggressive || v.enemy_facing > 90.0)));
    d.attack = ready && !d.eat && !v.immune && v.distance <= 3.0;
    Some(d)
}

pub fn support_safe(body_clear: bool, ground_distance: Option<f64>, hazard: bool) -> bool {
    body_clear && !hazard && ground_distance.is_some_and(|d| d.is_finite() && d >= 0.0 && d <= 1.25)
}
pub fn camera_step(current: [f32; 2], wanted: [f32; 2], speed: f64) -> Option<[f64; 2]> {
    if !current.iter().chain(wanted.iter()).all(|x| x.is_finite()) || !speed.is_finite() {
        return None;
    }
    let yaw = f64::from((wanted[0] - current[0] + 180.0).rem_euclid(360.0) - 180.0);
    let pitch = f64::from(wanted[1] - current[1]);
    let cap = 18.0 * speed.clamp(0.5, 1.5);
    let scale = (cap / yaw.hypot(pitch).max(cap)).min(1.0);
    Some([yaw * scale, pitch * scale])
}
