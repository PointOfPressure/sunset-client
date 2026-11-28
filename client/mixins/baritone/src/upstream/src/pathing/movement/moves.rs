// baritone.pathing.movement.Moves: every move there is, with the offset it goes by when it is fixed and
// the function that works out where it ends and what it costs. The search asks every move of every
// position it expands; a move that cannot be made leaves the result at COST_INF.
use super::baritone_calculation_context::{CalculationContext, MutableMoveResult};
use super::baritone_costs::COST_INF;
use super::baritone_movement_ascend as ascend;
use super::baritone_movement_descend as descend;
use super::baritone_movement_diagonal as diagonal;
use super::baritone_movement_downward as downward;
use super::baritone_movement_parkour as parkour;
use super::baritone_movement_pillar as pillar;
use super::baritone_movement_traverse as traverse;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moves {
    Downward,
    Pillar,
    TraverseNorth,
    TraverseSouth,
    TraverseEast,
    TraverseWest,
    AscendNorth,
    AscendSouth,
    AscendEast,
    AscendWest,
    DescendEast,
    DescendWest,
    DescendNorth,
    DescendSouth,
    DiagonalNortheast,
    DiagonalNorthwest,
    DiagonalSoutheast,
    DiagonalSouthwest,
    ParkourNorth,
    ParkourSouth,
    ParkourEast,
    ParkourWest,
}

/// In the order of the Java enum: the order the search expands them in decides which of two equally
/// cheap paths it finds.
pub const ALL: [Moves; 22] = [
    Moves::Downward,
    Moves::Pillar,
    Moves::TraverseNorth,
    Moves::TraverseSouth,
    Moves::TraverseEast,
    Moves::TraverseWest,
    Moves::AscendNorth,
    Moves::AscendSouth,
    Moves::AscendEast,
    Moves::AscendWest,
    Moves::DescendEast,
    Moves::DescendWest,
    Moves::DescendNorth,
    Moves::DescendSouth,
    Moves::DiagonalNortheast,
    Moves::DiagonalNorthwest,
    Moves::DiagonalSoutheast,
    Moves::DiagonalSouthwest,
    Moves::ParkourNorth,
    Moves::ParkourSouth,
    Moves::ParkourEast,
    Moves::ParkourWest,
];

impl Moves {
    /// The offset of a fixed move (x, y, z) and whether its x/z or y is decided by the world instead.
    pub fn offsets(self) -> (i32, i32, i32, bool, bool) {
        use Moves::*;
        match self {
            Downward => (0, -1, 0, false, false),
            Pillar => (0, 1, 0, false, false),
            TraverseNorth => (0, 0, -1, false, false),
            TraverseSouth => (0, 0, 1, false, false),
            TraverseEast => (1, 0, 0, false, false),
            TraverseWest => (-1, 0, 0, false, false),
            AscendNorth => (0, 1, -1, false, false),
            AscendSouth => (0, 1, 1, false, false),
            AscendEast => (1, 1, 0, false, false),
            AscendWest => (-1, 1, 0, false, false),
            DescendEast => (1, -1, 0, false, true),
            DescendWest => (-1, -1, 0, false, true),
            DescendNorth => (0, -1, -1, false, true),
            DescendSouth => (0, -1, 1, false, true),
            DiagonalNortheast => (1, 0, -1, false, true),
            DiagonalNorthwest => (-1, 0, -1, false, true),
            DiagonalSoutheast => (1, 0, 1, false, true),
            DiagonalSouthwest => (-1, 0, 1, false, true),
            ParkourNorth => (0, 0, -4, true, true),
            ParkourSouth => (0, 0, 4, true, true),
            ParkourEast => (4, 0, 0, true, true),
            ParkourWest => (-4, 0, 0, true, true),
        }
    }

    #[allow(dead_code)]
    pub fn dynamic_xz(self) -> bool {
        self.offsets().3
    }

    #[allow(dead_code)]
    pub fn dynamic_y(self) -> bool {
        self.offsets().4
    }

    /// Moves.apply: where the move from (x, y, z) ends and what it costs, in `result`.
    pub fn apply(self, ctx: &CalculationContext, x: i32, y: i32, z: i32, result: &mut MutableMoveResult) {
        self.apply_unfloored(ctx, x, y, z, result);
        if ctx.floor_y.is_some_and(|floor| result.y < floor) {
            result.cost = COST_INF;
        }
    }

    fn apply_unfloored(self, ctx: &CalculationContext, x: i32, y: i32, z: i32, result: &mut MutableMoveResult) {
        use Moves::*;
        match self {
            DescendEast => descend::cost(ctx, x, y, z, x + 1, z, result),
            DescendWest => descend::cost(ctx, x, y, z, x - 1, z, result),
            DescendNorth => descend::cost(ctx, x, y, z, x, z - 1, result),
            DescendSouth => descend::cost(ctx, x, y, z, x, z + 1, result),
            DiagonalNortheast => diagonal::cost(ctx, x, y, z, x + 1, z - 1, result),
            DiagonalNorthwest => diagonal::cost(ctx, x, y, z, x - 1, z - 1, result),
            DiagonalSoutheast => diagonal::cost(ctx, x, y, z, x + 1, z + 1, result),
            DiagonalSouthwest => diagonal::cost(ctx, x, y, z, x - 1, z + 1, result),
            ParkourNorth => parkour::cost(ctx, x, y, z, (0, -1), result),
            ParkourSouth => parkour::cost(ctx, x, y, z, (0, 1), result),
            ParkourEast => parkour::cost(ctx, x, y, z, (1, 0), result),
            ParkourWest => parkour::cost(ctx, x, y, z, (-1, 0), result),
            fixed => {
                let (dx, dy, dz, _, _) = fixed.offsets();
                result.x = x + dx;
                result.y = y + dy;
                result.z = z + dz;
                result.cost = fixed.cost(ctx, x, y, z);
            }
        }
    }

    /// The cost of a fixed move.
    pub fn cost(self, ctx: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
        use Moves::*;
        match self {
            Downward => downward::cost(ctx, x, y, z),
            Pillar => pillar::cost(ctx, x, y, z),
            TraverseNorth => traverse::cost(ctx, x, y, z, x, z - 1),
            TraverseSouth => traverse::cost(ctx, x, y, z, x, z + 1),
            TraverseEast => traverse::cost(ctx, x, y, z, x + 1, z),
            TraverseWest => traverse::cost(ctx, x, y, z, x - 1, z),
            AscendNorth => ascend::cost(ctx, x, y, z, x, z - 1),
            AscendSouth => ascend::cost(ctx, x, y, z, x, z + 1),
            AscendEast => ascend::cost(ctx, x, y, z, x + 1, z),
            AscendWest => ascend::cost(ctx, x, y, z, x - 1, z),
            _ => COST_INF,
        }
    }
}
