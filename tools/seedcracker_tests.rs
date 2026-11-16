// rustc --edition 2024 --test -O -D warnings tools/seedcracker_tests.rs -o <temporary-test-binary>
// Execute that binary to check the production engine without native Horizon dependencies.

#[path = "../mixins/sunset/seedcracker_version.rs"] pub mod sunset_scx_version;
#[path = "../mixins/sunset/seedcracker_mth.rs"] pub mod sunset_scx_mth;
#[path = "../mixins/sunset/seedcracker_lcg.rs"] pub mod sunset_scx_lcg;
#[path = "../mixins/sunset/seedcracker_sha256.rs"] pub mod sunset_scx_sha256;
#[path = "../mixins/sunset/seedcracker_rand.rs"] pub mod sunset_scx_rand;
#[path = "../mixins/sunset/seedcracker_seeds.rs"] pub mod sunset_scx_seeds;
#[path = "../mixins/sunset/seedcracker_dimension.rs"] pub mod sunset_scx_dimension;
#[path = "../mixins/sunset/seedcracker_biome.rs"] pub mod sunset_scx_biome;
#[path = "../mixins/sunset/seedcracker_worldgen_random.rs"] pub mod sunset_scx_worldgen_random;
#[path = "../mixins/sunset/seedcracker_feature.rs"] pub mod sunset_scx_feature;
#[path = "../mixins/sunset/seedcracker_features.rs"] pub mod sunset_scx_features;
#[path = "../mixins/sunset/seedcracker_cracker_data.rs"] pub mod sunset_scx_cracker_data;
#[path = "../mixins/sunset/seedcracker_cracker_storage.rs"] pub mod sunset_scx_cracker_storage;
#[path = "../mixins/sunset/seedcracker_cracker_time_machine.rs"] pub mod sunset_scx_cracker_time_machine;
#[path = "../mixins/sunset/seedcracker_config.rs"] pub mod sunset_scx_config;
#[path = "../mixins/sunset/seedcracker_lang.rs"] pub mod sunset_scx_lang;
#[path = "../mixins/sunset/seedcracker_database.rs"] pub mod sunset_scx_database;
#[path = "../mixins/sunset/seedcracker_level.rs"] pub mod sunset_scx_level;
#[path = "../mixins/sunset/seedcracker_finder_piece.rs"] pub mod sunset_scx_finder_piece;
#[path = "../mixins/sunset/seedcracker_finder_structures.rs"] pub mod sunset_scx_finder_structures;
#[path = "../mixins/sunset/seedcracker_finder.rs"] pub mod sunset_scx_finder;
#[path = "../mixins/sunset/seedcracker_finders.rs"] pub mod sunset_scx_finders;

mod arithmetic {
use crate::sunset_scx_feature::{DecoratorClass, Feature, FeatureData, Structure, StructureClass};
use crate::sunset_scx_lcg::LCG;
use crate::sunset_scx_rand::{ChunkRand, JRand};
use crate::sunset_scx_seeds::{pillar_seed, structure_seed, world_seed};
use crate::sunset_scx_sha256;
use crate::sunset_scx_version::{MCVersion, VersionMap};
use crate::sunset_scx_worldgen_random::WorldgenRandom;
use std::sync::Arc;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn sha256_matches_the_published_vectors() {
    assert_eq!(hex(&sunset_scx_sha256::digest(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(hex(&sunset_scx_sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(
        hex(&sunset_scx_sha256::digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn jrand_is_java_util_random() {
    // The values java.util.Random gives for these seeds.
    assert_eq!(JRand::new(42).next_int(), -1170105035);
    assert_eq!(JRand::new(42).next_long(), -5025562857975149833);
    assert_eq!(JRand::new(42).next_double(), 0.7275636800328681);
    assert_eq!(JRand::new(0).next_int_bound(100), 60);
    assert_eq!(JRand::new(0).next_gaussian(), 0.8025330637390305);
}

#[test]
fn the_static_forms_agree_with_the_generator() {
    let mut r = JRand::new(12345);
    let seed = r.get_seed();
    assert_eq!(JRand::next_int_at(LCG::JAVA.next_seed(seed)), r.next_int());
    let mut r = JRand::new(12345);
    assert_eq!(JRand::next_long_at(LCG::JAVA.next_seed(seed)), r.next_long());
    let mut r = JRand::new(12345);
    assert_eq!(JRand::next_double_at(LCG::JAVA.next_seed(seed)), r.next_double());
    // Not the powers of two: the Java's static form shifts the whole seed there where the generator shifts its 31 bits.
    for bound in [3, 24, 100, 1_000_003] {
        let mut r = JRand::new(999);
        let s = r.get_seed();
        assert_eq!(JRand::next_int_bound_at(LCG::JAVA.next_seed(s), bound), r.next_int_bound(bound));
    }
}

#[test]
fn an_lcg_steps_forward_and_back() {
    let seed = 0x1234_5678_9abc;
    let two = LCG::JAVA.combine_steps(2);
    let back = LCG::JAVA.combine_steps(-2);
    assert_eq!(two.next_seed(seed), LCG::JAVA.next_seed(LCG::JAVA.next_seed(seed)));
    assert_eq!(back.next_seed(two.next_seed(seed)), seed);
    assert_eq!(LCG::JAVA.invert().next_seed(LCG::JAVA.next_seed(seed)), seed);
}

#[test]
fn versions_compare_as_the_java_enum() {
    assert_eq!(MCVersion::latest(), MCVersion::v1_21_3);
    assert!(MCVersion::v1_21.is_older_than(MCVersion::v1_21_1));
    assert!(MCVersion::v1_18.is_newer_than(MCVersion::v1_17_1));
    assert!(MCVersion::v1_12_2.is_older_than(MCVersion::v1_13));
    assert!(MCVersion::v1_16_2.is_between(MCVersion::v1_16, MCVersion::v1_17));
    assert!(!MCVersion::v1_18.is_between(MCVersion::v1_16, MCVersion::v1_17));
    assert_eq!(MCVersion::from_string("1.18.2"), Some(MCVersion::v1_18_2));
    assert_eq!(MCVersion::v1_18.newer(), Some(MCVersion::v1_18_1));
    assert_eq!(MCVersion::v1_18.older(), Some(MCVersion::v1_17_1));
    assert_eq!(MCVersion::values().len(), 141);

    let map = VersionMap::new().add(MCVersion::v1_14, "a").add(MCVersion::v1_18, "b").add(MCVersion::v1_13, "c");
    assert_eq!(map.get_as_of(MCVersion::v1_16), Some(&"a"));
    assert_eq!(map.get_as_of(MCVersion::v1_21), Some(&"b"));
    assert_eq!(map.get_as_of(MCVersion::v1_12_2), None);
    assert_eq!(map.get_oldest_version(), Some(MCVersion::v1_13));
}

#[test]
fn chunk_rand_seedings_match_the_formulas() {
    let mut r = ChunkRand::new();
    // setRegionSeed: regionX * 341873128712 + regionZ * 132897987541 + worldSeed + salt, scrambled.
    let seed = r.set_region_seed(100, 2, 3, 14357617, MCVersion::latest());
    let expected = 2i64 * 341873128712 + 3 * 132897987541 + 100 + 14357617;
    assert_eq!(seed, expected & 0xFFFF_FFFF_FFFF);
    assert_eq!(r.get_seed(), expected ^ 0x5DEECE66D);
    // The population seed: the same chunk, the same world, the same answer; another chunk, another.
    let a = r.set_population_seed(42, 16, 32, MCVersion::latest());
    let b = r.set_population_seed(42, 16, 32, MCVersion::latest());
    let c = r.set_population_seed(42, 32, 16, MCVersion::latest());
    assert_eq!(a, b);
    assert_ne!(a, c);
    // Before 1.13 the two multipliers are made odd by another rule.
    let old = r.set_population_seed(42, 16, 32, MCVersion::v1_12_2);
    assert_ne!(old, a);
}

#[test]
fn pillar_seeds_are_what_the_end_makes_of_a_structure_seed() {
    for pillar in [0i64, 1, 12345, 65535] {
        let heights = pillar_seed::get_pillar_heights(pillar);
        let mut sorted = heights;
        sorted.sort();
        assert_eq!(sorted, [76, 79, 82, 85, 88, 91, 94, 97, 100, 103]);
    }
    // The structure seeds of a pillar seed lead back to it.
    let pillar = 4242;
    let mut it = pillar_seed::get_structure_seeds(pillar);
    for _ in 0..1000 {
        let s = it.next();
        assert_eq!(pillar_seed::from_structure_seed(s), pillar, "structure seed {s}");
    }
    // The heights name the pillar seed, as the End draws them with the structure seed's own random.
    let found = pillar_seed::from_pillar_heights(&pillar_seed::get_pillar_heights(pillar));
    assert!(found.contains(&pillar));
}

#[test]
fn world_seeds_of_a_structure_seed_and_the_hash_that_names_them() {
    let structure = 0x0123_4567_89ab_i64;
    let world = structure_seed::to_world_seed(structure, 0xBEEF);
    assert_eq!(world_seed::to_structure_seed(world), structure);
    let hash = world_seed::to_hash(world);
    assert_eq!(world_seed::from_hash(structure, hash), vec![world]);
    // The shadow seed is the other solution of the mixer.
    assert_ne!(world_seed::get_shadow_seed(world), world);
    assert_eq!(world_seed::get_shadow_seed(world_seed::get_shadow_seed(world)), world);
}

#[test]
fn a_random_world_seed_is_found_from_its_structure_seed() {
    for internal in [1i64, 777, 123456789, 0xABCD_EF12_3456 & 0xFFFF_FFFF_FFFF] {
        let world = JRand::of_internal_seed(internal).next_long();
        let structure = world_seed::to_structure_seed(world);
        let found = structure_seed::to_random_world_seeds(structure);
        assert!(found.contains(&world), "{world} not in {found:?}");
        assert!(world_seed::is_random(world));
    }
}

#[test]
fn worldgen_random_is_the_xoroshiro_of_the_game() {
    // The values Horizon's own XoroshiroRandom (checked against the game's worldgen) gives for these seeds.
    let cases: [(i64, i64, i64, [i32; 6], [i32; 4]); 5] = [
        (0, 3038984756725240190, -3694039286755638414, [15, 2, 2, 4, 14, 15], [308, 58, 48, 99]),
        (1, -1033667707219518978, 6451672561743293322, [6, 3, 3, 10, 2, 8], [129, 62, 61, 213]),
        (12345, -8118485274630516485, 8241557746459281790, [0, 13, 14, 0, 8, 11], [4, 261, 280, 5]),
        (-987654321, -8813114921905719051, -8071674531228274167, [2, 8, 5, 0, 2, 8], [43, 169, 111, 17]),
        (0x1234_5678_9abc_def0, 8158243457423347947, 5232595719026988595, [4, 3, 1, 4, 15, 6], [88, 77, 26, 86]),
    ];
    for (seed, a, b, small, big) in cases {
        let mut r = WorldgenRandom::new(seed);
        assert_eq!((r.next_long(), r.next_long()), (a, b), "seed {seed}");
        let mut r = WorldgenRandom::new(seed);
        assert_eq!([(); 6].map(|_| r.next_int_bound(16)), small, "seed {seed}");
        let mut r = WorldgenRandom::new(seed);
        assert_eq!([(); 4].map(|_| r.next_int_bound(320)), big, "seed {seed}");
    }
}

fn structure(class: StructureClass) -> Arc<Feature> {
    Arc::new(Feature::Structure(Structure::new(class, MCVersion::latest()).expect("a config for the latest version")))
}

/// Where a structure is for a structure seed, then the data at that place must test true for it, and not for most others.
#[test]
fn a_structure_is_started_where_the_seed_puts_it() {
    use StructureClass::*;
    let seed = 0x0000_9a3b_77c1_55d2_i64;
    for class in [Igloo, DesertPyramid, JunglePyramid, SwampHut, Shipwreck, Monument, TrialChambers, BuriedTreasure, Village] {
        let feature = structure(class);
        let Feature::Structure(s) = &*feature else { unreachable!() };
        let mut rand = ChunkRand::new();
        let mut checked = 0;
        for region_x in -3..3 {
            for region_z in -3..3 {
                let Some(pos) = s.get_in_region(seed, region_x, region_z, &mut rand) else { continue };
                let data = FeatureData::structure(&feature, pos.x, pos.z);
                assert!(data.test_start(seed, &mut rand), "{} at {pos:?}", s.name());
                let other = data.test_start(seed ^ 0x1_0000_0001, &mut rand);
                assert!(!other || class == BuriedTreasure, "{} also starts for another seed", s.name());
                checked += 1;
            }
        }
        assert!(checked > 0, "{}", s.name());
    }
}

#[test]
fn region_data_places_chunks_in_their_regions() {
    let feature = structure(StructureClass::Igloo);
    let Feature::Structure(s) = &*feature else { unreachable!() };
    // Spacing 32: chunk (-1, 0) is in region (-1, 0) at offset (31, 0); chunk (33, -33) in region (1, -2).
    let d = s.region_data(-1, 0);
    assert_eq!((d.region_x, d.region_z, d.offset_x, d.offset_z), (-1, 0, 31, 0));
    let d = s.region_data(33, -33);
    assert_eq!((d.region_x, d.region_z, d.offset_x, d.offset_z), (1, -2, 1, 31));
    assert_eq!(s.get_offset(), 24); // 32 - 8
}

#[test]
fn structure_configs_follow_the_version() {
    assert!(Structure::new(StructureClass::Igloo, MCVersion::v1_8).is_none());
    assert_eq!(Structure::new(StructureClass::Igloo, MCVersion::v1_12_2).unwrap().salt(), 14357617);
    assert_eq!(Structure::new(StructureClass::Igloo, MCVersion::v1_13).unwrap().salt(), 14357618);
    let wreck = Structure::new(StructureClass::Shipwreck, MCVersion::v1_16).unwrap();
    assert_eq!((wreck.spacing(), wreck.separation(), wreck.get_offset()), (24, 4, 20));
    assert!(Structure::new(StructureClass::TrialChambers, MCVersion::v1_20_6).is_none());
    assert!(Structure::new(StructureClass::PillagerOutpost, MCVersion::latest()).unwrap().village.is_some());
}

#[test]
fn decorators_exist_by_version() {
    use crate::sunset_scx_feature::Decorator;
    assert!(Decorator::new(DecoratorClass::Dungeon, MCVersion::v1_12_2).is_none());
    let d = Decorator::new(DecoratorClass::Dungeon, MCVersion::v1_16_5).unwrap();
    // 1.16: step 3, index 2, but 3 and 3 in the desert and the swamp.
    assert_eq!((d.get_step(None), d.get_index(None)), (3, 2));
    assert_eq!((d.get_step(Some(crate::sunset_scx_biome::biomes::DESERT)), d.get_index(Some(crate::sunset_scx_biome::biomes::DESERT))), (3, 3));
    let d = Decorator::new(DecoratorClass::Dungeon, MCVersion::latest()).unwrap();
    assert_eq!((d.get_step(None), d.get_index(None)), (3, 2));
}
}

mod cracker {
// The cracker end to end: structures are put where a made-up world has them, handed to the storage as the finders would,
// and the search must come back with that world's seeds.
use crate::sunset_scx_cracker_data::{BiomeSourceFactory, DataAddedEvent, HashedSeedData, Host, PillarData, SearchProgress, SearchStage};
use crate::sunset_scx_cracker_storage::DataStorage;
use crate::sunset_scx_cracker_time_machine::Phase;
use crate::sunset_scx_feature::{Feature, FeatureData, Structure, StructureClass};
use crate::sunset_scx_rand::ChunkRand;
use crate::sunset_scx_seeds::{pillar_seed, world_seed};
use crate::sunset_scx_version::MCVersion;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
struct TestHost {
    lines: Mutex<Vec<String>>,
    world_seed: Mutex<Option<i64>>,
    cancelled: AtomicBool,
    cancel_at: Mutex<Option<(SearchStage, u64)>>,
    panic_at: Mutex<Option<(SearchStage, u64)>>,
    stages: Mutex<Vec<SearchStage>>,
    last_progress: Mutex<Option<SearchProgress>>,
    late_progress: AtomicUsize,
}

impl Host for TestHost {
    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    fn progress(&self, progress: SearchProgress) {
        if self.cancelled() {
            self.late_progress.fetch_add(1, Ordering::Relaxed);
        }
        assert!(progress.total == 0 || progress.checked <= progress.total, "{progress:?}");
        let panic_at = *self.panic_at.lock().unwrap();
        if panic_at.is_some_and(|(stage, count)| stage == progress.stage && progress.checked >= count) {
            panic!("intentional solver worker panic");
        }
        let mut stages = self.stages.lock().unwrap();
        if !stages.contains(&progress.stage) {
            stages.push(progress.stage);
        }
        *self.last_progress.lock().unwrap() = Some(progress);
        if self.cancel_at.lock().unwrap().is_some_and(|(stage, count)| stage == progress.stage && progress.checked >= count) {
            self.cancelled.store(true, Ordering::Release);
        }
    }
    fn debug(&self, message: &str) {
        self.lines.lock().unwrap().push(message.to_owned());
    }
    fn warn(&self, key: &str, args: &[String]) {
        self.lines.lock().unwrap().push(format!("{key} {args:?}"));
    }
    fn error(&self, key: &str) {
        self.lines.lock().unwrap().push(format!("ERROR {key}"));
    }
    fn print_seed(&self, key: &str, seed: i64) {
        self.lines.lock().unwrap().push(format!("{key} {seed}"));
    }
    fn translate(&self, key: &str) -> String {
        key.to_owned()
    }
    fn version(&self) -> MCVersion {
        MCVersion::latest()
    }
    fn debug_enabled(&self) -> bool {
        false
    }
    fn world_seed_found(&self, seed: i64) {
        assert!(!self.cancelled(), "accepted a seed after retirement");
        *self.world_seed.lock().unwrap() = Some(seed);
    }
    fn biome_source_factory(&self) -> Option<BiomeSourceFactory> {
        None
    }
}

fn structure(class: StructureClass) -> Arc<Feature> {
    Arc::new(Feature::Structure(Structure::new(class, MCVersion::latest()).unwrap()))
}

/// The structures of a world, as the finders would see them: one of each of the classes, around the origin.
fn structures_of(structure_seed: i64, classes: &[StructureClass], regions: &[(i32, i32)]) -> Vec<FeatureData> {
    let mut rand = ChunkRand::new();
    let mut out = Vec::new();
    for &class in classes {
        let feature = structure(class);
        let Feature::Structure(s) = &*feature else { unreachable!() };
        for &(rx, rz) in regions {
            if let Some(pos) = s.get_in_region(structure_seed, rx, rz, &mut rand) {
                out.push(FeatureData::structure(&feature, pos.x, pos.z));
            }
        }
    }
    out
}

fn poke(phase: Phase) -> DataAddedEvent {
    Arc::new(move |s: &DataStorage| s.get_time_machine().poke(s, Some(phase)))
}

#[test]
fn liftable_structures_and_the_hashed_seed_give_the_world_seed() {
    let structure_seed = 0x0000_5c3a_91d7_0e42_i64;
    let world = (0x4b1d_i64 << 48) | structure_seed;
    let host = Arc::new(TestHost::default());
    let storage = DataStorage::new(host.clone());
    storage.get_time_machine().set_threads(8);

    use StructureClass::*;
    let found = structures_of(structure_seed, &[Igloo, DesertPyramid, JunglePyramid, SwampHut, Shipwreck, PillagerOutpost], &[(0, 0), (0, 1)]);
    assert!(found.len() >= 10, "{}", found.len());
    storage.add_hashed_seed_data(HashedSeedData::new(world_seed::to_hash(world)), poke(Phase::Biomes));
    for d in found {
        storage.add_base_data(d, poke(Phase::Lifting));
    }
    assert!(storage.add_base_data(structures_of(structure_seed, &[Monument], &[(0, 0)]).remove(0), poke(Phase::Lifting)));
    storage.tick().unwrap().join().unwrap();

    assert!(storage.get_lifting_bits() >= 40.0, "{}", storage.get_lifting_bits());
    let tm = storage.get_time_machine();
    assert!(tm.structure_seeds().contains(&structure_seed), "{:?}\n{:?}", tm.structure_seeds(), host.lines.lock().unwrap());
    assert_eq!(*host.world_seed.lock().unwrap(), Some(world), "{:?}", host.lines.lock().unwrap());
}

#[test]
fn the_pillars_of_the_end_name_the_pillar_seed() {
    let structure_seed = 0x0000_1234_5678_9abc_i64;
    let host = Arc::new(TestHost::default());
    let storage = DataStorage::new(host.clone());
    let heights = pillar_seed::get_pillar_heights(pillar_seed::from_structure_seed(structure_seed)).to_vec();
    storage.add_pillar_data(PillarData::new(heights), poke(Phase::Pillars));
    storage.tick().unwrap().join().unwrap();
    let pillars = storage.get_time_machine().pillar_seeds().unwrap();
    assert!(pillars.contains(&(pillar_seed::from_structure_seed(structure_seed) as i32)), "{pillars:?}");
}

/// The whole End route: five end cities and the pillars name one structure seed among 2^32 candidates. It scans them all,
/// so it takes a minute: `cargo test --release -- --ignored`.
#[test]
#[ignore]
fn end_cities_and_pillars_give_the_structure_seed() {
    let structure_seed = 0x0000_77aa_0f31_c4d9_i64;
    let host = Arc::new(TestHost::default());
    let storage = DataStorage::new(host.clone());
    storage.get_time_machine().set_threads(8);
    let heights = pillar_seed::get_pillar_heights(pillar_seed::from_structure_seed(structure_seed)).to_vec();
    storage.add_pillar_data(PillarData::new(heights), poke(Phase::Pillars));
    let cities = structures_of(structure_seed, &[StructureClass::EndCity], &[(0, 0), (0, 1), (1, 0), (1, 1), (-1, 0), (0, -1), (-1, -1)]);
    for d in cities {
        storage.add_base_data(d, poke(Phase::Structures));
    }
    storage.tick().unwrap().join().unwrap();
    let seeds = storage.get_time_machine().structure_seeds();
    assert!(seeds.contains(&structure_seed), "{seeds:?}\n{:?}", host.lines.lock().unwrap());
}

fn liftable_storage(host: Arc<TestHost>) -> Arc<DataStorage> {
    let storage = DataStorage::new(host);
    storage.get_time_machine().set_threads(1);
    let seed = 0x0000_5c3a_91d7_0e42_i64;
    use StructureClass::*;
    for data in structures_of(seed, &[Igloo, DesertPyramid, JunglePyramid, SwampHut, Shipwreck], &[(0, 0), (0, 1)]) {
        storage.add_base_data(data, poke(Phase::Lifting));
    }
    assert!(storage.observation_count() >= 10);
    storage
}

#[test]
fn idle_storage_does_not_dispatch_and_panics_release_the_running_flag() {
    let storage = DataStorage::new(Arc::new(TestHost::default()));
    for _ in 0..10 {
        assert!(storage.tick().is_none());
        assert!(!storage.get_time_machine().is_running());
    }
    let entered = Arc::new(std::sync::Barrier::new(2));
    let release = entered.clone();
    storage.schedule(Arc::new(move |_| {
        release.wait();
        release.wait();
        panic!("intentional dispatch panic");
    }));
    let handle = storage.tick().unwrap();
    entered.wait();
    assert!(storage.get_time_machine().is_running());
    assert!(storage.tick().is_none(), "dispatched a second solver");
    entered.wait();
    assert!(handle.join().is_err());
    assert!(!storage.get_time_machine().is_running());
    storage.schedule(Arc::new(|_| {}));
    storage.tick().unwrap().join().unwrap();
    assert!(!storage.get_time_machine().is_running());
}

#[test]
fn clear_forgets_hash_and_observations_and_retires_old_callbacks() {
    let host = Arc::new(TestHost::default());
    let storage = DataStorage::new(host);
    storage.add_hashed_seed_data(HashedSeedData::new(0), poke(Phase::Biomes));
    storage.add_pillar_data(PillarData::new(vec![76; 10]), Arc::new(|_| {}));
    storage.add_base_data(structures_of(77, &[StructureClass::Igloo], &[(0, 0)])[0].clone(), Arc::new(|_| {}));
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let child = barrier.clone();
    let attempted = Arc::new(AtomicBool::new(false));
    let attempted_by_child = attempted.clone();
    storage.schedule(Arc::new(move |storage| {
        child.wait();
        child.wait();
        attempted_by_child.store(true, Ordering::Release);
        assert!(!storage.add_hashed_seed_data(HashedSeedData::new(55), Arc::new(|_| {})));
        storage.schedule(Arc::new(|_| panic!("retired callback restarted")));
    }));
    let handle = storage.tick().unwrap();
    barrier.wait();
    storage.clear();
    barrier.wait();
    handle.join().unwrap();
    assert!(attempted.load(Ordering::Acquire));
    assert!(storage.hashed_seed_data.lock().unwrap().is_none());
    assert!(storage.pillar_data.lock().unwrap().is_none());
    assert_eq!(storage.observation_count(), 0);
    assert_eq!(storage.get_time_machine().structure_seed_count(), 0);
    assert_eq!(storage.get_time_machine().world_seed_count(), 0);
    assert!(!storage.get_time_machine().is_running());
    assert!(storage.tick().is_none());
}

#[test]
fn concurrent_finders_cannot_exceed_observation_or_event_capacity() {
    use crate::sunset_scx_cracker_data::BiomeData;
    let storage = DataStorage::new(Arc::new(TestHost::default()));
    let dispatched = Arc::new(AtomicUsize::new(0));
    let count = dispatched.clone();
    let event: DataAddedEvent = Arc::new(move |_| { count.fetch_add(1, Ordering::Relaxed); });
    let accepted = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for thread in 0..4 {
            let (storage, event, accepted) = (&storage, &event, &accepted);
            scope.spawn(move || {
                for id in 0..2048 {
                    if storage.add_biome_data(BiomeData::new(thread * 2048 + id, 0, 0), event.clone()) {
                        accepted.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    assert_eq!(accepted.load(Ordering::Relaxed), 4096);
    assert_eq!(storage.observation_count(), 4096);
    assert!(!storage.add_pillar_data(PillarData::new(vec![76; 10]), event.clone()));
    assert!(!storage.add_base_data(structures_of(77, &[StructureClass::Igloo], &[(0, 0)])[0].clone(), event.clone()));
    assert!(storage.add_hashed_seed_data(HashedSeedData::new(0), event.clone()));
    for _ in 0..10000 {
        storage.schedule(event.clone());
    }
    storage.tick().unwrap().join().unwrap();
    assert_eq!(dispatched.load(Ordering::Relaxed), 4097, "unbounded event backlog");
    assert!(storage.tick().is_none());
    storage.clear();
    assert_eq!(storage.observation_count(), 0);
}

#[test]
fn termination_and_host_cancellation_cannot_restart_retired_storage() {
    for through_host in [false, true] {
        let host = Arc::new(TestHost::default());
        let storage = DataStorage::new(host.clone());
        storage.schedule(Arc::new(|_| panic!("cancelled work ran")));
        if through_host {
            host.cancelled.store(true, Ordering::Release);
        } else {
            storage.get_time_machine().terminate();
        }
        assert!(storage.tick().is_none());
        host.cancelled.store(false, Ordering::Release);
        storage.schedule(Arc::new(|_| panic!("cancelled storage restarted")));
        assert!(storage.tick().is_none());
        assert!(!storage.add_hashed_seed_data(HashedSeedData::new(11), poke(Phase::Biomes)));
        assert!(host.last_progress.lock().unwrap().is_none());
    }
}

#[test]
fn actual_lifting_loops_observe_mid_search_cancellation() {
    for stage in [SearchStage::LiftingLow, SearchStage::LiftingHigh] {
        let host = Arc::new(TestHost::default());
        *host.cancel_at.lock().unwrap() = Some((stage, 1024));
        let storage = liftable_storage(host.clone());
        let start = std::time::Instant::now();
        storage.tick().unwrap().join().unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        assert!(host.cancelled());
        let progress = host.last_progress.lock().unwrap().unwrap();
        assert_eq!(progress.stage, stage);
        assert!(progress.checked >= 1024 && progress.checked < progress.total);
        assert_eq!(host.late_progress.load(Ordering::Relaxed), 0);
        assert!(host.world_seed.lock().unwrap().is_none());
        assert!(!storage.get_time_machine().is_running());
        assert!(storage.tick().is_none());
    }
}

#[test]
fn a_panicking_lifting_worker_cancels_its_siblings_and_releases_dispatch() {
    let host = Arc::new(TestHost::default());
    *host.panic_at.lock().unwrap() = Some((SearchStage::LiftingHigh, 1024));
    let storage = liftable_storage(host);
    storage.get_time_machine().set_threads(2);
    let start = std::time::Instant::now();
    assert!(storage.tick().unwrap().join().is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
    assert!(storage.get_time_machine().should_terminate());
    assert!(!storage.get_time_machine().is_running());
    assert!(storage.tick().is_none());
}

#[test]
fn pre_cancelled_lifting_does_not_publish_progress() {
    let host = Arc::new(TestHost::default());
    let storage = liftable_storage(host.clone());
    storage.get_time_machine().terminate();
    assert!(storage.tick().is_none());
    storage.get_time_machine().poke(&storage, Some(Phase::Lifting));
    assert!(host.last_progress.lock().unwrap().is_none());
    assert!(host.world_seed.lock().unwrap().is_none());
}

#[test]
fn actual_hash_loop_observes_mid_search_cancellation() {
    let seed = 0x4b1d_5c3a_91d7_0e42_i64;
    let host = Arc::new(TestHost::default());
    *host.cancel_at.lock().unwrap() = Some((SearchStage::Hash, 512));
    let storage = DataStorage::new(host.clone());
    storage.get_time_machine().add_structure_seed(world_seed::to_structure_seed(seed));
    storage.add_hashed_seed_data(HashedSeedData::new(world_seed::to_hash(seed)), poke(Phase::Biomes));
    storage.tick().unwrap().join().unwrap();
    assert!(host.cancelled());
    assert_eq!(host.last_progress.lock().unwrap().unwrap().checked, 512);
    assert_eq!(host.late_progress.load(Ordering::Relaxed), 0);
    assert_eq!(storage.get_time_machine().world_seed_count(), 0);
    assert!(host.world_seed.lock().unwrap().is_none());
    assert!(storage.tick().is_none());
}

#[test]
fn hash_cancellation_discards_even_an_earlier_matching_candidate() {
    use std::cell::Cell;
    let calls = Cell::new(0);
    let seed = 42;
    let result = world_seed::from_hash_cancellable(seed, world_seed::to_hash(seed), || {
        let n = calls.get();
        calls.set(n + 1);
        n >= 2
    });
    assert_eq!(calls.get(), 3);
    assert!(result.is_empty());
    assert!(world_seed::from_hash_cancellable(seed, world_seed::to_hash(seed), || true).is_empty());
}

fn dungeon_observations(world: i64) -> Vec<FeatureData> {
    use crate::sunset_scx_feature::{Decorator, DecoratorClass};
    use crate::sunset_scx_worldgen_random::WorldgenRandom;
    let feature = Arc::new(Feature::Decorator(Decorator::new(DecoratorClass::Dungeon, MCVersion::latest()).unwrap()));
    let Feature::Decorator(decorator) = &*feature else { unreachable!() };
    let mut rand = WorldgenRandom::new(0);
    [(0, 0), (3, -2), (-4, 1), (7, 9), (-5, -6)].into_iter().map(|(x, z)| {
        let population = rand.set_decoration_seed(world, x << 4, z << 4);
        rand.set_feature_seed(population, decorator.get_index(None), decorator.get_step(None));
        let bx = (x << 4) + rand.next_int_bound(16);
        let bz = (z << 4) + rand.next_int_bound(16);
        let by = rand.next_int_bound(320);
        FeatureData::dungeon(&feature, bx, by, bz, None, None, None, None)
    }).collect()
}

#[test]
fn a_conflicting_decorator_singleton_cannot_block_the_verified_hash() {
    let structure = 0x1234_5678_9abc;
    let bad_world = (0x1234_i64 << 48) | structure;
    let right_world = (-0x2345_i64 << 48) | structure;
    let host = Arc::new(TestHost::default());
    let storage = DataStorage::new(host.clone());
    storage.get_time_machine().add_structure_seed(structure);
    for data in dungeon_observations(bad_world) {
        storage.add_base_data(data, poke(Phase::Biomes));
    }
    storage.tick().unwrap().join().unwrap();
    assert_eq!(storage.get_time_machine().world_seeds(), [bad_world].into_iter().collect());
    assert!(host.world_seed.lock().unwrap().is_none(), "unverified singleton was accepted");
    host.stages.lock().unwrap().clear();
    storage.add_hashed_seed_data(HashedSeedData::new(world_seed::to_hash(right_world)), poke(Phase::Biomes));
    storage.tick().unwrap().join().unwrap();
    assert_eq!(*host.world_seed.lock().unwrap(), Some(right_world));
    assert_eq!(storage.get_time_machine().world_seeds(), [right_world].into_iter().collect());
    assert_eq!(*host.stages.lock().unwrap(), vec![SearchStage::Hash]);
}

#[test]
fn absent_zero_and_mismatching_hashes_never_become_unverified_fallbacks() {
    let world = 0x1234_5678_9abc_i64;
    for hash in [0, world_seed::to_hash(world + 1)] {
        let host = Arc::new(TestHost::default());
        let storage = DataStorage::new(host.clone());
        storage.get_time_machine().add_structure_seed(world);
        for data in dungeon_observations(world) {
            storage.add_base_data(data, poke(Phase::Biomes));
        }
        storage.add_hashed_seed_data(HashedSeedData::new(hash), poke(Phase::Biomes));
        storage.tick().unwrap().join().unwrap();
        assert!(host.world_seed.lock().unwrap().is_none());
        assert_eq!(storage.get_time_machine().world_seed_count(), 0);
        assert_eq!(storage.hashed_seed_data.lock().unwrap().unwrap().get_hashed_seed(), hash);
        assert_eq!(*host.stages.lock().unwrap(), vec![SearchStage::Hash]);
    }
}

#[test]
fn zero_world_seed_and_multiple_structure_candidates_use_the_hash() {
    for world in [0, -0x1234_5678_1234_5678_i64] {
        let host = Arc::new(TestHost::default());
        let storage = DataStorage::new(host.clone());
        let tm = storage.get_time_machine();
        tm.set_structure_seeds([world_seed::to_structure_seed(world), world_seed::to_structure_seed(world + 1)].into_iter().collect());
        storage.add_hashed_seed_data(HashedSeedData::new(world_seed::to_hash(world)), poke(Phase::Biomes));
        storage.tick().unwrap().join().unwrap();
        assert_eq!(*host.world_seed.lock().unwrap(), Some(world));
        assert_eq!(tm.structure_seed_count(), 2);
        assert_eq!(tm.world_seed_count(), 1);
    }
}

}

mod finders {
// The finders against a made-up world: a structure's blocks are put in the world where the structure would be, a chunk is
// handed to the finder, and the data it gives the storage must be the structure's.
use crate::sunset_scx_biome::biomes;
use crate::sunset_scx_config::Config;
use crate::sunset_scx_cracker_data::{BiomeSourceFactory, Host};
use crate::sunset_scx_cracker_storage::DataStorage;
use crate::sunset_scx_dimension::Dimension;
use crate::sunset_scx_feature::{Detail, Feature};
use crate::sunset_scx_features::Features;
use crate::sunset_scx_finder::{Finder, FinderContext, SearchPositions};
use crate::sunset_scx_finder_piece::{PieceFinder, PieceKind};
use crate::sunset_scx_finder_structures as dsl;
use crate::sunset_scx_finders::{BuriedTreasureFinder, DesertWellFinder, EndGatewayFinder, IglooFinder, ShipwreckFinder, TempleFinder, TrialChambersFinder};
use crate::sunset_scx_level::{BlockPos, ChunkPos, Direction, Level};
use crate::sunset_scx_version::MCVersion;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct NoHost;

impl Host for NoHost {
    fn debug(&self, _: &str) {}
    fn warn(&self, _: &str, _: &[String]) {}
    fn error(&self, _: &str) {}
    fn print_seed(&self, _: &str, _: i64) {}
    fn translate(&self, key: &str) -> String {
        key.to_owned()
    }
    fn version(&self) -> MCVersion {
        MCVersion::latest()
    }
    fn debug_enabled(&self) -> bool {
        false
    }
    fn world_seed_found(&self, _: i64) {}
    fn biome_source_factory(&self) -> Option<BiomeSourceFactory> {
        None
    }
}

/// A world of the blocks put in it, air everywhere else, the same biome everywhere.
struct MockLevel {
    dimension: Dimension,
    biome: &'static str,
    blocks: Mutex<HashMap<(i32, i32, i32), &'static str>>,
    properties: Mutex<HashMap<(i32, i32, i32, &'static str), &'static str>>,
    unloaded: Mutex<HashSet<(i32, i32)>>,
    reads: AtomicUsize,
    cancel_after: AtomicUsize,
}

impl MockLevel {
    fn new(dimension: Dimension, biome: &'static str) -> MockLevel {
        MockLevel { dimension, biome, blocks: Mutex::default(), properties: Mutex::default(), unloaded: Mutex::default(), reads: AtomicUsize::new(0), cancel_after: AtomicUsize::new(usize::MAX) }
    }

    fn set(&self, x: i32, y: i32, z: i32, block: &'static str) {
        self.blocks.lock().unwrap().insert((x, y, z), block);
    }

    fn set_property(&self, x: i32, y: i32, z: i32, name: &'static str, value: &'static str) {
        self.properties.lock().unwrap().insert((x, y, z, name), value);
    }
}

impl Level for MockLevel {
    fn cancelled(&self) -> bool {
        self.reads.load(Ordering::Acquire) >= self.cancel_after.load(Ordering::Acquire)
    }
    fn dimension(&self) -> Dimension {
        self.dimension
    }
    fn block(&self, x: i32, y: i32, z: i32) -> &'static str {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if !self.is_chunk_loaded(x >> 4, z >> 4) { return "air"; }
        self.blocks.lock().unwrap().get(&(x, y, z)).copied().unwrap_or("air")
    }
    fn property(&self, x: i32, y: i32, z: i32, name: &str) -> Option<&'static str> {
        self.properties.lock().unwrap().iter().find(|((px, py, pz, n), _)| (*px, *py, *pz) == (x, y, z) && *n == name).map(|(_, v)| *v)
    }
    fn has_block_entity(&self, x: i32, y: i32, z: i32, _kind: &str) -> bool {
        matches!(self.block(x, y, z), "chest" | "spawner")
    }
    fn block_in_tag(&self, _: i32, _: i32, _: i32, _: &str) -> bool {
        false
    }
    fn noise_biome(&self, _: i32, _: i32, _: i32) -> &'static str {
        self.biome
    }
    fn biome(&self, _: i32, _: i32, _: i32) -> &'static str {
        self.biome
    }
    fn world_surface_height(&self, _: i32, _: i32) -> i32 {
        64
    }
    fn is_chunk_loaded(&self, x: i32, z: i32) -> bool {
        !self.unloaded.lock().unwrap().contains(&(x, z))
    }
}

fn context() -> FinderContext {
    let config = Config::default();
    let features = Features::init(MCVersion::latest(), &config, &|_| {});
    FinderContext {
        features: Arc::new(features),
        search: Arc::new(SearchPositions::reload_height(-64, 320)),
        storage: DataStorage::new(Arc::new(NoHost)),
        anti_xray_bypass: true,
    }
}

fn stored(ctx: &FinderContext) -> Vec<(i32, i32, &'static str)> {
    ctx.storage.base_seed_data.lock().unwrap().dump();
    ctx.storage.base_data().iter().map(|d| (d.chunk_x, d.chunk_z, d.feature.name())).collect()
}

/// Puts a piece's blocks in the world with its centre at `center`.
fn place(level: &MockLevel, piece: &PieceFinder, center: BlockPos) {
    for (key, block) in piece.blocks() {
        let p = center.offset_pos(*key);
        level.set(p.x, p.y, p.z, block);
    }
}

fn piece_of(kind: PieceKind, d: Direction, size: [i32; 3], build: fn(&mut PieceFinder)) -> PieceFinder {
    let mut p = PieceFinder::new(kind, Some(d), size);
    build(&mut p);
    p
}

#[test]
fn a_buried_treasure_is_a_chest_in_stone_at_the_middle_of_a_beach_chunk() {
    let ctx = context();
    let level = MockLevel::new(Dimension::Overworld, "beach");
    let chunk = ChunkPos::new(3, -2);
    level.set(chunk.x * 16 + 9, 40, chunk.z * 16 + 9, "chest");
    level.set(chunk.x * 16 + 9, 39, chunk.z * 16 + 9, "sandstone");
    let mut finders = BuriedTreasureFinder::create(&ctx, &level, chunk);
    let found = finders[0].find_in_chunk(&ctx, &level);
    assert_eq!(found, vec![BlockPos::new(chunk.x * 16 + 9, 40, chunk.z * 16 + 9)]);
    assert_eq!(stored(&ctx), vec![(3, -2, "buried_treasure")]);
    assert!(!finders[0].is_useless());

    // A chest in the water, on a block that is not stone, or in a chunk of another biome, is not one.
    let ctx = context();
    level.set_property(chunk.x * 16 + 9, 40, chunk.z * 16 + 9, "waterlogged", "true");
    assert!(BuriedTreasureFinder::create(&ctx, &level, chunk)[0].find_in_chunk(&ctx, &level).is_empty());
    let desert = MockLevel::new(Dimension::Overworld, "desert");
    desert.set(chunk.x * 16 + 9, 40, chunk.z * 16 + 9, "chest");
    desert.set(chunk.x * 16 + 9, 39, chunk.z * 16 + 9, "sandstone");
    assert!(BuriedTreasureFinder::create(&ctx, &desert, chunk)[0].find_in_chunk(&ctx, &desert).is_empty());
    assert!(stored(&ctx).is_empty());
}

#[test]
fn a_desert_well_is_found_from_the_chunk_beside_it() {
    let ctx = context();
    // Config::default has the desert well off, but a finder asked by hand does not look at that.
    let level = MockLevel::new(Dimension::Overworld, "desert");
    let mut well = PieceFinder::new(PieceKind::Piece, Some(Direction::North), [5, 6, 5]);
    well.fill_with_outline(0, 0, 0, 4, 1, 4, "sandstone", "sandstone", false);
    well.fill_with_outline(1, 5, 1, 3, 5, 3, "sandstone_slab", "sandstone_slab", false);
    well.add_block("sandstone", 2, 5, 2);
    well.add_block("water", 2, 1, 2);
    for d in Direction::HORIZONTAL {
        let p = BlockPos::new(2, 1, 2).relative(d, 1);
        well.add_block("water", p.x, p.y, p.z);
    }
    let corner = BlockPos::new(5 * 16 + 14, 63, -3 * 16 + 4);
    place(&level, &well, corner);
    // Seen from the chunk at (5, -3) the corner is in it; the finder of that chunk finds it at the chunk's local position.
    let ctx_features = ctx.features.desert_well.is_some();
    assert!(ctx_features);
    let finders = DesertWellFinder::create(&ctx, &level, ChunkPos::new(5, -3));
    assert_eq!(finders.len(), 9);
    let mut found = 0;
    for mut f in finders {
        found += f.find_in_chunk(&ctx, &level).len();
    }
    assert!(found >= 1);
    let data = stored(&ctx);
    assert_eq!(data.len(), 1, "{data:?}");
    assert_eq!(data[0].2, "desert_well");
}

#[test]
fn an_end_gateway_is_read_with_its_height() {
    let ctx = context();
    let level = MockLevel::new(Dimension::End, "end_highlands");
    let chunk = ChunkPos::new(10, 4);
    let (x, z) = (chunk.x * 16 + 6, chunk.z * 16 + 3);
    level.set(x, 75, z, "end_gateway");
    // End stone four blocks below the gateway, bedrock between.
    level.set(x, 71, z, "end_stone");
    for y in 72..75 {
        level.set(x, y, z, "bedrock");
    }
    let mut f = EndGatewayFinder::create(&ctx, &level, chunk);
    assert_eq!(f[0].find_in_chunk(&ctx, &level).len(), 1);
    ctx.storage.base_seed_data.lock().unwrap().dump();
    let base = ctx.storage.base_data();
    assert_eq!(base.len(), 1);
    let Detail::Decorator(crate::sunset_scx_feature::DecoratorDetail::EndGateway { height, offset_x, offset_z, .. }) = &base[0].detail else { panic!() };
    // The Java counts the blocks down to the end stone, the end stone itself included, and takes one off: 3.
    assert_eq!((*height, *offset_x, *offset_z), (3, 6, 3));
    let _ = biomes::END_HIGHLANDS;
}

#[test]
fn every_temple_is_found_whichever_way_it_faces() {
    for (name, size, build, finder) in [
        ("desert_pyramid", [21, 15, 21], dsl::desert_pyramid as fn(&mut PieceFinder), TempleFinder::create_desert_pyramid as fn(&FinderContext, &dyn Level, ChunkPos) -> Vec<Box<dyn Finder>>),
        ("jungle_pyramid", [12, 10, 15], dsl::jungle_pyramid, TempleFinder::create_jungle_pyramid),
        ("swamp_hut", [7, 7, 9], dsl::swamp_hut, TempleFinder::create_swamp_hut),
    ] {
        let biome = match name {
            "desert_pyramid" => "desert",
            "jungle_pyramid" => "jungle",
            _ => "swamp",
        };
        for d in Direction::HORIZONTAL {
            let ctx = context();
            let level = MockLevel::new(Dimension::Overworld, biome);
            let chunk = ChunkPos::new(-7, 12);
            let center = BlockPos::new(chunk.x * 16, 70, chunk.z * 16);
            place(&level, &piece_of(PieceKind::Piece, d, size, build), center);
            let mut found = Vec::new();
            for mut f in finder(&ctx, &level, chunk) {
                found.extend(f.find_in_chunk(&ctx, &level));
            }
            assert!(found.contains(&center), "{name} facing {d:?}: {found:?}");
            let data = stored(&ctx);
            assert!(data.contains(&(-7, 12, name)), "{name} facing {d:?}: {data:?}");
        }
    }
}

#[test]
fn an_igloo_and_a_trial_chamber_are_found_in_each_orientation() {
    for d in Direction::HORIZONTAL {
        let ctx = context();
        let level = MockLevel::new(Dimension::Overworld, "snowy_plains");
        let chunk = ChunkPos::new(4, 4);
        let piece = piece_of(PieceKind::Jigsaw, d, [7, 5, 8], dsl::igloo);
        // A jigsaw piece is found where its search position puts it: a chunk corner moved by the rotation point.
        let search = ctx.search.igloo[&d].first().copied().unwrap();
        let center = BlockPos::new(chunk.x * 16 + search.x, 60, chunk.z * 16 + search.z);
        place(&level, &piece, center);
        let mut found = Vec::new();
        for mut f in IglooFinder::create(&ctx, &level, chunk) {
            found.extend(f.find_in_chunk(&ctx, &level));
        }
        assert!(found.contains(&center), "igloo {d:?}: {found:?}");
        assert!(stored(&ctx).contains(&(4, 4, "igloo")));
    }
    let ctx = context();
    let level = MockLevel::new(Dimension::Overworld, "plains");
    let chunk = ChunkPos::new(-3, -3);
    let piece = piece_of(PieceKind::Jigsaw, Direction::North, [19, 20, 19], dsl::trial_chambers_end1);
    place(&level, &piece, BlockPos::new(chunk.x * 16, 0, chunk.z * 16));
    let mut found = Vec::new();
    for mut f in TrialChambersFinder::create(&ctx, &level, chunk) {
        found.extend(f.find_in_chunk(&ctx, &level));
    }
    assert!(!found.is_empty());
    assert!(stored(&ctx).contains(&(-3, -3, "trial_chambers")));
}

#[test]
fn a_shipwreck_is_not_a_chest_in_the_open() {
    let ctx = context();
    let level = MockLevel::new(Dimension::Overworld, "ocean");
    let chunk = ChunkPos::new(1, 1);
    level.set(20, 50, 20, "chest");
    level.set_property(20, 50, 20, "type", "single");
    level.set_property(20, 50, 20, "facing", "north");
    let mut found = 0;
    for mut f in ShipwreckFinder::create(&ctx, &level, chunk) {
        found += f.find_in_chunk(&ctx, &level).len();
    }
    // A chest with no wreck around it: no stairs, no trapdoors, so no known layout and nothing stored.
    assert_eq!(found, 0);
    assert!(stored(&ctx).is_empty());
    let _ = Detail::Region;
    let _ = Feature::Structure;
}

#[test]
fn messages_are_the_language_files() {
    use crate::sunset_scx_lang::{format, translate};
    assert_eq!(translate("en_us", "tmachine.lookingForPillarSeed"), "Looking for pillar seeds...");
    // A key a language lacks falls back to English, and a key nobody has is itself.
    assert_eq!(translate("zh_cn", "no.such.key"), "no.such.key");
    assert_ne!(translate("ru_ru", "settings.active"), "");
    assert_eq!(format(&translate("en_us", "data.collectedBits"), &["12".into(), "32".into()]), "You currently have collected 12 bits out of 32.");
}

#[test]
fn the_database_sheet_is_read_by_address_then_by_hash() {
    use crate::sunset_scx_database::{post_body, Database};
    let mut db = Database::default();
    db.parse_csv("ip,dim,seed,version,user,hash,hashed\nplay.example.net/1.2.3.4:25565,overworld,123456789L,1.21,x,0,-777\nshort,row\nbad,overworld,xL,1.21,x,0,5\nother,overworld,-42L,1.21,x,0,31337\n");
    assert_eq!(db.len(), 2);
    assert_eq!(db.get_seed("play.example.net/1.2.3.4:25565", 0), Some(123456789));
    assert_eq!(db.get_seed("elsewhere", 31337), Some(-42));
    assert_eq!(db.get_seed("elsewhere", 1), None);
    assert_eq!(post_body("a b/1:2", "overworld", -5, "1.21.3", "Steve", true), "serverIp=a+b%2F1%3A2&dimension=overworld&seed=-5L&version=1.21.3&username=Steve&hash=1");
}

/// A development helper: the blocks of a pyramid the game generated (HORIZON_PYRAMID_BLOCKS, from horizon-ctl blocks) against
/// what the finder expects for each facing at the given origin.
#[test]
#[ignore]
fn pyramid_of_the_game_against_the_finder() {
    let Ok(path) = std::env::var("HORIZON_PYRAMID_BLOCKS") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    let mut world: HashMap<(i32, i32, i32), String> = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() == 4 {
            world.insert((f[0].parse().unwrap(), f[1].parse().unwrap(), f[2].parse().unwrap()), f[3].to_owned());
        }
    }
    let origin = (-2464, 70, 1664);
    for d in Direction::HORIZONTAL {
        let piece = piece_of(PieceKind::Piece, d, [21, 15, 21], dsl::desert_pyramid);
        let mut bad = 0;
        let mut shown = Vec::new();
        for (key, block) in piece.blocks() {
            let p = (origin.0 + key.x, origin.1 + key.y, origin.2 + key.z);
            let actual = world.get(&p).map(String::as_str).unwrap_or("air");
            if actual != *block {
                bad += 1;
                if shown.len() < 12 {
                    shown.push(format!("{key:?} expects {block} has {actual}"));
                }
            }
        }
        println!("{d:?}: {} blocks expected, {bad} differ {shown:#?}", piece.blocks().len());
    }
}

#[test]
fn a_plant_grown_over_an_air_cell_of_a_pyramid_does_not_hide_it() {
    // Measured on a generated world: dry grass stood where the structure holds air and the exact compare missed the pyramid.
    let ctx = context();
    let level = MockLevel::new(Dimension::Overworld, "desert");
    let chunk = ChunkPos::new(-7, 12);
    let center = BlockPos::new(chunk.x * 16, 70, chunk.z * 16);
    let piece = piece_of(PieceKind::Piece, Direction::West, [21, 15, 21], dsl::desert_pyramid);
    place(&level, &piece, center);
    let air = piece.blocks().iter().find(|(_, b)| *b == "air").map(|(k, _)| center.offset_pos(*k)).unwrap();
    level.set(air.x, air.y, air.z, "short_dry_grass");
    let found: Vec<BlockPos> = TempleFinder::create_desert_pyramid(&ctx, &level, chunk).into_iter().flat_map(|mut f| f.find_in_chunk(&ctx, &level)).collect();
    assert!(found.contains(&center));
    level.set(air.x, air.y, air.z, "stone");
    let found: Vec<BlockPos> = TempleFinder::create_desert_pyramid(&ctx, &level, chunk).into_iter().flat_map(|mut f| f.find_in_chunk(&ctx, &level)).collect();
    assert!(!found.contains(&center));
}

#[test]
fn unloaded_neighbor_air_is_not_a_template_match_and_rescan_finds_it() {
    let level = MockLevel::new(Dimension::Overworld, "desert");
    let mut piece = PieceFinder::new(PieceKind::Piece, None, [2, 1, 1]);
    piece.add_block("sandstone", 0, 0, 0);
    piece.add_block("air", 1, 0, 0);
    piece.search_positions = Arc::new(vec![BlockPos::new(15, 64, 0)]);
    level.set(15, 64, 0, "sandstone");
    level.unloaded.lock().unwrap().insert((1, 0));
    assert!(piece.find_in_chunk(&level, ChunkPos::new(0, 0)).is_empty());
    level.unloaded.lock().unwrap().clear();
    assert_eq!(piece.find_in_chunk(&level, ChunkPos::new(0, 0)), vec![BlockPos::new(15, 64, 0)]);
    level.set(16, 64, 0, "short_dry_grass");
    assert_eq!(piece.find_in_chunk(&level, ChunkPos::new(0, 0)), vec![BlockPos::new(15, 64, 0)]);
    level.set(16, 64, 0, "stone");
    assert!(piece.find_in_chunk(&level, ChunkPos::new(0, 0)).is_empty());
}

#[test]
fn template_and_block_finder_loops_stop_when_the_level_is_cancelled() {
    let level = MockLevel::new(Dimension::Overworld, "beach");
    let mut piece = PieceFinder::new(PieceKind::Piece, None, [1, 1, 1]);
    piece.add_block("air", 0, 0, 0);
    piece.search_positions = Arc::new((0..10000).map(|y| BlockPos::new(0, y, 0)).collect());
    level.cancel_after.store(32, Ordering::Release);
    assert!(piece.find_in_chunk(&level, ChunkPos::new(0, 0)).is_empty());
    assert_eq!(level.reads.load(Ordering::Relaxed), 32);

    let ctx = context();
    level.reads.store(0, Ordering::Release);
    let mut finder = BuriedTreasureFinder::create(&ctx, &level, ChunkPos::new(0, 0)).remove(0);
    assert!(finder.find_in_chunk(&ctx, &level).is_empty());
    assert_eq!(level.reads.load(Ordering::Relaxed), 32);
    assert!(stored(&ctx).is_empty());
}

#[test]
fn database_hash_lookup_ignores_address_priority_and_keeps_zero_values() {
    use crate::sunset_scx_database::Database;
    let mut database = Database::default();
    database.parse_csv("ip,dim,seed,version,user,hash,hashed\nsame,overworld,123L,1.21,x,0,456\nother,overworld,0L,1.21,x,0,0\nnegative,overworld,-42L,1.21,x,0,-789\n");
    assert_eq!(database.get_seed("same", 0), Some(123));
    assert_eq!(database.get_seed_by_hash(0), Some(0));
    assert_eq!(database.get_seed_by_hash(-789), Some(-42));
    assert_eq!(database.get_seed_by_hash(111), None);
}

}

mod java_diff {
// The port against the Java library itself. `tests/data/java_fixture.txt` is what the SeedFinding libraries (mc_core, mc_seed,
// mc_feature at the commits SeedCrackerX 2.16.2 pins) answered for the same questions, written by tools/Gen.java run on a
// JDK; every line is asked of the Rust port and must come out the same.
use crate::sunset_scx_feature::{Decorator, DecoratorClass, Feature, FeatureData, Structure, StructureClass};
use crate::sunset_scx_rand::{ChunkRand, JRand};
use crate::sunset_scx_seeds::{pillar_seed, structure_seed, world_seed};
use crate::sunset_scx_version::MCVersion;
use std::sync::Arc;

const FIXTURE: &str = include_str!("seedcracker_java_fixture.txt");

fn class_of(name: &str) -> StructureClass {
    use StructureClass::*;
    match name {
        "buried_treasure" => BuriedTreasure,
        "desert_pyramid" => DesertPyramid,
        "end_city" => EndCity,
        "igloo" => Igloo,
        "jungle_pyramid" => JunglePyramid,
        "monument" => Monument,
        "pillager_outpost" => PillagerOutpost,
        "shipwreck" => Shipwreck,
        "swamp_hut" => SwampHut,
        "village" => Village,
        "trial_chambers" => TrialChambers,
        other => panic!("{other}"),
    }
}

fn version(name: &str) -> MCVersion {
    MCVersion::from_string(name).unwrap_or_else(|| panic!("{name}"))
}

fn list(text: &str) -> Vec<i64> {
    let inner = text.trim_start_matches('[').trim_end_matches(']');
    if inner.is_empty() { Vec::new() } else { inner.split(", ").map(|s| s.parse().unwrap()).collect() }
}

#[test]
fn every_line_of_the_java_fixture_is_answered_the_same() {
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut features: std::collections::HashMap<(String, MCVersion), Option<Arc<Feature>>> = std::collections::HashMap::new();
    let mut rand = ChunkRand::new();
    for line in FIXTURE.lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        let kind = parts[0];
        *counts.entry(kind).or_default() += 1;
        match kind {
            "CFG" => {
                let (name, v) = (parts[1], version(parts[2]));
                let s = Structure::new(class_of(name), v);
                if parts[3] == "none" {
                    // Buried treasure is the one the Java builds with no config before 1.13; a port that has none is the same.
                    assert!(s.is_none(), "{line}");
                } else {
                    let s = s.as_ref().unwrap_or_else(|| panic!("{line}"));
                    assert_eq!((s.spacing(), s.separation(), s.salt()), (parts[3].parse().unwrap(), parts[4].parse().unwrap(), parts[5].parse().unwrap()), "{line}");
                }
                features.insert((name.to_owned(), v), s.map(|s| Arc::new(Feature::Structure(s))));
            }
            "REG" => {
                let (name, v) = (parts[1], version(parts[2]));
                let feature = features[&(name.to_owned(), v)].as_ref().unwrap();
                let Feature::Structure(s) = &**feature else { unreachable!() };
                let got = s.get_in_region(parts[3].parse().unwrap(), parts[4].parse().unwrap(), parts[5].parse().unwrap(), &mut rand);
                let got = got.map_or("none".to_owned(), |p| format!("{},{}", p.x, p.z));
                assert_eq!(got, parts[6], "{line}");
            }
            "START" => {
                let (name, v) = (parts[1], version(parts[2]));
                let feature = features[&(name.to_owned(), v)].as_ref().unwrap();
                let seed: i64 = parts[3].parse().unwrap();
                let data = FeatureData::structure(feature, parts[4].parse().unwrap(), parts[5].parse().unwrap());
                assert_eq!(data.test_start(seed, &mut rand).to_string(), parts[6], "{line}");
                assert_eq!(data.test_start(seed ^ 0x10001, &mut rand).to_string(), parts[7], "{line}");
            }
            "WELL" | "GATE" => {
                if parts[5] == "none" {
                    continue;
                }
                let v = version(parts[1]);
                let class = if kind == "WELL" { DecoratorClass::DesertWell } else { DecoratorClass::EndGateway };
                let feature = Arc::new(Feature::Decorator(Decorator::new(class, v).unwrap()));
                let seed: i64 = parts[2].parse().unwrap();
                let (bx, bz): (i32, i32) = (parts[5].parse().unwrap(), parts[6].parse().unwrap());
                if kind == "WELL" {
                    assert_eq!(FeatureData::desert_well(&feature, bx, bz).test_start(seed, &mut rand).to_string(), parts[7], "{line}");
                    assert_eq!(FeatureData::desert_well(&feature, bx + 1, bz).test_start(seed, &mut rand).to_string(), parts[8], "{line}");
                } else {
                    let height: i32 = parts[7].parse().unwrap();
                    assert_eq!(FeatureData::end_gateway(&feature, bx, bz, height).test_start(seed, &mut rand).to_string(), parts[8], "{line}");
                    assert_eq!(FeatureData::end_gateway(&feature, bx, bz, height + 1).test_start(seed, &mut rand).to_string(), parts[9], "{line}");
                }
            }
            "SEEDING" => {
                let v = version(parts[1]);
                let w: i64 = parts[2].parse().unwrap();
                let (cx, cz): (i32, i32) = (parts[3].parse().unwrap(), parts[4].parse().unwrap());
                let mut cr = ChunkRand::new();
                let mut got: Vec<String> = Vec::new();
                let mut push = |seed: i64, cr: &mut ChunkRand| got.push(format!("{seed}/{}", cr.next_int()));
                let s = cr.set_terrain_seed(cx, cz, v);
                push(s, &mut cr);
                let s = cr.set_population_seed(w, cx << 4, cz << 4, v);
                push(s, &mut cr);
                if v.is_newer_or_equal_to(MCVersion::v1_13) {
                    let s = cr.set_decorator_seed_at_step(w, cx << 4, cz << 4, 3, 2, v);
                    push(s, &mut cr);
                    let s = cr.set_decorator_seed_at_salt(w, cx << 4, cz << 4, 20003, v);
                    push(s, &mut cr);
                }
                let s = cr.set_carver_seed(w, cx, cz, v);
                push(s, &mut cr);
                let s = cr.set_region_seed(w, cx, cz, 14357617, v);
                push(s, &mut cr);
                let s = cr.set_weak_seed(w, cx, cz, v);
                push(s, &mut cr);
                let s = cr.set_slime_seed(w, cx, cz, 987234911, v);
                push(s, &mut cr);
                let s = cr.set_position_seed(cx.wrapping_mul(7), 64, cz.wrapping_mul(3), v);
                push(s, &mut cr);
                let s = cr.set_base_stone_seed(w, cx, 5, cz);
                push(s, &mut cr);
                assert_eq!(got.join(" "), parts[5..].join(" "), "{line}");
            }
            "SEED" => {
                let s: i64 = parts[1].parse().unwrap();
                let structure = s & 0xFFFF_FFFF_FFFF;
                let field = |key: &str| parts.iter().find_map(|p| p.strip_prefix(key)).unwrap_or_else(|| panic!("{key} in {line}"));
                assert_eq!(pillar_seed::from_structure_seed(structure).to_string(), field("pillar="), "{line}");
                assert_eq!(world_seed::to_hash(s).to_string(), field("hash="), "{line}");
                assert_eq!(world_seed::get_shadow_seed(s).to_string(), field("shadow="), "{line}");
                assert_eq!(world_seed::is_random(s).to_string(), field("random="), "{line}");
                assert_eq!(world_seed::is_string(s).to_string(), field("string="), "{line}");
                // randomWorlds=[a, b] and heights=[...] hold spaces, so take them from the whole line.
                let rw = line.split("randomWorlds=").nth(1).unwrap().split(" heights=").next().unwrap();
                assert_eq!(structure_seed::to_random_world_seeds(structure), list(rw), "{line}");
                let heights = line.split("heights=").nth(1).unwrap();
                let got = pillar_seed::get_pillar_heights(structure & 0xFFFF);
                assert_eq!(format!("{got:?}"), heights, "{line}");
            }
            "RAND" => {
                let w: i64 = parts[1].parse().unwrap();
                assert_eq!(world_seed::is_random(w).to_string(), parts[2], "{line}");
                let rest = line.splitn(4, ' ').nth(3).unwrap();
                let (worlds, hash) = rest.rsplit_once(' ').unwrap();
                assert_eq!(structure_seed::to_random_world_seeds(w & 0xFFFF_FFFF_FFFF), list(worlds), "{line}");
                assert_eq!(world_seed::to_hash(w).to_string(), hash, "{line}");
            }
            "NEXTLONG" => {
                let internal: i64 = parts[1].parse().unwrap();
                let w: i64 = parts[2].parse().unwrap();
                assert_eq!(JRand::of_internal_seed(internal).next_long(), w, "{line}");
                let rest = line.splitn(4, ' ').nth(3).unwrap();
                let (worlds, random) = rest.rsplit_once(' ').unwrap();
                assert_eq!(structure_seed::to_random_world_seeds(w & 0xFFFF_FFFF_FFFF), list(worlds), "{line}");
                assert_eq!(world_seed::is_random(w).to_string(), random, "{line}");
            }
            "JRAND" => {
                let s: i64 = parts[1].parse().unwrap();
                let mut j = JRand::new(s);
                let got = format!(
                    "{} {} {} {} {} {} {} {} {}",
                    j.next_int(),
                    j.next_int_bound(24),
                    j.next_long(),
                    j.next_double(),
                    j.next_float(),
                    j.next_boolean(),
                    j.next_int_bound(16),
                    j.next_int_bound(7),
                    j.next_gaussian()
                );
                let expected = parts[2..].join(" ");
                // Java prints floats and doubles with its own shortest form; compare them as numbers.
                let (g, e): (Vec<&str>, Vec<&str>) = (got.split(' ').collect(), expected.split(' ').collect());
                for (g, e) in g.iter().zip(&e) {
                    match (g.parse::<f64>(), e.parse::<f64>()) {
                        (Ok(a), Ok(b)) if g.contains('.') || e.contains('.') => assert!((a - b).abs() <= 1e-12 * b.abs().max(1.0), "{line}: {g} vs {e}"),
                        _ => assert_eq!(g, e, "{line}"),
                    }
                }
            }
            other => panic!("unknown fixture line {other}"),
        }
    }
    // Every kind was in the file, and asked.
    for kind in ["CFG", "REG", "START", "WELL", "GATE", "SEEDING", "SEED", "RAND", "NEXTLONG", "JRAND"] {
        assert!(counts.get(kind).copied().unwrap_or(0) > 0, "no {kind} lines");
    }
}
}
