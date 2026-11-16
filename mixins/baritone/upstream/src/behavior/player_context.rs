// baritone.api.utils.IPlayerContext, IPlayerController, RayTraceUtils and the parts of LookBehavior,
// InventoryBehavior and InputOverrideHandler that movements use: everything a movement asks of the
// player and the game while it runs. The game implements this; the movements are written against it, so
// they can be tested with a player on paper. Every method takes `&self`: the implementation keeps the game
// behind a cell, so the world can be read (through `bsi`) while the player is being steered.
use super::baritone_block_state_interface::BlockStateInterface;
use super::baritone_calculation_context::PlayerFacts;
use super::baritone_movement_state::Input;
use super::baritone_rotation::Rotation;
use super::baritone_utils::Pos;

pub trait PlayerContext {
    /// The world as it is now (BlockStateInterface.get(ctx, pos)).
    fn bsi(&self) -> &dyn BlockStateInterface;

    // ---- the player
    fn player_feet(&self) -> Pos;
    fn position(&self) -> [f64; 3];
    /// The eyes (`playerHead`).
    fn player_head(&self) -> [f64; 3];
    fn delta_movement(&self) -> [f64; 3];
    fn player_rotations(&self) -> Rotation;
    fn on_ground(&self) -> bool;
    fn is_in_water(&self) -> bool;
    fn is_swimming(&self) -> bool;
    fn is_under_water(&self) -> bool;
    fn is_crouching(&self) -> bool;
    fn horizontal_collision(&self) -> bool;
    fn is_in_wall(&self) -> bool;
    fn food_level(&self) -> i32;
    fn air_supply(&self) -> i32;
    fn max_air_supply(&self) -> i32;
    /// `getFluidHeight(FluidTags.WATER)`.
    fn water_fluid_height(&self) -> f64;
    fn set_sprinting(&self, on: bool);
    fn is_nether(&self) -> bool;
    fn is_sprinting(&self) -> bool;
    fn is_in_lava(&self) -> bool;
    fn on_climbable(&self) -> bool;
    /// Jump boost, slow falling or levitation: the sprint jump simulation knows nothing of them.
    fn has_unsupported_motion_effects(&self) -> bool;
    /// `Player.getSpeed`: the movement speed while sprinting.
    fn movement_speed(&self) -> f64;

    // ---- what the crosshair is on
    fn is_looking_at(&self, pos: Pos) -> bool;
    fn selected_block(&self) -> Option<Pos>;
    fn selected_face(&self) -> Option<[i32; 3]>;

    // ---- reach and aim
    fn block_reach_distance(&self) -> f64;
    /// RotationUtils.reachable: an aim at a block from here, if one of its faces can be hit within reach.
    fn reachable(&self, pos: Pos, reach: f64, would_sneak: bool) -> Option<Rotation>;
    /// RotationUtils.reachableOffset: an aim at a point of a block, if the block is what is hit from there.
    fn reachable_offset(&self, pos: Pos, offset: [f64; 3], reach: f64, would_sneak: bool) -> Option<Rotation>;
    /// InventoryBehavior.throwaway(select, predicate): whether a hotbar slot holds an item `wanted` accepts; with
    /// `select`, the slot is held.
    fn select_item(&self, select: bool, wanted: &dyn Fn(&str) -> bool) -> bool;
    /// RayTraceUtils.rayTraceTowards: the block hit by an aim, and the face.
    fn ray_trace_towards(&self, rotation: Rotation, reach: f64, would_sneak: bool) -> Option<(Pos, [i32; 3])>;
    /// RayTraceUtils.inferSneakingEyePosition.
    fn infer_sneaking_eye_position(&self) -> [f64; 3];
    /// LookBehavior.getAimProcessor().peekRotation: where an aim will really be after smoothing.
    fn peek_rotation(&self, rotation: Rotation) -> Rotation;

    /// What a search needs to know of the player besides the settings (CalculationContext's constructor).
    fn calculation_facts(&self) -> PlayerFacts;

    // ---- the inventory
    fn has_generic_throwaway(&self) -> bool;
    /// InventoryBehavior.selectThrowawayForLocation.
    fn select_throwaway_for_location(&self, select: bool, x: i32, y: i32, z: i32) -> bool;
    /// MovementHelper.switchToBestToolFor.
    fn switch_to_best_tool_for(&self, pos: Pos);
    fn water_bucket_hotbar_slot(&self) -> Option<usize>;
    fn empty_bucket_hotbar_slot(&self) -> Option<usize>;
    fn select_hotbar_slot(&self, slot: usize);

    // ---- placing blocks (what BuilderProcess asks of the world and the held block item)
    /// `getStateForPlacement` of the block item in a hotbar slot, for a click on `face` of `clicked` at `hit`
    /// with the player turned to `rotation`; None when the slot holds no block item or the placement fails.
    fn state_for_placement(&self, slot: usize, clicked: Pos, face: [i32; 3], hit: [f64; 3], rotation: Rotation) -> Option<std::rc::Rc<super::baritone_block_state::BlockState>>;
    /// The same, as `BlockItem.place` would do it: only if the block can really be placed there (`canPlace`).
    fn would_place(&self, slot: usize, clicked: Pos, face: [i32; 3], hit: [f64; 3], rotation: Rotation) -> Option<std::rc::Rc<super::baritone_block_state::BlockState>>;
    /// `BuilderProcess.approxPlaceable`: what the first `size` slots would place at the feet (air for no block).
    fn approx_placeable(&self, size: usize) -> Vec<std::rc::Rc<super::baritone_block_state::BlockState>>;
    /// The bounds of the shape of the block at a position, relative to the block: min x, y, z then max x, y, z;
    /// None for an empty shape (`state.getShape(world, pos).bounds()`).
    fn shape_bounds(&self, pos: Pos) -> Option<[f64; 6]>;
    /// `BlockState.canSurvive(world, pos)`.
    fn can_survive(&self, pos: Pos, state: &super::baritone_block_state::BlockState) -> bool;
    /// `BuilderProcess.placementPlausible`: no entity stands where a solid block would go.
    fn placement_plausible(&self, pos: Pos, state: &super::baritone_block_state::BlockState) -> bool;

    // ---- entities
    /// Where the mobs worth steering clear of are (Avoidance.create's filters).
    fn mobs_to_avoid(&self) -> Vec<Pos>;
    /// The mob spawners in the loaded chunks around the player (the Java asks its cached world for one).
    fn mob_spawners_near(&self) -> Vec<Pos>;
    /// Whether a falling block is on the way into this block position.
    fn falling_block_near(&self, pos: Pos) -> bool;

    // ---- what the movement asks for
    fn update_look_target(&self, rotation: Rotation, force: bool);
    fn clear_all_keys(&self);
    fn set_input_force_state(&self, input: Input, forced: bool);
    fn is_input_forced_down(&self, input: Input) -> bool;
    /// BlockBreakHelper.stopBreakingBlock.
    fn stop_breaking_block(&self);

    /// `logDebug`, from a movement.
    fn log_debug(&self, _line: &str) {}
}
