// baritone.cache.WorldProvider, WorldData and the game's side of ChunkPacker: the cached world of the world and
// dimension being played in, kept under the mod's data folder, fed with the chunks as they load and saved now and
// then. The Java does this on threads; here each tick packs the few chunks it has time for.
use super::baritone_block_state::{BlockState, Class};
use super::baritone_cached_world::{pack, CacheSettings, CachedChunk, CachedWorld, Dimension, PackSource, PathingBlockType, RegionStore};
use super::baritone_calculation_context::CalculationContext;
use super::baritone_movement_helper as mh;
use super::baritone_state_factory::StateFactory;
use super::Game;
use horizon_client::chunk::{Chunk, HEIGHT, MIN_Y};
use horizon_core::position::BlockPos;
use horizon_world::fluid::FluidState;
use horizon_world::fluid_flow::{flow_at, FluidFlowWorld};
use horizon_world::registry::BlockStateId;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// The region files of one world and dimension: `r.X.Z.bcr`, gzipped.
pub struct FileStore {
    pub dir: PathBuf,
}

impl FileStore {
    fn file(&self, rx: i32, rz: i32) -> PathBuf {
        self.dir.join(format!("r.{rx}.{rz}.bcr"))
    }
}

impl super::baritone_elytra_region::RegionSource for FileStore {
    fn read_region(&self, rx: i32, rz: i32) -> Option<Vec<u8>> {
        RegionStore::load(self, rx, rz)
    }

    fn layout(&self) -> super::baritone_elytra_region::Layout {
        super::baritone_elytra_region::Layout { height: horizon_client::chunk::HEIGHT as usize, min_y: horizon_client::chunk::MIN_Y }
    }
}

impl RegionStore for FileStore {
    fn load(&self, rx: i32, rz: i32) -> Option<Vec<u8>> {
        let compressed = std::fs::read(self.file(rx, rz)).ok()?;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&compressed[..]).read_to_end(&mut out).ok()?;
        Some(out)
    }

    fn save(&self, rx: i32, rz: i32, bytes: &[u8]) {
        let _ = std::fs::create_dir_all(&self.dir);
        let file = self.file(rx, rz);
        let temp = file.with_extension("bcr.tmp");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        if encoder.write_all(bytes).is_err() {
            return;
        }
        let Ok(compressed) = encoder.finish() else { return };
        if std::fs::write(&temp, compressed).is_ok() {
            let _ = std::fs::rename(&temp, &file);
        }
    }
}

/// The dimension a dimension id stands for.
pub fn dimension_of(id: &str) -> Dimension {
    let (min_y, height) = (horizon_client::chunk::MIN_Y, horizon_client::chunk::HEIGHT);
    match id {
        "minecraft:the_nether" => Dimension { min_y, height, logical_height: 128, has_ceiling: true, natural: false, ultra_warm: true, end: false },
        "minecraft:the_end" => Dimension { min_y, height, logical_height: 256, has_ceiling: false, natural: false, ultra_warm: false, end: true },
        _ => Dimension { min_y, height, logical_height: height, has_ceiling: false, natural: true, ultra_warm: false, end: false },
    }
}

/// The folder name of the world being played in: the server's address, or the single-player world's name.
pub fn world_name(game: &Game<'_>) -> String {
    match game.remote.as_ref() {
        Some(r) if r.session.address != "local" => sanitize(&r.session.address),
        // The integrated server holds the name of the world (the client's own `level` is empty then, as `state` shows).
        _ => match game.integrated.as_ref() {
            Some(server) => sanitize(&server.world_name),
            None => sanitize(&game.level.name),
        },
    }
}

/// `WorldData`: the cached world of one world and dimension.
pub struct WorldData {
    pub key: String,
    pub dir: PathBuf,
    pub cache: CachedWorld,
    seen: HashSet<(i32, i32)>,
    revision: u64,
    last_save: u64,
    last_requeue: u64,
}

/// `WorldProvider`.
#[derive(Default)]
pub struct WorldProvider {
    pub current: Option<WorldData>,
}

fn sanitize(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// A chunk of the game as the packer reads it.
struct LoadedChunk<'a> {
    game: &'a Game<'a>,
    calc: &'a CalculationContext<'a>,
    factory: &'a StateFactory,
    cx: i32,
    cz: i32,
    /// The chunk as it was when the server dropped it, once it is gone from the world.
    snapshot: Option<Arc<Chunk>>,
}

impl LoadedChunk<'_> {
    fn state(&self, x: i32, y: i32, z: i32) -> Rc<BlockState> {
        match &self.snapshot {
            Some(chunk) if (0..16).contains(&x) && (0..16).contains(&z) && (MIN_Y..MIN_Y + HEIGHT).contains(&y) => {
                self.factory.state(BlockStateId(chunk.get(x, y, z)))
            }
            Some(_) => self.factory.state(self.game.res.facts().air),
            None => self.calc.bsi.get0(self.cx * 16 + x, y, self.cz * 16 + z),
        }
    }

    /// The chunk the blocks are read from: the one in the world, or the snapshot.
    fn chunk(&self) -> Option<Arc<Chunk>> {
        self.snapshot.clone().or_else(|| self.game.world.chunks.get(&(self.cx, self.cz)).cloned())
    }
}

/// The world, as `FlowingFluid.getFlow` reads it: with the chunk being packed read from its snapshot when it has one.
struct FlowView<'a> {
    game: &'a Game<'a>,
    snapshot: Option<(i32, i32, Arc<Chunk>)>,
}

impl FlowView<'_> {
    /// The block at a position: from the snapshot inside its chunk, else from the world.
    fn at(&self, p: BlockPos) -> BlockStateId {
        if let Some((cx, cz, chunk)) = &self.snapshot {
            if p.x >> 4 == *cx && p.z >> 4 == *cz {
                return if (MIN_Y..MIN_Y + HEIGHT).contains(&p.y) { BlockStateId(chunk.get(p.x & 15, p.y, p.z & 15)) } else { self.game.res.facts().air };
            }
        }
        self.game.world.block(p.x, p.y, p.z)
    }
}

impl FlowView<'_> {
    fn flow(mut self, p: [i32; 3]) -> std::io::Result<horizon_core::geometry::Vec3> {
        let at = BlockPos::new(p[0], p[1], p[2]);
        let fluid = FluidFlowWorld::fluid(&mut self, at)?;
        flow_at(&mut self, fluid, at)
    }
}

impl FluidFlowWorld for FlowView<'_> {
    type Error = std::io::Error;
    type Block = BlockStateId;

    fn fluid(&mut self, p: BlockPos) -> std::io::Result<FluidState> {
        let state = self.game.pack.assets.registry.state(self.at(p));
        FluidState::from_block(state.ok_or_else(|| std::io::Error::other("a block state the registry does not know"))?)
    }

    fn block(&mut self, p: BlockPos) -> std::io::Result<BlockStateId> {
        Ok(self.at(p))
    }

    fn blocks_fluid_flow(&self, s: BlockStateId) -> bool {
        let assets = &self.game.pack.assets;
        assets.registry.state(s).is_some_and(|state| assets.blocks_fluid_flow.contains(&state.name))
    }

    fn is_ice_class(&self, s: BlockStateId) -> bool {
        self.game.res.facts().get(s).classes.iter().any(|c| c.as_ref() == "IceBlock")
    }

    fn is_face_sturdy(&mut self, s: BlockStateId, _: BlockPos, d: horizon_core::geometry::Direction) -> std::io::Result<bool> {
        Ok(self.game.res.facts().get(s).sturdy_full & (1 << d as u8) != 0)
    }
}

/// The blocks `ChunkPacker` calls air for a path: AirBlock, TallGrassBlock, DoublePlantBlock and FlowerBlock.
fn plant_like(name: &str) -> bool {
    matches!(
        name,
        "short_grass" | "fern" | "tall_grass" | "large_fern" | "sunflower" | "lilac" | "rose_bush" | "peony" | "tall_seagrass" | "pitcher_plant"
            | "dandelion" | "poppy" | "blue_orchid" | "allium" | "azure_bluet" | "red_tulip" | "orange_tulip" | "white_tulip" | "pink_tulip"
            | "oxeye_daisy" | "cornflower" | "lily_of_the_valley" | "wither_rose" | "torchflower" | "open_eyeblossom" | "closed_eyeblossom"
    )
}

impl PackSource for LoadedChunk<'_> {
    fn x(&self) -> i32 {
        self.cx
    }

    fn z(&self) -> i32 {
        self.cz
    }

    fn min_y(&self) -> i32 {
        horizon_client::chunk::MIN_Y
    }

    fn height(&self) -> i32 {
        horizon_client::chunk::HEIGHT
    }

    fn section_has_only_air(&self, section: i32) -> bool {
        let Some(chunk) = self.chunk() else { return true };
        let air = self.game.res.facts().air.0 as u32;
        chunk.sections.get(section as usize).is_none_or(|s| !s.blocks.maybe_has(|v| v != air))
    }

    fn block_name(&self, x: i32, y: i32, z: i32) -> Arc<str> {
        let s = self.state(x, y, z);
        Arc::from(&*s.name)
    }

    fn pathing_type(&self, x: i32, y: i32, z: i32) -> PathingBlockType {
        let state = self.state(x, y, z);
        if mh::is_water(&state) {
            // Only water source blocks are plausibly usable, flowing water should be avoid.
            if mh::possibly_flowing(&state) {
                return PathingBlockType::Avoid;
            }
            let flowing_at = |dx: i32, dz: i32| mh::possibly_flowing(&self.state(x + dx, y, z + dz));
            if (x != 15 && flowing_at(1, 0)) || (x != 0 && flowing_at(-1, 0)) || (z != 15 && flowing_at(0, 1)) || (z != 0 && flowing_at(0, -1)) {
                return PathingBlockType::Avoid;
            }
            if x == 0 || x == 15 || z == 0 || z == 15 {
                // `getFlow`, by the game's own function: at the border of the chunk only water that moves is water to a path.
                let flow = FlowView { game: self.game, snapshot: self.snapshot.clone().map(|c| (self.cx, self.cz, c)) }.flow([self.cx * 16 + x, y, self.cz * 16 + z]);
                return if flow.is_ok_and(|f| f.x != 0.0 || f.z != 0.0) { PathingBlockType::Water } else { PathingBlockType::Avoid };
            }
            return PathingBlockType::Water;
        }
        if mh::avoid_walking_into(self.calc, &state) || mh::is_bottom_slab(&state) {
            return PathingBlockType::Avoid;
        }
        // We used to do an AABB check here; this failed in the Nether near a fortress, because fences check their
        // adjacent blocks for their connection status, so the names decide.
        if state.is(Class::Air) || plant_like(state.bare_name()) {
            return PathingBlockType::Air;
        }
        PathingBlockType::Solid
    }
}

impl WorldProvider {
    /// The cache settings from the settings.
    pub fn cache_settings(s: &super::baritone_settings::Settings) -> CacheSettings {
        CacheSettings { chunk_caching: s.flag("chunkCaching"), prune_regions_from_ram: s.flag("pruneRegionsFromRAM"), cached_chunks_expiry_seconds: i64::from(s.int("cachedChunksExpirySeconds")) }
    }

    /// Switch to the world and dimension being played in (saving the one left), when it changed.
    pub fn follow(&mut self, game: &Game<'_>, settings: &super::baritone_settings::Settings) {
        let dimension = game.native_player().map_or("minecraft:overworld", |p| p.dimension);
        let world = world_name(game);
        let key = format!("{world}/{}", sanitize(dimension));
        if self.current.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        self.save(game, settings);
        let dir = game.native_data_dir(super::baritone::ID).join("cache").join(&world).join(sanitize(dimension));
        let cache = CachedWorld::new(dimension_of(dimension), Box::new(FileStore { dir: dir.clone() }));
        self.current = Some(WorldData { key, dir, cache, seen: HashSet::new(), revision: 0, last_save: game.ticks, last_requeue: game.ticks });
    }

    /// Queue the chunks that appeared since the last look, and re-queue the ones near the player now and then (the Java
    /// packs a chunk again as it unloads: `pack_unloaded` does, from what the unload hook kept).
    pub fn queue_chunks(&mut self, game: &Game<'_>) {
        let Some(data) = self.current.as_mut() else { return };
        let revision = game.world.chunks.membership_revision();
        if revision != data.revision {
            data.revision = revision;
            let loaded: HashSet<(i32, i32)> = game.world.chunks.iter().map(|(k, _)| *k).collect();
            for k in &loaded {
                if data.seen.insert(*k) {
                    data.cache.queue_for_packing(k.0, k.1);
                }
            }
            data.seen.retain(|k| loaded.contains(k));
        }
        if game.ticks.saturating_sub(data.last_requeue) > 20 * 60 * 3 {
            data.last_requeue = game.ticks;
            for k in game.world.chunks.iter().map(|(k, _)| *k) {
                data.cache.queue_for_packing(k.0, k.1);
            }
        }
    }

    /// `repack`: queue every loaded chunk within `range` chunks of the player. How many were queued.
    pub fn repack(&mut self, game: &Game<'_>, range: i32) -> usize {
        let Some(data) = self.current.as_mut() else { return 0 };
        let Some(p) = game.player.as_ref() else { return 0 };
        let (px, pz) = ((p.position.x.floor() as i32) >> 4, (p.position.z.floor() as i32) >> 4);
        let mut queued = 0;
        for x in px - range..=px + range {
            for z in pz - range..=pz + range {
                if game.world.chunks.contains_key(&(x, z)) {
                    data.cache.queue_for_packing(x, z);
                    queued += 1;
                }
            }
        }
        queued
    }

    /// The next few chunks to pack: loaded ones only. A chunk taken while more than `queue_max` wait behind it is dropped, not
    /// packed (`chunkPackerQueueMaxSize`, which the Java's packer thread does the same with).
    pub fn take_batch(&mut self, game: &Game<'_>, n: usize, queue_max: usize) -> Vec<(i32, i32)> {
        let Some(data) = self.current.as_mut() else { return Vec::new() };
        let mut batch = Vec::new();
        while batch.len() < n {
            let Some(k) = data.cache.next_to_pack() else { break };
            if data.cache.pending() > queue_max {
                continue;
            }
            if game.world.chunks.contains_key(&k) {
                batch.push(k);
            }
        }
        batch
    }

    /// Pack a batch for up to `budget_ms`: the chunks packed, and the ones that did not fit in the time.
    pub fn pack_batch(game: &Game<'_>, calc: &CalculationContext, factory: &StateFactory, batch: Vec<(i32, i32)>, budget_ms: u128) -> (Vec<CachedChunk>, Vec<(i32, i32)>) {
        let start = Instant::now();
        let now_ms = super::baritone_command::now_ms() as i64;
        let mut done = Vec::new();
        let mut left = Vec::new();
        for (cx, cz) in batch {
            if start.elapsed().as_millis() >= budget_ms {
                left.push((cx, cz));
                continue;
            }
            let source = LoadedChunk { game, calc, factory, cx, cz, snapshot: None };
            done.push(pack(&source, now_ms));
        }
        (done, left)
    }

    /// `onChunkEvent` before an unload: the chunks the server dropped, packed from what they were, for up to `budget_ms`. The
    /// ones that did not fit in the time are given back.
    pub fn pack_unloaded(game: &Game<'_>, calc: &CalculationContext, factory: &StateFactory, unloaded: Vec<(i32, i32, Arc<Chunk>)>, budget_ms: u128) -> (Vec<CachedChunk>, Vec<(i32, i32, Arc<Chunk>)>) {
        let start = Instant::now();
        let now_ms = super::baritone_command::now_ms() as i64;
        let mut done = Vec::new();
        let mut left = Vec::new();
        for (cx, cz, chunk) in unloaded {
            if start.elapsed().as_millis() >= budget_ms {
                left.push((cx, cz, chunk));
                continue;
            }
            let source = LoadedChunk { game, calc, factory, cx, cz, snapshot: Some(chunk) };
            done.push(pack(&source, now_ms));
        }
        (done, left)
    }

    /// Put packed chunks into the cache, and the ones left for later back in the queue.
    pub fn finish_batch(&mut self, done: Vec<CachedChunk>, left: Vec<(i32, i32)>) {
        let Some(data) = self.current.as_mut() else { return };
        for chunk in done {
            data.cache.update_cached_chunk(chunk);
        }
        for (x, z) in left {
            data.cache.queue_for_packing(x, z);
        }
    }

    /// `reloadAllFromDisk`.
    pub fn reload_all(&mut self) {
        if let Some(data) = self.current.as_mut() {
            data.cache.reload_all_from_disk();
        }
    }

    /// `save`, now and then (every ten minutes, as the Java's saver thread does after its first thirty seconds).
    pub fn save_if_due(&mut self, game: &Game<'_>, settings: &super::baritone_settings::Settings) {
        let due = self.current.as_ref().is_some_and(|c| game.ticks.saturating_sub(c.last_save) >= 20 * 60 * 10);
        if due {
            self.save(game, settings);
        }
    }

    pub fn save(&mut self, game: &Game<'_>, settings: &super::baritone_settings::Settings) {
        let Some(data) = self.current.as_mut() else { return };
        let center = game.player.as_ref().map_or([0, 0, 0], |p| [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32]);
        data.cache.save(&Self::cache_settings(settings), super::baritone_command::now_ms() as i64, center);
        data.last_save = game.ticks;
    }
}
