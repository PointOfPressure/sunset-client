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

// Kill Aura's Crits Wait (Rust's): no hit while you rise in a jump or jump from the ground, for up to 8 ticks, so it
// lands as you fall (a critical); sprinting in the air, the sprint stops first. A hit on the ground or already
// falling goes at once.
#[derive(Default, Debug)]
pub(super) struct CriticalWindow {
    waits: u8,
    tick: Option<u64>,
    cached: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Wait {
    pub hold: bool,
    pub drop_sprint: bool,
}

impl CriticalWindow {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
    pub(super) fn plan(&mut self, tick: u64, airborne: bool, sprinting: bool, falling: bool, ground_jump: bool) -> Wait {
        if airborne && sprinting {
            return Wait { hold: true, drop_sprint: true };
        }
        if (airborne && falling) || (!airborne && !ground_jump) {
            return Wait { hold: false, drop_sprint: false };
        }
        if self.tick != Some(tick) {
            self.cached = self.waits < 8;
            self.waits = self.waits.saturating_add(1);
            self.tick = Some(tick);
        }
        Wait { hold: self.cached, drop_sprint: airborne && self.cached }
    }
}

// How far a target moved last tick, sideways only, to aim where it will be; a jump of more than a block is a
// teleport and counts as none.
pub(super) fn stride(observed: [f64; 3]) -> [f64; 3] {
    if observed.iter().all(|v| v.is_finite()) && observed[0] * observed[0] + observed[2] * observed[2] <= 1.0 {
        [observed[0], 0.0, observed[2]]
    } else {
        [0.0; 3]
    }
}
