// kaptainwutax.seedcrackerX.finder: Finder and its Type and Category, the search positions that ReloadFinders builds for a
// height, FinderControl (the finders that found something) and FinderQueue (what runs when a chunk arrives). The finders
// themselves are in finders.rs.
use super::sunset_scx_config::{Config, FeatureToggle};
use super::sunset_scx_cracker_data::Host;
use super::sunset_scx_cracker_storage::DataStorage;
use super::sunset_scx_dimension::Dimension;
use super::sunset_scx_feature::HeightContext;
use super::sunset_scx_features::Features;
use super::sunset_scx_finder_piece::Cuboid;
use super::sunset_scx_level::{BlockPos, ChunkPos, Direction, Level};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Category {
    Structures,
    Decorators,
    Biomes,
}

impl Category {
    pub const ALL: [Category; 3] = [Category::Structures, Category::Decorators, Category::Biomes];

    /// `toString`: the name of the Java constant, which the commands use.
    pub fn name(self) -> &'static str {
        match self {
            Category::Structures => "STRUCTURES",
            Category::Decorators => "DECORATORS",
            Category::Biomes => "BIOMES",
        }
    }
}

/// `Finder.Type`, in the order the Java declares it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FinderType {
    BuriedTreasure,
    DesertTemple,
    EndCity,
    JungleTemple,
    Monument,
    SwampHut,
    Shipwreck,
    PillagerOutpost,
    Igloo,
    TrialChambers,
    EndPillars,
    EndGateway,
    Dungeon,
    EmeraldOre,
    DesertWell,
    WarpedFungus,
    Biome,
}

impl FinderType {
    pub const ALL: [FinderType; 17] = [
        FinderType::BuriedTreasure,
        FinderType::DesertTemple,
        FinderType::EndCity,
        FinderType::JungleTemple,
        FinderType::Monument,
        FinderType::SwampHut,
        FinderType::Shipwreck,
        FinderType::PillagerOutpost,
        FinderType::Igloo,
        FinderType::TrialChambers,
        FinderType::EndPillars,
        FinderType::EndGateway,
        FinderType::Dungeon,
        FinderType::EmeraldOre,
        FinderType::DesertWell,
        FinderType::WarpedFungus,
        FinderType::Biome,
    ];

    /// `toString`: the name of the Java constant.
    pub fn name(self) -> &'static str {
        match self {
            FinderType::BuriedTreasure => "BURIED_TREASURE",
            FinderType::DesertTemple => "DESERT_TEMPLE",
            FinderType::EndCity => "END_CITY",
            FinderType::JungleTemple => "JUNGLE_TEMPLE",
            FinderType::Monument => "MONUMENT",
            FinderType::SwampHut => "SWAMP_HUT",
            FinderType::Shipwreck => "SHIPWRECK",
            FinderType::PillagerOutpost => "PILLAGER_OUTPOST",
            FinderType::Igloo => "IGLOO",
            FinderType::TrialChambers => "TRIAL_CHAMBERS",
            FinderType::EndPillars => "END_PILLARS",
            FinderType::EndGateway => "END_GATEWAY",
            FinderType::Dungeon => "DUNGEON",
            FinderType::EmeraldOre => "EMERALD_ORE",
            FinderType::DesertWell => "DESERT_WELL",
            FinderType::WarpedFungus => "WARPED_FUNGUS",
            FinderType::Biome => "BIOME",
        }
    }

    /// The translation key of the finder's name.
    pub fn name_key(self) -> &'static str {
        match self {
            FinderType::BuriedTreasure => "finder.buriedTreasures",
            FinderType::DesertTemple => "finder.desertTemples",
            FinderType::EndCity => "finder.endCities",
            FinderType::JungleTemple => "finder.jungleTemples",
            FinderType::Monument => "finder.monuments",
            FinderType::SwampHut => "finder.swampHuts",
            FinderType::Shipwreck => "finder.shipwrecks",
            FinderType::PillagerOutpost => "finder.outposts",
            FinderType::Igloo => "finder.igloo",
            FinderType::TrialChambers => "finder.trialChambers",
            FinderType::EndPillars => "finder.endPillars",
            FinderType::EndGateway => "finder.endGateways",
            FinderType::Dungeon => "finder.dungeons",
            FinderType::EmeraldOre => "finder.emeraldOres",
            FinderType::DesertWell => "finder.desertWells",
            FinderType::WarpedFungus => "finder.warpedFungus",
            FinderType::Biome => "finder.biomes",
        }
    }

    pub fn category(self) -> Category {
        match self {
            FinderType::BuriedTreasure
            | FinderType::DesertTemple
            | FinderType::EndCity
            | FinderType::JungleTemple
            | FinderType::Monument
            | FinderType::SwampHut
            | FinderType::Shipwreck
            | FinderType::PillagerOutpost
            | FinderType::Igloo
            | FinderType::TrialChambers => Category::Structures,
            FinderType::Biome => Category::Biomes,
            _ => Category::Decorators,
        }
    }

    /// `Type.enabled`: the toggle in the config.
    pub fn enabled(self, config: &Config) -> &FeatureToggle {
        match self {
            FinderType::BuriedTreasure => &config.buried_treasure,
            FinderType::DesertTemple => &config.desert_temple,
            FinderType::EndCity => &config.end_city,
            FinderType::JungleTemple => &config.jungle_temple,
            FinderType::Monument => &config.monument,
            FinderType::SwampHut => &config.swamp_hut,
            FinderType::Shipwreck => &config.shipwreck,
            FinderType::PillagerOutpost => &config.outpost,
            FinderType::Igloo => &config.igloo,
            FinderType::TrialChambers => &config.trial_chambers,
            FinderType::EndPillars => &config.end_pillars,
            FinderType::EndGateway => &config.end_gateway,
            FinderType::Dungeon => &config.dungeon,
            FinderType::EmeraldOre => &config.emerald_ore,
            FinderType::DesertWell => &config.desert_well,
            FinderType::WarpedFungus => &config.warped_fungus,
            FinderType::Biome => &config.biome,
        }
    }

    pub fn get_for_category(category: Category) -> Vec<FinderType> {
        Self::ALL.into_iter().filter(|t| t.category() == category).collect()
    }
}

/// The search positions `ReloadFinders.reloadHeight` builds for a dimension's height. Positions in a chunk are local:
/// x and z from 0 to 15.
pub struct SearchPositions {
    pub height_context: HeightContext,
    pub chunk_positions: Arc<Vec<BlockPos>>,
    pub emerald_ore: Arc<Vec<BlockPos>>,
    pub bedrock_marker: Arc<Vec<BlockPos>>,
    pub temple: Arc<Vec<BlockPos>>,
    pub buried_treasure: Arc<Vec<BlockPos>>,
    pub end_city: Arc<Vec<BlockPos>>,
    pub monument: Arc<Vec<BlockPos>>,
    pub outpost: HashMap<Direction, Arc<Vec<BlockPos>>>,
    pub igloo: HashMap<Direction, Arc<Vec<BlockPos>>>,
    pub trial_chambers: HashMap<Direction, Arc<Vec<BlockPos>>>,
}

pub const IGLOO_SIZE: [i32; 3] = [7, 5, 8];
pub const OUTPOST_SIZE: [i32; 3] = [15, 21, 15];
pub const TRIAL_CHAMBERS_SIZE: [i32; 3] = [19, 20, 19];

/// `buildSearchPositions(base, removeIf)`.
fn build_search_positions(base: &[BlockPos], remove_if: impl Fn(BlockPos) -> bool) -> Arc<Vec<BlockPos>> {
    Arc::new(base.iter().copied().filter(|p| !remove_if(*p)).collect())
}

/// `JigsawFinder.getSearchPositions(xRotation, zRotation, xOffset, zOffset, size)`.
pub fn jigsaw_search_positions(height: HeightContext, x_rotation: i32, z_rotation: i32, x_offset: i32, z_offset: i32, size: [i32; 3]) -> HashMap<Direction, Vec<BlockPos>> {
    let mut positions = HashMap::new();
    for direction in Direction::HORIZONTAL {
        let (x, z) = match direction {
            Direction::East => (x_rotation - size[2] + 1 + z_rotation - z_offset, z_rotation - x_rotation + x_offset),
            Direction::South => (x_rotation - size[0] + 1 + x_rotation - x_offset, z_rotation - size[2] + 1 + z_rotation - z_offset),
            Direction::West => (x_rotation - z_rotation + z_offset, z_rotation - size[0] + 1 + x_rotation - x_offset),
            Direction::North => (x_offset, z_offset),
        };
        let list: Vec<BlockPos> = (height.get_bottom_y()..height.get_top_y()).map(|y| BlockPos::new(x, y, z)).collect();
        positions.insert(direction, list);
    }
    positions
}

impl SearchPositions {
    /// `ReloadFinders.reloadHeight(minY, maxY)`.
    pub fn reload_height(min_y: i32, max_y: i32) -> SearchPositions {
        let mut chunk_positions = Vec::with_capacity(16 * 16 * (max_y - min_y).max(0) as usize);
        for x in 0..16 {
            for z in 0..16 {
                for y in min_y..max_y {
                    chunk_positions.push(BlockPos::new(x, y, z));
                }
            }
        }
        let height_context = HeightContext::new(min_y, max_y);
        let arc = |m: HashMap<Direction, Vec<BlockPos>>| m.into_iter().map(|(d, v)| (d, Arc::new(v))).collect::<HashMap<_, _>>();
        let mut outpost = jigsaw_search_positions(height_context, 0, 0, 0, 0, OUTPOST_SIZE);
        let additional = jigsaw_search_positions(height_context, 0, 0, 1, 0, OUTPOST_SIZE);
        for direction in Direction::HORIZONTAL {
            let extra = additional[&direction].clone();
            outpost.get_mut(&direction).unwrap().extend(extra);
        }
        SearchPositions {
            height_context,
            emerald_ore: build_search_positions(&chunk_positions, |p| p.y < 4 || p.y > 28 + 4),
            bedrock_marker: build_search_positions(&chunk_positions, |p| p.y < 76 || p.y > 76 + 3 * 10),
            temple: build_search_positions(&chunk_positions, |p| p.x != 0 || p.y < 0 || p.y > 200 || p.z != 0),
            buried_treasure: build_search_positions(&chunk_positions, |p| {
                if (p.x & 15) != 9 || (p.z & 15) != 9 {
                    return true;
                }
                p.y > 90 && p.y < 0
            }),
            end_city: build_search_positions(&chunk_positions, |p| p.y > 90 && p.y < 40),
            monument: build_search_positions(&chunk_positions, |p| p.y != 56),
            outpost: arc(outpost),
            igloo: arc(jigsaw_search_positions(height_context, 3, 5, 0, 0, IGLOO_SIZE)),
            trial_chambers: arc(jigsaw_search_positions(height_context, 0, 0, 0, 0, TRIAL_CHAMBERS_SIZE)),
            chunk_positions: Arc::new(chunk_positions),
        }
    }
}

/// What a finder needs: the features, the search positions, the storage it reports to, and the settings it reads.
pub struct FinderContext {
    pub features: Arc<Features>,
    pub search: Arc<SearchPositions>,
    pub storage: Arc<DataStorage>,
    pub anti_xray_bypass: bool,
}

impl FinderContext {
    pub fn host(&self) -> &Arc<dyn Host> {
        &self.storage.host
    }
}

/// What every finder has: where it looks, the world it looks in, and the outlines it found.
pub struct FinderBase {
    pub chunk_pos: ChunkPos,
    pub dimension: Dimension,
    pub cuboids: Vec<Cuboid>,
}

impl FinderBase {
    pub fn new(chunk_pos: ChunkPos, dimension: Dimension) -> FinderBase {
        FinderBase { chunk_pos, dimension, cuboids: Vec::new() }
    }
}

pub trait Finder: Send + Sync {
    fn base(&self) -> &FinderBase;
    fn base_mut(&mut self) -> &mut FinderBase;
    fn find_in_chunk(&mut self, ctx: &FinderContext, level: &dyn Level) -> Vec<BlockPos>;
    fn is_valid_dimension(&self, dimension: Dimension) -> bool;

    fn chunk_pos(&self) -> ChunkPos {
        self.base().chunk_pos
    }

    fn cuboids(&self) -> &[Cuboid] {
        &self.base().cuboids
    }

    /// `isUseless`.
    fn is_useless(&self) -> bool {
        self.base().cuboids.is_empty()
    }
}

/// `FinderControl`: the finders that found something, by type.
#[derive(Default)]
pub struct FinderControl {
    active: Mutex<HashMap<FinderType, Vec<Box<dyn Finder>>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl FinderControl {
    pub fn delete_finders(&self) {
        lock(&self.active).clear();
    }

    pub fn add_finder(&self, finder_type: FinderType, finder: Box<dyn Finder>) {
        if finder.is_useless() {
            return;
        }
        lock(&self.active).entry(finder_type).or_default().push(finder);
    }

    /// `getActiveFinders` with what is wanted of each: the useless ones are dropped first.
    pub fn with_active_finders<R>(&self, f: impl FnOnce(&mut dyn Iterator<Item = &Box<dyn Finder>>) -> R) -> R {
        let mut map = lock(&self.active);
        for finders in map.values_mut() {
            finders.retain(|finder| !finder.is_useless());
        }
        let mut it = map.values().flat_map(|v| v.iter());
        f(&mut it)
    }
}

/// How a finder type is built for a chunk: `Type.finderBuilder`.
pub type FinderBuilder = fn(&FinderContext, &dyn Level, ChunkPos) -> Vec<Box<dyn Finder>>;

type Job = Box<dyn FnOnce() + Send>;

/// `FinderQueue`: five threads that take a chunk and run every enabled finder on it.
pub struct FinderQueue {
    pub finder_control: Arc<FinderControl>,
    sender: Mutex<Sender<Job>>,
}

impl FinderQueue {
    pub fn new() -> FinderQueue {
        let (sender, receiver) = channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        for _ in 0..5 {
            let receiver = receiver.clone();
            std::thread::spawn(move || loop {
                let job = lock(&receiver).recv();
                match job {
                    Ok(job) => job(),
                    Err(_) => return,
                }
            });
        }
        FinderQueue { finder_control: Arc::new(FinderControl::default()), sender: Mutex::new(sender) }
    }

    /// `onChunkData(world, chunkPos)`.
    pub fn on_chunk_data(&self, ctx: Arc<FinderContext>, config: &Config, level: Arc<dyn Level>, chunk_pos: ChunkPos, builders: &'static [(FinderType, FinderBuilder)]) {
        if !config.active || ctx.storage.cancelled() || level.cancelled() {
            return;
        }
        for (finder_type, builder) in builders {
            if !finder_type.enabled(config).get() {
                continue;
            }
            let (finder_type, builder) = (*finder_type, *builder);
            let (ctx, level, control) = (ctx.clone(), level.clone(), self.finder_control.clone());
            let job: Job = Box::new(move || {
                if ctx.storage.cancelled() || level.cancelled() { return; }
                let finders = builder(&ctx, level.as_ref(), chunk_pos);
                for mut finder in finders {
                    if ctx.storage.cancelled() || level.cancelled() { return; }
                    if finder.is_valid_dimension(level.dimension()) {
                        finder.find_in_chunk(&ctx, level.as_ref());
                        control.add_finder(finder_type, finder);
                    }
                }
            });
            let _ = lock(&self.sender).send(job);
        }
    }

    pub fn clear(&mut self) {
        self.finder_control = Arc::new(FinderControl::default());
    }
}

impl Default for FinderQueue {
    fn default() -> Self {
        Self::new()
    }
}
