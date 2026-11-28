// baritone.pathing.movement.movements.MovementTraverse, the cost: walk one block along a side, or place
// the block to walk on (bridge).
use super::baritone_block_state::{Class, FluidKind};
use super::baritone_calculation_context::CalculationContext;
use super::baritone_costs::{COST_INF, SNEAK_ONE_BLOCK_COST, SPRINT_MULTIPLIER, WALK_ONE_BLOCK_COST, WALK_ONE_OVER_SOUL_SAND_COST};
use super::baritone_movement_helper as mh;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dest_x: i32, dest_z: i32) -> f64 {
    let pb0 = ctx.get(dest_x, y + 1, dest_z);
    let pb1 = ctx.get(dest_x, y, dest_z);
    let dest_on = ctx.get(dest_x, y - 1, dest_z);
    let src_down = ctx.get(x, y - 1, z);
    let standing_on_a_block = mh::must_be_solid_to_walk_on(ctx, x, y - 1, z, &src_down);
    let frost_walker = standing_on_a_block && !ctx.assume_walk_on_water && mh::can_use_frost_walker(ctx, &dest_on);
    if frost_walker || mh::can_walk_on(ctx, dest_x, y - 1, dest_z, &dest_on) {
        // This is a walk, not a bridge.
        let mut wc = WALK_ONE_BLOCK_COST;
        let mut water = false;
        let mut sneaking = false;
        if mh::is_water(&pb0) || mh::is_water(&pb1) {
            wc = ctx.water_walk_speed;
            water = true;
        } else {
            if dest_on.is_block("soul_sand") {
                wc += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
            } else if frost_walker {
                // With frostwalker we can walk on water without the penalty, if sure we will not use jesus.
            } else if dest_on.is_block("water") {
                wc += ctx.walk_on_water_one_penalty;
            }
            if src_down.is_block("soul_sand") {
                wc += (WALK_ONE_OVER_SOUL_SAND_COST - WALK_ONE_BLOCK_COST) / 2.0;
            } else if ctx.allow_walk_on_magma_blocks && src_down.is_block("magma_block") {
                sneaking = true;
                wc += (SNEAK_ONE_BLOCK_COST - WALK_ONE_BLOCK_COST) / 2.0;
            }
        }
        let mut hardness1 = mh::get_mining_duration_ticks(ctx, dest_x, y, dest_z, &pb1, false);
        if hardness1 >= COST_INF {
            return COST_INF;
        }
        let mut hardness2 = mh::get_mining_duration_ticks(ctx, dest_x, y + 1, dest_z, &pb0, true); // only the upper block counts falling
        if hardness1 == 0.0 && hardness2 == 0.0 {
            if !water && !sneaking && ctx.can_sprint {
                // Nothing in the way, not water, not sneak placing: we can sprint.
                wc *= SPRINT_MULTIPLIER;
            }
            return wc;
        }
        if mh::is_climbable(&src_down) {
            hardness1 *= 5.0;
            hardness2 *= 5.0;
        }
        wc + hardness1 + hardness2
    } else {
        // This is a bridge, so a block has to be placed.
        if mh::is_climbable(&src_down) {
            return COST_INF;
        }
        if mh::is_replaceable(dest_x, y - 1, dest_z, &dest_on, ctx.bsi) {
            let through_water = mh::is_water(&pb0) || mh::is_water(&pb1);
            if mh::is_water(&dest_on) && through_water {
                // Happens with assumeWalkOnWater on and a traverse in water, which is not allowed.
                return COST_INF;
            }
            let place_cost = ctx.cost_of_placing_at(dest_x, y - 1, dest_z, &dest_on);
            if place_cost >= COST_INF {
                return COST_INF;
            }
            let hardness1 = mh::get_mining_duration_ticks(ctx, dest_x, y, dest_z, &pb1, false);
            if hardness1 >= COST_INF {
                return COST_INF;
            }
            let hardness2 = mh::get_mining_duration_ticks(ctx, dest_x, y + 1, dest_z, &pb0, true);
            let mut wc = if through_water { ctx.water_walk_speed } else { WALK_ONE_BLOCK_COST };
            for (dx, dy, dz) in mh::HORIZONTALS_BUT_ALSO_DOWN {
                let (against_x, against_y, against_z) = (dest_x + dx, y - 1 + dy, dest_z + dz);
                if against_x == x && against_z == z {
                    // This would be a backplace.
                    continue;
                }
                if mh::can_place_against(ctx, against_x, against_y, against_z) {
                    // Found a side place option.
                    return wc + place_cost + hardness1 + hardness2;
                }
            }
            // All the directions to side place are checked: a backplace is needed.
            if src_down.is_block("soul_sand") || (src_down.is(Class::Slab) && src_down.prop("type") != Some("double")) {
                return COST_INF; // can not sneak and backplace against soul sand or half slabs
            }
            if !standing_on_a_block {
                // Standing on water or swimming.
                return COST_INF;
            }
            let block_src = ctx.get(x, y, z);
            if (block_src.is_block("lily_pad") || block_src.is(Class::Carpet)) && src_down.fluid != FluidKind::None {
                return COST_INF; // we can stand on these but cannot place against them
            }
            wc *= SNEAK_ONE_BLOCK_COST / WALK_ONE_BLOCK_COST; // sneak backplacing
            return wc + place_cost + hardness1 + hardness2;
        }
        COST_INF
    }
}
