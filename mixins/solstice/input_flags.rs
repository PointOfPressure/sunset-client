// No Slow's switch, read by the movement input hook.
use std::sync::atomic::AtomicBool;

pub static NO_SLOW: AtomicBool = AtomicBool::new(false);
