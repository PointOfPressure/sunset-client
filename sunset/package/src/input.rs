// No Slow's switch, read by the movement input hook.
use std::sync::atomic::{AtomicBool, AtomicU32};

pub static NO_SLOW: AtomicBool = AtomicBool::new(false);
pub static NO_SLOW_FACTOR: AtomicU32 = AtomicU32::new(1.0_f32.to_bits());
