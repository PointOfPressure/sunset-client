// The game side of baritone.behavior.PathingBehavior and baritone.utils.PathingControlManager: the
// processes ask for a `PathingCommand` every tick, and `Core::path_tick` hands it to the pathing behavior
// (pathing_behavior.rs) with the game wrapped as a PlayerContext (game/player_context.rs).
use super::baritone_input::PathState;
use super::baritone::Core;
use super::baritone_aim::AimSettings;
use super::baritone_calculation_context::{CalculationContext, CostHooks};
use super::baritone_player_context::PlayerContext;
use super::baritone_game_ctx::GameCtx;
use super::baritone_toolset::ToolSettings;
use super::baritone_pathing_behavior::PathingBehavior;
use super::baritone_settings::Settings;
use super::baritone_utils::Pos;
use super::Game;

pub use super::baritone_pathing_behavior::PathingCommand;

/// baritone.api.process.IBaritoneProcess.
pub trait Process {
    fn name(&self) -> &'static str;
    /// What `#proc` says it is doing.
    fn display(&self) -> String;
    fn is_active(&self) -> bool;
    /// The one with the higher priority is asked first and wins.
    fn priority(&self) -> f64;
    /// A temporary process (combat pause, backfill) may keep control without the ones under it being cancelled.
    fn is_temporary(&self) -> bool {
        false
    }
    /// Bring `is_active` up to date: the Java's `isActive` looks at the world each time it is asked.
    fn refresh(&mut self, _core: &mut Core, _game: &mut Game<'_>) {}
    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, safe_to_cancel: bool) -> PathingCommand;
    fn on_lost_control(&mut self, core: &mut Core, game: &mut Game<'_>);
    /// Stop for good (the user cancelled, or another process starts).
    fn cancel(&mut self);
}

/// `IBaritoneProcess.DEFAULT_PRIORITY`.
pub const DEFAULT_PRIORITY: f64 = -1.0;

impl Core {
    pub fn player_feet(&self, game: &Game<'_>) -> Option<Pos> {
        super::baritone_gworld::player_feet(game)
    }

    /// Run `f` with the pathing behavior and the game as the player it walks.
    pub fn drive<R>(&mut self, game: &mut Game<'_>, f: impl FnOnce(&mut PathingBehavior, &GameCtx<'_, '_>, &Settings) -> R) -> R {
        let Core { pathing, input, settings, throwaway, aim, look, inventory, states, world_data, .. } = self;
        let ctx = GameCtx::new(game, input, settings, throwaway, aim, look, inventory, states, world_data.current.as_ref().map(|d| &d.cache));
        f(pathing, &ctx, settings)
    }

    /// InventoryBehavior.onTick: the debug lines it has for the chat.
    pub fn inventory_tick(&mut self, game: &mut Game<'_>) -> Vec<String> {
        let tools = ToolSettings::of(&self.settings);
        let pathing = self.pathing.is_pathing();
        self.inventory.tick(game, &self.settings, &self.throwaway, pathing, &tools);
        let lines = self.inventory.take_debug();
        let mut said = std::mem::take(&mut self.pathing.direct);
        if self.settings.flag("chatDebug") {
            said.extend(lines);
        }
        said
    }

    /// WorldProvider and the chunk packer, once a tick: follow the world and dimension, queue the chunks that
    /// loaded, pack as many as there is time for, and save now and then.
    pub fn cache_tick(&mut self, game: &mut Game<'_>) {
        let world_before = self.world_data.current.as_ref().map(|d| d.key.clone());
        // `onChunkEvent` before an unload: the chunks the server dropped since the last tick, still in the world's last
        // dimension unless this tick changed it.
        let mut unloaded = std::mem::take(&mut self.unloading);
        unloaded.extend(horizon_client::streaming::baritone_unload::take());
        self.world_data.follow(game, &self.settings);
        if world_before != self.world_data.current.as_ref().map(|d| d.key.clone()) {
            // LookBehavior.onWorldEvent.
            self.look.on_world_event();
        }
        self.follow_waypoints(game);
        if !self.settings.flag("chunkCaching") {
            return;
        }
        // `onBlockChange` with repackOnAnyBlockChange: a chunk where a block the cache keeps track of was set is packed again.
        let tracked = super::baritone::take_tracked_block_changes();
        if self.settings.flag("repackOnAnyBlockChange") {
            if let Some(data) = self.world_data.current.as_mut() {
                for (cx, cz) in tracked {
                    data.cache.queue_for_packing(cx, cz);
                }
            }
        }
        let same_world = world_before == self.world_data.current.as_ref().map(|d| d.key.clone());
        if same_world && !unloaded.is_empty() {
            let factory = self.states.clone();
            let (done, left) = self.with_calc(game, |calc, ctx, _| ctx.with_game(|g| super::baritone_world_data::WorldProvider::pack_unloaded(g, calc, &factory, unloaded, 6)));
            self.world_data.finish_batch(done, Vec::new());
            self.unloading = left;
        }
        self.world_data.queue_chunks(game);
        let batch = self.world_data.take_batch(game, 4, self.settings.int("chunkPackerQueueMaxSize").max(0) as usize);
        if !batch.is_empty() {
            let factory = self.states.clone();
            let (done, left) = self.with_calc(game, |calc, ctx, _| ctx.with_game(|g| super::baritone_world_data::WorldProvider::pack_batch(g, calc, &factory, batch, 4)));
            self.world_data.finish_batch(done, left);
        }
        self.world_data.save_if_due(game, &self.settings);
    }

    /// Run `f` with a CalculationContext made now (`new CalculationContext(baritone)`).
    pub fn with_calc<R>(&mut self, game: &mut Game<'_>, f: impl FnOnce(&CalculationContext, &GameCtx<'_, '_>, &Settings) -> R) -> R {
        self.with_calc_hooks(game, None, f)
    }

    /// The same with a subclass's overrides.
    pub fn with_calc_hooks<R>(&mut self, game: &mut Game<'_>, hooks: Option<std::rc::Rc<dyn CostHooks>>, f: impl FnOnce(&CalculationContext, &GameCtx<'_, '_>, &Settings) -> R) -> R {
        self.drive(game, |_, ctx, s| {
            let facts = PlayerContext::calculation_facts(ctx);
            let calc = CalculationContext::new(s, PlayerContext::bsi(ctx), ctx, &facts).with_hooks(hooks);
            f(&calc, ctx, s)
        })
    }

    /// The ticks the path being walked has left (PathingBehavior.ticksRemainingInSegment).
    pub fn ticks_remaining_in_segment(&self) -> Option<f64> {
        self.pathing.ticks_remaining_in_segment()
    }

    /// PathingBehavior.estimatedTicksToGoal.
    pub fn estimated_ticks_to_goal(&mut self, game: &Game<'_>) -> Option<f64> {
        let feet = self.player_feet(game)?;
        self.pathing.estimated_ticks_to_goal(feet)
    }

    /// Following a path or searching for one.
    pub fn pathing_busy(&self) -> bool {
        self.pathing.is_busy()
    }

    /// A path can be dropped without leaving the player in mid-air.
    pub fn pathing_safe_to_cancel(&self) -> bool {
        self.pathing.is_safe_to_cancel()
    }

    /// Drop the path and the search, and let go of every key.
    pub fn pathing_cancel(&mut self, game: &mut Game<'_>) {
        self.drive(game, |p, ctx, _| p.force_cancel(ctx));
        self.input.release(game);
    }

    /// Forget the goal as well.
    pub fn pathing_reset(&mut self, game: &mut Game<'_>) {
        self.pathing_cancel(game);
        self.pathing.secret_internal_set_goal(None);
    }

    /// PathingBehavior.onTick, from the first half to the last, around what the processes asked for.
    /// `ctx.worldData().getCachedWorld().getLocationsOf`.
    pub fn cached_locations_of(&mut self, block: &str, maximum: usize, center: Pos, max_region_distance_sq: i32) -> Vec<Pos> {
        self.world_data.current.as_mut().map_or_else(Vec::new, |data| data.cache.get_locations_of(block, maximum, center[0], center[2], max_region_distance_sq))
    }

    pub fn path_tick(&mut self, game: &mut Game<'_>, command: Option<&PathingCommand>, real_process_changed: bool) -> Vec<String> {
        let ticks = game.ticks;
        // LookBehavior.onTick: the noise of this game tick.
        let aim_settings = AimSettings::of(&self.settings, f64::from(game.settings.sensitivity));
        self.aim.tick(&aim_settings);
        self.drive(game, |p, ctx, s| p.begin_tick(ctx, s, ctx));
        let hooks = self.path_hooks.take();
        self.drive(game, |p, ctx, s| {
            p.pre_tick(ctx, s, command, hooks.clone(), real_process_changed, ticks);
            p.run_tick(ctx, s, ctx, ticks);
            p.post_tick(ctx, s, command, hooks, ticks);
        });
        let lines: Vec<String> = self.pathing.log.drain(..).collect();
        if std::mem::take(&mut self.pathing.arrived_at_goal) && self.settings.flag("disconnectOnArrival") {
            // PathingBehavior: the path ended in the goal, so the world is left (`ctx.world().disconnect()`).
            let _ = game.close_world();
        }
        self.input.right_click_speed = self.settings.int("rightClickSpeed");
        self.input.block_break_speed = self.settings.int("blockBreakSpeed");
        self.input.path = PathState { has_path: self.pathing.has_path(), sprint: self.pathing.sprint_state(), allow_swimming: self.settings.flag("allowSwimming") };
        self.input.apply(game);
        if self.settings.flag("chatDebug") { lines } else { Vec::new() }
    }
}
