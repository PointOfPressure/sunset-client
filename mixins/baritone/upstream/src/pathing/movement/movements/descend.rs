// baritone.pathing.movement.movements.MovementDescend, the cost: step off an edge and drop one block,
// or (dynamic_fall_cost) drop further, into water, onto a ladder, or with a water bucket.
use super::baritone_block_state::{BlockState, Class};
use super::baritone_calculation_context::{CalculationContext, MutableMoveResult};
use super::baritone_costs::{fall_cost, CENTER_AFTER_FALL_COST, COST_INF, LADDER_DOWN_ONE_COST, WALK_OFF_BLOCK_COST, WALK_ONE_BLOCK_COST, WALK_ONE_OVER_SOUL_SAND_COST};
use super::baritone_movement_helper as mh;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32, res: &mut MutableMoveResult) {
    let mut total_cost = 0.0;
    let dest_down = ctx.get(dest_x, y - 1, dest_z);
    total_cost += mh::get_mining_duration_ticks(ctx, dest_x, y - 1, dest_z, &dest_down, false);
    if total_cost >= COST_INF {
        return;
    }
    total_cost += mh::get_mining_duration_ticks_at(ctx, dest_x, y, dest_z, false);
    if total_cost >= COST_INF {
        return;
    }
    total_cost += mh::get_mining_duration_ticks_at(ctx, dest_x, y + 1, dest_z, true); // only the top of the three blocks counts falling
    if total_cost >= COST_INF {
        return;
    }
    let from_down = ctx.get(x, y - 1, z);
    if mh::is_climbable(&from_down) {
        return;
    }
    // A
    //SA
    // A
    // B
    // C
    // D
    // If S is where you start, B needs to be air for a movementfall. A is plausibly breakable by either
    // descend or fall. C, D, etc. determine the length of the fall.
    let below = ctx.get(dest_x, y - 2, dest_z);
    if !mh::can_walk_on(ctx, dest_x, y - 2, dest_z, &below) {
        dynamic_fall_cost(ctx, x, y, z, dest_x, dest_z, total_cost, &below, res);
        return;
    }
    if dest_down.is_block("ladder") || dest_down.is_block("vine") {
        return;
    }
    if mh::can_use_frost_walker(ctx, &dest_down) {
        // No need to check assumeWalkOnWater: the water will freeze when we try to walk into it.
        return;
    }
    // We walk half the block plus 0.3 to the edge, then 0.2 more while falling at the same time.
    let mut walk = WALK_OFF_BLOCK_COST;
    if from_down.is_block("soul_sand") {
        // Apply the soul sand speed penalty to our 0.8 block distance.
        walk *= WALK_ONE_OVER_SOUL_SAND_COST / WALK_ONE_BLOCK_COST;
    }
    total_cost += walk + fall_cost(1).max(CENTER_AFTER_FALL_COST);
    res.x = dest_x;
    res.y = y - 1;
    res.z = dest_z;
    res.cost = total_cost;
}

/// The cost of falling from this step off an edge onto whatever is below. True when a water bucket is
/// needed for it.
#[allow(clippy::too_many_arguments)]
pub fn dynamic_fall_cost(ctx: &CalculationContext, _x: i32, y: i32, _z: i32, dest_x: i32, dest_z: i32, front_break: f64, below: &BlockState, res: &mut MutableMoveResult) -> bool {
    if front_break != 0.0 && ctx.get(dest_x, y + 2, dest_z).is(Class::Falling) {
        // If frontBreak is 0 this can be passed without updating the falling block. Otherwise blocks in
        // front are broken, so nothing may fall through this column and replace the water fallen into.
        return false;
    }
    if !mh::can_walk_through(ctx, dest_x, y - 2, dest_z, below) {
        return false;
    }
    let mut cost_so_far = 0.0;
    let mut effective_start_height = y;
    let mut fall_height = 3;
    loop {
        let new_y = y - fall_height;
        if new_y < ctx.min_y {
            // In the end, where the void can be fallen into: do not read below the lowest block.
            return false;
        }
        let reached_minimum = fall_height >= ctx.min_fall_height;
        let onto_block = ctx.get(dest_x, new_y, dest_z);
        let unprotected_fall_height = fall_height - (y - effective_start_height);
        let tentative_cost = WALK_OFF_BLOCK_COST + fall_cost(unprotected_fall_height) + front_break + cost_so_far;
        if reached_minimum && mh::is_water(&onto_block) && !mh::is_waterlogged_solid_top(&onto_block) {
            if !mh::can_walk_through(ctx, dest_x, new_y, dest_z, &onto_block) {
                return false;
            }
            if ctx.assume_walk_on_water {
                return false; // TODO fix
            }
            if mh::is_flowing(ctx.bsi, dest_x, new_y, dest_z, &onto_block) {
                return false; // TODO flowing check required here?
            }
            if !mh::can_walk_on_at(ctx, dest_x, new_y - 1, dest_z) {
                // We could punch right through the water into something else.
                return false;
            }
            // Found a fall into water.
            res.x = dest_x;
            res.y = new_y;
            res.z = dest_z;
            res.cost = tentative_cost; // TODO incorporate water swim up cost?
            return false;
        }
        if reached_minimum && ctx.allow_fall_into_lava && mh::is_lava(&onto_block) {
            // Found a fall into lava.
            res.x = dest_x;
            res.y = new_y;
            res.z = dest_z;
            res.cost = tentative_cost;
            return false;
        }
        if unprotected_fall_height <= 11 && mh::is_climbable(&onto_block) {
            // From a fall height of 11 up, ladders and vines are not grabbed. Grabbing one resets the speed.
            cost_so_far += fall_cost(unprotected_fall_height - 1); // fall until the top of this block
            cost_so_far += LADDER_DOWN_ONE_COST;
            effective_start_height = new_y;
            fall_height += 1;
            continue;
        }
        if mh::can_walk_through(ctx, dest_x, new_y, dest_z, &onto_block) {
            fall_height += 1;
            continue;
        }
        if !mh::can_walk_on(ctx, dest_x, new_y, dest_z, &onto_block) {
            return false;
        }
        if mh::is_bottom_slab(&onto_block) {
            return false; // falling onto a half slab is glitchy and can cause more fall damage than expected
        }
        if reached_minimum && unprotected_fall_height <= ctx.max_fall_height_no_water + 1 {
            // fallHeight 4 means onto.up() is 3 blocks down, which is the max.
            res.x = dest_x;
            res.y = new_y + 1;
            res.z = dest_z;
            res.cost = tentative_cost;
            return false;
        }
        if reached_minimum && ctx.has_water_bucket && unprotected_fall_height <= ctx.max_fall_height_bucket + 1 {
            res.x = dest_x;
            res.y = new_y + 1; // this is the block fallen onto, so the destination is one up
            res.z = dest_z;
            res.cost = tentative_cost + ctx.place_bucket_cost();
            return true;
        }
        return false;
    }
}
