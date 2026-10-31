// Original controller policies. No inventory, damage or movement mutation.
pub const POPULATION_LIMIT: usize = 512;
pub const CRYSTAL_LIMIT: usize = 128;
pub const ANCHOR_CELLS: usize = 64;
pub const THREAT_PREDICTIONS: usize = 4;

#[derive(Default)]
pub struct Intent {
    pub yielded: bool,
}
impl Intent {
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
    // Persistence receives a baseline copy; the active controller keeps its
    // own applied values and its ownership until an actual user edit/yield.
    pub fn baseline(&self, current: &mut T) -> bool {
        if !self.yielded && *current == self.applied {
            *current = self.before.clone();
            true
        } else {
            false
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Search,
    Press,
    Guard,
    Retreat,
    Eat,
    Supply,
}
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub tick: u64,
    pub health: f64,
    pub absorption: f64,
    pub distance: Option<f64>,
    pub enemies_near: usize,
    pub totems: u32,
    pub empty_offhand: bool,
    pub refill_possible: bool,
    pub supplied: bool,
    pub food: bool,
    pub using_food: bool,
    pub eat_below: f64,
    pub threat: Option<f64>,
    pub threat_complete: bool,
    pub strafe: bool,
    pub side: i8,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    pub phase: Phase,
    pub forward: i8,
    pub side: i8,
    pub sprint: bool,
    pub auras: bool,
    pub eat: bool,
    pub ideal: f64,
}
#[derive(Default)]
pub struct Brain {
    pub retreat_until: u64,
    pub guard_until: u64,
    pop_last: Option<u64>,
    pops: u8,
    pub needs_food: bool,
}
impl Brain {
    // Only an actual own totem entity event may call this. Empty inventory or
    // predicted lethal damage must never masquerade as an observed activation.
    pub fn pop(&mut self, tick: u64) {
        if self
            .pop_last
            .is_none_or(|old| tick < old || tick.saturating_sub(old) > 600)
        {
            self.pops = 0;
        }
        self.pop_last = Some(tick);
        self.pops = self.pops.saturating_add(1);
        self.guard_until = tick.saturating_add(40);
        if self.pops >= 3 {
            self.retreat_until = tick.saturating_add(200);
            self.pops = 0;
        }
    }
    pub fn decide(&mut self, v: View) -> Option<Decision> {
        if ![v.health, v.absorption, v.eat_below]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0)
            || v.health <= 0.0
            || v.distance.is_some_and(|d| !d.is_finite() || d < 0.0)
            || v.threat.is_some_and(|d| !d.is_finite() || d < 0.0)
        {
            return None;
        }
        let life = v.health + v.absorption;
        if v.health <= v.eat_below && v.absorption < 4.0 {
            self.needs_food = true;
        }
        if v.health > v.eat_below + 4.0 || v.absorption >= 4.0 {
            self.needs_food = false;
        }
        let overwhelmed = v.totems == 0 && v.enemies_near >= 2;
        let dangerous = v.threat.is_none_or(|d| d >= life * 0.5) || life <= 6.0;
        if overwhelmed || dangerous {
            self.retreat_until = self.retreat_until.max(v.tick.saturating_add(30));
        }
        let refill = v.empty_offhand && v.refill_possible;
        let eat = self.needs_food
            && v.food
            && !refill
            && (v.using_food || v.distance.is_none_or(|d| d >= 6.0));
        let phase = if refill {
            Phase::Supply
        } else if eat {
            Phase::Eat
        } else if v.tick < self.retreat_until || self.needs_food {
            Phase::Retreat
        } else if v.distance.is_none() {
            Phase::Search
        } else if !v.supplied {
            Phase::Supply
        } else if v.tick < self.guard_until
            || !v.threat_complete
            || life <= 8.0
            || v.threat.is_some_and(|d| d >= life * 0.25)
        {
            Phase::Guard
        } else {
            Phase::Press
        };
        let ideal = match phase {
            Phase::Press => 3.3,
            Phase::Guard => 4.3,
            Phase::Retreat | Phase::Eat => 8.0,
            Phase::Supply => 6.0,
            Phase::Search => 0.0,
        };
        let forward = match (phase, v.distance) {
            (Phase::Search, _) | (_, None) => 0,
            (Phase::Eat | Phase::Retreat | Phase::Supply, Some(d)) => {
                if d < ideal {
                    -1
                } else {
                    0
                }
            }
            (_, Some(d)) => {
                if d > ideal + 0.3 {
                    1
                } else if d < ideal - 0.3 {
                    -1
                } else {
                    0
                }
            }
        };
        Some(Decision {
            phase,
            forward,
            side: if v.strafe
                && matches!(phase, Phase::Press | Phase::Guard)
                && v.distance.is_some_and(|d| d < 6.0)
            {
                v.side.signum()
            } else {
                0
            },
            sprint: forward > 0 && phase == Phase::Press,
            auras: matches!(phase, Phase::Press | Phase::Guard),
            eat,
            ideal,
        })
    }
}
pub fn camera_step(current: [f32; 2], wanted: [f32; 2], speed: f64) -> Option<[f64; 2]> {
    if !current.iter().chain(wanted.iter()).all(|x| x.is_finite()) || !speed.is_finite() {
        return None;
    }
    let yaw = f64::from((wanted[0] - current[0] + 180.0).rem_euclid(360.0) - 180.0);
    let pitch = f64::from(wanted[1] - current[1]);
    let cap = 18.0 * speed.clamp(0.5, 1.5);
    let scale = cap / yaw.hypot(pitch).max(cap);
    Some([yaw * scale, pitch * scale])
}
pub fn support_safe(body_clear: bool, ground_distance: Option<f64>, hazard: bool) -> bool {
    body_clear && !hazard && ground_distance.is_some_and(|d| d.is_finite() && d >= 0.0 && d <= 1.25)
}
