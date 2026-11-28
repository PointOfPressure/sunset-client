// baritone.pathing.movement.MovementHelper, the half the cost calculation uses: which blocks can be
// walked on or through, which must not be broken, what breaking one costs, and what the fluids do. The
// functions keep the Java's names and its order of tests; where the Java asks the block's class
// (`block instanceof SlabBlock`) this asks the class flag the game filled in.
use super::baritone_block_state::{BlockState, Class, FluidKind};
use super::baritone_block_state_interface::{cache_key, BlockStateInterface};
use super::baritone_calculation_context::CalculationContext;
use super::baritone_costs::COST_INF;

/// The three answers of the Java's `Ternary`: yes, no, or look at the position as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ternary {
    Yes,
    No,
    Maybe,
}

/// Movement.HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP, as (dx, dy, dz): the faces a
/// block can be placed against.
pub const HORIZONTALS_BUT_ALSO_DOWN: [(i32, i32, i32); 5] = [(0, 0, -1), (0, 0, 1), (1, 0, 0), (-1, 0, 0), (0, -1, 0)];

fn in_list(list: &[String], state: &BlockState) -> bool {
    let bare = state.bare_name();
    list.iter().any(|b| b == bare)
}

pub fn avoid_breaking(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState) -> bool {
    !ctx.bsi.can_place_at(x, z)
        || never_break(ctx, state)
        || avoid_adjacent_breaking(ctx, x, y + 1, z, true)
        || avoid_adjacent_breaking(ctx, x + 1, y, z, false)
        || avoid_adjacent_breaking(ctx, x - 1, y, z, false)
        || avoid_adjacent_breaking(ctx, x, y, z + 1, false)
        || avoid_adjacent_breaking(ctx, x, y, z - 1, false)
}

/// PrecomputedData.neverBreak: the blocks that are never ok to break.
fn never_break(ctx: &CalculationContext, state: &BlockState) -> bool {
    in_list(&ctx.blocks_to_disallow_breaking, state)
        || state.is_block("ice") // ice becomes water, and water can mess up the path
        || state.is(Class::Infested) // obvious reasons
}

pub fn avoid_adjacent_breaking(ctx: &CalculationContext, x: i32, y: i32, z: i32, directly_above: bool) -> bool {
    // Returns true if you should avoid breaking a block that is adjacent to this one (lava that will
    // start flowing if you give it a way). Called for north, south, east, west and up, never down: it is
    // always ok to break the block above liquid.
    let state = ctx.bsi.get0(x, y, z);
    if !directly_above
        // It is fine to mine a block with a falling block directly above, that cost is in the calculation.
        && state.is(Class::Falling)
        && ctx.avoid_updating_falling_blocks
        && falling_is_free(&ctx.bsi.get0(x, y - 1, z))
    {
        // Do not break a block next to unsupported gravel: it can cause really weird things.
        return true;
    }
    // Only pure liquids for now: waterlogged blocks can have closed bottom sides and such.
    if state.is(Class::Liquid) {
        if directly_above || ctx.strict_liquid_check {
            return true;
        }
        let level: i32 = state.prop("level").and_then(|l| l.parse().ok()).unwrap_or(0);
        if level == 0 {
            return true; // source blocks like to flow horizontally
        }
        // Everything else will prefer flowing down.
        return !ctx.bsi.get0(x, y - 1, z).is(Class::Liquid);
    }
    state.has_fluid()
}

/// FallingBlock.isFree: the block below would let it fall (air, fire, a liquid, or replaceable).
fn falling_is_free(below: &BlockState) -> bool {
    below.is_air() || below.is(Class::BaseFire) || below.has_fluid() || below.replaceable
}

pub fn can_walk_through(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState) -> bool {
    match can_walk_through_block_state(ctx, state) {
        Ternary::Yes => true,
        Ternary::No => false,
        Ternary::Maybe => can_walk_through_position(ctx, x, y, z, state),
    }
}

pub fn can_walk_through_at(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    can_walk_through(ctx, x, y, z, &ctx.get(x, y, z))
}

pub fn can_walk_through_block_state(ctx: &CalculationContext, state: &BlockState) -> Ternary {
    if state.is(Class::Air) {
        return Ternary::Yes;
    }
    if state.is(Class::BaseFire)
        || state.is_block("cobweb")
        || state.is_block("end_portal")
        || state.is_block("cocoa")
        || state.is(Class::AbstractSkull)
        || state.is_block("bubble_column")
        || state.is(Class::ShulkerBox)
        || state.is(Class::Slab)
        || state.is(Class::TrapDoor)
        || state.is_block("honey_block")
        || state.is_block("end_rod")
        || state.is_block("sweet_berry_bush")
        || state.is_block("pointed_dripstone")
        || state.is(Class::AmethystCluster)
        || state.is(Class::Azalea)
    {
        return Ternary::No;
    }
    if state.is_block("big_dripleaf") || state.is_block("powder_snow") {
        return Ternary::No;
    }
    if in_list(&ctx.blocks_to_avoid, state) {
        return Ternary::No;
    }
    if state.is(Class::Door) {
        return if is_wooden_door(state) { Ternary::Yes } else { Ternary::No };
    }
    if state.is(Class::FenceGate) {
        return Ternary::Yes;
    }
    if state.is(Class::Carpet) {
        return Ternary::Maybe;
    }
    if state.is(Class::SnowLayer) {
        // Snow layers cached as the top layer of a packed chunk have no metadata, so no decision on their
        // depth here: it would make long distance pathing through snowy biomes impossible.
        return Ternary::Maybe;
    }
    if state.has_fluid() {
        return if state.fluid_amount != 8 { Ternary::No } else { Ternary::Maybe };
    }
    if state.is(Class::Cauldron) {
        return Ternary::No;
    }
    if state.pathfindable { Ternary::Yes } else { Ternary::No }
}

/// DoorBlock.isWoodenDoor: a door a hand can open.
pub fn is_wooden_door(state: &BlockState) -> bool {
    state.is(Class::Door) && !state.is_block("iron_door")
}

pub fn can_walk_through_position(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState) -> bool {
    if state.is(Class::Carpet) {
        return can_walk_on_at(ctx, x, y - 1, z);
    }
    if state.is(Class::SnowLayer) {
        // If they are cached as a top block, their metadata is not known: default to true.
        if !ctx.bsi.world_contains_loaded_chunk(x, z) {
            return true;
        }
        // The check in BlockSnow.isPassable is layers < 5, but 3 or more makes it impassable under a 2 high ceiling.
        let layers: i32 = state.prop("layers").and_then(|l| l.parse().ok()).unwrap_or(1);
        if layers >= 3 {
            return false;
        }
        // Low enough to walk through, but is it supported?
        return can_walk_on_at(ctx, x, y - 1, z);
    }
    if state.has_fluid() {
        if is_flowing(ctx.bsi, x, y, z, state) {
            return false;
        }
        // Everything after this relies on the water not flowing, so it is a special case.
        if ctx.assume_walk_on_water {
            return false;
        }
        let up = ctx.bsi.get0(x, y + 1, z);
        if up.has_fluid() || up.is(Class::Waterlily) {
            return false;
        }
        return matches!(state.fluid, FluidKind::Water | FluidKind::FlowingWater);
    }
    state.pathfindable
}

pub fn fully_passable_block_state(state: &BlockState) -> Ternary {
    if state.is(Class::Air) {
        return Ternary::Yes;
    }
    // Exceptions: blocks that are passable but cannot be jumped through.
    if state.is(Class::BaseFire)
        || state.is_block("tripwire")
        || state.is_block("cobweb")
        || state.is_block("vine")
        || state.is_block("ladder")
        || state.is_block("cocoa")
        || state.is(Class::Azalea)
        || state.is(Class::Door)
        || state.is(Class::FenceGate)
        || state.is(Class::SnowLayer)
        || state.has_fluid()
        || state.is(Class::TrapDoor)
        || state.is(Class::EndPortal)
        || state.is(Class::Skull)
        || state.is(Class::ShulkerBox)
    {
        return Ternary::No;
    }
    if state.pathfindable { Ternary::Yes } else { Ternary::No }
}

/// canWalkThrough that also will not impede movement at all: no doors or gates (a click is needed), no
/// water, no ladders, vines or cobwebs (they slow us down).
pub fn fully_passable(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    fully_passable_state(&ctx.get(x, y, z))
}

pub fn fully_passable_state(state: &BlockState) -> bool {
    match fully_passable_block_state(state) {
        Ternary::Yes => true,
        Ternary::No => false,
        Ternary::Maybe => state.pathfindable,
    }
}

pub fn is_replaceable(x: i32, _y: i32, z: i32, state: &BlockState, bsi: &dyn BlockStateInterface) -> bool {
    // For MovementTraverse and MovementAscend. Block double plant defaults to true when the block does not
    // match, all other overrides return true or false; the only case to deal with is snow.
    if state.is(Class::Air) {
        return true;
    }
    if state.is(Class::SnowLayer) {
        if !bsi.world_contains_loaded_chunk(x, z) {
            return true;
        }
        return state.prop("layers") == Some("1");
    }
    if state.is_block("large_fern") || state.is_block("tall_grass") {
        return true;
    }
    state.replaceable
}

pub fn avoid_walking_into(ctx: &CalculationContext, state: &BlockState) -> bool {
    state.has_fluid()
        || (state.is_block("magma_block") && !ctx.allow_walk_on_magma_blocks)
        || state.is_block("cactus")
        || state.is_block("sweet_berry_bush")
        || state.is(Class::BaseFire)
        || state.is_block("end_portal")
        || state.is_block("cobweb")
        || state.is_block("bubble_column")
}

/// Can I walk on this block without anything weird happening, like falling through? Includes water,
/// because we know that we automatically jump on water.
pub fn can_walk_on(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState) -> bool {
    match can_walk_on_block_state(ctx, state) {
        Ternary::Yes => true,
        Ternary::No => false,
        Ternary::Maybe => can_walk_on_position(ctx, x, y, z, state),
    }
}

pub fn can_walk_on_at(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    can_walk_on(ctx, x, y, z, &ctx.get(x, y, z))
}

pub fn can_walk_on_block_state(ctx: &CalculationContext, state: &BlockState) -> Ternary {
    if is_block_normal_cube(state)
        && (!state.is_block("magma_block") || ctx.allow_walk_on_magma_blocks)
        && !state.is_block("bubble_column")
        && !state.is_block("honey_block")
    {
        return Ternary::Yes;
    }
    if state.is(Class::Azalea) {
        return Ternary::Yes;
    }
    if state.is_block("ladder") || (is_climbable(state) && ctx.allow_vines) {
        return Ternary::Yes;
    }
    if state.is_block("farmland") || state.is_block("dirt_path") || state.is_block("soul_sand") {
        return Ternary::Yes;
    }
    if state.is_block("ender_chest") || state.is_block("chest") || state.is_block("trapped_chest") {
        return Ternary::Yes;
    }
    if state.is_block("glass") || state.is(Class::StainedGlass) {
        return Ternary::Yes;
    }
    if state.is(Class::Stair) {
        return Ternary::Yes;
    }
    if is_water(state) && !is_waterlogged_solid_top(state) {
        return Ternary::Maybe;
    }
    if is_lava(state) && ctx.assume_walk_on_lava {
        return Ternary::Maybe;
    }
    if state.is(Class::Slab) {
        if !ctx.allow_walk_on_bottom_slab {
            if state.prop("type") != Some("bottom") {
                return Ternary::Yes;
            }
            return Ternary::No;
        }
        return Ternary::Yes;
    }
    Ternary::No
}

pub fn can_walk_on_position(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState) -> bool {
    if is_water(state) {
        let up_state = ctx.bsi.get0(x, y + 1, z);
        if up_state.is_block("lily_pad") || up_state.is(Class::Carpet) {
            return true;
        }
        if is_flowing(ctx.bsi, x, y, z, state) || up_state.fluid == FluidKind::FlowingWater {
            // The only scenario in which we can walk on flowing water is under still water with jesus off.
            return is_water(&up_state) && !ctx.assume_walk_on_water;
        }
        // With assumeWalkOnWater, only water with no water above can be walked on; without it, only water with water above.
        return is_water(&up_state) ^ ctx.assume_walk_on_water;
    }
    if is_lava(state) && !is_flowing(ctx.bsi, x, y, z, state) && ctx.assume_walk_on_lava {
        return true;
    }
    false // anything not recognised is not walkable, to be safe
}

/// FrostedIceBlock.meltsInto() is a water block at level 0: a still water source.
pub fn can_use_frost_walker(ctx: &CalculationContext, state: &BlockState) -> bool {
    ctx.frost_walker != 0 && state.is_block("water") && state.prop("level").is_none_or(|l| l == "0")
}

/// If movements make us stand or walk on this block, will it have a top to walk on?
pub fn must_be_solid_to_walk_on(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState) -> bool {
    if is_climbable(state) {
        return false;
    }
    if state.has_fluid() {
        // Used for frostwalker, so only blocks where we are still on ground when leaving them to any side.
        if state.is(Class::Slab) {
            if state.prop("type") != Some("bottom") {
                return true;
            }
        } else if state.is(Class::Stair) {
            if state.prop("half") == Some("top") {
                return true;
            }
            if matches!(state.prop("shape"), Some("inner_left" | "inner_right")) {
                return true;
            }
        } else if state.is(Class::TrapDoor) {
            if !state.flag("open") && state.prop("half") == Some("top") {
                return true;
            }
        } else if state.is(Class::Scaffolding) || state.is(Class::Leaves) {
            return true;
        }
        if ctx.assume_walk_on_water {
            return false;
        }
        if ctx.get(x, y + 1, z).is(Class::Liquid) {
            return false;
        }
    }
    true
}

pub fn can_place_against(ctx: &CalculationContext, x: i32, y: i32, z: i32) -> bool {
    can_place_against_state(ctx, x, y, z, &ctx.get(x, y, z))
}

pub fn can_place_against_state(ctx: &CalculationContext, x: i32, _y: i32, z: i32, state: &BlockState) -> bool {
    if !ctx.bsi.can_place_at(x, z) {
        return false;
    }
    // Can we look at the center of a side face of this block and likely be able to place against it?
    // So not the weird things we technically could place against (carpet) but practically cannot.
    is_block_normal_cube(state) || state.is_block("glass") || state.is(Class::StainedGlass)
}

/// Can we climb up this block by pressing space while inside it? Also "if I start a movement on this,
/// can weird things happen?", because movements can end and start on these despite them not being
/// canWalkOn.
pub fn is_climbable(state: &BlockState) -> bool {
    state.is_block("ladder")
        || state.is_block("vine")
        || state.is_block("weeping_vines")
        || state.is_block("weeping_vines_plant")
        || state.is_block("twisting_vines")
        || state.is_block("twisting_vines_plant")
}

pub fn get_mining_duration_ticks_at(ctx: &CalculationContext, x: i32, y: i32, z: i32, include_falling: bool) -> f64 {
    get_mining_duration_ticks(ctx, x, y, z, &ctx.get(x, y, z), include_falling)
}

pub fn get_mining_duration_ticks(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState, include_falling: bool) -> f64 {
    let Some(cache) = &ctx.mining else {
        return get_mining_duration_ticks0(ctx, x, y, z, state, include_falling);
    };
    let key = cache_key(x, y - ctx.min_y, z);
    if let Some(known) = cache.get(include_falling, key) {
        return known;
    }
    let result = get_mining_duration_ticks0(ctx, x, y, z, state, include_falling);
    cache.put(include_falling, key, result);
    result
}

fn get_mining_duration_ticks0(ctx: &CalculationContext, x: i32, y: i32, z: i32, state: &BlockState, include_falling: bool) -> f64 {
    if !can_walk_through(ctx, x, y, z, state) {
        if state.has_fluid() {
            return COST_INF;
        }
        let mult = ctx.break_cost_multiplier_at(x, y, z, state);
        if mult >= COST_INF {
            return COST_INF;
        }
        if avoid_breaking(ctx, x, y, z, state) {
            return COST_INF;
        }
        let str_vs_block = ctx.tool_set.str_vs_block(state);
        if str_vs_block <= 0.0 {
            return COST_INF;
        }
        let mut result = 1.0 / str_vs_block;
        result += ctx.break_block_additional_cost;
        result *= mult;
        if include_falling {
            let above = ctx.get(x, y + 1, z);
            if above.is(Class::Falling) {
                result += get_mining_duration_ticks(ctx, x, y + 1, z, &above, true);
            }
        }
        return result;
    }
    0.0 // we will not actually mine it, so do not check the falling blocks above
}

pub fn is_bottom_slab(state: &BlockState) -> bool {
    state.is(Class::Slab) && state.prop("type") == Some("bottom")
}

/// Whether the block is water, flowing or not.
pub fn is_water(state: &BlockState) -> bool {
    matches!(state.fluid, FluidKind::Water | FluidKind::FlowingWater)
}

/// Water with a full, sturdy top face, such as a waterlogged top slab: its water stays below the feet of
/// anything standing on it, so it is a floor like its dry form.
pub fn is_waterlogged_solid_top(state: &BlockState) -> bool {
    state.has_prop("waterlogged") && state.flag("waterlogged") && state.sturdy_top
}

pub fn is_lava(state: &BlockState) -> bool {
    matches!(state.fluid, FluidKind::Lava | FluidKind::FlowingLava)
}

pub fn is_liquid(state: &BlockState) -> bool {
    state.has_fluid()
}

pub fn possibly_flowing(state: &BlockState) -> bool {
    // Every fluid of the game is a FlowingFluid.
    state.has_fluid() && state.fluid_amount != 8
}

pub fn is_flowing(bsi: &dyn BlockStateInterface, x: i32, y: i32, z: i32, state: &BlockState) -> bool {
    if !state.has_fluid() {
        return false;
    }
    if state.fluid_amount != 8 {
        return true;
    }
    possibly_flowing(&bsi.get0(x + 1, y, z))
        || possibly_flowing(&bsi.get0(x - 1, y, z))
        || possibly_flowing(&bsi.get0(x, y, z + 1))
        || possibly_flowing(&bsi.get0(x, y, z - 1))
}

pub fn is_block_normal_cube(state: &BlockState) -> bool {
    if state.is(Class::BambooStalk)
        || state.is(Class::MovingPiston)
        || state.is(Class::Scaffolding)
        || state.is(Class::ShulkerBox)
        || state.is(Class::PointedDripstone)
        || state.is(Class::AmethystCluster)
    {
        return false;
    }
    state.full_collision
}

pub fn is_transparent(state: &BlockState) -> bool {
    state.is(Class::Air) || state.is_block("lava") || state.is_block("water")
}
