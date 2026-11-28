// baritone.pathing.movement.CalculationContext: everything one path search needs to know, taken when it
// starts: the world, the player's tools and items, and a snapshot of the settings, so that all the
// moves of a search are costed the same way (a setting changed halfway would give an inconsistent path).
use super::baritone_block_state::BlockState;
use super::baritone_block_state_interface::{cache_slot, BlockStateInterface, CACHE_SIZE, NO_KEY};
use super::baritone_costs::{COST_INF, SWIM_ONE_BLOCK_COST, WALK_ONE_BLOCK_COST, WALK_ONE_IN_WATER_COST};
use super::baritone_settings::Settings;
use std::cell::RefCell;
use std::rc::Rc;

/// baritone.utils.ToolSet: how fast the best tool at hand breaks a block (`getStrVsBlock`).
pub trait ToolSet {
    fn str_vs_block(&self, state: &BlockState) -> f64;
}

/// A tool set that breaks everything at speed 1, for the tests and for a player with nothing.
#[allow(dead_code)]
pub struct BareHands;

impl ToolSet for BareHands {
    fn str_vs_block(&self, state: &BlockState) -> f64 {
        if state.hardness < 0.0 { 0.0 } else if state.hardness == 0.0 { 1.0 } else { 1.0 / (f64::from(state.hardness) * 5.0) }
    }
}

/// What the player has and is, which the settings do not say.
pub struct PlayerFacts {
    /// `allowPlace` and a throwaway block in the hotbar (`hasGenericThrowaway`).
    pub has_throwaway: bool,
    /// `allowWaterBucketFall`, a water bucket in the hotbar, and not the Nether.
    pub has_water_bucket: bool,
    /// `allowSprint` and food above 6.
    pub can_sprint: bool,
    pub frost_walker: i32,
    /// The water movement efficiency of the player's armor, 0 to 1 (`waterSpeedMultiplier`).
    pub water_speed_multiplier: f64,
}

impl Default for PlayerFacts {
    fn default() -> Self {
        Self { has_throwaway: true, has_water_bucket: false, can_sprint: true, frost_walker: 0, water_speed_multiplier: 0.0 }
    }
}

/// What a process overrides of a CalculationContext (the subclasses BuilderCalculationContext,
/// GetToBlockCalculationContext, MineCalculationContext and WalkOffCalculationContext of the Java): the cost
/// of placing a block and the multiplier on breaking one. Each answers None to leave it to the context.
pub trait CostHooks {
    /// What a subclass's constructor changes of the fields (BuilderCalculationContext raises the jump penalty).
    fn configure(&self, _ctx: &mut CalculationContext) {}

    fn cost_of_placing_at(&self, _base: &CalculationContext, _x: i32, _y: i32, _z: i32, _current: &BlockState) -> Option<f64> {
        None
    }

    fn break_cost_multiplier_at(&self, _base: &CalculationContext, _x: i32, _y: i32, _z: i32, _current: &BlockState) -> Option<f64> {
        None
    }

    fn place_bucket_cost(&self, _base: &CalculationContext) -> Option<f64> {
        None
    }
}

#[derive(Clone)]
pub struct CalculationContext<'a> {
    pub hooks: Option<Rc<dyn CostHooks>>,
    /// `miningKeys`: only while a search has claimed it.
    pub mining: Option<Rc<SearchMining>>,
    pub bsi: &'a dyn BlockStateInterface,
    pub tool_set: &'a dyn ToolSet,
    pub has_water_bucket: bool,
    pub has_throwaway: bool,
    pub can_sprint: bool,
    place_block_cost: f64,
    pub allow_break: bool,
    pub allow_break_anyway: Vec<String>,
    pub allow_parkour: bool,
    pub allow_parkour_place: bool,
    pub allow_jump_at_build_limit: bool,
    pub allow_parkour_ascend: bool,
    pub assume_walk_on_water: bool,
    pub assume_walk_on_lava: bool,
    /// The secret setting of ElytraBehavior.
    pub allow_fall_into_lava: bool,
    pub frost_walker: i32,
    pub allow_diagonal_descend: bool,
    pub allow_diagonal_ascend: bool,
    pub allow_downward: bool,
    pub min_fall_height: i32,
    pub max_fall_height_no_water: i32,
    pub max_fall_height_bucket: i32,
    pub water_walk_speed: f64,
    pub break_block_additional_cost: f64,
    pub backtrack_cost_favoring_coefficient: f64,
    pub jump_penalty: f64,
    pub walk_on_water_one_penalty: f64,
    pub allow_walk_on_magma_blocks: bool,
    pub min_y: i32,
    pub max_y: i32,
    /// No move may end below this height. Not in the Java: the surface walk of `#mine` uses it so a search never plans a
    /// fall into a pit or a lake far below.
    pub floor_y: Option<i32>,
    // Read by MovementHelper.
    pub blocks_to_avoid: Vec<String>,
    pub blocks_to_disallow_breaking: Vec<String>,
    pub avoid_updating_falling_blocks: bool,
    pub strict_liquid_check: bool,
    pub allow_vines: bool,
    pub allow_walk_on_bottom_slab: bool,
    pub allow_place_in_fluids_source: bool,
    pub allow_place_in_fluids_flow: bool,
}

/// `miningKeys`, `miningKeysFalling` and their values: `getMiningDurationTicks` by position. The block under you gets its break
/// cost worked out by four descends, a downward and then all your neighbours' descends, and every one of those reads five more
/// blocks for avoidBreaking. The answer does not change during a search. Two of everything, because includeFalling is part of
/// the question.
pub struct SearchMining {
    entries: RefCell<[Vec<(u64, f64)>; 2]>,
}

impl Default for SearchMining {
    fn default() -> Self {
        Self { entries: RefCell::new([vec![(NO_KEY, 0.0); CACHE_SIZE], vec![(NO_KEY, 0.0); CACHE_SIZE]]) }
    }
}

impl SearchMining {
    pub fn get(&self, include_falling: bool, key: u64) -> Option<f64> {
        let (found, value) = self.entries.borrow()[usize::from(include_falling)][cache_slot(key)];
        (found == key).then_some(value)
    }

    pub fn put(&self, include_falling: bool, key: u64, value: f64) {
        self.entries.borrow_mut()[usize::from(include_falling)][cache_slot(key)] = (key, value);
    }
}

impl<'a> CalculationContext<'a> {
    /// The context of a search at this moment.
    pub fn new(settings: &Settings, bsi: &'a dyn BlockStateInterface, tool_set: &'a dyn ToolSet, facts: &PlayerFacts) -> Self {
        let can_sprint = facts.can_sprint && settings.flag("allowSprint");
        let mut water_speed = WALK_ONE_IN_WATER_COST * (1.0 - facts.water_speed_multiplier) + WALK_ONE_BLOCK_COST * facts.water_speed_multiplier;
        if settings.flag("allowSwimming") && can_sprint {
            // Sprint swimming beats wading on the bottom, unless depth strider is good enough to flip that.
            water_speed = water_speed.min(SWIM_ONE_BLOCK_COST);
        }
        Self {
            hooks: None,
            mining: None,
            bsi,
            tool_set,
            has_water_bucket: facts.has_water_bucket,
            has_throwaway: facts.has_throwaway && settings.flag("allowPlace"),
            can_sprint,
            place_block_cost: settings.float("blockPlacementPenalty"),
            allow_break: settings.flag("allowBreak"),
            allow_break_anyway: settings.list("allowBreakAnyway"),
            allow_parkour: settings.flag("allowParkour"),
            allow_parkour_place: settings.flag("allowParkourPlace"),
            allow_jump_at_build_limit: settings.flag("allowJumpAtBuildLimit"),
            allow_parkour_ascend: settings.flag("allowParkourAscend"),
            assume_walk_on_water: settings.flag("assumeWalkOnWater"),
            assume_walk_on_lava: settings.flag("assumeWalkOnLava"),
            allow_fall_into_lava: false,
            frost_walker: facts.frost_walker,
            allow_diagonal_descend: settings.flag("allowDiagonalDescend"),
            allow_diagonal_ascend: settings.flag("allowDiagonalAscend"),
            allow_downward: settings.flag("allowDownward"),
            min_fall_height: 3,
            max_fall_height_no_water: settings.int("maxFallHeightNoWater"),
            max_fall_height_bucket: settings.int("maxFallHeightBucket"),
            water_walk_speed: water_speed,
            break_block_additional_cost: settings.float("blockBreakAdditionalPenalty"),
            backtrack_cost_favoring_coefficient: settings.float("backtrackCostFavoringCoefficient"),
            jump_penalty: settings.float("jumpPenalty"),
            walk_on_water_one_penalty: settings.float("walkOnWaterOnePenalty"),
            allow_walk_on_magma_blocks: settings.flag("allowWalkOnMagmaBlocks"),
            min_y: bsi.min_y(),
            max_y: bsi.max_y(),
            floor_y: None,
            blocks_to_avoid: settings.list("blocksToAvoid"),
            blocks_to_disallow_breaking: settings.list("blocksToDisallowBreaking"),
            avoid_updating_falling_blocks: settings.flag("avoidUpdatingFallingBlocks"),
            strict_liquid_check: settings.flag("strictLiquidCheck"),
            allow_vines: settings.flag("allowVines"),
            allow_walk_on_bottom_slab: settings.flag("allowWalkOnBottomSlab"),
            allow_place_in_fluids_source: settings.flag("allowPlaceInFluidsSource"),
            allow_place_in_fluids_flow: settings.flag("allowPlaceInFluidsFlow"),
        }
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> Rc<BlockState> {
        self.bsi.get0(x, y, z)
    }

    pub fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.bsi.is_loaded(x, z)
    }

    /// `claimSearchCaches`, for this context.
    pub fn with_mining_cache(mut self, cache: Rc<SearchMining>) -> Self {
        self.mining = Some(cache);
        self
    }

    /// The same context with a process's overrides.
    pub fn with_hooks(mut self, hooks: Option<Rc<dyn CostHooks>>) -> Self {
        if let Some(h) = &hooks {
            h.configure(&mut self);
        }
        self.hooks = hooks;
        self
    }

    pub fn place_block_cost(&self) -> f64 {
        self.place_block_cost
    }

    pub fn cost_of_placing_at(&self, x: i32, y: i32, z: i32, current: &BlockState) -> f64 {
        if let Some(cost) = self.hooks.as_ref().and_then(|h| h.cost_of_placing_at(self, x, y, z, current)) {
            return cost;
        }
        self.base_cost_of_placing_at(x, y, z, current)
    }

    /// `CalculationContext.costOfPlacingAt` itself, which an override may call as `super`.
    pub fn base_cost_of_placing_at(&self, x: i32, y: i32, z: i32, current: &BlockState) -> f64 {
        let _ = y;
        if !self.has_throwaway {
            // Only true if allowPlace is true, see the constructor.
            return COST_INF;
        }
        if self.is_possibly_protected() {
            return COST_INF;
        }
        if !self.bsi.can_place_at(x, z) {
            return COST_INF;
        }
        if !self.allow_place_in_fluids_source && current.has_fluid() && current.fluid_is_source() {
            return COST_INF;
        }
        if !self.allow_place_in_fluids_flow && current.has_fluid() && !current.fluid_is_source() {
            return COST_INF;
        }
        self.place_block_cost
    }

    pub fn break_cost_multiplier_at(&self, x: i32, y: i32, z: i32, current: &BlockState) -> f64 {
        if let Some(m) = self.hooks.as_ref().and_then(|h| h.break_cost_multiplier_at(self, x, y, z, current)) {
            return m;
        }
        self.base_break_cost_multiplier_at(x, y, z, current)
    }

    /// `CalculationContext.breakCostMultiplierAt` itself, which an override may call as `super`.
    pub fn base_break_cost_multiplier_at(&self, _x: i32, _y: i32, _z: i32, current: &BlockState) -> f64 {
        if !self.allow_break && !self.allow_break_anyway.iter().any(|b| b == current.bare_name()) {
            return COST_INF;
        }
        if self.is_possibly_protected() {
            return COST_INF;
        }
        1.0
    }

    pub fn place_bucket_cost(&self) -> f64 {
        if let Some(cost) = self.hooks.as_ref().and_then(|h| h.place_bucket_cost(self)) {
            return cost;
        }
        self.place_block_cost
    }

    fn is_possibly_protected(&self) -> bool {
        // TODO more protection logic here; see Baritone issue 220
        false
    }
}

/// baritone.utils.pathing.MutableMoveResult: where a move ends and what it costs.
#[derive(Clone, Copy, Debug)]
pub struct MutableMoveResult {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub cost: f64,
}

impl Default for MutableMoveResult {
    fn default() -> Self {
        Self { x: 0, y: 0, z: 0, cost: COST_INF }
    }
}
