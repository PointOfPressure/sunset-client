// baritone.pathing.movement.Movement and its eight subclasses, the data half: what kind of move it is, where
// it starts and ends, which blocks it has to break, where it has to place one, which positions the player
// may be in while it runs, and what it costs. How a movement is carried out tick by tick is in
// movement_exec.rs.
use super::baritone_calculation_context::{CalculationContext, MutableMoveResult};
use super::baritone_costs::COST_INF;
use super::baritone_moves::Moves;
use super::baritone_movement_descend as descend;
use super::baritone_movement_diagonal as diagonal;
use super::baritone_movement_downward as downward;
use super::baritone_movement_parkour as parkour;
use super::baritone_movement_pillar as pillar;
use super::baritone_movement_state::{Input, MovementState};
use super::baritone_utils::Pos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Traverse,
    Ascend,
    Descend,
    Fall,
    /// The two horizontal steps it is made of, as (dx, dz) each, and how far it rises or drops.
    Diagonal { dir1: (i32, i32), dir2: (i32, i32), dy: i32 },
    Downward,
    Pillar,
    Parkour { dir: (i32, i32), dist: i32, ascend: bool },
}

#[derive(Clone, Debug)]
pub struct Movement {
    pub kind: Kind,
    pub src: Pos,
    pub dest: Pos,
    pub positions_to_break: Vec<Pos>,
    pub position_to_place: Option<Pos>,
    cost: Option<f64>,
    /// `currentState`: the state the movement is in, carried from tick to tick.
    pub state: MovementState,
    /// Per-movement counters (numTicks, ticksWithoutPlacement) and flags (forceSafeMode, the bridge block).
    pub num_ticks: i32,
    pub ticks_without_placement: i32,
    pub force_safe_mode: bool,
    pub bridge_block_always_there: bool,
    pub calculated_while_loaded: bool,
    /// The inputs the last tick held, for `chatDebug` (the Java has its debugger for this).
    pub last_inputs: Vec<Input>,
}

fn above(p: Pos, n: i32) -> Pos {
    [p[0], p[1] + n, p[2]]
}

fn offset(p: Pos, d: (i32, i32)) -> Pos {
    [p[0] + d.0, p[1], p[2] + d.1]
}

impl Movement {
    fn new(kind: Kind, src: Pos, dest: Pos, to_break: Vec<Pos>, to_place: Option<Pos>) -> Self {
        Self {
            kind,
            src,
            dest,
            positions_to_break: to_break,
            position_to_place: to_place,
            cost: None,
            state: MovementState::new(),
            num_ticks: 0,
            ticks_without_placement: 0,
            force_safe_mode: false,
            bridge_block_always_there: true,
            calculated_while_loaded: true,
            last_inputs: Vec::new(),
        }
    }

    pub fn traverse(src: Pos, to: Pos) -> Self {
        Self::new(Kind::Traverse, src, to, vec![above(to, 1), to], Some(above(to, -1)))
    }

    pub fn ascend(src: Pos, dest: Pos) -> Self {
        Self::new(Kind::Ascend, src, dest, vec![dest, above(src, 2), above(dest, 1)], Some(above(dest, -1)))
    }

    pub fn descend(start: Pos, end: Pos) -> Self {
        Self::new(Kind::Descend, start, end, vec![above(end, 2), above(end, 1), end], Some(above(end, -1)))
    }

    pub fn fall(src: Pos, dest: Pos) -> Self {
        // MovementFall.buildPositionsToBreak
        let diff_x = src[0] - dest[0];
        let diff_z = src[2] - dest[2];
        let diff_y = (src[1] - dest[1]).abs();
        let to_break = (0..diff_y + 2).map(|i| [src[0] - diff_x, src[1] + 1 - i, src[2] - diff_z]).collect();
        Self::new(Kind::Fall, src, dest, to_break, None)
    }

    pub fn diagonal(start: Pos, dir1: (i32, i32), dir2: (i32, i32), dy: i32) -> Self {
        let d1 = offset(start, dir1);
        let d2 = offset(start, dir2);
        let end = above(offset(d1, dir2), dy);
        Self::new(Kind::Diagonal { dir1, dir2, dy }, start, end, vec![d1, above(d1, 1), d2, above(d2, 1), end, above(end, 1)], None)
    }

    pub fn downward(start: Pos, end: Pos) -> Self {
        Self::new(Kind::Downward, start, end, vec![end], None)
    }

    pub fn pillar(start: Pos, end: Pos) -> Self {
        Self::new(Kind::Pillar, start, end, vec![above(start, 2)], Some(start))
    }

    pub fn parkour(src: Pos, dist: i32, dir: (i32, i32), ascend: bool) -> Self {
        let landing = [src[0] + dir.0 * dist, src[1], src[2] + dir.1 * dist];
        let dest = above(landing, if ascend { 1 } else { 0 });
        let place = above(landing, if ascend { 0 } else { -1 });
        Self::new(Kind::Parkour { dir, dist, ascend }, src, dest, Vec::new(), Some(place))
    }

    /// The step the move goes in, as (dx, dz): the way the player faces while it runs.
    /// `reset`: back to preparing, with the counters cleared.
    pub fn reset(&mut self) {
        self.state = MovementState::new();
        self.num_ticks = 0;
        self.ticks_without_placement = 0;
        self.force_safe_mode = false;
        self.bridge_block_always_there = true;
    }

    pub fn direction(&self) -> (i32, i32) {
        let (dx, dz) = (self.dest[0] - self.src[0], self.dest[2] - self.src[2]);
        (dx.signum(), dz.signum())
    }

    /// `getCost`, working it out the first time.
    pub fn cost(&mut self, ctx: &CalculationContext) -> f64 {
        if let Some(c) = self.cost {
            return c;
        }
        let c = self.calculate_cost(ctx);
        self.cost = Some(c);
        c
    }

    /// `getCost()` without a context: what was worked out or overridden already.
    pub fn known_cost(&self) -> f64 {
        self.cost.unwrap_or(COST_INF)
    }

    pub fn recalculate_cost(&mut self, ctx: &CalculationContext) -> f64 {
        self.cost = None;
        self.cost(ctx)
    }

    /// `override`: set the cost without working it out.
    pub fn override_cost(&mut self, cost: f64) {
        self.cost = Some(cost);
    }

    pub fn calculate_cost(&self, ctx: &CalculationContext) -> f64 {
        let [x, y, z] = self.src;
        let mut res = MutableMoveResult::default();
        match self.kind {
            Kind::Traverse => Moves::TraverseNorth.cost_to(ctx, x, y, z, self.dest[0], self.dest[2]),
            Kind::Ascend => Moves::AscendNorth.cost_to(ctx, x, y, z, self.dest[0], self.dest[2]),
            Kind::Descend | Kind::Fall => {
                descend::cost(ctx, x, y, z, self.dest[0], self.dest[2], &mut res);
                if res.y != self.dest[1] {
                    return COST_INF; // a fall where this is a descend, or the other way round
                }
                res.cost
            }
            Kind::Diagonal { .. } => {
                diagonal::cost(ctx, x, y, z, self.dest[0], self.dest[2], &mut res);
                if res.y != self.dest[1] {
                    return COST_INF; // doesn't apply: this position is incorrect
                }
                res.cost
            }
            Kind::Downward => downward::cost(ctx, x, y, z),
            Kind::Pillar => pillar::cost(ctx, x, y, z),
            Kind::Parkour { dir, .. } => {
                parkour::cost(ctx, x, y, z, dir, &mut res);
                if res.x != self.dest[0] || res.y != self.dest[1] || res.z != self.dest[2] {
                    return COST_INF;
                }
                res.cost
            }
        }
    }

    /// `calculateValidPositions`: where the player may be while this runs.
    pub fn valid_positions(&self) -> Vec<Pos> {
        let (src, dest) = (self.src, self.dest);
        match self.kind {
            Kind::Traverse | Kind::Downward | Kind::Pillar => vec![src, dest],
            Kind::Ascend => {
                let (dx, dz) = self.direction();
                // Sometimes we back up to place the block, also sprint ascends, also skip descend to straight ascend:
                // src.subtract(direction).above(), the direction having its y of 1.
                let prior = above([src[0] - dx, src[1] - 1, src[2] - dz], 1);
                vec![src, above(src, 1), dest, prior, above(prior, 1)]
            }
            Kind::Descend => vec![src, above(dest, 1), dest],
            Kind::Fall => {
                let mut set = vec![src];
                for y in (0..=src[1] - dest[1]).rev() {
                    set.push(above(dest, y));
                }
                set
            }
            Kind::Diagonal { .. } => {
                let diag_a = [src[0], src[1], dest[2]];
                let diag_b = [dest[0], src[1], src[2]];
                if dest[1] < src[1] {
                    vec![src, above(dest, 1), diag_a, diag_b, dest, above(diag_a, -1), above(diag_b, -1)]
                } else if dest[1] > src[1] {
                    vec![src, above(src, 1), diag_a, diag_b, dest, above(diag_a, 1), above(diag_b, 1)]
                } else {
                    vec![src, dest, diag_a, diag_b]
                }
            }
            Kind::Parkour { dir, dist, .. } => {
                let mut set = Vec::new();
                for i in 0..=dist {
                    for y in 0..2 {
                        set.push([src[0] + dir.0 * i, src[1] + y, src[2] + dir.1 * i]);
                    }
                }
                set
            }
        }
    }
}

impl Moves {
    /// The cost of a traverse or ascend from (x, y, z) to the column (dest_x, dest_z).
    fn cost_to(self, ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32) -> f64 {
        match self {
            Moves::TraverseNorth | Moves::TraverseSouth | Moves::TraverseEast | Moves::TraverseWest => {
                super::baritone_movement_traverse::cost(ctx, x, y, z, dest_x, dest_z)
            }
            _ => super::baritone_movement_ascend::cost(ctx, x, y, z, dest_x, dest_z),
        }
    }

    /// `apply0`: the movement this move makes from `src`.
    pub fn apply0(self, ctx: &CalculationContext, src: Pos) -> Movement {
        use Moves::*;
        let [x, y, z] = src;
        match self {
            Downward => Movement::downward(src, [x, y - 1, z]),
            Pillar => Movement::pillar(src, [x, y + 1, z]),
            TraverseNorth => Movement::traverse(src, [x, y, z - 1]),
            TraverseSouth => Movement::traverse(src, [x, y, z + 1]),
            TraverseEast => Movement::traverse(src, [x + 1, y, z]),
            TraverseWest => Movement::traverse(src, [x - 1, y, z]),
            AscendNorth => Movement::ascend(src, [x, y + 1, z - 1]),
            AscendSouth => Movement::ascend(src, [x, y + 1, z + 1]),
            AscendEast => Movement::ascend(src, [x + 1, y + 1, z]),
            AscendWest => Movement::ascend(src, [x - 1, y + 1, z]),
            DescendEast | DescendWest | DescendNorth | DescendSouth => {
                let mut res = MutableMoveResult::default();
                self.apply(ctx, x, y, z, &mut res);
                if res.y == y - 1 {
                    Movement::descend(src, [res.x, res.y, res.z])
                } else {
                    Movement::fall(src, [res.x, res.y, res.z])
                }
            }
            DiagonalNortheast | DiagonalNorthwest | DiagonalSoutheast | DiagonalSouthwest => {
                let mut res = MutableMoveResult::default();
                self.apply(ctx, x, y, z, &mut res);
                let (dir1, dir2) = match self {
                    DiagonalNortheast => ((0, -1), (1, 0)),
                    DiagonalNorthwest => ((0, -1), (-1, 0)),
                    DiagonalSoutheast => ((0, 1), (1, 0)),
                    _ => ((0, 1), (-1, 0)),
                };
                Movement::diagonal(src, dir1, dir2, res.y - y)
            }
            ParkourNorth | ParkourSouth | ParkourEast | ParkourWest => {
                let dir = match self {
                    ParkourNorth => (0, -1),
                    ParkourSouth => (0, 1),
                    ParkourEast => (1, 0),
                    _ => (-1, 0),
                };
                let mut res = MutableMoveResult::default();
                self.apply(ctx, x, y, z, &mut res);
                let dist = (res.x - x).abs() + (res.z - z).abs();
                Movement::parkour(src, dist, dir, res.y > y)
            }
        }
    }
}
