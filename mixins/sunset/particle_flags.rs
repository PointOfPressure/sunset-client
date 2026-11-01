// No Render's particle switches, read where particles are made (so a hidden one costs no work) and set by the game:
// every particle hidden, or the types listed.
use super::ParticleType;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

static ALL: AtomicBool = AtomicBool::new(false);
static ANY: AtomicBool = AtomicBool::new(false);
static HIDDEN: RwLock<Vec<ParticleType>> = RwLock::new(Vec::new());

pub fn hidden(ty: ParticleType) -> bool {
    ALL.load(Ordering::Relaxed) || ANY.load(Ordering::Relaxed) && HIDDEN.read().is_ok_and(|h| h.contains(&ty))
}

// All hidden, or the types a list names (`flame, minecraft:explosion`).
pub fn set(all: bool, list: &str) {
    let types: Vec<ParticleType> = list.split(',').filter_map(|n| ParticleType::from_name(n.trim())).collect();
    ALL.store(all, Ordering::Relaxed);
    ANY.store(!types.is_empty(), Ordering::Relaxed);
    if let Ok(mut h) = HIDDEN.write() {
        *h = types;
    }
}

// Every particle type's name, for the picker.
pub fn names() -> Vec<&'static str> {
    ParticleType::ALL.iter().map(|t| t.name()).collect()
}
