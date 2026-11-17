// The game as the pathfinder's PlayerContext, BlockStateInterface and ToolSet: the one place that turns
// Horizon's player, world and inventory into what the ported Baritone code asks for. Block states are
// built from Horizon's facts once per state id and shared.
use super::baritone_aim::{AimProcessor, AimSettings};
use super::baritone_inventory as inv;
use super::baritone_inventory::InventoryBehavior;
use super::baritone_look::{LookBehavior, Mode};
use super::baritone_cached_world::CachedWorld;
use super::baritone_block_state::{BlockState, Class, FluidKind};
use super::baritone_state_factory::StateFactory;
use super::baritone_block_state_interface::{BlockStateInterface, SearchBlocks};
use super::baritone_calculation_context::{PlayerFacts, ToolSet};
use super::baritone_gworld::loaded;
use super::baritone_toolset::{str_vs_block, tool_slot_for, ToolSettings};
use super::baritone_input::Input as InputHandler;
use super::baritone_movement_state::Input;
use super::baritone_player_context::PlayerContext;
use super::baritone_rotation::{block_pos_center, calc_rotation_from_vec3d, Rotation};
use super::baritone_settings::Settings;
use super::baritone_utils::{block_pos_of, Pos};
use super::Game;
use horizon_core::geometry::{Axis, Direction, Vec3};
use horizon_core::position::BlockPos;
use horizon_world::level::{LevelAccess, PlaceContext};
use horizon_world::registry::BlockStateId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// `CalculationContext.frostWalkerLevel` and `waterSpeedMultiplier`, from everything the player has equipped:
/// the last frost walker found, and the first enchantment raising `water_movement_efficiency`, which is depth
/// strider (`0.33333334` a level in its data file).
fn worn_enchantments(g: &Game<'_>) -> (i32, f64) {
    let worn = || g.held().into_iter().chain(g.offhand.as_ref()).chain(g.armor.iter().flatten());
    let frost_walker = worn().map(|s| s.enchantment_level("minecraft:frost_walker")).filter(|l| *l > 0).last().unwrap_or(0);
    let depth_strider = worn().map(|s| s.enchantment_level("minecraft:depth_strider")).find(|l| *l > 0).unwrap_or(0);
    (frost_walker as i32, f64::from(0.333_333_34_f32 * depth_strider as f32))
}

pub struct GameCtx<'a, 'g> {
    game: RefCell<&'a mut Game<'g>>,
    input: RefCell<&'a mut InputHandler>,
    settings: &'a Settings,
    throwaway: &'a [String],
    aim: &'a AimProcessor,
    look: RefCell<&'a mut LookBehavior>,
    inventory: RefCell<&'a mut InventoryBehavior>,
    aim_settings: AimSettings,
    factory: &'a StateFactory,
    cache: Option<&'a CachedWorld>,
    tool_ticks: RefCell<HashMap<u32, f64>>,
    tools: ToolSettings,
    /// `pathThroughCachedOnly`.
    cached_only: bool,
    /// The cache a search has claimed.
    search_blocks: RefCell<Option<Rc<SearchBlocks>>>,
    /// `dimensionType().minY()` and `.height()`.
    min_y: i32,
    height: i32,
}

impl<'a, 'g> GameCtx<'a, 'g> {
    pub fn new(game: &'a mut Game<'g>, input: &'a mut InputHandler, settings: &'a Settings, throwaway: &'a [String], aim: &'a AimProcessor, look: &'a mut LookBehavior, inventory: &'a mut InventoryBehavior, factory: &'a StateFactory, cache: Option<&'a CachedWorld>) -> Self {
        let aim_settings = AimSettings::of(settings, f64::from(game.settings.sensitivity));
        let (min_y, height) = super::baritone_elytra_behavior::dimension_extent(game.dim.id());
        Self {
            game: RefCell::new(game),
            input: RefCell::new(input),
            settings,
            throwaway,
            aim,
            look: RefCell::new(look),
            inventory: RefCell::new(inventory),
            aim_settings,
            factory,
            cache,
            tool_ticks: RefCell::new(HashMap::new()),
            tools: ToolSettings::of(settings),
            cached_only: settings.flag("pathThroughCachedOnly"),
            search_blocks: RefCell::new(None),
            min_y,
            height,
        }
    }

    fn player<R>(&self, f: impl FnOnce(&super::Game<'_>) -> R) -> R {
        f(&**self.game.borrow())
    }

    fn eye_with(&self, sneak: bool) -> [f64; 3] {
        let g = self.game.borrow();
        let p = g.player.as_ref().map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
        g.bar_eye().map_or([p[0], p[1] + if sneak { 1.27 } else { 1.62 }, p[2]], |e| if sneak { [e[0], p[1] + 1.27, e[2]] } else { e })
    }

    /// RotationUtils.reachableOffset.
    fn reachable_offset_impl(&self, pos: Pos, offset: [f64; 3], reach: f64, would_sneak: bool) -> Option<Rotation> {
        let eyes = if would_sneak { self.infer_sneaking_eye_position() } else { self.player_head() };
        let rotation = calc_rotation_from_vec3d(eyes, offset, self.player_rotations());
        let actual = self.peek_rotation(rotation);
        let (hit, _) = self.ray_trace_towards(actual, reach, would_sneak)?;
        if hit == pos {
            return Some(rotation);
        }
        let name_is_fire = self.bsi_state(pos).is(Class::BaseFire);
        if name_is_fire && hit == [pos[0], pos[1] - 1, pos[2]] {
            return Some(rotation);
        }
        None
    }

    fn bsi_state(&self, pos: Pos) -> Rc<BlockState> {
        self.get0(pos[0], pos[1], pos[2])
    }
}

impl BlockStateInterface for GameCtx<'_, '_> {
    fn get0(&self, x: i32, y: i32, z: i32) -> Rc<BlockState> {
        if y < self.min_y || y >= self.min_y + self.height {
            return self.factory.state(self.game.borrow().res.facts().air);
        }
        match &*self.search_blocks.borrow() {
            Some(cache) => cache.get_or_read(x, y - self.min_y, z, || self.get_uncached(x, y, z)),
            None => self.get_uncached(x, y, z),
        }
    }

    fn claim_cache(&self, cache: Rc<SearchBlocks>) {
        *self.search_blocks.borrow_mut() = Some(cache);
    }

    fn release_cache(&self) {
        *self.search_blocks.borrow_mut() = None;
    }

    /// A chunk the search can read: in the world, or remembered by the cache.
    fn is_loaded(&self, x: i32, z: i32) -> bool {
        loaded(&self.game.borrow(), [x, 0, z]) || self.cache.is_some_and(|c| c.is_cached(x, z))
    }

    /// `provider.hasChunk`: in the world, only.
    fn world_contains_loaded_chunk(&self, x: i32, z: i32) -> bool {
        loaded(&self.game.borrow(), [x, 0, z])
    }

    fn min_y(&self) -> i32 {
        self.min_y
    }

    /// `BetterWorldBorder.entirelyContains`.
    fn entirely_contains(&self, x: i32, z: i32) -> bool {
        let [min_x, min_z, max_x, max_z] = self.game.borrow().border.bounds();
        f64::from(x) + 1.0 > min_x && f64::from(x) < max_x && f64::from(z) + 1.0 > min_z && f64::from(z) < max_z
    }

    /// `BetterWorldBorder.canPlaceAt`: a block cannot go at the very edge, against a block outside the border.
    fn can_place_at(&self, x: i32, z: i32) -> bool {
        let [min_x, min_z, max_x, max_z] = self.game.borrow().border.bounds();
        f64::from(x) > min_x && f64::from(x) + 1.0 < max_x && f64::from(z) > min_z && f64::from(z) + 1.0 < max_z
    }

    /// The highest block there can be: `minY + height - 1`.
    fn max_y(&self) -> i32 {
        self.min_y + self.height - 1
    }
}

impl ToolSet for GameCtx<'_, '_> {
    fn str_vs_block(&self, state: &BlockState) -> f64 {
        if let Some(t) = self.tool_ticks.borrow().get(&state.id) {
            return *t;
        }
        let g = self.game.borrow();
        let id = horizon_world::registry::BlockStateId(state.id as _);
        // The best tool's progress per tick (the Java's getStrVsBlock); none can break it: 0 or less.
        let value = str_vs_block(&g, id, &self.tools);
        self.tool_ticks.borrow_mut().insert(state.id, value);
        value
    }
}

impl PlayerContext for GameCtx<'_, '_> {
    fn bsi(&self) -> &dyn BlockStateInterface {
        self
    }

    fn player_feet(&self) -> Pos {
        self.player(|g| super::baritone_gworld::player_feet(g).unwrap_or_default())
    }

    fn position(&self) -> [f64; 3] {
        self.player(|g| g.player.as_ref().map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]))
    }

    fn player_head(&self) -> [f64; 3] {
        self.eye_with(false)
    }

    fn delta_movement(&self) -> [f64; 3] {
        self.player(|g| g.player.as_ref().map_or([0.0; 3], |p| [p.velocity.x, p.velocity.y, p.velocity.z]))
    }

    fn player_rotations(&self) -> Rotation {
        // BaritonePlayerContext.playerRotations: with freeLook the server's rotation, else the player's own.
        self.look.borrow().effective_rotation(self.settings).unwrap_or_else(|| self.player(|g| Rotation::new(g.yaw, g.pitch)))
    }

    fn on_ground(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.ground.on_ground))
    }

    fn is_in_water(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.fluids.was_touching_water))
    }

    fn is_swimming(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.swimming))
    }

    fn is_under_water(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.fluids.was_eye_in_water))
    }

    fn is_crouching(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.local_crouching))
    }

    fn horizontal_collision(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.collision.horizontal))
    }

    fn is_in_wall(&self) -> bool {
        let eye = self.player_head();
        let s = self.get0(eye[0].floor() as i32, eye[1].floor() as i32, eye[2].floor() as i32);
        s.full_collision
    }

    fn food_level(&self) -> i32 {
        self.player(|g| g.player.as_ref().map_or(20, |p| p.health.food.food as i32))
    }

    fn air_supply(&self) -> i32 {
        self.player(|g| g.player.as_ref().map_or(300, |p| p.air))
    }

    fn max_air_supply(&self) -> i32 {
        300
    }

    fn water_fluid_height(&self) -> f64 {
        // The height of the water column the feet stand in (Entity.getFluidHeight): whole blocks of water
        // above the feet, plus the top block's level.
        let feet = self.player_feet();
        let p = self.position();
        let mut top = None;
        for y in feet[1]..feet[1] + 4 {
            let s = self.get0(feet[0], y, feet[2]);
            if matches!(s.fluid, FluidKind::Water | FluidKind::FlowingWater) {
                top = Some((y, s.fluid_amount));
            } else {
                break;
            }
        }
        top.map_or(0.0, |(y, amount)| (f64::from(y) + f64::from(amount) / 9.0 - p[1]).max(0.0))
    }

    fn set_sprinting(&self, on: bool) {
        if let Some(p) = self.game.borrow_mut().player.as_mut() {
            p.sprinting = on;
        }
    }

    fn is_nether(&self) -> bool {
        self.player(|g| g.native_player().is_some_and(|p| p.dimension == "minecraft:the_nether"))
    }

    fn is_sprinting(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.sprinting))
    }

    fn is_in_lava(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.is_in_lava()))
    }

    fn on_climbable(&self) -> bool {
        self.player(|g| g.player.as_ref().is_some_and(|p| p.last_climbable.is_some()))
    }

    fn has_unsupported_motion_effects(&self) -> bool {
        self.player(|g| {
            let s = g.potions.effects.status();
            s.jump_boost.is_some() || s.slow_falling || s.levitation.is_some()
        })
    }

    fn movement_speed(&self) -> f64 {
        self.player(|g| g.player.as_ref().map_or(0.1, |p| f64::from(p.speed)))
    }

    fn is_looking_at(&self, pos: Pos) -> bool {
        self.selected_block() == Some(pos)
    }

    fn selected_block(&self) -> Option<Pos> {
        self.selected().map(|(p, _)| p)
    }

    fn selected_face(&self) -> Option<[i32; 3]> {
        self.selected().map(|(_, f)| f)
    }

    fn block_reach_distance(&self) -> f64 {
        // BaritonePlayerController.getBlockReachDistance.
        if self.player(|g| g.creative) { 5.0 } else { self.settings.float("blockReachDistance") }
    }

    fn reachable_offset(&self, pos: Pos, offset: [f64; 3], reach: f64, would_sneak: bool) -> Option<Rotation> {
        self.reachable_offset_impl(pos, offset, reach, would_sneak)
    }

    fn select_item(&self, select: bool, wanted: &dyn Fn(&str) -> bool) -> bool {
        // InventoryBehavior.throwaway(select, desired): the hotbar, the off hand, and the main inventory if allowInventory.
        let mut g = self.game.borrow_mut();
        self.inventory.borrow_mut().throwaway(&mut g, self.settings, select, &|s| wanted(&s.item), self.settings.flag("allowInventory"))
    }

    fn reachable(&self, pos: Pos, reach: f64, would_sneak: bool) -> Option<Rotation> {
        // remainWithExistingLookDirection
        if self.settings.flag("remainWithExistingLookDirection") && self.is_looking_at(pos) {
            let hypothetical = self.player_rotations().add(Rotation::new(0.0, 0.0001));
            if would_sneak {
                // What if we are looking at it now, but as soon as we start sneaking we no longer are?
                if let Some((hit, _)) = self.ray_trace_towards(hypothetical, reach, true) {
                    if hit == pos {
                        return Some(hypothetical);
                    }
                }
            } else {
                return Some(hypothetical);
            }
        }
        if let Some(r) = self.reachable_offset(pos, block_pos_center(pos), reach, would_sneak) {
            return Some(r);
        }
        // The sides of the block: (the Java's BLOCK_SIDE_MULTIPLIERS against a full block's shape).
        for (sx, sy, sz) in [(0.5, 0.0, 0.5), (0.5, 1.0, 0.5), (0.5, 0.5, 0.0), (0.5, 0.5, 1.0), (0.0, 0.5, 0.5), (1.0, 0.5, 0.5)] {
            let point = [f64::from(pos[0]) + (1.0 - sx), f64::from(pos[1]) + (1.0 - sy), f64::from(pos[2]) + (1.0 - sz)];
            if let Some(r) = self.reachable_offset(pos, point, reach, would_sneak) {
                return Some(r);
            }
        }
        None
    }

    fn ray_trace_towards(&self, rotation: Rotation, reach: f64, would_sneak: bool) -> Option<(Pos, [i32; 3])> {
        let eye = self.eye_with(would_sneak);
        self.player(|g| g.bar_ray(eye, rotation.yaw, rotation.pitch, reach))
    }

    fn infer_sneaking_eye_position(&self) -> [f64; 3] {
        self.eye_with(true)
    }

    fn peek_rotation(&self, rotation: Rotation) -> Rotation {
        self.aim.peek_rotation(rotation, self.player_rotations(), &self.aim_settings)
    }

    fn calculation_facts(&self) -> PlayerFacts {
        let (frost_walker, water_speed_multiplier) = self.player(worn_enchantments);
        PlayerFacts {
            has_throwaway: self.has_generic_throwaway(),
            has_water_bucket: self.settings.flag("allowWaterBucketFall") && self.water_bucket_hotbar_slot().is_some() && !self.is_nether(),
            can_sprint: self.food_level() > 6,
            frost_walker,
            water_speed_multiplier,
        }
    }

    fn has_generic_throwaway(&self) -> bool {
        let mut g = self.game.borrow_mut();
        self.inventory.borrow_mut().has_generic_throwaway(&mut g, self.settings, self.throwaway)
    }

    fn select_throwaway_for_location(&self, select: bool, x: i32, y: i32, z: i32) -> bool {
        // The block the build wants there, if one is being built (`placeAt`).
        let place_at = self.inventory.borrow().place_at.clone();
        let wanted = place_at.and_then(|f| f(x, y, z, &self.get0(x, y, z))).map(|s| s.name.to_string());
        let mut g = self.game.borrow_mut();
        self.inventory.borrow_mut().select_throwaway_for_location(&mut g, self.settings, self.throwaway, select, wanted.as_deref())
    }

    fn switch_to_best_tool_for(&self, pos: Pos) {
        if !self.settings.flag("autoTool") || self.settings.flag("assumeExternalAutoTool") {
            return;
        }
        let mut g = self.game.borrow_mut();
        let state = g.world.block(pos[0], pos[1], pos[2]);
        if let Some(slot) = tool_slot_for(&g, state, &self.tools) {
            inv::select(&mut g, slot);
        }
    }

    fn water_bucket_hotbar_slot(&self) -> Option<usize> {
        self.player(inv::bucket_slot)
    }

    fn empty_bucket_hotbar_slot(&self) -> Option<usize> {
        self.player(inv::empty_bucket_slot)
    }

    fn select_hotbar_slot(&self, slot: usize) {
        inv::select(&mut self.game.borrow_mut(), slot);
    }

    fn falling_block_near(&self, pos: Pos) -> bool {
        // getEntitiesOfClass(FallingBlockEntity.class, new AABB(0, 0, 0, 1, 1.1, 1).move(pos)) is not empty.
        let (x, y, z) = (f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
        self.player(|g| {
            g.falling.iter().any(|f| {
                let (half, height) = (f64::from(0.98_f32) / 2.0, f64::from(0.98_f32));
                f.position.x + half > x && f.position.x - half < x + 1.0 && f.position.y + height > y && f.position.y < y + 1.1 && f.position.z + half > z && f.position.z - half < z + 1.0
            })
        })
    }

    fn state_for_placement(&self, slot: usize, clicked: Pos, face: [i32; 3], hit: [f64; 3], rotation: Rotation) -> Option<Rc<BlockState>> {
        let (ctx, block) = self.place_context(slot, clicked, face, hit, rotation)?;
        let id = self.game.borrow_mut().with_level(|b, level, ticks| b.state_for_placement(level, ticks, block, &ctx))?;
        Some(self.rc_state(id))
    }

    fn would_place(&self, slot: usize, clicked: Pos, face: [i32; 3], hit: [f64; 3], rotation: Rotation) -> Option<Rc<BlockState>> {
        let (ctx, _) = self.place_context(slot, clicked, face, hit, rotation)?;
        let (_, id) = self.game.borrow_mut().with_level(|b, level, ticks| b.placement_state(level, ticks, &ctx))?;
        Some(self.rc_state(id))
    }

    fn approx_placeable(&self, size: usize) -> Vec<Rc<BlockState>> {
        let feet = self.player_feet();
        let p = self.position();
        let rotation = self.player_rotations();
        let air = {
            let g = self.game.borrow();
            self.rc_state(g.res.facts().air)
        };
        (0..size).map(|slot| self.state_for_placement(slot, feet, [0, 1, 0], p, rotation).unwrap_or_else(|| air.clone())).collect()
    }

    fn shape_bounds(&self, pos: Pos) -> Option<[f64; 6]> {
        let g = self.game.borrow();
        let id = g.world.block(pos[0], pos[1], pos[2]);
        match g.res.collision().cached_shapes().cached_shape(id) {
            Ok(Some(shape)) if !shape.is_empty() => Some([shape.min(Axis::X), shape.min(Axis::Y), shape.min(Axis::Z), shape.max(Axis::X), shape.max(Axis::Y), shape.max(Axis::Z)]),
            Ok(Some(_)) => None,
            // A shape that depends on its neighbours: a whole block is the guess.
            _ => Some([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
        }
    }

    fn can_survive(&self, pos: Pos, state: &BlockState) -> bool {
        self.game.borrow_mut().with_level(|b, level, ticks| b.can_survive(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]), BlockStateId(state.id as _)))
    }

    /// `placementPlausible`: the state's collision shape at the position, against every entity that blocks building.
    fn placement_plausible(&self, pos: Pos, state: &BlockState) -> bool {
        if !state.full_collision {
            return true; // an empty collision shape is unobstructed
        }
        self.game.borrow_mut().with_level(|_, level, _| level.is_unobstructed(BlockPos::new(pos[0], pos[1], pos[2]), BlockStateId(state.id)))
    }

    fn mobs_to_avoid(&self) -> Vec<Pos> {
        // Avoidance.create: every Mob, but a spider where the light is bright (Spider is the cave spider too), a
        // zombified piglin nobody has angered and an enderman that is not creepy (aggravated).
        let feet = self.player_feet();
        self.player(|g| {
            let light = g.world.light(feet[0], feet[1], feet[2]);
            let raw = (i32::from(light >> 4) - g.sky_darken()).max(i32::from(light & 15)).clamp(0, 15) as f32 / 15.0;
            let base = raw / (4.0 - 3.0 * raw);
            // DimensionType.brightness: lerp(ambientLight, curve, 1).
            let ambient = match g.dim {
                super::Dim::Nether => 0.1,
                super::Dim::End => 0.25,
                _ => 0.0,
            };
            let dark = base + ambient * (1.0 - base) < 0.5;
            g.mobs
                .iter()
                .filter(|m| m.alive())
                .filter(|m| match &*m.kind.id {
                    "minecraft:spider" | "minecraft:cave_spider" => dark,
                    "minecraft:zombified_piglin" => m.ai.target.is_some() || m.extra.anger > 0,
                    "minecraft:enderman" => m.ai.target.is_some(),
                    _ => true,
                })
                .map(|m| block_pos_of([m.position.x, m.position.y, m.position.z]))
                .collect()
        })
    }

    /// Avoidance.create asks the cache for `"mob_spawner"`. The cache keeps that block as `"spawner"`, so the Java
    /// finds none and its spawner avoidance never happens. Neither does it here, with the same result.
    fn mob_spawners_near(&self) -> Vec<Pos> {
        Vec::new()
    }

    fn update_look_target(&self, rotation: Rotation, block_interact: bool) {
        // LookBehavior.updateTarget: the mode is decided now, the rotation is applied by `baritone::look_pre` just before
        // the player's motion reads it.
        let fall_flying = self.player(|g| g.player.as_ref().is_some_and(|p| p.fall_flying));
        let mode = Mode::resolve(self.settings, fall_flying, self.is_swimming(), block_interact);
        self.look.borrow_mut().update_target(rotation, mode);
    }

    fn clear_all_keys(&self) {
        self.input.borrow_mut().clear_all_keys();
    }

    fn set_input_force_state(&self, input: Input, forced: bool) {
        self.input.borrow_mut().set_input_force_state(input, forced);
    }

    fn is_input_forced_down(&self, input: Input) -> bool {
        self.input.borrow().is_input_forced_down(input)
    }

    fn stop_breaking_block(&self) {
        self.input.borrow_mut().set_input_force_state(Input::ClickLeft, false);
    }

    fn log_debug(&self, line: &str) {
        self.inventory.borrow_mut().log_debug(line);
    }
}

impl GameCtx<'_, '_> {
    /// `getUncached`: the world, or what the cache remembers of a chunk that is not there.
    fn get_uncached(&self, x: i32, y: i32, z: i32) -> Rc<BlockState> {
        let g = self.game.borrow();
        if self.cached_only || !loaded(&g, [x, y, z]) {
            // A chunk that is not loaded (or pathThroughCachedOnly) is what the cached world remembers of it, air where it remembers nothing.
            let remembered = self.cache.and_then(|c| c.get_block(x, y, z)).and_then(|name| self.factory.default_state_of(&name));
            return remembered.unwrap_or_else(|| self.factory.state(g.res.facts().air));
        }
        self.factory.state(g.world.block(x, y, z))
    }

    /// Run `f` with the game.
    pub fn with_game<R>(&self, f: impl FnOnce(&Game<'_>) -> R) -> R {
        f(&self.game.borrow())
    }

    /// Air.
    pub fn air_state(&self) -> Rc<BlockState> {
        let id = self.game.borrow().res.facts().air;
        self.factory.state(id)
    }

    /// A state of the game as the pathfinder's BlockState, from the cache the reads share.
    pub fn rc_state(&self, id: BlockStateId) -> Rc<BlockState> {
        self.factory.state(id)
    }

    /// The PlaceContext for a click, and the block the item places.
    fn place_context(&self, slot: usize, clicked: Pos, face: [i32; 3], hit: [f64; 3], rotation: Rotation) -> Option<(PlaceContext, horizon_world::level::BlockId)> {
        let sneaking = self.is_crouching();
        let g = self.game.borrow();
        let stack = g.inventory.get(slot).and_then(Option::as_ref).filter(|s| s.count > 0)?;
        let info = g.res.items().get(&stack.item)?;
        let state = info.block?;
        let direction = match face {
            [0, 1, 0] => Direction::Up,
            [0, -1, 0] => Direction::Down,
            [0, 0, -1] => Direction::North,
            [0, 0, 1] => Direction::South,
            [-1, 0, 0] => Direction::West,
            _ => Direction::East,
        };
        let context = PlaceContext {
            clicked_pos: BlockPos::new(clicked[0], clicked[1], clicked[2]),
            face: direction,
            hit: Vec3::new(hit[0], hit[1], hit[2]),
            inside: false,
            yaw: rotation.yaw,
            pitch: rotation.pitch,
            sneaking,
            item: stack.item.to_string(),
            can_use_game_master_blocks: false,
        };
        let block = g.res.behaviours().block(state);
        Some((context, block))
    }

    /// `BaritonePlayerContext.objectMouseOver`: along the rotation Baritone works with, at its reach.
    fn selected(&self) -> Option<(Pos, [i32; 3])> {
        self.ray_trace_towards(self.player_rotations(), self.block_reach_distance(), false)
    }
}
