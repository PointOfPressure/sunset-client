// baritone.pathing.path.PathExecutor: walks a path, one movement at a time. Each tick it works out which
// movement the player is in, checks that the movement is still possible and not taking too long, and lets
// it steer. It also decides when to sprint, when to skip ahead, and when to give up on the path.
use super::baritone_calc_path::Path;
use super::baritone_calculation_context::CalculationContext;
use super::baritone_costs::COST_INF;
use super::baritone_movement::{Kind, Movement};
use super::baritone_movement_exec::{open_doors, Env};
use super::baritone_movement_helper as mh;
use super::baritone_movement_state::{Input, MovementState, MovementStatus};
use super::baritone_player_context::PlayerContext;
use super::baritone_rotation::calc_rotation_from_vec3d;
use super::baritone_sprint_jump::{SprintJump, MAX_TICKS as FLIGHT_MAX_TICKS};
use super::baritone_settings::Settings;
use super::baritone_utils::{maybe_censor, Pos};
use std::collections::HashSet;

const MAX_MAX_DIST_FROM_PATH: f64 = 3.0;
const MAX_DIST_FROM_PATH: f64 = 2.0;
const MAX_TICKS_AWAY: i32 = 200;
const MAX_RECURSION: u32 = 24;

/// What the executor asks of the rest of the pathing behavior each tick.
pub struct ExecEnv<'a> {
    pub ctx: &'a dyn PlayerContext,
    pub calc: &'a CalculationContext<'a>,
    pub settings: &'a Settings,
    /// The best path of a search in progress (`behavior.getInProgress()...bestPathSoFar()`).
    pub in_progress_best: Option<&'a Path>,
    pub in_progress: bool,
    pub ticks: u64,
}

impl<'a> ExecEnv<'a> {
    fn env<'b>(&'b self, path: Option<(&'b [Pos], usize)>) -> Env<'b>
    where
        'a: 'b,
    {
        Env { ctx: self.ctx, calc: self.calc, settings: self.settings, path }
    }
}

pub struct PathExecutor {
    pub path: Path,
    path_position: usize,
    ticks_away: i32,
    ticks_on_current: i32,
    current_movement_original_cost_estimate: f64,
    cost_estimate_index: Option<usize>,
    failed: bool,
    to_break: HashSet<Pos>,
    to_place: HashSet<Pos>,
    to_walk_into: HashSet<Pos>,
    sprint_next_tick: bool,
    /// The sprint jump being flown, if any.
    flight: Option<SprintJump>,
    /// Cut-off tracking for `closestPathPos` etc.
    pub log: Vec<String>,
    depth: u32,
}

fn add(p: Pos, d: (i32, i32, i32)) -> Pos {
    [p[0] + d.0, p[1] + d.1, p[2] + d.2]
}

fn dir_of(m: &Movement) -> (i32, i32, i32) {
    (m.dest[0] - m.src[0], m.dest[1] - m.src[1], m.dest[2] - m.src[2])
}

/// A position for a diagnostic line, hidden while censorCoordinates is on.
fn spot(p: Pos) -> String {
    format!("[{}, {}, {}]", maybe_censor(p[0]), maybe_censor(p[1]), maybe_censor(p[2]))
}

impl PathExecutor {
    pub fn new(path: Path) -> Self {
        Self {
            path,
            path_position: 0,
            ticks_away: 0,
            ticks_on_current: 0,
            current_movement_original_cost_estimate: 0.0,
            cost_estimate_index: None,
            failed: false,
            to_break: HashSet::new(),
            to_place: HashSet::new(),
            to_walk_into: HashSet::new(),
            sprint_next_tick: false,
            log: Vec::new(),
            depth: 0,
            flight: None,
        }
    }

    pub fn position(&self) -> usize {
        self.path_position
    }

    pub fn failed(&self) -> bool {
        self.failed
    }

    pub fn finished(&self) -> bool {
        self.path_position >= self.path.length()
    }

    pub fn is_sprinting(&self) -> bool {
        self.sprint_next_tick
    }

    pub fn to_break(&self) -> &HashSet<Pos> {
        &self.to_break
    }

    pub fn to_place(&self) -> &HashSet<Pos> {
        &self.to_place
    }

    pub fn to_walk_into(&self) -> &HashSet<Pos> {
        &self.to_walk_into
    }

    fn debug(&mut self, text: impl Into<String>) {
        self.log.push(text.into());
    }

    /// One tick. True when the path is done, or the executor can be cut onto the next path.
    pub fn on_tick(&mut self, e: &ExecEnv) -> bool {
        // The Java recurses here when the path position moves; a path whose positions disagree with the player
        // would recurse for ever and take the stack with it, so past a few levels the path is given up.
        if self.depth > MAX_RECURSION {
            self.debug("The path position keeps changing, cancelling path");
            self.cancel(e);
            return true;
        }
        self.depth += 1;
        let done = self.on_tick_inner(e);
        self.depth -= 1;
        done
    }

    fn on_tick_inner(&mut self, e: &ExecEnv) -> bool {
        let ctx = e.ctx;
        if self.flight.is_some() && self.fly(e) {
            return false; // mid air is not a stable state, no matter what the movement we left behind thinks
        }
        if self.path_position == self.path.length() - 1 {
            self.path_position += 1;
        }
        if self.path_position >= self.path.length() {
            return true; // stop bugging me, I'm done
        }
        let where_am_i = ctx.player_feet();
        if !self.path.movements[self.path_position].valid_positions().contains(&where_am_i) {
            // This happens for example when you lag out and get teleported back a couple of blocks.
            for i in 0..self.path_position.min(self.path.length()) {
                if self.path.movements[i].valid_positions().contains(&where_am_i) {
                    let previous_pos = self.path_position;
                    self.path_position = i;
                    for j in self.path_position..=previous_pos {
                        self.path.movements[j].reset();
                    }
                    self.on_change_in_path_position(e);
                    self.on_tick(e);
                    return false;
                }
            }
            // Do not check pathPosition + 1: the movement tells us when it is done (e.g. sneak placing).
            for i in self.path_position + 3..self.path.length().saturating_sub(1) {
                if self.path.movements[i].valid_positions().contains(&where_am_i) {
                    if i - self.path_position > 2 {
                        self.debug(format!("Skipping forward {} steps, to {i}", i - self.path_position));
                    }
                    self.path_position = i - 1;
                    self.on_change_in_path_position(e);
                    self.on_tick(e);
                    return false;
                }
            }
        }
        let status = self.closest_path_pos(e);
        if self.possibly_off_path(e, status, MAX_DIST_FROM_PATH) {
            self.ticks_away += 1;
            if self.ticks_away > MAX_TICKS_AWAY {
                self.debug("Too far away from path for too long, cancelling path");
                self.cancel(e);
                return false;
            }
        } else {
            self.ticks_away = 0;
        }
        if self.possibly_off_path(e, status, MAX_MAX_DIST_FROM_PATH) {
            // Stop right away, we are way too far.
            self.debug("too far from path");
            self.cancel(e);
            return false;
        }
        let positions = self.path.positions.clone();
        let env = e.env(Some((&positions, self.path_position)));
        // The blocks to break, place and walk into, from here on: worked out afresh from the world each tick
        // (the Java caches them per movement and recomputes when one changed).
        let (mut new_break, mut new_place, mut new_walk_into) = (HashSet::new(), HashSet::new(), HashSet::new());
        for i in self.path_position..self.path.movements.len() {
            let m = &self.path.movements[i];
            new_break.extend(m.to_break(&env));
            new_place.extend(m.to_place(&env));
            new_walk_into.extend(m.to_walk_into(&env));
        }
        self.to_break = new_break;
        self.to_place = new_place;
        self.to_walk_into = new_walk_into;
        if self.path_position + 1 < self.path.movements.len() {
            let next_dest = self.path.movements[self.path_position + 1].dest;
            if !ctx.bsi().world_contains_loaded_chunk(next_dest[0], next_dest[2]) {
                self.debug("Pausing since destination is at edge of loaded chunks");
                ctx.clear_all_keys();
                return true;
            }
        }
        let can_cancel = self.path.movements[self.path_position].safe_to_cancel(&env);
        if self.cost_estimate_index != Some(self.path_position) {
            self.cost_estimate_index = Some(self.path_position);
            self.current_movement_original_cost_estimate = self.path.movements[self.path_position].known_cost();
            let lookahead = e.settings.int("costVerificationLookahead").max(1) as usize;
            for i in 1..lookahead {
                if self.path_position + i >= self.path.length().saturating_sub(1) {
                    break;
                }
                if self.path.movements[self.path_position + i].calculate_cost(e.calc) >= COST_INF && can_cancel {
                    self.debug("Something has changed in the world and a future movement has become impossible. Cancelling.");
                    self.cancel(e);
                    return true;
                }
            }
        }
        let current_cost = self.path.movements[self.path_position].recalculate_cost(e.calc);
        if current_cost >= COST_INF && can_cancel {
            self.debug("Something has changed in the world and this movement has become impossible. Cancelling.");
            self.cancel(e);
            return true;
        }
        if !self.path.movements[self.path_position].calculated_while_loaded && current_cost - self.current_movement_original_cost_estimate > e.settings.float("maxCostIncrease") && can_cancel {
            self.debug(format!("Original cost {} current cost {current_cost}. Cancelling.", self.current_movement_original_cost_estimate));
            self.cancel(e);
            return true;
        }
        if self.should_pause(e, &env) {
            self.debug("Pausing since current best path is a backtrack");
            ctx.clear_all_keys();
            return true;
        }
        let movement_status = self.path.movements[self.path_position].update(&env);
        if matches!(movement_status, MovementStatus::Unreachable | MovementStatus::Failed) {
            self.debug(format!("Movement returns status {movement_status:?}"));
            self.cancel(e);
            return true;
        }
        if movement_status == MovementStatus::Success {
            self.path_position += 1;
            self.on_change_in_path_position(e);
            self.on_tick(e);
            return true;
        }
        if e.settings.flag("chatDebug") && self.ticks_on_current % 40 == 39 {
            let m = &self.path.movements[self.path_position];
            self.debug(format!("{:?} {} -> {} at {} y {}: {:?} holding {:?}", m.kind, spot(m.src), spot(m.dest), spot(ctx.player_feet()), maybe_censor(ctx.position()[1] as i32), movement_status, m.last_inputs));
        }
        self.sprint_next_tick = self.should_sprint_next_tick(e, &env);
        if !self.sprint_next_tick {
            ctx.set_sprinting(false); // letting go of control does not make you stop sprinting
        }
        self.ticks_on_current += 1;
        if f64::from(self.ticks_on_current) > self.current_movement_original_cost_estimate + e.settings.float("movementTimeoutTicks") {
            self.debug(format!("This movement has taken too long ({} ticks, expected {}). Cancelling.", self.ticks_on_current, self.current_movement_original_cost_estimate));
            self.cancel(e);
            return true;
        }
        can_cancel && self.flight.is_none() // movement is in progress, but if it reports cancellable, PathingBehavior may cut onto the next path
    }

    fn center_distance(pos: Pos, p: [f64; 3]) -> f64 {
        let (dx, dy, dz) = (f64::from(pos[0]) + 0.5 - p[0], f64::from(pos[1]) + 0.5 - p[1], f64::from(pos[2]) + 0.5 - p[2]);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// closestPathPos: how far the player is from the nearest valid position of the path, and which.
    fn closest_path_pos(&self, e: &ExecEnv) -> (f64, Pos) {
        let p = e.ctx.position();
        let mut best = -1.0;
        let mut best_pos = [0, 0, 0];
        for m in &self.path.movements {
            for pos in m.valid_positions() {
                let dist = Self::center_distance(pos, p);
                if dist < best || best == -1.0 {
                    best = dist;
                    best_pos = pos;
                }
            }
        }
        (best, best_pos)
    }

    fn possibly_off_path(&self, e: &ExecEnv, status: (f64, Pos), leniency: f64) -> bool {
        if status.0 > leniency {
            if self.path.movements[self.path_position].kind == Kind::Fall {
                let fall_dest = self.path.positions[self.path_position + 1]; // positions[pathPosition] is the block fallen off of
                // Ignore Y by using the flat distance.
                let p = e.ctx.position();
                let (dx, dz) = (f64::from(fall_dest[0]) + 0.5 - p[0], f64::from(fall_dest[2]) + 0.5 - p[2]);
                return dx.hypot(dz) >= leniency;
            }
            return true;
        }
        false
    }

    /// shouldPause: a better path is being worked out and the player stands on it.
    fn should_pause(&self, e: &ExecEnv, env: &Env) -> bool {
        if !e.in_progress {
            return false;
        }
        let ctx = e.ctx;
        if !ctx.on_ground() {
            return false;
        }
        let feet = ctx.player_feet();
        if !mh::can_walk_on_at(e.calc, feet[0], feet[1] - 1, feet[2]) {
            return false;
        }
        if !mh::can_walk_through_at(e.calc, feet[0], feet[1], feet[2]) || !mh::can_walk_through_at(e.calc, feet[0], feet[1] + 1, feet[2]) {
            return false;
        }
        if !self.path.movements[self.path_position].safe_to_cancel(env) {
            return false;
        }
        let Some(best) = e.in_progress_best else { return false };
        if best.positions.len() < 3 {
            return false; // not long enough yet to justify pausing: far from certain we will actually take this route
        }
        best.positions[1..].contains(&feet)
    }

    /// shouldSprintNextTick.
    fn should_sprint_next_tick(&mut self, e: &ExecEnv, env: &Env) -> bool {
        let ctx = e.ctx;
        let requested = ctx.is_input_forced_down(Input::Sprint);
        ctx.set_input_force_state(Input::Sprint, false);
        if !e.settings.flag("allowSprint") || ctx.food_level() <= 6 {
            return false;
        }
        let pp = self.path_position;
        let len = self.path.length();
        let current_kind = self.path.movements[pp].kind;
        if e.settings.flag("sprintJumping") && !ctx.is_input_forced_down(Input::Sneak) {
            self.flight = SprintJump::plan(e, &self.path, pp, env);
            if self.flight.is_some() {
                self.steer(e, true);
                return true;
            }
        }
        if current_kind == Kind::Traverse && pp + 3 < len {
            if self.path.movements[pp + 1].kind == Kind::Ascend && self.sprintable_ascend(e, pp, pp + 1, pp + 2) {
                if self.skip_now(e, pp) {
                    self.debug("Skipping traverse to straight ascend");
                    self.path_position += 1;
                    self.on_change_in_path_position(e);
                    self.on_tick(e);
                    ctx.set_input_force_state(Input::Jump, true);
                    return true;
                }
                self.debug("Too far to the side to safely sprint ascend");
            }
        }
        if requested {
            if e.settings.flag("headHitters") {
                self.head_hit_jump(e, env, pp);
            }
            return true;
        }
        if current_kind == Kind::Descend {
            if pp + 2 < len {
                let next = &self.path.movements[pp + 1];
                if mh::can_use_frost_walker(e.calc, &env.ctx.bsi().get0(next.dest[0], next.dest[1] - 1, next.dest[2])) && matches!(next.kind, Kind::Traverse | Kind::Parkour { .. }) {
                    let could_place_instead = e.settings.flag("allowPlace") && ctx.has_generic_throwaway() && matches!(next.kind, Kind::Parkour { .. });
                    let cur = dir_of(&self.path.movements[pp]);
                    let nd = dir_of(next);
                    let flat_cur = (cur.0, cur.2);
                    let flat_next = (nd.0, nd.2);
                    let same_flat_direction = flat_cur == flat_next && flat_cur != (0, 0);
                    if same_flat_direction && !could_place_instead {
                        self.path.movements[pp].force_safe_mode = true;
                    }
                }
            }
            if self.path.movements[pp].safe_mode(env) && !self.path.movements[pp].skip_to_ascend(env) {
                self.debug("Sprinting would be unsafe");
                return false;
            }
            if pp + 2 < len {
                let (cur_dir, next_kind, next_dir) = (dir_of(&self.path.movements[pp]), self.path.movements[pp + 1].kind, dir_of(&self.path.movements[pp + 1]));
                // current.getDirection().above().equals(next.getDirection().below())
                if next_kind == Kind::Ascend && (cur_dir.0, cur_dir.1 + 1, cur_dir.2) == (next_dir.0, next_dir.1 - 1, next_dir.2) {
                    self.path_position += 1;
                    self.on_change_in_path_position(e);
                    self.on_tick(e);
                    self.debug("Skipping descend to straight ascend");
                    return true;
                }
                if self.can_sprint_from_descend_into(e, pp, pp + 1) {
                    if self.path.movements[pp + 1].kind == Kind::Descend && pp + 3 < len {
                        let next_next_kind = self.path.movements[pp + 2].kind;
                        if next_next_kind == Kind::Descend && !self.can_sprint_from_descend_into(e, pp + 1, pp + 2) {
                            return false;
                        }
                    }
                    if ctx.player_feet() == self.path.movements[pp].dest {
                        self.path_position += 1;
                        self.on_change_in_path_position(e);
                        self.on_tick(e);
                    }
                    return true;
                }
            }
        }
        if current_kind == Kind::Ascend && pp != 0 {
            let prev_kind = self.path.movements[pp - 1].kind;
            let (prev_dir, cur_dir) = (dir_of(&self.path.movements[pp - 1]), dir_of(&self.path.movements[pp]));
            if prev_kind == Kind::Descend && (prev_dir.0, prev_dir.1 + 1, prev_dir.2) == (cur_dir.0, cur_dir.1 - 1, cur_dir.2) {
                let center = self.path.movements[pp].src[1] + 1;
                if ctx.position()[1] >= f64::from(center) - 0.07 {
                    ctx.set_input_force_state(Input::Jump, false);
                    return true;
                }
            }
            if pp + 2 < len && prev_kind == Kind::Traverse && self.sprintable_ascend(e, pp - 1, pp, pp + 1) {
                return true;
            }
        }
        if current_kind == Kind::Fall {
            if let Some((target, fall_dest)) = self.override_fall(e, env, pp) {
                assert!(self.path.positions.contains(&fall_dest), "Fall override at {:?} returned illegal destination {fall_dest:?}", self.path.movements[pp].src);
                if ctx.player_feet() == fall_dest {
                    self.path_position = self.path.positions.iter().position(|p| *p == fall_dest).unwrap_or(pp);
                    self.on_change_in_path_position(e);
                    self.on_tick(e);
                    return true;
                }
                ctx.clear_all_keys();
                let (src, dest) = (self.path.movements[pp].src, self.path.movements[pp].dest);
                let mut fake_state = MovementState::new();
                if !open_doors(env, &mut fake_state, src, [dest[0], src[1], dest[2]]) {
                    if let Some(rotation) = fake_state.target.rotation {
                        ctx.update_look_target(rotation, fake_state.target.force_rotations);
                    }
                    for (input, forced) in &fake_state.inputs {
                        ctx.set_input_force_state(*input, *forced);
                    }
                    return true;
                }
                ctx.update_look_target(calc_rotation_from_vec3d(ctx.player_head(), target, ctx.player_rotations()), false);
                ctx.set_input_force_state(Input::MoveForward, true);
                return true;
            }
        }
        false
    }

    fn head_hit_jump(&self, e: &ExecEnv, env: &Env, pp: usize) {
        let current = &self.path.movements[pp];
        let dir = dir_of(current);
        if !self.can_start_head_hitting(e, env, current) || !self.under_head_bonk_ceiling(e, dir, e.ctx.player_feet()) || !self.clear_of_ledges_ahead(e, env, dir) {
            return;
        }
        e.ctx.set_input_force_state(Input::Jump, true);
    }

    fn can_start_head_hitting(&self, e: &ExecEnv, env: &Env, current: &Movement) -> bool {
        let dir = dir_of(current);
        if !(current.kind == Kind::Traverse || matches!(current.kind, Kind::Diagonal { .. })) || dir.1 != 0 {
            return false; // head hitting only applies to flat walking movements
        }
        if matches!(current.kind, Kind::Diagonal { .. }) && !e.settings.flag("headHittersDiagonal") {
            return false;
        }
        let feet = e.ctx.player_feet();
        if !e.ctx.on_ground() || mh::is_liquid(&e.ctx.bsi().get0(feet[0], feet[1], feet[2])) || e.ctx.is_in_water() {
            return false; // hopping in water or on a vine just sticks us to it instead
        }
        if !current.to_break(env).is_empty() {
            return false; // breaking is about 5x slower when jumping
        }
        !e.ctx.is_input_forced_down(Input::Sneak)
    }

    fn under_head_bonk_ceiling(&self, e: &ExecEnv, dir: (i32, i32, i32), feet: Pos) -> bool {
        let ceiling = [feet[0], feet[1] + 2, feet[2]];
        let state = e.ctx.bsi().get0(ceiling[0], ceiling[1], ceiling[2]);
        if mh::fully_passable(e.calc, ceiling[0], ceiling[1], ceiling[2]) || !mh::is_block_normal_cube(&state) {
            return false; // not under a ceiling yet, or something overhead we cannot reliably bonk against
        }
        self.clear_of_ceiling_entrance(e, feet, dir.0, 0) && self.clear_of_ceiling_entrance(e, feet, 0, dir.2)
    }

    fn clear_of_ceiling_entrance(&self, e: &ExecEnv, feet: Pos, dx: i32, dz: i32) -> bool {
        if dx == 0 && dz == 0 {
            return true;
        }
        let behind = [feet[0] - dx, feet[1] + 2, feet[2] - dz];
        if mh::fully_passable(e.calc, behind[0], behind[1], behind[2]) {
            let p = e.ctx.position();
            let flat_dist = (f64::from(dx) * (f64::from(behind[0]) + 0.5 - p[0])).abs() + (f64::from(dz) * (f64::from(behind[2]) + 0.5 - p[2])).abs();
            return flat_dist >= 0.8; // just entered: wait until clear of the entrance face
        }
        true
    }

    fn clear_of_ledges_ahead(&self, e: &ExecEnv, env: &Env, dir: (i32, i32, i32)) -> bool {
        for i in 1..=2 {
            if self.path_position + i + 2 > self.path.length() || dir_of(&self.path.movements[self.path_position + i]) != dir {
                return false; // the path turns or ends within two blocks: do not add momentum
            }
            let next = &self.path.movements[self.path_position + i];
            if !next.to_place(env).is_empty() {
                return false; // it will place its own support, and momentum can arrive before the blocks do
            }
            let floor = [next.dest[0], next.dest[1] - 1, next.dest[2]];
            if mh::is_liquid(&e.ctx.bsi().get0(floor[0], floor[1], floor[2])) || !mh::can_walk_on_at(e.calc, floor[0], floor[1], floor[2]) {
                return false; // no real support under the destination yet
            }
        }
        true
    }

    fn override_fall(&self, e: &ExecEnv, env: &Env, pp: usize) -> Option<([f64; 3], Pos)> {
        let movement = &self.path.movements[pp];
        let dir = dir_of(movement);
        if dir.1 < -3 {
            return None;
        }
        if !movement.to_break(env).is_empty() {
            return None; // it is breaking
        }
        let flat_dir = (dir.0, 0, dir.2);
        let mut i = pp + 1;
        'outer: while i < self.path.length() - 1 && i < pp + 3 {
            let next = &self.path.movements[i];
            if next.kind != Kind::Traverse {
                break;
            }
            if flat_dir != dir_of(next) {
                break;
            }
            for y in next.dest[1]..=movement.src[1] + 1 {
                if !mh::fully_passable(e.calc, next.dest[0], y, next.dest[2]) {
                    break 'outer;
                }
            }
            if !mh::can_walk_on_at(e.calc, next.dest[0], next.dest[1] - 1, next.dest[2]) {
                break;
            }
            i += 1;
        }
        i -= 1;
        if i == pp {
            return None; // no valid extension exists
        }
        let len = (i - pp) as f64 - 0.4;
        Some((
            [f64::from(flat_dir.0) * len + f64::from(movement.dest[0]) + 0.5, f64::from(movement.dest[1]), f64::from(flat_dir.2) * len + f64::from(movement.dest[2]) + 0.5],
            add(movement.dest, (flat_dir.0 * (i - pp) as i32, 0, flat_dir.2 * (i - pp) as i32)),
        ))
    }

    fn skip_now(&self, e: &ExecEnv, idx: usize) -> bool {
        let current = &self.path.movements[idx];
        let dir = dir_of(current);
        let p = e.ctx.position();
        let off_target = (f64::from(dir.0) * (f64::from(current.src[2]) + 0.5 - p[2])).abs() + (f64::from(dir.2) * (f64::from(current.src[0]) + 0.5 - p[0])).abs();
        if off_target > 0.1 {
            return false;
        }
        let head_bonk = [current.src[0] - dir.0, current.src[1] - dir.1 + 2, current.src[2] - dir.2];
        if mh::fully_passable(e.calc, head_bonk[0], head_bonk[1], head_bonk[2]) {
            return true;
        }
        let flat_dist = (f64::from(dir.0) * (f64::from(head_bonk[0]) + 0.5 - p[0])).abs() + (f64::from(dir.2) * (f64::from(head_bonk[2]) + 0.5 - p[2])).abs();
        flat_dist > 0.8
    }

    fn sprintable_ascend(&self, e: &ExecEnv, current_i: usize, next_i: usize, nextnext_i: usize) -> bool {
        if !e.settings.flag("sprintAscends") {
            return false;
        }
        let (current, next, nextnext) = (&self.path.movements[current_i], &self.path.movements[next_i], &self.path.movements[nextnext_i]);
        let (cd, nd, nnd) = (dir_of(current), dir_of(next), dir_of(nextnext));
        if cd != (nd.0, nd.1 - 1, nd.2) {
            return false;
        }
        if nnd.0 != nd.0 || nnd.2 != nd.2 {
            return false;
        }
        let cdown = [current.dest[0], current.dest[1] - 1, current.dest[2]];
        if !mh::can_walk_on_at(e.calc, cdown[0], cdown[1], cdown[2]) {
            return false;
        }
        let ndown = [next.dest[0], next.dest[1] - 1, next.dest[2]];
        if !mh::can_walk_on_at(e.calc, ndown[0], ndown[1], ndown[2]) {
            return false;
        }
        let env = e.env(None);
        if !next.to_break(&env).is_empty() {
            return false; // it is breaking
        }
        for x in 0..2 {
            for y in 0..3 {
                let mut chk = [current.src[0], current.src[1] + y, current.src[2]];
                if x == 1 {
                    chk = add(chk, cd);
                }
                if !mh::fully_passable(e.calc, chk[0], chk[1], chk[2]) {
                    return false;
                }
            }
        }
        if mh::avoid_walking_into(e.calc, &e.ctx.bsi().get0(current.src[0], current.src[1] + 3, current.src[2])) {
            return false;
        }
        !mh::avoid_walking_into(e.calc, &e.ctx.bsi().get0(next.dest[0], next.dest[1] + 2, next.dest[2]))
    }

    fn can_sprint_from_descend_into(&self, e: &ExecEnv, current_i: usize, next_i: usize) -> bool {
        let (current, next) = (&self.path.movements[current_i], &self.path.movements[next_i]);
        let cd = dir_of(current);
        if next.kind == Kind::Descend && dir_of(next) == cd {
            return true;
        }
        let ahead = add(current.dest, cd);
        if !mh::can_walk_on_at(e.calc, ahead[0], ahead[1], ahead[2]) {
            return false;
        }
        if next.kind == Kind::Traverse && dir_of(next) == cd {
            return true;
        }
        matches!(next.kind, Kind::Diagonal { .. }) && e.settings.flag("allowOvershootDiagonalDescend")
    }

    /// True if we're still in the air and this tick is handled.
    fn fly(&mut self, e: &ExecEnv) -> bool {
        let ctx = e.ctx;
        let airborne = !ctx.on_ground() && !ctx.is_in_water() && !ctx.is_in_lava() && !ctx.on_climbable();
        if airborne && self.flight.as_ref().is_some_and(|f| f.ticks < FLIGHT_MAX_TICKS) && e.settings.flag("sprintJumping") {
            self.steer(e, false);
            self.sprint_next_tick = true;
            return true;
        }
        // We probably flew over a few movements, pick up at whichever one we came down in (diagonal side cells
        // count). Anywhere else and the usual valid positions / off path recovery takes it from here.
        let reach = self.flight.as_ref().map_or(0, |f| f.floors.len());
        let end = self.path.movements.len().min(self.path_position + reach);
        for i in self.path_position..end {
            if self.path.movements[i].valid_positions().contains(&ctx.player_feet()) {
                self.path_position = i;
                break;
            }
        }
        self.flight = None;
        self.on_change_in_path_position(e);
        false
    }

    fn steer(&mut self, e: &ExecEnv, takeoff: bool) {
        // The movements steer at their own dest and drop W the moment our feet are a block up ("Wrong Y
        // coordinate"), which was the mid air stall, so while we're up here nobody else gets a say. Space only on
        // takeoff: letting go in the air resets the vanilla 10 tick jump delay.
        let ctx = e.ctx;
        ctx.clear_all_keys();
        if let Some(flight) = self.flight.as_mut() {
            let rotation = flight.steer(ctx.position(), ctx.delta_movement(), ctx.player_rotations().pitch);
            ctx.update_look_target(rotation, false);
        }
        ctx.set_input_force_state(Input::MoveForward, true);
        ctx.set_input_force_state(Input::Jump, takeoff);
    }

    fn on_change_in_path_position(&mut self, e: &ExecEnv) {
        e.ctx.clear_all_keys();
        if e.settings.flag("chatDebug") && self.path_position < self.path.movements.len() {
            let m = &self.path.movements[self.path_position];
            let feet = e.ctx.player_feet();
            self.debug(format!("[tick {}] movement {}/{}: {:?} {} -> {}, took {} ticks, feet now {}", e.ticks, self.path_position, self.path.movements.len(), m.kind, spot(m.src), spot(m.dest), self.ticks_on_current, spot(feet)));
        }
        self.ticks_on_current = 0;
    }

    fn cancel(&mut self, e: &ExecEnv) {
        e.ctx.clear_all_keys();
        e.ctx.stop_breaking_block();
        self.path_position = self.path.length() + 3;
        self.failed = true;
    }

    /// snipsnapifpossible: jump to where the player already is on the path.
    pub fn snipsnapifpossible(&mut self, e: &ExecEnv) -> bool {
        let ctx = e.ctx;
        let feet = ctx.player_feet();
        if !ctx.on_ground() && !mh::is_liquid(&ctx.bsi().get0(feet[0], feet[1], feet[2])) {
            return false;
        }
        if ctx.delta_movement()[1] < -0.1 {
            return false; // so do not
        }
        let Some(index) = self.path.positions.iter().position(|p| *p == feet) else { return false };
        self.path_position = index; // jump directly to the current position
        ctx.clear_all_keys();
        true
    }

    /// trySplice: carry on with `next` when it starts where this path ends; else drop the old history.
    pub fn try_splice(&self, next: Option<&PathExecutor>, e: &ExecEnv) -> Option<PathExecutor> {
        match next {
            None => self.cut_if_too_long(e),
            Some(next) => match Path::try_splice(&self.path, &next.path, false) {
                Some(path) => {
                    assert!(path.dest() == next.path.dest(), "Path has end {:?} instead of {:?} after splicing", path.dest(), next.path.dest());
                    let mut ret = PathExecutor::new(path);
                    ret.path_position = self.path_position;
                    ret.current_movement_original_cost_estimate = self.current_movement_original_cost_estimate;
                    ret.cost_estimate_index = self.cost_estimate_index;
                    ret.ticks_on_current = self.ticks_on_current;
                    ret.flight = self.flight.clone(); // a new path showing up doesn't make us any less airborne
                    Some(ret)
                }
                None => self.cut_if_too_long(e),
            },
        }
    }

    fn cut_if_too_long(&self, e: &ExecEnv) -> Option<PathExecutor> {
        if self.path_position as i32 > e.settings.int("maxPathHistoryLength") {
            let cutoff_amt = e.settings.int("pathHistoryCutoffAmount").max(0) as usize;
            let mut new_path = self.path.cut_off(self.path.length() - 1);
            new_path.positions.drain(..cutoff_amt);
            new_path.movements.drain(..cutoff_amt);
            assert!(new_path.dest() == self.path.dest(), "Path has end {:?} instead of {:?} after trimming its start", new_path.dest(), self.path.dest());
            let shorter = format!("Discarding earliest segment movements, length cut from {} to {}", self.path.length(), new_path.length());
            let mut ret = PathExecutor::new(new_path);
            ret.debug(shorter);
            ret.path_position = self.path_position - cutoff_amt;
            ret.current_movement_original_cost_estimate = self.current_movement_original_cost_estimate;
            ret.cost_estimate_index = self.cost_estimate_index.map(|i| i.saturating_sub(cutoff_amt));
            ret.ticks_on_current = self.ticks_on_current;
            ret.flight = self.flight.clone();
            return Some(ret);
        }
        None
    }
}
