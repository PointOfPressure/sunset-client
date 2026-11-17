// baritone.cache.CachedChunk, CachedRegion, CachedWorld and the packing of ChunkPacker, with the Java's formats:
// a chunk is 2 bits per block (air, water, avoid, solid), the top block of every column, and the blocks it keeps
// track of (chests, spawners, beds and the like); a region is 32 x 32 chunks in the `r.X.Z.bcr` file. What needs
// the game (reading a loaded chunk, the gzip, the files) is behind `PackSource` and `RegionStore`.
use std::collections::HashMap;
use std::sync::Arc;

/// `CachedChunk.BLOCKS_TO_KEEP_TRACK_OF`, by the name `BlockUtils.blockToString` gives (no `minecraft:`).
pub const BLOCKS_TO_KEEP_TRACK_OF: &[&str] = &[
    "ender_chest", "furnace", "chest", "trapped_chest", "end_portal", "end_portal_frame", "spawner", "barrier", "observer",
    "white_shulker_box", "orange_shulker_box", "magenta_shulker_box", "light_blue_shulker_box", "yellow_shulker_box",
    "lime_shulker_box", "pink_shulker_box", "gray_shulker_box", "light_gray_shulker_box", "cyan_shulker_box",
    "purple_shulker_box", "blue_shulker_box", "brown_shulker_box", "green_shulker_box", "red_shulker_box",
    "black_shulker_box", "nether_portal", "hopper", "beacon", "brewing_stand", "creeper_head", "creeper_wall_head",
    "dragon_head", "dragon_wall_head", "player_head", "player_wall_head", "zombie_head", "zombie_wall_head",
    "skeleton_skull", "skeleton_wall_skull", "wither_skeleton_skull", "wither_skeleton_wall_skull", "enchanting_table",
    "anvil", "white_bed", "orange_bed", "magenta_bed", "light_blue_bed", "yellow_bed", "lime_bed", "pink_bed", "gray_bed",
    "light_gray_bed", "cyan_bed", "purple_bed", "blue_bed", "brown_bed", "green_bed", "red_bed", "black_bed", "dragon_egg",
    "jukebox", "end_gateway", "cobweb", "nether_wart", "ladder", "vine",
];

/// `PathingBlockType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathingBlockType {
    Air,
    Water,
    Avoid,
    Solid,
}

impl PathingBlockType {
    /// `getBits`: the two bits, high first.
    pub fn bits(self) -> (bool, bool) {
        match self {
            Self::Air => (false, false),
            Self::Water => (false, true),
            Self::Avoid => (true, false),
            Self::Solid => (true, true),
        }
    }

    /// `fromBits`.
    pub fn from_bits(b1: bool, b2: bool) -> Self {
        match (b1, b2) {
            (true, true) => Self::Solid,
            (true, false) => Self::Avoid,
            (false, true) => Self::Water,
            (false, false) => Self::Air,
        }
    }
}

/// The parts of a DimensionType the cache reads.
#[derive(Clone, Copy, Debug)]
pub struct Dimension {
    pub min_y: i32,
    pub height: i32,
    pub logical_height: i32,
    pub has_ceiling: bool,
    pub natural: bool,
    pub ultra_warm: bool,
    pub end: bool,
}

impl Dimension {
    #[allow(dead_code)]
    pub fn overworld() -> Self {
        Self { min_y: -64, height: 384, logical_height: 384, has_ceiling: false, natural: true, ultra_warm: false, end: false }
    }
}

/// `ChunkPacker.pathingTypeToBlock`: the block name that stands for a pathing type (None where the Java gives null).
pub fn pathing_type_to_block(t: PathingBlockType, d: &Dimension) -> Option<&'static str> {
    match t {
        PathingBlockType::Air => Some("air"),
        PathingBlockType::Water => Some("water"),
        PathingBlockType::Avoid => Some("lava"),
        PathingBlockType::Solid => {
            if d.natural {
                Some("stone")
            } else if d.ultra_warm {
                Some("netherrack")
            } else if d.end {
                Some("end_stone")
            } else {
                None
            }
        }
    }
}

/// `CachedChunk.size`: the bits of a chunk of this height.
pub fn size(dimension_height: i32) -> usize {
    2 * 16 * 16 * dimension_height as usize
}

/// `CachedChunk.sizeInBytes`.
pub fn size_in_bytes(size: usize) -> usize {
    size / 8
}

/// `CachedChunk.getPositionIndex`.
pub fn position_index(x: i32, y: i32, z: i32) -> usize {
    ((x << 1) | (z << 5) | (y << 9)) as usize
}

#[derive(Clone)]
pub struct CachedChunk {
    pub x: i32,
    pub z: i32,
    pub height: i32,
    /// 2 bits per block, `size_in_bytes` long.
    data: Vec<u8>,
    /// The block of the top of every column (z << 4 | x), by name.
    overview: Vec<Arc<str>>,
    height_map: Vec<i32>,
    /// The blocks it keeps track of: by name, where (x, y with the world's minimum added, z).
    special_block_locations: HashMap<String, Vec<[i32; 3]>>,
    special: HashMap<usize, String>,
    pub cache_timestamp: i64,
}

impl CachedChunk {
    pub fn new(x: i32, z: i32, height: i32, data: Vec<u8>, overview: Vec<Arc<str>>, special_block_locations: HashMap<String, Vec<[i32; 3]>>, cache_timestamp: i64, min_y: i32) -> Self {
        assert!(data.len() <= size_in_bytes(size(height)), "BitSet of invalid length provided");
        let mut data = data;
        data.resize(size_in_bytes(size(height)), 0);
        let mut special = HashMap::new();
        for (name, locs) in &special_block_locations {
            for l in locs {
                special.insert(position_index(l[0], l[1] - min_y, l[2]), name.clone());
            }
        }
        let mut chunk = Self { x, z, height, data, overview, height_map: vec![0; 256], special_block_locations, special, cache_timestamp };
        chunk.calculate_height_map();
        chunk
    }

    fn bit(&self, index: usize) -> bool {
        self.data.get(index >> 3).is_some_and(|b| b & (1 << (index & 7)) != 0)
    }

    fn get_type(&self, index: usize) -> PathingBlockType {
        PathingBlockType::from_bits(self.bit(index), self.bit(index + 1))
    }

    /// The highest non-air y of every column (z << 4 | x), 0 if the whole column is air.
    fn calculate_height_map(&mut self) {
        for z in 0..16 {
            for x in 0..16 {
                let mut top = 0;
                for y in (0..self.height).rev() {
                    let index = position_index(x, y, z);
                    if self.bit(index) || self.bit(index + 1) {
                        top = y;
                        break;
                    }
                }
                self.height_map[(z << 4 | x) as usize] = top;
            }
        }
    }

    /// `getBlock`: the block name cached for a position inside the chunk (y counted from the world's minimum).
    pub fn get_block(&self, x: i32, y: i32, z: i32, dimension: &Dimension) -> Option<String> {
        let index = position_index(x, y, z);
        let t = self.get_type(index);
        let internal = (z << 4 | x) as usize;
        if self.height_map[internal] == y && t != PathingBlockType::Avoid {
            // We have this exact block, it's a surface block.
            return Some(self.overview[internal].to_string());
        }
        if let Some(name) = self.special.get(&index) {
            return Some(name.clone());
        }
        if t == PathingBlockType::Solid {
            if y == dimension.logical_height - 1 && dimension.has_ceiling {
                // Nether roof is always unbreakable.
                return Some("bedrock".into());
            }
            if y < -59 && dimension.natural {
                // The Java means solid blocks below 5 here, which are commonly bedrock, but compares the height counted
                // from the world's minimum, so this never happens there either.
                // Returning bedrock always would be a little yikes: discourage paths that break them instead.
                return Some("obsidian".into());
            }
        }
        pathing_type_to_block(t, dimension).map(str::to_owned)
    }

    pub fn overview(&self) -> &[Arc<str>] {
        &self.overview
    }

    pub fn relative_blocks(&self) -> &HashMap<String, Vec<[i32; 3]>> {
        &self.special_block_locations
    }

    /// `getAbsoluteBlocks`.
    pub fn absolute_blocks(&self, block_type: &str) -> Option<Vec<[i32; 3]>> {
        self.special_block_locations.get(block_type).map(|locs| locs.iter().map(|p| [p[0] + self.x * 16, p[1], p[2] + self.z * 16]).collect())
    }

    /// `toByteArray`: the raw data without the zeros at its end.
    pub fn to_byte_array(&self) -> Vec<u8> {
        let mut v = self.data.clone();
        while v.last() == Some(&0) {
            v.pop();
        }
        v
    }

    pub fn size_in_bytes(&self) -> usize {
        self.data.len()
    }
}

/// `ChunkPacker`: what a loaded chunk shows to the packer.
pub trait PackSource {
    fn x(&self) -> i32;
    fn z(&self) -> i32;
    fn min_y(&self) -> i32;
    fn height(&self) -> i32;
    /// Whether a 16-high section is all air (so it can be skipped).
    fn section_has_only_air(&self, section: i32) -> bool;
    /// The block name at a position of the chunk (local x and z, absolute y), `air` out of range.
    fn block_name(&self, x: i32, y: i32, z: i32) -> Arc<str>;
    /// The pathing type of that block, with the neighbours the packer needs.
    fn pathing_type(&self, x: i32, y: i32, z: i32) -> PathingBlockType;
}

/// `ChunkPacker.pack`.
pub fn pack(source: &dyn PackSource, now_ms: i64) -> CachedChunk {
    let height = source.height();
    let min_y = source.min_y();
    let mut data = vec![0u8; size_in_bytes(size(height))];
    let mut special: HashMap<String, Vec<[i32; 3]>> = HashMap::new();
    let set = |data: &mut Vec<u8>, index: usize, on: bool| {
        if on {
            data[index >> 3] |= 1 << (index & 7);
        }
    };
    for section in 0..height / 16 {
        // Any 16x16x16 area that's all air can be skipped entirely.
        if source.section_has_only_air(section) {
            continue;
        }
        let y_real = section << 4;
        for y1 in 0..16 {
            let y = y1 | y_real;
            for z in 0..16 {
                for x in 0..16 {
                    let index = position_index(x, y, z);
                    let (b1, b2) = source.pathing_type(x, y + min_y, z).bits();
                    set(&mut data, index, b1);
                    set(&mut data, index + 1, b2);
                    let name = source.block_name(x, y + min_y, z);
                    let bare = name.strip_prefix("minecraft:").unwrap_or(&name);
                    if BLOCKS_TO_KEEP_TRACK_OF.contains(&bare) {
                        special.entry(bare.to_owned()).or_default().push([x, y + min_y, z]);
                    }
                }
            }
        }
    }
    // The top block in columns.
    let mut overview: Vec<Arc<str>> = vec![Arc::from("air"); 256];
    for z in 0..16 {
        for x in 0..16 {
            for y in (0..height).rev() {
                let index = position_index(x, y, z);
                let on = |i: usize| data[i >> 3] & (1 << (i & 7)) != 0;
                if on(index) || on(index + 1) {
                    let name = source.block_name(x, y + min_y, z);
                    overview[(z << 4 | x) as usize] = Arc::from(name.strip_prefix("minecraft:").unwrap_or(&name));
                    break;
                }
            }
        }
    }
    CachedChunk::new(source.x(), source.z(), height, data, overview, special, now_ms, min_y)
}

/// Where the regions are kept (the `.bcr` files, gzipped).
pub trait RegionStore {
    fn load(&self, region_x: i32, region_z: i32) -> Option<Vec<u8>>;
    fn save(&self, region_x: i32, region_z: i32, bytes: &[u8]);
}

/// `CachedRegion.CACHED_REGION_MAGIC`.
pub const CACHED_REGION_MAGIC: i32 = 456_022_911;

pub struct CachedRegion {
    x: i32,
    z: i32,
    chunks: Vec<Option<CachedChunk>>,
    dimension: Dimension,
    has_unsaved_changes: bool,
}

fn write_utf(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        if self.at + n > self.bytes.len() {
            return Err("unexpected end of file".into());
        }
        let s = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn i32(&mut self) -> Result<i32, String> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i64(&mut self) -> Result<i64, String> {
        let b = self.take(8)?;
        Ok(i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn utf(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }
}

impl CachedRegion {
    pub fn new(x: i32, z: i32, dimension: Dimension) -> Self {
        Self { x, z, chunks: vec![None; 1024], dimension, has_unsaved_changes: false }
    }

    pub fn x(&self) -> i32 {
        self.x
    }

    pub fn z(&self) -> i32 {
        self.z
    }

    fn slot(x: i32, z: i32) -> usize {
        (x as usize) * 32 + z as usize
    }

    /// `getBlock` for a position inside the region (x and z from 0 to 511).
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<String> {
        let adj_y = y - self.dimension.min_y;
        self.chunks[Self::slot(x >> 4, z >> 4)].as_ref().and_then(|c| c.get_block(x & 15, adj_y, z & 15, &self.dimension))
    }

    pub fn is_cached(&self, x: i32, z: i32) -> bool {
        self.chunks[Self::slot(x >> 4, z >> 4)].is_some()
    }

    pub fn locations_of(&self, block: &str) -> Vec<[i32; 3]> {
        let mut res = Vec::new();
        for chunk in self.chunks.iter().flatten() {
            if let Some(locs) = chunk.absolute_blocks(block) {
                res.extend(locs);
            }
        }
        res
    }

    pub fn update_cached_chunk(&mut self, chunk_x: i32, chunk_z: i32, chunk: CachedChunk) {
        self.chunks[Self::slot(chunk_x, chunk_z)] = Some(chunk);
        self.has_unsaved_changes = true;
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.has_unsaved_changes
    }

    #[allow(dead_code)]
    pub fn most_recently_modified(&self) -> Option<&CachedChunk> {
        self.chunks.iter().flatten().max_by_key(|c| c.cache_timestamp)
    }

    /// `removeExpired`: forget chunks cached longer than `expiry_seconds` ago (never with a negative expiry).
    pub fn remove_expired(&mut self, expiry_seconds: i64, now_ms: i64) {
        if expiry_seconds < 0 {
            return;
        }
        let oldest = now_ms - expiry_seconds * 1000;
        for slot in &mut self.chunks {
            if slot.as_ref().is_some_and(|c| c.cache_timestamp < oldest) {
                *slot = None;
            }
        }
    }

    /// `save`: the bytes of the file (before the gzip).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&CACHED_REGION_MAGIC.to_be_bytes());
        for x in 0..32 {
            for z in 0..32 {
                match &self.chunks[Self::slot(x, z)] {
                    None => out.push(0),
                    Some(chunk) => {
                        out.push(1);
                        let bytes = chunk.to_byte_array();
                        out.extend_from_slice(&bytes);
                        // Fills the empty 0s that should be trailing to fill up the space.
                        out.extend(std::iter::repeat_n(0u8, chunk.size_in_bytes() - bytes.len()));
                    }
                }
            }
        }
        for x in 0..32 {
            for z in 0..32 {
                if let Some(chunk) = &self.chunks[Self::slot(x, z)] {
                    for name in chunk.overview() {
                        write_utf(&mut out, name);
                    }
                }
            }
        }
        for x in 0..32 {
            for z in 0..32 {
                if let Some(chunk) = &self.chunks[Self::slot(x, z)] {
                    let locs = chunk.relative_blocks();
                    out.extend_from_slice(&(locs.len() as u16).to_be_bytes());
                    for (name, positions) in locs {
                        write_utf(&mut out, name);
                        out.extend_from_slice(&(positions.len() as u16).to_be_bytes());
                        for p in positions {
                            out.push(((p[2] << 4) | p[0]) as u8);
                            out.extend_from_slice(&(p[1] - self.dimension.min_y).to_be_bytes());
                        }
                    }
                }
            }
        }
        for x in 0..32 {
            for z in 0..32 {
                if let Some(chunk) = &self.chunks[Self::slot(x, z)] {
                    out.extend_from_slice(&chunk.cache_timestamp.to_be_bytes());
                }
            }
        }
        out
    }

    /// `load`: from the bytes of a file (after the gunzip). Only a whole, uncorrupted file changes the region.
    pub fn load_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        let mut r = Reader { bytes, at: 0 };
        let magic = r.i32()?;
        if magic != CACHED_REGION_MAGIC {
            return Err(format!("Bad magic value {magic}"));
        }
        let per_chunk = size_in_bytes(size(self.dimension.height));
        let mut present = vec![false; 1024];
        let mut data: Vec<Vec<u8>> = vec![Vec::new(); 1024];
        for x in 0..32 {
            for z in 0..32 {
                match r.u8()? {
                    1 => {
                        data[Self::slot(x, z)] = r.take(per_chunk)?.to_vec();
                        present[Self::slot(x, z)] = true;
                    }
                    0 => {}
                    _ => return Err("Malformed stream".into()),
                }
            }
        }
        let mut overview: Vec<Vec<Arc<str>>> = vec![Vec::new(); 1024];
        for x in 0..32 {
            for z in 0..32 {
                if present[Self::slot(x, z)] {
                    for _ in 0..256 {
                        overview[Self::slot(x, z)].push(Arc::from(r.utf()?.as_str()));
                    }
                }
            }
        }
        let mut locations: Vec<HashMap<String, Vec<[i32; 3]>>> = vec![HashMap::new(); 1024];
        for x in 0..32 {
            for z in 0..32 {
                if !present[Self::slot(x, z)] {
                    continue;
                }
                let types = r.u16()?;
                for _ in 0..types {
                    let name = r.utf()?;
                    let mut count = r.u16()? as usize;
                    if count == 0 {
                        // An entire chunk full of air can happen in the end.
                        count = 65536;
                    }
                    let mut locs = Vec::with_capacity(count.min(4096));
                    for _ in 0..count {
                        let xz = r.u8()?;
                        let y = r.i32()?;
                        locs.push([i32::from(xz & 0x0f), y + self.dimension.min_y, i32::from((xz >> 4) & 0x0f)]);
                    }
                    locations[Self::slot(x, z)].insert(name, locs);
                }
            }
        }
        let mut timestamp = vec![0i64; 1024];
        for x in 0..32 {
            for z in 0..32 {
                if present[Self::slot(x, z)] {
                    timestamp[Self::slot(x, z)] = r.i64()?;
                }
            }
        }
        for x in 0..32 {
            for z in 0..32 {
                let s = Self::slot(x, z);
                if present[s] {
                    let chunk = CachedChunk::new(x + 32 * self.x, z + 32 * self.z, self.dimension.height, std::mem::take(&mut data[s]), std::mem::take(&mut overview[s]), std::mem::take(&mut locations[s]), timestamp[s], self.dimension.min_y);
                    self.chunks[s] = Some(chunk);
                }
            }
        }
        self.has_unsaved_changes = false;
        Ok(())
    }

    pub fn mark_saved(&mut self) {
        self.has_unsaved_changes = false;
    }
}

/// What the cache reads of the settings.
#[derive(Clone, Copy)]
pub struct CacheSettings {
    pub chunk_caching: bool,
    pub prune_regions_from_ram: bool,
    pub cached_chunks_expiry_seconds: i64,
}

/// `REGION_MAX`: the number of regions in any direction from (0, 0).
const REGION_MAX: i32 = 30_000_000 / 512 + 1;

/// `CachedWorld`, without the threads: chunks queued for packing wait for the game to pack a few each tick.
pub struct CachedWorld {
    regions: HashMap<i64, CachedRegion>,
    dimension: Dimension,
    store: Box<dyn RegionStore>,
    /// Chunk positions waiting to be packed, in the order they were queued (each once).
    to_pack: Vec<(i32, i32)>,
}

impl CachedWorld {
    pub fn new(dimension: Dimension, store: Box<dyn RegionStore>) -> Self {
        Self { regions: HashMap::new(), dimension, store, to_pack: Vec::new() }
    }

    #[allow(dead_code)]
    pub fn dimension(&self) -> &Dimension {
        &self.dimension
    }

    fn region_id(rx: i32, rz: i32) -> i64 {
        if !(-REGION_MAX..=REGION_MAX).contains(&rx) || !(-REGION_MAX..=REGION_MAX).contains(&rz) {
            return 0;
        }
        i64::from(rx as u32) | (i64::from(rz as u32) << 32)
    }

    /// `queueForPacking`.
    pub fn queue_for_packing(&mut self, chunk_x: i32, chunk_z: i32) {
        if !self.to_pack.contains(&(chunk_x, chunk_z)) {
            self.to_pack.push((chunk_x, chunk_z));
        }
    }

    /// The next chunk to pack, if any.
    pub fn next_to_pack(&mut self) -> Option<(i32, i32)> {
        if self.to_pack.is_empty() { None } else { Some(self.to_pack.remove(0)) }
    }

    pub fn pending(&self) -> usize {
        self.to_pack.len()
    }

    pub fn is_cached(&self, block_x: i32, block_z: i32) -> bool {
        self.get_region(block_x >> 9, block_z >> 9).is_some_and(|r| r.is_cached(block_x & 511, block_z & 511))
    }

    pub fn region_loaded(&self, block_x: i32, block_z: i32) -> bool {
        self.regions.contains_key(&Self::region_id(block_x >> 9, block_z >> 9))
    }

    pub fn get_region(&self, rx: i32, rz: i32) -> Option<&CachedRegion> {
        self.regions.get(&Self::region_id(rx, rz))
    }

    fn get_or_create_region(&mut self, rx: i32, rz: i32) -> &mut CachedRegion {
        let id = Self::region_id(rx, rz);
        if !self.regions.contains_key(&id) {
            let mut region = CachedRegion::new(rx, rz, self.dimension);
            if let Some(bytes) = self.store.load(rx, rz) {
                // A corrupt file leaves the region empty, as in the Java.
                let _ = region.load_bytes(&bytes);
            }
            self.regions.insert(id, region);
        }
        self.regions.get_mut(&id).expect("just inserted")
    }

    /// `reloadAllFromDisk`: every region in RAM is read from its file again.
    pub fn reload_all_from_disk(&mut self) {
        let ids: Vec<(i32, i32)> = self.regions.values().map(|r| (r.x(), r.z())).collect();
        for (rx, rz) in ids {
            if let Some(bytes) = self.store.load(rx, rz) {
                if let Some(region) = self.regions.get_mut(&Self::region_id(rx, rz)) {
                    let _ = region.load_bytes(&bytes);
                }
            }
        }
    }

    /// `tryLoadFromDisk`.
    pub fn try_load_from_disk(&mut self, rx: i32, rz: i32) {
        self.get_or_create_region(rx, rz);
    }

    /// `getBlock` through the regions (None where nothing is cached).
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Option<String> {
        self.get_region(x >> 9, z >> 9).and_then(|r| r.get_block(x & 511, y, z & 511))
    }

    /// `getLocationsOf`.
    pub fn get_locations_of(&mut self, block: &str, maximum: usize, center_x: i32, center_z: i32, max_region_distance_sq: i32) -> Vec<[i32; 3]> {
        let mut res = Vec::new();
        let (crx, crz) = (center_x >> 9, center_z >> 9);
        let mut search_radius = 0;
        while search_radius <= max_region_distance_sq {
            for xoff in -search_radius..=search_radius {
                for zoff in -search_radius..=search_radius {
                    if xoff * xoff + zoff * zoff != search_radius {
                        continue;
                    }
                    let region = self.get_or_create_region(xoff + crx, zoff + crz);
                    res.extend(region.locations_of(block));
                }
            }
            if res.len() >= maximum {
                return res;
            }
            search_radius += 1;
        }
        res
    }

    /// `updateCachedChunk`.
    pub fn update_cached_chunk(&mut self, chunk: CachedChunk) {
        let (cx, cz) = (chunk.x, chunk.z);
        self.get_or_create_region(cx >> 5, cz >> 5).update_cached_chunk(cx & 31, cz & 31, chunk);
    }

    /// `save`: the regions that changed go to the store (gzipped by it), expired chunks go, far regions leave RAM.
    pub fn save(&mut self, settings: &CacheSettings, now_ms: i64, prune_center: [i32; 3]) {
        let expiry = settings.cached_chunks_expiry_seconds;
        if settings.chunk_caching {
            for region in self.regions.values_mut() {
                if !region.has_unsaved_changes() {
                    continue;
                }
                region.remove_expired(expiry, now_ms);
                self.store.save(region.x(), region.z(), &region.to_bytes());
                region.mark_saved();
            }
        } else {
            // Even if we aren't saving to disk, still delete expired old chunks from RAM.
            for region in self.regions.values_mut() {
                region.remove_expired(expiry, now_ms);
            }
        }
        self.prune(settings, prune_center);
    }

    /// `prune`: delete regions that are too far from the player.
    fn prune(&mut self, settings: &CacheSettings, center: [i32; 3]) {
        if !settings.prune_regions_from_ram {
            return;
        }
        self.regions.retain(|_, region| {
            let dist_x = f64::from(((region.x() << 9) + 256) - center[0]);
            let dist_z = f64::from(((region.z() << 9) + 256) - center[2]);
            (dist_x * dist_x + dist_z * dist_z).sqrt() <= 1024.0
        });
    }

    #[allow(dead_code)]
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A 16 x 384 x 16 chunk of stone up to y = 63 with a chest in it and water on top at one column.
    struct Flat;

    impl PackSource for Flat {
        fn x(&self) -> i32 {
            33
        }
        fn z(&self) -> i32 {
            -2
        }
        fn min_y(&self) -> i32 {
            -64
        }
        fn height(&self) -> i32 {
            384
        }
        fn section_has_only_air(&self, section: i32) -> bool {
            section > 8
        }
        fn block_name(&self, x: i32, y: i32, z: i32) -> Arc<str> {
            if (x, y, z) == (3, 40, 4) {
                return Arc::from("minecraft:chest");
            }
            if y <= 63 { Arc::from("minecraft:stone") } else { Arc::from("minecraft:air") }
        }
        fn pathing_type(&self, x: i32, y: i32, z: i32) -> PathingBlockType {
            if y <= 63 || (x, y, z) == (3, 40, 4) { PathingBlockType::Solid } else { PathingBlockType::Air }
        }
    }

    struct Memory(Rc<RefCell<HashMap<(i32, i32), Vec<u8>>>>);

    impl RegionStore for Memory {
        fn load(&self, rx: i32, rz: i32) -> Option<Vec<u8>> {
            self.0.borrow().get(&(rx, rz)).cloned()
        }
        fn save(&self, rx: i32, rz: i32, bytes: &[u8]) {
            self.0.borrow_mut().insert((rx, rz), bytes.to_vec());
        }
    }

    #[test]
    fn a_packed_chunk_remembers_its_surface_its_chest_and_what_is_solid() {
        let chunk = pack(&Flat, 1000);
        let d = Dimension::overworld();
        assert_eq!(chunk.absolute_blocks("chest"), Some(vec![[3 + 33 * 16, 40, 4 + -2 * 16]]));
        // The surface is stone at y = 63 (adjusted 127), the air above is air, the middle is solid stone.
        assert_eq!(chunk.get_block(5, 127, 5, &d).as_deref(), Some("stone"));
        assert_eq!(chunk.get_block(5, 200, 5, &d).as_deref(), Some("air"));
        assert_eq!(chunk.get_block(5, 100, 5, &d).as_deref(), Some("stone"));
        assert_eq!(chunk.get_block(5, 20, 5, &d).as_deref(), Some("stone"));
        assert_eq!(chunk.get_block(3, 104, 4, &d).as_deref(), Some("chest"));
    }

    #[test]
    fn a_region_round_trips_through_its_file_format() {
        let shared = Rc::new(RefCell::new(HashMap::new()));
        let mut world = CachedWorld::new(Dimension::overworld(), Box::new(Memory(shared.clone())));
        world.update_cached_chunk(pack(&Flat, 5000));
        assert!(world.is_cached(33 * 16, -2 * 16));
        assert!(!world.is_cached(0, 0));
        let settings = CacheSettings { chunk_caching: true, prune_regions_from_ram: false, cached_chunks_expiry_seconds: -1 };
        world.save(&settings, 6000, [0, 0, 0]);
        assert_eq!(shared.borrow().len(), 1);

        let mut again = CachedWorld::new(Dimension::overworld(), Box::new(Memory(shared)));
        assert!(!again.region_loaded(33 * 16, -2 * 16));
        assert!(!again.is_cached(33 * 16, -2 * 16)); // not until the region is read
        again.try_load_from_disk(1, -1);
        assert!(again.is_cached(33 * 16, -2 * 16));
        assert_eq!(again.get_locations_of("chest", 1, 33 * 16, -2 * 16, 2), vec![[3 + 33 * 16, 40, 4 - 32]]);
        assert_eq!(again.get_block(33 * 16 + 5, 63, -2 * 16 + 5).as_deref(), Some("stone"));
    }

    #[test]
    fn expired_chunks_go_and_a_bad_file_is_refused() {
        let mut region = CachedRegion::new(1, 0, Dimension::overworld());
        region.update_cached_chunk(2, 3, pack(&Flat, 1000));
        region.remove_expired(10, 12_000);
        assert!(!region.is_cached(2 * 16, 3 * 16));
        let mut other = CachedRegion::new(0, 0, Dimension::overworld());
        assert!(other.load_bytes(&[0, 0, 0, 1]).is_err());
        assert!(other.load_bytes(&[]).is_err());
    }

    #[test]
    fn pathing_types_and_their_bits() {
        for t in [PathingBlockType::Air, PathingBlockType::Water, PathingBlockType::Avoid, PathingBlockType::Solid] {
            let (a, b) = t.bits();
            assert_eq!(PathingBlockType::from_bits(a, b), t);
        }
        assert_eq!(position_index(1, 2, 3), (1 << 1) | (3 << 5) | (2 << 9));
    }
}
