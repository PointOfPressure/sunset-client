// Clear Water's switches, read by the mesher's hook (on its worker threads)
// and set by the game. Every water sprite is laid out again at each lower
// opacity next to the block sprites, so fainter water is only another layer on
// its vertices: no texture reload, the world is just rebuilt.
use std::sync::RwLock;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

// Opacity steps: copies at 0/20, 1/20 ... 19/20 of the water's own alpha; STEPS is the water as it is.
pub const STEPS: usize = 20;
pub const SPRITES: [&str; 3] = ["minecraft:block/water_still", "minecraft:block/water_flow", "minecraft:block/water_overlay"];
pub static LEVEL: AtomicU32 = AtomicU32::new(STEPS as u32);
// How far the water's colour goes towards its brightest channel, in hundredths.
pub static TINT: AtomicU32 = AtomicU32::new(0);
// Each water sprite's own layer and its copies' layers, faintest first.
static LAYERS: RwLock<[(u32, [u32; STEPS]); 3]> = RwLock::new([(u32::MAX, [0; STEPS]); 3]);

pub fn on() -> bool {
    (LEVEL.load(Relaxed) as usize) < STEPS || TINT.load(Relaxed) > 0
}
pub fn set_layers(sprite: usize, own: u32, copies: [u32; STEPS]) {
    if let Ok(mut l) = LAYERS.write() {
        l[sprite] = (own, copies);
    }
}
// A sprite's frames with every pixel's alpha at step/STEPS of its own.
pub fn faded(frames: &[Vec<u8>], step: usize) -> Vec<Vec<u8>> {
    let scale = |a: u8| (u32::from(a) * step as u32 / STEPS as u32) as u8;
    frames.iter().map(|f| f.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], scale(p[3])]).collect()).collect()
}

// A water colour (0xRRGGBB) taken TINT hundredths of the way towards its brightest channel.
pub fn lighter(rgb: u32) -> u32 {
    let t = TINT.load(Relaxed).min(100);
    if t == 0 {
        return rgb;
    }
    let [_, r, g, b] = rgb.to_be_bytes();
    let top = r.max(g).max(b) as u32;
    let lift = |c: u8| (c as u32 + (top - c as u32) * t / 100) as u8;
    u32::from_be_bytes([(rgb >> 24) as u8, lift(r), lift(g), lift(b)])
}
