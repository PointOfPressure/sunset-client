// Ore Sim's SeedCracker controller. Each run owns its Host, storage and immutable world views; only a
// current, hash-verified result (or an authoritative local seed) can leave this module.

use super::Game;
use super::sunset::Module;
use super::sunset_scx_config::{Config, RenderType};
use super::sunset_scx_cracker_data::{BiomeSourceFactory, DataAddedEvent, HashedSeedData, Host, SearchProgress, SearchStage};
use super::sunset_scx_cracker_storage::DataStorage;
use super::sunset_scx_cracker_time_machine::Phase;
use super::sunset_scx_database::{DATABASE_URL, Database};
use super::sunset_scx_dimension::Dimension;
use super::sunset_scx_features::Features;
use super::sunset_scx_finder::{FinderContext, SearchPositions};
use super::sunset_scx_finders::BUILDERS;
use super::sunset_scx_lang;
use super::sunset_scx_level::ChunkPos;
use super::sunset_scx_seeds::world_seed;
use super::sunset_scx_version::MCVersion;
use super::sunset_seedcracker_world::{At, ChunkSnapshot, MAX_CHUNKS, StateTable, WorldView};
use horizon_client::chunk::Chunk;
use horizon_client::net::sunset_packets::{Link, Linked, scan_configuring};
use horizon_protocol::connection::sunset_lifecycle::RUNTIME;
use horizon_protocol::packets::play::Clientbound;
use horizon_world::registry::BlockRegistry;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, RwLock, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_OBSERVATIONS: usize = 4096;
const MAX_DATABASE: u64 = 2 * 1024 * 1024;
const QUEUE_SIZE: usize = 16;
const SEND_PER_TICK: usize = 2;
const MAX_VERIFIED_SEEDS: usize = 64;

// Diagnostic only: exercise packet/chunk discovery even in an integrated world. Read once per process;
// no save/profile setting, local seed read or cache reuse can bypass that experiment.
static FORCE_CRACK: LazyLock<bool> = LazyLock::new(|| std::env::var_os("SUNSET_SEEDCRACKER_FORCE_CRACK").is_some_and(|v| v == "1"));
// An optional collection window makes live diagnostic runs practical without changing the solver or
// seeding it with fixtures. Ignored outside FORCE_CRACK; normal collection/search timing is unchanged.
static START_DELAY: LazyLock<Duration> = LazyLock::new(|| Duration::from_secs(if *FORCE_CRACK {
    std::env::var("SUNSET_SEEDCRACKER_START_DELAY").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0).min(300)
} else { 0 }));

#[derive(Default)]
struct VerifiedSeeds(VecDeque<(i64, i64)>);

impl VerifiedSeeds {
    fn remember(&mut self, hash: Option<i64>, seed: i64) {
        if !verified(seed, hash) { return; }
        let hash = hash.unwrap();
        self.0.retain(|&(h, _)| h != hash);
        self.0.push_back((hash, seed));
        if self.0.len() > MAX_VERIFIED_SEEDS { self.0.pop_front(); }
    }

    fn get(&mut self, hash: Option<i64>) -> Option<i64> {
        let hash = hash?;
        let index = self.0.iter().position(|&(h, _)| h == hash)?;
        let entry = self.0.remove(index)?;
        self.0.push_back(entry);
        Some(entry.1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SeedOrigin { Local, Cracked, Database, Reused }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FoundSeed {
    pub world: u64,
    pub run: u64,
    pub value: i64,
    pub origin: SeedOrigin,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Snapshot {
    pub world: u64,
    pub run: u64,
    pub phase: String,
    pub observations: usize,
    pub base_bits: f64,
    pub lifting_bits: f64,
    pub wanted_bits: f64,
    pub structure_candidates: usize,
    pub world_candidates: usize,
    pub checked: u64,
    pub total: u64,
    pub found: Option<FoundSeed>,
    pub detail: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> { m.lock().unwrap_or_else(|e| e.into_inner()) }
fn bounded(text: &str) -> String { text.chars().filter(|c| !c.is_control()).take(192).collect() }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Policy { automatic: bool, database: bool, threads: i32 }

impl Policy {
    fn of(module: Option<&Module>) -> Self {
        let Some(m) = module else { return Self::default(); };
        let automatic = m.on && m.p("Seed source") == "Auto-crack" && m.t("Seed").trim().is_empty();
        Self { automatic, database: automatic && m.b("Database fetch"), threads: thread_limit(m.n("Cracker threads")) }
    }
}

fn thread_limit(requested: f64) -> i32 {
    let available = std::thread::available_parallelism().map_or(1, usize::from).saturating_sub(1).max(1).min(8);
    let requested = if requested.is_finite() { requested.round().clamp(1.0, 8.0) as usize } else { 2 };
    requested.min(available) as i32
}

struct Identity {
    link: Option<Link>,
    local: Option<Weak<horizon_client::save::SaveDir>>,
    local_epoch: Option<u64>,
    dimension: String,
    hash: Option<i64>,
    registry: usize,
    biomes: usize,
}

impl Identity {
    fn of(game: &Game<'_>) -> Option<Self> {
        if !RUNTIME.running() || !game.native_window() || !game.native_on(super::sunset::ID)
            || game.player.is_none() || game.screen == super::Screen::Dead {
            return None;
        }
        let biomes = game.world_biomes.as_deref().unwrap_or_else(|| game.res.biomes());
        let (link, local, local_epoch, dimension, hash) = match &game.remote {
            Some(r) => {
                if !r.logged_in || scan_configuring(&r.session) { return None; }
                (Some(Link::of(&r.session)), None, None, r.level.clone(), Some(r.hashed_seed))
            }
            None => {
                let level = game.native_level(game.dim.id())?;
                let seed = game.native_level_seed(level)?;
                (None, Some(Arc::downgrade(&game.save)), game.native_lifecycle.epoch().map(|e| e.get()),
                    game.dim.id().to_owned(), Some(world_seed::to_hash(seed)))
            }
        };
        Some(Self { link, local, local_epoch, dimension, hash,
            registry: game.pack.assets.registry as *const BlockRegistry as usize,
            biomes: biomes as *const horizon_client::biome::Biomes as usize })
    }

    fn same(&self, other: &Self, game: &Game<'_>) -> bool {
        let connection = match (&self.link, &other.link) {
            (Some(link), Some(_)) => link.to(game.remote.as_ref().map(|r| &r.session)) == Linked::Same,
            (None, None) => self.local.as_ref().zip(other.local.as_ref()).is_some_and(|(a, b)| Weak::ptr_eq(a, b))
                && self.local_epoch == other.local_epoch,
            _ => false,
        };
        connection && self.dimension == other.dimension && self.hash == other.hash
            && self.registry == other.registry && self.biomes == other.biomes
    }
}

struct GameHost {
    world: u64,
    run: u64,
    hash: Option<i64>,
    cancelled: Arc<AtomicBool>,
    lifecycle: u64,
    published: Mutex<Snapshot>,
    finished: AtomicBool,
    database_started: AtomicBool,
    cache: Arc<Mutex<VerifiedSeeds>>,
}

impl GameHost {
    fn new(world: u64, run: u64, hash: Option<i64>, cache: Arc<Mutex<VerifiedSeeds>>) -> Arc<Self> {
        Arc::new(Self { world, run, hash, cancelled: Arc::new(AtomicBool::new(false)), lifecycle: RUNTIME.token(),
            published: Mutex::new(Snapshot { world, run, phase: "Collecting".into(), wanted_bits: 32.0,
                detail: if hash.is_none() { Some("Waiting for the world's seed hash".into()) }
                    else if !START_DELAY.is_zero() { Some(format!("Diagnostic collection window: {} seconds before solving", START_DELAY.as_secs())) }
                    else { None }, ..Snapshot::default() }),
            finished: AtomicBool::new(false), database_started: AtomicBool::new(false), cache })
    }

    fn update(&self, update: impl FnOnce(&mut Snapshot)) {
        if self.cancelled() { return; }
        let mut snap = lock(&self.published);
        if !self.cancelled() { update(&mut snap); }
    }

    fn found(&self, value: i64, origin: SeedOrigin) {
        // A local in-frame authoritative seed has no remote hash requirement. Local save-file results and
        // every cracked/database result pass the current run's hash check as well.
        if origin != SeedOrigin::Local || self.hash.is_some() {
            if !verified(value, self.hash) { return; }
        }
        self.update(|s| {
            // Publish and remember under the same cancellation gate. Manual input never reaches this Host.
            lock(&self.cache).remember(self.hash, value);
            s.found = Some(FoundSeed { world: self.world, run: self.run, value, origin });
            s.phase = "Seed found".into();
            s.world_candidates = 1;
            s.detail = None;
        });
    }

    fn failed(&self, why: &str) {
        self.update(|s| { if s.found.is_none() { s.phase = "Waiting".into(); s.detail = Some(bounded(why)); } });
    }

    fn metrics(&self, storage: &DataStorage) {
        let observations = storage.observation_count();
        let base_bits = storage.get_base_bits();
        let lifting_bits = storage.get_lifting_bits();
        let wanted_bits = storage.get_wanted_bits();
        let tm = storage.get_time_machine();
        let structures = tm.structure_seed_count();
        let worlds = tm.world_seed_count();
        self.update(|s| {
            s.observations = observations; s.base_bits = base_bits; s.lifting_bits = lifting_bits;
            s.wanted_bits = wanted_bits; s.structure_candidates = structures;
            if s.found.is_none() { s.world_candidates = worlds; }
            if !tm.is_running() && s.found.is_none() {
                s.checked = 0; s.total = 0;
                s.phase = if observations >= MAX_OBSERVATIONS { "Observation limit" } else { "Collecting" }.into();
            }
        });
    }
}

impl Host for GameHost {
    fn cancelled(&self) -> bool { self.cancelled.load(Ordering::Acquire) || !RUNTIME.current(self.lifecycle) }
    fn debug(&self, _message: &str) {}
    fn debug_enabled(&self) -> bool { false }
    fn version(&self) -> MCVersion { MCVersion::latest() }
    fn translate(&self, key: &str) -> String { sunset_scx_lang::translate("en_us", key) }
    fn biome_source_factory(&self) -> Option<BiomeSourceFactory> { None }

    fn warn(&self, key: &str, args: &[String]) {
        let args: Vec<String> = args.iter().take(4).map(|s| bounded(s)).collect();
        let message = sunset_scx_lang::format(&self.translate(key), &args);
        self.update(|s| { if s.found.is_none() { s.detail = Some(bounded(&message)); } });
    }

    fn error(&self, key: &str) { self.failed(&self.translate(key)); }

    fn print_seed(&self, _key: &str, _seed: i64) {
        // Printed structure/decorator seeds are candidates, never a delivery event.
    }

    fn world_seed_found(&self, seed: i64) { self.found(seed, SeedOrigin::Cracked); }

    fn progress(&self, progress: SearchProgress) {
        self.update(|s| {
            if s.found.is_some() { return; }
            s.phase = match progress.stage {
                SearchStage::Pillars => "Pillars", SearchStage::LiftingLow => "Lifting low bits",
                SearchStage::LiftingHigh => "Lifting high bits", SearchStage::Structures => "Structures",
                SearchStage::Reduce => "Reducing candidates", SearchStage::Decorators => "Decorators",
                SearchStage::Hash => "Hash verification", SearchStage::Biomes => "Biomes",
            }.into();
            s.checked = progress.checked;
            s.total = progress.total;
        });
    }
}

fn verified(seed: i64, hash: Option<i64>) -> bool { hash.is_some_and(|h| world_seed::to_hash(seed) == h) }

#[derive(Clone, Copy, Debug)]
struct Scan { at: At, revision: u64 }

struct Seen { chunk: Weak<Chunk>, arrived: u64, dirty: u64, sent: u64 }

#[derive(Default)]
struct Scans {
    seen: HashMap<At, Seen>,
    membership: u64,
    refreshed: u64,
    revision: u64,
    center: At,
}

impl Scans {
    fn refresh(&mut self, game: &Game<'_>, latest: &RwLock<Option<Arc<ChunkSnapshot>>>, dimension: Dimension) {
        let membership = game.world.chunks.membership_revision();
        let Some(player) = game.player.as_ref() else { return; };
        let center = ((player.position.x.floor() as i32) >> 4, (player.position.z.floor() as i32) >> 4);
        if membership == self.membership && center == self.center && game.ticks.saturating_sub(self.refreshed) < 20 { return; }
        self.membership = membership;
        self.refreshed = game.ticks;
        self.center = center;
        let mut nearby: Vec<At> = game.world.chunks.keys().copied().collect();
        nearby.sort_unstable_by_key(|&(x, z)| ((i64::from(x) - i64::from(center.0)).abs().max((i64::from(z) - i64::from(center.1)).abs()), x, z));
        nearby.truncate(MAX_CHUNKS);
        let chunks: HashMap<At, Arc<Chunk>> = nearby.into_iter().filter_map(|at| game.world.chunks.get(&at).map(|c| (at, c.clone()))).collect();
        self.publish(chunks, game.ticks, latest, dimension);
    }

    fn publish(&mut self, chunks: HashMap<At, Arc<Chunk>>, tick: u64,
        latest: &RwLock<Option<Arc<ChunkSnapshot>>>, dimension: Dimension) {
        self.revision = self.revision.wrapping_add(1).max(1);
        let revision = self.revision;
        let mut changed: Vec<At> = self.seen.keys().filter(|k| !chunks.contains_key(k)).copied().collect();
        self.seen.retain(|at, _| chunks.contains_key(at));
        for (&at, chunk) in &chunks {
            match self.seen.get_mut(&at) {
                Some(seen) => {
                    if !std::ptr::eq(seen.chunk.as_ptr(), Arc::as_ptr(chunk)) {
                        if tick.saturating_sub(seen.arrived) <= 600 { changed.push(at); }
                        seen.chunk = Arc::downgrade(chunk);
                    }
                }
                None => {
                    self.seen.insert(at, Seen { chunk: Arc::downgrade(chunk), arrived: tick, dirty: revision, sent: 0 });
                    changed.push(at);
                }
            }
        }
        // Templates span more than the replay's 3x3 neighborhood. End pillars inspect the entire ring.
        for &other in &changed {
            for dx in -4..=4 {
                for dz in -4..=4 {
                    let (Some(x), Some(z)) = (other.0.checked_add(dx), other.1.checked_add(dz)) else { continue; };
                    if let Some(seen) = self.seen.get_mut(&(x, z)) { seen.dirty = revision; }
                }
            }
        }
        if dimension == Dimension::End && changed.iter().any(|&at| in_pillar_ring(at)) {
            for (&at, seen) in &mut self.seen {
                if in_pillar_ring(at) { seen.dirty = revision; }
            }
        }
        *latest.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(ChunkSnapshot { revision, chunks }));
    }

    fn send(&mut self, sender: &SyncSender<Scan>) {
        let mut pending: Vec<At> = self.seen.iter().filter(|(_, s)| s.sent != s.dirty).map(|(&at, _)| at).collect();
        pending.sort_unstable_by_key(|&(x, z)| ((i64::from(x) - i64::from(self.center.0)).abs().max((i64::from(z) - i64::from(self.center.1)).abs()), x, z));
        for at in pending.into_iter().take(SEND_PER_TICK) {
            let seen = self.seen.get_mut(&at).unwrap();
            match sender.try_send(Scan { at, revision: seen.dirty }) {
                Ok(()) => seen.sent = seen.dirty,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => break,
            }
        }
    }
}

fn in_pillar_ring((x, z): At) -> bool { (-4..=4).contains(&x) && (-4..=4).contains(&z) }

enum LocalSeed { Direct(i64), Saved(PathBuf) }

struct WorkerInput {
    registry: &'static BlockRegistry,
    biomes: Vec<String>,
    dimension: Dimension,
    height: (i32, i32),
    local: Option<LocalSeed>,
    policy: Policy,
}

impl WorkerInput {
    fn of(game: &Game<'_>, policy: Policy) -> Self {
        let biomes = game.world_biomes.as_deref().unwrap_or_else(|| game.res.biomes());
        let dimension = match game.dim { super::Dim::Nether => Dimension::Nether, super::Dim::End => Dimension::End, _ => Dimension::Overworld };
        let local = if *FORCE_CRACK { None } else { match &game.remote {
            None => game.native_level(game.dim.id()).and_then(|l| game.native_level_seed(l)).map(LocalSeed::Direct),
            Some(remote) if game.integrated.is_some() => remote.save_folder.as_ref().and_then(|folder| {
                // This is the integrated server's actual save folder, not a server address or seed guess.
                std::env::current_dir().ok().map(|p| LocalSeed::Saved(p.join("saves").join(folder)))
            }),
            _ => None,
        } };
        Self { registry: game.pack.assets.registry, biomes: biomes.list.iter().take(8192).map(|b| b.name.clone()).collect(),
            dimension, height: (game.world.shape.min_y, game.world.shape.max_y()), local, policy }
    }
}

struct Run {
    host: Arc<GameHost>,
    storage: Arc<DataStorage>,
    input: SyncSender<Scan>,
    latest: Arc<RwLock<Option<Arc<ChunkSnapshot>>>>,
    worker: JoinHandle<()>,
    scans: Scans,
    dimension: Dimension,
}

impl Run {
    fn start(game: &Game<'_>, world: u64, run: u64, hash: Option<i64>, policy: Policy,
        cache: Arc<Mutex<VerifiedSeeds>>) -> Result<Self, String> {
        let host = GameHost::new(world, run, hash, cache);
        let storage = DataStorage::new(host.clone());
        storage.get_time_machine().set_threads(policy.threads);
        let latest = Arc::new(RwLock::new(None));
        let (input, receiver) = sync_channel(QUEUE_SIZE);
        let init = WorkerInput::of(game, policy);
        let dimension = init.dimension;
        let (h, s, view) = (host.clone(), storage.clone(), latest.clone());
        let worker = std::thread::Builder::new().name("sunset-seedcracker".into()).spawn(move || {
            // Retained handles live outside the unwind boundary, so even a finder panic cancels and joins
            // the solver/HTTP workers before this coordinator acknowledges retirement.
            let mut solver = None;
            let mut database = None;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
                coordinate(&h, &s, &view, receiver, init, &mut solver, &mut database)));
            s.get_time_machine().terminate();
            if let Some(handle) = solver { let _ = handle.join(); }
            if let Some(handle) = database { let _ = handle.join(); }
            if result.is_err() { h.failed("Seed search failed; use .seed restart to retry"); }
            *view.write().unwrap_or_else(|e| e.into_inner()) = None;
            h.finished.store(true, Ordering::Release);
        }).map_err(|e| bounded(&format!("Could not start seed search: {e}")))?;
        Ok(Self { host, storage, input, latest, worker, scans: Scans::default(), dimension })
    }

    fn retire(self) -> JoinHandle<()> {
        self.host.cancelled.store(true, Ordering::Release);
        self.storage.get_time_machine().terminate();
        *self.latest.write().unwrap_or_else(|e| e.into_inner()) = None;
        // Dropping input wakes the coordinator. Only the coordinator joins running solver/HTTP work.
        self.worker
    }
}

fn fetch_database(host: &GameHost, policy: Policy, fetch: impl FnOnce() -> Result<String, String>) {
    if !policy.automatic || !policy.database || host.cancelled() || host.hash.is_none() { return; }
    if host.database_started.swap(true, Ordering::AcqRel) || host.cancelled() { return; }
    match fetch() {
        Ok(csv) if !host.cancelled() => {
            let mut database = Database::default();
            database.parse_csv(&csv);
            if let Some(seed) = host.hash.and_then(|h| database.get_seed_by_hash(h)) {
                if verified(seed, host.hash) { host.found(seed, SeedOrigin::Database); }
                else { host.failed("Database row did not match this world's seed hash"); }
            } else {
                host.update(|s| { if s.found.is_none() { s.detail = Some("No matching database seed; collecting structures".into()); } });
            }
        }
        Err(why) => host.failed(&why),
        _ => {},
    }
}

fn database_get() -> Result<String, String> {
    let response = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).redirects(5).build()
        .get(DATABASE_URL).set("User-Agent", "Sunset SeedCracker").call()
        .map_err(|_| "Seed database could not be reached".to_owned())?;
    let mut body = Vec::new();
    response.into_reader().take(MAX_DATABASE + 1).read_to_end(&mut body)
        .map_err(|_| "Seed database response could not be read".to_owned())?;
    if body.len() as u64 > MAX_DATABASE { return Err("Seed database response exceeded 2 MiB".into()); }
    String::from_utf8(body).map_err(|_| "Seed database returned invalid text".into())
}

fn coordinate(host: &Arc<GameHost>, storage: &Arc<DataStorage>, latest: &RwLock<Option<Arc<ChunkSnapshot>>>,
    input: Receiver<Scan>, init: WorkerInput, solver: &mut Option<JoinHandle<()>>, database: &mut Option<JoinHandle<()>>) {
    if host.cancelled() { return; }
    let local = match init.local {
        Some(LocalSeed::Direct(value)) => Some(value),
        Some(LocalSeed::Saved(path)) => horizon_client::save::Level::read(&path).map(|l| l.seed),
        None => None,
    };
    if let Some(value) = local {
        host.found(value, SeedOrigin::Local);
        if lock(&host.published).found.is_some() { return; }
    }
    if host.cancelled() { return; }
    if init.policy.database && host.hash.is_some() {
        let h = host.clone();
        let policy = init.policy;
        match std::thread::Builder::new().name("sunset-seed-database".into()).spawn(move || fetch_database(&h, policy, database_get)) {
            Ok(handle) => *database = Some(handle),
            Err(_) => host.failed("Could not start seed database request"),
        }
    }
    let mut config = Config::default();
    config.active = true;
    config.render = RenderType::Off;
    config.anti_xray_bypass = false;
    config.database_submits = false;
    let features = Arc::new(Features::init(host.version(), &config, &|_| {}));
    let search = Arc::new(SearchPositions::reload_height(init.height.0, init.height.1));
    let table = Arc::new(StateTable::build(init.registry, &init.biomes));
    let ctx = FinderContext { features, search, storage: storage.clone(), anti_xray_bypass: false };
    if let Some(hash) = host.hash {
        let poke: DataAddedEvent = Arc::new(|s| s.get_time_machine().poke(s, Some(Phase::Biomes)));
        storage.add_hashed_seed_data(HashedSeedData::new(hash), poke);
    }
    let collecting_since = Instant::now();
    loop {
        if host.cancelled() || lock(&host.published).found.is_some() { break; }
        if solver.as_ref().is_some_and(JoinHandle::is_finished) {
            if solver.take().unwrap().join().is_err() { host.failed("Seed solver failed; use .seed restart to retry"); return; }
        }
        if solver.is_none() && collecting_since.elapsed() >= *START_DELAY { *solver = storage.tick(); }
        let job = match input.recv_timeout(Duration::from_millis(50)) {
            Ok(job) => Some(job),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Some(job) = job {
            let chunks = latest.read().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(chunks) = chunks {
                if chunks.revision < job.revision || !chunks.chunks.contains_key(&job.at) { continue; }
                let level = WorldView::new(chunks, table.clone(), init.dimension, host.cancelled.clone(), host.lifecycle);
                for (kind, builder) in &BUILDERS {
                    if host.cancelled() || !kind.enabled(&config).get() { continue; }
                    if storage.observation_count() >= MAX_OBSERVATIONS { break; }
                    for mut finder in builder(&ctx, &level, ChunkPos::new(job.at.0, job.at.1)) {
                        if host.cancelled() || storage.observation_count() >= MAX_OBSERVATIONS { break; }
                        if finder.is_valid_dimension(init.dimension) { finder.find_in_chunk(&ctx, &level); }
                    }
                }
            }
        }
        host.metrics(storage);
    }
}

#[derive(Default)]
struct Controller {
    world: u64,
    run: u64,
    identity: Option<Identity>,
    policy: Option<Policy>,
    active: Option<Run>,
    retiring: Option<JoinHandle<()>>,
    idle: Snapshot,
    start_failed: bool,
    // Kept for the process lifetime across boundaries, stop, restart and Panic; never serialized.
    cache: Arc<Mutex<VerifiedSeeds>>,
}

impl Controller {
    fn retire(&mut self) {
        if let Some(run) = self.active.take() { self.retiring = Some(run.retire()); }
        self.idle = Snapshot { world: self.world, run: self.run, ..Snapshot::default() };
        self.start_failed = false;
    }

    fn boundary(&mut self) {
        self.retire();
        self.world = self.world.wrapping_add(1).max(1);
        self.identity = None;
        self.policy = None;
        self.idle.world = self.world;
    }

    fn reap(&mut self) {
        if self.retiring.as_ref().is_some_and(JoinHandle::is_finished) { let _ = self.retiring.take().unwrap().join(); }
    }

    fn read(&self) -> Snapshot {
        if let Some(run) = &self.active {
            if !run.host.cancelled() { return lock(&run.host.published).clone(); }
        }
        self.idle.clone()
    }

    fn reuse(&mut self, hash: Option<i64>) {
        if self.active.is_some() || self.idle.found.is_some() { return; }
        let Some(value) = lock(&self.cache).get(hash) else { return; };
        self.run = self.run.wrapping_add(1).max(1);
        self.idle = Snapshot { world: self.world, run: self.run, phase: "Seed reused".into(), world_candidates: 1,
            found: Some(FoundSeed { world: self.world, run: self.run, value, origin: SeedOrigin::Reused }),
            ..Snapshot::default() };
    }

    fn synchronize(&mut self, game: &Game<'_>, policy: Policy) {
        self.reap();
        let next = Identity::of(game);
        let same = match (&self.identity, &next) {
            (Some(old), Some(new)) => old.same(new, game), (None, None) => true, _ => false,
        };
        if !same { self.boundary(); self.identity = next; }
        if self.policy != Some(policy) {
            self.retire();
            self.policy = Some(policy);
        }
        if !policy.automatic || self.identity.is_none() || !RUNTIME.running() {
            if self.active.is_some() { self.retire(); }
            self.idle.phase = if policy.automatic { "Waiting for world" } else { "Off" }.into();
        } else if !*FORCE_CRACK {
            self.reuse(self.identity.as_ref().and_then(|i| i.hash));
        }
    }
}

static CONTROLLER: LazyLock<Mutex<Controller>> = LazyLock::new(|| Mutex::new(Controller::default()));

fn current_policy() -> Policy {
    let modules = super::sunset::snapshot();
    Policy::of(modules.iter().find(|m| m.name == super::sunset_ore_sim::NAME))
}

pub(super) fn maintain(game: &Game<'_>) {
    if !game.native_window() { return; }
    // Acquire runtime settings before the controller; HUD/status reads never take the runtime lock.
    let policy = current_policy();
    lock(&CONTROLLER).synchronize(game, policy);
}

pub(super) fn tick(game: &mut Game<'_>, module: Option<&Module>) {
    if !game.native_window() { return; }
    let policy = Policy::of(module);
    let mut c = lock(&CONTROLLER);
    c.synchronize(game, policy);
    if !policy.automatic || c.identity.is_none() || c.start_failed || c.idle.found.is_some() { return; }
    if c.active.is_none() {
        if c.retiring.is_some() { c.idle.phase = "Stopping previous search".into(); return; }
        c.run = c.run.wrapping_add(1).max(1);
        let hash = c.identity.as_ref().and_then(|i| i.hash);
        match Run::start(game, c.world, c.run, hash, policy, c.cache.clone()) {
            Ok(run) => c.active = Some(run),
            Err(why) => { c.start_failed = true; c.idle.phase = "Waiting".into(); c.idle.detail = Some(why); }
        }
    }
    if let Some(run) = &mut c.active {
        if run.host.cancelled() || run.host.finished.load(Ordering::Acquire) || lock(&run.host.published).found.is_some() { return; }
        run.scans.refresh(game, &run.latest, run.dimension);
        run.scans.send(&run.input);
    }
}

pub(super) fn world_id(game: &Game<'_>) -> u64 {
    if !game.native_window() { return 0; }
    maintain(game);
    lock(&CONTROLLER).world
}

pub(super) fn resolved(game: &Game<'_>) -> Option<FoundSeed> {
    if !game.native_window() { return None; }
    maintain(game);
    let c = lock(&CONTROLLER);
    if !c.policy.is_some_and(|p| p.automatic) || c.identity.is_none() { return None; }
    let snap = c.read();
    if snap.world != c.world || snap.run != c.run { return None; }
    let found = snap.found?;
    (found.world == c.world && found.run == c.run).then_some(found)
}

pub(super) fn snapshot() -> Snapshot {
    let mut c = lock(&CONTROLLER);
    if !RUNTIME.running() { c.retire(); c.policy = None; }
    c.read()
}

pub(super) fn status_line() -> Option<String> {
    let s = snapshot();
    if s.phase.is_empty() || s.phase == "Off" { return None; }
    if s.found.is_some() { return Some(s.phase); }
    if s.total > 0 { return Some(format!("{} {}%", s.phase, (s.checked.saturating_mul(100) / s.total).min(100))); }
    if s.world_candidates > 0 { return Some(format!("{} world candidates", s.world_candidates)); }
    if s.structure_candidates > 0 { return Some(format!("{} structure candidates", s.structure_candidates)); }
    if s.phase == "Collecting" { return Some(format!("Collecting {:.0}/40 bits", s.lifting_bits)); }
    Some(s.phase)
}

pub(super) fn incoming_before(packet: &Clientbound) -> bool {
    if !matches!(packet, Clientbound::Login(_) | Clientbound::Respawn(_) | Clientbound::Disconnect(_)) { return false; }
    lock(&CONTROLLER).boundary();
    true
}

pub(super) fn restart() { stop(); }

pub(super) fn stop() {
    let mut c = lock(&CONTROLLER);
    c.retire();
    c.policy = None;
    c.idle.phase = "Off".into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn new_host(world: u64, run: u64, hash: Option<i64>) -> Arc<GameHost> {
        GameHost::new(world, run, hash, Arc::default())
    }

    fn module() -> Module {
        let mut m = Module::new("Ore Sim", super::super::sunset::Category::World, "test")
            .text("Seed", "").pick("Seed source", &["Auto-crack", "Manual"])
            .bool("Database fetch", false).num("Cracker threads", 2.0, 1.0, 8.0, 1.0);
        m.on = true;
        m
    }

    #[test]
    fn cancelled_and_unverified_callbacks_cannot_deliver_a_seed_or_progress() {
        let host = new_host(1, 1, Some(world_seed::to_hash(-42)));
        host.print_seed("candidate", -42);
        assert!(lock(&host.published).found.is_none());
        host.world_seed_found(7);
        assert!(lock(&host.published).found.is_none());
        host.world_seed_found(-42);
        assert_eq!(lock(&host.published).found.unwrap().value, -42);
        let before = lock(&host.published).clone();
        host.cancelled.store(true, Ordering::Release);
        host.progress(SearchProgress { stage: SearchStage::Hash, checked: 9, total: 10 });
        host.world_seed_found(-42);
        assert_eq!(lock(&host.published).phase, before.phase);
        assert_eq!(lock(&host.published).checked, before.checked);
        assert!(!verified(0, None));
        assert!(verified(0, Some(world_seed::to_hash(0))));
    }

    #[test]
    fn manual_values_always_close_the_automatic_policy() {
        let m = module();
        assert!(Policy::of(Some(&m)).automatic);
        for seed in ["0", "-42", "\u{1f305}", " text "] {
            let mut m = module();
            let setting = m.settings.iter_mut().find(|s| s.name == "Seed").unwrap();
            setting.value = super::super::sunset::Value::Text(seed.into());
            assert!(!Policy::of(Some(&m)).automatic);
        }
        assert!(!Policy::of(None).automatic);
        let mut off = module(); off.on = false;
        assert!(!Policy::of(Some(&off)).database);
    }

    #[test]
    fn verified_cache_is_bounded_and_recently_reused_seeds_survive_eviction() {
        let mut cache = VerifiedSeeds::default();
        for seed in 0..MAX_VERIFIED_SEEDS as i64 {
            cache.remember(Some(world_seed::to_hash(seed)), seed);
        }
        assert_eq!(cache.get(Some(world_seed::to_hash(0))), Some(0));
        cache.remember(Some(world_seed::to_hash(-42)), -42);
        assert_eq!(cache.0.len(), MAX_VERIFIED_SEEDS);
        assert_eq!(cache.get(Some(world_seed::to_hash(1))), None);
        assert_eq!(cache.get(Some(world_seed::to_hash(0))), Some(0));
        cache.remember(Some(world_seed::to_hash(-42)), -42);
        assert_eq!(cache.0.len(), MAX_VERIFIED_SEEDS);
    }

    #[test]
    fn only_current_hash_verified_callbacks_enter_the_cache() {
        let cache = Arc::new(Mutex::new(VerifiedSeeds::default()));
        let host = GameHost::new(1, 1, Some(world_seed::to_hash(-42)), cache.clone());
        host.print_seed("candidate", -42);
        host.world_seed_found(7);
        assert!(lock(&cache).0.is_empty());
        host.world_seed_found(-42);
        assert_eq!(lock(&cache).get(host.hash), Some(-42));
        let cancelled = GameHost::new(1, 2, Some(world_seed::to_hash(7)), cache.clone());
        cancelled.cancelled.store(true, Ordering::Release);
        cancelled.world_seed_found(7);
        let local = GameHost::new(1, 3, None, cache.clone());
        local.found(7, SeedOrigin::Local);
        lock(&cache).remember(None, 7);
        lock(&cache).remember(Some(world_seed::to_hash(-42)), 7);
        assert_eq!(lock(&cache).0.len(), 1);
        assert_eq!(lock(&cache).get(None), None);
        assert_eq!(lock(&cache).get(Some(world_seed::to_hash(7))), None);
    }

    #[test]
    fn lifecycle_boundaries_reuse_only_matching_hashes_with_fresh_delivery_ids() {
        let mut c = Controller::default();
        let hash = Some(world_seed::to_hash(-42));
        // The normal Host path populates the process cache; no manual-setting path does so.
        let host = GameHost::new(0, 1, hash, c.cache.clone());
        host.world_seed_found(-42);
        let mut previous = (0, 0);
        for _ in 0..3 { // death/Respawn, dimension/Respawn, reconnect/Login
            c.boundary();
            assert!(c.read().found.is_none());
            c.reuse(None);
            c.reuse(Some(world_seed::to_hash(7)));
            assert!(c.read().found.is_none());
            c.reuse(hash);
            let snap = c.read();
            let found = snap.found.unwrap();
            assert_eq!((found.value, found.origin), (-42, SeedOrigin::Reused));
            assert!(found.world > previous.0 && found.run > previous.1);
            previous = (found.world, found.run);
            assert_eq!(snap.phase, "Seed reused");
            assert_eq!((snap.observations, snap.structure_candidates), (0, 0));
            assert_eq!((snap.base_bits, snap.lifting_bits), (0.0, 0.0));
            assert!(c.active.is_none());
        }
        c.retire(); // off/restart also preserves the bounded process cache
        assert!(c.read().found.is_none());
        c.reuse(hash);
        assert_eq!(c.read().found.unwrap().value, -42);
    }

    #[test]
    fn database_gate_and_hash_recheck_reject_stale_rows() {
        let requests = AtomicUsize::new(0);
        let enabled = Policy { automatic: true, database: true, threads: 1 };
        let host = new_host(1, 1, None);
        fetch_database(&host, enabled, || { requests.fetch_add(1, Ordering::Relaxed); Ok(String::new()) });
        assert_eq!(requests.load(Ordering::Relaxed), 0);
        let host = new_host(1, 2, Some(world_seed::to_hash(42)));
        for policy in [Policy::default(), Policy { database: false, ..enabled }, Policy { automatic: false, ..enabled }] {
            fetch_database(&host, policy, || { requests.fetch_add(1, Ordering::Relaxed); Ok(String::new()) });
        }
        assert_eq!(requests.load(Ordering::Relaxed), 0);
        fetch_database(&host, enabled, || {
            requests.fetch_add(1, Ordering::Relaxed);
            Ok(format!("header\nserver,x,7L,x,x,x,{}", world_seed::to_hash(42)))
        });
        assert!(lock(&host.published).found.is_none());
        let current = new_host(1, 3, host.hash);
        fetch_database(&current, enabled, || {
            requests.fetch_add(1, Ordering::Relaxed);
            Ok(format!("header\nserver,x,42L,x,x,x,{}", world_seed::to_hash(42)))
        });
        assert_eq!(lock(&current.published).found.unwrap().origin, SeedOrigin::Database);
        fetch_database(&current, enabled, || { requests.fetch_add(1, Ordering::Relaxed); Ok(String::new()) });
        host.cancelled.store(true, Ordering::Release);
        fetch_database(&host, enabled, || { requests.fetch_add(1, Ordering::Relaxed); Ok(String::new()) });
        assert_eq!(requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn backpressure_retries_and_neighbor_arrival_reschedules_old_targets() {
        let latest = RwLock::new(None);
        let mut scans = Scans::default();
        let first = Arc::new(Chunk::empty(0, 0));
        scans.publish(HashMap::from([((0, 0), first.clone())]), 0, &latest, Dimension::Overworld);
        let (tx, rx) = sync_channel(1);
        tx.try_send(Scan { at: (9, 9), revision: 0 }).unwrap();
        scans.send(&tx);
        assert_eq!(scans.seen[&(0, 0)].sent, 0);
        rx.recv().unwrap();
        scans.send(&tx);
        assert_eq!(rx.recv().unwrap().at, (0, 0));
        let sent = scans.seen[&(0, 0)].sent;
        scans.publish(HashMap::from([((0, 0), first.clone()), ((3, 0), Arc::new(Chunk::empty(3, 0)))]), 1, &latest, Dimension::Overworld);
        assert!(scans.seen[&(0, 0)].dirty > sent);
        let before = scans.seen[&(0, 0)].dirty;
        scans.publish(HashMap::from([((0, 0), Arc::new((*first).clone()))]), 20, &latest, Dimension::Overworld);
        assert!(scans.seen[&(0, 0)].dirty > before);
        let mut end = Scans::default();
        let ring = HashMap::from([((-3, 0), Arc::new(Chunk::empty(-3, 0))), ((3, 0), Arc::new(Chunk::empty(3, 0)))]);
        end.publish(ring.clone(), 0, &latest, Dimension::End);
        let sent = end.seen[&(-3, 0)].dirty;
        let mut ring = ring;
        ring.insert((3, 0), Arc::new(Chunk::empty(3, 0)));
        end.publish(ring, 20, &latest, Dimension::End);
        assert!(end.seen[&(-3, 0)].dirty > sent);
    }

    #[test]
    fn boundary_and_stop_clear_published_delivery_without_changing_world_on_stop() {
        let mut c = Controller { world: 5, run: 7, idle: Snapshot {
            world: 5, run: 7, found: Some(FoundSeed { world: 5, run: 7, value: 42, origin: SeedOrigin::Cracked }),
            ..Snapshot::default() }, ..Controller::default() };
        c.retire();
        assert!(c.read().found.is_none());
        assert_eq!(c.world, 5);
        c.boundary();
        assert_eq!(c.world, 6);
        assert!(c.read().found.is_none());
    }

    #[test]
    fn retired_worker_cannot_deliver_into_a_reconnect_with_the_same_hash() {
        let hash = Some(world_seed::to_hash(-42));
        let old = new_host(1, 1, hash);
        let (release, wait) = sync_channel(1);
        let callback = old.clone();
        let worker = std::thread::spawn(move || {
            wait.recv().unwrap();
            callback.world_seed_found(-42);
            callback.progress(SearchProgress { stage: SearchStage::Hash, checked: 1, total: 1 });
        });
        let (input, _receiver) = sync_channel(QUEUE_SIZE);
        let run = Run { storage: DataStorage::new(old.clone()), host: old.clone(), input,
            latest: Arc::new(RwLock::new(None)), worker, scans: Scans::default(), dimension: Dimension::Overworld };
        let mut c = Controller { world: 1, run: 1, active: Some(run), ..Controller::default() };
        c.boundary();
        let retiring = c.retiring.as_ref().unwrap().thread().id();
        // Repeated off/restart requests retain the same retiring handle; no replacement pool can start.
        for _ in 0..20 { c.retire(); c.reap(); }
        assert_eq!(c.retiring.as_ref().unwrap().thread().id(), retiring);
        assert!(c.active.is_none());
        assert!(c.read().found.is_none());
        release.send(()).unwrap();
        c.retiring.take().unwrap().join().unwrap();
        assert!(lock(&old.published).found.is_none());
        assert_eq!(lock(&old.published).checked, 0);
        let current = new_host(c.world, 2, hash);
        current.world_seed_found(-42);
        let found = lock(&current.published).found.unwrap();
        assert_eq!((found.world, found.run), (2, 2));
        assert_ne!((found.world, found.run), (old.world, old.run));
    }
}
