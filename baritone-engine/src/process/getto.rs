// baritone.process.GetToBlockProcess, written from the Java: walk to the nearest known block of a kind, to stand
// next to it (or in front of a container to open it, or into a nether portal), exploring for more when none is
// known.
use super::baritone::Core;
use super::baritone_bom::BlockOptionalMetaLookup;
use super::baritone_calculation_context::{CalculationContext, CostHooks};
use super::baritone_goals::{rc, GoalBlock, GoalComposite, GoalGetToBlock, GoalRef, GoalRunAway, GoalTwoBlocks};
use super::baritone_movement_helper as mh;
use super::baritone_movement_state::Input;
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_player_context::PlayerContext;
use super::baritone_p_explore::{SurfaceFloor, SURFACE_DEPTH};
use super::baritone_p_mine::{lies_underground, search_world, Endless};
use super::baritone_utils::{dist_sq, Pos};
use super::{Game, Screen};
use std::rc::Rc;

/// GetToBlockCalculationContext: it is to signal to MineProcess that we don't care about the allowBreak setting. It
/// is NOT to be used to actually calculate a path.
pub struct GetToBlockHooks;

impl CostHooks for GetToBlockHooks {
    fn break_cost_multiplier_at(&self, _base: &CalculationContext, _x: i32, _y: i32, _z: i32, _current: &super::baritone_block_state::BlockState) -> Option<f64> {
        Some(1.0)
    }
}

pub struct GetToBlockProcess {
    getting_to: Option<super::baritone_bom::BlockOptionalMeta>,
    known_locations: Option<Vec<Pos>>,
    /// Locations we failed to calc to.
    blacklist: Vec<Pos>,
    start: Option<Pos>,
    tick_count: i32,
    arrival_tick_count: i32,
}

impl GetToBlockProcess {
    pub fn new() -> Self {
        Self { getting_to: None, known_locations: None, blacklist: Vec::new(), start: None, tick_count: 0, arrival_tick_count: 0 }
    }

    /// `getToBlock(block)`.
    pub fn get_to_block(&mut self, core: &mut Core, game: &mut Game<'_>, block: super::baritone_bom::BlockOptionalMeta) {
        self.on_lost_control_inner(core);
        self.getting_to = Some(block);
        self.start = core.player_feet(game);
        self.blacklist = Vec::new();
        self.arrival_tick_count = 0;
        self.rescan(core, game, Vec::new());
    }

    pub fn running(&self) -> bool {
        self.getting_to.is_some()
    }

    /// Give up on the closest known block and the ones next to it (`blacklistClosest`).
    pub fn blacklist_closest(&mut self, core: &mut Core, game: &mut Game<'_>) -> bool {
        let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
        let Some(known) = self.known_locations.as_mut() else { return false };
        let mut new_blacklist: Vec<Pos> = Vec::new();
        if let Some(p) = known.iter().copied().min_by_key(|p| dist_sq(*p, feet)) {
            new_blacklist.push(p);
        }
        'outer: loop {
            for k in 0..known.len() {
                let candidate = known[k];
                if new_blacklist.iter().any(|b| are_adjacent(candidate, *b)) {
                    new_blacklist.push(candidate);
                    known.remove(k);
                    continue 'outer;
                }
            }
            break;
        }
        if core.settings.flag("chatDebug") {
            core.say(game, &format!("Blacklisting unreachable locations {new_blacklist:?}"));
        }
        self.blacklist.extend(new_blacklist.iter().copied());
        !new_blacklist.is_empty()
    }

    fn on_lost_control_inner(&mut self, core: &mut Core) {
        self.getting_to = None;
        self.known_locations = None;
        self.start = None;
        self.blacklist = Vec::new();
        core.input.clear_all_keys();
    }

    /// `rescan`.
    fn rescan(&mut self, core: &mut Core, game: &mut Game<'_>, known: Vec<Pos>) {
        let Some(target) = self.getting_to.clone() else { return };
        let lookup = BlockOptionalMetaLookup::new(vec![target]);
        let mut dropped = Vec::new();
        let hooks: Rc<dyn CostHooks> = Rc::new(GetToBlockHooks);
        let mut positions = search_world(core, game, &lookup, 64, &known, &self.blacklist, &mut dropped, Some(hooks));
        positions.retain(|p| !self.blacklist.contains(p));
        self.known_locations = Some(positions);
    }

    fn create_goal(&self, core: &mut Core, game: &mut Game<'_>, pos: Pos) -> GoalRef {
        let block = self.getting_to.as_ref().map_or("", |b| b.block()).to_owned();
        if walk_into_instead_of_adjacent(core, &block) {
            return rc(GoalTwoBlocks(pos));
        }
        if block_on_top_must_be_removed(core, &block) {
            let above = [pos[0], pos[1] + 1, pos[2]];
            let normal = core.with_calc(game, |calc, _, _| mh::is_block_normal_cube(&calc.bsi.get0(above[0], above[1], above[2])));
            if normal {
                return rc(GoalBlock(above));
            }
        }
        rc(GoalGetToBlock(pos))
    }

    fn right_click(&mut self, core: &mut Core, game: &mut Game<'_>) -> bool {
        let known = self.known_locations.clone().unwrap_or_default();
        for pos in &known {
            let rot = core.drive(game, |_, ctx, _| ctx.reachable(*pos, ctx.block_reach_distance(), false));
            let Some(rot) = rot else { continue };
            let selected = core.drive(game, |_, ctx, _| {
                ctx.update_look_target(rot, true);
                ctx.selected_block()
            });
            if selected.is_some_and(|s| known.contains(&s)) {
                core.input.set_input_force_state(Input::ClickRight, true); // TODO find some way to right click even if we're in an ESC menu
                if game.screen != Screen::Playing {
                    return true; // the container is open
                }
            }
            self.arrival_tick_count += 1;
            if self.arrival_tick_count > 21 {
                core.say(game, "Right click timed out");
                return true;
            }
            return false; // trying to right click, will do it next tick or so
        }
        core.say(game, "Arrived but failed to right click open");
        true
    }

    fn tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, is_safe_to_cancel: bool) -> PathingCommand {
        if self.known_locations.is_none() {
            self.rescan(core, game, Vec::new());
        }
        let label = self.getting_to.as_ref().map_or("null".to_owned(), ToString::to_string);
        if self.known_locations.as_ref().is_none_or(Vec::is_empty) {
            if core.settings.flag("exploreForBlocks") && !calc_failed {
                let from = self.start.unwrap_or([0, 0, 0]);
                // Not in the Java: for a block that is not found underground the run keeps its height and never plans a
                // fall far below the player (see MineProcess), instead of ending up in a pit or a lake.
                let surface = self.getting_to.as_ref().is_some_and(|b| !lies_underground(b.block()));
                if surface {
                    if let Some(feet) = core.player_feet(game) {
                        core.path_hooks = Some(Rc::new(SurfaceFloor(feet[1] - SURFACE_DEPTH)));
                    }
                }
                let maintain_y = if surface { Some(from[1]) } else { None };
                return PathingCommand::ForceRevalidateGoalAndPath(rc(Endless(GoalRunAway { from: vec![from], distance: 1, maintain_y })));
            }
            core.say(game, &format!("No known locations of {label}, canceling GetToBlock"));
            if is_safe_to_cancel {
                self.on_lost_control_inner(core);
            }
            return PathingCommand::CancelAndClearGoal;
        }
        let known = self.known_locations.clone().unwrap_or_default();
        let goals: Vec<GoalRef> = known.iter().map(|p| self.create_goal(core, game, *p)).collect();
        let goal: GoalRef = rc(GoalComposite(goals));
        if calc_failed {
            if core.settings.flag("blacklistClosestOnFailure") {
                core.say(game, &format!("Unable to find any path to {label}, blacklisting presumably unreachable closest instances..."));
                self.blacklist_closest(core, game);
                return self.tick(core, game, false, is_safe_to_cancel); // gamer moment
            }
            core.say(game, &format!("Unable to find any path to {label}, canceling GetToBlock"));
            if is_safe_to_cancel {
                self.on_lost_control_inner(core);
            }
            return PathingCommand::CancelAndSetGoal(goal);
        }
        let interval = core.settings.int("mineGoalUpdateInterval") as i32;
        if interval != 0 {
            let due = self.tick_count % interval == 0;
            self.tick_count += 1;
            if due {
                let current = self.known_locations.clone().unwrap_or_default();
                self.rescan(core, game, current);
            }
        }
        let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
        let path_start = core.pathing.expected_segment_start();
        if goal.is_in_goal(feet) && goal.is_in_goal(path_start) && is_safe_to_cancel {
            // We're there.
            let block = self.getting_to.as_ref().map_or("", |b| b.block()).to_owned();
            if right_click_on_arrival(core, &block) {
                if self.right_click(core, game) {
                    self.on_lost_control_inner(core);
                    return PathingCommand::CancelAndClearGoal;
                }
            } else {
                self.on_lost_control_inner(core);
                return PathingCommand::CancelAndClearGoal;
            }
        }
        PathingCommand::RevalidateGoalAndPath(goal)
    }
}

/// `areAdjacent`: safer than direct double comparison from distanceSq.
fn are_adjacent(a: Pos, b: Pos) -> bool {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs() == 1
}

fn walk_into_instead_of_adjacent(core: &Core, block: &str) -> bool {
    core.settings.flag("enterPortal") && block == "minecraft:nether_portal"
}

fn right_click_on_arrival(core: &Core, block: &str) -> bool {
    core.settings.flag("rightClickContainerOnArrival")
        && matches!(
            block,
            "minecraft:crafting_table" | "minecraft:furnace" | "minecraft:blast_furnace" | "minecraft:ender_chest" | "minecraft:chest" | "minecraft:trapped_chest"
        )
}

fn block_on_top_must_be_removed(core: &Core, block: &str) -> bool {
    // Only if we plan to actually open it on arrival; and only these chests, you can open a crafting table or furnace
    // even with a block on top.
    right_click_on_arrival(core, block) && matches!(block, "minecraft:ender_chest" | "minecraft:chest" | "minecraft:trapped_chest")
}

impl Process for GetToBlockProcess {
    fn name(&self) -> &'static str {
        "get to block"
    }

    fn display(&self) -> String {
        let label = self.getting_to.as_ref().map_or("null".to_owned(), ToString::to_string);
        match &self.known_locations {
            Some(k) if k.is_empty() => format!("Exploring randomly to find {label}, no known locations"),
            Some(k) => format!("Get To {label}, {} known locations", k.len()),
            None => format!("Get To {label}"),
        }
    }

    fn is_active(&self) -> bool {
        self.getting_to.is_some()
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, safe_to_cancel: bool) -> PathingCommand {
        self.tick(core, game, calc_failed, safe_to_cancel)
    }

    fn on_lost_control(&mut self, core: &mut Core, _game: &mut Game<'_>) {
        self.on_lost_control_inner(core);
    }

    fn cancel(&mut self) {
        self.getting_to = None;
        self.known_locations = None;
        self.start = None;
        self.blacklist.clear();
    }
}
