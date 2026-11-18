// What the cracker asks of a feature: the structures and decorators of com.seedfinding.mcfeature (Feature, Structure,
// RegionStructure, UniformStructure, TriangularStructure, OldStructure and the classes under them that SeedCrackerX uses)
// and the decorators of kaptainwutax.seedcrackerX.cracker.decorator (Dungeon, DeepDungeon, EmeraldOre) with the
// BiomelessDecorators DesertWell and EndGateway. A feature is an `Arc<Feature>` and two data of the same feature are the same
// `Arc`, as `Entry.equals` compares `d1.feature == d2.feature`. What the Java does with a BiomeSource or a TerrainGenerator
// (`canSpawn`, `canGenerate`) is not here: the cracker never calls it.
use super::sunset_scx_biome::biomes;
use super::sunset_scx_dimension::Dimension;
use super::sunset_scx_rand::ChunkRand;
use super::sunset_scx_version::{MCVersion, VersionMap};
use super::sunset_scx_worldgen_random::WorldgenRandom;
use std::sync::Arc;

/// `CPos`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CPos {
    pub x: i32,
    pub z: i32,
}

impl CPos {
    pub fn new(x: i32, z: i32) -> CPos {
        CPos { x, z }
    }

    /// `distanceTo(other, DistanceMetric.CHEBYSHEV)`.
    pub fn chebyshev_distance_to(self, other: CPos) -> i32 {
        (self.x - other.x).abs().max((self.z - other.z).abs())
    }
}

/// The structures of the Java's `structure` package that SeedCrackerX uses, and Village for the outpost's check.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StructureClass {
    BuriedTreasure,
    DesertPyramid,
    EndCity,
    Igloo,
    JunglePyramid,
    Monument,
    PillagerOutpost,
    Shipwreck,
    SwampHut,
    TrialChambers,
    Village,
}

/// The shape of the offset inside a region: `UniformStructure`, `TriangularStructure`, or the single chance of
/// BuriedTreasure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionShape {
    Uniform,
    Triangular,
    Treasure,
}

/// `RegionStructure.Config` (with BuriedTreasure's `chance`).
#[derive(Clone, Copy, Debug)]
pub struct RegionConfig {
    pub spacing: i32,
    pub separation: i32,
    pub salt: i32,
    pub chance: f32,
}

fn old(salt: i32) -> RegionConfig {
    // OldStructure.Config: SPACING 32, SEPARATION 8.
    RegionConfig { spacing: 32, separation: 8, salt, chance: 0.0 }
}

fn region(spacing: i32, separation: i32, salt: i32) -> RegionConfig {
    RegionConfig { spacing, separation, salt, chance: 0.0 }
}

impl StructureClass {
    pub fn name(self) -> &'static str {
        match self {
            StructureClass::BuriedTreasure => "buried_treasure",
            StructureClass::DesertPyramid => "desert_pyramid",
            StructureClass::EndCity => "end_city",
            StructureClass::Igloo => "igloo",
            StructureClass::JunglePyramid => "jungle_pyramid",
            StructureClass::Monument => "monument",
            StructureClass::PillagerOutpost => "pillager_outpost",
            StructureClass::Shipwreck => "shipwreck",
            StructureClass::SwampHut => "swamp_hut",
            StructureClass::TrialChambers => "trial_chambers",
            StructureClass::Village => "village",
        }
    }

    pub fn shape(self) -> RegionShape {
        match self {
            StructureClass::BuriedTreasure => RegionShape::Treasure,
            StructureClass::EndCity | StructureClass::Monument => RegionShape::Triangular,
            _ => RegionShape::Uniform,
        }
    }

    /// `instanceof OldStructure`: the structures of a 32 by 8 region.
    pub fn is_old_structure(self) -> bool {
        matches!(self, StructureClass::DesertPyramid | StructureClass::Igloo | StructureClass::JunglePyramid | StructureClass::SwampHut | StructureClass::PillagerOutpost | StructureClass::Village)
    }

    pub fn valid_dimension(self) -> Dimension {
        if self == StructureClass::EndCity { Dimension::End } else { Dimension::Overworld }
    }

    /// The `CONFIGS` map of the class: the config that holds from each version on.
    fn configs(self) -> VersionMap<RegionConfig> {
        use MCVersion::*;
        match self {
            StructureClass::BuriedTreasure => VersionMap::new().add(v1_13, RegionConfig { spacing: 1, separation: 0, salt: 10387320, chance: 0.01 }),
            StructureClass::DesertPyramid => VersionMap::new().add(v1_8, old(14357617)),
            StructureClass::Igloo => VersionMap::new().add(v1_9, old(14357617)).add(v1_13, old(14357618)),
            StructureClass::JunglePyramid => VersionMap::new().add(v1_8, old(14357617)).add(v1_13, old(14357619)),
            StructureClass::SwampHut => VersionMap::new().add(v1_8, old(14357617)).add(v1_13, old(14357620)),
            StructureClass::Shipwreck => VersionMap::new().add(v1_13, region(15, 8, 165745295)).add(v1_13_1, region(16, 8, 165745295)).add(v1_16, region(24, 4, 165745295)),
            StructureClass::PillagerOutpost => VersionMap::new().add(v1_14, old(165745296)),
            StructureClass::Monument => VersionMap::new().add(v1_8, region(32, 5, 10387313)),
            StructureClass::EndCity => VersionMap::new().add(v1_9, region(20, 11, 10387313)),
            StructureClass::TrialChambers => VersionMap::new().add(v1_21, region(34, 12, 94251327)),
            StructureClass::Village => VersionMap::new().add(v1_8, old(10387312)).add(v1_18, region(34, 8, 10387312)),
        }
    }
}

/// A RegionStructure of one class for one version.
#[derive(Debug)]
pub struct Structure {
    pub class: StructureClass,
    pub version: MCVersion,
    pub config: RegionConfig,
    /// UniformStructure's `offset` (spacing - separation) or TriangularStructure's `peak`.
    pub offset: i32,
    /// PillagerOutpost's `village`.
    pub village: Option<Box<Structure>>,
}

impl Structure {
    /// `new Igloo(version)` and the rest: `None` where the class has no config for the version, which is where the Java
    /// throws, and Features.safe turns that into a disabled finder.
    pub fn new(class: StructureClass, version: MCVersion) -> Option<Structure> {
        let config = *class.configs().get_as_of(version)?;
        let village = if class == StructureClass::PillagerOutpost { Some(Box::new(Structure::new(StructureClass::Village, version)?)) } else { None };
        Some(Structure { class, version, config, offset: config.spacing - config.separation, village })
    }

    pub fn name(&self) -> &'static str {
        self.class.name()
    }

    pub fn spacing(&self) -> i32 {
        self.config.spacing
    }

    pub fn separation(&self) -> i32 {
        self.config.separation
    }

    pub fn salt(&self) -> i32 {
        self.config.salt
    }

    /// UniformStructure's `getOffset`.
    pub fn get_offset(&self) -> i32 {
        self.offset
    }

    /// TriangularStructure's `getPeak`.
    pub fn get_peak(&self) -> i32 {
        self.offset
    }

    pub fn get_chance(&self) -> f32 {
        self.config.chance
    }

    /// `at(chunkX, chunkZ)`.
    pub fn region_data(&self, chunk_x: i32, chunk_z: i32) -> RegionData {
        RegionData::new(self, chunk_x, chunk_z)
    }

    /// `canStart`: whether a world with this structure seed starts the structure at the data's chunk.
    pub fn can_start(&self, data: &RegionData, structure_seed: i64, rand: &mut ChunkRand) -> bool {
        match self.class.shape() {
            RegionShape::Uniform => {
                if !self.uniform_can_start(data, structure_seed, rand) {
                    return false;
                }
                if self.class == StructureClass::PillagerOutpost {
                    rand.set_weak_seed(structure_seed, data.chunk_x, data.chunk_z, self.version);
                    rand.next_int(); // Why? No one knows...
                    if rand.next_int_bound(5) != 0 {
                        return false;
                    }
                    if self.version.is_older_than(MCVersion::v1_16) {
                        return true;
                    }
                    return !self.has_nearby_village(structure_seed, data.chunk_x, data.chunk_z, rand);
                }
                true
            }
            RegionShape::Triangular => {
                rand.set_seed(data.base_region_seed.wrapping_add(structure_seed));
                (rand.next_int_bound(self.offset) + rand.next_int_bound(self.offset)) / 2 == data.offset_x
                    && (rand.next_int_bound(self.offset) + rand.next_int_bound(self.offset)) / 2 == data.offset_z
            }
            RegionShape::Treasure => {
                rand.set_seed(data.base_region_seed.wrapping_add(structure_seed));
                rand.next_float() < self.get_chance()
            }
        }
    }

    fn uniform_can_start(&self, data: &RegionData, structure_seed: i64, rand: &mut ChunkRand) -> bool {
        rand.set_seed(data.base_region_seed.wrapping_add(structure_seed));
        rand.next_int_bound(self.offset) == data.offset_x && rand.next_int_bound(self.offset) == data.offset_z
    }

    /// `getInRegion`: where the structure of a region is, `None` where the region has none.
    pub fn get_in_region(&self, structure_seed: i64, region_x: i32, region_z: i32, rand: &mut ChunkRand) -> Option<CPos> {
        match self.class.shape() {
            RegionShape::Uniform => {
                let pos = self.uniform_get_in_region(structure_seed, region_x, region_z, rand);
                if self.class != StructureClass::PillagerOutpost {
                    return Some(pos);
                }
                rand.set_weak_seed(structure_seed, pos.x, pos.z, self.version);
                rand.next_int();
                if rand.next_int_bound(5) != 0 {
                    return None;
                }
                if self.version.is_older_than(MCVersion::v1_16) {
                    return Some(pos);
                }
                if self.has_nearby_village(structure_seed, pos.x, pos.z, rand) { None } else { Some(pos) }
            }
            RegionShape::Triangular => {
                rand.set_region_seed(structure_seed, region_x, region_z, self.salt(), self.version);
                let x = region_x * self.spacing() + (rand.next_int_bound(self.offset) + rand.next_int_bound(self.offset)) / 2;
                let z = region_z * self.spacing() + (rand.next_int_bound(self.offset) + rand.next_int_bound(self.offset)) / 2;
                Some(CPos::new(x, z))
            }
            RegionShape::Treasure => {
                rand.set_region_seed(structure_seed, region_x, region_z, self.salt(), self.version);
                if rand.next_float() < self.get_chance() { Some(CPos::new(region_x, region_z)) } else { None }
            }
        }
    }

    fn uniform_get_in_region(&self, structure_seed: i64, region_x: i32, region_z: i32, rand: &mut ChunkRand) -> CPos {
        rand.set_region_seed(structure_seed, region_x, region_z, self.salt(), self.version);
        let x = region_x * self.spacing() + rand.next_int_bound(self.offset);
        let z = region_z * self.spacing() + rand.next_int_bound(self.offset);
        CPos::new(x, z)
    }

    /// `PillagerOutpost.hasNearbyVillage`.
    pub fn has_nearby_village(&self, structure_seed: i64, chunk_x: i32, chunk_z: i32, rand: &mut ChunkRand) -> bool {
        let village = self.village.as_deref().expect("an outpost has a village");
        let outpost_pos = CPos::new(chunk_x, chunk_z);
        let near = |pos: Option<CPos>| pos.is_some_and(|p| p.chebyshev_distance_to(outpost_pos) <= 10);
        let nw = village.region_data(chunk_x - 10, chunk_z - 10);
        let se = village.region_data(chunk_x + 10, chunk_z + 10);
        if near(village.get_in_region(structure_seed, nw.region_x, nw.region_z, rand)) {
            return true;
        }
        if nw.region_x == se.region_x && nw.region_z == se.region_z {
            return false;
        }
        if nw.region_x != se.region_x && nw.region_z != se.region_z {
            if near(village.get_in_region(structure_seed, se.region_x, se.region_z, rand)) {
                return true;
            }
            let sw = village.region_data(chunk_x - 10, chunk_z + 10);
            if near(village.get_in_region(structure_seed, sw.region_x, sw.region_z, rand)) {
                return true;
            }
            let ne = village.region_data(chunk_x + 10, chunk_z - 10);
            return near(village.get_in_region(structure_seed, ne.region_x, ne.region_z, rand));
        }
        near(village.get_in_region(structure_seed, se.region_x, se.region_z, rand))
    }

    /// `PillagerOutpost.getNearbyVillages`.
    pub fn get_nearby_villages(&self, structure_seed: i64, chunk_x: i32, chunk_z: i32, rand: &mut ChunkRand) -> Vec<CPos> {
        let village = self.village.as_deref().expect("an outpost has a village");
        let outpost_pos = CPos::new(chunk_x, chunk_z);
        let mut villages = Vec::new();
        let check = |pos: Option<CPos>, villages: &mut Vec<CPos>| {
            if let Some(p) = pos {
                if p.chebyshev_distance_to(outpost_pos) <= 10 {
                    villages.push(p);
                }
            }
        };
        let nw = village.region_data(chunk_x - 10, chunk_z - 10);
        let se = village.region_data(chunk_x + 10, chunk_z + 10);
        check(village.get_in_region(structure_seed, nw.region_x, nw.region_z, rand), &mut villages);
        if nw.region_x == se.region_x && nw.region_z == se.region_z {
            return villages;
        }
        if nw.region_x != se.region_x && nw.region_z != se.region_z {
            check(village.get_in_region(structure_seed, se.region_x, se.region_z, rand), &mut villages);
            let sw = village.region_data(chunk_x - 10, chunk_z + 10);
            check(village.get_in_region(structure_seed, sw.region_x, sw.region_z, rand), &mut villages);
            let ne = village.region_data(chunk_x + 10, chunk_z - 10);
            check(village.get_in_region(structure_seed, ne.region_x, ne.region_z, rand), &mut villages);
            return villages;
        }
        check(village.get_in_region(structure_seed, se.region_x, se.region_z, rand), &mut villages);
        villages
    }

    /// `isValidBiome`.
    pub fn is_valid_biome(&self, biome: i32) -> bool {
        use biomes::*;
        let category = super::sunset_scx_biome::get(biome).map(|b| b.category);
        match self.class {
            StructureClass::BuriedTreasure => biome == BEACH || biome == SNOWY_BEACH,
            StructureClass::DesertPyramid => biome == DESERT || biome == DESERT_HILLS,
            StructureClass::EndCity => biome == END_MIDLANDS || biome == END_HIGHLANDS,
            StructureClass::Igloo => biome == SNOWY_TAIGA || biome == SNOWY_TUNDRA,
            StructureClass::JunglePyramid => biome == JUNGLE || biome == JUNGLE_HILLS || biome == BAMBOO_JUNGLE || biome == BAMBOO_JUNGLE_HILLS,
            StructureClass::Monument => biome == DEEP_COLD_OCEAN || biome == DEEP_FROZEN_OCEAN || biome == DEEP_LUKEWARM_OCEAN || biome == DEEP_OCEAN || biome == DEEP_WARM_OCEAN,
            StructureClass::PillagerOutpost => biome == PLAINS || biome == DESERT || biome == SAVANNA || biome == TAIGA || biome == SNOWY_TUNDRA,
            StructureClass::Shipwreck => category == Some(super::sunset_scx_biome::Category::Ocean) || biome == BEACH || biome == SNOWY_BEACH,
            StructureClass::SwampHut => biome == SWAMP,
            StructureClass::TrialChambers => true,
            StructureClass::Village => {
                if biome == PLAINS || biome == DESERT || biome == SAVANNA {
                    return true;
                }
                if biome == TAIGA && self.version.is_newer_or_equal_to(MCVersion::v1_10) {
                    return true;
                }
                biome == SNOWY_TUNDRA && self.version.is_newer_or_equal_to(MCVersion::v1_14)
            }
        }
    }
}

/// `RegionStructure.Data`.
#[derive(Clone, Copy, Debug)]
pub struct RegionData {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub region_x: i32,
    pub region_z: i32,
    pub offset_x: i32,
    pub offset_z: i32,
    pub base_region_seed: i64,
}

impl RegionData {
    pub fn new(structure: &Structure, chunk_x: i32, chunk_z: i32) -> RegionData {
        let spacing = structure.spacing();
        let x = if chunk_x < 0 { chunk_x - spacing + 1 } else { chunk_x };
        let z = if chunk_z < 0 { chunk_z - spacing + 1 } else { chunk_z };
        let region_x = x / spacing;
        let region_z = z / spacing;
        RegionData {
            chunk_x,
            chunk_z,
            region_x,
            region_z,
            offset_x: chunk_x - region_x * spacing,
            offset_z: chunk_z - region_z * spacing,
            base_region_seed: ChunkRand::new().set_region_seed(0, region_x, region_z, structure.salt(), structure.version),
        }
    }
}

// The decorators.

/// `Decorator.Config` of SeedCrackerX: a default salt of `step * 10000 + index` and the salts of the biomes that differ.
#[derive(Clone, Debug)]
pub struct DecoratorConfig {
    pub default_salt: i32,
    pub salts: Vec<(i32, i32)>,
}

impl DecoratorConfig {
    pub fn new(step: i32, index: i32) -> DecoratorConfig {
        DecoratorConfig { default_salt: step * 10000 + index, salts: Vec::new() }
    }

    pub fn add(mut self, step: i32, index: i32, biomes: &[i32]) -> DecoratorConfig {
        for b in biomes {
            self.salts.retain(|(id, _)| id != b);
            self.salts.push((*b, step * 10000 + index));
        }
        self
    }

    pub fn get_salt(&self, biome: Option<i32>) -> i32 {
        biome.and_then(|b| self.salts.iter().find(|(id, _)| *id == b).map(|(_, s)| *s)).unwrap_or(self.default_salt)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecoratorClass {
    Dungeon,
    DeepDungeon,
    EmeraldOre,
    DesertWell,
    EndGateway,
}

#[derive(Debug)]
pub struct Decorator {
    pub class: DecoratorClass,
    pub version: MCVersion,
    pub config: DecoratorConfig,
    /// DesertWell's `chance`, EndGateway's `rarity`.
    pub chance: f32,
    pub rarity: i32,
}

impl Decorator {
    /// `new Dungeon(version)` and the rest: `None` where the class has no config for the version.
    pub fn new(class: DecoratorClass, version: MCVersion) -> Option<Decorator> {
        use MCVersion::*;
        let (config, chance, rarity) = match class {
            DecoratorClass::Dungeon => {
                let configs = VersionMap::new()
                    .add(v1_13, DecoratorConfig::new(2, 3))
                    .add(v1_16, DecoratorConfig::new(3, 2).add(3, 3, &[biomes::DESERT, biomes::SWAMP, biomes::SWAMP_HILLS]))
                    .add(v1_18, DecoratorConfig::new(3, 2));
                (configs.get_as_of(version)?.clone(), 0.0, 0)
            }
            DecoratorClass::DeepDungeon => (VersionMap::new().add(v1_18, DecoratorConfig::new(3, 3)).get_as_of(version)?.clone(), 0.0, 0),
            DecoratorClass::EmeraldOre => {
                let configs = VersionMap::new().add(v1_13, DecoratorConfig::new(4, 14)).add(v1_16, DecoratorConfig::new(6, 14)).add(v1_17, DecoratorConfig::new(6, 17));
                (configs.get_as_of(version)?.clone(), 0.0, 0)
            }
            DecoratorClass::DesertWell => {
                // BiomelessDecorator.Config(index, step, chance)
                let configs = VersionMap::new().add(v1_13, (3, 1)).add(v1_16, (13, 4));
                let (index, step) = *configs.get_as_of(version)?;
                (DecoratorConfig::new(step, index), 0.001, 0)
            }
            DecoratorClass::EndGateway => {
                let configs = VersionMap::new().add(v1_13, (0, 3)).add(v1_16, (0, 4)).add(v1_17, (13, 4));
                let (index, step) = *configs.get_as_of(version)?;
                (DecoratorConfig::new(step, index), 0.0, 700)
            }
        };
        Some(Decorator { class, version, config, chance, rarity })
    }

    /// The name the Java gives: DeepDungeon is a "dungeon" too.
    pub fn name(&self) -> &'static str {
        match self.class {
            DecoratorClass::Dungeon | DecoratorClass::DeepDungeon => "dungeon",
            DecoratorClass::EmeraldOre => "emerald_ore",
            DecoratorClass::DesertWell => "desert_well",
            DecoratorClass::EndGateway => "end_gateway",
        }
    }

    pub fn valid_dimension(&self) -> Dimension {
        if self.class == DecoratorClass::EndGateway { Dimension::End } else { Dimension::Overworld }
    }

    pub fn is_valid_dimension(&self, dimension: Dimension) -> bool {
        self.valid_dimension() == dimension
    }

    /// `getIndex(biome)` of SeedCrackerX's Decorator.
    pub fn get_index(&self, biome: Option<i32>) -> i32 {
        self.config.get_salt(biome) % 10000
    }

    /// `getStep(biome)`.
    pub fn get_step(&self, biome: Option<i32>) -> i32 {
        self.config.get_salt(biome) / 10000
    }

    /// `isValidBiome`.
    pub fn is_valid_biome(&self, biome: i32) -> bool {
        use biomes::*;
        match self.class {
            // The Java's own test: `... && biome == Biomes.THE_END`, so only the End passes. Kept as it is.
            DecoratorClass::Dungeon | DecoratorClass::DeepDungeon => {
                biome != NETHER_WASTES
                    && biome != SOUL_SAND_VALLEY
                    && biome != WARPED_FOREST
                    && biome != CRIMSON_FOREST
                    && biome != BASALT_DELTAS
                    && biome != END_MIDLANDS
                    && biome != END_HIGHLANDS
                    && biome != END_BARRENS
                    && biome != SMALL_END_ISLANDS
                    && biome != THE_VOID
                    && biome == THE_END
            }
            DecoratorClass::EmeraldOre => biome == GRAVELLY_MOUNTAINS || biome == MODIFIED_GRAVELLY_MOUNTAINS || biome == MOUNTAINS || biome == WOODED_MOUNTAINS || biome == MOUNTAIN_EDGE,
            DecoratorClass::DesertWell => biome == DESERT || biome == DESERT_HILLS || biome == DESERT_LAKES,
            DecoratorClass::EndGateway => biome == END_HIGHLANDS,
        }
    }
}

/// What a decorator's data adds to the chunk.
#[derive(Clone, Debug)]
pub enum DecoratorDetail {
    Dungeon(DungeonData),
    DeepDungeon { offset_x: i32, offset_z: i32, block_y: i32 },
    EmeraldOre { offset_x: i32, block_y: i32, offset_z: i32 },
    DesertWell { block_x: i32, block_z: i32, offset_x: i32, offset_z: i32 },
    EndGateway { block_x: i32, block_z: i32, offset_x: i32, offset_z: i32, height: i32 },
}

/// `Dungeon.Data`.
#[derive(Clone, Debug)]
pub struct DungeonData {
    pub offset_x: i32,
    pub block_x: i32,
    pub offset_z: i32,
    pub block_z: i32,
    pub size: Option<[i32; 3]>,
    pub floor_calls: Option<Vec<i32>>,
    pub block_y: i32,
    pub bits_count: f32,
    pub height_context: Option<HeightContext>,
}

pub const COBBLESTONE_CALL: i32 = 0;
pub const MOSSY_COBBLESTONE_CALL: i32 = 1;
pub const MIN_FLOOR_BITS: f32 = 26.0;
pub const MAX_FLOOR_BITS: f32 = 48.0;

impl DungeonData {
    pub fn uses_floor(&self) -> bool {
        self.bits_count >= MIN_FLOOR_BITS && self.bits_count <= MAX_FLOOR_BITS
    }
}

/// `HeightContext` (util): the dimension from its lowest block `minY` up to `maxY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightContext {
    pub bottom_y: i32,
    pub top_y: i32,
}

impl HeightContext {
    pub fn new(min_y: i32, max_y: i32) -> HeightContext {
        HeightContext { bottom_y: min_y, top_y: max_y }
    }

    pub fn get_top_y(&self) -> i32 {
        self.top_y
    }

    pub fn get_bottom_y(&self) -> i32 {
        self.bottom_y
    }

    pub fn get_height(&self) -> i32 {
        self.top_y - self.bottom_y
    }

    pub fn get_distance_to_bottom(&self, y: i32) -> i32 {
        y - self.bottom_y
    }

    pub fn get_distance_to_top(&self, y: i32) -> i32 {
        self.top_y - y - 1
    }
}

// The feature and its data.

/// A feature the cracker can be given data of.
#[derive(Debug)]
pub enum Feature {
    Structure(Structure),
    Decorator(Decorator),
}

impl Feature {
    pub fn version(&self) -> MCVersion {
        match self {
            Feature::Structure(s) => s.version,
            Feature::Decorator(d) => d.version,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Feature::Structure(s) => s.name(),
            Feature::Decorator(d) => d.name(),
        }
    }

    pub fn is_structure(&self) -> bool {
        matches!(self, Feature::Structure(_))
    }

    pub fn is_decorator(&self) -> bool {
        matches!(self, Feature::Decorator(_))
    }

    pub fn valid_dimension(&self) -> Dimension {
        match self {
            Feature::Structure(s) => s.class.valid_dimension(),
            Feature::Decorator(d) => d.valid_dimension(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Detail {
    Region(RegionData),
    Decorator(DecoratorDetail),
}

/// `Feature.Data`.
#[derive(Clone, Debug)]
pub struct FeatureData {
    pub feature: Arc<Feature>,
    pub chunk_x: i32,
    pub chunk_z: i32,
    /// `Decorator.Data.biome`.
    pub biome: Option<i32>,
    pub detail: Detail,
}

impl FeatureData {
    /// `Feature.Data.testStart(structureSeed, rand)`.
    pub fn test_start(&self, structure_seed: i64, rand: &mut ChunkRand) -> bool {
        match (&*self.feature, &self.detail) {
            (Feature::Structure(s), Detail::Region(d)) => s.can_start(d, structure_seed, rand),
            (Feature::Decorator(dec), Detail::Decorator(detail)) => dec.can_start(self, detail, structure_seed, rand),
            _ => panic!("data of one kind of feature given to another"),
        }
    }

    /// `Decorator.Data.testStart(worldSeed, WorldgenRandom)`: the 1.18 decorators only.
    pub fn test_start_world(&self, world_seed: i64, rand: &mut WorldgenRandom) -> bool {
        match (&*self.feature, &self.detail) {
            (Feature::Decorator(dec), Detail::Decorator(detail)) => dec.can_start_world(self, detail, world_seed, rand),
            _ => panic!("only a decorator tests a world seed"),
        }
    }
}

impl FeatureData {
    /// `RegionStructure.at(chunkX, chunkZ)`.
    pub fn structure(feature: &Arc<Feature>, chunk_x: i32, chunk_z: i32) -> FeatureData {
        let Feature::Structure(s) = &**feature else { panic!("not a structure") };
        FeatureData { feature: feature.clone(), chunk_x, chunk_z, biome: None, detail: Detail::Region(s.region_data(chunk_x, chunk_z)) }
    }

    /// `EmeraldOre.at(blockX, blockY, blockZ, biome)`.
    pub fn emerald_ore(feature: &Arc<Feature>, block_x: i32, block_y: i32, block_z: i32, biome: Option<i32>) -> FeatureData {
        let detail = DecoratorDetail::EmeraldOre { offset_x: block_x & 15, block_y, offset_z: block_z & 15 };
        FeatureData { feature: feature.clone(), chunk_x: block_x >> 4, chunk_z: block_z >> 4, biome, detail: Detail::Decorator(detail) }
    }

    /// `DeepDungeon.at(blockX, blockY, blockZ, biome)`.
    pub fn deep_dungeon(feature: &Arc<Feature>, block_x: i32, block_y: i32, block_z: i32, biome: Option<i32>) -> FeatureData {
        let detail = DecoratorDetail::DeepDungeon { offset_x: block_x & 15, offset_z: block_z & 15, block_y };
        FeatureData { feature: feature.clone(), chunk_x: block_x >> 4, chunk_z: block_z >> 4, biome, detail: Detail::Decorator(detail) }
    }

    /// `DesertWell.at(blockX, blockZ)`.
    pub fn desert_well(feature: &Arc<Feature>, block_x: i32, block_z: i32) -> FeatureData {
        let detail = DecoratorDetail::DesertWell { block_x, block_z, offset_x: block_x & 15, offset_z: block_z & 15 };
        FeatureData { feature: feature.clone(), chunk_x: block_x >> 4, chunk_z: block_z >> 4, biome: None, detail: Detail::Decorator(detail) }
    }

    /// `EndGateway.at(blockX, blockZ, height)`.
    pub fn end_gateway(feature: &Arc<Feature>, block_x: i32, block_z: i32, height: i32) -> FeatureData {
        let detail = DecoratorDetail::EndGateway { block_x, block_z, offset_x: block_x & 15, offset_z: block_z & 15, height };
        FeatureData { feature: feature.clone(), chunk_x: block_x >> 4, chunk_z: block_z >> 4, biome: None, detail: Detail::Decorator(detail) }
    }

    /// `Dungeon.at(blockX, blockY, blockZ, size, floorCalls, biome, heightContext)`.
    #[allow(clippy::too_many_arguments)]
    pub fn dungeon(feature: &Arc<Feature>, block_x: i32, block_y: i32, block_z: i32, size: Option<[i32; 3]>, floor_calls: Option<Vec<i32>>, biome: Option<i32>, height_context: Option<HeightContext>) -> FeatureData {
        let Feature::Decorator(d) = &**feature else { panic!("not a decorator") };
        let (mut bx, mut bz) = (block_x, block_z);
        if d.version.is_older_than(MCVersion::v1_13) {
            // 1.12
            bx -= 8;
            bz -= 8;
        }
        let mut bits_count = 0.0f32;
        if let Some(calls) = &floor_calls {
            for call in calls {
                bits_count += if *call == COBBLESTONE_CALL { 2.0 } else { 0.0 };
            }
        }
        let detail = DecoratorDetail::Dungeon(DungeonData { offset_x: bx & 15, block_x: bx, offset_z: bz & 15, block_z: bz, size, floor_calls, block_y, bits_count, height_context });
        FeatureData { feature: feature.clone(), chunk_x: block_x >> 4, chunk_z: block_z >> 4, biome, detail: Detail::Decorator(detail) }
    }

    /// Two data are the same when they are of one feature object and one chunk (`Entry.equals`).
    pub fn same_place(&self, other: &FeatureData) -> bool {
        Arc::ptr_eq(&self.feature, &other.feature) && self.chunk_x == other.chunk_x && self.chunk_z == other.chunk_z
    }
}

impl Decorator {
    /// Whether this is one of SeedCrackerX's own decorators (`cracker.decorator.Decorator`), not a BiomelessDecorator
    /// of the library: the DataStorage and the TimeMachine tell them apart with `instanceof`.
    pub fn is_seedcracker_decorator(&self) -> bool {
        matches!(self.class, DecoratorClass::Dungeon | DecoratorClass::DeepDungeon | DecoratorClass::EmeraldOre)
    }

    fn set_decorator_seed(&self, data: &FeatureData, structure_seed: i64, rand: &mut ChunkRand) {
        // The biomeless decorators seed with the default salt, the others with the salt of their biome.
        match self.class {
            DecoratorClass::DesertWell | DecoratorClass::EndGateway => {
                rand.set_decorator_seed_at_salt(structure_seed, data.chunk_x << 4, data.chunk_z << 4, self.config.default_salt, self.version);
            }
            _ => {
                rand.set_decorator_seed_at_step(structure_seed, data.chunk_x << 4, data.chunk_z << 4, self.get_index(data.biome), self.get_step(data.biome), self.version);
            }
        }
    }

    /// `canStart(data, structureSeed, rand)`: the decorators of before 1.18, by a ChunkRand.
    pub fn can_start(&self, data: &FeatureData, detail: &DecoratorDetail, structure_seed: i64, rand: &mut ChunkRand) -> bool {
        match (self.class, detail) {
            (DecoratorClass::Dungeon, DecoratorDetail::Dungeon(d)) => {
                self.set_decorator_seed(data, structure_seed, rand);
                for _ in 0..8 {
                    let (x, y, z);
                    if self.version.is_older_than(MCVersion::v1_15) {
                        x = rand.next_int_bound(16);
                        y = rand.next_int_bound(256);
                        z = rand.next_int_bound(16);
                    } else {
                        x = rand.next_int_bound(16);
                        z = rand.next_int_bound(16);
                        let hc = d.height_context.expect("a dungeon of before 1.18 knows its height");
                        y = rand.next_int_bound(hc.get_height()) + hc.get_bottom_y();
                    }
                    if y == d.block_y && x == d.offset_x && z == d.offset_z {
                        return true;
                    }
                    rand.next_int_bound(2);
                    rand.next_int_bound(2);
                }
                false
            }
            (DecoratorClass::DeepDungeon, DecoratorDetail::DeepDungeon { .. }) => true,
            (DecoratorClass::EmeraldOre, DecoratorDetail::EmeraldOre { offset_x, block_y, offset_z }) => {
                if self.version.is_newer_than(MCVersion::v1_17_1) {
                    return true;
                }
                self.set_decorator_seed(data, structure_seed, rand);
                let bound = if self.version == MCVersion::v1_17 { rand.next_int_bound(19) + 6 } else { rand.next_int_bound(6) + 3 };
                for _ in 0..bound {
                    let (x, y, z);
                    if self.version.is_older_than(MCVersion::v1_15) {
                        x = rand.next_int_bound(16);
                        y = rand.next_int_bound(28) + 4;
                        z = rand.next_int_bound(16);
                    } else {
                        x = rand.next_int_bound(16);
                        z = rand.next_int_bound(16);
                        y = rand.next_int_bound(28) + 4;
                    }
                    if y == *block_y && x == *offset_x && z == *offset_z {
                        return true;
                    }
                }
                false
            }
            (DecoratorClass::DesertWell, DecoratorDetail::DesertWell { offset_x, offset_z, .. }) => {
                self.set_decorator_seed(data, structure_seed, rand);
                if rand.next_float() >= self.chance {
                    return false;
                }
                if rand.next_int_bound(16) != *offset_x {
                    return false;
                }
                rand.next_int_bound(16) == *offset_z
            }
            (DecoratorClass::EndGateway, DecoratorDetail::EndGateway { offset_x, offset_z, height, .. }) => {
                self.set_decorator_seed(data, structure_seed, rand);
                if self.version.is_newer_or_equal_to(MCVersion::v1_17) {
                    if rand.next_float() >= 1.0f32 / self.rarity as f32 {
                        return false;
                    }
                } else if rand.next_int_bound(self.rarity) != 0 {
                    return false;
                }
                if rand.next_int_bound(16) != *offset_x {
                    return false;
                }
                if rand.next_int_bound(16) != *offset_z {
                    return false;
                }
                rand.next_int_bound(7) == *height - 3
            }
            _ => panic!("data of one decorator given to another"),
        }
    }

    /// `canStart(data, worldSeed, WorldgenRandom)`: the decorators of 1.18 and later.
    pub fn can_start_world(&self, data: &FeatureData, detail: &DecoratorDetail, world_seed: i64, rand: &mut WorldgenRandom) -> bool {
        let seed_for = |rand: &mut WorldgenRandom| {
            let l = rand.set_decoration_seed(world_seed, data.chunk_x << 4, data.chunk_z << 4);
            rand.set_feature_seed(l, self.get_index(data.biome), self.get_step(data.biome));
        };
        match (self.class, detail) {
            (DecoratorClass::Dungeon, DecoratorDetail::Dungeon(d)) => {
                seed_for(rand);
                for _ in 0..10 {
                    let x = rand.next_int_bound(16);
                    let z = rand.next_int_bound(16);
                    let y = rand.next_int_bound(320);
                    if y == d.block_y && x == d.offset_x && z == d.offset_z {
                        return true;
                    }
                    rand.next_int_bound(2);
                    rand.next_int_bound(2);
                }
                false
            }
            (DecoratorClass::DeepDungeon, DecoratorDetail::DeepDungeon { offset_x, offset_z, block_y }) => {
                seed_for(rand);
                for _ in 0..4 {
                    let x = rand.next_int_bound(16);
                    let z = rand.next_int_bound(16);
                    let y = rand.next_int_bound(58) - 58;
                    if y == *block_y && x == *offset_x && z == *offset_z {
                        return true;
                    }
                    rand.next_int_bound(2);
                    rand.next_int_bound(2);
                }
                false
            }
            (DecoratorClass::EmeraldOre, _) => true,
            // The two biomeless decorators of the library have no WorldgenRandom test: they are only asked with a structure seed.
            _ => panic!("{} is tested with a structure seed only", self.name()),
        }
    }
}
