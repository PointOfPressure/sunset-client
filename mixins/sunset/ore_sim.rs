// Ore Sim: where the ores are, from the world's seed, for servers that hide them (anti-xray). Each loaded chunk
// with its eight neighbours goes to a thread that replays the dimension's ore features for that seed on the
// blocks the server sent: the engine's own ore roots from the worldgen bundle (counts, heights, sizes, air
// exposure, which biomes have them, and vanilla's decoration and feature seeds), so the blobs land where the
// world generator put them. What the server sends as stone where an ore should be is drawn as that ore; a block
// mined away stops being drawn. When the chunks round you are done it says how many it found and how many of
// them already show as ore, which tells a wrong seed (almost none) from the right one.

use super::{Dim, Game, Screen};
use super::sunset::{Camera, Canvas, Category, Module};
use super::sunset_seedcracker::{self as seedcracker, SeedOrigin};
use super::sunset_util::{dist, player_pos, toggle_off};
use super::sunset_visual::{Look, draw_box};
use horizon_client::chunk::{Chunk, MIN_Y};
use horizon_world::registry::{BlockRegistry, BlockStateId};
use horizon_world::worldgen::generator::sunset_ore_sim::{KINDS, SeenWorld, ore_sim};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub(super) const NAME: &str = "Ore Sim";

// Highest point the client keeps blocks for (chunk.rs: MIN_Y + HEIGHT).
const TOP: i32 = 320;
// Chunks handed to the thread a tick, so one turning on doesn't queue a whole render distance at once.
const SEND_PER_TICK: usize = 6;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::World, "Shows ores from the world seed.")
            .text("Seed", "")
            .pick("Seed source", &["Manual", "Auto-crack"])
            .bool("Database fetch", false)
            .num("Cracker threads", 2.0, 1.0, 8.0, 1.0)
            .num("Chunks", 5.0, 1.0, 10.0, 1.0)
            .text("Ores", "coal,copper,iron,lapis,redstone,gold,diamond,emerald,quartz,debris")
            .bool("Hide shown ores", false)
            .num("Max", 4096.0, 64.0, 16384.0, 64.0),
    ]
}

// Each kind's box colour, in KINDS order.
const COLOURS: [u32; 10] = [
    0xFF34_3434, 0xFFE0_7A4F, 0xFFD8_AF93, 0xFF1E_4ED8, 0xFFFF_2020, 0xFFFF_D700, 0xFF4F_E8E0, 0xFF17_DD62,
    0xFFEE_E6DC, 0xFF8A_5A44,
];

// One chunk to replay: where it is and its 3x3 neighbourhood, row by row from the north-west.
struct Job {
    at: (i32, i32),
    chunks: Vec<Arc<Chunk>>,
}

enum Done {
    Chunk((i32, i32), Vec<([i32; 3], u8)>),
    Failed(String),
}

struct Sim {
    // Sessions with the same seed and dimension still need separate replay state.
    key: (u64, i64, &'static str),
    jobs: SyncSender<Job>,
    done: Receiver<Done>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    sent: HashSet<(i32, i32)>,
    ores: HashMap<(i32, i32), Vec<([i32; 3], u8)>>,
    // Block states that are an ore, for the summary and Hide shown ores.
    ore_states: Vec<bool>,
    said: bool,
}

static SIM: Mutex<Option<Sim>> = Mutex::new(None);
static RETIRING: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

// A seed as vanilla's world creation reads one: a number, or else the text's hash.
fn seed(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.parse::<i64>().unwrap_or_else(|_| {
        i64::from(text.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c))))
    }))
}

pub(super) fn manual_seed(module: &Module) -> Option<i64> {
    seed(module.t("Seed"))
}

// This is also called while the runtime's settings lock is held: only read the controller's small status.
pub(super) fn info(module: &Module) -> Option<String> {
    if manual_seed(module).is_some() {
        Some(if module.p("Seed source") == "Auto-crack" { "Manual override" } else { "Manual" }.into())
    } else if module.p("Seed source") == "Auto-crack" && module.on {
        seedcracker::status_line().or_else(|| Some("Waiting for observations".into()))
    } else {
        Some("Manual".into())
    }
}

pub(super) fn status(game: &Game<'_>, module: &Module) -> Vec<String> {
    let enabled = if module.on { "" } else { "Off; " };
    if let Some(value) = manual_seed(module) {
        let source = if module.p("Seed source") == "Auto-crack" { "Manual override" } else { "Manual" };
        return vec![format!("Ore Sim seed: {enabled}{source} {value}.")];
    }
    if module.p("Seed source") != "Auto-crack" {
        return vec![format!("Ore Sim seed: {enabled}Manual; set a seed with .seed <seed> or select .seed auto.")];
    }
    if !module.on {
        return vec!["Ore Sim seed: Off; Auto-crack is selected.".into()];
    }
    let found = seedcracker::resolved(game);
    let snap = seedcracker::snapshot();
    let mut lines = vec![match found {
        Some(found) => format!("Ore Sim seed: {} {}.", origin(found.origin), found.value),
        None => format!("Ore Sim seed: {}.", seedcracker::status_line().unwrap_or_else(|| "Waiting for observations".into())),
    }];
    lines.push(format!(
        "Ore Sim seed: {} observations; {:.1}/{:.0} base bits, {:.1}/40 lifting bits; {} structure candidates (48-bit), {} world candidates.",
        snap.observations, snap.base_bits, snap.wanted_bits, snap.lifting_bits, snap.structure_candidates, snap.world_candidates,
    ));
    lines.push(format!("Ore Sim seed: verification hash {}.", if game.remote.is_some() && game.player.is_some() { "available" } else { "unavailable" }));
    if snap.total > 0 {
        lines.push(format!("Ore Sim seed: {} {}/{} checked.", snap.phase, snap.checked, snap.total));
    }
    if let Some(detail) = snap.detail {
        lines.push(format!("Ore Sim seed: {detail}"));
    }
    lines
}

fn origin(source: SeedOrigin) -> &'static str {
    match source {
        SeedOrigin::Local => "Local seed",
        SeedOrigin::Cracked => "Hash-verified seed",
        SeedOrigin::Database => "Hash-verified database seed",
    }
}

// Read the controller before SIM; never hold the two locks together.
fn input(game: &Game<'_>, module: &Module) -> Option<(u64, i64)> {
    let world = seedcracker::world_id(game);
    if let Some(value) = manual_seed(module) {
        Some((world, value))
    } else if module.p("Seed source") == "Auto-crack" {
        seedcracker::resolved(game).filter(|found| found.world == world).map(|found| (world, found.value))
    } else {
        None
    }
}

// Request cancellation immediately, without waiting for a worldgen chunk already being replayed.
pub(super) fn stop() {
    *SIM.lock().unwrap_or_else(|e| e.into_inner()) = None;
    reap_worker();
}

fn reap_worker() -> bool {
    let mut retired = RETIRING.lock().unwrap_or_else(|e| e.into_inner());
    if retired.as_ref().is_some_and(JoinHandle::is_finished) {
        if let Some(worker) = retired.take() { let _ = worker.join(); }
    }
    retired.is_none()
}

// Runs even when the normal module tick cannot: no old ores or work survive a world/lifecycle boundary.
pub(super) fn maintain(game: &Game<'_>) {
    if !game.native_window() { return; }
    if !super::sunset::running() || !game.native_on(super::sunset::ID)
        || game.player.is_none() || game.screen == Screen::Dead
    {
        stop();
        return;
    }
    let module = super::sunset::with(|s| s.module(NAME).filter(|m| m.on).cloned());
    let key = module.as_ref().and_then(|m| input(game, m)).map(|(world, value)| (world, value, dimension(game).0));
    let mut slot = SIM.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_some_and(|sim| Some(sim.key) != key) {
        *slot = None;
    }
    drop(slot);
    reap_worker();
}

// The dimension's id, its build range and its generator's range (vanilla's noise settings).
fn dimension(game: &Game<'_>) -> (&'static str, (i32, i32), (i32, i32, i32)) {
    match game.dim {
        Dim::Overworld => ("minecraft:overworld", (-64, 384), (-64, 384, 63)),
        Dim::Nether => ("minecraft:the_nether", (0, 256), (0, 128, 32)),
        Dim::End => ("minecraft:the_end", (0, 256), (0, 128, 0)),
    }
}

// What a column's floor (OCEAN_FLOOR_WG) is measured to as the ores generated: the terrain's full blocks, without
// the trees, mushrooms and crops features grow on it later. A floor too high or low runs (or skips) a blob the
// generator didn't, and that shifts every later blob of the feature in the chunk.
fn floor_blocks(game: &Game<'_>) -> Vec<bool> {
    const LATER: [&str; 10] = ["_log", "_wood", "_stem", "_hyphae", "mushroom_block", "bee_nest", "pumpkin", "melon", "cactus", "bamboo"];
    game.res
        .facts()
        .states
        .iter()
        .map(|s| {
            s.full_collision
                && !s.classes.iter().any(|c| &**c == "LeavesBlock")
                && !LATER.iter().any(|w| s.name.contains(w))
        })
        .collect()
}

fn ore_states(registry: &BlockRegistry) -> Vec<bool> {
    (0..registry.len() as u32)
        .map(|id| {
            registry
                .state(BlockStateId(id))
                .is_some_and(|s| s.name.ends_with("_ore") || s.name == "minecraft:ancient_debris")
        })
        .collect()
}

impl Sim {
    fn start(game: &Game<'_>, key: (u64, i64, &'static str), build: (i32, i32), generation: (i32, i32, i32)) -> Option<Self> {
        if !reap_worker() { return None; }
        let lifecycle = horizon_protocol::connection::sunset_lifecycle::RUNTIME.token();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let registry: &'static BlockRegistry = game.pack.assets.registry;
        let dir: PathBuf = game.options.resources.join("worldgen");
        let biome_names: Vec<String> = game.res.biomes().list.iter().map(|b| b.name.clone()).collect();
        let solid = floor_blocks(game);
        let (jobs, rx) = sync_channel::<Job>(SEND_PER_TICK * 2);
        let (tx, done) = channel();
        let thread = move || {
            let current = || !cancel.load(Ordering::Acquire) && horizon_protocol::connection::sunset_lifecycle::RUNTIME.current(lifecycle);
            if !current() { return; }
            let loaded = horizon_client::vanillagen::shared_data(&dir, registry)
                .map_err(|e| e.to_string())
                .and_then(|data| ore_sim(&data, "minecraft:normal", key.2, build, generation));
            if !current() { return; }
            let sim = match loaded {
                Ok(Some(sim)) if sim.roots() > 0 => sim,
                Ok(_) => {
                    let _ = tx.send(Done::Failed("no ores generate in this dimension".into()));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(Done::Failed(format!("the world generator would not load ({e})")));
                    return;
                }
            };
            let biomes: Vec<Option<u32>> = biome_names.iter().map(|n| sim.biome_index(n)).collect();
            let air = registry.default_state("minecraft:air").map_or(0, |s| s.0);
            for job in rx {
                if !current() { return; }
                let view = View { at: job.at, chunks: &job.chunks, biomes: &biomes, solid: &solid, air };
                let found = sim.chunk(key.1, job.at.0, job.at.1, &view);
                if !current() { return; }
                if tx.send(Done::Chunk(job.at, found)).is_err() {
                    return;
                }
            }
        };
        let worker = std::thread::Builder::new().name("sunset-ore-sim".into()).spawn(thread).ok()?;
        Some(Self {
            key,
            jobs,
            done,
            cancelled,
            worker: Some(worker),
            sent: HashSet::new(),
            ores: HashMap::new(),
            ore_states: ore_states(registry),
            said: false,
        })
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        // start() waits for this handle before launching another replay, so rapid toggles keep one retiring worker.
        *RETIRING.lock().unwrap_or_else(|e| e.into_inner()) = self.worker.take();
    }
}

// The blocks round one chunk as the replay reads them.
struct View<'a> {
    at: (i32, i32),
    chunks: &'a [Arc<Chunk>],
    biomes: &'a [Option<u32>],
    solid: &'a [bool],
    air: u32,
}

impl View<'_> {
    fn chunk(&self, x: i32, z: i32) -> Option<&Chunk> {
        let (dx, dz) = ((x >> 4) - self.at.0 + 1, (z >> 4) - self.at.1 + 1);
        ((0..3).contains(&dx) && (0..3).contains(&dz)).then(|| &*self.chunks[(dz * 3 + dx) as usize])
    }
}

impl SeenWorld for View<'_> {
    fn block(&self, p: [i32; 3]) -> Option<BlockStateId> {
        let chunk = self.chunk(p[0], p[2])?;
        Some(BlockStateId(if (MIN_Y..TOP).contains(&p[1]) { chunk.get(p[0], p[1], p[2]) } else { self.air }))
    }
    fn biome(&self, p: [i32; 3]) -> Option<u32> {
        let chunk = self.chunk(p[0], p[2])?;
        *self.biomes.get(usize::from(chunk.biome(p[0], p[1], p[2])))?
    }
    fn floor(&self, x: i32, z: i32) -> Option<i32> {
        let chunk = self.chunk(x, z)?;
        let top = chunk.highest(self.air, |y| self.solid.get(chunk.get(x, y, z) as usize).copied().unwrap_or(false));
        Some(top.map_or(MIN_Y, |y| y + 1))
    }
    fn biomes_near(&self) -> Vec<u32> {
        let mut found = HashSet::new();
        for chunk in self.chunks {
            let (x0, z0) = (chunk.x * 16, chunk.z * 16);
            for y in (MIN_Y..TOP).step_by(4) {
                for q in 0..16 {
                    let biome = chunk.biome(x0 + (q & 3) * 4, y, z0 + (q >> 2) * 4);
                    if let Some(Some(b)) = self.biomes.get(usize::from(biome)) {
                        found.insert(*b);
                    }
                }
            }
        }
        found.into_iter().collect()
    }
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else {
        stop();
        return;
    };
    let Some((world, seed)) = input(game, m) else {
        stop();
        if m.p("Seed source") != "Auto-crack" {
            super::sunset::say(game, "Ore Sim: set the world's seed first (.seed <seed> or .set oresim seed <seed>).");
            toggle_off(game, NAME);
        }
        return;
    };
    let Some(here) = player_pos(game) else { return };
    let (dim, build, generation) = dimension(game);
    let key = (world, seed, dim);
    let mut slot = SIM.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_none_or(|s| s.key != key) {
        *slot = None;
        *slot = Sim::start(game, key, build, generation);
        let started = slot.is_some();
        drop(slot);
        if started && manual_seed(m).is_none() {
            if let Some(found) = seedcracker::resolved(game) {
                super::sunset::say(game, &format!("Ore Sim seed: {} {}.", origin(found.origin), found.value));
            }
        }
        slot = SIM.lock().unwrap_or_else(|e| e.into_inner());
    }
    let Some(sim) = slot.as_mut() else { return };
    let mut failed = None;
    for done in sim.done.try_iter() {
        match done {
            Done::Chunk(at, found) => {
                sim.ores.insert(at, found);
            }
            Done::Failed(why) => failed = Some(why),
        }
    }
    if let Some(why) = failed {
        *slot = None;
        drop(slot);
        super::sunset::say(game, &format!("Ore Sim: {why}."));
        toggle_off(game, NAME);
        return;
    }
    let r = m.n("Chunks") as i32;
    let (cx, cz) = ((here[0].floor() as i32) >> 4, (here[2].floor() as i32) >> 4);
    let chunks = &game.world.chunks;
    // Chunks gone (or now far) are forgotten, and done again when they come back.
    let near = |c: &(i32, i32)| (c.0 - cx).abs() <= r + 1 && (c.1 - cz).abs() <= r + 1 && chunks.contains_key(c);
    sim.sent.retain(near);
    sim.ores.retain(|c, _| near(c));
    let mut sent = 0;
    let mut waiting = false;
    for dz in -r..=r {
        for dx in -r..=r {
            let at = (cx + dx, cz + dz);
            if sim.sent.contains(&at) {
                continue;
            }
            let around: Option<Vec<Arc<Chunk>>> =
                (-1..=1).flat_map(|j| (-1..=1).map(move |i| (at.0 + i, at.1 + j))).map(|c| chunks.get(&c).cloned()).collect();
            let Some(around) = around else { continue };
            waiting = true;
            if sent < SEND_PER_TICK && sim.jobs.try_send(Job { at, chunks: around }).is_ok() {
                sim.sent.insert(at);
                sent += 1;
            }
        }
    }
    if sim.said || waiting || sim.ores.is_empty() || sim.ores.len() < sim.sent.len() {
        return;
    }
    sim.said = true;
    let found: usize = sim.ores.values().map(Vec::len).sum();
    let shown = sim
        .ores
        .values()
        .flatten()
        .filter(|(p, _)| sim.ore_states.get(game.world.block(p[0], p[1], p[2]).0 as usize).copied().unwrap_or(false))
        .count();
    let line = format!("Ore Sim: {found} ores in {} chunks, {shown} of them already show as ore.", sim.ores.len());
    drop(slot);
    super::sunset::say(game, &line);
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else { return };
    let Some((world, seed)) = input(game, m) else { return };
    let key = (world, seed, dimension(game).0);
    let Some(here) = player_pos(game) else { return };
    let wanted: Vec<bool> = KINDS.iter().map(|k| m.t("Ores").split(',').any(|o| o.trim() == *k)).collect();
    let (hide_shown, max) = (m.b("Hide shown ores"), m.n("Max") as usize);
    let range = (m.n("Chunks") * 16.0 + 16.0) * 1.5;
    let slot = SIM.lock().unwrap_or_else(|e| e.into_inner());
    let Some(sim) = slot.as_ref() else { return };
    if sim.key != key { return; }
    let mut drawn = 0;
    for &(p, kind) in sim.ores.values().flatten() {
        if !wanted[kind as usize] {
            continue;
        }
        let lo = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
        if dist(here, [lo[0] + 0.5, lo[1] + 0.5, lo[2] + 0.5]) > range {
            continue;
        }
        let state = game.world.block(p[0], p[1], p[2]);
        if game.world.is_air(state) || (hide_shown && sim.ore_states.get(state.0 as usize).copied().unwrap_or(false)) {
            continue;
        }
        if draw_box(cam, out, &Look::BOXED, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], COLOURS[kind as usize], lw) {
            drawn += 1;
            if drawn >= max {
                break;
            }
        }
    }
}
