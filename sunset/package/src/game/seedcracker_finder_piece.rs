// kaptainwutax.seedcrackerX.finder.structure.PieceFinder and JigsawFinder, and render.Cuboid. A PieceFinder holds the blocks
// a structure is made of, turned to face one way, and looks for them around each of a list of centres; a structure is
// found where every block is the block it should be. The blocks are put in by the finders' buildStructure (structures_dsl.rs).
use super::sunset_scx_level::{BlockPos, ChunkPos, Direction, Level};
use std::collections::HashMap;
use std::sync::Arc;

/// `Cuboid`: a box to outline, in the colour `argb`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cuboid {
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub argb: u32,
    pub center_pos: BlockPos,
}

/// `ARGB.color(r, g, b)`: opaque.
pub fn argb_color(r: u32, g: u32, b: u32) -> u32 {
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

impl Cuboid {
    fn from_box(min: [f64; 3], max: [f64; 3], argb: u32) -> Cuboid {
        let center = BlockPos::new(((min[0] + max[0]) / 2.0).floor() as i32, ((min[1] + max[1]) / 2.0).floor() as i32, ((min[2] + max[2]) / 2.0).floor() as i32);
        Cuboid { min, max, argb, center_pos: center }
    }

    /// `new Cuboid(pos, argb)`: the one block.
    pub fn block(pos: BlockPos, argb: u32) -> Cuboid {
        Cuboid::from_box([pos.x as f64, pos.y as f64, pos.z as f64], [pos.x as f64 + 1.0, pos.y as f64 + 1.0, pos.z as f64 + 1.0], argb)
    }

    /// `new Cuboid(pos, size, argb)`: `AABB.encapsulatingFullBlocks(pos, pos.offset(size))`.
    pub fn sized(pos: BlockPos, size: [i32; 3], argb: u32) -> Cuboid {
        let end = pos.offset(size[0], size[1], size[2]);
        Cuboid::from_box(
            [pos.x.min(end.x) as f64, pos.y.min(end.y) as f64, pos.z.min(end.z) as f64],
            [pos.x.max(end.x) as f64 + 1.0, pos.y.max(end.y) as f64 + 1.0, pos.z.max(end.z) as f64 + 1.0],
            argb,
        )
    }

    /// `new Cuboid(boundingBox, argb)`: `AABB.of(box)`, the maximum corner included.
    pub fn bounding_box(min: BlockPos, max: BlockPos, argb: u32) -> Cuboid {
        Cuboid::from_box([min.x as f64, min.y as f64, min.z as f64], [max.x as f64 + 1.0, max.y as f64 + 1.0, max.z as f64 + 1.0], argb)
    }
}

/// How a piece finder turns what it is told: the PieceFinder's own, or the JigsawFinder's, which turns the piece the way a
/// jigsaw structure's pieces are turned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceKind {
    Piece,
    Jigsaw,
}

/// `PieceFinder`.
pub struct PieceFinder {
    kind: PieceKind,
    /// `boundingBox`: the corners, both inside.
    min: [i32; 3],
    max: [i32; 3],
    /// The blocks, in the order they were first put in (`LinkedHashMap`), and where each is.
    structure: Vec<(BlockPos, &'static str)>,
    index: HashMap<BlockPos, usize>,
    pub facing: Option<Direction>,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub search_positions: Arc<Vec<BlockPos>>,
}

/// Not in the Java: a plant the decoration puts on the ground after the structure, where the structure holds air. The Java
/// compares the block exactly, so one dead bush or (since 1.21.5) a tuft of dry grass in front of a pyramid made it miss
/// the whole structure; a plant over an air cell says nothing against the structure, so it counts as air here.
fn is_decoration_plant(block: &str) -> bool {
    matches!(
        block,
        "short_grass" | "tall_grass" | "fern" | "large_fern" | "dead_bush" | "short_dry_grass" | "tall_dry_grass" | "cactus" | "bush" | "firefly_bush" | "leaf_litter" | "wildflowers"
    )
}

impl PieceFinder {
    pub fn new(kind: PieceKind, facing: Option<Direction>, size: [i32; 3]) -> PieceFinder {
        let (min, max) = match facing {
            Some(f) if !f.is_z_axis() => ([0, 0, 0], [size[2] - 1, size[1] - 1, size[0] - 1]),
            _ => ([0, 0, 0], [size[0] - 1, size[1] - 1, size[2] - 1]),
        };
        // The Java asks the facing's axis, and a null facing is never given.
        PieceFinder { kind, min, max, structure: Vec::new(), index: HashMap::new(), facing, width: size[0], height: size[1], depth: size[2], search_positions: Arc::new(Vec::new()) }
    }

    /// The blocks put in so far, with where each one is.
    pub fn blocks(&self) -> &[(BlockPos, &'static str)] {
        &self.structure
    }

    /// `getLayout()`.
    pub fn get_layout(&self) -> [i32; 3] {
        match self.facing {
            Some(f) if !f.is_z_axis() => [self.depth, self.height, self.width],
            _ => [self.width, self.height, self.depth],
        }
    }

    /// `findInChunk`: the centres, as world positions, at which every block of the structure is where it should be.
    pub fn find_in_chunk(&self, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<BlockPos> {
        let mut result = Vec::new();
        if self.structure.is_empty() {
            return result;
        }
        let origin = chunk_pos.get_world_position();
        for center in self.search_positions.iter() {
            if level.cancelled() { return Vec::new(); }
            let base = origin.offset_pos(*center);
            let found = self.structure.iter().all(|(key, block)| {
                let pos = base.offset_pos(*key);
                if level.cancelled() || !level.is_chunk_loaded(pos.x >> 4, pos.z >> 4) {
                    return false;
                }
                let actual = level.block(pos.x, pos.y, pos.z);
                actual == *block || (*block == "air" && is_decoration_plant(actual))
            });
            if level.cancelled() { return Vec::new(); }
            if found {
                result.push(base);
            }
        }
        result
    }

    fn apply_x_transform(&self, x: i32, z: i32) -> i32 {
        match self.kind {
            PieceKind::Piece => match self.facing {
                None => x,
                Some(Direction::North | Direction::South) => self.min[0] + x,
                Some(Direction::West) => self.max[0] - z,
                Some(Direction::East) => self.min[0] + z,
            },
            PieceKind::Jigsaw => match self.facing {
                None => x,
                Some(Direction::East) => -z + self.get_layout()[0] - 1,
                Some(Direction::South) => -x + self.get_layout()[0] - 1,
                Some(Direction::West) => z,
                Some(Direction::North) => x,
            },
        }
    }

    fn apply_y_transform(&self, y: i32) -> i32 {
        if self.facing.is_none() { y } else { y + self.min[1] }
    }

    fn apply_z_transform(&self, x: i32, z: i32) -> i32 {
        match self.kind {
            PieceKind::Piece => match self.facing {
                None => z,
                Some(Direction::North) => self.max[2] - z,
                Some(Direction::South) => self.min[2] + z,
                Some(Direction::West | Direction::East) => self.min[2] + x,
            },
            PieceKind::Jigsaw => match self.facing {
                None => z,
                Some(Direction::East) => x,
                Some(Direction::South) => -z + self.get_layout()[2] - 1,
                Some(Direction::West) => -x + self.get_layout()[2] - 1,
                Some(Direction::North) => z,
            },
        }
    }

    fn is_inside(&self, pos: BlockPos) -> bool {
        pos.x >= self.min[0] && pos.x <= self.max[0] && pos.y >= self.min[1] && pos.y <= self.max[1] && pos.z >= self.min[2] && pos.z <= self.max[2]
    }

    /// `getBlockAt(ox, oy, oz)`.
    fn get_block_at(&self, ox: i32, oy: i32, oz: i32) -> &'static str {
        let pos = BlockPos::new(self.apply_x_transform(ox, oz), self.apply_y_transform(oy), self.apply_z_transform(ox, oz));
        if !self.is_inside(pos) {
            return "air";
        }
        self.index.get(&pos).map_or("air", |i| self.structure[*i].1)
    }

    /// `addBlock(state, x, y, z)`.
    pub fn add_block(&mut self, block: &'static str, x: i32, y: i32, z: i32) {
        let pos = BlockPos::new(self.apply_x_transform(x, z), self.apply_y_transform(y), self.apply_z_transform(x, z));
        if self.is_inside(pos) {
            match self.index.get(&pos) {
                Some(i) => self.structure[*i].1 = block,
                None => {
                    self.index.insert(pos, self.structure.len());
                    self.structure.push((pos, block));
                }
            }
        }
    }

    /// `addBlock(null, x, y, z)`: take the block out.
    pub fn remove_block(&mut self, x: i32, y: i32, z: i32) {
        let pos = BlockPos::new(self.apply_x_transform(x, z), self.apply_y_transform(y), self.apply_z_transform(x, z));
        if self.is_inside(pos) {
            if let Some(i) = self.index.remove(&pos) {
                // A LinkedHashMap keeps the order of the rest.
                self.structure.remove(i);
                for v in self.index.values_mut() {
                    if *v > i {
                        *v -= 1;
                    }
                }
            }
        }
    }

    /// `fillWithOutline(minX, minY, minZ, maxX, maxY, maxZ, outline, inside, onlyReplaceAir)`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_with_outline(&mut self, min_x: i32, min_y: i32, min_z: i32, max_x: i32, max_y: i32, max_z: i32, outline: &'static str, inside: &'static str, only_replace_air: bool) {
        self.fill_with_outline_opt(min_x, min_y, min_z, max_x, max_y, max_z, Some(outline), Some(inside), only_replace_air);
    }

    /// The same where a state is `null`, which takes the block out.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_with_outline_opt(&mut self, min_x: i32, min_y: i32, min_z: i32, max_x: i32, max_y: i32, max_z: i32, outline: Option<&'static str>, inside: Option<&'static str>, only_replace_air: bool) {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                for z in min_z..=max_z {
                    if !only_replace_air || self.get_block_at(x, y, z) != "air" {
                        let state = if y != min_y && y != max_y && x != min_x && x != max_x && z != min_z && z != max_z { inside } else { outline };
                        match state {
                            Some(b) => self.add_block(b, x, y, z),
                            None => self.remove_block(x, y, z),
                        }
                    }
                }
            }
        }
    }
}
