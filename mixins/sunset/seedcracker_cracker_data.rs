// kaptainwutax.seedcrackerX.cracker: BiomeData, PillarData, HashedSeedData and the storage's ScheduledSet. What the Java
// asks of the game (the log, the config, the biome generator, the database) is the `Host` trait, so the cracker runs, and
// is tested, without a game.
use super::sunset_scx_feature::FeatureData;
use super::sunset_scx_rand::JRand;
use super::sunset_scx_seeds::pillar_seed;
use super::sunset_scx_seeds::world_seed;
use super::sunset_scx_version::MCVersion;
use std::cmp::Ordering;
use std::sync::Arc;

/// What a biome generator answers (`BiomeSource`), for the biome phase of the search.
pub trait BiomeSource: Send {
    fn get_biome(&self, x: i32, y: i32, z: i32) -> i32;
    fn get_biome_for_noise_gen(&self, x: i32, y: i32, z: i32) -> i32;
}

/// `new OverworldBiomeSource(version, worldSeed)`, made by the game: `None` where it cannot.
pub type BiomeSourceFactory = Arc<dyn Fn(MCVersion, i64) -> Option<Box<dyn BiomeSource>> + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStage {
    Pillars, LiftingLow, LiftingHigh, Structures, Reduce, Decorators, Hash, Biomes,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchProgress {
    pub stage: SearchStage,
    pub checked: u64,
    /// Zero means indeterminate; a phase is not an overall search percentage.
    pub total: u64,
}

/// `Log` and `Config` and the rest of what the cracker reaches for.
pub trait Host: Send + Sync {
    fn cancelled(&self) -> bool { false }
    fn progress(&self, _progress: SearchProgress) {}
    /// `Log.debug`: the text as it is.
    fn debug(&self, message: &str);
    /// `Log.warn(key, args)`: a translated line in green.
    fn warn(&self, key: &str, args: &[String]);
    /// `Log.error(key)`: a translated line in red.
    fn error(&self, key: &str);
    /// `Log.printSeed(key, seed)`: the translated line with the seed, clickable, in place of `${SEED}`.
    fn print_seed(&self, key: &str, seed: i64);
    /// `Log.translate`.
    fn translate(&self, key: &str) -> String;
    /// `Config.get().getVersion()`.
    fn version(&self) -> MCVersion;
    /// `Config.get().debug`.
    fn debug_enabled(&self) -> bool;
    /// A world seed was found and is the only one: `SeedCracker.entrypoints.forEach(pushWorldSeed)`, then the database
    /// when the player asked for it and the server qualifies.
    fn world_seed_found(&self, seed: i64);
    /// The biome generator of the game, if it has one.
    fn biome_source_factory(&self) -> Option<BiomeSourceFactory>;
}

/// `BiomeData`: a biome seen at a place. Two are the same when their biome is (`equals`).
#[derive(Clone, Copy, Debug)]
pub struct BiomeData {
    pub biome: i32,
    pub x: i32,
    pub z: i32,
}

impl BiomeData {
    pub fn new(biome: i32, x: i32, z: i32) -> BiomeData {
        BiomeData { biome, x, z }
    }

    pub fn test(&self, version: MCVersion, source: &dyn BiomeSource) -> bool {
        if version.is_newer_or_equal_to(MCVersion::v1_15) {
            source.get_biome_for_noise_gen(self.x, 0, self.z) == self.biome
        } else {
            source.get_biome(self.x, 0, self.z) == self.biome
        }
    }
}

/// `HashedSeedData`: the hash of the world seed the server sent.
#[derive(Clone, Copy, Debug)]
pub struct HashedSeedData {
    hashed_seed: i64,
}

impl HashedSeedData {
    pub fn new(hashed_seed: i64) -> HashedSeedData {
        HashedSeedData { hashed_seed }
    }

    pub fn test(&self, seed: i64, _rand: &mut JRand) -> bool {
        world_seed::to_hash(seed) == self.hashed_seed
    }

    pub fn get_hashed_seed(&self) -> i64 {
        self.hashed_seed
    }
}

/// `PillarData`: the heights of the end pillars, in the order they were found.
#[derive(Clone, Debug)]
pub struct PillarData {
    heights: Vec<i32>,
}

impl PillarData {
    pub fn new(heights: Vec<i32>) -> PillarData {
        PillarData { heights }
    }

    pub fn test(&self, seed: i64) -> bool {
        // `(int) seed`, then java.util.Random(pillarSeed): the 16 bits the loop counts through.
        pillar_seed::get_pillar_heights(seed as i32 as i64)[..] == self.heights[..]
    }

    pub fn get_pillar_heights(&self, pillar_seed: i32) -> Vec<i32> {
        pillar_seed::get_pillar_heights(pillar_seed as i64).to_vec()
    }
}

/// `DataAddedEvent`: what runs, off the game's thread, once data has been stored.
pub type DataAddedEvent = Arc<dyn Fn(&super::sunset_scx_cracker_storage::DataStorage) + Send + Sync>;

/// `ScheduledSet`: a set that is added to in a queue and takes the queue in on `dump`, so a reader never sees it change
/// halfway. With a comparator it keeps its order, as the Java's TreeSet does: an element is put after the ones it ties with.
pub struct ScheduledSet<T> {
    base: Vec<T>,
    scheduled: Vec<T>,
    comparator: Option<fn(&T, &T) -> Ordering>,
    same: fn(&T, &T) -> bool,
}

impl<T: Clone> ScheduledSet<T> {
    pub fn new(comparator: Option<fn(&T, &T) -> Ordering>, same: fn(&T, &T) -> bool) -> ScheduledSet<T> {
        ScheduledSet { base: Vec::new(), scheduled: Vec::new(), comparator, same }
    }

    pub fn schedule_add(&mut self, e: T) {
        if !self.scheduled.iter().any(|s| (self.same)(s, &e)) {
            self.scheduled.push(e);
        }
    }

    pub fn dump(&mut self) {
        for e in std::mem::take(&mut self.scheduled) {
            if self.base.iter().any(|b| (self.same)(b, &e)) {
                continue;
            }
            match self.comparator {
                None => self.base.push(e),
                Some(cmp) => {
                    // A TreeSet with this comparator: it goes in front of the first element that sorts after it.
                    let at = self.base.iter().position(|b| cmp(&e, b) == Ordering::Less).unwrap_or(self.base.len());
                    self.base.insert(at, e);
                }
            }
        }
    }

    pub fn contains(&self, e: &T) -> bool {
        self.base.iter().any(|b| (self.same)(b, e)) || self.scheduled.iter().any(|s| (self.same)(s, e))
    }

    pub fn get_base_set(&mut self) -> &mut Vec<T> {
        &mut self.base
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.base.iter()
    }

    pub fn size(&self) -> usize {
        self.base.len()
    }

    pub(super) fn total_size(&self) -> usize {
        self.base.len() + self.scheduled.len()
    }
}

/// `DataStorage.Entry<Feature.Data<?>>`: the data and the event that runs when it was stored.
#[derive(Clone)]
pub struct FeatureEntry {
    pub data: FeatureData,
    pub event: DataAddedEvent,
}

/// `DataStorage.Entry<BiomeData>`.
#[derive(Clone)]
pub struct BiomeEntry {
    pub data: BiomeData,
    pub event: DataAddedEvent,
}

/// `ProgressListener`: the percent of the search that is done, printed every fourth step.
pub struct ProgressListener {
    progress: f32,
    count: i32,
}

/// Java's `Float.toString`: a whole number keeps its `.0`.
pub fn java_float(f: f32) -> String {
    let s = format!("{f}");
    if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
}

impl ProgressListener {
    pub fn new() -> ProgressListener {
        ProgressListener::with_progress(0.0)
    }

    pub fn with_progress(progress: f32) -> ProgressListener {
        ProgressListener { progress, count: 0 }
    }

    pub fn add_percent(&mut self, percent: f32, debug: bool, host: &dyn Host) {
        if host.cancelled() {
            return;
        }
        if (self.count & 3) == 0 && debug {
            host.debug(&format!("{}: {}%", host.translate("tmachine.progress"), java_float(self.progress)));
        }
        self.count += 1;
        self.progress += percent;
    }
}

impl Default for ProgressListener {
    fn default() -> Self {
        Self::new()
    }
}
