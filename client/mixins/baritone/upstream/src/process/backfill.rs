// baritone.process.BackfillProcess and InventoryPauserProcess, written from the Java. Backfill puts a throwaway
// block back where the path broke one (backfill is off by default, and cannot be used with allowParkour). The
// pauser holds the path still for as long as the inventory is being moved. Both are temporary: they take control
// for a tick without cancelling the processes under them.
use super::baritone::Core;
use super::baritone_movement_exec::{attempt_to_place_a_block, Env, PlaceResult};
use super::baritone_movement_state::{Input, MovementState};
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_player_context::PlayerContext;
use super::baritone_utils::{dist_sq, Pos};
use super::Game;
use std::collections::HashMap;

pub struct BackfillProcess {
    /// The blocks to put back, and what was there.
    blocks_to_replace: HashMap<Pos, String>,
    active: bool,
}

impl BackfillProcess {
    pub fn new() -> Self {
        Self { blocks_to_replace: HashMap::new(), active: false }
    }

    /// `amIBreakingABlockHMMMMMMM`.
    fn am_i_breaking_a_block(&mut self, core: &mut Core, game: &mut Game<'_>) {
        if !core.pathing.is_pathing() {
            return;
        }
        let selected = core.drive(game, |_, ctx, _| ctx.selected_block());
        if let Some(pos) = selected {
            let name = core.with_calc(game, |calc, _, _| calc.bsi.get0(pos[0], pos[1], pos[2]).name.to_string());
            self.blocks_to_replace.insert(pos, name);
        }
    }

    /// `toFillIn`: farthest first.
    fn to_fill_in(&self, core: &mut Core, game: &mut Game<'_>) -> Vec<Pos> {
        let feet = core.player_feet(game).unwrap_or([0, 0, 0]);
        let current_breaks: Vec<Pos> = core.pathing.current.as_ref().filter(|e| !e.finished() && !e.failed()).and_then(|e| e.path.movements.get(e.position())).map(|m| m.positions_to_break.clone()).unwrap_or_default();
        let candidates: Vec<Pos> = self.blocks_to_replace.keys().copied().collect();
        let dirt = core.states.default_state_of("dirt");
        let mut fill = core.with_calc(game, |calc, ctx, _| {
            candidates
                .into_iter()
                .filter(|pos| calc.bsi.get0(pos[0], pos[1], pos[2]).is_air())
                .filter(|pos| dirt.as_ref().is_none_or(|d| ctx.placement_plausible(*pos, d)))
                .filter(|pos| !current_breaks.contains(pos))
                .collect::<Vec<Pos>>()
        });
        fill.sort_by_key(|p| std::cmp::Reverse(dist_sq(*p, feet)));
        fill
    }
}

impl Process for BackfillProcess {
    fn name(&self) -> &'static str {
        "backfill"
    }

    fn display(&self) -> String {
        "Backfill".to_owned()
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn refresh(&mut self, core: &mut Core, game: &mut Game<'_>) {
        self.active = false;
        if game.player.is_none() || !core.settings.flag("backfill") {
            return;
        }
        if core.settings.flag("allowParkour") {
            core.say(game, "Backfill cannot be used with allowParkour true");
            let _ = core.set_setting(game, "backfill", "false");
            return;
        }
        let pending: Vec<Pos> = self.blocks_to_replace.keys().copied().collect();
        for pos in pending {
            let gone = !super::baritone_gworld::loaded(game, pos) || !super::baritone_gworld::is_air_at(game, pos);
            if gone {
                self.blocks_to_replace.remove(&pos);
            }
        }
        self.am_i_breaking_a_block(core, game);
        core.input.clear_all_keys();
        self.active = !self.to_fill_in(core, game).is_empty();
    }

    fn priority(&self) -> f64 {
        5.0
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, _calc_failed: bool, is_safe_to_cancel: bool) -> PathingCommand {
        if !is_safe_to_cancel {
            return PathingCommand::RequestPause;
        }
        core.input.clear_all_keys();
        for to_place in self.to_fill_in(core, game) {
            let mut fake = MovementState::new();
            let result = core.with_calc(game, |calc, ctx, s| {
                let env = Env { ctx, calc, settings: s, path: None };
                attempt_to_place_a_block(&env, &mut fake, to_place, false, false)
            });
            match result {
                PlaceResult::NoOption => continue,
                PlaceResult::ReadyToPlace => {
                    core.input.set_input_force_state(Input::ClickRight, true);
                    return PathingCommand::RequestPause;
                }
                PlaceResult::Attempting => {
                    // Patience.
                    if let Some(rot) = fake.target.rotation {
                        core.drive(game, |_, ctx, _| ctx.update_look_target(rot, true));
                    }
                    return PathingCommand::RequestPause;
                }
            }
        }
        PathingCommand::Defer // cede to other process
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {
        self.blocks_to_replace.clear();
    }

    fn cancel(&mut self) {
        self.blocks_to_replace.clear();
    }
}

/// The state of the process is the inventory behavior's (`Core::inventory`): the behavior asks it to hold the player still,
/// and the tick below answers.
pub struct InventoryPauserProcess;

impl InventoryPauserProcess {
    pub fn new() -> Self {
        Self
    }
}

impl Process for InventoryPauserProcess {
    fn name(&self) -> &'static str {
        "inventory pauser"
    }

    fn display(&self) -> String {
        "inventory pauser".to_owned()
    }

    fn is_active(&self) -> bool {
        true
    }

    fn priority(&self) -> f64 {
        5.1 // slightly higher than backfill
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, _calc_failed: bool, is_safe_to_cancel: bool) -> PathingCommand {
        let pauser = core.inventory.pauser();
        pauser.safe_to_cancel_last_tick = is_safe_to_cancel;
        if pauser.pause_requested_last_tick {
            pauser.pause_requested_last_tick = false;
            let motion = game.player.as_ref().map_or(0.0, |p| (p.velocity.x * p.velocity.x + p.velocity.z * p.velocity.z).sqrt());
            if motion < 0.00001 {
                pauser.ticks_of_stationary += 1;
            }
            return PathingCommand::RequestPause;
        }
        pauser.ticks_of_stationary = 0;
        PathingCommand::Defer
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {}

    fn cancel(&mut self) {}
}
