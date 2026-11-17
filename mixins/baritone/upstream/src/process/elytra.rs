// baritone.process.ElytraProcess, written from the Java: fly to a goal on an elytra. Walk off a ledge (or glide from where the
// player is), let ElytraBehavior steer from path node to path node and fire rockets, and when the path is done look for a safe
// spot to land and come down on it. The path is searched by the Nether pathfinder (baritone_elytra_*), in the nether with the
// seed's terrain predicted and everywhere else through the chunks the game has loaded.
use super::baritone::Core;
use super::baritone_calculation_context::{CalculationContext, CostHooks};
use super::baritone_costs::COST_INF;
use super::baritone_elytra_behavior::{dimension_extent, firework_count, ElytraBehavior, ProcessFlags};
use super::baritone_elytra_context::NetherPathfinderContext;
use super::baritone_elytra_pathfinder::Dimension;
use super::baritone_elytra_path_manager::Note;
use super::baritone_elytra_region::RegionSource;
use super::baritone_goals::{rc, GoalRef, GoalYLevel};
use super::baritone_gworld as gw;
use super::baritone_movement::Kind;
use super::baritone_movement_state::Input;
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_rotation::{calc_rotation_from_vec3d, Rotation};
use super::baritone_world_data::FileStore;
use super::Game;
use super::baritone_block_state::BlockState;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Pos = [i32; 3];

const SHORT_LANDING_COLUMN_HEIGHT: i32 = 15;
const LONG_LANDING_COLUMN_HEIGHT: i32 = 39;
/// Half a tick.
const LANDING_SEARCH_BUDGET: Duration = Duration::from_millis(25);
const AUTO_JUMP_FAILURE_MSG: &str = "Failed to compute a walking path to a spot to jump off from. Consider starting from a higher location, near an overhang. Or, you can disable elytraAutoJump and just manually begin gliding.";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    LocateJump,
    Pause,
    GetToJump,
    StartFlying,
    Flying,
    Landing,
}

impl State {
    fn description(self) -> &'static str {
        match self {
            State::LocateJump => "Finding spot to jump off",
            State::Pause => "Waiting for elytra path",
            State::GetToJump => "Walking to takeoff",
            State::StartFlying => "Begin flying",
            State::Flying => "Flying",
            State::Landing => "Landing",
        }
    }
}

/// `WalkOffCalculationContext`: the walk to the jump-off ledge may fall into lava and from any height, and may not place or break.
struct WalkOffHooks;

impl CostHooks for WalkOffHooks {
    fn configure(&self, ctx: &mut CalculationContext) {
        ctx.allow_fall_into_lava = true;
        ctx.min_fall_height = 8;
        ctx.max_fall_height_no_water = 10000;
    }

    fn cost_of_placing_at(&self, _base: &CalculationContext, _x: i32, _y: i32, _z: i32, _current: &BlockState) -> Option<f64> {
        Some(COST_INF)
    }

    fn break_cost_multiplier_at(&self, _base: &CalculationContext, _x: i32, _y: i32, _z: i32, _current: &BlockState) -> Option<f64> {
        Some(COST_INF)
    }

    fn place_bucket_cost(&self, _base: &CalculationContext) -> Option<f64> {
        Some(COST_INF)
    }
}

fn block_at<'a>(game: &'a Game<'_>, p: Pos) -> &'a str {
    gw::bare(&game.res.facts().get(game.world.block(p[0], p[1], p[2])).name)
}

fn is_air_block(name: &str) -> bool {
    matches!(name, "air" | "cave_air" | "void_air")
}

/// The chest slot: whether it holds an elytra, and the durability it has left.
fn chest(game: &Game<'_>) -> Option<(bool, i32)> {
    game.armor[1].as_ref().map(|s| {
        let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage as i32);
        (&*s.item == "minecraft:elytra", max - s.damage as i32)
    })
}

fn chunk_loaded(game: &Game<'_>, p: Pos) -> bool {
    game.world.chunks.contains_key(&(p[0] >> 4, p[2] >> 4))
}

fn dist_sq(a: Pos, b: Pos) -> i64 {
    let (dx, dy, dz) = (i64::from(a[0] - b[0]), i64::from(a[1] - b[1]), i64::from(a[2] - b[2]));
    dx * dx + dy * dy + dz * dz
}

/// The block states of the landing search, from the settings it asks for: `isSafeBlock`.
fn is_safe_block(name: &str, allow_fortress: bool) -> bool {
    matches!(
        name,
        "netherrack" | "gravel" | "soul_sand" | "soul_soil" | "stone" | "deepslate" | "grass_block" | "sand" | "red_sand" | "terracotta" | "snow" | "ice" | "mycelium" | "podzol"
            | "dark_oak_leaves" | "jungle_leaves" | "end_stone" | "bedrock" | "obsidian" | "cobblestone"
    ) || (name == "nether_bricks" && allow_fortress)
}

fn is_at_edge(game: &Game<'_>, p: Pos, fortress: bool) -> bool {
    let safe = |dx: i32, dz: i32| is_safe_block(block_at(game, [p[0] + dx, p[1], p[2] + dz]), fortress);
    !safe(0, -1) || !safe(0, 1) || !safe(1, 0) || !safe(-1, 0) || !safe(-1, -1) || !safe(1, -1) || !safe(-1, 1) || !safe(1, 1)
}

fn is_column_air(game: &Game<'_>, landing_spot: Pos, min_height: i32) -> bool {
    (landing_spot[1] + 1..=landing_spot[1] + min_height).all(|y| is_air_block(block_at(game, [landing_spot[0], y, landing_spot[2]])))
}

fn has_air_bubble(game: &Game<'_>, p: Pos) -> bool {
    // Half of the full width, rounded down, as we're counting blocks in each direction from the center.
    let radius = 4;
    for x in -radius..=radius {
        for y in -radius..=radius {
            for z in -radius..=radius {
                if !is_air_block(block_at(game, [p[0] + x, p[1] + y, p[2] + z])) {
                    return false;
                }
            }
        }
    }
    true
}

fn check_landing_spot(game: &Game<'_>, pos: Pos, checked: &mut HashSet<Pos>, fortress: bool) -> Option<Pos> {
    let mut cur = pos;
    while cur[1] >= horizon_client::chunk::MIN_Y {
        if !checked.insert(cur) {
            return None;
        }
        let name = block_at(game, cur);
        if is_safe_block(name, fortress) {
            return if !is_at_edge(game, cur, fortress) { Some(cur) } else { None };
        } else if name != "air" {
            return None;
        }
        cur[1] -= 1;
    }
    None // void
}

/// `LandingSearchState`: a best-first search outwards from the destination (or the player) for a spot with a safe floor and air above.
struct LandingSearch {
    origin: Pos,
    use_heightmap: bool,
    target: Pos,
    queue: BinaryHeap<Reverse<(i64, i32, Pos)>>,
    visited: HashSet<Pos>,
    checked: HashSet<Pos>,
    exhausted: bool,
}

impl LandingSearch {
    fn new(game: &Game<'_>, origin: Pos, dest: Pos, use_heightmap: bool) -> Self {
        let target = if chunk_loaded(game, dest) { dest } else { origin };
        let mut s = Self { origin, use_heightmap, target, queue: BinaryHeap::new(), visited: HashSet::new(), checked: HashSet::new(), exhausted: false };
        s.push(target);
        s
    }

    fn key(&self, p: Pos) -> Reverse<(i64, i32, Pos)> {
        let (dx, dz) = (i64::from(p[0] - self.target[0]), i64::from(p[2] - self.target[2]));
        Reverse((dx * dx + dz * dz, -p[1], p))
    }

    fn push(&mut self, p: Pos) {
        let key = self.key(p);
        self.queue.push(key);
    }

    fn add(&mut self, p: Pos) {
        if self.visited.insert(p) {
            self.push(p);
        }
    }

    fn is_compatible(&self, start: Pos, use_heightmap: bool) -> bool {
        // Restart if we've moved more than a chunk so the priority adjusts and newly loaded chunks get revisited.
        self.use_heightmap == use_heightmap && dist_sq(self.origin, start) <= 16 * 16
    }

    fn update_start(&mut self, start: Pos) {
        self.add(start);
    }
}

pub struct ElytraProcess {
    pub state: State,
    going_to_landing_spot: bool,
    landing_spot: Option<Pos>,
    reached_goal: bool,
    goal: Option<GoalRef>,
    behavior: Option<ElytraBehavior>,
    npf: Option<Arc<NetherPathfinderContext>>,
    predicting_terrain: bool,
    allow_tight: bool,
    allow_above_build_limit: bool,
    allow_above_roof: bool,
    landing_column_height: i32,
    bad_landing_spots: HashSet<Pos>,
    landing_search: Option<LandingSearch>,
    dimension: String,
}

impl ElytraProcess {
    pub fn new() -> Self {
        Self {
            state: State::StartFlying,
            going_to_landing_spot: false,
            landing_spot: None,
            reached_goal: false,
            goal: None,
            behavior: None,
            npf: None,
            predicting_terrain: false,
            allow_tight: false,
            allow_above_build_limit: false,
            allow_above_roof: false,
            landing_column_height: SHORT_LANDING_COLUMN_HEIGHT,
            bad_landing_spots: HashSet::new(),
            landing_search: None,
            dimension: String::new(),
        }
    }

    /// `isLoaded`: there is no native library to load here.
    #[allow(dead_code)]
    pub fn is_loaded(&self) -> bool {
        true
    }

    /// The flight, for the renderer.
    pub fn behavior(&self) -> Option<&ElytraBehavior> {
        self.behavior.as_ref()
    }

    pub fn current_destination(&self) -> Option<Pos> {
        self.behavior.as_ref().map(|b| b.destination)
    }

    #[allow(dead_code)]
    pub fn path(&self) -> Vec<Pos> {
        self.behavior.as_ref().map_or_else(Vec::new, |b| b.path_manager.path().blocks().to_vec())
    }

    /// `onLostControl(destroyNpf)`.
    fn lost(&mut self, destroy_npf: bool) {
        self.state = State::StartFlying;
        self.going_to_landing_spot = false;
        self.landing_spot = None;
        self.landing_search = None;
        self.reached_goal = false;
        self.goal = None;
        if let Some(mut b) = self.behavior.take() {
            b.destroy();
        }
        if destroy_npf {
            if let Some(npf) = self.npf.take() {
                npf.cancel();
                npf.destroy();
            }
        }
    }

    fn say_notes(&mut self, core: &Core, game: &mut Game<'_>) {
        let Some(b) = self.behavior.as_mut() else { return };
        let verbose = core.settings.flag("elytraChatSpam");
        for note in b.take_notes() {
            match note {
                Note::Direct(s) => core.say(game, &s),
                Note::Verbose(s) if verbose => core.say(game, &s),
                Note::Verbose(_) => {}
            }
        }
    }

    fn npf_context(&mut self, core: &Core, game: &Game<'_>, dimension: &str) -> Result<Arc<NetherPathfinderContext>, String> {
        if let Some(npf) = &self.npf {
            return Ok(npf.clone());
        }
        let s = &core.settings;
        let (min_y, height) = dimension_extent(dimension);
        let kind = match dimension {
            "minecraft:the_nether" => Dimension::Nether,
            "minecraft:the_end" => Dimension::End,
            _ => Dimension::Overworld,
        };
        let cache: Option<Arc<dyn RegionSource>> = if s.flag("elytraUseCache") {
            core.world_data.current.as_ref().map(|d| Arc::new(FileStore { dir: d.dir.clone() }) as Arc<dyn RegionSource>)
        } else {
            None
        };
        let _ = game;
        let npf = Arc::new(NetherPathfinderContext::new(s.long("elytraNetherSeed"), cache, kind, min_y, height, s.flag("elytraAllowAboveRoof"))?);
        self.npf = Some(npf.clone());
        Ok(npf)
    }

    fn in_nether(&self) -> bool {
        self.dimension == "minecraft:the_nether"
    }

    /// `isSupportedPos`.
    fn is_supported_pos(&self, core: &Core, pos: Pos, dimension: &str) -> bool {
        let nether = dimension == "minecraft:the_nether";
        let (min_y, height) = dimension_extent(dimension);
        let max_y = if nether && !core.settings.flag("elytraAllowAboveRoof") { 127 } else { (min_y + 384).min(height + min_y) };
        let above_roof = core.settings.flag("elytraAllowAboveRoof");
        let above_build = core.settings.flag("elytraAllowAboveBuildLimit");
        let enforce_max_y = if nether { !(above_roof && above_build) } else { !above_build };
        if pos[1] < min_y {
            return false;
        }
        !enforce_max_y || pos[1] < max_y
    }

    /// `pathTo(Goal)`: an error is what the Java throws as an IllegalArgumentException.
    pub fn path_to(&mut self, core: &mut Core, game: &mut Game<'_>, goal: &GoalRef) -> Result<(), String> {
        let Some(target) = goal.elytra_target() else { return Err("The goal must be a GoalXZ or GoalBlock".into()) };
        let dimension = game.native_player().map_or("minecraft:overworld", |p| p.dimension).to_owned();
        if !self.is_supported_pos(core, target, &dimension) {
            return Err("The goal must be within bounds to use elytra flight.".into());
        }
        if let Some(feet) = core.player_feet(game) {
            if !self.is_supported_pos(core, feet, &dimension) {
                return Err("The player must be within bounds to use elytra flight.".into());
            }
        }
        self.path_to0(core, game, target, false);
        Ok(())
    }

    fn path_to0(&mut self, core: &mut Core, game: &mut Game<'_>, destination: Pos, append_destination: bool) {
        if game.player.is_none() {
            return;
        }
        self.lost(false);
        let dimension = game.native_player().map_or("minecraft:overworld", |p| p.dimension).to_owned();
        self.dimension = dimension.clone();
        let s = &core.settings;
        self.predicting_terrain = self.in_nether() && s.flag("elytraPredictTerrain");
        self.allow_tight = s.flag("elytraAllowTightSpaces");
        self.allow_above_build_limit = s.flag("elytraAllowAboveBuildLimit");
        self.allow_above_roof = s.flag("elytraAllowAboveRoof");
        let npf = match self.npf_context(core, game, &dimension) {
            Ok(n) => n,
            Err(e) => {
                core.say_red(game, &e);
                return;
            }
        };
        let in_nether = self.in_nether();
        let mut behavior = ElytraBehavior::new(core, game, npf, destination, append_destination, in_nether, &dimension);
        behavior.repack_chunks(game);
        self.behavior = Some(behavior);
        let flags = self.flags();
        if let Some(mut b) = self.behavior.take() {
            let settings = core.settings.clone();
            core.drive(game, |_p, ctx, _s| {
                let snap = super::baritone_elytra_behavior::PlayerSnapshot::take(ctx);
                b.path_to(ctx, &snap, &settings, flags);
            });
            self.behavior = Some(b);
        }
    }

    /// `resetState`: start again towards the same destination.
    pub fn reset_state(&mut self, core: &mut Core, game: &mut Game<'_>) -> Result<(), String> {
        let destination = self.current_destination();
        self.lost(true);
        if let Some(d) = destination {
            let dimension = game.native_player().map_or("minecraft:overworld", |p| p.dimension).to_owned();
            if !self.is_supported_pos(core, d, &dimension) {
                return Err("The goal must be within bounds to use elytra flight.".into());
            }
            self.path_to0(core, game, d, false);
            self.repack(game);
        }
        Ok(())
    }

    /// `repackChunks`.
    pub fn repack(&mut self, game: &Game<'_>) {
        if let Some(b) = self.behavior.as_mut() {
            b.repack_chunks(game);
        }
    }

    fn flags(&self) -> ProcessFlags {
        ProcessFlags { landing: self.state == State::Landing, get_to_jump: self.state == State::GetToJump }
    }

    /// `landingSpotIsBad`.
    fn landing_spot_is_bad(&mut self, end: Pos) {
        self.bad_landing_spots.insert(end);
        self.going_to_landing_spot = false;
        self.landing_spot = None;
        self.landing_search = None;
        self.state = State::Flying;
    }

    /// `shouldLandForSafety`.
    fn should_land_for_safety(&self, core: &Core, game: &Game<'_>) -> bool {
        let chest_ok = chest(game).is_some_and(|(is_elytra, left)| is_elytra && left >= core.settings.int("elytraMinimumDurability"));
        if !chest_ok {
            // ElytraBehavior replaces when durability <= minimumDurability, so if durability < minimumDurability then we can
            // reasonably assume that the elytra will soon be broken without replacement.
            return true;
        }
        i64::from(firework_count(game)) <= i64::from(core.settings.int("elytraMinFireworksBeforeLanding"))
    }

    /// `findSafeLandingSpot`.
    fn find_safe_landing_spot(&mut self, core: &Core, game: &Game<'_>, start: Pos) -> Option<Pos> {
        let use_heightmap = game.player.as_ref().is_some_and(|p| p.position.y > f64::from(game.rain_height(start[0], start[2])));
        let dest = self.behavior.as_ref().map(|b| b.destination)?;
        let compatible = self.landing_search.as_ref().is_some_and(|s| s.is_compatible(start, use_heightmap));
        if !compatible {
            self.landing_search = Some(LandingSearch::new(game, start, dest, use_heightmap));
        } else if let Some(s) = self.landing_search.as_mut() {
            s.update_start(start);
        }
        let spot = self.advance_landing(core, game, start);
        if spot.is_some() || self.landing_search.as_ref().is_some_and(|s| s.exhausted) {
            self.landing_search = None;
        }
        spot
    }

    fn in_bounds(&self, core: &Core, game: &Game<'_>, pos: Pos) -> bool {
        let dimension = game.native_player().map_or("minecraft:overworld", |p| p.dimension);
        let (min_y, height) = dimension_extent(dimension);
        let max_y = if dimension == "minecraft:the_nether" && !core.settings.flag("elytraAllowAboveRoof") { 127 } else { (min_y + 384).min(height + min_y) };
        pos[1] >= min_y && pos[1] < max_y
    }

    fn advance_landing(&mut self, core: &Core, game: &Game<'_>, player_feet: Pos) -> Option<Pos> {
        let deadline = Instant::now() + LANDING_SEARCH_BUDGET;
        let fortress = core.settings.flag("elytraAllowLandOnNetherFortress");
        loop {
            let Reverse((_, _, q_pos)) = {
                let search = self.landing_search.as_mut()?;
                if search.queue.is_empty() {
                    search.exhausted = true;
                    return None;
                }
                if Instant::now() >= deadline {
                    return None;
                }
                search.queue.pop()?
            };
            if !chunk_loaded(game, q_pos) {
                continue;
            }
            let use_heightmap = self.landing_search.as_ref()?.use_heightmap;
            let found = if use_heightmap { self.advance_heightmap(core, game, q_pos, player_feet, fortress) } else { self.advance_underground(core, game, q_pos, fortress) };
            if found.is_some() {
                return found;
            }
        }
    }

    fn advance_underground(&mut self, core: &Core, game: &Game<'_>, pos: Pos, fortress: bool) -> Option<Pos> {
        if self.in_bounds(core, game, pos) && block_at(game, pos) == "air" {
            let search = self.landing_search.as_mut()?;
            let actual = check_landing_spot(game, pos, &mut search.checked, fortress);
            if let Some(actual) = actual {
                self.landing_column_height = SHORT_LANDING_COLUMN_HEIGHT;
                let above = [actual[0], actual[1] + self.landing_column_height, actual[2]];
                if is_column_air(game, actual, self.landing_column_height) && has_air_bubble(game, above) && !self.bad_landing_spots.contains(&above) {
                    return Some(above);
                }
            }
            let search = self.landing_search.as_mut()?;
            for d in [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0], [0, 1, 0], [0, -1, 0]] {
                search.add([pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]);
            }
        }
        None
    }

    fn advance_heightmap(&mut self, core: &Core, game: &Game<'_>, q_pos: Pos, player_feet: Pos, fortress: bool) -> Option<Pos> {
        let height = game.rain_height(q_pos[0], q_pos[2]);
        let pos = [q_pos[0], height + 1, q_pos[2]];
        if self.in_bounds(core, game, pos) && block_at(game, pos) == "air" {
            let search = self.landing_search.as_mut()?;
            let actual = check_landing_spot(game, pos, &mut search.checked, fortress);
            if let Some(actual) = actual {
                self.landing_column_height = if player_feet[1] - actual[1] < LONG_LANDING_COLUMN_HEIGHT { SHORT_LANDING_COLUMN_HEIGHT } else { LONG_LANDING_COLUMN_HEIGHT };
                let above = [actual[0], actual[1] + self.landing_column_height, actual[2]];
                if has_air_bubble(game, above) && !self.bad_landing_spots.contains(&above) {
                    return Some(above);
                }
            }
            let search = self.landing_search.as_mut()?;
            for d in [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]] {
                search.add([pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]);
            }
        }
        None
    }

    /// `pathTo0(landingSpot, true)`.
    fn path_to_landing(&mut self, core: &mut Core, game: &mut Game<'_>, spot: Pos) {
        self.path_to0(core, game, spot, true);
    }
}

impl Process for ElytraProcess {
    fn name(&self) -> &'static str {
        "elytra"
    }

    fn display(&self) -> String {
        format!("Elytra - {}", self.state.description())
    }

    fn is_active(&self) -> bool {
        self.behavior.is_some()
    }

    fn priority(&self) -> f64 {
        0.0 // higher priority than CustomGoalProcess
    }

    fn refresh(&mut self, _core: &mut Core, game: &mut Game<'_>) {
        // Exiting the world, or changing dimension, just destroys the flight.
        let dimension = game.native_player().map(|p| p.dimension);
        if self.behavior.is_some() && (game.player.is_none() || dimension.is_some_and(|d| d != self.dimension)) {
            self.lost(true);
        }
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, safe_to_cancel: bool) -> PathingCommand {
        if self.behavior.is_none() {
            return PathingCommand::Defer;
        }
        let setting = |s: &str| core.settings.flag(s);
        let in_nether = self.in_nether();
        let mut changed: Option<&str> = None;
        if self.npf.as_ref().is_some_and(|n| core.settings.long("elytraNetherSeed") != n.seed()) {
            changed = Some("Nether seed changed, recalculating path");
        } else if self.predicting_terrain != setting("elytraPredictTerrain") && in_nether {
            self.predicting_terrain = setting("elytraPredictTerrain");
            changed = Some("elytraPredictTerrain setting changed, recalculating path from scratch");
        } else if self.allow_tight != setting("elytraAllowTightSpaces") {
            self.allow_tight = setting("elytraAllowTightSpaces");
            changed = Some("elytraAllowTightSpaces setting changed, recalculating path from scratch");
        } else if self.allow_above_build_limit != setting("elytraAllowAboveBuildLimit") {
            self.allow_above_build_limit = setting("elytraAllowAboveBuildLimit");
            changed = Some("elytraAllowAboveBuildLimit setting changed, recalculating path from scratch");
        } else if self.allow_above_roof != setting("elytraAllowAboveRoof") && in_nether {
            self.allow_above_roof = setting("elytraAllowAboveRoof");
            changed = Some("elytraAllowAboveRoof setting changed, recalculating path from scratch");
        }
        if let Some(message) = changed {
            core.say(game, message);
            if let Err(e) = self.reset_state(core, game) {
                core.say_red(game, &e);
                return PathingCommand::CancelAndClearGoal;
            }
            if self.behavior.is_none() {
                return PathingCommand::CancelAndClearGoal;
            }
        }

        let flags = self.flags();
        if let Some(b) = self.behavior.as_mut() {
            b.on_tick(core, game, flags);
        }
        self.say_notes(core, game);

        if let Some(spot) = self.behavior.as_mut().and_then(|b| b.bad_landing_spot.take()) {
            self.landing_spot_is_bad(spot);
        }
        if let Some(done) = self.behavior.as_mut().and_then(|b| b.take_destination_result()) {
            if self.state == State::Pause {
                if done {
                    self.state = State::GetToJump;
                } else {
                    self.lost(true);
                    return PathingCommand::CancelAndClearGoal;
                }
            }
        }

        if calc_failed {
            self.lost(true);
            core.say(game, AUTO_JUMP_FAILURE_MSG);
            return PathingCommand::CancelAndClearGoal;
        }

        let (flying, on_ground, motion) = match game.player.as_ref() {
            Some(p) => (p.fall_flying, p.ground.on_ground, [p.velocity.x, p.velocity.y, p.velocity.z]),
            None => return PathingCommand::Defer,
        };
        let position = gw::player_pos(game).unwrap_or([0.0; 3]);
        let feet = core.player_feet(game).unwrap_or([0, 0, 0]);

        let mut safety_landing = false;
        if flying && self.should_land_for_safety(core, game) {
            if core.settings.flag("elytraAllowEmergencyLand") {
                core.say(game, "Emergency landing - almost out of elytra durability or fireworks");
                safety_landing = true;
            } else {
                core.say(game, "almost out of elytra durability or fireworks, but I'm going to continue since elytraAllowEmergencyLand is false");
            }
        }
        let path_complete = self.behavior.as_ref().is_some_and(|b| b.path_manager.is_complete());
        if flying && self.state != State::Landing && (path_complete || safety_landing) {
            let last = self.behavior.as_ref().and_then(|b| b.path_manager.path().last());
            let center = |p: Pos| [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5];
            let dist_to = |p: Pos| {
                let c = center(p);
                (position[0] - c[0]).powi(2) + (position[1] - c[1]).powi(2) + (position[2] - c[2]).powi(2)
            };
            if let Some(last) = last {
                if (dist_to(last) < 48.0 * 48.0 || safety_landing) && (!self.going_to_landing_spot || (safety_landing && self.landing_spot.is_none())) {
                    if self.landing_search.is_none() {
                        core.say(game, "Path complete, searching for safe landing spot...");
                    }
                    // If this fails we will just keep orbiting the last node until we run out of rockets or the user intervenes.
                    match self.find_safe_landing_spot(core, game, feet) {
                        Some(spot) => {
                            core.say(game, "Found potential landing spot.");
                            self.path_to_landing(core, game, spot);
                            self.landing_spot = Some(spot);
                            self.going_to_landing_spot = true;
                        }
                        None => self.going_to_landing_spot = false,
                    }
                }
            }
            let last = self.behavior.as_ref().and_then(|b| b.path_manager.path().last());
            if let Some(last) = last {
                if dist_to(last) < 1.0 {
                    if core.settings.flag("notificationOnPathComplete") && !self.reached_goal {
                        core.notify("Pathing complete", false);
                    }
                    if core.settings.flag("disconnectOnArrival") && !self.reached_goal {
                        // Don't be active when the user logs back in.
                        self.lost(true);
                        let _ = game.close_world();
                        return PathingCommand::CancelAndClearGoal;
                    }
                    self.reached_goal = true;
                    // We are goingToLandingSpot and we are in the last node of the path.
                    if self.going_to_landing_spot && self.landing_spot.is_some() {
                        self.state = State::Landing;
                        core.say(game, "Above the landing spot, landing...");
                    }
                }
            }
        }

        if self.state == State::Landing {
            let end = self.landing_spot.or_else(|| self.behavior.as_ref().and_then(|b| b.path_manager.path().last()));
            if let (true, Some(end)) = (flying, end) {
                let to = [f64::from(end[0]) + 0.5, position[1], f64::from(end[2]) + 0.5];
                let column = self.landing_column_height;
                let mut bad = false;
                core.drive(game, |_p, ctx, _s| {
                    use super::baritone_player_context::PlayerContext;
                    let rotation = calc_rotation_from_vec3d(position, to, ctx.player_rotations());
                    // This will be overwritten, probably, by the behavior tick.
                    ctx.update_look_target(Rotation::new(rotation.yaw, 0.0), false);
                    bad = position[1] < f64::from(end[1] - column);
                });
                if bad {
                    core.say(game, "bad landing spot, trying again...");
                    self.landing_spot_is_bad(end);
                }
            }
        }

        if flying {
            let flags = self.flags();
            self.goal = None;
            core.input.clear_all_keys();
            if let Some(mut b) = self.behavior.take() {
                b.tick(core, game, flags);
                self.behavior = Some(b);
            }
            self.say_notes(core, game);
            return PathingCommand::CancelAndClearGoal;
        } else if self.state == State::Landing {
            if motion[0].hypot(motion[2]) > 0.001 {
                core.say(game, "Landed, but still moving, waiting for velocity to die down... ");
                core.input.set_input_force_state(Input::Sneak, true);
                return PathingCommand::RequestPause;
            }
            core.say(game, "Done :)");
            core.input.clear_all_keys();
            self.lost(true);
            return PathingCommand::RequestPause;
        }

        if self.state == State::Flying || self.state == State::StartFlying {
            self.state = if on_ground && core.settings.flag("elytraAutoJump") { State::LocateJump } else { State::StartFlying };
        }

        if self.state == State::LocateJump {
            if self.should_land_for_safety(core, game) {
                core.say(game, "Not taking off, because elytra durability or fireworks are so low that I would immediately emergency land anyway.");
                self.lost(true);
                return PathingCommand::CancelAndClearGoal;
            }
            if self.goal.is_none() {
                self.goal = Some(rc(GoalYLevel(31)));
            }
            let goal = self.goal.clone().expect("set above");
            let from = match core.pathing.current.as_ref() {
                Some(executor) if Rc::ptr_eq(&executor.path.goal, &goal) => {
                    let fall = executor.path.movements.iter().find(|m| m.kind == Kind::Fall);
                    match fall {
                        Some(fall) => Ok(Some([(fall.src[0] + fall.dest[0]) / 2, (fall.src[1] + fall.dest[1]) / 2, (fall.src[2] + fall.dest[2]) / 2])),
                        None => Err(()),
                    }
                }
                _ => Ok(None),
            };
            match from {
                Err(()) => {
                    self.lost(true);
                    core.say(game, AUTO_JUMP_FAILURE_MSG);
                    return PathingCommand::CancelAndClearGoal;
                }
                Ok(Some(from)) => {
                    if let Some(mut b) = self.behavior.take() {
                        let settings = core.settings.clone();
                        let flags = self.flags();
                        core.drive(game, |_p, ctx, _s| {
                            let snap = super::baritone_elytra_behavior::PlayerSnapshot::take(ctx);
                            b.path_to_destination_from(ctx, &snap, from, flags, &settings);
                        });
                        self.behavior = Some(b);
                    }
                    self.state = State::Pause;
                }
                Ok(None) => {}
            }
            core.path_hooks = Some(Rc::new(WalkOffHooks));
            return PathingCommand::SetGoalAndPause(goal);
        }

        // yucky
        if self.state == State::Pause {
            return PathingCommand::RequestPause;
        }

        if self.state == State::GetToJump {
            let can_start_flying = motion[1] < -0.377
                && !safe_to_cancel
                && core.pathing.current.as_ref().is_some_and(|e| e.path.movements.get(e.position()).is_some_and(|m| m.kind == Kind::Fall));
            if can_start_flying {
                self.state = State::StartFlying;
            } else {
                return match self.goal.clone() {
                    Some(g) => PathingCommand::SetGoalAndPath(g),
                    None => PathingCommand::CancelAndClearGoal,
                };
            }
        }

        if self.state == State::StartFlying {
            if !safe_to_cancel {
                // owned
                core.drive(game, |pathing, ctx, _s| pathing.secret_internal_segment_cancel(ctx));
            }
            core.input.clear_all_keys();
            if motion[1] < -0.377 {
                core.input.set_input_force_state(Input::Jump, true);
            }
        }
        PathingCommand::CancelAndClearGoal
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {
        self.lost(true);
    }

    fn cancel(&mut self) {
        self.lost(true);
    }
}
