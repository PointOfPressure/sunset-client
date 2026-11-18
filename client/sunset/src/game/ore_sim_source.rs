// Sunset's Ore Sim, the part that replays the world generator's ore placement. On this Horizon build the generator's
// decoration (horizon_world::worldgen::generator_decoration) is a private module that function replacement can't
// reach, so the replay is not available: ore_sim says so and Ore Sim shows the message instead of boxes. The rest of
// the module (its settings, the view of the world, the drawing) is kept for when the generator exposes it again.

pub const KINDS: [&str; 10] = ["coal", "copper", "iron", "lapis", "redstone", "gold", "diamond", "emerald", "quartz", "debris"];

// What the replay reads of the world the player sees.
pub trait SeenWorld {
    // The block at a position, None where no chunk is loaded.
    fn block(&self, p: [i32; 3]) -> Option<horizon_world::registry::BlockStateId>;
    // The decoration biome (an index into the bundle's biome names) at a position.
    fn biome(&self, p: [i32; 3]) -> Option<u32>;
    // One above the highest solid block of a column (OCEAN_FLOOR_WG).
    fn floor(&self, x: i32, z: i32) -> Option<i32>;
    // The decoration biomes of the 3x3 chunks round this one, which choose the roots that run.
    fn biomes_near(&self) -> Vec<u32>;
}

// A dimension's ore roots, ready to replay: never made on this build.
pub enum OreSim {}

impl OreSim {
    pub fn biome_index(&self, _name: &str) -> Option<u32> {
        match *self {}
    }
    pub fn roots(&self) -> usize {
        match *self {}
    }
    pub fn chunk(&self, _seed: i64, _x: i32, _z: i32, _seen: &impl SeenWorld) -> Vec<([i32; 3], u8)> {
        match *self {}
    }
}

pub fn ore_sim(
    _data: &std::sync::Arc<horizon_world::worldgen::generator::GeneratorData>,
    _preset: &str,
    _dimension: &str,
    _build: (i32, i32),
    _generation: (i32, i32, i32),
) -> Result<Option<OreSim>, String> {
    Err("Ore Sim needs the world generator's ore placement, which this Horizon build keeps private".into())
}
