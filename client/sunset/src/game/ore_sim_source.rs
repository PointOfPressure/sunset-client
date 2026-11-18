// Sunset's Ore Sim, the part that replays the world generator's ore placement: the dimension's ore roots (their
// placement, feature and seeds are the engine's, from horizon_world::worldgen::decoration_replay, Sunset's fork patch
// 0001) replayed for a chunk against the blocks the player sees, recording what each would set.

use horizon_world::registry::BlockStateId;
use horizon_world::worldgen::decoration_replay::{DecorationReplay, ReplayView};
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

// A dimension's ore roots, ready to replay for any seed.
pub struct OreSim {
    replay: DecorationReplay,
    // (identity, kind) of each ore root that runs on this build.
    roots: Vec<(u32, u8)>,
    // An ore block as the player sees it, mapped to what it replaced when it generated (stone, deepslate,
    // netherrack), so the replay's rules match where the server already shows the ore.
    hosts: Arc<HashMap<BlockStateId, BlockStateId>>,
}

// The player's world with ores read as their hosts.
struct Seen<'a, S: SeenWorld> {
    seen: &'a S,
    hosts: &'a HashMap<BlockStateId, BlockStateId>,
}

impl<S: SeenWorld> ReplayView for Seen<'_, S> {
    fn block(&self, p: [i32; 3]) -> Option<BlockStateId> {
        let state = self.seen.block(p)?;
        Some(self.hosts.get(&state).copied().unwrap_or(state))
    }
    fn biome(&self, p: [i32; 3]) -> Option<u32> {
        self.seen.biome(p)
    }
    fn floor(&self, x: i32, z: i32) -> Option<i32> {
        self.seen.floor(x, z)
    }
}

impl OreSim {
    // The decoration biome index of a biome name, for SeenWorld::biome.
    pub fn biome_index(&self, name: &str) -> Option<u32> {
        self.replay.biome_names().iter().position(|b| &**b == name).map(|i| i as u32)
    }

    // How many ore roots this dimension runs (none: nothing to show here).
    pub fn roots(&self) -> usize {
        self.roots.len()
    }

    // Every block the dimension's ore roots would set in chunk (x, z) of a world made from `seed`, with its kind
    // (an index into KINDS).
    pub fn chunk(&self, seed: i64, x: i32, z: i32, seen: &impl SeenWorld) -> Vec<([i32; 3], u8)> {
        let present: BTreeSet<u32> = seen.biomes_near().into_iter().collect();
        let roots: Vec<(u32, u8)> = self
            .roots
            .iter()
            .copied()
            .filter(|(id, _)| present.iter().any(|b| self.replay.biome_has(*b, *id)))
            .collect();
        let ids: Vec<u32> = roots.iter().map(|r| r.0).collect();
        let mut placed = HashMap::new();
        let view = Seen { seen, hosts: &self.hosts };
        self.replay.replay(seed, x, z, &ids, &view, &mut |root, p, _| {
            placed.insert(p, roots[root].1);
        });
        placed.into_iter().collect()
    }
}

// The ore roots of `preset`'s `dimension`, ready to replay for any seed: `build` is the level's build range
// (minimum, height), `generation` the generator's (minimum, height, sea level). None when the bundle has no
// features for it.
pub fn ore_sim(
    data: &Arc<horizon_world::worldgen::generator::GeneratorData>,
    preset: &str,
    dimension: &str,
    build: (i32, i32),
    generation: (i32, i32, i32),
) -> Result<Option<OreSim>, String> {
    let Some(replay) = DecorationReplay::new(data, preset, dimension, build, generation)? else {
        return Ok(None);
    };
    let roots = replay
        .features()
        .iter()
        .enumerate()
        .filter_map(|(id, name)| {
            let short = name.strip_prefix("minecraft:ore_")?;
            let kind = KINDS.iter().position(|k| short.contains(k))? as u8;
            replay.runs(id as u32).then_some((id as u32, kind))
        })
        .collect();
    let registry = data.registry();
    let host = |name: &str| registry.default_state(name);
    let (stone, deepslate, netherrack) = (host("minecraft:stone"), host("minecraft:deepslate"), host("minecraft:netherrack"));
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
    Ok(Some(OreSim { replay, roots, hosts: Arc::new(hosts) }))
}
