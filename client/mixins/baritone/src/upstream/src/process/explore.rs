// baritone.process.ExploreProcess, written from the Java: head for the closest chunks the cached world has no
// record of, ring by ring (by manhattan distance) from where exploring began, a goal for each of the nearest few
// and a render distance further out than the chunk itself. A chunk file lists, with #explorefilter, chunks
// already explored elsewhere (or, inverted, the only chunks worth exploring).
use super::baritone::Core;
use super::baritone_calculation_context::{CalculationContext, CostHooks};
use super::baritone_cached_world::CachedWorld;
use super::baritone_goals::{rc, y_heuristic, Goal, GoalComposite, GoalRef, GoalXZ};
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_utils::Pos;
use super::Game;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Explored,
    NotExplored,
    Unknown,
}

/// `JsonChunkFilter`: chunks listed in a file.
#[derive(Clone)]
struct JsonFilter {
    in_filter: HashSet<(i32, i32)>,
    positions: Vec<(i32, i32)>,
    /// If true, the list is the chunks that are NOT explored, if false the ones that ARE.
    invert: bool,
}

/// `BaritoneChunkCache.isAlreadyExplored`.
fn cache_status(cache: Option<&mut CachedWorld>, chunk_x: i32, chunk_z: i32) -> Status {
    let Some(cache) = cache else { return Status::Unknown };
    let (cx, cz) = (chunk_x << 4, chunk_z << 4);
    if cache.is_cached(cx, cz) {
        return Status::Explored;
    }
    if !cache.region_loaded(cx, cz) {
        // We still need to load regions from disk in order to decide properly.
        cache.try_load_from_disk(cx >> 9, cz >> 9);
        return Status::Unknown;
    }
    Status::NotExplored
}

impl JsonFilter {
    fn status(&self, chunk_x: i32, chunk_z: i32) -> Status {
        if self.in_filter.contains(&(chunk_x, chunk_z)) != self.invert {
            // Either it's on the list of explored chunks, or it's not on the list of unexplored chunks: either way,
            // we have it. Otherwise it depends on if baritone has cached it, so defer to that.
            Status::Explored
        } else {
            Status::Unknown
        }
    }

    fn count_remain(&self, mut cache: Option<&mut CachedWorld>, minimum: usize) -> usize {
        if !self.invert {
            // If invert is false, anything not on the list is uncached.
            return usize::MAX;
        }
        // But if invert is true, anything not on the list IS assumed cached, so we are done if everything on our
        // list is cached!
        let mut count = 0;
        for (x, z) in &self.positions {
            if cache_status(cache.as_deref_mut(), *x, *z) != Status::Explored {
                // Either waiting for it or don't have it at all.
                count += 1;
                if count >= minimum {
                    return count;
                }
            }
        }
        count
    }
}

pub struct ExploreProcess {
    exploration_origin: Option<Pos>,
    filter: Option<JsonFilter>,
    distance_completed: i32,
    /// The goal of the last tick, for `displayName0`.
    going_to: String,
    /// A height to keep on the way to every goal, whatever `exploreMaintainY` says. Set by the surface walk of MineProcess.
    maintain_y: Option<i32>,
}

/// The anonymous GoalXZ of `createGoal` that also keeps the height (`exploreMaintainY`).
struct GoalXzMaintainY {
    x: i32,
    z: i32,
    y: i32,
}

impl Goal for GoalXzMaintainY {
    fn is_in_goal(&self, p: Pos) -> bool {
        p[0] == self.x && p[2] == self.z
    }
    fn heuristic(&self, p: Pos) -> f64 {
        GoalXZ(self.x, self.z).heuristic(p) + y_heuristic(self.y, p[1])
    }
    fn label(&self) -> String {
        GoalXZ(self.x, self.z).label()
    }
}

/// How far below the player a search of the surface walk may plan to end a move.
pub const SURFACE_DEPTH: i32 = 6;

/// The surface walk's limit: a search never ends a move deeper than `SURFACE_DEPTH` below where the walk stands.
pub struct SurfaceFloor(pub i32);

impl CostHooks for SurfaceFloor {
    fn configure(&self, ctx: &mut CalculationContext) {
        ctx.floor_y = Some(self.0);
    }
}

fn create_goal(core: &Core, forced_y: Option<i32>, x: i32, z: i32) -> GoalRef {
    let y = forced_y.unwrap_or_else(|| core.settings.int("exploreMaintainY") as i32);
    if y == -1 {
        return rc(GoalXZ(x, z));
    }
    // Don't use a goalblock because we still want isInGoal to return true if X and Z are correct: we just want to try
    // and maintain Y on the way there, not necessarily end at that specific Y.
    rc(GoalXzMaintainY { x, z, y })
}

impl ExploreProcess {
    pub fn new() -> Self {
        Self { exploration_origin: None, filter: None, distance_completed: 0, going_to: String::new(), maintain_y: None }
    }

    /// `explore(centerX, centerZ)`.
    pub fn explore(&mut self, x: i32, z: i32) {
        self.exploration_origin = Some([x, 0, z]);
        self.distance_completed = 0;
    }

    /// Keep this height on the way to every goal (the surface walk of `#mine` uses the height it began at).
    pub fn set_maintain_y(&mut self, y: i32) {
        self.maintain_y = Some(y);
    }

    /// `applyJsonFilter`: chunks (x, z) from a file. `invert`: the list is the chunks that are NOT explored.
    pub fn set_filter(&mut self, chunks: HashSet<(i32, i32)>, invert: bool) {
        let positions: Vec<(i32, i32)> = chunks.iter().copied().collect();
        self.filter = Some(JsonFilter { in_filter: chunks, positions, invert });
    }

    /// The `EitherChunk` of the filter and the cache, or the cache alone.
    fn is_already_explored(&self, cache: Option<&mut CachedWorld>, chunk_x: i32, chunk_z: i32) -> Status {
        if let Some(f) = &self.filter {
            if f.status(chunk_x, chunk_z) == Status::Explored {
                return Status::Explored;
            }
        }
        cache_status(cache, chunk_x, chunk_z)
    }

    fn count_remain(&self, cache: Option<&mut CachedWorld>, minimum: usize) -> usize {
        match &self.filter {
            Some(f) => f.count_remain(cache, minimum),
            None => usize::MAX,
        }
    }

    /// `closestUncachedChunks`: None while a region is being read.
    fn closest_uncached_chunks(&mut self, core: &mut Core, center: Pos) -> Option<Vec<GoalRef>> {
        let (chunk_x, chunk_z) = (center[0] >> 4, center[2] >> 4);
        let minimum = core.settings.int("exploreChunkSetMinimumSize").max(0) as usize;
        let render_distance = core.settings.int("worldExploringChunkOffset") as i32;
        let mut count = self.count_remain(core.world_data.current.as_mut().map(|d| &mut d.cache), minimum).min(minimum);
        let mut centers: Vec<(i32, i32)> = Vec::new();
        let mut dist = self.distance_completed;
        loop {
            for dx in -dist..=dist {
                let zval = dist - dx.abs();
                for mult in 0..2 {
                    let dz = (mult * 2 - 1) * zval; // dz can be either -zval or zval
                    match self.is_already_explored(core.world_data.current.as_mut().map(|d| &mut d.cache), chunk_x + dx, chunk_z + dz) {
                        Status::Unknown => return None, // awaiting load
                        Status::Explored => continue,
                        Status::NotExplored => {}
                    }
                    let mut center_x = ((chunk_x + dx) << 4) + 8;
                    let mut center_z = ((chunk_z + dz) << 4) + 8;
                    let offset = render_distance << 4;
                    center_x += if dx < 0 { -offset } else { offset };
                    center_z += if dz < 0 { -offset } else { offset };
                    centers.push((center_x, center_z));
                }
            }
            if dist % 10 == 0 {
                count = self.count_remain(core.world_data.current.as_mut().map(|d| &mut d.cache), minimum).min(minimum);
            }
            if centers.len() >= count {
                return Some(centers.iter().map(|(x, z)| create_goal(core, self.maintain_y, *x, *z)).collect());
            }
            if centers.is_empty() {
                // We have explored everything from 0 to dist inclusive; next time we should start our check at dist + 1.
                self.distance_completed = dist + 1;
            }
            dist += 1;
            if dist > 4096 {
                // The Java has no end to this loop; the world has one.
                return Some(centers.iter().map(|(x, z)| create_goal(core, self.maintain_y, *x, *z)).collect());
            }
        }
    }
}

/// Read a filter file: chunk coordinates `[{"x":0,"z":0},...]`. The error is what #explorefilter says.
pub fn read_filter(path: &std::path::Path) -> Result<HashSet<(i32, i32)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound { "File not found".to_owned() } else { format!("Cannot read the file: {e}") }
    })?;
    let t = text.trim();
    if !(t.starts_with('[') && t.ends_with(']')) {
        return Err("Invalid JSON syntax".into());
    }
    Ok(chunk_pairs(t))
}

/// The pairs of whole numbers in a text, in order: `[[1,2],[3,4]]` or `{"x":1,"z":2}` lines both read.
pub fn chunk_pairs(text: &str) -> HashSet<(i32, i32)> {
    let mut numbers: Vec<i32> = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, numbers: &mut Vec<i32>| {
        if let Ok(n) = current.parse::<i32>() {
            numbers.push(n);
        }
        current.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() || (c == '-' && current.is_empty()) {
            current.push(c);
        } else {
            flush(&mut current, &mut numbers);
        }
    }
    flush(&mut current, &mut numbers);
    numbers.chunks_exact(2).map(|p| (p[0], p[1])).collect()
}

impl Process for ExploreProcess {
    fn name(&self) -> &'static str {
        "explore"
    }

    fn display(&self) -> String {
        match self.exploration_origin {
            Some(o) => format!("Exploring around BlockPos{{x={}, y={}, z={}}}, distance completed {}, currently going to {}", o[0], o[1], o[2], self.distance_completed, self.going_to),
            None => "Not exploring".into(),
        }
    }

    fn is_active(&self) -> bool {
        self.exploration_origin.is_some()
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, _safe: bool) -> PathingCommand {
        let Some(origin) = self.exploration_origin else { return PathingCommand::Defer };
        if calc_failed {
            core.say(game, "Failed");
            if core.settings.flag("notificationOnExploreFinished") {
                core.notify("Exploration failed", true);
            }
            self.exploration_origin = None;
            return PathingCommand::Defer;
        }
        let minimum = core.settings.int("exploreChunkSetMinimumSize").max(0) as usize;
        if !core.settings.flag("disableCompletionCheck") && self.count_remain(core.world_data.current.as_mut().map(|d| &mut d.cache), minimum) == 0 {
            core.say(game, "Explored all chunks");
            if core.settings.flag("notificationOnExploreFinished") {
                core.notify("Explored all chunks", false);
            }
            self.exploration_origin = None;
            return PathingCommand::Defer;
        }
        match self.closest_uncached_chunks(core, origin) {
            None => PathingCommand::RequestPause, // awaiting region load from disk
            Some(goals) => {
                let goal = rc(GoalComposite(goals));
                self.going_to = goal.label();
                if self.maintain_y.is_some() {
                    if let Some(feet) = core.player_feet(game) {
                        core.path_hooks = Some(Rc::new(SurfaceFloor(feet[1] - SURFACE_DEPTH)));
                    }
                }
                PathingCommand::ForceRevalidateGoalAndPath(goal)
            }
        }
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {
        self.exploration_origin = None;
    }

    fn cancel(&mut self) {
        self.exploration_origin = None;
    }
}
