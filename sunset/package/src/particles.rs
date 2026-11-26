// No Render's particle switches, read where particles are made (so a hidden one costs no work) and set by the game:
// every particle hidden, or the types listed, by their full names (minecraft:flame).
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

static ALL: AtomicBool = AtomicBool::new(false);
static ANY: AtomicBool = AtomicBool::new(false);
static HIDDEN: LazyLock<RwLock<HashSet<String>>> = LazyLock::new(|| RwLock::new(HashSet::new()));

// Whether a particle type, by its full name, is hidden.
pub fn hidden(name: &str) -> bool {
    ALL.load(Ordering::Relaxed) || ANY.load(Ordering::Relaxed) && HIDDEN.read().is_ok_and(|h| h.contains(name))
}

// Whether anything at all is hidden (so the particle code asks no further when nothing is).
pub fn any() -> bool {
    ALL.load(Ordering::Relaxed) || ANY.load(Ordering::Relaxed)
}

// All hidden, or the types named (full names; the game resolves what the player typed).
pub fn set(all: bool, names: Vec<String>) {
    ALL.store(all, Ordering::Relaxed);
    ANY.store(!names.is_empty(), Ordering::Relaxed);
    if let Ok(mut h) = HIDDEN.write() {
        *h = names.into_iter().collect();
    }
}
