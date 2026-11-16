// baritone.pathing.movement.movements.MovementAscend, the cost: jump up one block onto the next one.
use super::baritone_block_state::Class;
use super::baritone_calculation_context::CalculationContext;
use super::baritone_costs::{jump_one_block_cost, COST_INF, SNEAK_ONE_BLOCK_COST, WALK_ONE_BLOCK_COST, WALK_ONE_OVER_SOUL_SAND_COST};
use super::baritone_movement_helper as mh;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32) -> f64 {
    let to_place = ctx.get(dest_x, y, dest_z);
    let mut additional_placement_cost = 0.0;
    if !mh::can_walk_on(ctx, dest_x, y, dest_z, &to_place) {
        additional_placement_cost = ctx.cost_of_placing_at(dest_x, y, dest_z, &to_place);
        if additional_placement_cost >= COST_INF {
            return COST_INF;
        }
        if !mh::is_replaceable(dest_x, y, dest_z, &to_place, ctx.bsi) {
            return COST_INF;
        }
        let mut found_place_option = false;
        for (dx, dy, dz) in mh::HORIZONTALS_BUT_ALSO_DOWN {
            let (against_x, against_y, against_z) = (dest_x + dx, y + dy, dest_z + dz);
            if against_x == x && against_z == z {
                // A backplace might be possible now, but it will have been broken by the time it is needed.
                continue;
            }
            if mh::can_place_against(ctx, against_x, against_y, against_z) {
                found_place_option = true;
                break;
            }
        }
        if !found_place_option {
            return COST_INF;
        }
    }
    let src_up2 = ctx.get(x, y + 2, z);
    if ctx.get(x, y + 3, z).is(Class::Falling) && (mh::can_walk_through_at(ctx, x, y + 1, z) || !src_up2.is(Class::Falling)) {
        // It would fall on us and possibly suffocate us. We assume we stand in the start position, so src
        // and src.up(1) are both air by the time this starts: if the lower one cannot be walked through and
        // the upper one is falling, then standing on src means the whole falling stack above was cleared.
        return COST_INF;
    }
    let src_down = ctx.get(x, y - 1, z);
    if mh::is_climbable(&src_down) {
        return COST_INF;
    }
    // We can jump from soul sand, but not from a bottom slab.
    let jumping_from_bottom_slab = mh::is_bottom_slab(&src_down);
    let jumping_to_bottom_slab = mh::is_bottom_slab(&to_place);
    if jumping_from_bottom_slab && !jumping_to_bottom_slab {
        return COST_INF; // the only thing to ascend onto from a bottom slab is another bottom slab
    }
    let jump = jump_one_block_cost();
    let walk = if jumping_to_bottom_slab {
        if jumping_from_bottom_slab {
            jump.max(WALK_ONE_BLOCK_COST) + ctx.jump_penalty // space is hit immediately on entering this action
        } else {
            WALK_ONE_BLOCK_COST // no space, just walk into the slab
        }
    } else {
        let base = if to_place.is_block("soul_sand") {
            WALK_ONE_OVER_SOUL_SAND_COST
        } else if to_place.is_block("magma_block") {
            SNEAK_ONE_BLOCK_COST
        } else {
            jump.max(WALK_ONE_BLOCK_COST)
        };
        base + ctx.jump_penalty
    };
    let mut total_cost = walk + additional_placement_cost;
    // Start with srcUp2, whose state is known. includeFalling is not needed because of the falling check above.
    total_cost += mh::get_mining_duration_ticks(ctx, x, y + 2, z, &src_up2, false);
    if total_cost >= COST_INF {
        return COST_INF;
    }
    total_cost += mh::get_mining_duration_ticks_at(ctx, dest_x, y + 1, dest_z, false);
    if total_cost >= COST_INF {
        return COST_INF;
    }
    total_cost += mh::get_mining_duration_ticks_at(ctx, dest_x, y + 2, dest_z, true);
    total_cost
}
