// Atmosphere's settings handed to the renderer crate, numbers only.
use std::sync::Mutex;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub enabled: bool,
    /// The game must positively identify a live Overworld before enabling the sky.
    pub world_supported: bool,
    /// 0 Space, 1 Aurora, 2 Ringed Planet.
    pub mode: u32,
    pub brightness: f32,
    pub strength: f32,
    pub stars: f32,
    /// Zero pauses the procedural animation.
    pub speed: f32,
    pub day_blend: f32,
    /// 0 Performance, 1 Balanced, 2 Detailed.
    pub quality: u32,
}

impl Default for Config {
    fn default() -> Self {
        DEFAULT
    }
}

const DEFAULT: Config = Config {
    enabled: false,
    world_supported: false,
    mode: 0,
    brightness: 1.0,
    strength: 1.0,
    stars: 1.0,
    speed: 1.0,
    day_blend: 0.65,
    quality: 1,
};

static CONFIG: Mutex<Config> = Mutex::new(DEFAULT);

fn finite(value: f32, fallback: f32, maximum: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, maximum)
    } else {
        fallback
    }
}

pub fn configure(mut config: Config) {
    config.mode = config.mode.min(2);
    config.quality = config.quality.min(2);
    config.brightness = finite(config.brightness, 1.0, 2.0);
    config.strength = finite(config.strength, 1.0, 2.0);
    config.stars = finite(config.stars, 1.0, 2.0);
    config.speed = finite(config.speed, 1.0, 2.0);
    config.day_blend = finite(config.day_blend, 0.65, 1.0);
    *CONFIG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = config;
}

pub(super) fn snapshot() -> Config {
    *CONFIG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
