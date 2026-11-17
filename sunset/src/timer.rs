// Timer's multiplier (an f32's bits), read by the clock's hook.
use std::sync::atomic::{AtomicBool, AtomicU32};

pub static MULT: AtomicU32 = AtomicU32::new(0x3f80_0000);
pub static BUSY: AtomicBool = AtomicBool::new(false);
