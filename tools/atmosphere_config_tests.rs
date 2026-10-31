#[path = "../mixins/sunset/atmosphere_flags.rs"]
mod flags;
use flags::{Config, configure, snapshot};
use std::sync::Mutex;
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn invalid_public_inputs_cannot_become_gpu_specialization_values() {
    let _guard = TEST_LOCK.lock().unwrap();
    configure(Config {
        enabled: false, world_supported: false, mode: u32::MAX, quality: u32::MAX,
        brightness: f32::NAN, strength: f32::INFINITY, stars: -100.0,
        speed: 1000.0, day_blend: f32::NEG_INFINITY,
    });
    let got = snapshot();
    assert!(!got.enabled && !got.world_supported);
    assert_eq!((got.mode, got.quality), (2,2));
    assert_eq!((got.brightness, got.strength, got.stars, got.speed, got.day_blend), (1.0,1.0,0.0,2.0,0.65));
}

#[test]
fn zero_speed_and_day_strength_and_normal_quality_survive_the_bridge() {
    let _guard = TEST_LOCK.lock().unwrap();
    let initial = Config::default();
    assert!(!initial.enabled && !initial.world_supported);
    for quality in 0..=2 {
        configure(Config { enabled:true, world_supported:true, mode:1, quality,
            speed:0.0, day_blend:0.0, stars:2.0, brightness:0.2, strength:2.0 });
        let got = snapshot();
        assert!(got.enabled && got.world_supported);
        assert_eq!((got.speed,got.day_blend,got.stars,got.quality), (0.0,0.0,2.0,quality));
    }
}
