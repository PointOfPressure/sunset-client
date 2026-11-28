// baritone.pathing.movement.movements.MovementDiagonal, the cost: one step across a corner, possibly
// one up or one down, either way round the corner that is free (or can be broken).
use super::baritone_block_state::{BlockState, Class};
use super::baritone_calculation_context::{CalculationContext, MutableMoveResult};
use super::baritone_costs::{
    fall_cost, jump_one_block_cost, CENTER_AFTER_FALL_COST, SNEAK_ONE_BLOCK_COST, SPRINT_MULTIPLIER, WALK_ONE_BLOCK_COST, WALK_ONE_OVER_SOUL_SAND_COST,
};
use super::baritone_movement_helper as mh;

const SQRT_2: f64 = std::f64::consts::SQRT_2;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32, res: &mut MutableMoveResult) {
    let dest_into_upper = ctx.get(dest_x, y + 1, dest_z);
    if !mh::can_walk_through(ctx, dest_x, y + 1, dest_z, &dest_into_upper) {
        return;
    }
    let dest_into = ctx.get(dest_x, y, dest_z);
    let from_down;
    let mut ascend = false;
    let dest_walk_on;
    let mut descend = false;
    let mut frost_walker = false;
    let mut sneaking = false;
    if !mh::can_walk_through(ctx, dest_x, y, dest_z, &dest_into) {
        ascend = true;
        if !ctx.allow_diagonal_ascend
            || !mh::can_walk_through_at(ctx, x, y + 2, z)
            || !mh::can_walk_on(ctx, dest_x, y, dest_z, &dest_into)
            || !mh::can_walk_through_at(ctx, dest_x, y + 2, dest_z)
        {
            return;
        }
        dest_walk_on = dest_into.clone();
        from_down = ctx.get(x, y - 1, z);
    } else {
        dest_walk_on = ctx.get(dest_x, y - 1, dest_z);
        from_down = ctx.get(x, y - 1, z);
        let standing_on_a_block = mh::must_be_solid_to_walk_on(ctx, x, y - 1, z, &from_down);
        frost_walker = standing_on_a_block && mh::can_use_frost_walker(ctx, &dest_walk_on);
        if !frost_walker && !mh::can_walk_on(ctx, dest_x, y - 1, dest_z, &dest_walk_on) {
            descend = true;
            if !ctx.allow_diagonal_descend || !mh::can_walk_on_at(ctx, dest_x, y - 2, dest_z) || !mh::can_walk_through(ctx, dest_x, y - 1, dest_z, &dest_walk_on) {
                return;
            }
        }
        // Do this after checking for descends: jesus cannot stop the water freezing, it only stops us relying on it.
        frost_walker &= !ctx.assume_walk_on_water;
    }
    let start_state = ctx.get(x, y, z);
    if is_blocking_door(&start_state, x, z, dest_x, dest_z) || is_blocking_door(if ascend { &dest_into_upper } else { &dest_into }, dest_x, dest_z, x, y) {
        return; // easier to just traverse instead
    }
    let mut multiplier = WALK_ONE_BLOCK_COST;
    // For either possible soul sand, that affects half of our walking.
    if dest_walk_on.is_block("soul_sand") {
        multiplier += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
    } else if ctx.allow_walk_on_magma_blocks && dest_walk_on.is_block("magma_block") {
        multiplier += (SNEAK_ONE_BLOCK_COST - WALK_ONE_BLOCK_COST) / 2.0;
        sneaking = true;
    } else if frost_walker {
        // Frostwalker lets us walk on water without the penalty.
    } else if dest_walk_on.is_block("water") {
        multiplier += ctx.walk_on_water_one_penalty * SQRT_2;
    }
    if mh::is_climbable(&from_down) {
        return;
    }
    if from_down.is_block("soul_sand") {
        multiplier += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
    } else if ctx.allow_walk_on_magma_blocks && from_down.is_block("magma_block") {
        multiplier += (SNEAK_ONE_BLOCK_COST - WALK_ONE_BLOCK_COST) / 2.0;
        sneaking = true;
    }
    let cutting_over1 = ctx.get(x, y - 1, dest_z);
    if (!ctx.allow_walk_on_magma_blocks && cutting_over1.is_block("magma_block")) || mh::is_lava(&cutting_over1) {
        return;
    }
    let cutting_over2 = ctx.get(dest_x, y - 1, z);
    if (!ctx.allow_walk_on_magma_blocks && cutting_over2.is_block("magma_block")) || mh::is_lava(&cutting_over2) {
        return;
    }
    let mut water = false;
    if mh::is_water(&start_state) || mh::is_water(&dest_into) {
        if ascend {
            return;
        }
        // Ignore the previous multiplier: whatever we walk on does not matter, as we float on the water.
        multiplier = ctx.water_walk_speed;
        water = true;
    }
    let pb0 = ctx.get(x, y, dest_z);
    let pb2 = ctx.get(dest_x, y, z);
    if ascend {
        let a_top = mh::can_walk_through_at(ctx, x, y + 2, dest_z);
        let a_mid = mh::can_walk_through_at(ctx, x, y + 1, dest_z);
        let mut a_low = mh::can_walk_through(ctx, x, y, dest_z, &pb0);
        let b_top = mh::can_walk_through_at(ctx, dest_x, y + 2, z);
        let b_mid = mh::can_walk_through_at(ctx, dest_x, y + 1, z);
        let mut b_low = mh::can_walk_through(ctx, dest_x, y, z, &pb2);
        a_low &= !is_blocking_door(&pb0, x, dest_z, dest_x, z);
        b_low &= !is_blocking_door(&pb2, dest_x, z, x, dest_z);
        if (!(a_top && a_mid && a_low) && !(b_top && b_mid && b_low)) // no option
            || mh::avoid_walking_into(ctx, &pb0) // bad
            || mh::avoid_walking_into(ctx, &pb2) // bad
            || (a_top && a_mid && mh::can_walk_on(ctx, x, y, dest_z, &pb0)) // we could just ascend
            || (b_top && b_mid && mh::can_walk_on(ctx, dest_x, y, z, &pb2)) // we could just ascend
            || (!a_top && a_mid && a_low) // head bonk A
            || (!b_top && b_mid && b_low)
        // head bonk B
        {
            return;
        }
        res.cost = multiplier * SQRT_2 + jump_one_block_cost() + ctx.jump_penalty;
        res.x = dest_x;
        res.z = dest_z;
        res.y = y + 1;
        return;
    }
    let mut option_a = mh::get_mining_duration_ticks(ctx, x, y, dest_z, &pb0, false);
    let mut option_b = mh::get_mining_duration_ticks(ctx, dest_x, y, z, &pb2, false);
    option_a += if is_blocking_door(&pb0, x, dest_z, dest_x, z) { 1.0 } else { 0.0 };
    option_b += if is_blocking_door(&pb2, dest_x, z, x, dest_z) { 1.0 } else { 0.0 };
    if option_a != 0.0 && option_b != 0.0 {
        // Checked one at a time: if pb0 and pb2 were nonzero, we already know the answer, so return early.
        return;
    }
    let pb1 = ctx.get(x, y + 1, dest_z);
    option_a += mh::get_mining_duration_ticks(ctx, x, y + 1, dest_z, &pb1, true);
    if option_a != 0.0 && option_b != 0.0 {
        // Same: if pb1 makes optionA nonzero and optionB already was nonzero, pb3 cannot affect the result.
        return;
    }
    let pb3 = ctx.get(dest_x, y + 1, z);
    if option_a == 0.0 && ((mh::avoid_walking_into(ctx, &pb2) && !pb2.is_block("water")) || mh::avoid_walking_into(ctx, &pb3)) {
        // optionA is complete here, so check whether it is possible to edge around in that direction.
        return;
    }
    option_b += mh::get_mining_duration_ticks(ctx, dest_x, y + 1, z, &pb3, true);
    if option_a != 0.0 && option_b != 0.0 {
        // If the cost is nonzero for both ways to approach this diagonal, it is not possible.
        return;
    }
    if option_b == 0.0 && ((mh::avoid_walking_into(ctx, &pb0) && !pb0.is_block("water")) || mh::avoid_walking_into(ctx, &pb1)) {
        // optionB is complete here, so see if we can edge around that way.
        return;
    }
    if option_a != 0.0 || option_b != 0.0 {
        multiplier *= SQRT_2 - 0.001; // TODO tune
        if mh::is_climbable(&start_state) {
            // Edging around does not work if it would climb a ladder or vine instead of moving sideways.
            return;
        }
    } else if ctx.can_sprint && !water && !sneaking {
        // Only sprint if not edging around anything and not in water. Soul sand is not checked: sprinting works on it.
        multiplier *= SPRINT_MULTIPLIER;
    }
    res.cost = multiplier * SQRT_2;
    if descend {
        res.cost += fall_cost(1).max(CENTER_AFTER_FALL_COST);
        res.y = y - 1;
    } else {
        res.y = y;
    }
    res.x = dest_x;
    res.z = dest_z;
}

/// Whether this door, at (door_x, door_z), is the one in the way of going to (other_x, other_z).
pub fn is_blocking_door(state: &BlockState, door_x: i32, door_z: i32, other_x: i32, other_z: i32) -> bool {
    if !state.is(Class::Door) {
        return false;
    }
    let (ox, oz) = facing_step(state.prop("facing").unwrap_or("north"));
    let (nbr_x, nbr_z) = if state.prop("hinge") == Some("left") {
        (door_x - ox + oz, door_z - oz - ox)
    } else {
        (door_x - ox - oz, door_z - oz + ox)
    };
    nbr_x == other_x && nbr_z == other_z
}

/// Direction.getUnitVec3i of a horizontal facing, as (x, z).
pub fn facing_step(facing: &str) -> (i32, i32) {
    match facing {
        "north" => (0, -1),
        "south" => (0, 1),
        "east" => (1, 0),
        "west" => (-1, 0),
        _ => (0, 0),
    }
}
