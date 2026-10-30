// Xray's switches, read by the mesher's hooks (on its worker threads) and set
// by the game. While a thread builds a section mesh, blocks that are not on
// the list read as air.
use std::cell::Cell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

pub static ON: AtomicBool = AtomicBool::new(false);
pub static VISIBLE: LazyLock<RwLock<HashSet<u32>>> = LazyLock::new(|| RwLock::new(HashSet::new()));
thread_local! {
    pub static MESHING: Cell<bool> = const { Cell::new(false) };
}
pub fn meshing() -> bool {
    MESHING.with(Cell::get)
}
pub fn set_meshing(v: bool) {
    MESHING.with(|b| b.set(v));
}
// One block state as the mesh builder sees it.
pub fn filter(v: u32) -> u32 {
    if v == 0 || !ON.load(Ordering::Relaxed) || !meshing() {
        return v;
    }
    match VISIBLE.read() {
        Ok(set) if !set.contains(&v) => 0,
        _ => v,
    }
}
// A whole section's states.
pub fn filter_all(out: &mut [u32; 4096]) {
    if !ON.load(Ordering::Relaxed) || !meshing() {
        return;
    }
    if let Ok(set) = VISIBLE.read() {
        for v in out.iter_mut() {
            if *v != 0 && !set.contains(v) {
                *v = 0;
            }
        }
    }
}
