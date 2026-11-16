// baritone.process.MineProcess, written from the Java: a list of the places the wanted blocks are, kept
// pruned and sorted by distance; a goal made of one GoalThreeBlocks, GoalTwoBlocks or GoalBlock per place; a
// shaft mined from where the player stands when the ore is straight above; and, when none is known, a walk away
// from where the mining began (exploreForBlocks) or a scan of what a player could see (legitMine).
// The Java scans on another thread; here the scan runs in the tick, every mineGoalUpdateInterval ticks.
//
// One thing differs from the Java on purpose. With none of the wanted blocks known, the Java always runs away from where
// the mining began at legitMineYLevel (-59), which is where ores are but not where trees, sand or flowers are: `#mine oak_log`
// in a place without oaks went underground. Here that is done only when every wanted block is one that lies underground
// (`lies_underground`); for anything else the process explores the surface (ExploreProcess's walk over the chunks not yet
// seen) until one shows up in the loaded chunks, which the rescan then finds. That walk keeps the height it began at and
// its searches may not end a move more than six blocks below where it stands (CalculationContext.floor_y), so it never
// jumps into a pit or a lake on the way. This holds in legitMine too.
use super::baritone_p_explore::ExploreProcess;
use super::baritone::Core;
use super::baritone_bom::BlockOptionalMetaLookup;
use super::baritone_calculation_context::{CalculationContext, CostHooks};
use super::baritone_costs::COST_INF;
use super::baritone_goals::{rc, Goal, GoalBlock, GoalComposite, GoalRef, GoalRunAway, GoalThreeBlocks, GoalTwoBlocks};
use super::baritone_gworld::dropped_items;
use super::baritone_movement_helper as mh;
use super::baritone_movement_state::Input;
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_scanner::{lookup_state_ids, scan_chunk_radius};
use super::baritone_utils::{dist_sq, Pos};
use super::Game;
use std::collections::HashMap;
use std::rc::Rc;

/// Whether the block is found underground and nowhere else worth walking to: ores, ancient debris, the deepslate layer, tuff,
/// calcite, amethyst, obsidian, spawners and sculk. Everything else (logs, leaves, sand, gravel, dirt, flowers, crops,
/// stone and the like, which reach the surface) is looked for on the surface.
pub fn lies_underground(name: &str) -> bool {
    let bare = name.strip_prefix("minecraft:").unwrap_or(name);
    bare.ends_with("_ore")
        || bare.starts_with("deepslate")
        || bare.starts_with("cobbled_deepslate")
        || bare.starts_with("reinforced_deepslate")
        || bare.starts_with("raw_")
        || bare.starts_with("sculk")
        || bare.starts_with("tuff")
        || bare.contains("amethyst")
        || matches!(bare, "ancient_debris" | "calcite" | "obsidian" | "crying_obsidian" | "spawner" | "bedrock" | "smooth_basalt" | "gilded_blackstone" | "nether_gold_ore" | "glowstone")
}

/// `CachedChunk.BLOCKS_TO_KEEP_TRACK_OF`: the blocks the cached world remembers beyond the loaded chunks.
pub const BLOCKS_TO_KEEP_TRACK_OF: &[&str] = &[
    "ender_chest", "furnace", "chest", "trapped_chest", "end_portal", "end_portal_frame", "spawner", "barrier", "observer",
    "white_shulker_box", "orange_shulker_box", "magenta_shulker_box", "light_blue_shulker_box", "yellow_shulker_box",
    "lime_shulker_box", "pink_shulker_box", "gray_shulker_box", "light_gray_shulker_box", "cyan_shulker_box",
    "purple_shulker_box", "blue_shulker_box", "brown_shulker_box", "green_shulker_box", "red_shulker_box",
    "black_shulker_box", "nether_portal", "hopper", "beacon", "brewing_stand", "creeper_head", "creeper_wall_head",
    "dragon_head", "dragon_wall_head", "player_head", "player_wall_head", "zombie_head", "zombie_wall_head",
    "skeleton_skull", "skeleton_wall_skull", "wither_skeleton_skull", "wither_skeleton_wall_skull", "enchanting_table",
    "anvil", "white_bed", "orange_bed", "magenta_bed", "light_blue_bed", "yellow_bed", "lime_bed", "pink_bed", "gray_bed",
    "light_gray_bed", "cyan_bed", "purple_bed", "blue_bed", "brown_bed", "green_bed", "red_bed", "black_bed", "dragon_egg",
    "jukebox", "end_gateway", "cobweb", "nether_wart", "ladder", "vine",
];

/// Walks away from a point for ever: no position is ever the goal, so the path always goes on. (The anonymous
/// GoalRunAway subclass of MineProcess and GetToBlockProcess.)
pub struct Endless(pub GoalRunAway);

impl Goal for Endless {
    fn is_in_goal(&self, _p: Pos) -> bool {
        false
    }
    fn heuristic(&self, p: Pos) -> f64 {
        self.0.heuristic(p)
    }
    fn heuristic0(&self) -> f64 {
        f64::NEG_INFINITY
    }
    fn label(&self) -> String {
        self.0.label()
    }
}

pub struct MineProcess {
    filter: Option<BlockOptionalMetaLookup>,
    known_ore_locations: Vec<Pos>,
    /// Inaccessible.
    blacklist: Vec<Pos>,
    /// Where a broken block's drop is expected to be, until when (ms).
    anticipated_drops: HashMap<Pos, u64>,
    branch_point: Option<Pos>,
    branch_point_runaway: Option<GoalRef>,
    desired_quantity: i32,
    tick_count: i32,
    /// The walk over the surface that stands in for the underground branch, for blocks that are not underground.
    surface: Option<ExploreProcess>,
}

impl MineProcess {
    pub fn new() -> Self {
        Self {
            filter: None,
            known_ore_locations: Vec::new(),
            blacklist: Vec::new(),
            anticipated_drops: HashMap::new(),
            branch_point: None,
            branch_point_runaway: None,
            desired_quantity: 0,
            tick_count: 0,
            surface: None,
        }
    }

    /// `mine(quantity, filter)`.
    pub fn mine(&mut self, core: &mut Core, game: &mut Game<'_>, quantity: i32, filter: Option<BlockOptionalMetaLookup>) {
        self.filter = filter;
        if self.filter_filter(core, game).is_none() {
            self.filter = None;
        }
        self.desired_quantity = quantity;
        self.known_ore_locations = Vec::new();
        self.blacklist = Vec::new();
        self.branch_point = None;
        self.branch_point_runaway = None;
        self.surface = None;
        self.anticipated_drops = HashMap::new();
        if self.filter.is_some() {
            self.rescan(core, game, Vec::new());
        }
    }

    /// The ore it is after, nearest first, for drawing.
    #[allow(dead_code)]
    pub fn targets(&self) -> &[Pos] {
        &self.known_ore_locations
    }

    #[allow(dead_code)]
    pub fn filter(&self) -> Option<&BlockOptionalMetaLookup> {
        self.filter.as_ref()
    }

    /// `filterFilter`: the filter, minus what allowBreak forbids.
    fn filter_filter(&self, core: &mut Core, game: &mut Game<'_>) -> Option<BlockOptionalMetaLookup> {
        let filter = self.filter.as_ref()?;
        if !core.settings.flag("allowBreak") {
            let anyway = core.settings.list("allowBreakAnyway");
            let kept: Vec<_> = filter.blocks().iter().filter(|b| anyway.iter().any(|a| *a == b.block().trim_start_matches("minecraft:"))).cloned().collect();
            if kept.is_empty() {
                core.say(game, "Unable to mine when allowBreak is false and target block is not in allowBreakAnyway!");
                return None;
            }
            return Some(BlockOptionalMetaLookup::new(kept));
        }
        Some(filter.clone())
    }

    fn rescan(&mut self, core: &mut Core, game: &mut Game<'_>, already: Vec<Pos>) {
        let Some(filter) = self.filter_filter(core, game) else { return };
        if core.settings.flag("legitMine") {
            return;
        }
        let mut dropped = self.dropped_items_scan(core, game);
        let max = core.settings.int("mineMaxOreLocationsCount").max(0) as usize;
        let mut locs = search_world(core, game, &filter, max, &already, &self.blacklist, &mut dropped, None);
        locs.extend(dropped);
        if locs.is_empty() && !core.settings.flag("exploreForBlocks") {
            core.say(game, &format!("No locations for {filter} known, cancelling"));
            if core.settings.flag("notificationOnMineFail") {
                core.notify(&format!("No locations for {filter} known, cancelling"), true);
            }
            self.cancel_now(core, game);
            return;
        }
        self.known_ore_locations = locs;
    }

    /// `cancel()` → onLostControl.
    fn cancel_now(&mut self, core: &mut Core, game: &mut Game<'_>) {
        self.mine(core, game, 0, None);
    }

    /// `droppedItemsScan`.
    fn dropped_items_scan(&self, core: &Core, game: &Game<'_>) -> Vec<Pos> {
        if !core.settings.flag("mineScanDroppedItems") {
            return Vec::new();
        }
        let Some(filter) = &self.filter else { return Vec::new() };
        let mut ret: Vec<Pos> = dropped_items(game)
            .into_iter()
            .filter(|(_, item)| filter.has_item(item))
            .map(|(p, _)| [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32])
            .collect();
        ret.extend(self.anticipated_drops.keys().copied());
        ret
    }

    /// `updateLoucaSystem`: the block being broken is where a drop will land.
    fn update_louca_system(&mut self, core: &mut Core, game: &mut Game<'_>) {
        let now = super::baritone_command::now_ms();
        let selected = core.drive(game, |_, ctx, _| super::baritone_player_context::PlayerContext::selected_block(ctx));
        if let Some(pos) = selected {
            if self.known_ore_locations.contains(&pos) {
                self.anticipated_drops.insert(pos, now + core.settings.int("mineDropLoiterDurationMSThanksLouca").max(0) as u64);
            }
        }
        self.anticipated_drops.retain(|_, until| *until >= now);
    }

    /// `addNearby` (legitMine): what a player standing here could see, in a cube of radius 10.
    fn add_nearby(&mut self, core: &mut Core, game: &mut Game<'_>) -> bool {
        let dropped = self.dropped_items_scan(core, game);
        self.known_ore_locations.extend(dropped.iter().copied());
        let Some(filter) = self.filter_filter(core, game) else { return false };
        let search_dist = 10;
        let faked_block_reach_distance = 20.0;
        let include_diagonals = core.settings.flag("legitMineIncludeDiagonals");
        let already: std::collections::HashSet<Pos> = self.known_ore_locations.iter().copied().collect();
        let known_snapshot = self.known_ore_locations.clone();
        let mut added: Vec<Pos> = Vec::new();
        core.drive(game, |_, ctx, _| {
            let feet = super::baritone_player_context::PlayerContext::player_feet(ctx);
            for x in feet[0] - search_dist..=feet[0] + search_dist {
                for y in feet[1] - search_dist..=feet[1] + search_dist {
                    for z in feet[2] - search_dist..=feet[2] + search_dist {
                        // Crucial to only add blocks we can see because otherwise this is an x-ray and it'll get caught.
                        let state = super::baritone_block_state_interface::BlockStateInterface::get0(ctx, x, y, z);
                        if !filter.has(&state) || already.contains(&[x, y, z]) {
                            continue;
                        }
                        let pos = [x, y, z];
                        let near_known = include_diagonals && known_snapshot.iter().any(|ore| dist_sq(*ore, pos) <= 2);
                        if near_known || super::baritone_player_context::PlayerContext::reachable(ctx, pos, faked_block_reach_distance, false).is_some() {
                            added.push(pos);
                        }
                    }
                }
            }
        });
        self.known_ore_locations.extend(added);
        let mut dropped = dropped;
        let max = core.settings.int("mineMaxOreLocationsCount").max(0) as usize;
        let known = std::mem::take(&mut self.known_ore_locations);
        self.known_ore_locations = prune(core, game, known, &filter, max, &self.blacklist, &mut dropped, None);
        true
    }

    /// `coalesce`: the goal that gets the player to breaking `loc`.
    fn coalesce(&self, core: &mut Core, game: &mut Game<'_>, loc: Pos, locs: &[Pos], filter: &BlockOptionalMetaLookup) -> GoalRef {
        let above = [loc[0], loc[1] + 1, loc[2]];
        let assume_vertical_shaft_mine = core.drive(game, |_, ctx, _| !super::baritone_block_state_interface::BlockStateInterface::get0(ctx, above[0], above[1], above[2]).is(super::baritone_block_state::Class::Falling));
        if !core.settings.flag("forceInternalMining") {
            return if assume_vertical_shaft_mine {
                // We can get directly below the block.
                rc(GoalThreeBlocks(loc))
            } else {
                // We need to get feet or head into the block.
                rc(GoalTwoBlocks(loc))
            };
        }
        let internal = |core: &mut Core, game: &mut Game<'_>, pos: Pos| self.internal_mining_goal(core, game, pos, locs, filter);
        let upward_goal = internal(core, game, [loc[0], loc[1] + 1, loc[2]]);
        let downward_goal = internal(core, game, [loc[0], loc[1] - 1, loc[2]]);
        let double_downward_goal = internal(core, game, [loc[0], loc[1] - 2, loc[2]]);
        if upward_goal == downward_goal {
            // Symmetric.
            if double_downward_goal && assume_vertical_shaft_mine {
                // We have a checkerboard like pattern: this one, and the one two below it. Therefore it's fine to path
                // to immediately below this one, since your feet will be in the doubleDownwardGoal, but only if
                // assumeVerticalShaftMine.
                return rc(GoalThreeBlocks(loc));
            }
            // This block has nothing interesting two below, but is symmetric vertically so we can get either feet or
            // head into it.
            return rc(GoalTwoBlocks(loc));
        }
        if upward_goal {
            // downwardGoal known to be false. Ignore the gap then potential doubleDownward, because we want to path
            // feet into this one and head into upwardGoal.
            return rc(GoalBlock(loc));
        }
        // upwardGoal known to be false, downwardGoal known to be true.
        if double_downward_goal && assume_vertical_shaft_mine {
            // This block and two below it are goals. Path into the center of the one below, because that includes
            // directly below this one.
            return rc(GoalTwoBlocks([loc[0], loc[1] - 1, loc[2]]));
        }
        // upwardGoal false, downwardGoal true, doubleDownwardGoal false: just this block and the one immediately
        // below, no others.
        rc(GoalBlock([loc[0], loc[1] - 1, loc[2]]))
    }

    fn internal_mining_goal(&self, core: &mut Core, game: &mut Game<'_>, pos: Pos, locs: &[Pos], filter: &BlockOptionalMetaLookup) -> bool {
        // Here, BlockStateInterface is used because the position may be in a cached chunk (the targeted block is one
        // that is kept track of).
        if locs.contains(&pos) {
            return true;
        }
        let air_exception = core.settings.flag("internalMiningAirException");
        core.with_calc(game, |calc, _, _| {
            let state = calc.bsi.get0(pos[0], pos[1], pos[2]);
            if air_exception && state.is_air() {
                return true;
            }
            filter.has(&state) && plausible_to_break(calc, pos)
        })
    }

    /// `updateGoal`.
    fn update_goal(&mut self, core: &mut Core, game: &mut Game<'_>) -> Option<PathingCommand> {
        let filter = self.filter_filter(core, game)?;
        let legit = core.settings.flag("legitMine");
        if !self.known_ore_locations.is_empty() {
            let max = core.settings.int("mineMaxOreLocationsCount").max(0) as usize;
            let mut dropped = self.dropped_items_scan(core, game);
            let locs = self.known_ore_locations.clone();
            let locs2 = prune(core, game, locs, &filter, max, &self.blacklist, &mut dropped, None);
            let goals: Vec<GoalRef> = locs2.iter().map(|loc| self.coalesce(core, game, *loc, &locs2, &filter)).collect();
            let goal = rc(GoalComposite(goals));
            self.known_ore_locations = locs2;
            return Some(if legit { PathingCommand::ForceRevalidateGoalAndPath(goal) } else { PathingCommand::RevalidateGoalAndPath(goal) });
        }
        // We don't know any ore locations at the moment.
        if !legit && !core.settings.flag("exploreForBlocks") {
            return None;
        }
        // Only when we should explore for blocks or are in legit mode we do this.
        if !filter.blocks().iter().all(|bom| lies_underground(bom.block())) {
            return self.explore_surface(core, game, &filter);
        }
        let y = core.settings.int("legitMineYLevel") as i32;
        if self.branch_point.is_none() {
            self.branch_point = core.player_feet(game);
        }
        // TODO shaft mode, mine 1x1 shafts to either side.
        if self.branch_point_runaway.is_none() {
            let from = self.branch_point?;
            self.branch_point_runaway = Some(rc(Endless(GoalRunAway { from: vec![from], distance: 1, maintain_y: Some(y) })));
        }
        Some(PathingCommand::RevalidateGoalAndPath(self.branch_point_runaway.clone()?))
    }
}

impl MineProcess {
    /// The walk over the surface for blocks that are not underground: ExploreProcess's goals from where the mining began,
    /// until one of the wanted blocks is in the loaded chunks. Nothing left to explore ends the mining.
    fn explore_surface(&mut self, core: &mut Core, game: &mut Game<'_>, filter: &BlockOptionalMetaLookup) -> Option<PathingCommand> {
        if self.surface.is_none() {
            let feet = core.player_feet(game)?;
            let mut explorer = ExploreProcess::new();
            explorer.explore(feet[0], feet[2]);
            explorer.set_maintain_y(feet[1]);
            self.surface = Some(explorer);
        }
        let command = self.surface.as_mut()?.on_tick(core, game, false, true);
        if matches!(command, PathingCommand::Defer) {
            core.say(game, &format!("No {filter} found on the surface, cancelling"));
            return None;
        }
        Some(command)
    }
}

/// `MineProcess.plausibleToBreak`.
pub fn plausible_to_break(ctx: &CalculationContext, pos: Pos) -> bool {
    let state = ctx.bsi.get0(pos[0], pos[1], pos[2]);
    if mh::get_mining_duration_ticks(ctx, pos[0], pos[1], pos[2], &state, true) >= COST_INF {
        return false;
    }
    if mh::avoid_breaking(ctx, pos[0], pos[1], pos[2], &state) {
        return false;
    }
    // Bedrock above and below makes it implausible, otherwise we're good.
    !(ctx.bsi.get0(pos[0], pos[1] + 1, pos[2]).is_block("bedrock") && ctx.bsi.get0(pos[0], pos[1] - 1, pos[2]).is_block("bedrock"))
}

/// `MineProcess.isNextToAir`.
pub fn is_next_to_air(ctx: &CalculationContext, pos: Pos, radius: i32) -> bool {
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() + dy.abs() + dz.abs() <= radius && mh::is_transparent(&ctx.bsi.get0(pos[0] + dx, pos[1] + dy, pos[2] + dz)) {
                    return true;
                }
            }
        }
    }
    false
}

/// `MineProcess.prune`: forget what is gone, implausible, out of the height limits or blacklisted, and keep the
/// nearest `max`. A drop already next to a known block of the filter is not a place of its own.
pub fn prune(core: &mut Core, game: &mut Game<'_>, locs2: Vec<Pos>, filter: &BlockOptionalMetaLookup, max: usize, blacklist: &[Pos], dropped: &mut Vec<Pos>, hooks: Option<Rc<dyn CostHooks>>) -> Vec<Pos> {
    let exposed_only = core.settings.flag("allowOnlyExposedOres");
    let exposed_distance = core.settings.int("allowOnlyExposedOresDistance") as i32;
    let min_y = core.settings.int("minYLevelWhileMining") as i32;
    let max_y = core.settings.int("maxYLevelWhileMining") as i32;
    let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
    core.with_calc_hooks(game, hooks, |ctx, _, _| {
        dropped.retain(|drop| {
            !locs2.iter().any(|pos| dist_sq(*pos, *drop) <= 9 && filter.has(&ctx.get(pos[0], pos[1], pos[2])) && plausible_to_break(ctx, *pos))
        });
        let mut seen = std::collections::HashSet::new();
        let mut locs: Vec<Pos> = locs2
            .into_iter()
            .filter(|pos| seen.insert(*pos))
            // Remove any that are within loaded chunks that aren't actually what we want.
            .filter(|pos| !ctx.bsi.world_contains_loaded_chunk(pos[0], pos[2]) || filter.has(&ctx.get(pos[0], pos[1], pos[2])) || dropped.contains(pos))
            // Remove any that are implausible to mine (encased in bedrock, or touching lava).
            .filter(|pos| plausible_to_break(ctx, *pos))
            .filter(|pos| !exposed_only || is_next_to_air(ctx, *pos, exposed_distance))
            .filter(|pos| pos[1] >= min_y + ctx.bsi.min_y())
            .filter(|pos| pos[1] <= max_y)
            .filter(|pos| !blacklist.contains(pos))
            .collect();
        locs.sort_by_key(|p| dist_sq(*p, feet));
        locs.truncate(max);
        locs
    })
}

/// `MineProcess.searchWorld`: the places the wanted blocks are, from the cache for the blocks it keeps track of
/// and from a scan of the loaded chunks for the rest, pruned.
pub fn search_world(core: &mut Core, game: &mut Game<'_>, filter: &BlockOptionalMetaLookup, max: usize, already_known: &[Pos], blacklist: &[Pos], dropped: &mut Vec<Pos>, hooks: Option<Rc<dyn CostHooks>>) -> Vec<Pos> {
    let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
    let mut locs: Vec<Pos> = Vec::new();
    let mut untracked = false;
    for bom in filter.blocks() {
        let bare = bom.block().trim_start_matches("minecraft:");
        if BLOCKS_TO_KEEP_TRACK_OF.contains(&bare) {
            // maxRegionDistanceSq 2 means adjacent directly or adjacent diagonally; nothing further than that.
            let cap = core.settings.int("maxCachedWorldScanCount").max(0) as usize;
            locs.extend(core.cached_locations_of(bare, cap, feet, 2));
        } else {
            untracked = true;
        }
    }
    locs = prune(core, game, locs, filter, max, blacklist, dropped, hooks.clone());
    if untracked || (core.settings.flag("extendCacheOnThreshold") && locs.len() < max) {
        let ids = lookup_state_ids(game, filter);
        locs.extend(scan_chunk_radius(game, &ids, feet, max, 32)); // maxSearchRadius is NOT sq
    }
    locs.extend(already_known.iter().copied());
    prune(core, game, locs, filter, max, blacklist, dropped, hooks)
}

impl Process for MineProcess {
    fn name(&self) -> &'static str {
        "mine"
    }

    fn display(&self) -> String {
        format!("Mine {}", self.filter.as_ref().map_or("null".to_owned(), ToString::to_string))
    }

    fn is_active(&self) -> bool {
        self.filter.is_some()
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, is_safe_to_cancel: bool) -> PathingCommand {
        let Some(filter) = self.filter.clone() else { return PathingCommand::Defer };
        if self.desired_quantity > 0 {
            let curr: i32 = game.inventory.iter().take(36).flatten().filter(|s| filter.has_item(&s.item)).map(|s| s.count as i32).sum();
            if curr >= self.desired_quantity {
                core.say(game, &format!("Have {curr} valid items"));
                self.cancel_now(core, game);
                return PathingCommand::Defer;
            }
        }
        let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
        if calc_failed {
            if !self.known_ore_locations.is_empty() && core.settings.flag("blacklistClosestOnFailure") {
                core.say(game, &format!("Unable to find any path to {filter}, blacklisting presumably unreachable closest instance..."));
                if core.settings.flag("notificationOnMineFail") {
                    core.notify(&format!("Unable to find any path to {filter}, blacklisting presumably unreachable closest instance..."), true);
                }
                if let Some(p) = self.known_ore_locations.iter().copied().min_by_key(|t| dist_sq(*t, feet)) {
                    self.blacklist.push(p);
                }
                let blacklist = self.blacklist.clone();
                self.known_ore_locations.retain(|p| !blacklist.contains(p));
            } else {
                core.say(game, &format!("Unable to find any path to {filter}, canceling mine"));
                if core.settings.flag("notificationOnMineFail") {
                    core.notify(&format!("Unable to find any path to {filter}, canceling mine"), true);
                }
                self.cancel_now(core, game);
                return PathingCommand::Defer;
            }
        }

        self.update_louca_system(core, game);
        let interval = core.settings.int("mineGoalUpdateInterval") as i32;
        let curr = self.known_ore_locations.clone();
        let legit = core.settings.flag("legitMine");
        // rescan does nothing in legit mode.
        let due = interval != 0 && {
            let due = self.tick_count % interval == 0;
            self.tick_count += 1;
            due
        };
        if due && !legit {
            self.rescan(core, game, curr.clone());
            if self.filter.is_none() {
                return PathingCommand::Defer;
            }
        }
        if legit && !self.add_nearby(core, game) {
            self.cancel_now(core, game);
            return PathingCommand::Defer;
        }
        let on_ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
        let shaft = curr
            .iter()
            .copied()
            .filter(|p| p[0] == feet[0] && p[2] == feet[2])
            .filter(|p| p[1] >= feet[1])
            // After breaking a block, it takes mineGoalUpdateInterval ticks for it to actually update this list.
            .filter(|p| !core.with_calc(game, |calc, _, _| calc.bsi.get0(p[0], p[1], p[2]).is_air()))
            .min_by_key(|p| dist_sq(*p, [feet[0], feet[1] + 1, feet[2]]));
        core.input.clear_all_keys();
        if let Some(pos) = shaft {
            if on_ground {
                let state_avoid = core.with_calc(game, |calc, _, _| {
                    let state = calc.bsi.get0(pos[0], pos[1], pos[2]);
                    mh::avoid_breaking(calc, pos[0], pos[1], pos[2], &state)
                });
                if !state_avoid {
                    let rot = core.drive(game, |_, ctx, _| {
                        use super::baritone_player_context::PlayerContext as P;
                        P::reachable(ctx, pos, P::block_reach_distance(ctx), false)
                    });
                    if let (Some(rot), true) = (rot, is_safe_to_cancel) {
                        let mut looking = false;
                        core.drive(game, |_, ctx, _| {
                            use super::baritone_player_context::PlayerContext as P;
                            P::update_look_target(ctx, rot, true);
                            P::switch_to_best_tool_for(ctx, pos);
                            looking = P::is_looking_at(ctx, pos) || P::player_rotations(ctx).is_really_close_to(rot);
                        });
                        if looking {
                            core.input.set_input_force_state(Input::ClickLeft, true);
                        }
                        return PathingCommand::RequestPause;
                    }
                }
            }
        }
        match self.update_goal(core, game) {
            Some(command) => command,
            None => {
                // None in range. Maybe say something in chat? (ahem impact)
                self.cancel_now(core, game);
                PathingCommand::Defer
            }
        }
    }

    fn on_lost_control(&mut self, core: &mut Core, game: &mut Game<'_>) {
        self.mine(core, game, 0, None);
    }

    fn cancel(&mut self) {
        self.filter = None;
        self.known_ore_locations.clear();
        self.blacklist.clear();
        self.branch_point = None;
        self.branch_point_runaway = None;
        self.surface = None;
        self.anticipated_drops.clear();
        self.desired_quantity = 0;
    }
}

