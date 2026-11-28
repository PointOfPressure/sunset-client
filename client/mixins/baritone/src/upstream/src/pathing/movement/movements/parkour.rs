// baritone.pathing.movement.movements.MovementParkour, the cost: a sprint jump over a gap of one to
// three blocks (or onto a block one higher), or onto a block placed in mid-jump.
use super::baritone_calculation_context::{CalculationContext, MutableMoveResult};
use super::baritone_costs::{COST_INF, SPRINT_ONE_BLOCK_COST, WALK_ONE_BLOCK_COST};
use super::baritone_block_state::Class;
use super::baritone_movement_helper as mh;

/// `dir` is the horizontal step (dx, dz) of the jump.
pub fn cost(ctx: &CalculationContext, x: i32, y: i32, z: i32, dir: (i32, i32), res: &mut MutableMoveResult) {
    if !ctx.allow_parkour {
        return;
    }
    if !ctx.allow_jump_at_build_limit && y >= ctx.max_y {
        return;
    }
    let (x_diff, z_diff) = dir;
    if !mh::fully_passable(ctx, x + x_diff, y, z + z_diff) {
        // Most common case at the top: the adjacent block is not air.
        return;
    }
    let adj = ctx.get(x + x_diff, y - 1, z + z_diff);
    if mh::can_walk_on(ctx, x + x_diff, y - 1, z + z_diff, &adj) {
        // Do not parkour if we could just traverse (for now).
        return;
    }
    if mh::avoid_walking_into(ctx, &adj) && !mh::is_water(&adj) {
        // Magma sucks.
        return;
    }
    if !mh::fully_passable(ctx, x + x_diff, y + 1, z + z_diff) {
        return;
    }
    if !mh::fully_passable(ctx, x + x_diff, y + 2, z + z_diff) {
        return;
    }
    if !mh::fully_passable(ctx, x, y + 2, z) {
        return;
    }
    let standing_on = ctx.get(x, y - 1, z);
    if mh::is_climbable(&standing_on) || standing_on.is(Class::Stair) || mh::is_bottom_slab(&standing_on) {
        return;
    }
    // We cannot jump from (frozen) water with assumeWalkOnWater: we cannot be sure it will be frozen.
    if ctx.assume_walk_on_water && standing_on.has_fluid() && !mh::is_waterlogged_solid_top(&standing_on) {
        return;
    }
    if ctx.get(x, y, z).has_fluid() {
        return; // cannot jump out of water
    }
    let max_jump = if ctx.allow_walk_on_magma_blocks && standing_on.is_block("magma_block") {
        2
    } else if standing_on.is_block("soul_sand") {
        2 // 1 block gap
    } else if ctx.can_sprint {
        4
    } else {
        3
    };
    // Check parkour jumps from smallest to largest for obstacles, walls and landing positions.
    let mut verified_max_jump = 1; // i - 1 (when i = 2)
    for i in 2..=max_jump {
        let dest_x = x + x_diff * i;
        let dest_z = z + z_diff * i;
        // Check head and feet.
        if !mh::fully_passable(ctx, dest_x, y + 1, dest_z) {
            break;
        }
        if !mh::fully_passable(ctx, dest_x, y + 2, dest_z) {
            break;
        }
        // Check for an ascend landing position.
        let dest_into = ctx.bsi.get0(dest_x, y, dest_z);
        if !mh::fully_passable_state(&dest_into) {
            if i <= 3 && ctx.allow_parkour_ascend && ctx.can_sprint && mh::can_walk_on(ctx, dest_x, y, dest_z, &dest_into) && check_overshoot_safety(ctx, dest_x + x_diff, y + 1, dest_z + z_diff) {
                res.x = dest_x;
                res.y = y + 1;
                res.z = dest_z;
                res.cost = f64::from(i) * SPRINT_ONE_BLOCK_COST + ctx.jump_penalty;
                return;
            }
            break;
        }
        // Check for a flat landing position.
        let landing_on = ctx.bsi.get0(dest_x, y - 1, dest_z);
        // Farmland needs to be canWalkOn or farming never works, but a jump ending on it is disallowed.
        // Frostwalker works here because we cannot jump from possibly unfrozen water.
        if (!landing_on.is_block("farmland") && mh::can_walk_on(ctx, dest_x, y - 1, dest_z, &landing_on))
            || (16.min(ctx.frost_walker + 2) >= i && mh::can_use_frost_walker(ctx, &landing_on))
        {
            if check_overshoot_safety(ctx, dest_x + x_diff, y, dest_z + z_diff) {
                res.x = dest_x;
                res.y = y;
                res.z = dest_z;
                res.cost = cost_from_jump_distance(i) + ctx.jump_penalty;
                return;
            }
            break;
        }
        if !mh::fully_passable(ctx, dest_x, y + 3, dest_z) {
            break;
        }
        verified_max_jump = i;
    }
    // Parkour place starts here.
    if !ctx.allow_parkour_place {
        return;
    }
    // Check parkour jumps from largest to smallest for positions to place blocks.
    for i in (2..=verified_max_jump).rev() {
        let dest_x = x + i * x_diff;
        let dest_z = z + i * z_diff;
        let to_replace = ctx.get(dest_x, y - 1, dest_z);
        let place_cost = ctx.cost_of_placing_at(dest_x, y - 1, dest_z, &to_replace);
        if place_cost >= COST_INF {
            continue;
        }
        if !mh::is_replaceable(dest_x, y - 1, dest_z, &to_replace, ctx.bsi) {
            continue;
        }
        if !check_overshoot_safety(ctx, dest_x + x_diff, y, dest_z + z_diff) {
            continue;
        }
        for (dx, dy, dz) in mh::HORIZONTALS_BUT_ALSO_DOWN {
            let (against_x, against_y, against_z) = (dest_x + dx, y - 1 + dy, dest_z + dz);
            if against_x == dest_x - x_diff && against_z == dest_z - z_diff {
                // We cannot turn around that fast.
                continue;
            }
            if mh::can_place_against(ctx, against_x, against_y, against_z) {
                res.x = dest_x;
                res.y = y;
                res.z = dest_z;
                res.cost = cost_from_jump_distance(i) + place_cost + ctx.jump_penalty;
                return;
            }
        }
    }
}

fn check_overshoot_safety(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    // We walk into these two blocks after the landing of the parkour anyway: they must not be avoidWalkingInto.
    !mh::avoid_walking_into(ctx, &ctx.bsi.get0(x, y, z)) && !mh::avoid_walking_into(ctx, &ctx.bsi.get0(x, y + 1, z))
}

fn cost_from_jump_distance(dist: i32) -> f64 {
    match dist {
        2 => WALK_ONE_BLOCK_COST * 2.0, // IDK LOL
        3 => WALK_ONE_BLOCK_COST * 3.0,
        4 => SPRINT_ONE_BLOCK_COST * 4.0,
        _ => panic!("LOL {dist}"),
    }
}
