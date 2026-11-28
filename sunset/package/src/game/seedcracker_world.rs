// Immutable decoded chunks for one SeedCracker run. Tables are built off the game thread, and no finder can
// follow a mutable store into a different world. The Level trait's static strings come only from registries.

use super::sunset_scx_dimension::Dimension;
use super::sunset_scx_level::Level;
use horizon_client::chunk::Chunk;
use ::sunset_core::lifecycle::RUNTIME;
use horizon_world::registry::{BlockRegistry, BlockStateId};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

pub(super) type At = (i32, i32);
pub(super) const MAX_CHUNKS: usize = 4096;
const MAX_STATES: usize = 262_144;
const MAX_BIOME_NAMES: usize = 8192;
const UNKNOWN: &str = "__sunset_unknown__";
const UNLOADED: &str = "__sunset_unloaded__";

fn bare(name: &str) -> &str { name.strip_prefix("minecraft:").unwrap_or(name) }

// Dynamic biome tables belong to a connection, whereas Level returns static strings. Intern only bounded
// registry identifiers, with a process-wide ceiling even across repeated server/resource-pack changes.
fn biome_name(name: &str) -> &'static str {
    static NAMES: LazyLock<Mutex<HashSet<&'static str>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
    let name = bare(name);
    if name.is_empty() || name.len() > 128
        || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_:/.-".contains(&b)) {
        return "the_void";
    }
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(found) = names.get(name) { return found; }
    if names.len() >= MAX_BIOME_NAMES { return "the_void"; }
    let name: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(name);
    name
}

pub(super) struct StateTable {
    names: Vec<&'static str>,
    properties: Vec<Box<[(&'static str, &'static str)]>>,
    tags: Vec<u8>,
    biomes: Vec<&'static str>,
}

impl StateTable {
    pub(super) fn build(registry: &'static BlockRegistry, biomes: &[String]) -> Self {
        let count = registry.len().min(MAX_STATES);
        let mut table = Self {
            names: Vec::with_capacity(count), properties: Vec::with_capacity(count),
            tags: Vec::with_capacity(count), biomes: biomes.iter().take(MAX_BIOME_NAMES).map(|s| biome_name(s)).collect(),
        };
        for id in 0..count {
            // The game's BlockRegistry is static, so borrowing these names/properties requires no leaking.
            if let Some(state) = registry.state(BlockStateId(id as u32)) {
                let name = bare(&state.name);
                table.names.push(name);
                table.properties.push(state.properties.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect());
                table.tags.push(tag_bits(name));
            } else {
                table.names.push(UNKNOWN);
                table.properties.push(Box::default());
                table.tags.push(0);
            }
        }
        table
    }
}

// These are the two tag predicates used by the inherited finders. Restrict the axe predicate to wood
// families: stone stairs/slabs and iron/copper doors cannot establish a shipwreck match.
fn tag_bits(name: &str) -> u8 {
    let logs = ["_log", "_wood", "_stem", "_hyphae"].iter().any(|s| name.ends_with(s));
    let wood = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak",
        "bamboo", "crimson", "warped"].iter().any(|w| name.strip_prefix(w).is_some_and(|s|
            ["_planks", "_stairs", "_slab", "_fence", "_fence_gate", "_door", "_trapdoor"].contains(&s)));
    u8::from(logs) | (u8::from(logs || wood || matches!(name, "chest" | "trapped_chest" | "barrel" | "bookshelf" | "crafting_table")) << 1)
}

pub(super) struct ChunkSnapshot {
    pub(super) revision: u64,
    pub(super) chunks: HashMap<At, Arc<Chunk>>,
}

pub(super) struct WorldView {
    chunks: Arc<ChunkSnapshot>,
    table: Arc<StateTable>,
    dimension: Dimension,
    cancelled: Arc<AtomicBool>,
    lifecycle: u64,
}

impl WorldView {
    pub(super) fn new(chunks: Arc<ChunkSnapshot>, table: Arc<StateTable>, dimension: Dimension,
        cancelled: Arc<AtomicBool>, lifecycle: u64) -> Self {
        Self { chunks, table, dimension, cancelled, lifecycle }
    }

    fn chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        self.chunks.chunks.get(&(x >> 4, z >> 4)).map(Arc::as_ref)
    }

    fn state(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let chunk = self.chunk(x, z)?;
        (chunk.min_y()..chunk.max_y()).contains(&y).then(|| chunk.get(x, y, z) as usize)
    }
}

impl Level for WorldView {
    fn cancelled(&self) -> bool { self.cancelled.load(Ordering::Acquire) || !RUNTIME.current(self.lifecycle) }
    fn dimension(&self) -> Dimension { self.dimension }

    fn block(&self, x: i32, y: i32, z: i32) -> &'static str {
        let Some(chunk) = self.chunk(x, z) else { return UNLOADED; };
        if !(chunk.min_y()..chunk.max_y()).contains(&y) { return "air"; }
        self.table.names.get(chunk.get(x, y, z) as usize).copied().unwrap_or(UNKNOWN)
    }

    fn property(&self, x: i32, y: i32, z: i32, name: &str) -> Option<&'static str> {
        self.table.properties.get(self.state(x, y, z)?)?.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
    }

    fn has_block_entity(&self, x: i32, y: i32, z: i32, kind: &str) -> bool {
        // Chunk arrival moves block-entity payloads into the live world's maps. The finders need only this
        // kind predicate, never container contents; a decoded chest/spawner block establishes its kind.
        self.is_chunk_loaded(x >> 4, z >> 4) && self.block(x, y, z) == kind
    }

    fn block_in_tag(&self, x: i32, y: i32, z: i32, tag: &str) -> bool {
        let Some(bits) = self.state(x, y, z).and_then(|s| self.table.tags.get(s)) else { return false; };
        match tag { "logs" => bits & 1 != 0, "mineable_with_axe" => bits & 2 != 0, _ => false }
    }

    fn noise_biome(&self, x: i32, y: i32, z: i32) -> &'static str {
        // Level's input is in quarts on all three axes; Chunk::biome takes block coordinates.
        self.biome(x.saturating_mul(4), y.saturating_mul(4), z.saturating_mul(4))
    }

    fn biome(&self, x: i32, y: i32, z: i32) -> &'static str {
        self.chunk(x, z).and_then(|c| self.table.biomes.get(usize::from(c.biome(x, y, z))))
            .copied().unwrap_or("the_void")
    }

    fn world_surface_height(&self, x: i32, z: i32) -> i32 {
        self.chunk(x, z).map_or(0, |c| c.world_surface(x & 15, z & 15, |id|
            self.table.names.get(id as usize).is_some_and(|n| matches!(*n, "air" | "cave_air" | "void_air"))))
    }

    fn is_chunk_loaded(&self, x: i32, z: i32) -> bool { self.chunks.chunks.contains_key(&(x, z)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Arc<StateTable> {
        Arc::new(StateTable { names: vec!["air", "sandstone", "chest"],
            properties: vec![Box::default(), Box::default(), vec![("facing", "east")].into_boxed_slice()],
            tags: vec![0, 0, 2], biomes: vec!["plains", "desert"] })
    }

    fn view(chunk: Chunk) -> WorldView {
        WorldView::new(Arc::new(ChunkSnapshot { revision: 1, chunks: HashMap::from([((chunk.x, chunk.z), Arc::new(chunk))]) }),
            table(), Dimension::Overworld, Arc::new(AtomicBool::new(false)), RUNTIME.token())
    }

    #[test]
    fn snapshot_does_not_follow_replaced_chunks_or_missing_neighbors() {
        let mut chunk = Chunk::empty(-1, 0);
        chunk.set(-1, 10, 0, 2);
        let old = view(chunk.clone());
        chunk.set(-1, 10, 0, 1);
        let new = view(chunk);
        assert_eq!(old.block(-1, 10, 0), "chest");
        assert_eq!(old.property(-1, 10, 0, "facing"), Some("east"));
        assert_eq!(new.block(-1, 10, 0), "sandstone");
        assert!(!old.is_chunk_loaded(0, 0));
        assert_ne!(old.block(0, 10, 0), "air");
        assert!(!old.has_block_entity(0, 10, 0, "chest"));
        assert_eq!(old.world_surface_height(-1, 0), 11);
    }

    #[test]
    fn quart_biomes_convert_vertical_coordinates_and_unknown_ids_stay_unknown() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.set_biome_cell(0, (8 - chunk.min_y()) >> 2, 0, 1);
        chunk.set(0, 9, 0, 999_999);
        let level = view(chunk);
        assert_eq!(level.noise_biome(0, 2, 0), "desert");
        assert_eq!(level.biome(0, 8, 0), "desert");
        assert_eq!(level.block(0, 9, 0), UNKNOWN);
        assert_eq!(level.biome(16, 8, 0), "the_void");
    }

    #[test]
    fn registry_identifiers_and_wood_tags_are_conservative() {
        assert_eq!(biome_name("minecraft:desert"), "desert");
        assert_eq!(biome_name("player supplied text"), "the_void");
        assert_ne!(tag_bits("oak_stairs") & 2, 0);
        assert_ne!(tag_bits("spruce_slab") & 2, 0);
        assert_eq!(tag_bits("stone_stairs"), 0);
        assert_eq!(tag_bits("iron_door"), 0);
    }
}
