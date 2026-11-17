// baritone.utils.BlockStateInterface: what the pathfinder reads of the world, one block state at a
// time. The game answers it from the loaded chunks; the tests answer it from a map.
use super::baritone_block_state::BlockState;
use std::cell::RefCell;
use std::rc::Rc;

/// `CACHE_BITS` and `MINING_CACHE_BITS`: both caches of a search hold this many entries, each position in one slot.
const CACHE_BITS: u32 = 16;
pub const CACHE_SIZE: usize = 1 << CACHE_BITS;
/// y is shifted so it is never negative, which means this key would need y = 4095.
pub const NO_KEY: u64 = u64::MAX;

/// The key of a position, with `y` already shifted so that 0 is the bottom of the world.
pub fn cache_key(x: i32, y: i32, z: i32) -> u64 {
    ((u64::from(x as u32) & 0x3FF_FFFF) << 38) | ((u64::from(z as u32) & 0x3FF_FFFF) << 12) | (u64::from(y as u32) & 0xFFF)
}

pub fn cache_slot(key: u64) -> usize {
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - CACHE_BITS)) as usize
}

/// `cacheKeys` and `cacheVals`. The 22 movements out of one node read the same 50 blocks about 110 times between them, and
/// the next node reads most of them again. It lives as long as the search that claimed it.
pub struct SearchBlocks {
    entries: RefCell<Vec<(u64, Option<Rc<BlockState>>)>>,
}

impl Default for SearchBlocks {
    fn default() -> Self {
        Self { entries: RefCell::new(vec![(NO_KEY, None); CACHE_SIZE]) }
    }
}

impl SearchBlocks {
    /// The block at the (shifted) position, from `read` the first time.
    pub fn get_or_read(&self, x: i32, y: i32, z: i32, read: impl FnOnce() -> Rc<BlockState>) -> Rc<BlockState> {
        let key = cache_key(x, y, z);
        let slot = cache_slot(key);
        if let (found, Some(state)) = &self.entries.borrow()[slot] {
            if *found == key {
                return state.clone();
            }
        }
        let state = read();
        self.entries.borrow_mut()[slot] = (key, Some(state.clone()));
        state
    }
}

pub trait BlockStateInterface {
    /// `get0`: the block at the position, air where nothing is loaded (the Java does the same).
    fn get0(&self, x: i32, y: i32, z: i32) -> Rc<BlockState>;

    /// `claimCache`: the reads of `get0` go through this one until `release_cache`.
    fn claim_cache(&self, _cache: Rc<SearchBlocks>) {}

    /// `releaseCache`.
    fn release_cache(&self) {}

    /// Whether the chunk holding this column is loaded.
    fn is_loaded(&self, x: i32, z: i32) -> bool;

    /// `worldContainsLoadedChunk`.
    fn world_contains_loaded_chunk(&self, x: i32, z: i32) -> bool {
        self.is_loaded(x, z)
    }

    /// The lowest block y of the world.
    fn min_y(&self) -> i32;

    /// The highest block y of the world (`BlockStateInterface.maxY`: `minY + height - 1`).
    fn max_y(&self) -> i32;

    /// `BetterWorldBorder.entirelyContains`: the whole block column is inside the world border.
    fn entirely_contains(&self, _x: i32, _z: i32) -> bool {
        true
    }

    /// `BetterWorldBorder.canPlaceAt`: inside the world border.
    fn can_place_at(&self, _x: i32, _z: i32) -> bool {
        true
    }
}
