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
// A section's water drawn with the copy at the chosen opacity and its tint lightened.
pub fn clear(mesh: &mut super::SectionMesh) {
    let (level, tint) = (LEVEL.load(Relaxed) as usize, TINT.load(Relaxed).min(100));
    let Ok(layers) = LAYERS.read() else {
        return;
    };
    for v in &mut mesh.translucent {
        let own = u32::from(v.layer & !super::OPAQUE_LAYER);
        let Some((_, copies)) = layers.iter().find(|(l, _)| *l == own) else {
            continue;
        };
        if level < STEPS {
            v.layer = v.layer & super::OPAQUE_LAYER | copies[level] as u16;
        }
        if tint > 0 {
            let rgb = [0, 8, 16].map(|s| v.color >> s & 255);
            let top = rgb.into_iter().max().unwrap_or(0);
            let lit = rgb.map(|c| c + (top - c) * tint / 100);
            v.color = v.color & 0xFF00_0000 | lit[0] | lit[1] << 8 | lit[2] << 16;
        }
    }
    // The renderer reads the layers a mesh uses from this list, not from its vertices.
    if level < STEPS {
        for (_, copies) in layers.iter() {
            if mesh.translucent.iter().any(|v| u32::from(v.layer & !super::OPAQUE_LAYER) == copies[level]) {
                mesh.note_sprite_layer(copies[level] as u16);
            }
        }
        mesh.finish_sprite_layers();
    }
}
