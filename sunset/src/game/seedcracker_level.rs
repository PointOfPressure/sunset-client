// What the finders ask of the world (`Level`, `ChunkPos`, `BlockPos`, `Direction`), and BiomeFixer, which turns the names
// of the game's biomes into the library's. The game fills in `Level`; the finders are written, and tested, against it.
use super::sunset_scx_biome::{self, biomes};
use super::sunset_scx_dimension::Dimension;

/// `BlockPos`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    pub fn offset(self, dx: i32, dy: i32, dz: i32) -> BlockPos {
        BlockPos::new(self.x + dx, self.y + dy, self.z + dz)
    }

    pub fn offset_pos(self, other: BlockPos) -> BlockPos {
        self.offset(other.x, other.y, other.z)
    }

    pub fn below(self) -> BlockPos {
        self.offset(0, -1, 0)
    }

    /// `relative(direction, n)`.
    pub fn relative(self, direction: Direction, n: i32) -> BlockPos {
        let (dx, dz) = direction.step();
        self.offset(dx * n, 0, dz * n)
    }
}

/// `ChunkPos`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> ChunkPos {
        ChunkPos { x, z }
    }

    /// `getWorldPosition()`: the block at the chunk's corner, at height 0.
    pub fn get_world_position(self) -> BlockPos {
        BlockPos::new(self.x << 4, 0, self.z << 4)
    }

    /// `ChunkPos.containing(pos)`.
    pub fn containing(pos: BlockPos) -> ChunkPos {
        ChunkPos::new(pos.x >> 4, pos.z >> 4)
    }
}

/// `Direction`, the horizontal ones, in the order of `Direction.Plane.HORIZONTAL`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub const HORIZONTAL: [Direction; 4] = [Direction::North, Direction::East, Direction::South, Direction::West];

    /// The step on x and z.
    pub fn step(self) -> (i32, i32) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
        }
    }

    pub fn get_opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::East => Direction::West,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
        }
    }

    pub fn get_clockwise(self) -> Direction {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
        }
    }

    pub fn get_counter_clockwise(self) -> Direction {
        match self {
            Direction::North => Direction::West,
            Direction::East => Direction::North,
            Direction::South => Direction::East,
            Direction::West => Direction::South,
        }
    }

    /// Whether the direction runs along z (`getAxis() == Axis.Z`).
    pub fn is_z_axis(self) -> bool {
        matches!(self, Direction::North | Direction::South)
    }

    pub fn from_name(name: &str) -> Option<Direction> {
        match name {
            "north" => Some(Direction::North),
            "east" => Some(Direction::East),
            "south" => Some(Direction::South),
            "west" => Some(Direction::West),
            _ => None,
        }
    }
}

/// The world as a finder reads it. Block and biome names are the game's registry paths, without the namespace
/// (`"sandstone"`, `"snowy_plains"`).
pub trait Level: Send + Sync {
    fn cancelled(&self) -> bool { false }
    /// `Finder.inferDimension(dimensionType)`: the dimension this level is.
    fn dimension(&self) -> Dimension;
    /// The block at a position (`getBlockState(pos).getBlock()`); `"air"` where the chunk is not loaded.
    fn block(&self, x: i32, y: i32, z: i32) -> &'static str;
    /// A property of the state at a position, as the game writes it (`"true"`, `"single"`, `"north"`).
    fn property(&self, x: i32, y: i32, z: i32, name: &str) -> Option<&'static str>;
    /// Whether the position holds a block entity of this kind (`"chest"`, `"spawner"`).
    fn has_block_entity(&self, x: i32, y: i32, z: i32, kind: &str) -> bool;
    /// Whether the block at a position is in a block tag (`"mineable_with_axe"`, `"logs"`).
    fn block_in_tag(&self, x: i32, y: i32, z: i32, tag: &str) -> bool;
    /// `getNoiseBiome(x, y, z)`, in quart coordinates: the biome's registry path.
    fn noise_biome(&self, x: i32, y: i32, z: i32) -> &'static str;
    /// `getBiome(pos)`, in block coordinates.
    fn biome(&self, x: i32, y: i32, z: i32) -> &'static str;
    /// `getHeightmapPos(WORLD_SURFACE, pos).getY()`.
    fn world_surface_height(&self, x: i32, z: i32) -> i32;
    /// `getChunkSource().getChunkNow(x, z) != null`.
    fn is_chunk_loaded(&self, chunk_x: i32, chunk_z: i32) -> bool;
}

/// BiomeFixer: the library's biome for a game biome's registry path. What the library has no biome for is the void.
pub fn biome_fixer_swap(path: &str) -> i32 {
    let renamed = match path {
        // renamed
        "snowy_plains" => biomes::SNOWY_TUNDRA,
        "old_growth_birch_forest" => biomes::TALL_BIRCH_FOREST,
        "old_growth_pine_taiga" => biomes::GIANT_TREE_TAIGA,
        "old_growth_spruce_taiga" => biomes::GIANT_TREE_TAIGA,
        "windswept_hills" => biomes::EXTREME_HILLS,
        "windswept_forest" => biomes::WOODED_MOUNTAINS,
        "windswept_gravelly_hills" => biomes::GRAVELLY_MOUNTAINS,
        "windswept_savanna" => biomes::SHATTERED_SAVANNA,
        "sparse_jungle" => biomes::JUNGLE_EDGE,
        "stony_shore" => biomes::STONE_SHORE,
        // new
        "meadow" => biomes::PLAINS,
        "grove" => biomes::TAIGA,
        "snowy_slopes" => biomes::SNOWY_TUNDRA,
        "frozen_peaks" => biomes::TAIGA,
        "jagged_peaks" => biomes::TAIGA,
        "stony_peaks" => biomes::TAIGA,
        "mangrove_swamp" => biomes::SWAMP,
        // unsure what to do with those, they return THE_VOID for now: dripstone_caves, lush_caves, deep_dark
        other => return sunset_scx_biome::by_name(other).map_or(biomes::THE_VOID, |b| b.id),
    };
    renamed
}
