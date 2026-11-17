// baritone.pathing.calc.AStarPathFinder, PathNode, IOpenSet, BinaryHeapOpenSet, LinkedListOpenSet and AbstractNodeCostSearch. The
// search is the Java's, move for move and cost for cost, but it runs in slices: the game has one thread,
// so `run` expands a number of nodes and returns, and the next call carries on from the same open set.
// The time limits of the Java (primaryTimeout, failureTimeout) are decided by the caller in ticks.
use super::baritone_calculation_context::{CalculationContext, MutableMoveResult};
use super::baritone_costs::COST_INF;
use super::baritone_goals::GoalRef;
use super::baritone_moves::{Moves, ALL};
use super::baritone_calc_path::Path;
use super::baritone_utils::Pos;
use std::collections::HashMap;

/// AbstractNodeCostSearch.COEFFICIENTS: the weights the best-so-far nodes are tracked at.
pub const COEFFICIENTS: [f64; 7] = [1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 10.0];
pub const MIN_DIST_PATH: f64 = 5.0;
pub const MIN_IMPROVEMENT: f64 = 0.01;

pub struct PathNode {
    pub pos: Pos,
    pub estimated_cost_to_goal: f64,
    pub cost: f64,
    pub combined_cost: f64,
    pub previous: Option<usize>,
    heap_position: i32,
    pub favor: f64,
}

impl PathNode {
    fn new(pos: Pos, goal: &GoalRef) -> Self {
        let h = goal.heuristic(pos);
        assert!(!h.is_nan(), "{} calculated implausible heuristic NaN at {pos:?}", goal.label());
        Self { pos, estimated_cost_to_goal: h, cost: COST_INF, combined_cost: 0.0, previous: None, heap_position: -1, favor: 0.0 }
    }

    pub fn is_open(&self) -> bool {
        self.heap_position != -1
    }
}

/// IOpenSet: what the search asks of its open set. The nodes live in one arena, so the set holds indices into it.
pub trait IOpenSet {
    fn is_empty(&self) -> bool;
    /// Add a node whose combined cost is set.
    fn insert(&mut self, nodes: &mut [PathNode], value: usize);
    /// Tell the set a node's combined cost went down.
    fn update(&mut self, nodes: &mut [PathNode], value: usize);
    /// Take the node of the lowest combined cost.
    fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> Option<usize>;
}

impl IOpenSet for BinaryHeapOpenSet {
    fn is_empty(&self) -> bool {
        BinaryHeapOpenSet::is_empty(self)
    }
    fn insert(&mut self, nodes: &mut [PathNode], value: usize) {
        BinaryHeapOpenSet::insert(self, nodes, value);
    }
    fn update(&mut self, nodes: &mut [PathNode], value: usize) {
        BinaryHeapOpenSet::update(self, nodes, value);
    }
    fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> Option<usize> {
        Some(BinaryHeapOpenSet::remove_lowest(self, nodes)) // the heap refuses an empty pop, as the Java throws
    }
}

/// LinkedListOpenSet, the original open set of MineBot: insert is a push, removeLowest walks the whole list. The Java
/// notes it finds a tenth of the nodes the heap does in the same time, and nothing but its tests use it; the search here
/// uses BinaryHeapOpenSet. The list is kept with its head at the end of the vector, so a tie goes to the newest node as
/// it does in the Java.
#[derive(Default)]
pub struct LinkedListOpenSet {
    list: Vec<usize>,
}

impl IOpenSet for LinkedListOpenSet {
    fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
    fn insert(&mut self, _nodes: &mut [PathNode], value: usize) {
        self.list.push(value);
    }
    fn update(&mut self, _nodes: &mut [PathNode], _value: usize) {}
    fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> Option<usize> {
        let mut best = self.list.len().checked_sub(1)?;
        let mut best_value = nodes[self.list[best]].combined_cost;
        for i in (0..best).rev() {
            let cost = nodes[self.list[i]].combined_cost;
            if cost < best_value {
                best_value = cost;
                best = i;
            }
        }
        Some(self.list.remove(best))
    }
}

/// BinaryHeapOpenSet: a 1-based binary heap of node indices keyed on the combined cost. The order in
/// which equal keys come out decides which of two equally cheap paths is found, so it is kept as it is.
#[derive(Default)]
pub struct BinaryHeapOpenSet {
    array: Vec<usize>,
    keys: Vec<f64>,
}

impl BinaryHeapOpenSet {
    pub fn new() -> Self {
        Self { array: vec![usize::MAX], keys: vec![0.0] }
    }

    fn size(&self) -> usize {
        self.array.len() - 1
    }

    fn is_empty(&self) -> bool {
        self.size() == 0
    }

    fn insert(&mut self, nodes: &mut [PathNode], value: usize) {
        self.array.push(value);
        self.keys.push(0.0);
        let size = self.size();
        let cost = nodes[value].combined_cost;
        self.sift_up(nodes, size, value, cost);
    }

    fn update(&mut self, nodes: &mut [PathNode], value: usize) {
        // The cost only ever goes down, so it only ever goes up (the heap, that is).
        let at = nodes[value].heap_position as usize;
        let cost = nodes[value].combined_cost;
        self.sift_up(nodes, at, value, cost);
    }

    fn sift_up(&mut self, nodes: &mut [PathNode], mut index: usize, val: usize, cost: f64) {
        let mut parent = index >> 1;
        while index > 1 && self.keys[parent] > cost {
            let parent_node = self.array[parent];
            self.array[index] = parent_node;
            self.keys[index] = self.keys[parent];
            nodes[parent_node].heap_position = index as i32;
            index = parent;
            parent = index >> 1;
        }
        self.array[index] = val;
        self.keys[index] = cost;
        nodes[val].heap_position = index as i32;
    }

    fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> usize {
        assert!(!self.is_empty(), "Cannot remove from empty heap");
        let size = self.size();
        let result = self.array[1];
        nodes[result].heap_position = -1;
        let val = self.array[size];
        let cost = self.keys[size];
        self.array.pop();
        self.keys.pop();
        let size = size - 1;
        if size == 0 {
            return result;
        }
        let mut index = 1;
        let mut child = 2;
        while child <= size {
            let mut child_cost = self.keys[child];
            if child < size && self.keys[child + 1] < child_cost {
                child += 1;
                child_cost = self.keys[child];
            }
            if cost <= child_cost {
                break;
            }
            let child_node = self.array[child];
            self.array[index] = child_node;
            self.keys[index] = child_cost;
            nodes[child_node].heap_position = index as i32;
            index = child;
            child = index << 1;
        }
        self.array[index] = val;
        self.keys[index] = cost;
        nodes[val].heap_position = index as i32;
        result
    }
}

/// How a slice of the search ended.
pub enum Status {
    /// The open set is not empty and no limit was hit: call `run` again.
    Running,
    /// A node in the goal was reached.
    Done(Path),
    /// The open set ran out (or too many empty chunks were met): ask `best_so_far`.
    Exhausted,
}

pub struct AStarPathFinder {
    real_start: Pos,
    start: Pos,
    goal: GoalRef,
    favoring: HashMap<Pos, f64>,
    nodes: Vec<PathNode>,
    index: HashMap<Pos, usize>,
    open: BinaryHeapOpenSet,
    start_node: usize,
    best_so_far: [usize; 7],
    best_heuristic: [f64; 7],
    most_recent_considered: Option<usize>,
    pub failing: bool,
    pub num_nodes: usize,
    pub num_movements_considered: usize,
    num_empty_chunk: usize,
    max_chunk_border_fetch: usize,
    minimum_improvement: f64,
    started: bool,
}

impl AStarPathFinder {
    /// `pathingMapDefaultSize`: how many nodes the table is made for. (`pathingMapLoadFactor` tunes the Java's open hash map and
    /// has no counterpart in this one.)
    pub fn with_map_size(mut self, size: usize) -> Self {
        self.index = HashMap::with_capacity(size);
        self
    }

    /// `favoring` maps a position to the factor its moves cost (the previous path, avoided mobs).
    pub fn new(real_start: Pos, start: Pos, goal: GoalRef, favoring: HashMap<Pos, f64>, max_chunk_border_fetch: usize, minimum_improvement_repropagation: bool) -> Self {
        Self {
            real_start,
            start,
            goal,
            favoring,
            nodes: Vec::new(),
            index: HashMap::new(),
            open: BinaryHeapOpenSet::new(),
            start_node: 0,
            best_so_far: [0; 7],
            best_heuristic: [0.0; 7],
            most_recent_considered: None,
            failing: true,
            num_nodes: 0,
            num_movements_considered: 0,
            num_empty_chunk: 0,
            max_chunk_border_fetch,
            minimum_improvement: if minimum_improvement_repropagation { MIN_IMPROVEMENT } else { 0.0 },
            started: false,
        }
    }

    fn node_at(&mut self, pos: Pos) -> usize {
        if let Some(&i) = self.index.get(&pos) {
            return i;
        }
        self.nodes.push(PathNode::new(pos, &self.goal));
        let i = self.nodes.len() - 1;
        self.index.insert(pos, i);
        i
    }

    fn dist_from_start_sq(&self, n: usize) -> f64 {
        let p = self.nodes[n].pos;
        let (dx, dy, dz) = (f64::from(p[0] - self.start[0]), f64::from(p[1] - self.start[1]), f64::from(p[2] - self.start[2]));
        dx * dx + dy * dy + dz * dz
    }

    /// Expand up to `budget` nodes. The caller applies the time limits (`failing` says whether the
    /// failure limit or the primary limit is the one that applies).
    pub fn run(&mut self, ctx: &CalculationContext, budget: usize) -> Status {
        if !self.started {
            self.started = true;
            self.start_node = self.node_at(self.start);
            let s = self.start_node;
            self.nodes[s].cost = 0.0;
            self.nodes[s].combined_cost = self.nodes[s].estimated_cost_to_goal;
            self.open.insert(&mut self.nodes, s);
            for i in 0..COEFFICIENTS.len() {
                self.best_heuristic[i] = self.nodes[s].estimated_cost_to_goal;
                self.best_so_far[i] = s;
            }
        }
        let is_favoring = !self.favoring.is_empty();
        let mut res = MutableMoveResult::default();
        let mut slice = 0;
        while !self.open.is_empty() && self.num_empty_chunk < self.max_chunk_border_fetch {
            if slice >= budget {
                return Status::Running;
            }
            slice += 1;
            let current = self.open.remove_lowest(&mut self.nodes);
            self.most_recent_considered = Some(current);
            self.num_nodes += 1;
            let cur = self.nodes[current].pos;
            if self.goal.is_in_goal(cur) {
                return Status::Done(Path::from_nodes(self.real_start, self.start, &self.nodes_chain(current), self.num_nodes, self.goal.clone()));
            }
            for &moves in ALL.iter() {
                let (xo, yo, zo, dynamic_xz, dynamic_y) = moves.offsets();
                let (new_x, new_z) = (cur[0] + xo, cur[2] + zo);
                if ((new_x >> 4 != cur[0] >> 4) || (new_z >> 4 != cur[2] >> 4)) && !ctx.is_loaded(new_x, new_z) {
                    // Only the destination of a move into another chunk needs checking.
                    if !dynamic_xz {
                        // Only counted when the move would have gone out of bounds guaranteed.
                        self.num_empty_chunk += 1;
                    }
                    continue;
                }
                if !dynamic_xz && !ctx.bsi.entirely_contains(new_x, new_z) {
                    continue;
                }
                if cur[1] + yo > ctx.max_y || cur[1] + yo < ctx.min_y {
                    continue;
                }
                res = MutableMoveResult::default();
                moves.apply(ctx, cur[0], cur[1], cur[2], &mut res);
                self.num_movements_considered += 1;
                let mut action_cost = res.cost;
                if action_cost >= COST_INF {
                    continue;
                }
                assert!(action_cost > 0.0 && !action_cost.is_nan(), "{moves:?} from {cur:?} calculated implausible cost {action_cost}");
                // Check the destination after verifying the cost is not COST_INF: some moves return it without setting one.
                if dynamic_xz && !ctx.bsi.entirely_contains(res.x, res.z) {
                    continue;
                }
                assert!(dynamic_xz || (res.x == new_x && res.z == new_z), "{moves:?} from {cur:?} ended at x z {} {} instead of {new_x} {new_z}", res.x, res.z);
                assert!(dynamic_y || res.y == cur[1] + yo, "{moves:?} from {cur:?} ended at y {} instead of {}", res.y, cur[1] + yo);
                let neighbor = self.node_at([res.x, res.y, res.z]);
                if is_favoring {
                    // Favoring only cares where you end up, not where you came from.
                    if self.nodes[neighbor].favor == 0.0 {
                        self.nodes[neighbor].favor = self.favoring.get(&self.nodes[neighbor].pos).copied().unwrap_or(1.0);
                    }
                    action_cost *= self.nodes[neighbor].favor;
                }
                let tentative_cost = self.nodes[current].cost + action_cost;
                if self.nodes[neighbor].cost - tentative_cost > self.minimum_improvement {
                    self.nodes[neighbor].previous = Some(current);
                    self.nodes[neighbor].cost = tentative_cost;
                    self.nodes[neighbor].combined_cost = tentative_cost + self.nodes[neighbor].estimated_cost_to_goal;
                    if self.nodes[neighbor].is_open() {
                        self.open.update(&mut self.nodes, neighbor);
                    } else {
                        self.open.insert(&mut self.nodes, neighbor);
                    }
                    for i in 0..COEFFICIENTS.len() {
                        let heuristic = self.nodes[neighbor].estimated_cost_to_goal + self.nodes[neighbor].cost * (1.0 / COEFFICIENTS[i]);
                        if self.best_heuristic[i] - heuristic > self.minimum_improvement {
                            self.best_heuristic[i] = heuristic;
                            self.best_so_far[i] = neighbor;
                            if self.failing && self.dist_from_start_sq(neighbor) > MIN_DIST_PATH * MIN_DIST_PATH {
                                self.failing = false;
                            }
                        }
                    }
                }
            }
        }
        let _ = res;
        Status::Exhausted
    }

    /// The chain of positions and costs from the start to `end`, in order.
    fn nodes_chain(&self, end: usize) -> Vec<(Pos, f64)> {
        let mut chain = Vec::new();
        let mut current = Some(end);
        while let Some(i) = current {
            chain.push((self.nodes[i].pos, self.nodes[i].cost));
            current = self.nodes[i].previous;
        }
        chain.reverse();
        chain
    }

    /// AbstractNodeCostSearch.bestSoFar: the path to the node that came nearest at the smallest weight,
    /// if any came further than MIN_DIST_PATH from the start.
    pub fn best_so_far(&self, num_nodes: usize) -> Option<Path> {
        if !self.started {
            return None;
        }
        for i in 0..COEFFICIENTS.len() {
            let dist = self.dist_from_start_sq(self.best_so_far[i]);
            if dist > MIN_DIST_PATH * MIN_DIST_PATH {
                return Some(Path::from_nodes(self.real_start, self.start, &self.nodes_chain(self.best_so_far[i]), num_nodes, self.goal.clone()));
            }
        }
        None
    }

    /// What `bestSoFar(true, ..)` logs: the coefficient of the path it picks, or how far the best one came.
    pub fn best_so_far_report(&self) -> Result<f64, f64> {
        let mut best_dist = 0.0_f64;
        for (i, coefficient) in COEFFICIENTS.iter().enumerate() {
            let dist = self.dist_from_start_sq(self.best_so_far[i]);
            best_dist = best_dist.max(dist);
            if dist > MIN_DIST_PATH * MIN_DIST_PATH {
                return Ok(*coefficient);
            }
        }
        Err(best_dist.sqrt())
    }

    /// The path to the node considered last, for drawing.
    pub fn path_to_most_recent_node_considered(&self) -> Option<Path> {
        self.most_recent_considered.map(|n| Path::from_nodes(self.real_start, self.start, &self.nodes_chain(n), 0, self.goal.clone()))
    }

    #[allow(dead_code)]
    pub fn open_set_size(&self) -> usize {
        self.open.size()
    }

    #[allow(dead_code)]
    pub fn moves_considered(&self) -> usize {
        self.num_movements_considered
    }
}

#[allow(dead_code)]
fn _moves_used(_: Moves) {}

#[cfg(test)]
mod open_set_tests {
    use super::*;
    use crate::baritone_goals::{rc, GoalXZ};

    /// Both open sets hand out the same costs in the same order, whatever the insertion order and the updates.
    #[test]
    fn linked_list_and_binary_heap_agree_on_the_order_of_costs() {
        let goal = rc(GoalXZ(0, 0));
        let costs = [9.0, 3.5, 7.25, 3.5, 12.0, 0.5, 8.0, 3.5, 100.0, 1.0, 6.5, 2.0];
        let mut nodes: Vec<PathNode> = costs.iter().enumerate().map(|(i, c)| {
            let mut n = PathNode::new([i as i32, 64, 0], &goal);
            n.combined_cost = *c;
            n
        }).collect();
        let mut heap = BinaryHeapOpenSet::new();
        let mut list = LinkedListOpenSet::default();
        for i in 0..nodes.len() {
            heap.insert(&mut nodes, i);
            IOpenSet::insert(&mut list, &mut nodes, i);
        }
        nodes[4].combined_cost = 0.25;
        IOpenSet::update(&mut heap, &mut nodes, 4);
        IOpenSet::update(&mut list, &mut nodes, 4);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        while !IOpenSet::is_empty(&heap) {
            let n = IOpenSet::remove_lowest(&mut heap, &mut nodes).unwrap();
            a.push(nodes[n].combined_cost);
        }
        while let Some(n) = IOpenSet::remove_lowest(&mut list, &mut nodes) {
            b.push(nodes[n].combined_cost);
        }
        assert_eq!(a, b);
        assert!(a.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(a.len(), costs.len());
    }
}
