// Sunset's Ore Sim, the part that needs decoration's own data: a dimension's ore roots (their placement, feature,
// step and step-local index) replayed for a chunk against the blocks a player sees, recording what each would set.
// The ores, counts, heights, sizes, air exposure and biome lists are the engine's (the worldgen bundle), not a table.

use super::{
    BlockStateId, Decoration, FeatureIdentity, FeaturePlacer, FeatureWorld, HeightmapType, PlacementContext,
    PlacementQueryError, WorldGenerationContext, WorldgenRandom, XoroshiroRandom,
};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

// The ore kinds, in the order Sunset's settings list them. A root's kind is the first whose word its name holds.
pub const KINDS: [&str; 10] = ["coal", "copper", "iron", "lapis", "redstone", "gold", "diamond", "emerald", "quartz", "debris"];

// What the replay reads of the world the player sees.
pub trait SeenWorld {
    // The block at a position, None where no chunk is loaded.
    fn block(&self, p: [i32; 3]) -> Option<BlockStateId>;
    // The decoration biome (an index into the bundle's biome names) at a position.
    fn biome(&self, p: [i32; 3]) -> Option<u32>;
    // One above the highest solid block of a column (OCEAN_FLOOR_WG).
    fn floor(&self, x: i32, z: i32) -> Option<i32>;
    // The decoration biomes of the 3x3 chunks round this one, which choose the roots that run.
    fn biomes_near(&self) -> Vec<u32>;
}

struct Root {
    identity: u32,
    index: i32,
    step: i32,
    kind: u8,
}

pub struct OreSim {
    decoration: Arc<Decoration>,
    roots: Vec<Root>,
    // An ore block as the player sees it, mapped to what it replaced when it generated (stone, deepslate,
    // netherrack), so the replay's rules match where the server already shows the ore.
    hosts: HashMap<BlockStateId, BlockStateId>,
    biome_names: Vec<Arc<str>>,
    // The level's build range (minimum, height) and the generator's range (WorldGenerationContext).
    build: (i32, i32),
    generation: WorldGenerationContext,
}

impl OreSim {
    pub(in crate::worldgen) fn new(
        decoration: Arc<Decoration>,
        registry: &crate::registry::BlockRegistry,
        biome_names: &[Arc<str>],
        build: (i32, i32),
        generation: WorldGenerationContext,
    ) -> Self {
        let roots = decoration
            .names
            .iter()
            .filter_map(|name| {
                let short = name.strip_prefix("minecraft:ore_")?;
                let kind = KINDS.iter().position(|k| short.contains(k))? as u8;
                let (identity, index, step) = decoration.seed(name)?;
                decoration.features[identity as usize].as_ref().ok()?;
                Some(Root { identity, index, step, kind })
            })
            .collect();
        let host = |name: &str| registry.default_state(name);
        let (stone, deepslate, netherrack) =
            (host("minecraft:stone"), host("minecraft:deepslate"), host("minecraft:netherrack"));
        let mut hosts = HashMap::new();
        for id in 0..registry.len() as u32 {
            let id = BlockStateId(id);
            let Some(state) = registry.state(id) else { continue };
            let name = state.name.as_str();
            if !(name.ends_with("_ore") || name == "minecraft:ancient_debris") {
                continue;
            }
            let to = if name.starts_with("minecraft:deepslate_") {
                deepslate
            } else if name.starts_with("minecraft:nether_") || name == "minecraft:ancient_debris" {
                netherrack
            } else {
                stone
            };
            if let Some(to) = to {
                hosts.insert(id, to);
            }
        }
        Self { decoration, roots, hosts, biome_names: biome_names.to_vec(), build, generation }
    }

    // The decoration biome index of a biome name, for SeenWorld::biome.
    pub fn biome_index(&self, name: &str) -> Option<u32> {
        self.biome_names.iter().position(|b| &**b == name).map(|i| i as u32)
    }

    // How many ore roots this dimension runs (none: nothing to show here).
    pub fn roots(&self) -> usize {
        self.roots.len()
    }

    // Every block the dimension's ore roots would set in chunk (x, z) of a world made from `seed`, with its kind
    // (an index into KINDS).
    pub fn chunk(&self, seed: i64, x: i32, z: i32, seen: &impl SeenWorld) -> Vec<([i32; 3], u8)> {
        let present: BTreeSet<u32> = seen.biomes_near().into_iter().collect();
        let origin = [x.wrapping_shl(4), self.build.0, z.wrapping_shl(4)];
        let mut random = WorldgenRandom::new(XoroshiroRandom::new(0));
        let decoration_seed = random.set_decoration_seed(seed, origin[0], origin[2]);
        let mut world = Replay { sim: self, seen, placed: HashMap::new(), kind: 0 };
        let mut placer = FeaturePlacer::new();
        for root in &self.roots {
            let id = FeatureIdentity(root.identity);
            if !present.iter().any(|b| self.decoration.biome_features.get(*b as usize).is_some_and(|f| f.contains(&id))) {
                continue;
            }
            let Ok(feature) = &self.decoration.features[root.identity as usize] else { continue };
            random.set_feature_seed(decoration_seed, root.index, root.step);
            world.kind = root.kind;
            let blocks = self.decoration.blocks;
            let _ = placer.place_with_biome_check(&feature.placement, &mut world, &mut random, origin, id, |w, r, p| {
                feature.feature.place(w, r, p, blocks)
            });
        }
        world.placed.into_iter().map(|(p, (_, kind))| (p, kind)).collect()
    }
}

// The world one chunk's replay runs on: the player's blocks under what the replay has set so far.
struct Replay<'a, S: SeenWorld> {
    sim: &'a OreSim,
    seen: &'a S,
    placed: HashMap<[i32; 3], (BlockStateId, u8)>,
    kind: u8,
}

impl<S: SeenWorld> Replay<'_, S> {
    fn read(&self, p: [i32; 3]) -> Option<BlockStateId> {
        if let Some((state, _)) = self.placed.get(&p) {
            return Some(*state);
        }
        let state = self.seen.block(p)?;
        Some(self.sim.hosts.get(&state).copied().unwrap_or(state))
    }

    fn set(&mut self, p: [i32; 3], state: BlockStateId) {
        self.placed.insert(p, (state, self.kind));
    }
}

impl<S: SeenWorld> FeatureWorld for Replay<'_, S> {
    type Error = ();
    fn block(&mut self, p: [i32; 3]) -> Result<BlockStateId, ()> {
        self.read(p).ok_or(())
    }
    fn set_block(&mut self, p: [i32; 3], state: BlockStateId, _: i32) -> Result<bool, ()> {
        self.set(p, state);
        Ok(true)
    }
}

impl<S: SeenWorld> PlacementContext for Replay<'_, S> {
    fn generation(&self) -> WorldGenerationContext {
        self.sim.generation
    }
    fn min_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(self.sim.build.0)
    }
    fn max_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(self.sim.build.0 + self.sim.build.1 - 1)
    }
    fn height(&mut self, _: HeightmapType, x: i32, z: i32) -> Result<i32, PlacementQueryError> {
        self.seen.floor(x, z).ok_or(PlacementQueryError::Unavailable)
    }
    fn biome_allows_feature(&mut self, p: [i32; 3], feature: FeatureIdentity) -> Result<bool, PlacementQueryError> {
        let biome = self.seen.biome(p).ok_or(PlacementQueryError::Unavailable)?;
        Ok(self.sim.decoration.biome_features.get(biome as usize).is_some_and(|f| f.contains(&feature)))
    }
    fn block_state(&mut self, p: [i32; 3]) -> Result<BlockStateId, PlacementQueryError> {
        self.read(p).ok_or(PlacementQueryError::Unavailable)
    }
    fn set_section_block(&mut self, p: [i32; 3], state: BlockStateId) -> Result<(), PlacementQueryError> {
        self.set(p, state);
        Ok(())
    }
}
