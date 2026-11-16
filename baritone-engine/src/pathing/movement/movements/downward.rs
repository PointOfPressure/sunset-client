// baritone.pathing.movement.movements.MovementDownward, the cost: break the block under the feet and
// drop into its place (or climb down a ladder or vine).
use super::baritone_calculation_context::CalculationContext;
use super::baritone_costs::{fall_cost, COST_INF, LADDER_DOWN_ONE_COST};
use super::baritone_movement_helper as mh;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
    if !ctx.allow_downward {
        return COST_INF;
    }
    if !mh::can_walk_on_at(ctx, x, y - 2, z) {
        return COST_INF;
    }
    let down = ctx.get(x, y - 1, z);
    if down.is_block("ladder") || down.is_block("vine") {
        LADDER_DOWN_ONE_COST
    } else {
        // We are standing on it: while it might be a falling block, it will be air by the time we get here.
        fall_cost(1) + mh::get_mining_duration_ticks(ctx, x, y - 1, z, &down, false)
    }
}
