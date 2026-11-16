// baritone.pathing.calc.Path, baritone.pathing.path.CutoffPath and baritone.utils.pathing.PathBase: the
// positions a search found, the movements between them, and the cuts made before the path is walked.
use super::baritone_calculation_context::CalculationContext;
use super::baritone_goals::GoalRef;
use super::baritone_moves::ALL;
use super::baritone_movement::Movement;
use super::baritone_utils::Pos;

#[derive(Clone)]
pub struct Path {
    pub positions: Vec<Pos>,
    pub movements: Vec<Movement>,
    /// The cost of reaching each position, from the search.
    node_costs: Vec<f64>,
    pub num_nodes: usize,
    pub goal: GoalRef,
    verified: bool,
    /// The `logDebug` of a movement that became impossible during the calculation, for whoever post processed it.
    pub note: Option<String>,
}

impl Path {
    /// The Path constructor: `chain` is the start node to the end node, with their costs.
    pub fn from_nodes(real_start: Pos, start: Pos, chain: &[(Pos, f64)], num_nodes: usize, goal: GoalRef) -> Self {
        let mut positions: Vec<Pos> = chain.iter().map(|(p, _)| *p).collect();
        let mut node_costs: Vec<f64> = chain.iter().map(|(_, c)| *c).collect();
        // If the position the player is at differs from the position A* was told to start from, and A*
        // gave no movements, add a fake node so that a movement to the single position can be created.
        if real_start != start && chain.len() == 1 {
            positions.insert(0, real_start);
            node_costs.insert(0, 0.0);
        }
        Self { positions, movements: Vec::new(), node_costs, num_nodes, goal, verified: false, note: None }
    }

    pub fn src(&self) -> Pos {
        self.positions[0]
    }

    pub fn dest(&self) -> Pos {
        self.positions[self.positions.len() - 1]
    }

    pub fn length(&self) -> usize {
        self.positions.len()
    }

    /// IPath.ticksRemainingFrom.
    pub fn ticks_remaining_from(&self, path_position: usize) -> f64 {
        self.movements.iter().skip(path_position).map(Movement::known_cost).sum()
    }

    /// Path.runBackwards: the movement that goes from `src` to `dest`.
    fn run_backwards(ctx: &CalculationContext, src: Pos, dest: Pos, cost: f64) -> Option<Movement> {
        for moves in ALL {
            let mut movement = moves.apply0(ctx, src);
            if movement.dest == dest {
                // The cost is calculated now so a cost increase between the cached calculation and the real
                // execution can be judged; with favoring possibly skewing the node cost, the stricter of the two.
                let calculated = movement.calculate_cost(ctx);
                movement.override_cost(calculated.min(cost));
                return Some(movement);
            }
        }
        None // the movement became impossible during the calculation
    }

    /// Path.postProcess: the movements between the positions, cut off where one became impossible.
    pub fn post_process(mut self, ctx: &CalculationContext) -> Self {
        assert!(!self.verified, "Path must not be verified twice");
        self.verified = true;
        let mut movements = Vec::new();
        let mut failed = false;
        for i in 0..self.positions.len().saturating_sub(1) {
            let cost = self.node_costs[i + 1] - self.node_costs[i];
            match Self::run_backwards(ctx, self.positions[i], self.positions[i + 1], cost) {
                Some(m) => movements.push(m),
                None => {
                    let (src, dest) = (self.positions[i], self.positions[i + 1]);
                    self.note = Some(format!("Movement became impossible during calculation {src:?} {dest:?} {:?}", [dest[0] - src[0], dest[1] - src[1], dest[2] - src[2]]));
                    failed = true;
                    break;
                }
            }
        }
        self.movements = movements;
        if failed {
            let keep = self.movements.len();
            let note = self.note.take();
            let mut cut = self.cut_off(keep);
            cut.note = note;
            return cut;
        }
        self.sanity_check();
        self
    }

    /// CutoffPath(prev, lastPositionToInclude): the first `last + 1` positions and `last` movements.
    pub fn cut_off(&self, last_position_to_include: usize) -> Self {
        let last = last_position_to_include.min(self.positions.len() - 1);
        let cut = Self {
            positions: self.positions[..=last].to_vec(),
            movements: self.movements[..last.min(self.movements.len())].to_vec(),
            node_costs: self.node_costs[..=last].to_vec(),
            num_nodes: self.num_nodes,
            goal: self.goal.clone(),
            verified: true,
            note: None,
        };
        cut.sanity_check();
        cut
    }

    /// PathBase.cutoffAtLoadedChunks.
    pub fn cutoff_at_loaded_chunks(&self, enabled: bool, loaded: &dyn Fn(i32, i32) -> bool) -> Self {
        if !enabled {
            return self.clone();
        }
        for (i, p) in self.positions.iter().enumerate() {
            if !loaded(p[0], p[2]) {
                return self.cut_off(i);
            }
        }
        self.clone()
    }

    /// PathBase.staticCutoff.
    pub fn static_cutoff(&self, min: usize, factor: f64) -> Self {
        if self.length() < min {
            return self.clone();
        }
        if self.goal.is_in_goal(self.dest()) {
            return self.clone();
        }
        let new_length = ((self.length() - min) as f64 * factor) as usize + min - 1;
        self.cut_off(new_length)
    }

    /// SplicedPath.trySplice: `second` carried on from the end of `first`.
    pub fn try_splice(first: &Path, second: &Path, allow_overlap_cutoff: bool) -> Option<Path> {
        if first.dest() != second.src() {
            return None;
        }
        let second_pos: std::collections::HashSet<Pos> = second.positions.iter().copied().collect();
        // An overlap in the very last element is fine (and required), so only go up to length - 1.
        let mut first_position_in_second = (0..first.length() - 1).find(|&i| second_pos.contains(&first.positions[i]));
        match first_position_in_second {
            Some(_) if !allow_overlap_cutoff => return None,
            Some(_) => {}
            None => first_position_in_second = Some(first.length() - 1),
        }
        let first_in_second = first_position_in_second?;
        let position_in_second = second.positions.iter().position(|p| *p == first.positions[first_in_second])?;
        assert!(allow_overlap_cutoff || position_in_second == 0, "Paths to be spliced are overlapping incorrectly");
        let mut positions = first.positions[..=first_in_second].to_vec();
        let mut movements = first.movements[..first_in_second].to_vec();
        positions.extend_from_slice(&second.positions[position_in_second + 1..]);
        movements.extend_from_slice(&second.movements[position_in_second..second.length() - 1]);
        let node_costs = vec![0.0; positions.len()];
        let path = Path { positions, movements, node_costs, num_nodes: first.num_nodes + second.num_nodes, goal: first.goal.clone(), verified: true, note: None };
        path.sanity_check();
        Some(path)
    }

    /// IPath.sanityCheck.
    pub fn sanity_check(&self) {
        assert!(self.positions.len() == self.movements.len() + 1, "Size of path array is unexpected");
        let mut seen = std::collections::HashSet::new();
        for (i, p) in self.positions.iter().enumerate() {
            assert!(seen.insert(*p), "Path doubles back on itself, making a loop");
            if i + 1 < self.positions.len() {
                let m = &self.movements[i];
                assert!(m.src == *p, "Path source is not equal to the movement source");
                assert!(m.dest == self.positions[i + 1], "Path destination is not equal to the next movement destination");
            }
        }
    }
}
