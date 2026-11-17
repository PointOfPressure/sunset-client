// baritone.process.BuilderProcess, written from the Java: make the world match a schematic. Each tick it
// finds the blocks that are wrong around the player, breaks one that is in reach, places one that can be
// placed from here (choosing the hotbar slot whose block would come out as the schematic wants), and otherwise
// asks the pathing behavior for a goal that gets it next to the work (GoalBreak, GoalPlace, GoalAdjacent), with
// a calculation context (BuilderHooks) that makes placing a correct block free. `#tunnel` and `#sel` are builds
// of the same kind.
use super::baritone::Core;
use super::baritone_block_state::{BlockState, Class};
use super::baritone_builder_logic::{contains_blockstate, valid, BuilderHooks, ISchematic, LayeredSchematic, ValidSettings};
use super::baritone_calculation_context::{CalculationContext, CostHooks};
use super::baritone_goals::{rc, GoalAdjacent, GoalBlock, GoalBreak, GoalBreakFromAbove, GoalComposite, GoalPlace, GoalRef, JankyGoalComposite};
use super::baritone_movement_helper as mh;
use super::baritone_movement_state::Input;
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_player_context::PlayerContext;
use super::baritone_rotation::{calc_rotation_from_vec3d, Rotation};
use super::baritone_schematic::{BlockSpec, Schematic, StaticSchematic};
use super::baritone_schematic_wrappers::{map_art_heights, MaskSchematic, MirroredSchematic, RotatedSchematic, StateOps, SubstituteSchematic};
use super::baritone_state_ops::GameStateOps;
use super::baritone_state_factory::StateFactory;
use super::baritone_utils::{dist_sq, Pos};
use super::Game;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// `Direction.values()`: down, up, north, south, west, east.
const DIRECTIONS: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/// What `Placement` holds: the slot, the block to place against, the face of it, and the aim.
struct Placement {
    hotbar_selection: usize,
    place_against: Pos,
    side: [i32; 3],
    rot: Rotation,
}

/// A schematic of this crate's kind (what block belongs at a position) read as block states.
struct SpecSchematic {
    inner: Rc<dyn Schematic>,
    factory: Rc<StateFactory>,
    cache: RefCell<HashMap<(String, Vec<(String, String)>), Rc<BlockState>>>,
    air: Rc<BlockState>,
}

impl SpecSchematic {
    fn state(&self, spec: &BlockSpec) -> Rc<BlockState> {
        let key = (spec.name.clone(), spec.props.clone());
        if let Some(s) = self.cache.borrow().get(&key) {
            return s.clone();
        }
        let state = self.factory.resolve(&spec.name, &spec.props).unwrap_or_else(|| self.air.clone());
        self.cache.borrow_mut().insert(key, state.clone());
        state
    }
}

impl ISchematic for SpecSchematic {
    fn desired_state(&self, x: i32, y: i32, z: i32, current: &BlockState, _approx_placeable: &[Rc<BlockState>]) -> Rc<BlockState> {
        match self.inner.desired([x, y, z], &current.name) {
            Some(spec) => self.state(&spec),
            None => self.air.clone(),
        }
    }

    fn width_x(&self) -> i32 {
        self.inner.size()[0]
    }

    fn height_y(&self) -> i32 {
        self.inner.size()[1]
    }

    fn length_z(&self) -> i32 {
        self.inner.size()[2]
    }

    fn in_schematic(&self, x: i32, y: i32, z: i32, current: &BlockState) -> bool {
        if !(x >= 0 && x < self.width_x() && y >= 0 && y < self.height_y() && z >= 0 && z < self.length_z()) {
            return false;
        }
        self.inner.desired([x, y, z], &current.name).is_some()
    }
}

pub struct BuilderProcess {
    incorrect_positions: Option<HashSet<Pos>>,
    /// Positions that are completed even if they're out of render distance and we can't make sure right now.
    observed_completed: HashSet<Pos>,
    name: Option<String>,
    real_schematic: Option<Rc<dyn ISchematic>>,
    schematic: Option<Rc<dyn ISchematic>>,
    origin: Pos,
    ticks: i32,
    paused: bool,
    layer: i32,
    num_repeats: i32,
    approx_placeable: Vec<Rc<BlockState>>,
    pub stop_at_height: i32,
    /// `pathStart()` of this tick.
    expected_start: Pos,
}

impl BuilderProcess {
    pub fn new() -> Self {
        Self {
            incorrect_positions: None,
            observed_completed: HashSet::new(),
            name: None,
            real_schematic: None,
            schematic: None,
            origin: [0, 0, 0],
            ticks: 0,
            paused: false,
            layer: 0,
            num_repeats: 0,
            approx_placeable: Vec::new(),
            stop_at_height: 0,
            expected_start: [0, 0, 0],
        }
    }

    /// `build(name, schematic, origin)` for a schematic of this crate's kind.
    pub fn build(&mut self, core: &mut Core, game: &mut Game<'_>, name: String, schematic: Rc<dyn Schematic>, origin: Pos) {
        let spec = self.spec_of(core, game, schematic);
        self.build_states(core, game, name, spec, origin, false);
    }

    fn spec_of(&self, core: &mut Core, game: &mut Game<'_>, schematic: Rc<dyn Schematic>) -> Rc<dyn ISchematic> {
        let air = core.drive(game, |_, ctx, _| ctx.air_state());
        Rc::new(SpecSchematic { inner: schematic, factory: core.states.clone(), cache: RefCell::new(HashMap::new()), air })
    }

    /// `build(name, File, origin)` after the parse, with `applyMapArtAndSelection`: mapArtMode keeps only the top block of each
    /// column, and buildOnlySelection only what the selections cover.
    pub fn build_file(&mut self, core: &mut Core, game: &mut Game<'_>, name: String, parsed: StaticSchematic, origin: Pos) {
        let size = parsed.size;
        let heights = core.settings.flag("mapArtMode").then(|| {
            map_art_heights(size[0], size[2], &|x, z| (0..size[1]).map(|y| parsed.blocks[parsed.index([x, y, z])].as_ref().is_some_and(|b| !b.is_air())).collect())
        });
        let mut schematic = self.spec_of(core, game, Rc::new(parsed));
        if let Some(heights) = heights {
            schematic = Rc::new(MaskSchematic::map_art(schematic, heights, size[2]));
        }
        let selecting = core.settings.flag("buildOnlySelection");
        if selecting {
            // The selections, relative to where the schematic is built.
            let boxes = core.selections.selections.iter().map(|s| {
                let (min, max) = (s.min(), s.max());
                ([min[0] - origin[0], min[1] - origin[1], min[2] - origin[2]], [max[0] - origin[0], max[1] - origin[1], max[2] - origin[2]])
            });
            schematic = Rc::new(MaskSchematic::selection(schematic, boxes.collect()));
        }
        self.build_states(core, game, name, schematic, origin, selecting);
    }

    /// `build(name, ISchematic, origin)`: the substitutes, the mirror, the rotation and the skipped blocks go round it, in that
    /// order. `selection` is whether it is a SelectionSchematic.
    pub fn build_states(&mut self, core: &mut Core, game: &mut Game<'_>, name: String, schematic: Rc<dyn ISchematic>, origin: Pos, selection: bool) {
        let size = [schematic.width_x(), schematic.height_y(), schematic.length_z()];
        let ops: Rc<dyn StateOps> = Rc::new(GameStateOps::new(game, core.states.clone()));
        let mut wrapped = schematic;
        let substitutes = core.settings.block_map("buildSubstitutes");
        if !substitutes.is_empty() {
            wrapped = Rc::new(SubstituteSchematic::new(wrapped, substitutes, ops.clone()));
        }
        let mirror = core.settings.mirror("buildSchematicMirror");
        if mirror != 0 {
            wrapped = Rc::new(MirroredSchematic::new(wrapped, mirror, ops.clone()));
        }
        let rotation = core.settings.rotation("buildSchematicRotation");
        if rotation != 0 {
            wrapped = Rc::new(RotatedSchematic::new(wrapped, rotation, ops));
        }
        // TODO of the Java: the setting is read when the schematic is made, not each time it is asked.
        let skip = core.settings.list("buildSkipBlocks");
        let inner = wrapped.clone();
        wrapped = Rc::new(MaskSchematic::new(
            wrapped,
            Rc::new(move |x, y, z, current| {
                let wanted = inner.desired_state(x, y, z, current, &[]);
                !skip.iter().any(|b| b == wanted.bare_name())
            }),
        ));
        self.name = Some(name);
        self.real_schematic = None;
        self.schematic = Some(wrapped);
        // The orientation offsets use the size of the schematic as it came, as the Java does.
        let (mut x, mut y, mut z) = (origin[0], origin[1], origin[2]);
        if core.settings.flag("schematicOrientationX") {
            x += size[0];
        }
        if core.settings.flag("schematicOrientationY") {
            y += size[1];
        }
        if core.settings.flag("schematicOrientationZ") {
            z += size[2];
        }
        self.origin = [x, y, z];
        self.paused = false;
        self.layer = core.settings.int("startAtLayer") as i32;
        self.stop_at_height = size[1];
        if core.settings.flag("buildOnlySelection") && selection {
            let selections = &core.selections.selections;
            if selections.is_empty() {
                core.say(game, "Poor little kitten forgot to set a selection while BuildOnlySelection is true");
                self.stop_at_height = 0;
            } else if core.settings.flag("buildInLayers") {
                let minim = selections.iter().map(|s| s.min()[1]).min().unwrap_or(0);
                let maxim = selections.iter().map(|s| s.max()[1]).max().unwrap_or(0);
                let top_down = core.settings.flag("layerOrder");
                let start_at_height = if top_down { y + size[1] - maxim } else { minim - y };
                self.stop_at_height = (if top_down { y + size[1] - minim } else { maxim - y }) + 1;
                // startAtLayer or startAtHeight, whichever is highest.
                self.layer = self.layer.max(start_at_height / (core.settings.int("layerHeight") as i32).max(1));
                if core.settings.flag("chatDebug") {
                    core.say(game, &format!("Schematic starts at y={y} with height {}", size[1]));
                    core.say(game, &format!("Selection starts at y={minim} and ends at y={maxim}"));
                    core.say(game, &format!("Considering relevant height {start_at_height} - {}", self.stop_at_height));
                }
            }
        }
        self.num_repeats = 0;
        self.observed_completed = HashSet::new();
        self.incorrect_positions = None;
    }

    /// `clearArea(corner1, corner2)`.
    pub fn clear_area(&mut self, core: &mut Core, game: &mut Game<'_>, c1: Pos, c2: Pos) {
        let origin = [c1[0].min(c2[0]), c1[1].min(c2[1]), c1[2].min(c2[2])];
        let size = [(c1[0] - c2[0]).abs() + 1, (c1[1] - c2[1]).abs() + 1, (c1[2] - c2[2]).abs() + 1];
        let fill = super::baritone_schematic::FillSchematic { size, spec: BlockSpec::air() };
        self.build(core, game, "clear area".into(), Rc::new(fill), origin);
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    #[allow(dead_code)]
    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// The positions that are not yet as the schematic wants, for drawing.
    #[allow(dead_code)]
    pub fn incorrect(&self) -> impl Iterator<Item = Pos> + '_ {
        self.incorrect_positions.iter().flatten().copied()
    }

    fn on_lost_control_inner(&mut self, core: &mut Core) {
        core.inventory.place_at = None;
        self.incorrect_positions = None;
        self.name = None;
        self.schematic = None;
        self.real_schematic = None;
        self.layer = core.settings.int("startAtLayer") as i32;
        self.num_repeats = 0;
        self.paused = false;
        self.observed_completed = HashSet::new();
    }

    /// `onTick` with the recursion counter the Java keeps.
    fn tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, is_safe_to_cancel: bool, recursions: u32) -> PathingCommand {
        if recursions > 100 {
            // onTick calls itself, don't crash.
            return PathingCommand::Defer;
        }
        self.approx_placeable = core.drive(game, |_, ctx, _| ctx.approx_placeable(36));
        if core.input.is_input_forced_down(Input::ClickLeft) {
            self.ticks = 5;
        } else {
            self.ticks -= 1;
        }
        core.input.clear_all_keys();
        if self.paused {
            return PathingCommand::CancelAndClearGoal;
        }
        let build_in_layers = core.settings.flag("buildInLayers");
        let layer_height = core.settings.int("layerHeight") as i32;
        if build_in_layers {
            if self.real_schematic.is_none() {
                self.real_schematic = self.schematic.clone();
            }
            let Some(real) = self.real_schematic.clone() else { return PathingCommand::Defer };
            // layer = 0 should be nothing, layer = realSchematic.heightY() should be everything.
            let (min_y, max_y) = if core.settings.flag("layerOrder") {
                // Top to bottom.
                (real.height_y() - self.layer * layer_height, real.height_y() - 1)
            } else {
                (0, self.layer * layer_height - 1)
            };
            self.schematic = Some(Rc::new(LayeredSchematic { real, min_y_inclusive: min_y, max_y_inclusive: max_y }));
        }
        let Some(schematic) = self.schematic.clone() else { return PathingCommand::Defer };
        {
            // `placeAt`, for the inventory behavior when a movement places a block while this build is going.
            let (wanted, origin, placeable) = (schematic.clone(), self.origin, self.approx_placeable.clone());
            core.inventory.place_at = Some(Rc::new(move |x, y, z, current| {
                let (rx, ry, rz) = (x - origin[0], y - origin[1], z - origin[2]);
                if !wanted.in_schematic(rx, ry, rz, current) {
                    return None;
                }
                let state = wanted.desired_state(rx, ry, rz, current, &placeable);
                (!state.is_air()).then_some(state)
            }));
        }
        let valid_settings = ValidSettings::of(&core.settings);
        let hooks = Rc::new(BuilderHooks {
            placeable: self.approx_placeable.iter().take(9).cloned().collect(),
            schematic: schematic.clone(),
            origin: self.origin,
            valid: valid_settings.clone(),
            place_incorrect_penalty: core.settings.float("placeIncorrectBlockPenaltyMultiplier"),
            break_correct_penalty: core.settings.float("breakCorrectBlockPenaltyMultiplier"),
        });
        let dyn_hooks: Rc<dyn CostHooks> = hooks.clone();

        let recalculated = core.with_calc_hooks(game, Some(dyn_hooks.clone()), |calc, ctx, s| self.recalc(calc, ctx, s, &hooks, &valid_settings));
        if !recalculated {
            if build_in_layers && self.layer * layer_height < self.stop_at_height {
                core.say(game, &format!("Starting layer {}", self.layer));
                self.layer += 1;
                return self.tick(core, game, calc_failed, is_safe_to_cancel, recursions + 1);
            }
            let repeat = core.settings.vec3("buildRepeat");
            let max = core.settings.int("buildRepeatCount");
            self.num_repeats += 1;
            if repeat == [0, 0, 0] || (max != -1 && self.num_repeats >= max) {
                core.say(game, "Done building");
                if core.settings.flag("notificationOnBuildFinished") {
                    core.notify("Done building", false);
                }
                self.on_lost_control_inner(core);
                return PathingCommand::Defer;
            }
            // Build repeat time.
            self.layer = 0;
            self.origin = [self.origin[0] + repeat[0], self.origin[1] + repeat[1], self.origin[2] + repeat[2]];
            if !core.settings.flag("buildRepeatSneaky") {
                schematic.reset();
            }
            core.say(
                game,
                &format!(
                    "Repeating build in vector Vec3i{{x={}, y={}, z={}}}, new origin is BlockPos{{x={}, y={}, z={}}}",
                    repeat[0], repeat[1], repeat[2], self.origin[0], self.origin[1], self.origin[2]
                ),
            );
            return self.tick(core, game, calc_failed, is_safe_to_cancel, recursions + 1);
        }
        if core.settings.flag("distanceTrim") {
            self.trim(core, game);
        }

        let to_break = core.with_calc_hooks(game, Some(dyn_hooks.clone()), |calc, ctx, s| self.to_break_near_player(calc, ctx, s, &hooks, &valid_settings));
        if let Some((pos, rot)) = to_break {
            if is_safe_to_cancel && game.player.as_ref().is_some_and(|p| p.ground.on_ground) {
                // We'd like to pause to break this block; only change look direction if it's safe (don't want to
                // mess up an in-progress parkour for example).
                let mut sneaking = false;
                let mut looking = false;
                core.drive(game, |_, ctx, _| {
                    ctx.update_look_target(rot, true);
                    ctx.switch_to_best_tool_for(pos);
                    sneaking = ctx.is_crouching();
                    looking = ctx.is_looking_at(pos) || ctx.player_rotations().is_really_close_to(rot);
                });
                if sneaking {
                    // Really horrible bug where a block is visible for breaking while sneaking but not otherwise.
                    core.input.set_input_force_state(Input::Sneak, true);
                }
                if looking {
                    core.input.set_input_force_state(Input::ClickLeft, true);
                }
                return PathingCommand::CancelAndClearGoal;
            }
        }
        let mut desirable_on_hotbar: Vec<Rc<BlockState>> = Vec::new();
        let to_place = core.with_calc_hooks(game, Some(dyn_hooks.clone()), |calc, ctx, s| self.search_for_placables(calc, ctx, s, &hooks, &valid_settings, &mut desirable_on_hotbar));
        if let Some(placement) = to_place {
            if is_safe_to_cancel && game.player.as_ref().is_some_and(|p| p.ground.on_ground) && self.ticks <= 0 {
                let mut click = false;
                core.drive(game, |_, ctx, _| {
                    ctx.update_look_target(placement.rot, true);
                    ctx.select_hotbar_slot(placement.hotbar_selection);
                    let facing = ctx.is_looking_at(placement.place_against) && ctx.selected_face() == Some(placement.side);
                    click = facing || ctx.player_rotations().is_really_close_to(placement.rot);
                });
                core.input.set_input_force_state(Input::Sneak, true);
                if click {
                    core.input.set_input_force_state(Input::ClickRight, true);
                }
                return PathingCommand::CancelAndClearGoal;
            }
        }

        if core.settings.flag("allowInventory") {
            let mut useful_slots: Vec<usize> = Vec::new();
            let mut no_valid_hotbar_option: Vec<Rc<BlockState>> = Vec::new();
            'desired: for desired in &desirable_on_hotbar {
                for (i, slot) in self.approx_placeable.iter().enumerate().take(9) {
                    if valid(slot, Some(desired), true, &valid_settings) {
                        useful_slots.push(i);
                        continue 'desired;
                    }
                }
                no_valid_hotbar_option.push(desired.clone());
            }
            'inventory: for i in 9..36 {
                let Some(stack) = self.approx_placeable.get(i) else { break };
                for desired in &no_valid_hotbar_option {
                    if valid(stack, Some(desired), true, &valid_settings) {
                        if !core.inventory.attempt_to_put_on_hotbar(game, &core.settings, i, &|s| useful_slots.contains(&s)) {
                            // Awaiting an inventory move, so pause.
                            return PathingCommand::RequestPause;
                        }
                        break 'inventory;
                    }
                }
            }
        }

        let hotbar: Vec<Rc<BlockState>> = self.approx_placeable.iter().take(9).cloned().collect();
        let mut goal = core.with_calc_hooks(game, Some(dyn_hooks.clone()), |calc, ctx, s| self.assemble(calc, ctx, s, &hooks, &valid_settings, &hotbar, false));
        if goal.is_none() {
            // We're far away, so assume that we have our whole inventory to recalculate placeable properly.
            let all = self.approx_placeable.clone();
            let mut missing_report = Vec::new();
            goal = core.with_calc_hooks(game, Some(dyn_hooks.clone()), |calc, ctx, s| self.assemble_log(calc, ctx, s, &hooks, &valid_settings, &all, &mut missing_report));
            for line in missing_report {
                core.say(game, &line);
            }
            if goal.is_none() {
                let height = self.real_schematic.as_ref().map_or(self.stop_at_height, |r| r.height_y());
                if core.settings.flag("skipFailedLayers") && build_in_layers && self.layer * layer_height < height {
                    core.say(game, &format!("Skipping layer that I cannot construct! Layer #{}", self.layer));
                    self.layer += 1;
                    return self.tick(core, game, calc_failed, is_safe_to_cancel, recursions + 1);
                }
                core.say(game, "Unable to do it. Pausing. resume to resume, cancel to cancel");
                self.paused = true;
                return PathingCommand::RequestPause;
            }
        }
        core.path_hooks = Some(dyn_hooks);
        match goal {
            Some(g) => PathingCommand::ForceRevalidateGoalAndPath(g),
            None => PathingCommand::RequestPause,
        }
    }

    // ---- what is wrong

    fn recalc(&mut self, calc: &CalculationContext, ctx: &dyn PlayerContext, s: &super::baritone_settings::Settings, hooks: &BuilderHooks, vs: &ValidSettings) -> bool {
        if self.incorrect_positions.is_none() {
            self.incorrect_positions = Some(HashSet::new());
            self.full_recalc(calc, s, hooks, vs);
            if self.incorrect_positions.as_ref().is_none_or(HashSet::is_empty) {
                return false;
            }
        }
        self.recalc_nearby(calc, ctx, s, hooks, vs);
        if self.incorrect_positions.as_ref().is_none_or(HashSet::is_empty) {
            self.full_recalc(calc, s, hooks, vs);
        }
        self.incorrect_positions.as_ref().is_some_and(|p| !p.is_empty())
    }

    fn trim(&mut self, core: &mut Core, game: &Game<'_>) {
        let Some(me) = game.player.as_ref().map(|p| super::baritone_utils::block_pos_of([p.position.x, p.position.y, p.position.z])) else { return };
        let _ = core;
        if let Some(set) = &self.incorrect_positions {
            let copy: HashSet<Pos> = set.iter().copied().filter(|p| dist_sq(*p, me) <= 200).collect();
            if !copy.is_empty() {
                self.incorrect_positions = Some(copy);
            }
        }
    }

    fn recalc_nearby(&mut self, calc: &CalculationContext, ctx: &dyn PlayerContext, s: &super::baritone_settings::Settings, hooks: &BuilderHooks, vs: &ValidSettings) {
        let center = ctx.player_feet();
        let radius = s.int("builderTickScanRadius") as i32;
        let Some(incorrect) = self.incorrect_positions.as_mut() else { return };
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    let (x, y, z) = (center[0] + dx, center[1] + dy, center[2] + dz);
                    let current = calc.bsi.get0(x, y, z);
                    if let Some(desired) = hooks.get_schematic(x, y, z, &current) {
                        // We care about this position.
                        let pos = [x, y, z];
                        if valid(&current, Some(&desired), false, vs) {
                            incorrect.remove(&pos);
                            self.observed_completed.insert(pos);
                        } else {
                            incorrect.insert(pos);
                            self.observed_completed.remove(&pos);
                        }
                    }
                }
            }
        }
    }

    fn full_recalc(&mut self, calc: &CalculationContext, s: &super::baritone_settings::Settings, hooks: &BuilderHooks, vs: &ValidSettings) {
        let mut incorrect = HashSet::new();
        let limit = s.int("incorrectSize").max(0) as usize;
        let Some(schematic) = self.schematic.clone() else { return };
        'all: for y in 0..schematic.height_y() {
            for z in 0..schematic.length_z() {
                for x in 0..schematic.width_x() {
                    let (bx, by, bz) = (x + self.origin[0], y + self.origin[1], z + self.origin[2]);
                    let current = calc.bsi.get0(bx, by, bz);
                    if !schematic.in_schematic(x, y, z, &current) {
                        continue;
                    }
                    let pos = [bx, by, bz];
                    if calc.bsi.world_contains_loaded_chunk(bx, bz) {
                        // Check if it's in render distance, not if it's in cache: we can directly observe this block.
                        let desired = schematic.desired_state(x, y, z, &current, &self.approx_placeable);
                        if valid(&current, Some(&desired), false, vs) {
                            self.observed_completed.insert(pos);
                        } else {
                            incorrect.insert(pos);
                            self.observed_completed.remove(&pos);
                            if incorrect.len() > limit {
                                break 'all;
                            }
                        }
                        continue;
                    }
                    // This is not in render distance.
                    if !self.observed_completed.contains(&pos) {
                        // And we've never seen this position be correct, therefore mark as incorrect.
                        incorrect.insert(pos);
                        if incorrect.len() > limit {
                            break 'all;
                        }
                    }
                }
            }
        }
        let _ = hooks;
        self.incorrect_positions = Some(incorrect);
    }

    // ---- breaking

    fn to_break_near_player(&self, calc: &CalculationContext, ctx: &dyn PlayerContext, s: &super::baritone_settings::Settings, hooks: &BuilderHooks, vs: &ValidSettings) -> Option<(Pos, Rotation)> {
        let center = ctx.player_feet();
        let path_start = self.expected_start;
        let from_above = s.flag("breakFromAbove");
        for dx in -5..=5 {
            for dy in (if from_above { -1 } else { 0 })..=5 {
                for dz in -5..=5 {
                    let (x, y, z) = (center[0] + dx, center[1] + dy, center[2] + dz);
                    if dy == -1 && x == path_start[0] && z == path_start[2] {
                        continue; // don't mine what we're supported by, but not directly standing on
                    }
                    let curr = calc.bsi.get0(x, y, z);
                    let Some(desired) = hooks.get_schematic(x, y, z, &curr) else { continue };
                    if !curr.is_air() && !(curr.is_block("water") || curr.is_block("lava")) && !valid(&curr, Some(&desired), false, vs) {
                        let pos = [x, y, z];
                        if let Some(rot) = ctx.reachable(pos, ctx.block_reach_distance(), false) {
                            return Some((pos, rot));
                        }
                    }
                }
            }
        }
        None
    }

    // ---- placing

    fn search_for_placables(&self, calc: &CalculationContext, ctx: &dyn PlayerContext, _s: &super::baritone_settings::Settings, hooks: &BuilderHooks, vs: &ValidSettings, desirable_on_hotbar: &mut Vec<Rc<BlockState>>) -> Option<Placement> {
        let center = ctx.player_feet();
        for dx in -5..=5 {
            for dy in -5..=1 {
                for dz in -5..=5 {
                    let (x, y, z) = (center[0] + dx, center[1] + dy, center[2] + dz);
                    let curr = calc.bsi.get0(x, y, z);
                    let Some(desired) = hooks.get_schematic(x, y, z, &curr) else { continue };
                    if mh::is_replaceable(x, y, z, &curr, calc.bsi) && !valid(&curr, Some(&desired), false, vs) {
                        if dy == 1 && calc.bsi.get0(x, y + 1, z).is_air() {
                            continue;
                        }
                        desirable_on_hotbar.push(desired.clone());
                        if let Some(placement) = self.possible_to_place(calc, ctx, &desired, [x, y, z], vs) {
                            return Some(placement);
                        }
                    }
                }
            }
        }
        None
    }

    fn possible_to_place(&self, calc: &CalculationContext, ctx: &dyn PlayerContext, to_place: &BlockState, at: Pos, vs: &ValidSettings) -> Option<Placement> {
        // These two don't care which face we're placing against, but they were asked for every face.
        let mut checked_placeable = false;
        for against in DIRECTIONS {
            let against_pos = [at[0] + against[0], at[1] + against[1], at[2] + against[2]];
            let against_state = calc.bsi.get0(against_pos[0], against_pos[1], against_pos[2]);
            if mh::is_replaceable(against_pos[0], against_pos[1], against_pos[2], &against_state, calc.bsi) {
                continue;
            }
            if !checked_placeable {
                if !ctx.can_survive(at, to_place) || !ctx.placement_plausible(at, to_place) {
                    return None;
                }
                checked_placeable = true;
            }
            let Some(bounds) = ctx.shape_bounds(against_pos) else { continue };
            for m in aabb_side_multipliers(against) {
                let px = f64::from(against_pos[0]) + bounds[0] * m[0] + bounds[3] * (1.0 - m[0]);
                let py = f64::from(against_pos[1]) + bounds[1] * m[1] + bounds[4] * (1.0 - m[1]);
                let pz = f64::from(against_pos[2]) + bounds[2] * m[2] + bounds[5] * (1.0 - m[2]);
                let rot = calc_rotation_from_vec3d(ctx.infer_sneaking_eye_position(), [px, py, pz], ctx.player_rotations());
                let actual = ctx.peek_rotation(rot);
                let opposite = [-against[0], -against[1], -against[2]];
                if let Some((hit, face)) = ctx.ray_trace_towards(actual, ctx.block_reach_distance(), true) {
                    if hit == against_pos && face == opposite {
                        if let Some(slot) = self.has_any_item_that_would_place(ctx, to_place, against_pos, face, [px, py, pz], actual, vs) {
                            return Some(Placement { hotbar_selection: slot, place_against: against_pos, side: opposite, rot });
                        }
                    }
                }
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn has_any_item_that_would_place(&self, ctx: &dyn PlayerContext, desired: &BlockState, clicked: Pos, face: [i32; 3], hit: [f64; 3], rot: Rotation, vs: &ValidSettings) -> Option<usize> {
        for slot in 0..9 {
            // The state depends on the facing of the player sometimes.
            let Some(would_be_placed) = ctx.would_place(slot, clicked, face, hit, rot) else { continue };
            if valid(&would_be_placed, Some(desired), true, vs) {
                return Some(slot);
            }
        }
        None
    }

    // ---- the goal

    #[allow(clippy::too_many_arguments)]
    fn assemble(&mut self, calc: &CalculationContext, ctx: &dyn PlayerContext, s: &super::baritone_settings::Settings, hooks: &BuilderHooks, vs: &ValidSettings, approx_placeable: &[Rc<BlockState>], _log: bool) -> Option<GoalRef> {
        let mut sink = Vec::new();
        self.assemble_log(calc, ctx, s, hooks, vs, approx_placeable, &mut sink)
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble_log(&mut self, calc: &CalculationContext, ctx: &dyn PlayerContext, s: &super::baritone_settings::Settings, hooks: &BuilderHooks, vs: &ValidSettings, approx_placeable: &[Rc<BlockState>], report: &mut Vec<String>) -> Option<GoalRef> {
        let log_missing = approx_placeable.len() > 9;
        let mut placeable: Vec<Pos> = Vec::new();
        let mut breakable: Vec<Pos> = Vec::new();
        let mut source_liquids: Vec<Pos> = Vec::new();
        let mut flowing_liquids: Vec<Pos> = Vec::new();
        let mut missing: Vec<(Rc<BlockState>, u32)> = Vec::new();
        let mut out_of_bounds: Vec<Pos> = Vec::new();
        let positions: Vec<Pos> = self.incorrect_positions.iter().flatten().copied().collect();
        for pos in positions {
            let state = calc.bsi.get0(pos[0], pos[1], pos[2]);
            if state.is_air() {
                match hooks.get_schematic(pos[0], pos[1], pos[2], &state) {
                    None => out_of_bounds.push(pos),
                    Some(desired) => {
                        if contains_blockstate(approx_placeable, &desired, vs) {
                            placeable.push(pos);
                        } else if let Some(entry) = missing.iter_mut().find(|(m, _)| m.name == desired.name && m.props == desired.props) {
                            entry.1 += 1;
                        } else {
                            missing.push((desired, 1));
                        }
                    }
                }
            } else if state.is(Class::Liquid) {
                // If the block itself is JUST a liquid (i.e. not just a waterlogged block), we CANNOT break it.
                if !mh::possibly_flowing(&state) {
                    // If it's a source block then we want to replace it with a throwaway.
                    source_liquids.push(pos);
                } else {
                    flowing_liquids.push(pos);
                }
            } else {
                breakable.push(pos);
            }
        }
        if let Some(set) = self.incorrect_positions.as_mut() {
            for p in &out_of_bounds {
                set.remove(p);
            }
        }
        let to_break: Vec<GoalRef> = breakable.iter().map(|p| self.break_goal(calc, s, *p)).collect();
        let placeable_set: HashSet<Pos> = placeable.iter().copied().collect();
        let mut to_place: Vec<GoalRef> = Vec::new();
        for pos in &placeable {
            let below = [pos[0], pos[1] - 1, pos[2]];
            let below2 = [pos[0], pos[1] - 2, pos[2]];
            if !placeable_set.contains(&below) && !placeable_set.contains(&below2) {
                to_place.push(self.placement_goal(calc, ctx, hooks, *pos));
            }
        }
        for pos in &source_liquids {
            to_place.push(rc(GoalBlock([pos[0], pos[1] + 1, pos[2]])));
        }
        if !to_place.is_empty() {
            return Some(rc(JankyGoalComposite { primary: rc(GoalComposite(to_place)), fallback: rc(GoalComposite(to_break)) }));
        }
        if to_break.is_empty() {
            if log_missing && !missing.is_empty() {
                report.push("Missing materials for at least:".to_owned());
                report.push(missing.iter().map(|(state, n)| format!("{n}x {}", state_label(state))).collect::<Vec<_>>().join("\n"));
            }
            if log_missing && !flowing_liquids.is_empty() {
                report.push("Unreplaceable liquids at at least:".to_owned());
                report.push(flowing_liquids.iter().map(|p| format!("{} {} {}", p[0], p[1], p[2])).collect::<Vec<_>>().join("\n"));
            }
            return None;
        }
        Some(rc(GoalComposite(to_break)))
    }

    fn placement_goal(&self, calc: &CalculationContext, ctx: &dyn PlayerContext, hooks: &BuilderHooks, pos: Pos) -> GoalRef {
        let current = calc.bsi.get0(pos[0], pos[1], pos[2]);
        if !current.is_air() {
            // TODO can this even happen?
            return rc(GoalPlace::new(pos));
        }
        let allow_same_level = !calc.bsi.get0(pos[0], pos[1] + 1, pos[2]).is_air();
        for (dx, dy, dz) in mh::HORIZONTALS_BUT_ALSO_DOWN {
            let against = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
            if let Some(desired) = hooks.get_schematic(pos[0], pos[1], pos[2], &current) {
                if mh::can_place_against(calc, against[0], against[1], against[2]) && ctx.placement_plausible(pos, &desired) {
                    return rc(GoalAdjacent::new(pos, against, allow_same_level));
                }
            }
        }
        rc(GoalPlace::new(pos))
    }

    fn break_goal(&self, calc: &CalculationContext, s: &super::baritone_settings::Settings, pos: Pos) -> GoalRef {
        let above = [pos[0], pos[1] + 1, pos[2]];
        if s.flag("goalBreakFromAbove") && calc.bsi.get0(above[0], above[1], above[2]).is_air() && calc.bsi.get0(above[0], above[1] + 1, above[2]).is_air() {
            return rc(JankyGoalComposite { primary: rc(GoalBreak(pos)), fallback: rc(GoalBreakFromAbove(above)) });
        }
        rc(GoalBreak(pos))
    }
}

fn state_label(state: &BlockState) -> String {
    if state.props.is_empty() {
        format!("Block{{{}}}", state.name)
    } else {
        format!("Block{{{}}}[{}]", state.name, state.props.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(","))
    }
}

/// `aabbSideMultipliers`: where on the face of the block placed against to aim.
fn aabb_side_multipliers(side: [i32; 3]) -> Vec<[f64; 3]> {
    match side {
        [0, 1, 0] => vec![[0.5, 1.0, 0.5], [0.1, 1.0, 0.5], [0.9, 1.0, 0.5], [0.5, 1.0, 0.1], [0.5, 1.0, 0.9]],
        [0, -1, 0] => vec![[0.5, 0.0, 0.5], [0.1, 0.0, 0.5], [0.9, 0.0, 0.5], [0.5, 0.0, 0.1], [0.5, 0.0, 0.9]],
        _ => {
            let x = if side[0] == 0 { 0.5 } else { f64::from(1 + side[0]) / 2.0 };
            let z = if side[2] == 0 { 0.5 } else { f64::from(1 + side[2]) / 2.0 };
            vec![[x, 0.25, z], [x, 0.75, z]]
        }
    }
}

impl Process for BuilderProcess {
    fn name(&self) -> &'static str {
        "builder"
    }

    fn display(&self) -> String {
        if self.paused {
            "Builder Paused".to_owned()
        } else {
            format!("Building {}", self.name.as_deref().unwrap_or("null"))
        }
    }

    fn is_active(&self) -> bool {
        self.schematic.is_some()
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, safe_to_cancel: bool) -> PathingCommand {
        self.expected_start = core.pathing.expected_segment_start();
        self.tick(core, game, calc_failed, safe_to_cancel, 0)
    }

    fn on_lost_control(&mut self, core: &mut Core, _game: &mut Game<'_>) {
        self.on_lost_control_inner(core);
    }

    fn cancel(&mut self) {
        self.incorrect_positions = None;
        self.name = None;
        self.schematic = None;
        self.real_schematic = None;
        self.layer = 0;
        self.num_repeats = 0;
        self.paused = false;
        self.observed_completed = HashSet::new();
    }
}
