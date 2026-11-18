// baritone.behavior.look.ForkableRandom and LookBehavior's AimProcessor: where the player's aim really ends
// up when a movement asks for a rotation. The aim is a whole number of mouse steps from the one before, so the
// server sees what a mouse could have done, and a little noise is added every tick (randomLooking,
// randomLooking113).
use super::baritone_rotation::Rotation;

const DOUBLE_UNIT: f64 = 1.0 / 9_007_199_254_740_992.0; // 0x1.0p-53

/// Xoroshiro256++, as in the Java, with copies that share the state they were made at.
#[derive(Clone)]
pub struct ForkableRandom {
    s: [u64; 4],
}

impl ForkableRandom {
    pub fn new(seed: u64) -> Self {
        let mut state = seed;
        let mut splitmix64 = || {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        };
        Self { s: [splitmix64(), splitmix64(), splitmix64(), splitmix64()] }
    }

    pub fn next_double(&mut self) -> f64 {
        (self.next() >> 11) as f64 * DOUBLE_UNIT
    }

    pub fn next(&mut self) -> u64 {
        let result = self.s[0].wrapping_add(self.s[3]).rotate_left(23).wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    pub fn fork(&self) -> Self {
        self.clone()
    }
}

/// What the aim processor reads of the settings.
#[derive(Clone, Copy)]
pub struct AimSettings {
    pub random_looking: f64,
    pub random_looking_113: f64,
    /// The game's mouse sensitivity option, 0 to 1.
    pub sensitivity: f64,
}

impl AimSettings {
    pub fn of(settings: &super::baritone_settings::Settings, sensitivity: f64) -> Self {
        Self { random_looking: settings.float("randomLooking"), random_looking_113: settings.float("randomLooking113"), sensitivity }
    }
}

#[derive(Clone)]
pub struct AimProcessor {
    rand: ForkableRandom,
    random_yaw_offset: f64,
    random_pitch_offset: f64,
}

impl AimProcessor {
    pub fn new(seed: u64) -> Self {
        Self { rand: ForkableRandom::new(seed), random_yaw_offset: 0.0, random_pitch_offset: 0.0 }
    }

    /// `peekRotation`: where an aim at `rotation` would really be, `prev` being the rotation now.
    pub fn peek_rotation(&self, rotation: Rotation, prev: Rotation, s: &AimSettings) -> Rotation {
        let mut desired_yaw = rotation.yaw;
        let mut desired_pitch = rotation.pitch;
        // The target doesn't care about the pitch, so it used playerRotations().getPitch() and it's safe to
        // adjust it to a normal level.
        if desired_pitch == prev.pitch {
            desired_pitch = nudge_to_level(desired_pitch);
        }
        desired_yaw += self.random_yaw_offset as f32;
        desired_pitch += self.random_pitch_offset as f32;
        Rotation::new(calculate_mouse_move(prev.yaw, desired_yaw, s.sensitivity), calculate_mouse_move(prev.pitch, desired_pitch, s.sensitivity)).clamp()
    }

    /// `tick`: the noise of the next game tick.
    pub fn tick(&mut self, s: &AimSettings) {
        self.random_yaw_offset = (self.rand.next_double() - 0.5) * s.random_looking;
        self.random_pitch_offset = (self.rand.next_double() - 0.5) * s.random_looking;
        let mut random = self.rand.next_double() - 0.5;
        if random.abs() < 0.1 {
            random *= 4.0;
        }
        self.random_yaw_offset += random * s.random_looking_113;
    }

    /// `nextRotation`: the aim, then advance a tick.
    pub fn next_rotation(&mut self, rotation: Rotation, prev: Rotation, s: &AimSettings) -> Rotation {
        let actual = self.peek_rotation(rotation, prev, s);
        self.tick(s);
        actual
    }

    #[allow(dead_code)]
    pub fn advance(&mut self, ticks: i32, s: &AimSettings) {
        for _ in 0..ticks {
            self.tick(s);
        }
    }

    /// `fork`: an independent copy to look ahead with.
    pub fn fork(&self) -> Self {
        Self { rand: self.rand.fork(), random_yaw_offset: self.random_yaw_offset, random_pitch_offset: self.random_pitch_offset }
    }
}

/// Nudges the pitch to a regular level (between -20 and 10, in steps of 1).
fn nudge_to_level(pitch: f32) -> f32 {
    if pitch < -20.0 {
        pitch + 1.0
    } else if pitch > 10.0 {
        pitch - 1.0
    } else {
        pitch
    }
}

fn calculate_mouse_move(current: f32, target: f32, sensitivity: f64) -> f32 {
    let delta = target - current;
    let delta_px = angle_to_mouse(delta, sensitivity);
    current + mouse_to_angle(delta_px, sensitivity)
}

fn angle_to_mouse(angle_delta: f32, sensitivity: f64) -> f64 {
    let min_angle_change = mouse_to_angle(1.0, sensitivity);
    f64::from(angle_delta / min_angle_change).round()
}

fn mouse_to_angle(mouse_delta: f64, sensitivity: f64) -> f32 {
    // Casting float literals to double gets us the precise values used by minecraft.
    let f = sensitivity * f64::from(0.6_f32) + f64::from(0.2_f32);
    (mouse_delta * f * f * f * 8.0) as f32 * 0.15
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: AimSettings = AimSettings { random_looking: 0.0, random_looking_113: 0.0, sensitivity: 0.5 };

    #[test]
    fn the_aim_is_a_whole_number_of_mouse_steps_from_where_it_was() {
        let p = AimProcessor::new(1);
        let prev = Rotation::new(10.0, 5.0);
        let got = p.peek_rotation(Rotation::new(37.3, 5.0), prev, &S);
        let step = mouse_to_angle(1.0, 0.5);
        let steps = (got.yaw - prev.yaw) / step;
        assert!((steps - steps.round()).abs() < 1e-3, "{steps}");
        assert!((got.yaw - 37.3).abs() <= step, "{} {}", got.yaw, step);
    }

    #[test]
    fn a_pitch_that_is_left_alone_is_nudged_back_to_a_normal_level() {
        let p = AimProcessor::new(1);
        let prev = Rotation::new(0.0, 60.0);
        let got = p.peek_rotation(Rotation::new(0.0, 60.0), prev, &S);
        assert!(got.pitch < 60.0 && got.pitch > 58.0, "{}", got.pitch);
    }

    #[test]
    fn the_noise_changes_each_tick_and_a_fork_repeats_it() {
        let s = AimSettings { random_looking: 0.01, random_looking_113: 2.0, sensitivity: 0.5 };
        let mut p = AimProcessor::new(7);
        p.tick(&s);
        let a = p.random_yaw_offset;
        let mut f = p.fork();
        p.tick(&s);
        f.tick(&s);
        assert_ne!(a, p.random_yaw_offset);
        assert_eq!(p.random_yaw_offset, f.random_yaw_offset);
        assert!(p.random_yaw_offset.abs() < 2.1);
    }
}
