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
