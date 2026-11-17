// Hand View's switches that the hands' code below the game reads (the sway, the totem pop and the equip drop), set by
// the game (crate::game::sunset_hand_view, which keeps the rest).
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU32};

pub static NO_SWAY: AtomicBool = AtomicBool::new(false);
pub static TOTEM: RwLock<Option<TotemPop>> = RwLock::new(None);
pub static TOTEM_EXTRA_TICKS: AtomicU32 = AtomicU32::new(0);
pub static NO_EQUIP_DROP: AtomicBool = AtomicBool::new(false);
// Hand View's Animations: 1.8 (else 1.9+).
pub static LEGACY: AtomicBool = AtomicBool::new(false);

// The totem pop's size, and where it flies on the screen (fractions of half the screen, right and up).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TotemPop {
    pub scale: f32,
    pub x: f32,
    pub y: f32,
}

// The hand pass's field of view is 70 degrees: half the screen's height, one block out, is tan(35).
pub const HALF_SCREEN: f32 = 0.700_207_5;

// The totem pop's move on the screen for an item this far out: right and up, in blocks.
pub fn totem_shift(pop: TotemPop, depth: f32, aspect: f32) -> [f32; 2] {
    [pop.x * HALF_SCREEN * aspect * depth, pop.y * HALF_SCREEN * depth]
}
