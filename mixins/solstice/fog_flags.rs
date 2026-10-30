// No Render's fog switches, read by the fog type hook.
use std::cell::Cell;
use std::sync::atomic::AtomicBool;

pub static NO_WATER: AtomicBool = AtomicBool::new(false);
pub static NO_LAVA: AtomicBool = AtomicBool::new(false);
thread_local! {
    pub static BUSY: Cell<bool> = const { Cell::new(false) };
}
pub fn busy() -> bool {
    BUSY.with(Cell::get)
}
pub fn set_busy(v: bool) {
    BUSY.with(|b| b.set(v));
}
