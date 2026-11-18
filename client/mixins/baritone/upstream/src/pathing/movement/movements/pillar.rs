// baritone.pathing.movement.movements.MovementPillar, the cost: jump and place a block under the feet,
// or climb a ladder or vine, or swim up a column of water.
use super::baritone_block_state::Class;
use super::baritone_calculation_context::CalculationContext;
use super::baritone_costs::{jump_one_block_cost, COST_INF, LADDER_UP_ONE_COST};
use super::baritone_movement_helper as mh;

pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
    let from_state = ctx.get(x, y, z);
    let ladder = mh::is_climbable(&from_state);
    let from_down = ctx.get(x, y - 1, z);
    if !ladder {
        if mh::is_climbable(&from_down) {
            return COST_INF; // cannot pillar from a ladder or vine onto something that is not also climbable
        }
        if from_down.is(Class::Slab) && from_down.prop("type") == Some("bottom") {
            return COST_INF; // cannot pillar up from a bottom slab onto a non ladder
        }
    }
    let to_break = ctx.get(x, y + 2, z);
    if to_break.is(Class::FenceGate) {
        // See Baritone issue 172.
        return COST_INF;
    }
    let mut src_up = None;
    if mh::is_water(&to_break) && mh::is_water(&from_state) {
        // TODO should this also be allowed if toBreakBlock is air?
        let up = ctx.get(x, y + 1, z);
        if mh::is_water(&up) {
            return LADDER_UP_ONE_COST; // ascending pillars of water are allowed, but only when already in one
        }
        src_up = Some(up);
    }
    let mut place_cost = 0.0;
    if !ladder {
        // A block has to be placed where we started, to jump on it.
        place_cost = ctx.cost_of_placing_at(x, y, z, &from_state);
        if place_cost >= COST_INF {
            return COST_INF;
        }
        if from_down.is(Class::Air) {
            place_cost += 0.1; // slightly (1/200th of a second) penalize pillaring on what is currently air
        }
    }
    if (mh::is_liquid(&from_state) && !mh::can_place_against_state(ctx, x, y - 1, z, &from_down))
        || (mh::is_liquid(&from_down) && !mh::is_waterlogged_solid_top(&from_down) && ctx.assume_walk_on_water)
    {
        // Standing in water, we cannot pillar. Standing on water with assumeWalkOnWater, we cannot pillar.
        // Standing on water without it, we must have ascended to here or sneak backplaced, so we can pillar again.
        return COST_INF;
    }
    if (from_state.is_block("lily_pad") || from_state.is(Class::Carpet)) && from_down.has_fluid() {
        // To ascend here we would have to break the block we are standing on.
        return COST_INF;
    }
    let mut hardness = mh::get_mining_duration_ticks(ctx, x, y + 2, z, &to_break, true);
    if hardness >= COST_INF {
        return COST_INF;
    }
    if hardness != 0.0 {
        if mh::is_climbable(&to_break) {
            hardness = 0.0; // the ladder or vine is not broken: it is going to be used
        } else {
            let check = ctx.get(x, y + 3, z); // the block on top of the one to break: could it fall on us?
            if check.is(Class::Falling) {
                // See MovementAscend's identical check for breaking a falling block above our head.
                let src_up = match src_up {
                    Some(s) => s,
                    None => ctx.get(x, y + 1, z),
                };
                if !to_break.is(Class::Falling) || !src_up.is(Class::Falling) {
                    return COST_INF;
                }
            }
        }
    }
    if ladder {
        LADDER_UP_ONE_COST + hardness * 5.0
    } else {
        jump_one_block_cost() + place_cost + ctx.jump_penalty + hardness
    }
}
