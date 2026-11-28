// baritone.process.elytra.pathfinder.PathFinder: A* over cubes of the chunk octrees, as the native PathFinder.cpp had it.
// A node is a cube of 2 to 16 blocks that is all air; a step to a neighbour grows the neighbour to the largest empty cube
// it sits in, or shrinks it into the four smaller cubes on the shared face until they are empty or too small.
use super::baritone_elytra_chunk::{Chunk, NodePos, Size};
use super::baritone_elytra_path_node::{BinaryHeapOpenSet, PathNode};
use super::baritone_elytra_pathfinder::{key, CacheMiss, NetherPathfinder};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

/// might want to increase this
pub const MIN_DIST_PATH: f64 = 5.0;
const MIN_IMPROVEMENT: f64 = 0.01;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Up,
    Down,
    North,
    South,
    East,
    West,
}

// The native PathFinder.cpp's order, so the search expands neighbours as it always has.
const ALL_FACES: [Face; 6] = [Face::Up, Face::Down, Face::North, Face::South, Face::East, Face::West];

impl Face {
    fn step(self) -> [i32; 3] {
        match self {
            Face::Up => [0, 1, 0],
            Face::Down => [0, -1, 0],
            Face::North => [0, 0, -1],
            Face::South => [0, 0, 1],
            Face::East => [1, 0, 0],
            Face::West => [-1, 0, 0],
        }
    }
}

pub struct Path {
    /// FINISHED when it reached the goal, otherwise a SEGMENT.
    pub finished: bool,
    #[allow(dead_code)]
    pub start: [i32; 3],
    /// Where the path wants to go, not necessarily where it ends.
    #[allow(dead_code)]
    pub goal: [i32; 3],
    pub blocks: Vec<[i32; 3]>,
}

impl Path {
    #[allow(dead_code)]
    pub fn end_pos(&self) -> [i32; 3] {
        // This should basically never be empty.
        self.blocks.last().copied().unwrap_or(self.start)
    }
}

pub fn is_in_bounds(world_height: i32, y: i32) -> bool {
    y >= 0 && y < world_height
}

fn close_to_goal(node: &NodePos, goal: [i32; 3]) -> bool {
    node.center_dist_sqr(goal) <= 16.0 * 16.0
}

fn in_goal(node: &NodePos, goal: [i32; 3]) -> bool {
    if !close_to_goal(node, goal) {
        return false;
    }
    let (x, y, z) = (node.min_x(), node.min_y(), node.min_z());
    let w = node.size.width();
    goal[0] >= x && goal[0] <= x + w && goal[1] >= y && goal[1] <= y + w && goal[2] >= z && goal[2] <= z + w
}

struct Search {
    nodes: Vec<PathNode>,
    map: HashMap<NodePos, usize>,
    open_set: BinaryHeapOpenSet,
    goal_center: [i32; 3],
    start_center: [i32; 3],
    fake_chunk_cost: f64,
    current_node: usize,
    best_so_far: usize,
    best_heuristic_so_far: f64,
    failing: bool,
}

impl Search {
    fn new(goal_center: [i32; 3], start_center: [i32; 3], fake_chunk_cost: f64) -> Self {
        Self {
            nodes: Vec::new(),
            map: HashMap::new(),
            open_set: BinaryHeapOpenSet::new(),
            goal_center,
            start_center,
            fake_chunk_cost,
            current_node: 0,
            best_so_far: 0,
            best_heuristic_so_far: 0.0,
            failing: true,
        }
    }

    /// `getNodeAtPosition`.
    fn node_at(&mut self, pos: NodePos, goal: [i32; 3]) -> usize {
        if let Some(i) = self.map.get(&pos) {
            return *i;
        }
        self.nodes.push(PathNode::new(pos, goal));
        self.map.insert(pos, self.nodes.len() - 1);
        self.nodes.len() - 1
    }

    /// `accept`: a neighbour cube of the current node that is all air.
    fn accept(&mut self, neighbor_pos: NodePos, from_caller: bool) {
        let n = self.node_at(neighbor_pos, self.goal_center);
        let cost = if from_caller { 1.0 } else { self.fake_chunk_cost };
        let tentative_cost = self.nodes[self.current_node].cost + cost;
        if self.nodes[n].cost - tentative_cost > MIN_IMPROVEMENT {
            let combined = tentative_cost + self.nodes[n].estimated_cost_to_goal;
            let node = &mut self.nodes[n];
            node.previous = Some(self.current_node);
            node.cost = tentative_cost;
            node.combined_cost = combined;

            if self.nodes[n].is_open() {
                self.open_set.update(&mut self.nodes, n);
            } else {
                self.open_set.insert(&mut self.nodes, n); // dont double count, dont insert into open set if it's already there
            }

            let heuristic = self.nodes[n].combined_cost;
            if self.best_heuristic_so_far - heuristic > MIN_IMPROVEMENT {
                self.best_heuristic_so_far = heuristic;
                self.best_so_far = n;
                if self.failing && neighbor_pos.center_dist_sqr(self.start_center) > MIN_DIST_PATH * MIN_DIST_PATH {
                    self.failing = false;
                }
            }
        }
    }

    fn create_path(&self, end: usize, start_pos: [i32; 3], goal: [i32; 3], finished: bool) -> Path {
        let mut blocks = Vec::new();
        let mut current = Some(end);
        while let Some(i) = current {
            blocks.push(self.nodes[i].pos.absolute_pos_center());
            current = self.nodes[i].previous;
        }
        blocks.reverse();
        Path { finished, start: start_pos, goal, blocks }
    }

    fn best_path_so_far(&self, end: usize, start_pos: [i32; 3], goal: [i32; 3]) -> Option<Path> {
        let dist_sq = self.nodes[end].pos.center_dist_sqr(start_pos);
        if dist_sq > MIN_DIST_PATH * MIN_DIST_PATH {
            return Some(self.create_path(end, start_pos, goal, false));
        }
        // The path took too long and got nowhere.
        None
    }

    /// Called inside a big neighbour cube whose lowest corner is (x, y, z); iterates the 4 sub cubes that are adjacent to
    /// the original cube: the corner of the layer that touches it, one step along each of the layer's two axes, and both.
    /// The face is relative to the original cube, the size is the sub cubes'.
    #[allow(clippy::too_many_arguments)]
    fn for_each_sub_cube(&mut self, chunk: &Chunk, from_caller: bool, mut x: i32, mut y: i32, mut z: i32, face: Face, sz: Size, min_size: Size) {
        let w = sz.width();
        // The layer's two axes: ay and bx are never set, as in the Java.
        let (mut ax, mut az, mut by, mut bz) = (0, 0, 0, 0);
        let (ay, bx) = (0, 0);
        match face {
            Face::Up => {
                ax = w;
                bz = w;
            }
            Face::Down => {
                y += w;
                ax = w;
                bz = w;
            }
            Face::North => {
                z += w;
                ax = w;
                by = w;
            }
            Face::South => {
                ax = w;
                by = w;
            }
            Face::East => {
                az = w;
                by = w;
            }
            Face::West => {
                x += w;
                az = w;
                by = w;
            }
        }
        self.for_each_neighbor_in_cube(chunk, from_caller, NodePos::new(sz, x, y, z), face, sz, false, min_size);
        self.for_each_neighbor_in_cube(chunk, from_caller, NodePos::new(sz, x + ax, y + ay, z + az), face, sz, false, min_size);
        self.for_each_neighbor_in_cube(chunk, from_caller, NodePos::new(sz, x + bx, y + by, z + bz), face, sz, false, min_size);
        self.for_each_neighbor_in_cube(chunk, from_caller, NodePos::new(sz, x + ax + bx, y + ay + by, z + az + bz), face, sz, false, min_size);
    }

    #[allow(clippy::too_many_arguments)]
    fn for_each_neighbor_in_cube(&mut self, chunk: &Chunk, from_caller: bool, neighbor_node: NodePos, face: Face, size: Size, size_change: bool, min_size: Size) {
        if size_change {
            self.accept(neighbor_node, from_caller);
            return;
        }
        let (x, y, z) = (neighbor_node.min_x(), neighbor_node.min_y(), neighbor_node.min_z());
        if chunk.is_empty(size, x & 15, y, z & 15) {
            self.accept(neighbor_node, from_caller);
            return;
        }
        if size != Size::X1 {
            let next_size = size.smaller();
            // Don't shrink cubes to X1 because they suck and make the path try to squeeze through small areas.
            if next_size.ordinal() < min_size.ordinal() {
                return;
            }
            self.for_each_sub_cube(chunk, from_caller, x, y, z, face, next_size, min_size);
        }
    }

    /// Grows the neighbour to the largest empty cube it is in, then iterates what is on the face.
    fn grow_then_iterate(&mut self, chunk: &Chunk, from_caller: bool, pos: NodePos, face: Face, min_size: Size) {
        let original_size = pos.size;
        let (x, y, z) = (pos.min_x(), pos.min_y(), pos.min_z());
        let mut s = original_size;
        loop {
            if s == Size::X16 || !chunk.is_empty(s.larger(), x & 15, y, z & 15) {
                self.for_each_neighbor_in_cube(chunk, from_caller, NodePos::new(s, x, y, z), face, s, original_size != s, min_size);
                return;
            }
            s = s.larger();
        }
    }
}

/// One segment of a path from start towards goal. Returns a FINISHED path if it reached the goal, a SEGMENT if it ran out
/// of time but got somewhere, and None if it got nowhere or was cancelled.
pub fn find_path_segment(ctx: &NetherPathfinder, start: NodePos, goal: NodePos, x4_min: bool, timeout_ms: u64, air_if_fake: bool, fake_chunk_cost: f64) -> Option<Path> {
    let fake_chunk_mode = if air_if_fake { CacheMiss::Air } else { CacheMiss::Generate };
    let min_size = if x4_min { Size::X4 } else { Size::X2 };
    let goal_center = goal.absolute_pos_center();
    let start_center = start.absolute_pos_center();

    let mut s = Search::new(goal_center, start_center, fake_chunk_cost);
    let mut done_full: HashSet<u64> = HashSet::new();

    let start_node = s.node_at(start, [goal.min_x(), goal.min_y(), goal.min_z()]);
    ctx.try_load_region(start.min_x() >> 4, start.min_z() >> 4);
    // The neighbours are nearly always in the region we just asked about.
    let mut last_region = key(start.min_x() >> 9, start.min_z() >> 9);
    s.nodes[start_node].cost = 0.0;
    s.nodes[start_node].combined_cost = s.nodes[start_node].estimated_cost_to_goal;
    s.open_set.insert(&mut s.nodes, start_node);
    ctx.real_chunk_from_cache_or_fake_chunk_maybe_gen(start.min_x() >> 4, start.min_z() >> 4, fake_chunk_mode);

    s.best_so_far = start_node;
    s.best_heuristic_so_far = s.nodes[start_node].estimated_cost_to_goal;

    let started = Instant::now();
    let primary_timeout = 500u64;
    let timeout = if timeout_ms != 0 { timeout_ms } else { 30_000 };
    let mut time_doing_io = 0u64; // milliseconds spent reading region files, which the timeouts do not count

    let mut num_nodes = 0u64;
    let mut fake_chunk_visits = 0; // if this gets too high we return
    while !s.open_set.is_empty() {
        if num_nodes & 63 == 0 {
            // Only look at the clock once every 64 nodes.
            let now = (started.elapsed().as_millis() as u64).saturating_sub(time_doing_io);
            if now >= timeout || (!s.failing && now >= primary_timeout) {
                break;
            } else if ctx.is_cancelled() {
                return None;
            }
        }
        num_nodes += 1;

        let current = s.open_set.remove_lowest(&mut s.nodes);
        s.current_node = current;
        let pos = s.nodes[current].pos;
        if in_goal(&pos, goal_center) {
            return Some(s.create_path(current, start_center, goal_center, true));
        }
        let size = pos.size;
        let (x, y, z) = (pos.min_x(), pos.min_y(), pos.min_z());
        let w = size.width();
        let (cx, cz) = (x >> 4, z >> 4);
        let current_chunk = ctx.get_chunk_or_air(cx, cz);
        if !current_chunk.from_caller {
            fake_chunk_visits += 1;
        } else {
            fake_chunk_visits = 0;
        }
        if fake_chunk_visits >= 100 && air_if_fake {
            return s.best_path_so_far(s.best_so_far, start_center, goal_center);
        }
        if !air_if_fake && done_full.insert(key(cx, cz)) {
            generate_missing_neighbours(ctx, cx, cz);
        }

        for face in ALL_FACES {
            let [sx, sy, sz] = face.step();
            let (nx, ny, nz) = (x + sx * w, y + sy * w, z + sz * w);
            if (face == Face::Up || face == Face::Down) && !is_in_bounds(ctx.max_height, ny) {
                continue;
            }
            let (neighbor_cx, neighbor_cz) = (nx >> 4, nz >> 4);
            let region = key(neighbor_cx >> 5, neighbor_cz >> 5);
            if region != last_region {
                time_doing_io += ctx.try_load_region(neighbor_cx, neighbor_cz);
                last_region = region;
            }
            let entry = if neighbor_cx == cx && neighbor_cz == cz { current_chunk.clone() } else { ctx.get_chunk_or_air(neighbor_cx, neighbor_cz) };
            s.grow_then_iterate(&entry.chunk, entry.from_caller, NodePos::new(size, nx, ny, nz), face, min_size);
        }
    }
    s.best_path_so_far(s.best_so_far, start_center, goal_center)
}

const NEIGHBOUR_OFFSETS: [[i32; 2]; 4] = [[0, -1], [0, 1], [1, 0], [-1, 0]]; // north, south, east, west

/// Generates those of the four chunks around (cx, cz) that the table does not have yet. The Java generates them all at
/// once on its common pool; here they come one after another.
fn generate_missing_neighbours(ctx: &NetherPathfinder, cx: i32, cz: i32) {
    for [dx, dz] in NEIGHBOUR_OFFSETS {
        let (nx, nz) = (cx + dx, cz + dz);
        if !ctx.has_chunk(nx, nz) {
            ctx.get_or_gen_chunk(nx, nz);
        }
    }
}

/// Segment after segment until the goal, generating terrain. What the native main.cpp ran.
#[allow(dead_code)]
pub fn find_path_full(ctx: &NetherPathfinder, start: NodePos, goal: NodePos, fake_chunk_cost: f64) -> Result<Option<Path>, String> {
    if !is_in_bounds(ctx.max_height, start.absolute_pos_center()[1]) {
        return Err("start is out of bounds".into());
    }
    let mut segments: Vec<Path> = Vec::new();
    loop {
        let last_path_end = match segments.last() {
            Some(last) => {
                let e = last.end_pos();
                NodePos::new(Size::X2, e[0], e[1], e[2])
            }
            None => start,
        };
        let Some(path) = find_path_segment(ctx, last_path_end, goal, true, 0, false, fake_chunk_cost) else {
            if ctx.clear_cancelled() {
                return Ok(None);
            }
            break;
        };
        let end = path.end_pos();
        ctx.cull_far_chunks(end[0] >> 4, end[2] >> 4, 200);
        let finished = path.finished;
        segments.push(path);
        if finished {
            break;
        }
    }
    let Some(first) = segments.first() else { return Ok(None) };
    let (start_pos, goal_pos) = (first.start, first.goal);
    let finished = segments.last().is_some_and(|p| p.finished);
    let blocks = segments.into_iter().flat_map(|p| p.blocks).collect();
    Ok(Some(Path { finished, start: start_pos, goal: goal_pos, blocks }))
}

/// The nearest cube of the given size around block (x, y, z) that is all air, searching outwards.
pub fn find_air(ctx: &NetherPathfinder, size: Size, x: i32, y: i32, z: i32, air_if_fake: bool) -> Result<NodePos, String> {
    if !is_in_bounds(ctx.max_height, y) {
        return Err(format!("position is out of bounds: {x}, {y}, {z}"));
    }
    let start = NodePos::new(size, x, y, z);
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    queue.push_back(start);
    visited.insert(start);
    let w = size.width();
    while let Some(node) = queue.pop_front() {
        let (nx, ny, nz) = (node.min_x(), node.min_y(), node.min_z());
        if is_in_bounds(ctx.max_height, ny) {
            let chunk = if air_if_fake { ctx.get_chunk_or_air(nx >> 4, nz >> 4).chunk.clone() } else { ctx.get_or_gen_chunk(nx >> 4, nz >> 4) };
            if chunk.is_empty(size, nx & 15, ny, nz & 15) {
                return Ok(node);
            }
            for (dx, dy, dz) in [(-w, 0, 0), (w, 0, 0), (0, 0, -w), (0, 0, w), (0, w, 0), (0, -w, 0)] {
                // west, east, north, south, above, below
                let next = NodePos::new(size, nx + dx, ny + dy, nz + dz);
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }
    // Shouldn't be possible to exit the while loop.
    Err(format!("no air anywhere around {x}, {y}, {z}"))
}
