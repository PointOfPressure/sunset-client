// baritone.api.utils.Rotation and the pure half of RotationUtils: an aim as a yaw and a pitch in degrees,
// the aim from one point to another, and the look direction of an aim. What needs the world (which block
// an aim hits) is asked of the player context.
const DEG_TO_RAD: f64 = std::f64::consts::PI / 180.0;
const RAD_TO_DEG: f64 = 180.0 / std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    pub fn new(yaw: f32, pitch: f32) -> Self {
        assert!(yaw.is_finite() && pitch.is_finite(), "{yaw} {pitch}");
        Self { yaw, pitch }
    }

    pub fn add(self, other: Rotation) -> Rotation {
        Rotation::new(self.yaw + other.yaw, self.pitch + other.pitch)
    }

    pub fn subtract(self, other: Rotation) -> Rotation {
        Rotation::new(self.yaw - other.yaw, self.pitch - other.pitch)
    }

    pub fn clamp(self) -> Rotation {
        Rotation::new(self.yaw, clamp_pitch(self.pitch))
    }

    pub fn normalize(self) -> Rotation {
        Rotation::new(normalize_yaw(self.yaw), self.pitch)
    }

    #[allow(dead_code)]
    pub fn normalize_and_clamp(self) -> Rotation {
        Rotation::new(normalize_yaw(self.yaw), clamp_pitch(self.pitch))
    }

    pub fn with_pitch(self, pitch: f32) -> Rotation {
        Rotation::new(self.yaw, pitch)
    }

    pub fn is_really_close_to(self, other: Rotation) -> bool {
        self.yaw_is_really_close(other) && (self.pitch - other.pitch).abs() < 0.01
    }

    pub fn yaw_is_really_close(self, other: Rotation) -> bool {
        let yaw_diff = (normalize_yaw(self.yaw) - normalize_yaw(other.yaw)).abs(); // you cant fool me
        !(0.01..=359.99).contains(&yaw_diff)
    }
}

pub fn clamp_pitch(pitch: f32) -> f32 {
    pitch.clamp(-90.0, 90.0)
}

/// Java's `%` on floats keeps the sign of the dividend, as Rust's does.
pub fn normalize_yaw(yaw: f32) -> f32 {
    let mut new_yaw = yaw % 360.0;
    if new_yaw < -180.0 {
        new_yaw += 360.0;
    }
    if new_yaw > 180.0 {
        new_yaw -= 360.0;
    }
    new_yaw
}

/// Rotation.yawDistanceFromOffset.
pub fn yaw_distance_from_offset(from: f32, to: f32) -> f32 {
    normalize_yaw(to - from)
}

/// RotationUtils.wrapAnglesToRelative.
pub fn wrap_angles_to_relative(current: Rotation, target: Rotation) -> Rotation {
    if current.yaw_is_really_close(target) {
        return Rotation::new(current.yaw, target.pitch);
    }
    target.subtract(current).normalize().add(current)
}

fn calc_rotation_from_vec3d_raw(orig: [f64; 3], dest: [f64; 3]) -> Rotation {
    let delta = [orig[0] - dest[0], orig[1] - dest[1], orig[2] - dest[2]];
    let yaw = delta[0].atan2(-delta[2]);
    let dist = (delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
    let pitch = delta[1].atan2(dist);
    Rotation::new((yaw * RAD_TO_DEG) as f32, (pitch * RAD_TO_DEG) as f32)
}

/// RotationUtils.calcRotationFromVec3d: the aim from `orig` to `dest`, turned the short way from `current`.
pub fn calc_rotation_from_vec3d(orig: [f64; 3], dest: [f64; 3], current: Rotation) -> Rotation {
    wrap_angles_to_relative(current, calc_rotation_from_vec3d_raw(orig, dest))
}

fn sin_table() -> &'static [f32; 65536] {
    static TABLE: std::sync::OnceLock<Box<[f32; 65536]>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = Box::new([0f32; 65536]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = (i as f64 * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32;
        }
        t
    })
}

/// `Mth.sin`: the game's lookup table, not the exact sine.
pub fn mth_sin(f: f32) -> f32 {
    sin_table()[((f * 10430.378f32) as i32 & 65535) as usize]
}

/// `Mth.cos`.
pub fn mth_cos(f: f32) -> f32 {
    sin_table()[((f * 10430.378f32 + 16384.0f32) as i32 & 65535) as usize]
}

/// RotationUtils.calcLookDirectionFromRotation.
pub fn calc_look_direction_from_rotation(r: Rotation) -> [f64; 3] {
    let pi = std::f32::consts::PI;
    let deg = DEG_TO_RAD as f32;
    let flat_z = mth_cos((-r.yaw * deg) - pi);
    let flat_x = mth_sin((-r.yaw * deg) - pi);
    let pitch_base = -mth_cos(-r.pitch * deg);
    let pitch_height = mth_sin(-r.pitch * deg);
    [f64::from(flat_x * pitch_base), f64::from(pitch_height), f64::from(flat_z * pitch_base)]
}

/// VecUtils.getBlockPosCenter.
pub fn block_pos_center(p: [i32; 3]) -> [f64; 3] {
    [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5]
}
