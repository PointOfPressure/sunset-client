// baritone.process.elytra.NetherPathfinderContext and BlockStateOctreeInterface: the pathfinder as the elytra behaviour uses
// it. It shifts y by the world's lowest block (the octrees start at 0), turns a search into a job for a worker thread, and
// answers "is this block passable". The Java keeps a read/write lock around its mutable chunks, and two executors for
// packing and searching. A chunk here is complete before it is inserted and a block update replaces the chunk with a
// changed copy, so searches and rays hold immutable snapshots and need no lock: nothing ever waits for a search to end.
use super::baritone_elytra_chunk::{Chunk, HEIGHT};
use super::baritone_elytra_pathfinder::{CacheMiss, Dimension, NetherPathfinder};
use super::baritone_elytra_raytracer::raytrace;
use super::baritone_elytra_region::RegionSource;
use super::baritone_elytra_segment::{PathCalculationError, UnpackedSegment};
use std::cell::RefCell;
use std::sync::mpsc;
use std::sync::Arc;

type Pos = [i32; 3];

pub struct NetherPathfinderContext {
    npf: Arc<NetherPathfinder>,
    min_y: i32,
    max_height: i32,
    seed: i64,
}

/// What `pathFindAsync` reads of the settings and the dimension at the moment of the call.
#[derive(Clone, Copy)]
pub struct SearchOptions {
    /// `elytraPredictTerrain` in the nether: unknown chunks are generated from the seed, not taken as air.
    pub predict_terrain: bool,
    /// `elytraAllowTightSpaces`.
    pub allow_tight_spaces: bool,
}

impl NetherPathfinderContext {
    /// `world_height` is the dimension's height in blocks; `min_y` its lowest block. `allow_above_roof` is
    /// `elytraAllowAboveRoof`, which in the nether lifts the 128 block limit.
    pub fn new(seed: i64, cache: Option<Arc<dyn RegionSource>>, dimension: Dimension, min_y: i32, world_height: i32, allow_above_roof: bool) -> Result<Self, String> {
        let mut height = world_height.min(HEIGHT);
        if !allow_above_roof && dimension == Dimension::Nether {
            height = height.min(128);
        }
        let npf = NetherPathfinder::new(seed, cache, dimension, height)?.with_min_y(min_y);
        Ok(Self { npf: Arc::new(npf), min_y, max_height: height, seed })
    }

    #[allow(dead_code)]
    pub fn pathfinder(&self) -> &Arc<NetherPathfinder> {
        &self.npf
    }

    #[allow(dead_code)]
    pub fn min_y(&self) -> i32 {
        self.min_y
    }

    pub fn max_height(&self) -> i32 {
        self.max_height
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    /// `hasChunk`: the pathfinder holds the chunk, and the game gave it.
    pub fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.npf.has_chunk_from_caller(chunk_x, chunk_z)
    }

    /// `queueCacheCulling`.
    pub fn queue_cache_culling(&self, chunk_x: i32, chunk_z: i32, max_distance_blocks: i32) {
        self.npf.cull_far_chunks(chunk_x, chunk_z, max_distance_blocks);
    }

    /// `queueForPacking`, after the packing: the chunk the game gave, replacing the one before.
    pub fn insert_chunk(&self, chunk_x: i32, chunk_z: i32, chunk: Chunk) {
        self.npf.insert_chunk(chunk_x, chunk_z, chunk);
    }

    /// `queueBlockUpdate`: blocks of one chunk changed, as (x, y, z, is it anything but air). The chunk is replaced by a
    /// changed copy.
    pub fn update_blocks(&self, chunk_x: i32, chunk_z: i32, changes: &[(i32, i32, i32, bool)]) {
        let Some(old) = self.npf.get_chunk(chunk_x, chunk_z) else { return }; // this shouldn't ever happen
        let mut chunk = (*old).clone();
        for &(x, y, z, solid) in changes {
            let y = y - self.min_y;
            if !(0..HEIGHT).contains(&y) {
                continue;
            }
            chunk.set_block(x & 15, y, z & 15, solid);
        }
        self.npf.insert_chunk(chunk_x, chunk_z, chunk);
    }

    /// `pathFindAsync`, blocking: run it on a worker thread (`spawn_job`).
    pub fn path_find(&self, src: Pos, dst: Pos, options: SearchOptions) -> Result<UnpackedSegment, PathCalculationError> {
        let adjusted_src = [src[0], src[1] - self.min_y, src[2]];
        let adjusted_dst = [dst[0], dst[1] - self.min_y, dst[2]];
        let segment = self
            .npf
            .path_find(
                adjusted_src,
                adjusted_dst,
                !options.allow_tight_spaces, // atLeastX4
                false,                       // refine
                10000,                       // timeoutMs
                !options.predict_terrain,    // useAirIfChunkNotLoaded
                // TODO (Java): determine an appropriate cost value.
                8.0,
            )
            .map_err(PathCalculationError)?
            .ok_or_else(|| PathCalculationError("Path calculation failed".into()))?;

        let collected = UnpackedSegment::new(segment.blocks, segment.finished).collect();
        Ok(UnpackedSegment::new(collected.into_iter().map(|p| [p[0], p[1] + self.min_y, p[2]]).collect(), segment.finished))
    }

    /// Whether there is visibility between two points.
    pub fn raytrace(&self, start: [f64; 3], end: [f64; 3]) -> bool {
        let shifted = |p: [f64; 3]| [p[0], p[1] - f64::from(self.min_y), p[2]];
        // A coordinate that is not a finite number is refused by the ray: nothing can be seen along it.
        raytrace(&self.npf, shifted(start), shifted(end), CacheMiss::Solid).is_ok_and(|hit| hit.is_none())
    }

    /// `raytrace(count, src, dst, visibility)` with ALL: every ray of the batch reaches its end.
    pub fn raytrace_all(&self, src: &[f64], dst: &[f64]) -> bool {
        (0..src.len() / 3).all(|i| self.raytrace([src[i * 3], src[i * 3 + 1], src[i * 3 + 2]], [dst[i * 3], dst[i * 3 + 1], dst[i * 3 + 2]]))
    }

    /// The hit position of a ray, in world coordinates, if it hit.
    #[allow(dead_code)]
    pub fn raytrace_hit(&self, start: [f64; 3], end: [f64; 3]) -> Option<[f64; 3]> {
        let shifted = |p: [f64; 3]| [p[0], p[1] - f64::from(self.min_y), p[2]];
        raytrace(&self.npf, shifted(start), shifted(end), CacheMiss::Solid).ok().flatten().map(|h| [h[0], h[1] + f64::from(self.min_y), h[2]])
    }

    pub fn cancel(&self) {
        self.npf.cancel();
    }

    /// `destroy`.
    pub fn destroy(&self) {
        self.npf.close();
    }
}

/// `BlockStateOctreeInterface`: one block of the octrees, read through the chunk the last lookup fell in. One per thread.
pub struct BlockStateOctreeInterface {
    npf: Arc<NetherPathfinder>,
    min_y: i32,
    last: RefCell<Option<(i32, i32, Arc<Chunk>)>>,
}

impl BlockStateOctreeInterface {
    pub fn new(context: &NetherPathfinderContext) -> Self {
        Self { npf: context.npf.clone(), min_y: context.min_y, last: RefCell::new(None) }
    }

    /// Whether the block is anything but air. A chunk the pathfinder does not hold is solid, and so is nothing outside the
    /// pathfinder's height.
    pub fn get0(&self, x: i32, y: i32, z: i32) -> bool {
        let adjusted_y = y - self.min_y;
        if !(0..HEIGHT).contains(&adjusted_y) {
            return false;
        }
        let (chunk_x, chunk_z) = (x >> 4, z >> 4);
        let mut last = self.last.borrow_mut();
        if !last.as_ref().is_some_and(|(cx, cz, _)| *cx == chunk_x && *cz == chunk_z) {
            *last = Some((chunk_x, chunk_z, self.npf.get_chunk_or_default(chunk_x, chunk_z, true)));
        }
        last.as_ref().is_some_and(|(_, _, chunk)| chunk.is_solid(x & 0xF, adjusted_y, z & 0xF))
    }

    /// `passable`: `ElytraBehavior.passable(x, y, z)` for a block inside the world.
    pub fn passable(&self, x: i32, y: i32, z: i32) -> bool {
        !self.get0(x, y, z)
    }
}

/// An answer to come: `CompletableFuture<UnpackedSegment>`.
pub struct PathFuture {
    rx: mpsc::Receiver<Result<UnpackedSegment, PathCalculationError>>,
}

impl PathFuture {
    /// The answer, once there is one.
    pub fn poll(&self) -> Option<Result<UnpackedSegment, PathCalculationError>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(PathCalculationError("Path calculation failed".into()))),
        }
    }
}

/// Runs a search on a thread of its own. A panic in it is an answer of failure, not the end of the game.
pub fn spawn_job<F>(job: F) -> PathFuture
where
    F: FnOnce() -> Result<UnpackedSegment, PathCalculationError> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("elytra-path".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).unwrap_or_else(|_| Err(PathCalculationError("Path calculation failed".into())));
            let _ = tx.send(result);
        })
        .expect("a thread for the search");
    PathFuture { rx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn overworld(seed: i64) -> NetherPathfinderContext {
        NetherPathfinderContext::new(seed, None, Dimension::Overworld, -64, 384, false).unwrap()
    }

    fn wait(f: &PathFuture) -> Result<UnpackedSegment, PathCalculationError> {
        let start = Instant::now();
        loop {
            if let Some(r) = f.poll() {
                return r;
            }
            assert!(start.elapsed() < Duration::from_secs(20), "the job never answered");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn the_nether_is_held_to_128_unless_above_the_roof_is_allowed() {
        let nether = |roof| NetherPathfinderContext::new(1, None, Dimension::Nether, 0, 256, roof).unwrap();
        assert_eq!(nether(false).max_height(), 128);
        assert_eq!(nether(true).max_height(), 256);
        assert_eq!(overworld(1).max_height(), 384);
    }

    #[test]
    fn a_search_in_a_world_with_a_low_floor_comes_back_in_world_coordinates() {
        let ctx = Arc::new(overworld(1));
        let job = {
            let ctx = ctx.clone();
            spawn_job(move || ctx.path_find([0, 60, 0], [300, 60, 0], SearchOptions { predict_terrain: false, allow_tight_spaces: false }))
        };
        let segment = wait(&job).expect("a path");
        assert!(segment.is_finished());
        let path = segment.collect();
        assert!(path.len() >= 2);
        assert!(path.iter().all(|p| (p[1] - 60).abs() <= 16), "{path:?}");
    }

    #[test]
    fn unknown_chunks_are_solid_for_passable_and_a_block_update_changes_a_chunk() {
        let ctx = overworld(1);
        let boi = BlockStateOctreeInterface::new(&ctx);
        assert!(!boi.passable(5, 10, 5), "no chunk there");
        ctx.insert_chunk(0, 0, Chunk::new());
        let boi = BlockStateOctreeInterface::new(&ctx);
        assert!(boi.passable(5, 10, 5));
        ctx.update_blocks(0, 0, &[(5, 10, 5, true), (6, 10, 5, true), (6, 10, 5, false), (7, -500, 5, true)]);
        let boi = BlockStateOctreeInterface::new(&ctx);
        assert!(!boi.passable(5, 10, 5));
        assert!(boi.passable(6, 10, 5));
        // Outside the pathfinder's height there is nothing to hit.
        assert!(boi.passable(5, 5000, 5));
        // A ray sees the changed block, at its world height.
        assert!(!ctx.raytrace([1.5, 10.5, 5.5], [9.5, 10.5, 5.5]));
        assert!(ctx.raytrace([1.5, 12.5, 5.5], [9.5, 12.5, 5.5]));
        let hit = ctx.raytrace_hit([1.5, 10.5, 5.5], [9.5, 10.5, 5.5]).expect("a hit");
        assert!((hit[0] - 5.0).abs() < 1e-9 && (hit[1] - 10.5).abs() < 1e-9, "{hit:?}");
    }

    #[test]
    fn an_old_snapshot_is_not_changed_by_an_update() {
        let ctx = overworld(1);
        ctx.insert_chunk(0, 0, Chunk::new());
        let before = ctx.pathfinder().get_chunk(0, 0).unwrap();
        ctx.update_blocks(0, 0, &[(5, 80, 5, true)]);
        assert!(!before.is_solid(5, 80 + 64, 5), "a search holding the old chunk keeps seeing it");
        assert!(ctx.pathfinder().get_chunk(0, 0).unwrap().is_solid(5, 80 + 64, 5));
    }

    #[test]
    fn a_job_that_panics_answers_with_a_failure() {
        let f = spawn_job(|| panic!("boom"));
        assert!(wait(&f).is_err());
    }
}
