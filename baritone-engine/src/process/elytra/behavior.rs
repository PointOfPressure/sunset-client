// baritone.process.elytra.ElytraBehavior, the part that touches the game: each tick it feeds the loaded chunks to the
// pathfinder's octrees, runs the path manager (which asks for searches on worker threads), solves where to aim, aims, and
// fires a rocket when the speed drops. The flight physics and the solver are `baritone_elytra_flight`, the search
// scheduling `baritone_elytra_path_manager`; this is what Java's ElytraBehavior adds around them.
//
// Where it differs from the Java, on purpose:
//  * The solver runs on the game thread every tick (the Java's fallback when its solution from the end of the last tick does
//    not match), not on a thread of its own with a result to reuse.
//  * A chunk event does not reach a mod, so loaded chunks are noticed by the chunk set changing, and the ones around the player
//    are packed again now and then in case a chunk was sent again. The blocks the game sets (BlockChangeEvent) do reach it, and
//    go into the packed chunks at once.
//  * Rays that leave the pathfinder's height are taken as clear: there is nothing above the build limit or under bedrock.
use super::baritone::Core;
use super::baritone_aim::AimSettings;
use super::baritone_elytra_build_limit::{BuildLimitPathFinder, NetherSearch, SkyProbe};
use super::baritone_elytra_chunk::Chunk;
use super::baritone_elytra_context::{spawn_job, BlockStateOctreeInterface, NetherPathfinderContext, PathFuture, SearchOptions};
use super::baritone_elytra_flight::{wants_firework, Aabb, FireworkBoost, FlightSettings, FlightWorld, Solution, Solver, SolverContext, TickableAim, V3};
use super::baritone_elytra_path_manager::{fix_destination, Job, Note, PathEnv, PathManager};
use super::baritone_elytra_segment::{PathCalculationError, UnpackedSegment};
use super::baritone_gworld as gw;
use super::baritone_player_context::PlayerContext;
use super::baritone_rotation::Rotation;
use super::baritone_settings::Settings;
use super::Game;
use super::baritone_game_ctx::GameCtx;
use std::cell::Cell;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

thread_local! {
    /// A `ClientboundPlayerPositionPacket` arrived since the last tick.
    static SET_BACK: Cell<bool> = const { Cell::new(false) };
}

/// The game's handler of that packet, for `onReceivePacket`.
pub fn on_set_back() {
    SET_BACK.with(|s| s.set(true));
}

type Pos = [i32; 3];

/// A dimension's lowest block and height, as the game defines them.
pub fn dimension_extent(dimension: &str) -> (i32, i32) {
    match dimension {
        "minecraft:the_nether" | "minecraft:the_end" => (0, 256),
        _ => (-64, 384),
    }
}

/// What the behaviour reads of the player once a tick.
#[derive(Clone)]
pub struct PlayerSnapshot {
    pub feet: Pos,
    pub position: V3,
    pub head: V3,
    pub motion: V3,
    pub rotation: Rotation,
    pub fall_flying: bool,
    pub in_lava: bool,
    #[allow(dead_code)]
    pub on_ground: bool,
    pub bounding_box: Aabb,
    pub tick: u64,
}

impl PlayerSnapshot {
    pub fn take(ctx: &GameCtx<'_, '_>) -> Self {
        let position = V3::from_array(ctx.position());
        let fall_flying = ctx.with_game(|g| g.player.as_ref().is_some_and(|p| p.fall_flying));
        // The flying pose is as high as it is wide.
        let (w, h) = (0.3, if fall_flying { 0.6 } else { 1.8 });
        Self {
            feet: ctx.player_feet(),
            position,
            head: V3::from_array(ctx.player_head()),
            motion: V3::from_array(ctx.delta_movement()),
            rotation: ctx.player_rotations(),
            fall_flying,
            in_lava: ctx.is_in_lava(),
            on_ground: ctx.on_ground(),
            bounding_box: Aabb { min_x: position.x - w, min_y: position.y, min_z: position.z - w, max_x: position.x + w, max_y: position.y + h, max_z: position.z + w },
            tick: ctx.with_game(|g| g.ticks),
        }
    }
}

/// What the process says of itself that the behaviour reads.
#[derive(Clone, Copy, Default)]
pub struct ProcessFlags {
    pub landing: bool,
    pub get_to_jump: bool,
}

/// The world as the solver and the path manager see it.
pub struct GameEnv<'a, 'c, 'g> {
    ctx: &'a GameCtx<'c, 'g>,
    npf: &'a NetherPathfinderContext,
    boi: BlockStateOctreeInterface,
    snap: &'a PlayerSnapshot,
    flags: ProcessFlags,
    destination: Pos,
    in_nether: bool,
    world_min_y: i32,
    world_max_y: i32,
    render_raytraces: bool,
    lines: std::cell::RefCell<(Vec<(V3, V3)>, Vec<(V3, V3)>)>,
}

impl<'a, 'c, 'g> GameEnv<'a, 'c, 'g> {
    #[allow(clippy::too_many_arguments)]
    fn new(ctx: &'a GameCtx<'c, 'g>, npf: &'a NetherPathfinderContext, snap: &'a PlayerSnapshot, flags: ProcessFlags, destination: Pos, in_nether: bool, world_min_y: i32, world_max_y: i32, render_raytraces: bool) -> Self {
        Self { ctx, npf, boi: BlockStateOctreeInterface::new(npf), snap, flags, destination, in_nether, world_min_y, world_max_y, render_raytraces, lines: Default::default() }
    }

    fn block_name_at(&self, x: i32, y: i32, z: i32) -> String {
        self.ctx.with_game(|g| gw::block_name(g, [x, y, z]))
    }

    fn is_air_or_lava(&self, x: i32, y: i32, z: i32) -> bool {
        let name = self.block_name_at(x, y, z);
        matches!(gw::bare(&name), "air" | "cave_air" | "void_air" | "lava")
    }

    /// `world.clip(COLLIDER, no fluid)` came back with nothing: sampled along the ray.
    fn world_clip_clear(&self, start: V3, dest: V3) -> bool {
        let delta = dest - start;
        let length = delta.length();
        let steps = (length / 0.1).ceil().max(1.0) as i32;
        let bsi = PlayerContext::bsi(self.ctx);
        for i in 0..=steps {
            let p = start + delta.scale(f64::from(i) / f64::from(steps));
            let state = bsi.get0(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            if state.full_collision {
                return false;
            }
        }
        true
    }

    /// The rays recorded for rendering, clear and blocked.
    fn take_lines(&self) -> (Vec<(V3, V3)>, Vec<(V3, V3)>) {
        std::mem::take(&mut *self.lines.borrow_mut())
    }

    /// `ElytraBehavior.raytrace(start, end)`.
    fn raytrace(&self, start: V3, end: V3) -> bool {
        let max_height = self.npf.max_height() + self.world_min_y;
        let out_of_bounds = start.y >= f64::from(max_height) || end.y >= f64::from(max_height) || start.y < f64::from(self.world_min_y) || end.y < f64::from(self.world_min_y);
        if out_of_bounds {
            return true;
        }
        self.npf.raytrace(start.to_array(), end.to_array())
    }
}

impl FlightWorld for GameEnv<'_, '_, '_> {
    fn passable(&self, x: i32, y: i32, z: i32, ignore_lava: bool) -> bool {
        if ignore_lava {
            return self.is_air_or_lava(x, y, z);
        }
        if y >= self.world_max_y || y < self.world_min_y {
            return true;
        }
        self.boi.passable(x, y, z)
    }

    fn clear_view(&self, start: V3, dest: V3, ignore_lava: bool) -> bool {
        let clear = if ignore_lava {
            self.world_clip_clear(start, dest)
        } else {
            // If start == dest then the raytracer dies.
            start == dest || self.raytrace(start, dest)
        };
        if self.render_raytraces {
            let mut lines = self.lines.borrow_mut();
            if clear { lines.0.push((start, dest)) } else { lines.1.push((start, dest)) }
        }
        clear
    }

    fn all_clear(&self, src: &[f64; 24], dst: &[f64; 24]) -> bool {
        let max_height = f64::from(self.npf.max_height() + self.world_min_y);
        let min_height = f64::from(self.world_min_y);
        let out_of_bounds = (0..8).any(|i| src[i * 3 + 1] >= max_height || src[i * 3 + 1] < min_height || dst[i * 3 + 1] >= max_height || dst[i * 3 + 1] < min_height);
        if out_of_bounds {
            return true;
        }
        self.npf.raytrace_all(src, dst)
    }
}

impl PathEnv for GameEnv<'_, '_, '_> {
    fn player_feet(&self) -> Pos {
        self.snap.feet
    }

    fn player_y(&self) -> f64 {
        self.snap.position.y
    }

    fn is_nether(&self) -> bool {
        self.in_nether
    }

    fn player_feet_vec(&self) -> V3 {
        self.snap.position
    }

    fn player_head(&self) -> V3 {
        self.snap.head
    }

    fn is_fall_flying(&self) -> bool {
        self.snap.fall_flying
    }

    fn npf_has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.npf.has_chunk(chunk_x, chunk_z)
    }

    fn world_has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.ctx.with_game(|g| g.world.chunks.contains_key(&(chunk_x, chunk_z)))
    }

    fn destination_fixed(&self) -> Pos {
        fix_destination(self.destination, self.snap.position.y, self.in_nether)
    }

    fn is_landing(&self) -> bool {
        self.flags.landing
    }

    fn is_get_to_jump(&self) -> bool {
        self.flags.get_to_jump
    }

    fn player_distance_sqr_to_center(&self, pos: Pos) -> f64 {
        let c = V3::new(f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5);
        (self.snap.position - c).length_sqr()
    }
}

/// The nether pathfinder as a blocking search, for the build limit finder to call.
struct NpfSearch {
    ctx: Arc<NetherPathfinderContext>,
    options: SearchOptions,
}

impl NetherSearch for NpfSearch {
    fn path_find(&self, src: Pos, dst: Pos) -> Result<UnpackedSegment, PathCalculationError> {
        self.ctx.path_find(src, dst, self.options)
    }
}

/// The sky above one chunk, read on the game thread: all the build limit finder asks of the world is about the chunk it starts in.
struct SkySnapshot {
    chunk: (i32, i32),
    loaded: bool,
    /// The height of the highest block of each column of the chunk (x + 16 z), at or above `floor`, else `floor - 1`.
    heights: Vec<i32>,
}

impl SkyProbe for SkySnapshot {
    fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.loaded && (chunk_x, chunk_z) == self.chunk
    }

    fn motion_blocking_height(&self, x: i32, z: i32) -> i32 {
        if (x >> 4, z >> 4) != self.chunk {
            return i32::MIN;
        }
        self.heights[((x & 15) + 16 * (z & 15)) as usize]
    }
}

/// Which search a trip uses (the constructor of the Java's ElytraBehavior).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FinderKind {
    Nether,
    BuildLimit,
}

/// A `GameEnv` over the behaviour's fields, borrowing only those (a method would borrow all of it, and the path manager is
/// mutated while the env is in use).
macro_rules! game_env {
    ($b:expr, $ctx:expr, $snap:expr, $flags:expr, $settings:expr) => {
        GameEnv::new($ctx, &$b.npf, $snap, $flags, $b.destination, $b.in_nether, $b.min_y, $b.world_max_y, $settings.flag("elytraRenderRaytraces"))
    };
}

pub struct ElytraBehavior {
    pub destination: Pos,
    pub npf: Arc<NetherPathfinderContext>,
    pub path_manager: PathManager,
    finder: FinderKind,
    options: SearchOptions,
    long_distance_threshold: i64,
    running: Option<(Job, PathFuture)>,
    /// How the last search to the destination from `pathToDestination(from)` ended: Some(true) it worked.
    destination_result: Option<bool>,
    /// Remaining cool-down ticks between firework usage.
    remaining_firework_ticks: i32,
    /// Remaining cool-down ticks after the player's position and rotation are reset by the server.
    remaining_set_back_ticks: i32,
    pub landing_mode: bool,
    /// The most recent minimum number of firework boost ticks, equivalent to `10 * (1 + Flight)`.
    minimum_boost_ticks: i32,
    deployed_firework_last_tick: bool,
    next_tick_boost_counter: [i32; 2],
    /// The tick a rocket was first seen attached to the player.
    attached_since: Option<u64>,
    time_last_cache_cull: u64,
    // Render.
    pub simulation_line: Option<Vec<V3>>,
    pub aim_pos: Option<Pos>,
    pub visible_path: Vec<Pos>,
    pub clear_lines: Vec<(V3, V3)>,
    pub blocked_lines: Vec<(V3, V3)>,
    notes: Vec<Note>,
    // Packing.
    /// `invTransactionQueue`: the clicks of swapping a worn-out elytra for a good one, one every ticksBetweenInventoryMoves.
    inv_queue: VecDeque<usize>,
    inv_countdown: i32,
    pack_queue: VecDeque<(i32, i32)>,
    pack_seen: HashSet<(i32, i32)>,
    pack_revision: u64,
    last_repack: u64,
    ids: Option<PackIds>,
    min_y: i32,
    world_max_y: i32,
    in_nether: bool,
    pub bad_landing_spot: Option<Pos>,
}

/// The block state ids the packer tells from solid.
struct PackIds {
    air: u32,
    cave_air: u32,
    red_mushroom: u32,
    brown_mushroom: u32,
    any_air: HashSet<u32>,
}

impl ElytraBehavior {
    #[allow(clippy::too_many_arguments)]
    pub fn new(core: &Core, game: &Game<'_>, npf: Arc<NetherPathfinderContext>, destination: Pos, append_destination: bool, in_nether: bool, dimension: &str) -> Self {
        let s = &core.settings;
        SET_BACK.with(Cell::take); // the behavior only hears the packets from its creation on
        let (min_y, height) = dimension_extent(dimension);
        let finder = if in_nether {
            if s.flag("elytraAllowAboveRoof") && s.flag("elytraAllowAboveBuildLimit") { FinderKind::BuildLimit } else { FinderKind::Nether }
        } else if s.flag("elytraAllowAboveBuildLimit") {
            FinderKind::BuildLimit
        } else {
            FinderKind::Nether
        };
        Self {
            destination,
            npf,
            path_manager: PathManager::new(append_destination),
            finder,
            options: SearchOptions { predict_terrain: s.flag("elytraPredictTerrain") && in_nether, allow_tight_spaces: s.flag("elytraAllowTightSpaces") },
            long_distance_threshold: i64::from(s.int("elytraLongDistanceThreshold")),
            running: None,
            destination_result: None,
            remaining_firework_ticks: 0,
            remaining_set_back_ticks: 0,
            landing_mode: false,
            minimum_boost_ticks: 0,
            deployed_firework_last_tick: false,
            next_tick_boost_counter: [0; 2],
            attached_since: None,
            time_last_cache_cull: game.ticks,
            simulation_line: None,
            aim_pos: None,
            visible_path: Vec::new(),
            clear_lines: Vec::new(),
            blocked_lines: Vec::new(),
            notes: Vec::new(),
            inv_queue: VecDeque::new(),
            inv_countdown: 0,
            pack_queue: VecDeque::new(),
            pack_seen: HashSet::new(),
            pack_revision: u64::MAX,
            last_repack: 0,
            ids: None,
            min_y,
            world_max_y: min_y + height - 1,
            in_nether,
            bad_landing_spot: None,
        }
    }

    /// The chat lines said since the last call: `Direct` always, `Verbose` for elytraChatSpam.
    pub fn take_notes(&mut self) -> Vec<Note> {
        let mut notes = std::mem::take(&mut self.notes);
        notes.extend(self.path_manager.take_notes());
        notes
    }

    fn verbose(&mut self, s: &str) {
        self.notes.push(Note::Verbose(s.to_owned()));
    }

    fn direct(&mut self, s: &str) {
        self.notes.push(Note::Direct(s.to_owned()));
    }

    /// The last search to the destination begun by `path_to_destination_from`, once it ended.
    pub fn take_destination_result(&mut self) -> Option<bool> {
        self.destination_result.take()
    }

    #[allow(dead_code)]
    pub fn is_searching(&self) -> bool {
        self.running.is_some()
    }

    // ---- searches ----

    fn start(&mut self, ctx: &GameCtx<'_, '_>, snap: &PlayerSnapshot, job: Job) {
        let npf = self.npf.clone();
        let options = self.options;
        let (src, dst) = (job.src, job.dst);
        let future = match self.finder {
            FinderKind::Nether => spawn_job(move || npf.path_find(src, dst, options)),
            FinderKind::BuildLimit => {
                let sky = self.sky_snapshot(ctx, src);
                let (min_y, max_y, threshold) = (self.min_y, self.world_max_y, self.long_distance_threshold);
                let _ = snap;
                spawn_job(move || {
                    let nether = NpfSearch { ctx: npf.clone(), options };
                    match BuildLimitPathFinder::new(min_y, max_y, npf.max_height(), threshold, &nether, &sky) {
                        Ok(finder) => finder.path_find(src, dst),
                        Err(e) => Err(PathCalculationError(e)),
                    }
                })
            }
        };
        self.running = Some((job, future));
    }

    fn sky_snapshot(&self, ctx: &GameCtx<'_, '_>, src: Pos) -> SkySnapshot {
        let chunk = (src[0] >> 4, src[2] >> 4);
        let nether_top = self.npf.max_height() + self.min_y - 1;
        let floor = src[1].min(nether_top - 16);
        let loaded = ctx.with_game(|g| g.world.chunks.contains_key(&chunk));
        let mut heights = vec![floor - 1; 256];
        if loaded {
            ctx.with_game(|g| {
                for lz in 0..16 {
                    for lx in 0..16 {
                        let (x, z) = ((chunk.0 << 4) + lx, (chunk.1 << 4) + lz);
                        if let Some(y) = (floor..=self.world_max_y).rev().find(|&y| !g.res.facts().is_air(g.world.block(x, y, z))) {
                            heights[(lx + 16 * lz) as usize] = y + 1;
                        }
                    }
                }
            });
        }
        SkySnapshot { chunk, loaded, heights }
    }

    /// `pathTo`: begin the trip. `Some(job)` is not needed by the caller: the search starts here.
    pub fn path_to(&mut self, ctx: &GameCtx<'_, '_>, snap: &PlayerSnapshot, settings: &Settings, flags: ProcessFlags) {
        if !settings.flag("elytraAutoJump") || snap.fall_flying {
            self.path_to_destination_from(ctx, snap, snap.feet, flags, settings);
        }
    }

    /// `pathManager.pathToDestination(from)`.
    pub fn path_to_destination_from(&mut self, ctx: &GameCtx<'_, '_>, snap: &PlayerSnapshot, from: Pos, flags: ProcessFlags, settings: &Settings) {
        let env = game_env!(self, ctx, snap, flags, settings);
        let job = self.path_manager.path_to_destination(from, &env);
        drop(env);
        self.start(ctx, snap, job);
    }

    /// The answer of the running search, if it came.
    fn poll_search(&mut self, ctx: &GameCtx<'_, '_>, snap: &PlayerSnapshot, flags: ProcessFlags, settings: &Settings) {
        let Some((_, future)) = &self.running else { return };
        let Some(result) = future.poll() else { return };
        let (job, _) = self.running.take().expect("checked above");
        let was_destination = matches!(job.kind(), super::baritone_elytra_path_manager::JobKind::Destination);
        let ok = result.is_ok();
        let env = game_env!(self, ctx, snap, flags, settings);
        let out = self.path_manager.finish(job, result, &env);
        drop(env);
        if let Some(spot) = out.bad_landing_spot {
            self.bad_landing_spot = Some(spot);
        }
        if was_destination {
            self.destination_result = Some(ok);
        }
    }

    // ---- packing ----

    fn pack_ids(game: &Game<'_>) -> PackIds {
        let facts = game.res.facts();
        let mut ids = PackIds { air: facts.air.0 as u32, cave_air: u32::MAX, red_mushroom: u32::MAX, brown_mushroom: u32::MAX, any_air: HashSet::new() };
        for (i, s) in facts.states.iter().enumerate() {
            match gw::bare(&s.name) {
                "air" => {
                    ids.any_air.insert(i as u32);
                }
                "cave_air" => {
                    ids.cave_air = i as u32;
                    ids.any_air.insert(i as u32);
                }
                "void_air" => {
                    ids.any_air.insert(i as u32);
                }
                "red_mushroom" if ids.red_mushroom == u32::MAX => ids.red_mushroom = i as u32,
                "brown_mushroom" if ids.brown_mushroom == u32::MAX => ids.brown_mushroom = i as u32,
                _ => {}
            }
        }
        ids
    }

    /// `writeChunkData`: the chunk as one bit per block.
    fn pack_chunk(&self, game: &Game<'_>, cx: i32, cz: i32) -> Option<Chunk> {
        let ids = self.ids.as_ref()?;
        let chunk = game.world.chunks.get(&(cx, cz))?;
        let shift = (horizon_client::chunk::MIN_Y - self.min_y) >> 4;
        let mut packed = Chunk::new();
        let mut values = [0u32; 4096];
        for (s, section) in chunk.sections.iter().enumerate() {
            let target = s as i32 + shift;
            // The pathfinder's support stops at 384 / 16 sections.
            if !(0..24).contains(&target) {
                continue;
            }
            if !section.blocks.maybe_has(|v| !ids.any_air.contains(&v)) {
                continue; // hasOnlyAir
            }
            // Mushrooms spawn on the roof and writing them as solid would allocate pages for nothing.
            if !section.blocks.maybe_has(|v| v == ids.air || v == ids.cave_air) {
                packed.fill_section(target as usize, true);
                continue;
            }
            section.unpack(&mut values);
            let y_real = target * 16;
            for (idx, &value) in values.iter().enumerate() {
                if !(value == ids.air || value == ids.cave_air) && value != ids.red_mushroom && value != ids.brown_mushroom {
                    packed.set_block((idx & 15) as i32, y_real + (idx >> 8) as i32, ((idx >> 4) & 15) as i32, true);
                }
            }
        }
        Some(packed)
    }

    /// Noticing the chunks that loaded, and packing as many as there is time for.
    fn pack_tick(&mut self, game: &Game<'_>, changes: Vec<(i32, i32, i32, bool)>) {
        if self.ids.is_none() {
            self.ids = Some(Self::pack_ids(game));
        }
        // `onBlockChange`: the blocks that were set go into the packed chunks they belong to.
        let mut by_chunk: std::collections::HashMap<(i32, i32), Vec<(i32, i32, i32, bool)>> = std::collections::HashMap::new();
        for change in changes {
            by_chunk.entry((change.0 >> 4, change.2 >> 4)).or_default().push(change);
        }
        for ((cx, cz), list) in by_chunk {
            self.npf.update_blocks(cx, cz, &list);
        }
        let revision = game.world.chunks.membership_revision();
        if revision != self.pack_revision {
            self.pack_revision = revision;
            let loaded: HashSet<(i32, i32)> = game.world.chunks.iter().map(|(k, _)| *k).collect();
            for k in &loaded {
                if self.pack_seen.insert(*k) {
                    self.pack_queue.push_back(*k);
                }
            }
            self.pack_seen.retain(|k| loaded.contains(k));
        }
        // A chunk that was sent again keeps its place in the chunk set, so the chunks around the player are packed again now and
        // then.
        if game.ticks.saturating_sub(self.last_repack) > 20 * 5 {
            self.last_repack = game.ticks;
            if let Some(p) = game.player.as_ref() {
                let (px, pz) = ((p.position.x.floor() as i32) >> 4, (p.position.z.floor() as i32) >> 4);
                for x in px - 8..=px + 8 {
                    for z in pz - 8..=pz + 8 {
                        if game.world.chunks.contains_key(&(x, z)) && !self.pack_queue.contains(&(x, z)) {
                            self.pack_queue.push_back((x, z));
                        }
                    }
                }
            }
        }
        let started = Instant::now();
        while let Some((cx, cz)) = self.pack_queue.pop_front() {
            if let Some(chunk) = self.pack_chunk(game, cx, cz) {
                self.npf.insert_chunk(cx, cz, chunk);
            }
            if started.elapsed().as_micros() > 4000 {
                break;
            }
        }
    }

    /// `tickInventoryTransactions`.
    fn tick_inventory_transactions(&mut self, game: &mut Game<'_>, settings: &super::baritone_settings::Settings) {
        if self.inv_countdown <= 0 {
            if let Some(slot) = self.inv_queue.pop_front() {
                super::baritone_inventory::pickup_click(game, slot);
                self.inv_countdown = settings.int("ticksBetweenInventoryMoves");
            }
        }
        if self.inv_countdown > 0 {
            self.inv_countdown -= 1;
        }
    }

    /// `trySwapElytra`: with elytraAutoSwap, a worn elytra that is nearly gone is traded for one in the inventory that is not.
    fn try_swap_elytra(&mut self, game: &Game<'_>, settings: &super::baritone_settings::Settings) {
        if !settings.flag("elytraAutoSwap") || !self.inv_queue.is_empty() {
            return;
        }
        let minimum = settings.int("elytraMinimumDurability");
        let left = |s: &horizon_client::items::ItemStack| game.res.items().get(&s.item).map_or(0, |i| i.max_damage as i32) - s.damage as i32;
        let worn_out = game.armor[1].as_ref().is_some_and(|s| &*s.item == "minecraft:elytra" && left(s) <= minimum);
        if !worn_out {
            return;
        }
        let good = (0..36).find(|&i| game.inventory.get(i).and_then(Option::as_ref).is_some_and(|s| s.count > 0 && &*s.item == "minecraft:elytra" && left(s) > minimum));
        if let Some(slot) = good {
            // The chest slot of the armor is 501 in the game's numbering (6 in the menu's).
            self.inv_queue.extend([slot, 501, slot]);
        }
    }

    /// `repackChunks`: queue every loaded chunk within 40 of the player.
    pub fn repack_chunks(&mut self, game: &Game<'_>) {
        let Some(p) = game.player.as_ref() else { return };
        let (px, pz) = ((p.position.x.floor() as i32) >> 4, (p.position.z.floor() as i32) >> 4);
        for x in px - 40..=px + 40 {
            for z in pz - 40..=pz + 40 {
                if game.world.chunks.contains_key(&(x, z)) && !self.pack_queue.contains(&(x, z)) {
                    self.pack_queue.push_back((x, z));
                }
            }
        }
    }

    // ---- the tick ----

    /// `getAttachedFirework`: the ticks the rocket boosting the player has existed, if there is one.
    fn attached_firework_age(&mut self, game: &Game<'_>) -> Option<i32> {
        let attached = rocket_attached(game);
        if !attached {
            self.attached_since = None;
            return None;
        }
        let since = *self.attached_since.get_or_insert(game.ticks);
        Some(game.ticks.saturating_sub(since) as i32)
    }

    /// `onTick`: the part that runs whenever the process is in control.
    pub fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, flags: ProcessFlags) {
        self.pack_tick(game, super::baritone::take_block_changes());
        self.tick_inventory_transactions(game, &core.settings);
        let cull_after = (core.settings.int("elytraTimeBetweenCacheCullSecs") * 20) as u64;
        let cull_distance = core.settings.int("elytraCacheCullDistance") as i32;
        let setback_delay = core.settings.int("elytraFireworkSetbackUseDelay") as i32;
        let behavior = self;
        core.drive(game, |_pathing, ctx, settings| {
            let snap = PlayerSnapshot::take(ctx);
            // onReceivePacket: the server set the player's position.
            if SET_BACK.with(Cell::take) {
                behavior.remaining_set_back_ticks = setback_delay;
            }

            behavior.poll_search(ctx, &snap, flags, settings);

            // Certified mojang employee incident.
            if behavior.remaining_firework_ticks > 0 {
                behavior.remaining_firework_ticks -= 1;
            }
            if behavior.remaining_set_back_ticks > 0 {
                behavior.remaining_set_back_ticks -= 1;
            }
            if ctx.with_game(|g| rocket_attached(g).then_some(0)).is_none() {
                behavior.minimum_boost_ticks = 0;
            }

            // Reset rendered elements.
            behavior.clear_lines.clear();
            behavior.blocked_lines.clear();
            behavior.visible_path.clear();
            behavior.simulation_line = None;
            behavior.aim_pos = None;

            if !behavior.path_manager.path().is_empty() {
                let env = game_env!(behavior, ctx, &snap, flags, settings);
                let job = behavior.path_manager.tick(&env);
                let (clear, blocked) = env.take_lines();
                drop(env);
                behavior.clear_lines.extend(clear);
                behavior.blocked_lines.extend(blocked);
                if let Some(job) = job {
                    behavior.start(ctx, &snap, job);
                }
                let near = behavior.path_manager.near();
                let path = behavior.path_manager.path();
                behavior.visible_path = path.blocks()[near.saturating_sub(30)..(near + 100).min(path.len())].to_vec();
            }
            // The chunks far from the player are forgotten now and then.
            if snap.tick.saturating_sub(behavior.time_last_cache_cull) > cull_after {
                behavior.npf.queue_cache_culling(snap.feet[0] >> 4, snap.feet[2] >> 4, cull_distance);
                behavior.time_last_cache_cull = snap.tick;
            }
        });
    }

    /// `tick`: called when the process is in control and the player is flying.
    pub fn tick(&mut self, core: &mut Core, game: &mut Game<'_>, flags: ProcessFlags) {
        if self.path_manager.path().is_empty() {
            return;
        }
        self.try_swap_elytra(game, &core.settings);
        self.landing_mode = flags.landing;
        let age = self.attached_firework_age(game);
        let sensitivity = f64::from(game.settings.sensitivity);
        let settings = &core.settings;
        let flight = FlightSettings {
            minimum_avoidance: settings.float("elytraMinimumAvoidance"),
            pitch_range: settings.int("elytraPitchRange") as i32,
            simulation_ticks: settings.int("elytraSimulationTicks") as i32,
            render_hitbox_raytraces: settings.flag("elytraRenderHitboxRaytraces"),
        };
        let known = core.look.effective_rotation(&core.settings).unwrap_or(Rotation::new(game.yaw, game.pitch));
        let (yaw, pitch) = (known.yaw, known.pitch);
        let aim_proto = TickableAim::new(core.aim.fork(), Rotation::new(yaw, pitch), AimSettings::of(settings, sensitivity));
        let behavior = self;
        let mut solution: Option<Solution> = None;
        let mut chosen: Option<(Rotation, bool)> = None;
        core.drive(game, |_pathing, ctx, settings| {
            let snap = PlayerSnapshot::take(ctx);
            let env = game_env!(behavior, ctx, &snap, flags, settings);

            let solver_context = SolverContext {
                path: behavior.path_manager.path().shared(),
                player_near: behavior.path_manager.near(),
                start: snap.position,
                motion: snap.motion,
                bounding_box: snap.bounding_box,
                ignore_lava: snap.in_lava,
                boost: FireworkBoost::new(age, behavior.minimum_boost_ticks),
                aim: aim_proto.clone(),
                player_rotations: snap.rotation,
            };
            let never = || false;
            let solver = Solver { world: &env, settings: &flight, landing_mode: behavior.landing_mode, cancelled: &never };
            let found = solver.solve_angles(&solver_context);
            let (clear, blocked) = env.take_lines();
            drop(env);
            behavior.clear_lines.extend(clear);
            behavior.blocked_lines.extend(blocked);

            if behavior.deployed_firework_last_tick {
                behavior.next_tick_boost_counter[usize::from(solver_context.boost.is_boosted())] += 1;
                behavior.deployed_firework_last_tick = false;
            }
            if snap.in_lava {
                ctx.set_input_force_state(super::baritone_movement_state::Input::Jump, true);
            }
            match found {
                None => behavior.verbose("no solution"),
                Some(s) => {
                    ctx.update_look_target(s.rotation, false);
                    if !s.solved_pitch {
                        behavior.verbose("no pitch solution, probably gonna crash in a few ticks LOL!!!");
                    } else {
                        let to = s.going_to.expect("a solved pitch has somewhere to go");
                        behavior.aim_pos = Some([to.x.floor() as i32, to.y.floor() as i32, to.z.floor() as i32]);
                        behavior.simulation_line = s.simulation_line.clone();
                        chosen = Some((s.rotation, s.force_use_firework || snap.in_lava));
                    }
                    solution = Some(s);
                }
            }
        });
        if let (Some(s), Some((_, force))) = (&solution, chosen) {
            let going_to = s.going_to.expect("solved");
            behavior.use_fireworks(core, game, s.context.start, going_to, s.context.boost.is_boosted(), force);
        }
    }

    /// `tickUseFireworks`.
    fn use_fireworks(&mut self, core: &mut Core, game: &mut Game<'_>, start: V3, going_to: V3, is_boosted: bool, force: bool) {
        if self.remaining_set_back_ticks > 0 {
            let n = self.remaining_set_back_ticks;
            self.verbose(&format!("waiting for elytraFireworkSetbackUseDelay: {n}"));
            return;
        }
        if self.landing_mode {
            return;
        }
        let Some(p) = game.player.as_ref() else { return };
        let (player_y, motion) = (p.position.y, V3::new(p.velocity.x, p.velocity.y, p.velocity.z));
        if self.remaining_firework_ticks > 0 || !wants_firework(start, going_to, is_boosted, force, core.settings.flag("elytraConserveFireworks"), core.settings.float("elytraFireworkSpeed"), player_y, motion) {
            return;
        }
        // Boosting fireworks have a flight duration and no explosions; the Java asks for those and then for any firework, which
        // is the same test.
        let allow = core.settings.flag("allowInventory");
        if !core.inventory.throwaway(game, &core.settings, true, &|s| firework_flight(s).is_some(), allow) {
            self.direct("no fireworks");
            return;
        }
        let slot = game.selected;
        self.verbose(&format!("attempting to use firework{}", if force { " (forced)" } else { "" }));
        game.bar_use();
        let flight = game.inventory.get(slot).and_then(Option::as_ref).and_then(firework_flight).unwrap_or(1);
        self.minimum_boost_ticks = 10 * (1 + i32::from(flight));
        self.remaining_firework_ticks = 10;
        self.deployed_firework_last_tick = true;
    }

    /// `destroy`: the behaviour only stops. The pathfinder context is shared with the next behaviour (a landing is a new
    /// behaviour on the same context), so a search is not cancelled here: a flag raised with no search running would stay up
    /// and fail the next one. A search still running ends by itself and its answer is dropped. The context is cancelled and
    /// destroyed by the process when it lets go of it.
    pub fn destroy(&mut self) {
        self.running = None;
    }
}

/// Whether a rocket is boosting the player (`FireworkRocketEntity.getBoostedEntity() == player`).
fn rocket_attached(game: &Game<'_>) -> bool {
    game.rockets.iter().any(|r| r.attached)
}

/// `getFireworkBoost`: the flight duration of a firework with no explosions, else None.
pub fn firework_flight(stack: &horizon_client::items::ItemStack) -> Option<u8> {
    if &*stack.item != "minecraft:firework_rocket" {
        return None;
    }
    let (flight, explosions) = stack.data().and_then(|d| d.fireworks.clone()).unwrap_or((1, Vec::new()));
    explosions.is_empty().then_some(flight)
}

/// How many boosting fireworks the inventory holds.
pub fn firework_count(game: &Game<'_>) -> u32 {
    game.inventory.iter().flatten().filter(|s| firework_flight(s).is_some()).map(|s| s.count).sum()
}
