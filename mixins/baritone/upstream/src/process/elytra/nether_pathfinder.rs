// baritone.process.elytra.pathfinder.NetherPathfinder and PathSegment: a pathfinder and raytracer over a cache of
// one-bit-per-block chunks, for flying the nether by elytra. One instance is what the native library called a context: a
// world's seed and dimension, the chunks it has been given or has generated, and a search that can be cancelled. The
// table is behind a lock and the cancel flag is atomic, so a search can run on its own thread while the game thread
// inserts chunks and asks it to stop.
use super::baritone_elytra_chunk::{Chunk, Size, HEIGHT};
use super::baritone_elytra_generator::ChunkGeneratorHell;
use super::baritone_elytra_raytracer as raytracer;
use super::baritone_elytra_region::{self as region, RegionSource};
use super::baritone_elytra_search as search;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// What a ray or a search makes of a chunk the table does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheMiss {
    /// Generate it from the seed, and keep it.
    Generate,
    /// Treat it as all air.
    Air,
    /// Treat it as all solid.
    Solid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

/// A chunk in the table, where it is, and whether it came from the game or was made up.
pub struct Entry {
    pub chunk: Arc<Chunk>,
    pub x: i32,
    pub z: i32,
    /// True for a chunk the game gave; false for one generated from the seed or assumed to be air.
    pub from_caller: bool,
}

pub struct PathSegment {
    pub finished: bool,
    pub blocks: Vec<[i32; 3]>,
}

/// The table's key for chunk (x, z): the pair packed into a long, multiplied by an odd constant that spreads every bit of
/// both halves over the upper one. The key is not decoded anywhere: an entry knows its own position.
pub fn key(x: i32, z: i32) -> u64 {
    ((((i64::from(x)) << 32) | i64::from(z as u32)) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

fn in_bounds(y: i32) -> bool {
    (0..HEIGHT).contains(&y)
}

fn air_entry() -> Arc<Entry> {
    static AIR_ENTRY: OnceLock<Arc<Entry>> = OnceLock::new();
    AIR_ENTRY.get_or_init(|| Arc::new(Entry { chunk: Chunk::air(), x: 0, z: 0, from_caller: false })).clone()
}

pub struct NetherPathfinder {
    chunks: Mutex<HashMap<u64, Arc<Entry>>>,
    checked_regions: Mutex<HashSet<u64>>,
    cancel_flag: AtomicBool,
    generator: ChunkGeneratorHell,
    baritone_cache: Option<Arc<dyn RegionSource>>,
    #[allow(dead_code)]
    seed: i64,
    #[allow(dead_code)]
    dimension: Dimension,
    pub max_height: i32,
    /// The world y that the octrees start at, which the region files' heights are shifted by.
    min_y: i32,
}

impl NetherPathfinder {
    /// `seed` is the world seed, for generating terrain where no chunk is known; `baritone_cache` is Baritone's region
    /// files for the dimension, whose chunks a search loads as it reaches them; `dimension` decides the height of the
    /// region files; `max_height` is the height the search and rays stay under, 1 to 384.
    pub fn new(seed: i64, baritone_cache: Option<Arc<dyn RegionSource>>, dimension: Dimension, max_height: i32) -> Result<Self, String> {
        if max_height <= 0 || max_height > HEIGHT {
            return Err("Invalid max height (must be between 0 and 384)".into());
        }
        Ok(Self {
            chunks: Mutex::new(HashMap::new()),
            checked_regions: Mutex::new(HashSet::new()),
            cancel_flag: AtomicBool::new(false),
            generator: ChunkGeneratorHell::from_seed(seed),
            baritone_cache,
            seed,
            dimension,
            max_height,
            min_y: 0,
        })
    }

    pub fn with_min_y(mut self, min_y: i32) -> Self {
        self.min_y = min_y;
        self
    }

    #[allow(dead_code)]
    pub fn seed(&self) -> i64 {
        self.seed
    }

    #[allow(dead_code)]
    pub fn dimension(&self) -> Dimension {
        self.dimension
    }

    /// `close`: cancels a running search and forgets every chunk.
    pub fn close(&self) {
        self.cancel();
        self.chunks.lock().expect("chunk table").clear();
        self.checked_regions.lock().expect("region set").clear();
    }

    // ---- the chunk table ----

    /// Inserts a chunk from the game at (x, z), replacing any chunk there. The chunk is filled before it goes in, where
    /// the Java hands back an empty one to fill.
    pub fn insert_chunk(&self, x: i32, z: i32, chunk: Chunk) {
        self.chunks.lock().expect("chunk table").insert(key(x, z), Arc::new(Entry { chunk: Arc::new(chunk), x, z, from_caller: true }));
    }

    /// The chunk at (x, z), or the shared all-solid or all-air chunk if there is none. Do not write to the shared ones.
    pub fn get_chunk_or_default(&self, x: i32, z: i32, solid: bool) -> Arc<Chunk> {
        match self.chunks.lock().expect("chunk table").get(&key(x, z)) {
            Some(e) => e.chunk.clone(),
            None if solid => Chunk::solid(),
            None => Chunk::air(),
        }
    }

    /// The chunk at (x, z), or None.
    pub fn get_chunk(&self, x: i32, z: i32) -> Option<Arc<Chunk>> {
        self.chunks.lock().expect("chunk table").get(&key(x, z)).map(|e| e.chunk.clone())
    }

    pub fn has_chunk_from_caller(&self, x: i32, z: i32) -> bool {
        self.chunks.lock().expect("chunk table").get(&key(x, z)).is_some_and(|e| e.from_caller)
    }

    /// Whether the table holds chunk (x, z), whichever way it got there.
    pub fn has_chunk(&self, x: i32, z: i32) -> bool {
        self.chunks.lock().expect("chunk table").contains_key(&key(x, z))
    }

    /// Forgets every chunk more than `max_distance_blocks` (in whole chunks) from (chunk_x, chunk_z), and that the Baritone
    /// regions they were in have been read, so a search that comes back this way reads those regions again.
    pub fn cull_far_chunks(&self, chunk_x: i32, chunk_z: i32, max_distance_blocks: i32) {
        let dist_chunks = i64::from(max_distance_blocks / 16);
        let dist_sq = dist_chunks * dist_chunks;
        let mut culled_regions = HashSet::new();
        self.chunks.lock().expect("chunk table").retain(|_, entry| {
            let dx = i64::from(entry.x) - i64::from(chunk_x);
            let dz = i64::from(entry.z) - i64::from(chunk_z);
            if dx * dx + dz * dz > dist_sq {
                culled_regions.insert(key(entry.x >> 5, entry.z >> 5));
                return false;
            }
            true
        });
        let mut checked = self.checked_regions.lock().expect("region set");
        for region in culled_regions {
            checked.remove(&region);
        }
    }

    /// How many chunks the table holds.
    #[allow(dead_code)]
    pub fn chunk_count(&self) -> usize {
        self.chunks.lock().expect("chunk table").len()
    }

    // ---- searching ----

    /// A path from (x1, y1, z1) towards (x2, y2, z2), or None if none was found in time or the search was cancelled. The
    /// segment is finished if it reached the goal.
    ///
    /// `at_least_x4`: do not squeeze through cubes smaller than 4 blocks. `refine`: drop points that are visible from an
    /// earlier one. `fail_timeout_ms`: give up after this long with nothing; 0 for 30 seconds. `default_air_else_generate`:
    /// treat unknown chunks as air, instead of generating them from the seed. `fake_chunk_cost`: the cost of a step through
    /// a chunk that was not given by the game, against 1 for one that was.
    #[allow(clippy::too_many_arguments)]
    pub fn path_find(&self, from: [i32; 3], to: [i32; 3], at_least_x4: bool, refine: bool, fail_timeout_ms: u64, default_air_else_generate: bool, fake_chunk_cost: f64) -> Result<Option<PathSegment>, String> {
        if !in_bounds(from[1]) || !in_bounds(to[1]) {
            return Err("Invalid y1 or y2".into());
        }
        // The flag is cleared when the search ends, not when one starts, so a cancel that arrived while this call was
        // still waiting is honoured by the search it was aimed at and never leaks into the search after it.
        let result = (|| {
            let size = if at_least_x4 { Size::X4 } else { Size::X2 };
            let start = search::find_air(self, size, from[0], from[1], from[2], default_air_else_generate)?;
            let goal = search::find_air(self, size, to[0], to[1], to[2], default_air_else_generate)?;
            let Some(path) = search::find_path_segment(self, start, goal, at_least_x4, fail_timeout_ms, default_air_else_generate, fake_chunk_cost) else {
                return Ok(None);
            };
            let blocks = if refine { raytracer::refine(self, &path.blocks) } else { path.blocks };
            Ok(Some(PathSegment { finished: path.finished, blocks }))
        })();
        self.cancel_flag.store(false, Ordering::SeqCst);
        result
    }

    /// Asks a running search to stop. Returns whether it had already been asked.
    pub fn cancel(&self) -> bool {
        self.cancel_flag.swap(true, Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    pub fn clear_cancelled(&self) -> bool {
        self.cancel_flag.swap(false, Ordering::SeqCst)
    }

    // ---- for the search and the rays ----

    pub fn get_chunk_or_air(&self, cx: i32, cz: i32) -> Arc<Entry> {
        self.chunks.lock().expect("chunk table").get(&key(cx, cz)).cloned().unwrap_or_else(air_entry)
    }

    pub fn get_real_chunk_or_default(&self, cx: i32, cz: i32, solid: bool) -> Arc<Chunk> {
        match self.chunks.lock().expect("chunk table").get(&key(cx, cz)) {
            Some(e) if e.from_caller => e.chunk.clone(),
            _ if solid => Chunk::solid(),
            _ => Chunk::air(),
        }
    }

    /// The chunk at (cx, cz), generated from the seed and inserted if there is none.
    pub fn get_or_gen_chunk(&self, cx: i32, cz: i32) -> Arc<Chunk> {
        let k = key(cx, cz);
        if let Some(e) = self.chunks.lock().expect("chunk table").get(&k) {
            return e.chunk.clone();
        }
        // Generated without the lock: it is the slow part.
        let chunk = Arc::new(self.generator.generate_chunk(cx, cz));
        let mut chunks = self.chunks.lock().expect("chunk table");
        // Someone else may have generated this chunk while we were generating it.
        chunks.entry(k).or_insert_with(|| Arc::new(Entry { chunk, x: cx, z: cz, from_caller: false })).chunk.clone()
    }

    pub fn real_chunk_from_cache_or_fake_chunk_maybe_gen(&self, cx: i32, cz: i32, fake_chunk_mode: CacheMiss) -> Arc<Chunk> {
        if fake_chunk_mode == CacheMiss::Generate {
            return self.get_or_gen_chunk(cx, cz);
        }
        self.get_real_chunk_or_default(cx, cz, fake_chunk_mode == CacheMiss::Solid)
    }

    /// Loads the Baritone region holding chunk (cx, cz) the first time it is asked about. A chunk that is already in the
    /// table wins over the file's. Returns the milliseconds spent on the file.
    pub fn try_load_region(&self, cx: i32, cz: i32) -> u64 {
        let Some(source) = &self.baritone_cache else { return 0 };
        let (region_x, region_z) = (cx >> 5, cz >> 5);
        if !self.checked_regions.lock().expect("region set").insert(key(region_x, region_z)) {
            return 0;
        }
        let started = Instant::now();
        let read = region::load(source.as_ref(), region_x, region_z, self.min_y, &mut |x, z, chunk| {
            self.chunks.lock().expect("chunk table").entry(key(x, z)).or_insert_with(|| Arc::new(Entry { chunk: Arc::new(chunk), x, z, from_caller: true }));
        });
        if read { started.elapsed().as_millis() as u64 } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> NetherPathfinder {
        NetherPathfinder::new(1, None, Dimension::Nether, 128).unwrap()
    }

    #[test]
    fn the_table_inserts_replaces_and_culls() {
        let ctx = context();
        assert!(Arc::ptr_eq(&ctx.get_chunk_or_default(1, 1, false), &Chunk::air()));
        assert!(Arc::ptr_eq(&ctx.get_chunk_or_default(1, 1, true), &Chunk::solid()));
        assert!(ctx.get_chunk(1, 1).is_none());

        ctx.insert_chunk(1, 1, Chunk::new());
        let first = ctx.get_chunk(1, 1).unwrap();
        assert!(ctx.has_chunk_from_caller(1, 1));
        assert!(Arc::ptr_eq(&first, &ctx.get_chunk_or_default(1, 1, true)));
        assert!(Arc::ptr_eq(&first, &ctx.get_real_chunk_or_default(1, 1, true)));
        assert!(Arc::ptr_eq(&first, &ctx.get_chunk_or_air(1, 1).chunk));

        // Replacing keeps the new one.
        ctx.insert_chunk(1, 1, Chunk::new());
        assert!(!Arc::ptr_eq(&first, &ctx.get_chunk(1, 1).unwrap()));

        ctx.insert_chunk(40, 0, Chunk::new());
        ctx.insert_chunk(-40, 0, Chunk::new());
        assert_eq!(ctx.chunk_count(), 3);
        ctx.cull_far_chunks(0, 0, 200); // 12 chunks
        assert_eq!(ctx.chunk_count(), 1);
        assert!(ctx.get_chunk(1, 1).is_some());
        ctx.close();
        assert_eq!(ctx.chunk_count(), 0);
    }

    #[test]
    fn generated_chunks_are_fake() {
        let ctx = NetherPathfinder::new(146_008_555_100_680, None, Dimension::Nether, 128).unwrap();
        let chunk = ctx.get_or_gen_chunk(0, 0);
        assert!(Arc::ptr_eq(&chunk, &ctx.get_or_gen_chunk(0, 0)));
        assert!(!ctx.has_chunk_from_caller(0, 0));
        // A fake chunk is a default to the search's real-chunk lookup, and itself to the rest.
        assert!(Arc::ptr_eq(&ctx.get_real_chunk_or_default(0, 0, true), &Chunk::solid()));
        assert!(Arc::ptr_eq(&ctx.real_chunk_from_cache_or_fake_chunk_maybe_gen(0, 0, CacheMiss::Air), &Chunk::air()));
        assert!(Arc::ptr_eq(&ctx.real_chunk_from_cache_or_fake_chunk_maybe_gen(0, 0, CacheMiss::Generate), &chunk));
        assert!(Arc::ptr_eq(&ctx.get_chunk_or_air(0, 0).chunk, &chunk));
    }

    #[test]
    fn a_bad_height_is_refused() {
        assert!(NetherPathfinder::new(1, None, Dimension::Nether, 0).is_err());
        assert!(NetherPathfinder::new(1, None, Dimension::Nether, 385).is_err());
    }
}
