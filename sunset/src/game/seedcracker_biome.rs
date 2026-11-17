// com.seedfinding.mcbiome.biome.Biome and Biomes: the 79 biomes the library registers, with the names the Java gives them.
// Generated from Biomes.java, one row for one `register(new Biome(...))`; the aliases (`HELL`, `SKY`, ...) follow the table.
// A biome is named by its id, as the Java compares them (`Biome.equals` is the id).
use super::sunset_scx_dimension::Dimension;
use super::sunset_scx_version::MCVersion;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Category { None, Taiga, ExtremeHills, Jungle, Mesa, BadlandsPlateau, Plains, Savanna, Icy, TheEnd, Beach, Forest, Ocean, Desert, River, Swamp, Mushroom, Nether }

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::None => "none",
            Category::Taiga => "taiga",
            Category::ExtremeHills => "extreme_hills",
            Category::Jungle => "jungle",
            Category::Mesa => "mesa",
            Category::BadlandsPlateau => "badlands_plateau",
            Category::Plains => "plains",
            Category::Savanna => "savanna",
            Category::Icy => "icy",
            Category::TheEnd => "the_end",
            Category::Beach => "beach",
            Category::Forest => "forest",
            Category::Ocean => "ocean",
            Category::Desert => "desert",
            Category::River => "river",
            Category::Swamp => "swamp",
            Category::Mushroom => "mushroom",
            Category::Nether => "nether",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Precipitation { None, Rain, Snow }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Temperature { Ocean, Cold, Medium, Warm }

/// A biome: `id` is what the Java compares, `parent` the id of the biome it is a mutation of.
#[derive(Debug)]
pub struct Biome {
    pub version: MCVersion,
    pub dimension: Option<Dimension>,
    pub id: i32,
    pub name: &'static str,
    pub category: Category,
    pub precipitation: Precipitation,
    pub temperature: f32,
    pub scale: f32,
    pub depth: f32,
    pub parent: Option<i32>,
}

/// The registered biomes, in the order of Biomes.java.
pub static REGISTRY: [Biome; 79] = [
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 0, name: "ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.0, parent: None }, // OCEAN
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 1, name: "plains", category: Category::Plains, precipitation: Precipitation::Rain, temperature: 0.8, scale: 0.05, depth: 0.125, parent: None }, // PLAINS
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 2, name: "desert", category: Category::Desert, precipitation: Precipitation::None, temperature: 2.0, scale: 0.05, depth: 0.125, parent: None }, // DESERT
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 3, name: "mountains", category: Category::ExtremeHills, precipitation: Precipitation::Rain, temperature: 0.2, scale: 0.5, depth: 1.0, parent: None }, // MOUNTAINS
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 4, name: "forest", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.7, scale: 0.2, depth: 0.1, parent: None }, // FOREST
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 5, name: "taiga", category: Category::Taiga, precipitation: Precipitation::Rain, temperature: 0.25, scale: 0.2, depth: 0.2, parent: None }, // TAIGA
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 6, name: "swamp", category: Category::Swamp, precipitation: Precipitation::Rain, temperature: 0.8, scale: 0.1, depth: -0.2, parent: None }, // SWAMP
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Overworld), id: 7, name: "river", category: Category::River, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.0, depth: -0.5, parent: None }, // RIVER
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::Nether), id: 8, name: "nether_wastes", category: Category::Nether, precipitation: Precipitation::None, temperature: 2.0, scale: 0.2, depth: 0.1, parent: None }, // NETHER_WASTES
    Biome { version: MCVersion::vb1_8_1, dimension: Some(Dimension::End), id: 9, name: "the_end", category: Category::TheEnd, precipitation: Precipitation::None, temperature: 0.5, scale: 0.2, depth: 0.1, parent: None }, // THE_END
    Biome { version: MCVersion::v1_0, dimension: Some(Dimension::Overworld), id: 10, name: "frozen_ocean", category: Category::Ocean, precipitation: Precipitation::Snow, temperature: 0.0, scale: 0.1, depth: -1.0, parent: None }, // FROZEN_OCEAN
    Biome { version: MCVersion::v1_0, dimension: Some(Dimension::Overworld), id: 11, name: "frozen_river", category: Category::River, precipitation: Precipitation::Snow, temperature: 0.0, scale: 0.0, depth: -0.5, parent: None }, // FROZEN_RIVER
    Biome { version: MCVersion::v1_0, dimension: Some(Dimension::Overworld), id: 12, name: "snowy_tundra", category: Category::Icy, precipitation: Precipitation::Snow, temperature: 0.0, scale: 0.05, depth: 0.125, parent: None }, // SNOWY_TUNDRA
    Biome { version: MCVersion::v1_0, dimension: Some(Dimension::Overworld), id: 13, name: "snowy_mountains", category: Category::Icy, precipitation: Precipitation::Snow, temperature: 0.0, scale: 0.3, depth: 0.45, parent: None }, // SNOWY_MOUNTAINS
    Biome { version: MCVersion::v1_0, dimension: Some(Dimension::Overworld), id: 14, name: "mushroom_fields", category: Category::Mushroom, precipitation: Precipitation::Rain, temperature: 0.9, scale: 0.3, depth: 0.2, parent: None }, // MUSHROOM_FIELDS
    Biome { version: MCVersion::v1_0, dimension: Some(Dimension::Overworld), id: 15, name: "mushroom_field_shore", category: Category::Mushroom, precipitation: Precipitation::Rain, temperature: 0.9, scale: 0.025, depth: 0.0, parent: None }, // MUSHROOM_FIELD_SHORE
    Biome { version: MCVersion::v1_1, dimension: Some(Dimension::Overworld), id: 16, name: "beach", category: Category::Beach, precipitation: Precipitation::Rain, temperature: 0.8, scale: 0.025, depth: 0.0, parent: None }, // BEACH
    Biome { version: MCVersion::v1_1, dimension: Some(Dimension::Overworld), id: 17, name: "desert_hills", category: Category::Desert, precipitation: Precipitation::None, temperature: 2.0, scale: 0.3, depth: 0.45, parent: None }, // DESERT_HILLS
    Biome { version: MCVersion::v1_1, dimension: Some(Dimension::Overworld), id: 18, name: "wooded_hills", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.7, scale: 0.3, depth: 0.45, parent: None }, // WOODED_HILLS
    Biome { version: MCVersion::v1_1, dimension: Some(Dimension::Overworld), id: 19, name: "taiga_hills", category: Category::Taiga, precipitation: Precipitation::Rain, temperature: 0.25, scale: 0.3, depth: 0.45, parent: None }, // TAIGA_HILLS
    Biome { version: MCVersion::v1_1, dimension: Some(Dimension::Overworld), id: 20, name: "mountain_edge", category: Category::ExtremeHills, precipitation: Precipitation::Rain, temperature: 0.2, scale: 0.3, depth: 0.8, parent: None }, // MOUNTAIN_EDGE
    Biome { version: MCVersion::v1_2_1, dimension: Some(Dimension::Overworld), id: 21, name: "jungle", category: Category::Jungle, precipitation: Precipitation::Rain, temperature: 0.95, scale: 0.2, depth: 0.1, parent: None }, // JUNGLE
    Biome { version: MCVersion::v1_2_1, dimension: Some(Dimension::Overworld), id: 22, name: "jungle_hills", category: Category::Jungle, precipitation: Precipitation::Rain, temperature: 0.95, scale: 0.3, depth: 0.45, parent: None }, // JUNGLE_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 23, name: "jungle_edge", category: Category::Jungle, precipitation: Precipitation::Rain, temperature: 0.95, scale: 0.2, depth: 0.1, parent: None }, // JUNGLE_EDGE
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 24, name: "deep_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.8, parent: None }, // DEEP_OCEAN
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 25, name: "stone_shore", category: Category::None, precipitation: Precipitation::Rain, temperature: 0.2, scale: 0.8, depth: 0.1, parent: None }, // STONE_SHORE
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 26, name: "snowy_beach", category: Category::Beach, precipitation: Precipitation::Snow, temperature: 0.05, scale: 0.025, depth: 0.0, parent: None }, // SNOWY_BEACH
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 27, name: "birch_forest", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.6, scale: 0.2, depth: 0.1, parent: None }, // BIRCH_FOREST
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 28, name: "birch_forest_hills", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.6, scale: 0.3, depth: 0.45, parent: None }, // BIRCH_FOREST_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 29, name: "dark_forest", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.7, scale: 0.2, depth: 0.1, parent: None }, // DARK_FOREST
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 30, name: "snowy_taiga", category: Category::Taiga, precipitation: Precipitation::Snow, temperature: -0.5, scale: 0.2, depth: 0.2, parent: None }, // SNOWY_TAIGA
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 31, name: "snowy_taiga_hills", category: Category::Taiga, precipitation: Precipitation::Snow, temperature: -0.5, scale: 0.3, depth: 0.45, parent: None }, // SNOWY_TAIGA_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 32, name: "giant_tree_taiga", category: Category::Taiga, precipitation: Precipitation::Rain, temperature: 0.3, scale: 0.2, depth: 0.2, parent: None }, // GIANT_TREE_TAIGA
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 33, name: "giant_tree_taiga_hills", category: Category::Taiga, precipitation: Precipitation::Rain, temperature: 0.3, scale: 0.3, depth: 0.45, parent: None }, // GIANT_TREE_TAIGA_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 34, name: "wooded_mountains", category: Category::ExtremeHills, precipitation: Precipitation::Rain, temperature: 0.2, scale: 0.5, depth: 1.0, parent: None }, // WOODED_MOUNTAINS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 35, name: "savanna", category: Category::Savanna, precipitation: Precipitation::None, temperature: 1.2, scale: 0.05, depth: 0.125, parent: None }, // SAVANNA
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 36, name: "savanna_plateau", category: Category::Savanna, precipitation: Precipitation::None, temperature: 1.0, scale: 0.025, depth: 1.5, parent: None }, // SAVANNA_PLATEAU
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 37, name: "badlands", category: Category::Mesa, precipitation: Precipitation::None, temperature: 2.0, scale: 0.2, depth: 0.1, parent: None }, // BADLANDS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 38, name: "wooded_badlands_plateau", category: Category::Mesa, precipitation: Precipitation::None, temperature: 2.0, scale: 0.025, depth: 1.5, parent: None }, // WOODED_BADLANDS_PLATEAU
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 39, name: "badlands_plateau", category: Category::Mesa, precipitation: Precipitation::None, temperature: 2.0, scale: 0.025, depth: 1.5, parent: None }, // BADLANDS_PLATEAU
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::End), id: 40, name: "small_end_islands", category: Category::TheEnd, precipitation: Precipitation::None, temperature: 0.5, scale: 0.2, depth: 0.1, parent: None }, // SMALL_END_ISLANDS
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::End), id: 41, name: "end_midlands", category: Category::TheEnd, precipitation: Precipitation::None, temperature: 0.5, scale: 0.2, depth: 0.1, parent: None }, // END_MIDLANDS
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::End), id: 42, name: "end_highlands", category: Category::TheEnd, precipitation: Precipitation::None, temperature: 0.5, scale: 0.2, depth: 0.1, parent: None }, // END_HIGHLANDS
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::End), id: 43, name: "end_barrens", category: Category::TheEnd, precipitation: Precipitation::None, temperature: 0.5, scale: 0.2, depth: 0.1, parent: None }, // END_BARRENS
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::Overworld), id: 44, name: "warm_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.0, parent: None }, // WARM_OCEAN
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::Overworld), id: 45, name: "lukewarm_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.0, parent: None }, // LUKEWARM_OCEAN
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::Overworld), id: 46, name: "cold_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.0, parent: None }, // COLD_OCEAN
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::Overworld), id: 47, name: "deep_warm_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.8, parent: None }, // DEEP_WARM_OCEAN
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::Overworld), id: 48, name: "deep_lukewarm_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.8, parent: None }, // DEEP_LUKEWARM_OCEAN
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::Overworld), id: 49, name: "deep_cold_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.8, parent: None }, // DEEP_COLD_OCEAN
    Biome { version: MCVersion::v1_13, dimension: Some(Dimension::Overworld), id: 50, name: "deep_frozen_ocean", category: Category::Ocean, precipitation: Precipitation::Rain, temperature: 0.5, scale: 0.1, depth: -1.8, parent: None }, // DEEP_FROZEN_OCEAN
    Biome { version: MCVersion::v1_9, dimension: None, id: 127, name: "the_void", category: Category::None, precipitation: Precipitation::None, temperature: 0.5, scale: 0.2, depth: 0.1, parent: None }, // THE_VOID
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 129, name: "sunflower_plains", category: Category::Plains, precipitation: Precipitation::Rain, temperature: 0.8, scale: 0.05, depth: 0.125, parent: Some(1) }, // SUNFLOWER_PLAINS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 130, name: "desert_lakes", category: Category::Desert, precipitation: Precipitation::None, temperature: 2.0, scale: 0.25, depth: 0.225, parent: Some(2) }, // DESERT_LAKES
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 131, name: "gravelly_mountains", category: Category::ExtremeHills, precipitation: Precipitation::Rain, temperature: 0.2, scale: 0.5, depth: 1.0, parent: Some(3) }, // GRAVELLY_MOUNTAINS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 132, name: "flower_forest", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.7, scale: 0.4, depth: 0.1, parent: Some(4) }, // FLOWER_FOREST
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 133, name: "taiga_mountains", category: Category::Taiga, precipitation: Precipitation::Rain, temperature: 0.25, scale: 0.4, depth: 0.3, parent: Some(5) }, // TAIGA_MOUNTAINS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 134, name: "swamp_hills", category: Category::Swamp, precipitation: Precipitation::Rain, temperature: 0.8, scale: 0.3, depth: -0.1, parent: Some(6) }, // SWAMP_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 140, name: "ice_spikes", category: Category::Icy, precipitation: Precipitation::Snow, temperature: 0.0, scale: 0.45, depth: 0.425, parent: Some(12) }, // ICE_SPIKES
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 149, name: "modified_jungle", category: Category::Jungle, precipitation: Precipitation::Rain, temperature: 0.95, scale: 0.4, depth: 0.2, parent: Some(21) }, // MODIFIED_JUNGLE
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 151, name: "modified_jungle_edge", category: Category::Jungle, precipitation: Precipitation::Rain, temperature: 0.95, scale: 0.4, depth: 0.2, parent: Some(23) }, // MODIFIED_JUNGLE_EDGE
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 155, name: "tall_birch_forest", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.6, scale: 0.4, depth: 0.2, parent: Some(27) }, // TALL_BIRCH_FOREST
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 156, name: "tall_birch_hills", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.6, scale: 0.5, depth: 0.55, parent: Some(28) }, // TALL_BIRCH_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 157, name: "dark_forest_hills", category: Category::Forest, precipitation: Precipitation::Rain, temperature: 0.7, scale: 0.4, depth: 0.2, parent: Some(29) }, // DARK_FOREST_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 158, name: "snowy_taiga_mountains", category: Category::Taiga, precipitation: Precipitation::Snow, temperature: -0.5, scale: 0.4, depth: 0.3, parent: Some(30) }, // SNOWY_TAIGA_MOUNTAINS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 160, name: "giant_spruce_taiga", category: Category::Taiga, precipitation: Precipitation::Rain, temperature: 0.25, scale: 0.2, depth: 0.2, parent: Some(32) }, // GIANT_SPRUCE_TAIGA
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 161, name: "giant_spruce_taiga_hills", category: Category::Taiga, precipitation: Precipitation::Rain, temperature: 0.25, scale: 0.2, depth: 0.2, parent: Some(33) }, // GIANT_SPRUCE_TAIGA_HILLS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 162, name: "modified_gravelly_mountains", category: Category::ExtremeHills, precipitation: Precipitation::Rain, temperature: 0.2, scale: 0.5, depth: 1.0, parent: Some(34) }, // MODIFIED_GRAVELLY_MOUNTAINS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 163, name: "shattered_savanna", category: Category::Savanna, precipitation: Precipitation::None, temperature: 1.1, scale: 1.225, depth: 0.3625, parent: Some(35) }, // SHATTERED_SAVANNA
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 164, name: "shattered_savanna_plateau", category: Category::Savanna, precipitation: Precipitation::None, temperature: 1.0, scale: 1.212, depth: 1.05, parent: Some(36) }, // SHATTERED_SAVANNA_PLATEAU
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 165, name: "eroded_badlands", category: Category::Mesa, precipitation: Precipitation::None, temperature: 2.0, scale: 0.2, depth: 0.1, parent: Some(37) }, // ERODED_BADLANDS
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 166, name: "modified_wooded_badlands_plateau", category: Category::Mesa, precipitation: Precipitation::None, temperature: 2.0, scale: 0.3, depth: 0.45, parent: Some(38) }, // MODIFIED_WOODED_BADLANDS_PLATEAU
    Biome { version: MCVersion::v1_7_2, dimension: Some(Dimension::Overworld), id: 167, name: "modified_badlands_plateau", category: Category::Mesa, precipitation: Precipitation::None, temperature: 2.0, scale: 0.3, depth: 0.45, parent: Some(39) }, // MODIFIED_BADLANDS_PLATEAU
    Biome { version: MCVersion::v1_14, dimension: Some(Dimension::Overworld), id: 168, name: "bamboo_jungle", category: Category::Jungle, precipitation: Precipitation::Rain, temperature: 0.95, scale: 0.2, depth: 0.1, parent: None }, // BAMBOO_JUNGLE
    Biome { version: MCVersion::v1_14, dimension: Some(Dimension::Overworld), id: 169, name: "bamboo_jungle_hills", category: Category::Jungle, precipitation: Precipitation::Rain, temperature: 0.95, scale: 0.3, depth: 0.45, parent: None }, // BAMBOO_JUNGLE_HILLS
    Biome { version: MCVersion::v1_16, dimension: Some(Dimension::Nether), id: 170, name: "soul_sand_valley", category: Category::Nether, precipitation: Precipitation::None, temperature: 2.0, scale: 0.2, depth: 0.1, parent: None }, // SOUL_SAND_VALLEY
    Biome { version: MCVersion::v1_16, dimension: Some(Dimension::Nether), id: 171, name: "crimson_forest", category: Category::Nether, precipitation: Precipitation::None, temperature: 2.0, scale: 0.2, depth: 0.1, parent: None }, // CRIMSON_FOREST
    Biome { version: MCVersion::v1_16, dimension: Some(Dimension::Nether), id: 172, name: "warped_forest", category: Category::Nether, precipitation: Precipitation::None, temperature: 2.0, scale: 0.2, depth: 0.1, parent: None }, // WARPED_FOREST
    Biome { version: MCVersion::v1_16, dimension: Some(Dimension::Nether), id: 173, name: "basalt_deltas", category: Category::Nether, precipitation: Precipitation::None, temperature: 2.0, scale: 0.2, depth: 0.1, parent: None }, // BASALT_DELTAS
];

pub mod biomes {
    //! The constants of Biomes.java, as ids.
    pub const OCEAN: i32 = 0;
    pub const PLAINS: i32 = 1;
    pub const DESERT: i32 = 2;
    pub const MOUNTAINS: i32 = 3;
    pub const FOREST: i32 = 4;
    pub const TAIGA: i32 = 5;
    pub const SWAMP: i32 = 6;
    pub const RIVER: i32 = 7;
    pub const NETHER_WASTES: i32 = 8;
    pub const THE_END: i32 = 9;
    pub const FROZEN_OCEAN: i32 = 10;
    pub const FROZEN_RIVER: i32 = 11;
    pub const SNOWY_TUNDRA: i32 = 12;
    pub const SNOWY_MOUNTAINS: i32 = 13;
    pub const MUSHROOM_FIELDS: i32 = 14;
    pub const MUSHROOM_FIELD_SHORE: i32 = 15;
    pub const BEACH: i32 = 16;
    pub const DESERT_HILLS: i32 = 17;
    pub const WOODED_HILLS: i32 = 18;
    pub const TAIGA_HILLS: i32 = 19;
    pub const MOUNTAIN_EDGE: i32 = 20;
    pub const JUNGLE: i32 = 21;
    pub const JUNGLE_HILLS: i32 = 22;
    pub const JUNGLE_EDGE: i32 = 23;
    pub const DEEP_OCEAN: i32 = 24;
    pub const STONE_SHORE: i32 = 25;
    pub const SNOWY_BEACH: i32 = 26;
    pub const BIRCH_FOREST: i32 = 27;
    pub const BIRCH_FOREST_HILLS: i32 = 28;
    pub const DARK_FOREST: i32 = 29;
    pub const SNOWY_TAIGA: i32 = 30;
    pub const SNOWY_TAIGA_HILLS: i32 = 31;
    pub const GIANT_TREE_TAIGA: i32 = 32;
    pub const GIANT_TREE_TAIGA_HILLS: i32 = 33;
    pub const WOODED_MOUNTAINS: i32 = 34;
    pub const SAVANNA: i32 = 35;
    pub const SAVANNA_PLATEAU: i32 = 36;
    pub const BADLANDS: i32 = 37;
    pub const WOODED_BADLANDS_PLATEAU: i32 = 38;
    pub const BADLANDS_PLATEAU: i32 = 39;
    pub const SMALL_END_ISLANDS: i32 = 40;
    pub const END_MIDLANDS: i32 = 41;
    pub const END_HIGHLANDS: i32 = 42;
    pub const END_BARRENS: i32 = 43;
    pub const WARM_OCEAN: i32 = 44;
    pub const LUKEWARM_OCEAN: i32 = 45;
    pub const COLD_OCEAN: i32 = 46;
    pub const DEEP_WARM_OCEAN: i32 = 47;
    pub const DEEP_LUKEWARM_OCEAN: i32 = 48;
    pub const DEEP_COLD_OCEAN: i32 = 49;
    pub const DEEP_FROZEN_OCEAN: i32 = 50;
    pub const THE_VOID: i32 = 127;
    pub const SUNFLOWER_PLAINS: i32 = 129;
    pub const DESERT_LAKES: i32 = 130;
    pub const GRAVELLY_MOUNTAINS: i32 = 131;
    pub const FLOWER_FOREST: i32 = 132;
    pub const TAIGA_MOUNTAINS: i32 = 133;
    pub const SWAMP_HILLS: i32 = 134;
    pub const ICE_SPIKES: i32 = 140;
    pub const MODIFIED_JUNGLE: i32 = 149;
    pub const MODIFIED_JUNGLE_EDGE: i32 = 151;
    pub const TALL_BIRCH_FOREST: i32 = 155;
    pub const TALL_BIRCH_HILLS: i32 = 156;
    pub const DARK_FOREST_HILLS: i32 = 157;
    pub const SNOWY_TAIGA_MOUNTAINS: i32 = 158;
    pub const GIANT_SPRUCE_TAIGA: i32 = 160;
    pub const GIANT_SPRUCE_TAIGA_HILLS: i32 = 161;
    pub const MODIFIED_GRAVELLY_MOUNTAINS: i32 = 162;
    pub const SHATTERED_SAVANNA: i32 = 163;
    pub const SHATTERED_SAVANNA_PLATEAU: i32 = 164;
    pub const ERODED_BADLANDS: i32 = 165;
    pub const MODIFIED_WOODED_BADLANDS_PLATEAU: i32 = 166;
    pub const MODIFIED_BADLANDS_PLATEAU: i32 = 167;
    pub const BAMBOO_JUNGLE: i32 = 168;
    pub const BAMBOO_JUNGLE_HILLS: i32 = 169;
    pub const SOUL_SAND_VALLEY: i32 = 170;
    pub const CRIMSON_FOREST: i32 = 171;
    pub const WARPED_FOREST: i32 = 172;
    pub const BASALT_DELTAS: i32 = 173;
    pub const EXTREME_HILLS: i32 = 3;
    pub const SWAMPLAND: i32 = 6;
    pub const HELL: i32 = 8;
    pub const SKY: i32 = 9;
    pub const ICE_PLAINS: i32 = 12;
    pub const ICE_FLATS: i32 = 12;
    pub const ICE_MOUNTAINS: i32 = 13;
    pub const MUSHROOM_ISLAND: i32 = 14;
    pub const MUSHROOM_ISLAND_SHORE: i32 = 15;
    pub const FOREST_HILLS: i32 = 18;
    pub const EXTREME_HILLS_EDGE: i32 = 20;
    pub const SMALLER_EXTREME_HILLS: i32 = 20;
    pub const STONE_BEACH: i32 = 25;
    pub const COLD_BEACH: i32 = 26;
    pub const ROOFED_FOREST: i32 = 29;
    pub const COLD_TAIGA: i32 = 30;
    pub const MEGA_TAIGA: i32 = 32;
    pub const REDWOOD_TAIGA: i32 = 32;
    pub const EXTREME_HILLS_PLUS: i32 = 34;
    pub const EXTREME_HILLS_WITH_TREES: i32 = 34;
    pub const SAVANNA_ROCK: i32 = 36;
    pub const MESA: i32 = 37;
    pub const MESA_PLATEAU_F: i32 = 38;
    pub const MESA_ROCK: i32 = 38;
    pub const MESA_PLATEAU: i32 = 39;
    pub const MESA_CLEAR_ROCK: i32 = 39;
    pub const VOID: i32 = 127;
    pub const MUTATED_PLAINS: i32 = 129;
    pub const DESERT_M: i32 = 130;
    pub const MUTATED_DESERT: i32 = 130;
    pub const EXTREME_HILLS_M: i32 = 131;
    pub const MUTATED_EXTREME_HILLS: i32 = 131;
    pub const MUTATED_FOREST: i32 = 132;
    pub const TAIGA_M: i32 = 133;
    pub const MUTATED_TAIGA: i32 = 133;
    pub const SWAMPLAND_M: i32 = 134;
    pub const MUTATED_SWAMPLAND: i32 = 134;
    pub const ICE_PLAINS_SPIKES: i32 = 140;
    pub const MUTATED_ICE_FLATS: i32 = 140;
    pub const JUNGLE_M: i32 = 149;
    pub const MUTATED_JUNGLE: i32 = 149;
    pub const JUNGLE_EDGE_M: i32 = 151;
    pub const MUTATED_JUNGLE_EDGE: i32 = 151;
    pub const BIRCH_FOREST_M: i32 = 155;
    pub const MUTATED_BIRCH_FOREST: i32 = 155;
    pub const BIRCH_FOREST_HILLS_M: i32 = 156;
    pub const MUTATED_BIRCH_FOREST_HILLS: i32 = 156;
    pub const ROOFED_FOREST_M: i32 = 157;
    pub const MUTATED_ROOFED_FOREST: i32 = 157;
    pub const COLD_TAIGA_M: i32 = 158;
    pub const MUTATED_TAIGA_COLD: i32 = 158;
    pub const MEGA_SPRUCE_TAIGA: i32 = 160;
    pub const MUTATED_REDWOOD_TAIGA: i32 = 160;
}

/// `Biomes.REGISTRY.get(id)`.
pub fn get(id: i32) -> Option<&'static Biome> {
    REGISTRY.iter().find(|b| b.id == id)
}

/// A biome by name, as BiomeFixer fills its registry.
pub fn by_name(name: &str) -> Option<&'static Biome> {
    REGISTRY.iter().find(|b| b.name == name)
}

impl Biome {
    pub fn temperature_group(&self) -> Temperature {
        if self.category == Category::Ocean {
            Temperature::Ocean
        } else if self.temperature < 0.2 {
            Temperature::Cold
        } else if self.temperature < 1.0 {
            Temperature::Medium
        } else {
            Temperature::Warm
        }
    }

    pub fn is_ocean(id: i32) -> bool {
        use biomes::*;
        [WARM_OCEAN, LUKEWARM_OCEAN, OCEAN, COLD_OCEAN, FROZEN_OCEAN, DEEP_WARM_OCEAN, DEEP_LUKEWARM_OCEAN, DEEP_OCEAN, DEEP_COLD_OCEAN, DEEP_FROZEN_OCEAN].contains(&id)
    }

    pub fn is_river(id: i32) -> bool {
        id == biomes::RIVER || id == biomes::FROZEN_RIVER
    }
}
