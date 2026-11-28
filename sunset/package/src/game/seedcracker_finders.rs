// The finders of kaptainwutax.seedcrackerX.finder (structure, decorator and BiomeFinder): each reads a chunk of the world,
// and what it recognises it hands to the DataStorage as data of a feature, with the outline to draw. Written from the Java
// finder by finder; where a finder looks (the search positions) is built in finder.rs, and the blocks of a structure in
// structures_dsl.rs. Not here yet: the finders of before 1.18 that need the lattice cracker (WarpedFungus, and Dungeon's
// floor reading with its block update exploit).
use super::sunset_scx_biome::{biomes, Category as BiomeCategory};
use super::sunset_scx_cracker_data::{BiomeData, DataAddedEvent, PillarData};
use super::sunset_scx_cracker_storage::DataStorage;
use super::sunset_scx_cracker_time_machine::Phase;
use super::sunset_scx_dimension::Dimension;
use super::sunset_scx_feature::{Feature, FeatureData, HeightContext};
use super::sunset_scx_finder::{Finder, FinderBase, FinderBuilder, FinderContext, FinderType};
use super::sunset_scx_finder_piece::{argb_color, Cuboid, PieceFinder, PieceKind};
use super::sunset_scx_finder_structures as dsl;
use super::sunset_scx_level::{biome_fixer_swap, BlockPos, ChunkPos, Direction, Level};
use super::sunset_scx_version::MCVersion;
use std::sync::Arc;

fn poke(phase: Phase) -> DataAddedEvent {
    Arc::new(move |s: &DataStorage| s.get_time_machine().poke(s, Some(phase)))
}

/// `world.getNoiseBiome((chunkX << 2) + 2, y, (chunkZ << 2) + 2)`, as the library's biome.
fn noise_biome_at(level: &dyn Level, chunk_pos: ChunkPos, y: i32) -> i32 {
    biome_fixer_swap(level.noise_biome((chunk_pos.x << 2) + 2, y, (chunk_pos.z << 2) + 2))
}

/// `BlockFinder.findInChunk`: the world positions in the chunk, from the local search positions, holding one of the blocks.
fn block_find(level: &dyn Level, chunk_pos: ChunkPos, positions: &[BlockPos], targets: &[&str]) -> Vec<BlockPos> {
    let origin = chunk_pos.get_world_position();
    let mut result = Vec::new();
    if !level.is_chunk_loaded(chunk_pos.x, chunk_pos.z) { return result; }
    for p in positions {
        if level.cancelled() { return Vec::new(); }
        let w = origin.offset_pos(*p);
        if targets.contains(&level.block(w.x, w.y, w.z)) {
            result.push(w);
        }
    }
    result
}

macro_rules! finder_base {
    () => {
        fn base(&self) -> &FinderBase {
            &self.base
        }
        fn base_mut(&mut self) -> &mut FinderBase {
            &mut self.base
        }
    };
}

fn structure_feature(f: &Option<Arc<Feature>>) -> Option<&Arc<Feature>> {
    f.as_ref()
}

fn is_valid_structure_biome(feature: &Arc<Feature>, biome: i32) -> bool {
    match &**feature {
        Feature::Structure(s) => s.is_valid_biome(biome),
        Feature::Decorator(d) => d.is_valid_biome(biome),
    }
}

// BuriedTreasureFinder

const CHEST_HOLDERS: [&str; 9] = ["sandstone", "stone", "andesite", "granite", "diorite", "coal_ore", "iron_ore", "gold_ore", "gravel"];

pub struct BuriedTreasureFinder {
    base: FinderBase,
}

impl BuriedTreasureFinder {
    pub fn create(_ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<Box<dyn Finder>> {
        vec![Box::new(BuriedTreasureFinder { base: FinderBase::new(chunk_pos, level.dimension()) })]
    }
}

impl Finder for BuriedTreasureFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.buried_treasure) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 64);
        if !is_valid_structure_biome(feature, biome) {
            return Vec::new();
        }
        let mut result = block_find(level, chunk_pos, &ctx.search.buried_treasure, &["chest"]);
        result.retain(|pos| {
            if level.property(pos.x, pos.y, pos.z, "waterlogged") == Some("true") {
                return false;
            }
            CHEST_HOLDERS.contains(&level.block(pos.x, pos.y - 1, pos.z))
        });
        for pos in &result {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let data = FeatureData::structure(feature, chunk_pos.x, chunk_pos.z);
            if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
                self.base.cuboids.push(Cuboid::block(*pos, argb_color(255, 255, 0)));
            }
        }
        result
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

// DesertWellFinder

const DESERT_WELL_SIZE: [i32; 3] = [5, 6, 5];

pub struct DesertWellFinder {
    base: FinderBase,
    piece: PieceFinder,
}

impl DesertWellFinder {
    fn new(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> DesertWellFinder {
        let mut piece = PieceFinder::new(PieceKind::Piece, Some(Direction::North), DESERT_WELL_SIZE);
        piece.search_positions = ctx.search.chunk_positions.clone();
        // buildStructure
        piece.fill_with_outline(0, 0, 0, 4, 1, 4, "sandstone", "sandstone", false);
        piece.fill_with_outline(1, 5, 1, 3, 5, 3, "sandstone_slab", "sandstone_slab", false);
        piece.add_block("sandstone", 2, 5, 2);
        let p1 = BlockPos::new(2, 1, 2);
        piece.add_block("water", p1.x, p1.y, p1.z);
        for facing in Direction::HORIZONTAL {
            let p2 = p1.relative(facing, 1);
            piece.add_block("water", p2.x, p2.y, p2.z);
        }
        DesertWellFinder { base: FinderBase::new(chunk_pos, level.dimension()), piece }
    }

    pub fn create(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        let offsets = [(0, 0), (-1, 0), (0, -1), (-1, -1), (1, 0), (0, 1), (1, 1), (1, -1), (-1, 1)];
        offsets.iter().map(|(dx, dz)| Box::new(DesertWellFinder::new(ctx, level, ChunkPos::new(c.x + dx, c.z + dz))) as Box<dyn Finder>).collect()
    }
}

impl Finder for DesertWellFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.desert_well) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 0);
        if !is_valid_structure_biome(feature, biome) {
            return Vec::new();
        }
        let result = self.piece.find_in_chunk(level, chunk_pos);
        for pos in &result {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let pos = pos.offset(2, 1, 2);
            let data = FeatureData::desert_well(feature, pos.x, pos.z);
            if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
                self.base.cuboids.push(Cuboid::sized(pos.offset(-2, -1, -2), DESERT_WELL_SIZE, argb_color(128, 128, 255)));
                self.base.cuboids.push(Cuboid::block(pos, argb_color(128, 128, 255)));
            }
        }
        result
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

// EndGatewayFinder

pub struct EndGatewayFinder {
    base: FinderBase,
}

impl EndGatewayFinder {
    pub fn create(_ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<Box<dyn Finder>> {
        vec![Box::new(EndGatewayFinder { base: FinderBase::new(chunk_pos, level.dimension()) })]
    }
}

fn find_gateway_height(level: &dyn Level, pos: BlockPos) -> i32 {
    let mut pos = pos;
    let mut height = 0;
    while pos.y >= 0 {
        if level.cancelled() { return -1; }
        pos = pos.below();
        height += 1;
        let block = level.block(pos.x, pos.y, pos.z);
        if block == "bedrock" || block != "end_stone" {
            continue;
        }
        break;
    }
    height - 1
}

impl Finder for EndGatewayFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.end_gateway) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 64);
        if !is_valid_structure_biome(feature, biome) {
            return Vec::new();
        }
        let result = block_find(level, chunk_pos, &ctx.search.chunk_positions, &["end_gateway"]);
        let mut new_result = Vec::new();
        for pos in result {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let height = find_gateway_height(level, pos);
            if (3..=9).contains(&height) {
                new_result.push(pos);
                let data = FeatureData::end_gateway(feature, pos.x, pos.z, height);
                if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
                    self.base.cuboids.push(Cuboid::sized(pos.offset(-1, -2, -1), [3, 5, 3], argb_color(102, 102, 210)));
                }
            }
        }
        new_result
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::End
    }
}

// EndPillarsFinder

pub struct EndPillarsFinder {
    base: FinderBase,
    already_found: bool,
    /// The chunk and the position of each pillar's bedrock marker.
    markers: Vec<ChunkPos>,
}

impl EndPillarsFinder {
    pub fn create(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<Box<dyn Finder>> {
        let already_found = ctx.storage.pillar_data.lock().unwrap_or_else(|e| e.into_inner()).is_some();
        let mut markers = Vec::new();
        if !already_found {
            let old = ctx.host().version().is_older_than(MCVersion::v1_14);
            for i in 0..10 {
                let angle = 2.0 * (-std::f64::consts::PI + (std::f64::consts::PI / 10.0) * i as f64);
                let (mut x, mut z) = (42.0 * angle.cos(), 42.0 * angle.sin());
                if old {
                    x = (x + 0.5).floor();
                    z = (z + 0.5).floor();
                }
                markers.push(ChunkPos::new((x.floor() as i32) >> 4, (z.floor() as i32) >> 4));
            }
        }
        vec![Box::new(EndPillarsFinder { base: FinderBase::new(chunk_pos, level.dimension()), already_found, markers })]
    }
}

impl Finder for EndPillarsFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let mut result = Vec::new();
        if self.markers.iter().any(|m| !level.is_chunk_loaded(m.x, m.z)) { return Vec::new(); }
        for marker in &self.markers {
            result.extend(block_find(level, *marker, &ctx.search.bedrock_marker, &["bedrock"]));
        }
        if !self.already_found && result.len() == self.markers.len() {
            let data = PillarData::new(result.iter().map(|p| p.y).collect());
            if ctx.storage.add_pillar_data(data, poke(Phase::Pillars)) {
                for pos in &result {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
                    self.base.cuboids.push(Cuboid::block(*pos, argb_color(128, 0, 128)));
                }
            }
        }
        result
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::End
    }
}

// EmeraldOreFinder

pub struct EmeraldOreFinder {
    base: FinderBase,
}

impl EmeraldOreFinder {
    pub fn create(_ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<Box<dyn Finder>> {
        vec![Box::new(EmeraldOreFinder { base: FinderBase::new(chunk_pos, level.dimension()) })]
    }
}

impl Finder for EmeraldOreFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.emerald_ore) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 0);
        let result = block_find(level, chunk_pos, &ctx.search.emerald_ore, &["emerald_ore"]);
        let Some(pos) = result.first().copied() else { return result };
        let data = FeatureData::emerald_ore(feature, pos.x, pos.y, pos.z, Some(biome));
        if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
            self.base.cuboids.push(Cuboid::block(pos, argb_color(0, 255, 0)));
        }
        result
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

// BiomeFinder

pub struct BiomeFinder {
    base: FinderBase,
}

impl BiomeFinder {
    pub fn create(_ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<Box<dyn Finder>> {
        vec![Box::new(BiomeFinder { base: FinderBase::new(chunk_pos, level.dimension()) })]
    }
}

impl Finder for BiomeFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let mut result = Vec::new();
        let version = ctx.host().version();
        let origin = self.base.chunk_pos.get_world_position();
        let mut x = 0;
        while x < 16 {
            let mut z = 0;
            while z < 16 {
                if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
                let block_pos = origin.offset(x, 0, z);
                let path = if version.is_newer_or_equal_to(MCVersion::v1_15) {
                    level.noise_biome(block_pos.x >> 2, 0, block_pos.z >> 2)
                } else {
                    level.biome(block_pos.x, block_pos.y, block_pos.z)
                };
                let other = biome_fixer_swap(path);
                if other != biomes::THE_VOID {
                    let data = if version.is_newer_or_equal_to(MCVersion::v1_15) {
                        BiomeData::new(other, block_pos.x >> 2, block_pos.z >> 2)
                    } else {
                        BiomeData::new(other, block_pos.x, block_pos.z)
                    };
                    if ctx.storage.add_biome_data(data, poke(Phase::Biomes)) {
                        let surface = BlockPos::new(block_pos.x, level.world_surface_height(block_pos.x, block_pos.z) - 1, block_pos.z);
                        if ctx.host().debug_enabled() {
                            let name = super::sunset_scx_biome::get(other).map_or("", |b| b.name);
                            ctx.host().warn(&format!("{}, {}, {}, {name}", surface.x, surface.y, surface.z), &[]);
                        }
                        result.push(surface);
                    }
                }
                z += 4;
            }
            x += 4;
        }
        for pos in &result {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            self.base.cuboids.push(Cuboid::block(*pos, argb_color(51, 204, 128)));
        }
        result
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

// The finders that look for a structure's blocks: temples, igloo, monument, end city, outpost, trial chambers.

type Builder = fn(&mut PieceFinder);

fn pieces(kind: PieceKind, size: [i32; 3], positions: impl Fn(Direction) -> Arc<Vec<BlockPos>>, build: Builder) -> Vec<PieceFinder> {
    Direction::HORIZONTAL
        .iter()
        .map(|d| {
            let mut p = PieceFinder::new(kind, Some(*d), size);
            p.search_positions = positions(*d);
            build(&mut p);
            p
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TempleKind {
    DesertPyramid,
    JunglePyramid,
    SwampHut,
}

pub struct TempleFinder {
    base: FinderBase,
    kind: TempleKind,
    pieces: Vec<PieceFinder>,
}

impl TempleFinder {
    fn new(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos, kind: TempleKind) -> TempleFinder {
        let (size, build): ([i32; 3], Builder) = match kind {
            TempleKind::DesertPyramid => ([21, 15, 21], dsl::desert_pyramid),
            TempleKind::JunglePyramid => ([12, 10, 15], dsl::jungle_pyramid),
            TempleKind::SwampHut => ([7, 7, 9], dsl::swamp_hut),
        };
        let temple = ctx.search.temple.clone();
        TempleFinder { base: FinderBase::new(chunk_pos, level.dimension()), kind, pieces: pieces(PieceKind::Piece, size, |_| temple.clone(), build) }
    }

    pub fn create_desert_pyramid(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        [(0, 0), (-1, 0), (0, -1), (-1, -1)].iter().map(|(dx, dz)| Box::new(TempleFinder::new(ctx, level, ChunkPos::new(c.x + dx, c.z + dz), TempleKind::DesertPyramid)) as Box<dyn Finder>).collect()
    }

    pub fn create_jungle_pyramid(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        vec![Box::new(TempleFinder::new(ctx, level, c, TempleKind::JunglePyramid))]
    }

    pub fn create_swamp_hut(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        vec![Box::new(TempleFinder::new(ctx, level, c, TempleKind::SwampHut))]
    }
}

impl Finder for TempleFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let feature = match self.kind {
            TempleKind::DesertPyramid => &ctx.features.desert_pyramid,
            TempleKind::JunglePyramid => &ctx.features.jungle_pyramid,
            TempleKind::SwampHut => &ctx.features.swamp_hut,
        };
        let Some(feature) = structure_feature(feature) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let mut combined = Vec::new();
        for piece in &self.pieces {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            // findInChunkPiece: the biome of the chunk first.
            let biome = noise_biome_at(level, chunk_pos, 64);
            let positions = if is_valid_structure_biome(feature, biome) { piece.find_in_chunk(level, chunk_pos) } else { Vec::new() };
            for pos in &positions {
                let data = FeatureData::structure(feature, chunk_pos.x, chunk_pos.z);
                if ctx.storage.add_base_data(data, poke(Phase::Lifting)) {
                    // addRenderers
                    self.base.cuboids.push(Cuboid::sized(*pos, piece.get_layout(), argb_color(255, 0, 255)));
                    let chunk_start = BlockPos::new(pos.x & -16, pos.y, pos.z & -16);
                    self.base.cuboids.push(Cuboid::block(chunk_start, argb_color(255, 0, 255)));
                }
            }
            combined.extend(positions);
        }
        combined
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

pub struct IglooFinder {
    base: FinderBase,
    pieces: Vec<PieceFinder>,
}

impl IglooFinder {
    fn new(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> IglooFinder {
        let search = ctx.search.clone();
        IglooFinder { base: FinderBase::new(chunk_pos, level.dimension()), pieces: pieces(PieceKind::Jigsaw, super::sunset_scx_finder::IGLOO_SIZE, |d| search.igloo[&d].clone(), dsl::igloo) }
    }

    pub fn create(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        vec![Box::new(IglooFinder::new(ctx, level, c)), Box::new(IglooFinder::new(ctx, level, ChunkPos::new(c.x + 1, c.z)))]
    }
}

impl Finder for IglooFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.igloo) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let mut combined = Vec::new();
        for piece in &self.pieces {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let positions = piece.find_in_chunk(level, chunk_pos);
            for pos in &positions {
                let data = FeatureData::structure(feature, chunk_pos.x, chunk_pos.z);
                if ctx.storage.add_base_data(data, poke(Phase::Lifting)) {
                    self.base.cuboids.push(Cuboid::sized(*pos, piece.get_layout(), argb_color(176, 207, 252)));
                    self.base.cuboids.push(Cuboid::block(chunk_pos.get_world_position().offset(0, pos.y, 0), argb_color(176, 207, 252)));
                }
            }
            combined.extend(positions);
        }
        combined
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

pub struct MonumentFinder {
    base: FinderBase,
    pieces: Vec<PieceFinder>,
}

impl MonumentFinder {
    fn new(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> MonumentFinder {
        let mut piece = PieceFinder::new(PieceKind::Piece, Some(Direction::North), [8, 5, 8]);
        piece.search_positions = ctx.search.monument.clone();
        dsl::monument(&mut piece);
        MonumentFinder { base: FinderBase::new(chunk_pos, level.dimension()), pieces: vec![piece] }
    }

    pub fn create(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        [(0, 0), (-1, 0), (0, -1), (-1, -1)].iter().map(|(dx, dz)| Box::new(MonumentFinder::new(ctx, level, ChunkPos::new(c.x + dx, c.z + dz))) as Box<dyn Finder>).collect()
    }
}

impl Finder for MonumentFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.monument) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 64);
        if super::sunset_scx_biome::get(biome).map(|b| b.category) != Some(BiomeCategory::Ocean) {
            return Vec::new();
        }
        let mut combined = Vec::new();
        for piece in &self.pieces {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let positions = piece.find_in_chunk(level, chunk_pos);
            for pos in &positions {
                let monument_start = ChunkPos::new(chunk_pos.x + 1, chunk_pos.z + 1);
                let data = FeatureData::structure(feature, monument_start.x, monument_start.z);
                if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
                    self.base.cuboids.push(Cuboid::sized(*pos, piece.get_layout(), argb_color(0, 0, 255)));
                    self.base.cuboids.push(Cuboid::block(monument_start.get_world_position().offset(0, pos.y, 0), argb_color(0, 0, 255)));
                }
            }
            combined.extend(positions);
        }
        combined
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

pub struct EndCityFinder {
    base: FinderBase,
    pieces: Vec<PieceFinder>,
}

impl EndCityFinder {
    fn new(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> EndCityFinder {
        let positions = ctx.search.end_city.clone();
        EndCityFinder { base: FinderBase::new(chunk_pos, level.dimension()), pieces: pieces(PieceKind::Piece, [8, 4, 8], |_| positions.clone(), dsl::end_city) }
    }

    pub fn create(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        [(0, 0), (-1, 0), (0, -1), (-1, -1)].iter().map(|(dx, dz)| Box::new(EndCityFinder::new(ctx, level, ChunkPos::new(c.x + dx, c.z + dz))) as Box<dyn Finder>).collect()
    }
}

impl Finder for EndCityFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.end_city) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 64);
        if !is_valid_structure_biome(feature, biome) {
            return Vec::new();
        }
        let version = ctx.host().version();
        let mut combined = Vec::new();
        for piece in &self.pieces {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let positions = piece.find_in_chunk(level, chunk_pos);
            for pos in &positions {
                if version.is_newer_or_equal_to(MCVersion::v1_19) {
                    let pos_fix = pos.offset(1, 0, 1);
                    let data = FeatureData::structure(feature, pos_fix.x >> 4, pos_fix.z >> 4);
                    if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
                        self.base.cuboids.push(Cuboid::sized(*pos, piece.get_layout(), argb_color(153, 0, 153)));
                        self.base.cuboids.push(Cuboid::block(pos_fix, argb_color(153, 0, 153)));
                    }
                } else {
                    let data = FeatureData::structure(feature, chunk_pos.x, chunk_pos.z);
                    if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
                        self.base.cuboids.push(Cuboid::sized(*pos, piece.get_layout(), argb_color(153, 0, 153)));
                        self.base.cuboids.push(Cuboid::block(*pos, argb_color(153, 0, 153)));
                    }
                }
            }
            combined.extend(positions);
        }
        combined
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::End
    }
}

pub struct OutpostFinder {
    base: FinderBase,
    pieces: Vec<PieceFinder>,
}

impl OutpostFinder {
    fn new(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> OutpostFinder {
        let search = ctx.search.clone();
        OutpostFinder { base: FinderBase::new(chunk_pos, level.dimension()), pieces: pieces(PieceKind::Jigsaw, super::sunset_scx_finder::OUTPOST_SIZE, |d| search.outpost[&d].clone(), dsl::outpost) }
    }

    pub fn create(ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        [(0, 0), (0, 1), (1, 0), (1, 1)].iter().map(|(dx, dz)| Box::new(OutpostFinder::new(ctx, level, ChunkPos::new(c.x + dx, c.z + dz))) as Box<dyn Finder>).collect()
    }
}

impl Finder for OutpostFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.pillager_outpost) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 64);
        if !is_valid_structure_biome(feature, biome) {
            return Vec::new();
        }
        let mut combined = Vec::new();
        for piece in &self.pieces {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let positions = piece.find_in_chunk(level, chunk_pos);
            for pos in &positions {
                let data = FeatureData::structure(feature, chunk_pos.x, chunk_pos.z);
                if ctx.storage.add_base_data(data, poke(Phase::Lifting)) {
                    self.base.cuboids.push(Cuboid::sized(*pos, piece.get_layout(), argb_color(170, 84, 3)));
                    self.base.cuboids.push(Cuboid::block(chunk_pos.get_world_position().offset(0, pos.y, 0), argb_color(170, 84, 3)));
                }
            }
            combined.extend(positions);
        }
        combined
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

pub struct TrialChambersFinder {
    base: FinderBase,
    pieces: Vec<PieceFinder>,
}

impl TrialChambersFinder {
    pub fn create(ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<Box<dyn Finder>> {
        let mut pieces = Vec::new();
        for d in Direction::HORIZONTAL {
            for build in [dsl::trial_chambers_end1 as Builder, dsl::trial_chambers_end2 as Builder] {
                let mut p = PieceFinder::new(PieceKind::Jigsaw, Some(d), super::sunset_scx_finder::TRIAL_CHAMBERS_SIZE);
                p.search_positions = ctx.search.trial_chambers[&d].clone();
                build(&mut p);
                pieces.push(p);
            }
        }
        vec![Box::new(TrialChambersFinder { base: FinderBase::new(chunk_pos, level.dimension()), pieces })]
    }
}

impl Finder for TrialChambersFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.trial_chambers) else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        if level.noise_biome((chunk_pos.x << 2) + 2, 64, (chunk_pos.z << 2) + 2) == "deep_dark" {
            return Vec::new();
        }
        let mut combined = Vec::new();
        for piece in &self.pieces {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            let positions = piece.find_in_chunk(level, chunk_pos);
            for pos in &positions {
                let data = FeatureData::structure(feature, chunk_pos.x, chunk_pos.z);
                if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
                    self.base.cuboids.push(Cuboid::sized(*pos, piece.get_layout(), argb_color(170, 84, 3)));
                    self.base.cuboids.push(Cuboid::block(chunk_pos.get_world_position().offset(0, pos.y, 0), argb_color(170, 84, 3)));
                }
            }
            combined.extend(positions);
        }
        combined
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

// ShipwreckFinder

pub struct ShipwreckFinder {
    base: FinderBase,
}

impl ShipwreckFinder {
    pub fn create(_ctx: &FinderContext, level: &dyn Level, c: ChunkPos) -> Vec<Box<dyn Finder>> {
        // The Java lists the chunk (-1, -1) twice and never (1, -1); kept as it is.
        let offsets = [(0, 0), (-1, 0), (0, -1), (-1, -1), (1, 0), (0, 1), (1, 1), (-1, -1), (-1, 1)];
        offsets.iter().map(|(dx, dz)| Box::new(ShipwreckFinder { base: FinderBase::new(ChunkPos::new(c.x + dx, c.z + dz), level.dimension()) }) as Box<dyn Finder>).collect()
    }

    fn on_chest_found(&mut self, ctx: &FinderContext, level: &dyn Level, feature: &Arc<Feature>, pos: BlockPos) -> bool {
        let Some(chest_facing) = level.property(pos.x, pos.y, pos.z, "facing").and_then(Direction::from_name) else { return false };
        let mut stairs = [0i32; 4];
        let mut total_stairs = 0;
        let mut trapdoors = [0i32; 4];
        let mut total_trapdoors = 0;
        for y in -1..=2 {
            for x in -1..=1 {
                for z in -1..=1 {
                    if x == 0 && y == 0 && z == 0 {
                        continue;
                    }
                    if level.cancelled() || !level.is_chunk_loaded((pos.x + x) >> 4, (pos.z + z) >> 4) { return false; }
                    let neighbor = level.block(pos.x + x, pos.y + y, pos.z + z);
                    if neighbor == "void_air" {
                        return false;
                    }
                    if neighbor.ends_with("_stairs") {
                        stairs[(y + 1) as usize] += 1;
                        total_stairs += 1;
                    } else if neighbor.ends_with("_trapdoor") {
                        trapdoors[(y + 1) as usize] += 1;
                        total_trapdoors += 1;
                    }
                }
            }
        }
        let mut chest_x = 4;
        let mut chest_y = 2;
        let mut chest_z = 0;
        let mut length = 16;
        let mut height = 9;
        let mut direction = chest_facing;
        if trapdoors[3] > 4 {
            // with_mast[_degraded]
            chest_z = 9;
            height = 21;
            length = 28;
        } else if total_trapdoors == 0 && stairs[3] == 3 {
            // upsidedown_backhalf[_degraded]
            if stairs[0] == 0 {
                chest_x = 2;
                chest_z = 12;
                direction = chest_facing.get_opposite();
            } else {
                // upsidedown front
                chest_x = 3;
                chest_y = 5;
                chest_z = 17;
                length = 22;
                direction = chest_facing.get_clockwise();
            }
        } else if total_trapdoors == 0 {
            // rightsideup that have backhalf
            if stairs[0] == 4 {
                if total_stairs > 4 {
                    chest_x = 6;
                    chest_y = 4;
                    chest_z = 12;
                    direction = chest_facing.get_opposite();
                } else {
                    // sideways backhalf
                    chest_x = 6;
                    chest_y = 3;
                    chest_z = 8;
                    length = 17;
                    direction = chest_facing.get_opposite();
                }
            } else if stairs[0] == 3 && total_stairs > 5 {
                chest_x = 5;
                chest_z = 6;
                direction = chest_facing.get_counter_clockwise();
            } else if total_stairs == 3 && stairs[0] == 1 && stairs[2] == 1 && stairs[3] == 1 {
                // upsidedown full
                chest_x = 3;
                chest_y = 5;
                chest_z = 17;
                length = 28;
                direction = chest_facing.get_clockwise();
            }
            let mut m = pos;
            m = m.offset(0, -chest_y, 0);
            m = m.relative(direction.get_clockwise(), chest_x - 4);
            m = m.relative(direction, -chest_z - 1);
            if level.cancelled() || !level.is_chunk_loaded(m.x >> 4, m.z >> 4) { return false; }
            if level.block_in_tag(m.x, m.y, m.z, "mineable_with_axe") {
                if length == 17 {
                    // sideways
                    chest_z += 11;
                    length += 11;
                } else {
                    chest_z += 12;
                    length += 12;
                }
                m = m.offset(0, 10, 0);
                if level.block_in_tag(m.x, m.y, m.z, "logs") {
                    height = 21;
                }
            }
        } else if total_trapdoors == 2 && trapdoors[3] == 2 && stairs[3] == 3 {
            // rightsideup_fronthalf[_degraded]
            chest_z = 8;
            length = 24;
        } else if total_trapdoors == 2 && total_stairs == 4 && stairs[0] == 2 {
            // sideways-fronthalf
            chest_x = 5;
            chest_y = 3;
            chest_z = 8;
            length = 24;
        }
        if chest_z != 0 {
            let mut m = pos;
            m = m.relative(direction, 15 - chest_z);
            m = m.relative(direction.get_clockwise(), chest_x - 4);
            let mut pos2 = m;
            pos2 = pos2.offset(0, -chest_y, 0);
            pos2 = pos2.relative(direction, -15);
            pos2 = pos2.relative(direction.get_clockwise(), 4);
            let mut pos3 = pos2;
            pos3 = pos3.relative(direction, length - 1);
            pos3 = pos3.relative(direction.get_clockwise(), -8);
            pos3 = pos3.offset(0, height - 1, 0);
            let (bx0, bz0) = (pos2.x.min(pos3.x), pos2.z.min(pos3.z));
            let (bx1, bz1) = (pos2.x.max(pos3.x) + 1, pos2.z.max(pos3.z) + 1);
            m = m.offset(-4, -chest_y, -15);
            if (m.x & 0xf) == 0 && (m.z & 0xf) == 0 {
                let chunk = ChunkPos::containing(m);
                let data = FeatureData::structure(feature, chunk.x, chunk.z);
                if ctx.storage.add_base_data(data, poke(Phase::Lifting)) {
                    self.base.cuboids.push(Cuboid::bounding_box(BlockPos::new(bx0, pos2.y, bz0), BlockPos::new(bx1, pos3.y + 1, bz1), argb_color(0, 255, 255)));
                    self.base.cuboids.push(Cuboid::block(chunk.get_world_position().offset(0, m.y, 0), argb_color(0, 255, 255)));
                    return true;
                }
            }
        }
        false
    }
}

impl Finder for ShipwreckFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let Some(feature) = structure_feature(&ctx.features.shipwreck).cloned() else { return Vec::new() };
        let chunk_pos = self.base.chunk_pos;
        let biome = noise_biome_at(level, chunk_pos, 64);
        if !is_valid_structure_biome(&feature, biome) {
            return Vec::new();
        }
        let mut result = block_find(level, chunk_pos, &ctx.search.chunk_positions, &["chest"]);
        let mut kept = Vec::new();
        for pos in result.drain(..) {
            if ctx.storage.cancelled() || level.cancelled() { return Vec::new(); }
            if level.property(pos.x, pos.y, pos.z, "type") != Some("single") {
                continue;
            }
            if !level.has_block_entity(pos.x, pos.y, pos.z, "chest") {
                continue;
            }
            if self.on_chest_found(ctx, level, &feature, pos) {
                kept.push(pos);
            }
        }
        kept
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

// DungeonFinder

pub struct DungeonFinder {
    base: FinderBase,
}

impl DungeonFinder {
    pub fn create(_ctx: &FinderContext, level: &dyn Level, chunk_pos: ChunkPos) -> Vec<Box<dyn Finder>> {
        let mut finders: Vec<Box<dyn Finder>> = Vec::new();
        for chunk_x in chunk_pos.x - 1..=chunk_pos.x + 1 {
            for chunk_z in chunk_pos.z - 1..=chunk_pos.z + 1 {
                if surrounding_chunks_loaded(chunk_x, chunk_z, level) {
                    finders.push(Box::new(DungeonFinder { base: FinderBase::new(ChunkPos::new(chunk_x, chunk_z), level.dimension()) }));
                }
            }
        }
        finders
    }

    /// `getDungeonSize`: 4 where more than two of the blocks along the wall are cobblestone.
    fn get_dungeon_size(level: &dyn Level, spawner: BlockPos) -> [i32; 3] {
        let count = |from: BlockPos, to: BlockPos| {
            let mut n = 0;
            for x in from.x..=to.x {
                for z in from.z..=to.z {
                    for y in from.y..=to.y {
                        if level.block(x, y, z) == "cobblestone" {
                            n += 1;
                        }
                    }
                }
            }
            n
        };
        let x = if count(spawner.offset(4, 3, -4), spawner.offset(4, 3, 4)) > 2 { 4 } else { 3 };
        let z = if count(spawner.offset(-4, 3, 4), spawner.offset(4, 3, 4)) > 2 { 4 } else { 3 };
        [x, 0, z]
    }

    /// `getFloorCalls`.
    fn get_floor_calls(level: &dyn Level, size: [i32; 3], spawner: BlockPos) -> Vec<i32> {
        let mut calls = Vec::new();
        for xo in -size[0]..=size[0] {
            for zo in -size[2]..=size[2] {
                let block = level.block(spawner.x + xo, spawner.y - 1, spawner.z + zo);
                calls.push(match block {
                    "mossy_cobblestone" => super::sunset_scx_feature::MOSSY_COBBLESTONE_CALL,
                    "cobblestone" => super::sunset_scx_feature::COBBLESTONE_CALL,
                    "air" | "cave_air" => 3,
                    _ => 2,
                });
            }
        }
        calls
    }
}

fn surrounding_chunks_loaded(chunk_x: i32, chunk_z: i32, level: &dyn Level) -> bool {
    for x in chunk_x - 1..=chunk_x + 1 {
        for z in chunk_z - 1..=chunk_z + 1 {
            if !level.is_chunk_loaded(x, z) {
                return false;
            }
        }
    }
    true
}

impl Finder for DungeonFinder {
    finder_base!();

    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos> {
        if ctx.storage.cancelled() || level.cancelled() || !level.is_chunk_loaded(self.base.chunk_pos.x, self.base.chunk_pos.z) { return Vec::new(); }
        let chunk_pos = self.base.chunk_pos;
        if !surrounding_chunks_loaded(chunk_pos.x, chunk_pos.z, level) { return Vec::new(); }
        let mut result = block_find(level, chunk_pos, &ctx.search.chunk_positions, &["spawner"]);
        if result.len() != 1 {
            return Vec::new();
        }
        result.retain(|pos| {
            if !level.has_block_entity(pos.x, pos.y, pos.z, "spawner") {
                return false;
            }
            let mut count = 0;
            for x in -4..=4 {
                for z in -4..=4 {
                    let b = level.block(pos.x + x, pos.y - 1, pos.z + z);
                    if b == "cobblestone" || b == "mossy_cobblestone" {
                        count += 1;
                    }
                }
            }
            count >= 20
        });
        if result.len() != 1 {
            return Vec::new();
        }
        let biome = noise_biome_at(level, chunk_pos, 0);
        let pos = result[0];
        let version = ctx.host().version();
        if version.is_newer_than(MCVersion::v1_17_1) {
            let data = if pos.y < 0 {
                let Some(feature) = &ctx.features.deep_dungeon else { return result };
                FeatureData::deep_dungeon(feature, pos.x, pos.y, pos.z, Some(biome))
            } else {
                let Some(feature) = &ctx.features.dungeon else { return result };
                FeatureData::dungeon(feature, pos.x, pos.y, pos.z, None, None, Some(biome), None)
            };
            self.base.cuboids.push(Cuboid::block(pos, argb_color(255, 0, 0)));
            ctx.storage.add_base_data(data, poke(Phase::Biomes));
            return result;
        }
        let size = Self::get_dungeon_size(level, pos);
        let floor_calls = Self::get_floor_calls(level, size, pos);
        let Some(feature) = &ctx.features.dungeon else { return result };
        let height_context = HeightContext::new(ctx.search.height_context.bottom_y, ctx.search.height_context.top_y);
        let data = FeatureData::dungeon(feature, pos.x, pos.y, pos.z, Some(size), Some(floor_calls), Some(biome), Some(height_context));
        // The Java asks the dungeon itself what to do when it is stored (Dungeon.Data.onDataAdded), which for these versions
        // is the lattice search of the dungeon floor; that is the last thing to port. Until then it starts the structure search.
        let uses_floor = matches!(&data.detail, super::sunset_scx_feature::Detail::Decorator(super::sunset_scx_feature::DecoratorDetail::Dungeon(d)) if d.uses_floor());
        if ctx.storage.add_base_data(data, poke(Phase::Structures)) {
            self.base.cuboids.push(Cuboid::block(pos, argb_color(255, 0, 0)));
            if uses_floor {
                self.base.cuboids.push(Cuboid::sized(pos.offset(-size[0], -1, -size[2]), [size[0] * 2 + 1, 0, size[2] * 2 + 1], argb_color(255, 0, 0)));
            }
        }
        result
    }

    fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        dimension == Dimension::Overworld
    }
}

/// `Finder.Type.finderBuilder` for each type that is written; the finder of the warped fungus is not yet.
pub static BUILDERS: [(FinderType, FinderBuilder); 16] = [
    (FinderType::BuriedTreasure, BuriedTreasureFinder::create),
    (FinderType::DesertTemple, TempleFinder::create_desert_pyramid),
    (FinderType::EndCity, EndCityFinder::create),
    (FinderType::JungleTemple, TempleFinder::create_jungle_pyramid),
    (FinderType::Monument, MonumentFinder::create),
    (FinderType::SwampHut, TempleFinder::create_swamp_hut),
    (FinderType::Shipwreck, ShipwreckFinder::create),
    (FinderType::PillagerOutpost, OutpostFinder::create),
    (FinderType::Igloo, IglooFinder::create),
    (FinderType::TrialChambers, TrialChambersFinder::create),
    (FinderType::EndPillars, EndPillarsFinder::create),
    (FinderType::EndGateway, EndGatewayFinder::create),
    (FinderType::Dungeon, DungeonFinder::create),
    (FinderType::EmeraldOre, EmeraldOreFinder::create),
    (FinderType::DesertWell, DesertWellFinder::create),
    (FinderType::Biome, BiomeFinder::create),
];
