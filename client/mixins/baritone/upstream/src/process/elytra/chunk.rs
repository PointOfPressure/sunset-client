// baritone.process.elytra.pathfinder.Chunk, Size and NodePos: a 16 by 384 by 16 column of blocks, one bit each, laid
// out as an octree so that a cube of 2, 4, 8 or 16 blocks can be tested for emptiness with a few reads, and the
// cubes the elytra search is made of. The layout is the Java's (the native library's): 24 sections of 16x16x16
// blocks (512 bytes each); a section is 8 x8 cubes of 64 bytes; an x8 is 8 x4 cubes of 8 bytes; an x4 is 8 x2
// cubes of one byte; the 8 bits of that byte are the blocks.
use std::sync::{Arc, OnceLock};

pub const HEIGHT: i32 = 384;
pub const SECTIONS: usize = (HEIGHT / 16) as usize;
const SECTION_LONGS: usize = 64; // 512 bytes
const X8_BYTES: usize = 64;
const X4_BYTES: usize = 8;

/// `Size`: the width of a cube is 1 << ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Size {
    X1,
    X2,
    X4,
    X8,
    X16,
}

impl Size {
    const VALUES: [Size; 5] = [Size::X1, Size::X2, Size::X4, Size::X8, Size::X16];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    pub fn shift(self) -> i32 {
        self as i32
    }

    pub fn width(self) -> i32 {
        1 << (self as i32)
    }

    pub fn smaller(self) -> Size {
        Self::VALUES[self as usize - 1]
    }

    pub fn larger(self) -> Size {
        Self::VALUES[self as usize + 1]
    }
}

/// `NodePos`: the cube of the given size that holds a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodePos {
    pub size: Size,
    /// Absolute position divided by the width.
    x: i32,
    y: i32,
    z: i32,
}

impl NodePos {
    pub fn new(size: Size, x: i32, y: i32, z: i32) -> Self {
        let shift = size.shift();
        Self { size, x: x >> shift, y: y >> shift, z: z >> shift }
    }

    /// The absolute coordinates of the cube's lowest corner.
    pub fn min_x(&self) -> i32 {
        self.x << self.size.shift()
    }

    pub fn min_y(&self) -> i32 {
        self.y << self.size.shift()
    }

    pub fn min_z(&self) -> i32 {
        self.z << self.size.shift()
    }

    /// The block at the cube's centre, which is what a path is made of.
    pub fn absolute_pos_center(&self) -> [i32; 3] {
        let half = self.size.width() / 2;
        [self.min_x() + half, self.min_y() + half, self.min_z() + half]
    }

    /// The squared distance from the block at the cube's centre to a position.
    pub fn center_dist_sqr(&self, pos: [i32; 3]) -> f64 {
        let half = self.size.width() / 2;
        let dx = f64::from(self.min_x() + half - pos[0]);
        let dy = f64::from(self.min_y() + half - pos[1]);
        let dz = f64::from(self.min_z() + half - pos[2]);
        dx * dx + dy * dy + dz * dz
    }
}

#[derive(Clone)]
pub struct Chunk {
    sections: Vec<Option<Box<[u64; SECTION_LONGS]>>>,
    /// Bit i of entry s says whether x8 cube i of section s holds a block; a section that is None has 0.
    filled: [u8; SECTIONS],
    shared: bool,
}

impl Chunk {
    pub fn new() -> Self {
        Self { sections: (0..SECTIONS).map(|_| None).collect(), filled: [0; SECTIONS], shared: false }
    }

    /// The one chunk with no blocks. Shared; writes to it panic.
    pub fn air() -> Arc<Chunk> {
        static AIR: OnceLock<Arc<Chunk>> = OnceLock::new();
        AIR.get_or_init(|| {
            let mut c = Chunk::new();
            c.shared = true;
            Arc::new(c)
        })
        .clone()
    }

    /// The one chunk with every block solid. Shared; writes to it panic.
    pub fn solid() -> Arc<Chunk> {
        static SOLID: OnceLock<Arc<Chunk>> = OnceLock::new();
        SOLID
            .get_or_init(|| {
                let mut c = Chunk::new();
                c.shared = true;
                for i in 0..SECTIONS {
                    c.sections[i] = Some(Box::new([u64::MAX; SECTION_LONGS]));
                    c.filled[i] = 0xFF;
                }
                Arc::new(c)
            })
            .clone()
    }

    /// The chunk's 12288 bytes in the native layout (little endian words), for hashing in tests.
    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![0u8; SECTIONS * SECTION_LONGS * 8];
        for (i, section) in self.sections.iter().enumerate() {
            if let Some(s) = section {
                for (j, v) in s.iter().enumerate() {
                    let at = (i * SECTION_LONGS + j) * 8;
                    out[at..at + 8].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
        out
    }

    // The index math, the same as the native Chunk.h.
    fn x8_index(x: i32, y: i32, z: i32) -> usize {
        (((x & 8) >> 1) | ((y & 8) >> 2) | ((z & 8) >> 3)) as usize
    }

    fn x4_index(x: i32, y: i32, z: i32) -> usize {
        ((x & 4) | ((y & 4) >> 1) | ((z & 4) >> 2)) as usize
    }

    fn x2_index(x: i32, y: i32, z: i32) -> usize {
        ((((x & 2) << 1) | (y & 2) | ((z & 2) >> 1))) as usize
    }

    pub fn bit_index(x: i32, y: i32, z: i32) -> u32 {
        ((((x & 1) << 2) | ((y & 1) << 1) | (z & 1))) as u32
    }

    /// The byte offset, within its section, of the x2 cube that holds (x, y, z).
    pub fn x2_offset(x: i32, y: i32, z: i32) -> usize {
        Self::x8_index(x, y, z) * X8_BYTES + Self::x4_index(x, y, z) * X4_BYTES + Self::x2_index(x, y, z)
    }

    /// The byte at `off` in a section.
    pub fn byte_at(section: &[u64; SECTION_LONGS], off: usize) -> u8 {
        ((section[off >> 3] >> ((off & 7) << 3)) & 0xFF) as u8
    }

    /// The section holding y, or None if nothing has been set in it (or y is outside the chunk).
    pub fn section(&self, y: i32) -> Option<&[u64; SECTION_LONGS]> {
        let i = y >> 4;
        if i >= 0 && (i as usize) < SECTIONS { self.sections[i as usize].as_deref() } else { None }
    }

    /// Which x8 cubes of the section holding y hold a block, one bit each in x8 order; 0 outside the chunk.
    pub fn filled(&self, y: i32) -> u8 {
        let i = y >> 4;
        if i >= 0 && (i as usize) < SECTIONS { self.filled[i as usize] } else { 0 }
    }

    /// Coordinates are chunk relative: x and z in 0..15, y in 0..383.
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        let Some(s) = self.sections[(y >> 4) as usize].as_deref() else { return false };
        let off = Self::x2_offset(x, y, z);
        (s[off >> 3] >> (((off & 7) << 3) as u32 + Self::bit_index(x, y, z))) & 1 != 0
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, solid: bool) {
        assert!(!self.shared, "the shared air and solid chunks are read only");
        let section = (y >> 4) as usize;
        if self.sections[section].is_none() {
            if !solid {
                return;
            }
            self.sections[section] = Some(Box::new([0; SECTION_LONGS]));
        }
        let off = Self::x2_offset(x, y, z);
        let mask = 1u64 << (((off & 7) << 3) as u32 + Self::bit_index(x, y, z));
        let x8 = Self::x8_index(x, y, z);
        let s = self.sections[section].as_mut().expect("allocated above");
        if solid {
            self.filled[section] |= 1 << x8;
            s[off >> 3] |= mask;
        } else {
            s[off >> 3] &= !mask;
            let from = x8 * (X8_BYTES / 8);
            if s[from..from + X8_BYTES / 8].iter().all(|v| *v == 0) {
                self.filled[section] &= !(1 << x8);
            }
        }
    }

    /// Sets every block of one 16x16x16 section (0..23) at once.
    pub fn fill_section(&mut self, section: usize, solid: bool) {
        assert!(!self.shared, "the shared air and solid chunks are read only");
        if !solid {
            self.filled[section] = 0;
            self.sections[section] = None;
            return;
        }
        self.filled[section] = 0xFF;
        self.sections[section] = Some(Box::new([u64::MAX; SECTION_LONGS]));
    }

    pub fn is_empty(&self, size: Size, x: i32, y: i32, z: i32) -> bool {
        match size {
            Size::X1 => !self.is_solid(x, y, z),
            Size::X2 => self.is_empty_x2(x, y, z),
            Size::X4 => self.is_empty_x4(x, y, z),
            Size::X8 => self.is_empty_x8(x, y, z),
            Size::X16 => self.is_empty_x16(y),
        }
    }

    pub fn is_empty_x16(&self, y: i32) -> bool {
        self.filled[(y >> 4) as usize] == 0
    }

    pub fn is_empty_x8(&self, x: i32, y: i32, z: i32) -> bool {
        self.filled[(y >> 4) as usize] & (1 << Self::x8_index(x, y, z)) == 0
    }

    pub fn is_empty_x4(&self, x: i32, y: i32, z: i32) -> bool {
        match self.sections[(y >> 4) as usize].as_deref() {
            None => true,
            Some(s) => s[Self::x8_index(x, y, z) * (X8_BYTES / 8) + Self::x4_index(x, y, z)] == 0,
        }
    }

    pub fn is_empty_x2(&self, x: i32, y: i32, z: i32) -> bool {
        match self.sections[(y >> 4) as usize].as_deref() {
            None => true,
            Some(s) => Self::byte_at(s, Self::x2_offset(x, y, z)) == 0,
        }
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_set_and_cleared_show_in_every_size_of_cube() {
        let mut c = Chunk::new();
        assert!(c.is_empty(Size::X16, 0, 100, 0));
        c.set_block(5, 100, 9, true);
        assert!(c.is_solid(5, 100, 9));
        assert!(!c.is_solid(4, 100, 9));
        assert!(!c.is_empty(Size::X2, 5, 100, 9));
        assert!(!c.is_empty(Size::X4, 5, 100, 9));
        assert!(!c.is_empty(Size::X8, 5, 100, 9));
        assert!(!c.is_empty(Size::X16, 5, 100, 9));
        // The cube next to it is empty at the sizes that do not reach it.
        assert!(c.is_empty(Size::X2, 7, 100, 9));
        assert!(c.is_empty(Size::X4, 8, 100, 9));
        assert!(c.is_empty(Size::X8, 8, 100, 9));
        c.set_block(5, 100, 9, false);
        assert!(c.is_empty(Size::X16, 5, 100, 9));
        assert!(!c.is_solid(5, 100, 9));
    }

    #[test]
    fn every_block_of_a_section_reads_back() {
        let mut c = Chunk::new();
        let pattern = |x: i32, y: i32, z: i32| (x * 7 + y * 3 + z * 11) & 3 == 0;
        for x in 0..16 {
            for y in 48..64 {
                for z in 0..16 {
                    c.set_block(x, y, z, pattern(x, y, z));
                }
            }
        }
        for x in 0..16 {
            for y in 48..64 {
                for z in 0..16 {
                    assert_eq!(pattern(x, y, z), c.is_solid(x, y, z));
                }
            }
        }
        c.set_block(3, 50, 4, false);
        assert!(!c.is_solid(3, 50, 4));
        // Untouched sections stay air and unallocated.
        assert!(c.section(0).is_none());
        assert!(!c.is_solid(0, 0, 0));
    }

    #[test]
    fn a_clear_empties_its_cubes_only_with_the_last_block() {
        let mut c = Chunk::new();
        c.set_block(9, 70, 3, true);
        assert!(!c.is_empty_x8(9, 70, 3));
        assert!(c.is_empty_x8(1, 70, 3));
        assert!(!c.is_empty_x4(9, 70, 3));
        assert!(c.is_empty_x4(13, 70, 3));
        assert!(!c.is_empty_x2(9, 70, 3));
        assert!(c.is_empty_x2(11, 70, 3));
        assert!(c.is_empty(Size::X16, 0, 80, 0));
        c.set_block(10, 70, 3, true);
        c.set_block(9, 70, 3, false);
        assert!(!c.is_empty_x8(9, 70, 3));
        assert!(!c.is_empty_x16(64));
        c.set_block(10, 70, 3, false);
        assert!(c.is_empty_x8(9, 70, 3));
        assert!(c.is_empty_x16(64));
        assert!(c.section(64).is_some()); // the section stays allocated
    }

    #[test]
    fn filling_a_section() {
        let mut c = Chunk::new();
        c.fill_section(4, true);
        assert!(c.is_solid(0, 64, 0));
        assert!(c.is_solid(15, 79, 15));
        assert!(!c.is_solid(0, 80, 0));
        assert!(!c.is_empty_x8(15, 79, 15));
        assert!(c.is_empty_x8(0, 80, 0));
        c.fill_section(4, false);
        assert!(c.is_empty_x16(64));
        assert!(c.is_empty_x8(15, 79, 15));
        c.fill_section(4, true);
        c.set_block(15, 79, 15, false);
        assert!(!c.is_empty_x8(15, 79, 15), "the x8 still has its other blocks");
    }

    #[test]
    #[should_panic(expected = "read only")]
    fn the_shared_air_chunk_refuses_writes() {
        let mut c = Chunk::new();
        c.shared = true;
        c.set_block(0, 0, 0, true);
    }

    #[test]
    fn node_positions_are_cubes_and_their_centres() {
        let n = NodePos::new(Size::X4, 13, 70, -3);
        assert_eq!((n.min_x(), n.min_y(), n.min_z()), (12, 68, -4));
        assert_eq!(n.absolute_pos_center(), [14, 70, -2]);
        assert_eq!(NodePos::new(Size::X4, 12, 68, -4), n);
        assert_eq!(Size::X8.smaller(), Size::X4);
        assert_eq!(Size::X2.larger(), Size::X4);
    }

    #[test]
    fn the_shared_chunks_are_air_and_stone() {
        assert!(Chunk::air().is_empty(Size::X16, 0, 0, 0));
        assert!(Chunk::solid().is_solid(3, 383, 15));
        assert!(!Chunk::solid().is_empty(Size::X2, 0, 0, 0));
    }
}
