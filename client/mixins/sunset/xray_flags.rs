// Xray's switches, read by the mesher's hooks (on its worker threads) and set
// by the game. While a thread builds a section mesh, blocks that are not on
// the list (or at a remembered place) read as air.
use std::cell::Cell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

pub static ON: AtomicBool = AtomicBool::new(false);
pub static VISIBLE: LazyLock<RwLock<HashSet<u32>>> = LazyLock::new(|| RwLock::new(HashSet::new()));
// Blocks shown wherever they are, whatever they are: the ones the player placed (Show placed blocks).
pub static PLACED: LazyLock<RwLock<HashSet<[i32; 3]>>> = LazyLock::new(|| RwLock::new(HashSet::new()));
static ANY_PLACED: AtomicBool = AtomicBool::new(false);
thread_local! {
    pub static MESHING: Cell<bool> = const { Cell::new(false) };
    // The lowest corner of the section being meshed, in blocks.
    static ORIGIN: Cell<[i32; 3]> = const { Cell::new([0; 3]) };
}
pub fn meshing() -> bool {
    MESHING.with(Cell::get)
}
pub fn set_meshing(v: bool) {
    MESHING.with(|b| b.set(v));
}
pub fn set_origin(at: [i32; 3]) {
    ORIGIN.with(|o| o.set(at));
}
pub fn set_placed(at: HashSet<[i32; 3]>) {
    ANY_PLACED.store(!at.is_empty(), Ordering::Relaxed);
    if let Ok(mut set) = PLACED.write() {
        *set = at;
    }
}
fn placed(at: [i32; 3]) -> bool {
    ANY_PLACED.load(Ordering::Relaxed) && PLACED.read().is_ok_and(|set| set.contains(&at))
}
// One block state as the mesh builder sees it, at a place in the world.
pub fn filter(v: u32, at: [i32; 3]) -> u32 {
    if v == 0 || !ON.load(Ordering::Relaxed) || !meshing() {
        return v;
    }
    match VISIBLE.read() {
        Ok(set) if !set.contains(&v) && !placed(at) => 0,
        _ => v,
    }
}
// A whole section's states (the one being meshed).
pub fn filter_all(out: &mut [u32; 4096]) {
    if !ON.load(Ordering::Relaxed) || !meshing() {
        return;
    }
    let [x0, y0, z0] = ORIGIN.with(Cell::get);
    let any = ANY_PLACED.load(Ordering::Relaxed);
    let (Ok(set), Ok(places)) = (VISIBLE.read(), PLACED.read()) else {
        return;
    };
    for (i, v) in out.iter_mut().enumerate() {
        let (x, y, z) = (i as i32 & 15, i as i32 >> 8, i as i32 >> 4 & 15);
        if *v != 0 && !set.contains(v) && !(any && places.contains(&[x0 + x, y0 + y, z0 + z])) {
            *v = 0;
        }
    }
}
