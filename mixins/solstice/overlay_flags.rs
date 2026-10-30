// No Render's overlay switches, read by the screen overlay hook.
use std::cell::Cell;
use std::sync::atomic::AtomicBool;

pub static NO_FIRE: AtomicBool = AtomicBool::new(false);
pub static NO_LIQUID: AtomicBool = AtomicBool::new(false);
pub static NO_WALL: AtomicBool = AtomicBool::new(false);
pub static NO_GUI: AtomicBool = AtomicBool::new(false);
thread_local! {
    pub static BUSY: Cell<bool> = const { Cell::new(false) };
}
pub fn busy() -> bool {
    BUSY.with(Cell::get)
}
pub fn set_busy(v: bool) {
    BUSY.with(|b| b.set(v));
}
