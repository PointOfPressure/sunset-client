// baritone.pathing.movement.Movement.update / updateState / prepared and the updateState of the eight
// subclasses, plus the half of MovementHelper that steers the player: moveTowards, openDoors,
// attemptToPlaceABlock. Each tick a movement looks at the player and the world and fills its
// MovementState with the keys to hold and the aim to turn to; the caller hands that to the game.
use super::baritone_block_state::{BlockState, Class};
use super::baritone_calculation_context::CalculationContext;
use super::baritone_costs::COST_INF;
use super::baritone_movement::{Kind, Movement};
use super::baritone_movement_diagonal as diagonal;
use super::baritone_movement_helper as mh;
use super::baritone_movement_state::{Input, MovementOption, MovementState, MovementStatus, MovementTarget};
use super::baritone_player_context::PlayerContext;
use super::baritone_rotation::{block_pos_center, calc_rotation_from_vec3d, yaw_distance_from_offset, Rotation};
use super::baritone_settings::Settings;
use super::baritone_utils::Pos;

/// What a movement needs to run: the player, a fresh calculation context, the settings, and where the
/// path is (for turning into a bend early while swimming).
pub struct Env<'a> {
    pub ctx: &'a dyn PlayerContext,
    pub calc: &'a CalculationContext<'a>,
    pub settings: &'a Settings,
    pub path: Option<(&'a [Pos], usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceResult {
    ReadyToPlace,
    Attempting,
    NoOption,
}

fn above(p: Pos, n: i32) -> Pos {
    [p[0], p[1] + n, p[2]]
}

fn walk_through(env: &Env, p: Pos) -> bool {
    mh::can_walk_through_at(env.calc, p[0], p[1], p[2])
}

fn walk_on(env: &Env, p: Pos) -> bool {
    mh::can_walk_on_at(env.calc, p[0], p[1], p[2])
}

fn block_at(env: &Env, p: Pos) -> std::rc::Rc<BlockState> {
    env.ctx.bsi().get0(p[0], p[1], p[2])
}

// ---------------------------------------------------------------------------------------
// MovementHelper: steering.

/// MovementHelper.moveTowards: aim at the block (keeping the pitch) and push forward.
pub fn move_towards(env: &Env, state: &mut MovementState, pos: Pos) {
    let rot = calc_rotation_from_vec3d(env.ctx.player_head(), block_pos_center(pos), env.ctx.player_rotations()).with_pitch(env.ctx.player_rotations().pitch);
    state.set_target(MovementTarget::new(rot, false)).set_input(Input::MoveForward, true);
}

pub fn move_towards_without_rotation_yaw(env: &Env, state: &mut MovementState, ideal_yaw: f32) {
    let deg = std::f32::consts::PI / 180.0;
    let yaw = env.ctx.player_rotations().yaw;
    let options = MovementOption::get_options((yaw * deg).sin(), (yaw * deg).cos(), env.settings.flag("allowSprint"));
    let (ix, iz) = ((ideal_yaw * deg).sin(), (ideal_yaw * deg).cos());
    if let Some(best) = options.iter().min_by(|a, b| a.distance_to_sq(ix, iz).total_cmp(&b.distance_to_sq(ix, iz))) {
        best.set_inputs(state);
    }
}

#[allow(dead_code)]
pub fn move_towards_without_rotation(env: &Env, state: &mut MovementState, dest: Pos) {
    let ideal_yaw = calc_rotation_from_vec3d(env.ctx.player_head(), block_pos_center(dest), env.ctx.player_rotations()).yaw;
    move_towards_without_rotation_yaw(env, state, ideal_yaw);
}

pub fn move_towards_with_slight_rotation(env: &Env, state: &mut MovementState, dest: Pos) {
    let ideal_yaw = calc_rotation_from_vec3d(env.ctx.player_head(), block_pos_center(dest), env.ctx.player_rotations()).yaw;
    let distance = yaw_distance_from_offset(env.ctx.player_rotations().yaw, ideal_yaw) % 45.0;
    let new_yaw = if distance > 0.0 {
        if distance > 22.5 { distance - 45.0 } else { distance }
    } else if distance < -22.5 {
        distance + 45.0
    } else {
        distance
    };
    let r = env.ctx.player_rotations();
    state.set_target(MovementTarget::new(Rotation::new(r.yaw - new_yaw, r.pitch), true));
    move_towards_without_rotation_yaw(env, state, ideal_yaw);
}

/// DoorBlock.isWoodenDoor.
fn is_wooden_door(state: &BlockState) -> bool {
    mh::is_wooden_door(state)
}

/// Direction.getClockWise of a horizontal direction as (dx, dz).
fn clockwise(d: (i32, i32)) -> (i32, i32) {
    (-d.1, d.0)
}

fn counter_clockwise(d: (i32, i32)) -> (i32, i32) {
    (d.1, -d.0)
}

/// MovementHelper.isDoorPassable.
pub fn is_door_passable(state: &BlockState, side: (i32, i32)) -> bool {
    let open = state.flag("open");
    let closed_facing = diagonal::facing_step(state.prop("facing").unwrap_or("north"));
    let open_facing = if state.prop("hinge") == Some("left") { clockwise(closed_facing) } else { counter_clockwise(closed_facing) };
    side != if open { open_facing } else { closed_facing }
}

/// MovementHelper.isGatePassable.
pub fn is_gate_passable(env: &Env, gate_pos: Pos, player_pos: Pos) -> bool {
    if player_pos == gate_pos {
        return false;
    }
    let state = block_at(env, gate_pos);
    if !state.is(Class::FenceGate) {
        return true;
    }
    state.flag("open")
}

/// MovementHelper.openDoors: click a closed wooden door in the way of a step from `from` to `to`.
pub fn open_doors(env: &Env, state: &mut MovementState, from: Pos, to: Pos) -> bool {
    let direction = ((to[0] - from[0]).signum(), (to[2] - from[2]).signum());
    // The Java picks the direction d with from.relative(d) == to; a vertical step has none of the four.
    let direction = if (to[0] - from[0]).abs() + (to[2] - from[2]).abs() == 1 && to[1] == from[1] { direction } else { direction };
    for pos in [from, to, above(from, 1), above(to, 1)] {
        let side = if pos == to || pos == above(to, 1) { direction } else { (-direction.0, -direction.1) };
        let door = block_at(env, pos);
        if is_wooden_door(&door) && !is_door_passable(&door, side) {
            let rot = calc_rotation_from_vec3d(env.ctx.player_head(), block_pos_center(pos), env.ctx.player_rotations());
            state.set_target(MovementTarget::new(rot, true)).set_input(Input::ClickRight, true);
            return false;
        }
    }
    true
}

/// MovementHelper.steppingOnBlocks: the blocks under the feet, by how the box overlaps them.
pub fn stepping_on_blocks(env: &Env) -> Vec<Pos> {
    let pos = env.ctx.position();
    let feet = env.ctx.player_feet();
    let half = 0.3;
    let (min_x, max_x, min_z, max_z) = (pos[0] - half, pos[0] + half, pos[2] - half, pos[2] + half);
    let mut blocks = Vec::new();
    for x in -1..=1 {
        for z in -1..=1 {
            let (bx, bz) = (f64::from(feet[0] + x), f64::from(feet[2] + z));
            if max_x > bx && min_x < bx + 1.0 && max_z > bz && min_z < bz + 1.0 {
                blocks.push([feet[0] + x, feet[1] - 1, feet[2] + z]);
            }
        }
    }
    blocks
}

/// MovementHelper.attemptToPlaceABlock: aim at the best face to place against, report whether the aim is
/// already right.
pub fn attempt_to_place_a_block(env: &Env, state: &mut MovementState, place_at: Pos, prefer_down: bool, would_sneak: bool) -> PlaceResult {
    let ctx = env.ctx;
    let direct = ctx.reachable(place_at, ctx.block_reach_distance(), would_sneak); // a block there is assumed replaceable
    let mut found = false;
    if let Some(rot) = direct {
        state.set_target(MovementTarget::new(rot, true));
        found = true;
    }
    for (dx, dy, dz) in mh::HORIZONTALS_BUT_ALSO_DOWN {
        let against1 = [place_at[0] + dx, place_at[1] + dy, place_at[2] + dz];
        if mh::can_place_against(env.calc, against1[0], against1[1], against1[2]) {
            if !ctx.select_throwaway_for_location(false, place_at[0], place_at[1], place_at[2]) {
                ctx.log_debug("bb pls get me some blocks. dirt, netherrack, cobble");
                state.set_status(MovementStatus::Unreachable);
                return PlaceResult::NoOption;
            }
            let face_x = (f64::from(place_at[0]) + f64::from(against1[0]) + 1.0) * 0.5;
            let face_y = (f64::from(place_at[1]) + f64::from(against1[1]) + 0.5) * 0.5;
            let face_z = (f64::from(place_at[2]) + f64::from(against1[2]) + 1.0) * 0.5;
            let eyes = if would_sneak { ctx.infer_sneaking_eye_position() } else { ctx.player_head() };
            let place = calc_rotation_from_vec3d(eyes, [face_x, face_y, face_z], ctx.player_rotations());
            let actual = ctx.peek_rotation(place);
            if let Some((hit, face)) = ctx.ray_trace_towards(actual, ctx.block_reach_distance(), would_sneak) {
                if hit == against1 && [hit[0] + face[0], hit[1] + face[1], hit[2] + face[2]] == place_at {
                    state.set_target(MovementTarget::new(place, true));
                    found = true;
                    if !prefer_down {
                        // If preferDown, the last option is wanted; otherwise the first.
                        break;
                    }
                }
            }
        }
    }
    if let Some(selected) = ctx.selected_block() {
        let side = ctx.selected_face().unwrap_or([0, 0, 0]);
        // The only way selectedBlock == placeAt is for it to be replaceable.
        if selected == place_at
            || (mh::can_place_against(env.calc, selected[0], selected[1], selected[2]) && [selected[0] + side[0], selected[1] + side[1], selected[2] + side[2]] == place_at)
        {
            if would_sneak {
                state.set_input(Input::Sneak, true);
            }
            ctx.select_throwaway_for_location(true, place_at[0], place_at[1], place_at[2]);
            return PlaceResult::ReadyToPlace;
        }
    }
    if found {
        if would_sneak {
            state.set_input(Input::Sneak, true);
        }
        ctx.select_throwaway_for_location(true, place_at[0], place_at[1], place_at[2]);
        return PlaceResult::Attempting;
    }
    PlaceResult::NoOption
}

// ---------------------------------------------------------------------------------------
// Movement.

// Vanilla swim physics per tick, from LivingEntity.jumpInLiquid, Player.travel and LivingEntity.travel:
// holding jump adds SWIM_JUMP_BOOST, then the vertical speed is pulled toward the look vector's y by
// SWIM_PULL (SWIM_PULL_STEEP when looking down steeper than SWIM_STEEP_LOOK), then scaled by WATER_DRAG_Y.
const WATERLINE_DEPTH: f64 = 0.15;
const DEST_CLEARANCE: f64 = 0.3;
const SURFACE_AT_AIR: i32 = 100;
const SWIM_JUMP_BOOST: f64 = 0.04;
const SWIM_PULL: f64 = 0.06;
const SWIM_PULL_STEEP: f64 = 0.085;
const SWIM_STEEP_LOOK: f64 = -0.2;
const WATER_DRAG_Y: f64 = 0.8;
const WATERLINE_GAIN: f64 = 0.3;
const WATERLINE_DAMPING: f64 = 0.9;
/// Coast distance in blocks, per block per tick of speed, at water drag 0.8: 1 / (1 - 0.8).
const WATER_COAST_TICKS: f64 = 5.0;

fn look_for_swim_velocity(velocity: f64, pull: f64) -> f64 {
    // Steady state of v = drag * ((1 - pull) * (v + boost) + pull * look), solved for look.
    let carry = WATER_DRAG_Y * (1.0 - pull);
    (velocity * (1.0 - carry) - carry * SWIM_JUMP_BOOST) / (WATER_DRAG_Y * pull)
}

fn pitch_for_swim_velocity(velocity: f64) -> f32 {
    let mut look = look_for_swim_velocity(velocity, SWIM_PULL);
    if look < SWIM_STEEP_LOOK {
        look = SWIM_STEEP_LOOK.min(look_for_swim_velocity(velocity, SWIM_PULL_STEEP));
    }
    look = look.clamp(-1.0, 1.0);
    // The look vector's y is -sin(pitch).
    (-look.asin().to_degrees()) as f32
}

impl Movement {
    /// Movement.update: run one tick and hand the result to the game. Returns the status.
    pub fn update(&mut self, env: &Env) -> MovementStatus {
        let mut state = std::mem::take(&mut self.state);
        state = self.update_state(env, state);
        let ctx = env.ctx;
        let feet = ctx.player_feet();
        let in_liquid = mh::is_liquid(&block_at(env, feet)) || ctx.is_in_water();
        if in_liquid {
            if self.should_swim(env, &state) {
                // Sprint is what enters and keeps the swim state. Vanilla only starts sprinting from the key
                // on the ground or with the eyes underwater and cancels it at the surface, so arm the flag.
                state.set_input(Input::Sprint, true);
                ctx.set_sprinting(true);
                if state.status == MovementStatus::Running && ctx.is_swimming() && !ctx.on_ground() {
                    self.hold_waterline(env, &mut state);
                    self.pre_turn_into_the_bend(env, &mut state);
                } else if !ctx.is_swimming() && ctx.on_ground() && ctx.position()[1] < f64::from(self.dest[1]) + 0.6 {
                    // The swim state has not latched yet and we are on the bottom: the old bob, for one tick.
                    state.set_input(Input::Jump, true);
                }
            } else {
                if env.settings.flag("allowSwimming") && ctx.is_swimming() {
                    // Standing up to breathe. Drop sprint explicitly or we crawl along the bottom, head under.
                    state.set_input(Input::Sprint, false);
                    ctx.set_sprinting(false);
                }
                if ctx.position()[1] < f64::from(self.dest[1]) + 0.6 {
                    state.set_input(Input::Jump, true);
                }
            }
        }
        if ctx.is_in_wall() {
            if let Some(sel) = ctx.selected_block() {
                ctx.switch_to_best_tool_for(sel);
            }
            state.set_input(Input::ClickLeft, true);
        }
        // If the target has to force the new rotations, or silent move is not used, force the rotations.
        if let Some(rotation) = state.target.rotation {
            ctx.update_look_target(rotation, state.target.force_rotations);
        }
        ctx.clear_all_keys();
        self.last_inputs = state.inputs.iter().filter(|(_, on)| **on).map(|(i, _)| *i).collect();
        for (input, forced) in &state.inputs {
            ctx.set_input_force_state(*input, *forced);
        }
        state.inputs.clear();
        if state.status.is_complete() {
            ctx.clear_all_keys();
        }
        let status = state.status;
        self.state = state;
        status
    }

    fn should_swim(&self, env: &Env, state: &MovementState) -> bool {
        let ctx = env.ctx;
        if !env.settings.flag("allowSwimming") || !ctx.is_in_water() || !env.settings.flag("allowSprint") || ctx.food_level() <= 6 {
            return false;
        }
        if state.status == MovementStatus::Prepping {
            // Something is in the way and prepared() is mining it: the swim pose is the worst place to do
            // that from. Stand up instead: eye out, bobbing at the surface, full speed on the bottom.
            return false;
        }
        if !ctx.on_ground() {
            // Floating: holdWaterline keeps the eye out, so air is never a problem here.
            return true;
        }
        // Shallow water: the swim pose crawls along the bottom with the eye under, so air is a clock.
        // Crawl until it runs low, then stand up and stay standing until it is full.
        let air = ctx.air_supply();
        if ctx.is_under_water() {
            return air >= SURFACE_AT_AIR;
        }
        ctx.is_swimming() || air >= ctx.max_air_supply()
    }

    fn hold_waterline(&self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let feet_y = ctx.position()[1];
        let surface_y = feet_y + ctx.water_fluid_height();
        // The waterline, unless the movement is taking us higher (climbing out onto land).
        let target_y = (surface_y - WATERLINE_DEPTH).max(f64::from(self.dest[1]) + DEST_CLEARANCE);
        let wanted = WATERLINE_GAIN * (target_y - feet_y) - WATERLINE_DAMPING * ctx.delta_movement()[1];
        let mut pitch = pitch_for_swim_velocity(wanted);
        if pitch == ctx.player_rotations().pitch {
            // Same convention as RotationUtils.reachable: equal to the current pitch means "don't care".
            pitch += 0.0001;
        }
        if let Some(rotation) = state.target.rotation {
            state.set_target(MovementTarget::new(rotation.with_pitch(pitch), false));
        }
        state.set_input(Input::Jump, true);
    }

    fn pre_turn_into_the_bend(&self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        if !mh::is_water(&block_at(env, self.dest)) {
            return; // climbing out onto land wants the speed
        }
        if self.dest[0] == self.src[0] && self.dest[2] == self.src[2] {
            return; // vertical (pillar, downward): no corner in the horizontal plane
        }
        let Some((positions, pos)) = env.path else { return };
        if pos + 2 >= positions.len() {
            return; // the path is over: nothing to aim the early turn at
        }
        let next = positions[pos + 2];
        let (ax, az) = (f64::from(self.dest[0] - self.src[0]), f64::from(self.dest[2] - self.src[2]));
        let (bx, bz) = (f64::from(next[0] - self.dest[0]), f64::from(next[2] - self.dest[2]));
        if bx == 0.0 && bz == 0.0 {
            return; // the next leg is vertical (a descend): no corner to cut either
        }
        if ax * bx + az * bz >= 0.99 * ax.hypot(az) * bx.hypot(bz) {
            return; // straight chains share their heading exactly, anything else is a corner
        }
        let dm = ctx.delta_movement();
        let speed = dm[0].hypot(dm[2]);
        let p = ctx.position();
        let dist = (f64::from(self.dest[0]) + 0.5 - p[0]).hypot(f64::from(self.dest[2]) + 0.5 - p[2]);
        if dist < speed * WATER_COAST_TICKS {
            let yaw = calc_rotation_from_vec3d(ctx.player_head(), block_pos_center(next), ctx.player_rotations()).yaw;
            if let Some(rotation) = state.target.rotation {
                state.set_target(MovementTarget::new(Rotation::new(yaw, rotation.pitch), false));
            }
        }
    }

    /// Movement.prepared: break what is in the way, one block at a time. False while it is not ready.
    fn prepared(&self, env: &Env, state: &mut MovementState) -> bool {
        match self.kind {
            Kind::Diagonal { .. } => return true,
            Kind::Pillar => {
                let ctx = env.ctx;
                let feet = ctx.player_feet();
                if feet == self.src || feet == above(self.src, -1) {
                    let below = block_at(env, above(self.src, -1));
                    if mh::is_climbable(&below) {
                        state.set_input(Input::Sneak, true);
                    }
                }
                if mh::is_water(&block_at(env, above(self.dest, 1))) {
                    return true;
                }
            }
            Kind::Traverse => {
                let ctx = env.ctx;
                let feet = ctx.player_feet();
                if feet == self.src || feet == above(self.src, -1) {
                    let below = block_at(env, above(self.src, -1));
                    if mh::is_climbable(&below) {
                        state.set_input(Input::Sneak, true);
                    }
                }
            }
            Kind::Fall => {
                if state.status == MovementStatus::Waiting {
                    return true;
                }
                // Only break if one of the first four needs it: the last one may be water.
                for p in self.positions_to_break.iter().take(4) {
                    if !walk_through(env, *p) {
                        return self.prepared_base(env, state);
                    }
                }
                return true;
            }
            _ => {}
        }
        self.prepared_base(env, state)
    }

    fn prepared_base(&self, env: &Env, state: &mut MovementState) -> bool {
        if state.status == MovementStatus::Waiting {
            return true;
        }
        let ctx = env.ctx;
        for &p in &self.positions_to_break {
            if env.settings.flag("pauseMiningForFallingBlocks") && ctx.falling_block_near(p) {
                return false;
            }
            if !walk_through(env, p) {
                // Cannot break air, so do not try.
                ctx.switch_to_best_tool_for(p);
                if let Some(rot_towards_block) = ctx.reachable(p, ctx.block_reach_distance(), false) {
                    state.set_target(MovementTarget::new(rot_towards_block, true));
                    if ctx.is_looking_at(p) || ctx.player_rotations().is_really_close_to(rot_towards_block) {
                        state.set_input(Input::ClickLeft, true);
                    }
                    return false;
                }
                // No face can be seen: break the block anyway, aiming at its center. This is a fallback, so the
                // selected block is not checked; it is intended to break the "incorrect" block.
                let rot = calc_rotation_from_vec3d(ctx.player_head(), block_pos_center(p), ctx.player_rotations());
                state.set_target(MovementTarget::new(rot, true));
                state.set_input(Input::ClickLeft, true);
                return false;
            }
        }
        true
    }

    /// Movement.safeToCancel(state).
    pub fn safe_to_cancel(&self, env: &Env) -> bool {
        let state = &self.state;
        let feet = env.ctx.player_feet();
        match self.kind {
            Kind::Traverse => state.status != MovementStatus::Running || walk_on(env, above(self.dest, -1)),
            Kind::Ascend => state.status != MovementStatus::Running || self.ticks_without_placement == 0,
            Kind::Fall => feet == self.src || state.status != MovementStatus::Running,
            Kind::Parkour { .. } => state.status != MovementStatus::Running,
            Kind::Diagonal { .. } => self.diagonal_safe_to_cancel(env),
            _ => true,
        }
    }

    fn diagonal_safe_to_cancel(&self, env: &Env) -> bool {
        let ctx = env.ctx;
        let offset = 0.25;
        let p = ctx.position();
        let (x, y, z) = (p[0], p[1] - 1.0, p[2]);
        let (src, dest) = (self.src, self.dest);
        // Standard.
        if ctx.player_feet() == src {
            return true;
        }
        // Both corners are walkable.
        if walk_on(env, [src[0], src[1] - 1, dest[2]]) && walk_on(env, [dest[0], src[1] - 1, src[2]]) {
            return true;
        }
        // We are in a likely unwalkable corner: check for a supporting block.
        if ctx.player_feet() == [src[0], src[1], dest[2]] || ctx.player_feet() == [dest[0], src[1], src[2]] {
            let at = |dx: f64, dz: f64| walk_on(env, [(x + dx).floor() as i32, y.floor() as i32, (z + dz).floor() as i32]);
            return at(offset, offset) || at(offset, -offset) || at(-offset, offset) || at(-offset, -offset);
        }
        true
    }

    /// Movement.toBreak.
    pub fn to_break(&self, env: &Env) -> Vec<Pos> {
        let range = if matches!(self.kind, Kind::Diagonal { .. }) { 4..6 } else { 0..self.positions_to_break.len() };
        self.positions_to_break[range].iter().copied().filter(|p| !walk_through(env, *p)).collect()
    }

    /// Movement.toPlace.
    pub fn to_place(&self, env: &Env) -> Vec<Pos> {
        match self.position_to_place {
            Some(p) if !walk_on(env, p) => vec![p],
            _ => Vec::new(),
        }
    }

    /// Movement.toWalkInto (only the diagonal has any).
    pub fn to_walk_into(&self, env: &Env) -> Vec<Pos> {
        if matches!(self.kind, Kind::Diagonal { .. }) {
            self.positions_to_break[..4].iter().copied().filter(|p| !walk_through(env, *p)).collect()
        } else {
            Vec::new()
        }
    }

    fn player_in_valid_position(&self, env: &Env, path_start: Pos) -> bool {
        let valid = self.valid_positions();
        valid.contains(&env.ctx.player_feet()) || valid.contains(&path_start)
    }

    /// Movement.updateState, then the subclass's.
    fn update_state(&mut self, env: &Env, mut state: MovementState) -> MovementState {
        // MovementAscend checks first, even while preparing (breaking blocks).
        if self.kind == Kind::Ascend && env.ctx.player_feet()[1] < self.src[1] {
            return state.set_status(MovementStatus::Unreachable).clone();
        }
        // Movement.updateState
        if !self.prepared(env, &mut state) {
            state.set_status(MovementStatus::Prepping);
            if self.kind == Kind::Traverse {
                self.traverse_prepping(env, &mut state);
            }
            return state;
        } else if state.status == MovementStatus::Prepping {
            state.set_status(MovementStatus::Waiting);
        }
        if state.status == MovementStatus::Waiting {
            state.set_status(MovementStatus::Running);
        }
        if self.kind == Kind::Traverse && state.status != MovementStatus::Running {
            // (the walk-while-breaking branch is in traverse_prepping)
            return state;
        }
        if state.status != MovementStatus::Running {
            return state;
        }
        match self.kind {
            Kind::Traverse => self.run_traverse(env, &mut state),
            Kind::Ascend => self.run_ascend(env, &mut state),
            Kind::Descend => self.run_descend(env, &mut state),
            Kind::Fall => self.run_fall(env, &mut state),
            Kind::Diagonal { .. } => self.run_diagonal(env, &mut state),
            Kind::Downward => self.run_downward(env, &mut state),
            Kind::Pillar => self.run_pillar(env, &mut state),
            Kind::Parkour { .. } => self.run_parkour(env, &mut state),
        }
        state
    }

    // ---- MovementTraverse

    /// The `walkWhileBreaking` branch of MovementTraverse.updateState, used while it is still preparing.
    fn traverse_prepping(&self, env: &Env, state: &mut MovementState) {
        if state.status != MovementStatus::Prepping {
            return;
        }
        if !env.settings.flag("walkWhileBreaking") {
            return;
        }
        let ctx = env.ctx;
        let pb0 = block_at(env, self.positions_to_break[0]);
        let pb1 = block_at(env, self.positions_to_break[1]);
        // And if it is fine to walk into the blocks in front.
        if mh::avoid_walking_into(env.calc, &pb0) || mh::avoid_walking_into(env.calc, &pb1) {
            return;
        }
        // And we are not already pressed up against the block.
        let p = ctx.position();
        let dist = (p[0] - (f64::from(self.dest[0]) + 0.5)).abs().max((p[2] - (f64::from(self.dest[2]) + 0.5)).abs());
        if dist < 0.83 {
            return;
        }
        let Some(target_rotation) = state.target.rotation else { return };
        // Combine the yaw to the center of the destination and the pitch to the block being broken. Safe: the
        // two blocks broken in a traverse are right on top of each other and have the same yaw.
        let yaw_to_dest = calc_rotation_from_vec3d(ctx.player_head(), block_pos_center(self.dest), ctx.player_rotations()).yaw;
        let mut pitch_to_break = target_rotation.pitch;
        if mh::is_block_normal_cube(&pb0) || pb0.is(Class::Air) && (mh::is_block_normal_cube(&pb1) || pb1.is(Class::Air)) {
            // Before we are right up against the block, we can break efficiently at this angle.
            pitch_to_break = 26.0;
        }
        state.set_target(MovementTarget::new(Rotation::new(yaw_to_dest, pitch_to_break), true)).set_input(Input::MoveForward, true).set_input(Input::Sprint, true);
    }

    fn run_traverse(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        let pb0 = block_at(env, self.positions_to_break[0]);
        let pb1 = block_at(env, self.positions_to_break[1]);
        let fd = block_at(env, above(src, -1));
        let ladder = mh::is_climbable(&fd);
        // Sneak may have been set while mining an adjacent block, but it is still wanted if about to go on magma.
        let on_magma = env.settings.flag("allowWalkOnMagmaBlocks") && stepping_on_blocks(env).iter().any(|b| block_at(env, *b).is_block("magma_block"));
        state.set_input(Input::Sneak, on_magma);
        if !open_doors(env, state, src, dest) {
            return;
        }
        if pb0.is(Class::FenceGate) || pb1.is(Class::FenceGate) {
            let blocked = if !is_gate_passable(env, self.positions_to_break[0], above(src, 1)) {
                Some(self.positions_to_break[0])
            } else if !is_gate_passable(env, self.positions_to_break[1], src) {
                Some(self.positions_to_break[1])
            } else {
                None
            };
            if let Some(b) = blocked {
                if let Some(rot) = ctx.reachable(b, ctx.block_reach_distance(), false) {
                    state.set_target(MovementTarget::new(rot, true)).set_input(Input::ClickRight, true);
                    return;
                }
            }
        }
        let place = self.position_to_place.unwrap_or(above(dest, -1));
        let is_the_bridge_block_there = walk_on(env, place) || ladder || mh::can_use_frost_walker(env.calc, &block_at(env, place));
        let feet = ctx.player_feet();
        if feet[1] != dest[1] && !ladder {
            ctx.log_debug("Wrong Y coordinate");
            if feet[1] < dest[1] {
                state.set_input(Input::Jump, true);
            }
            return;
        }
        if is_the_bridge_block_there {
            if feet == dest {
                state.set_status(MovementStatus::Success);
                return;
            }
            let dir = self.direction();
            if env.settings.flag("overshootTraverse") {
                let one = [dest[0] + dir.0, dest[1], dest[2] + dir.1];
                let two = [dest[0] + 2 * dir.0, dest[1], dest[2] + 2 * dir.1];
                if feet == one || feet == two {
                    state.set_status(MovementStatus::Success);
                    return;
                }
            }
            let low = block_at(env, src);
            let high = block_at(env, above(src, 1));
            if ctx.position()[1] > f64::from(src[1]) + 0.1 && !ctx.on_ground() && !ctx.is_in_water() && (mh::is_climbable(&low) || mh::is_climbable(&high)) {
                // Hitting W could climb the ladder instead of going forward: wait until on the ground.
                return;
            }
            let into = [dest[0] + (dest[0] - src[0]), dest[1] + (dest[1] - src[1]), dest[2] + (dest[2] - src[2])];
            let into_below = block_at(env, into);
            let into_above = block_at(env, above(into, 1));
            if self.bridge_block_always_there
                && (!mh::is_liquid(&block_at(env, feet)) || env.settings.flag("sprintInWater"))
                && (!mh::avoid_walking_into(env.calc, &into_below) || mh::is_water(&into_below))
                && !mh::avoid_walking_into(env.calc, &into_above)
            {
                state.set_input(Input::Sprint, true);
            }
            let dest_down = block_at(env, above(dest, -1));
            if feet[1] != dest[1] && ladder && mh::is_climbable(&dest_down) {
                state.set_input(Input::Jump, true);
            }
            move_towards(env, state, self.positions_to_break[0]);
        } else {
            self.bridge_block_always_there = false;
            let standing_on = block_at(env, above(feet, -1));
            if standing_on.is_block("soul_sand") || standing_on.is(Class::Slab) {
                // See Baritone issue 118.
                let p = ctx.position();
                let dist = (f64::from(dest[0]) + 0.5 - p[0]).abs().max((f64::from(dest[2]) + 0.5 - p[2]).abs());
                if dist < 0.85 {
                    // 0.5 + 0.3 + epsilon
                    move_towards(env, state, dest);
                    state.set_input(Input::MoveForward, false).set_input(Input::MoveBack, true);
                    return;
                }
            }
            let p = ctx.position();
            let dist1 = (p[0] - (f64::from(dest[0]) + 0.5)).abs().max((p[2] - (f64::from(dest[2]) + 0.5)).abs());
            let assume_safe_walk = env.settings.flag("assumeSafeWalk");
            let placed = attempt_to_place_a_block(env, state, above(dest, -1), false, !assume_safe_walk);
            if (placed == PlaceResult::ReadyToPlace || dist1 < 0.6) && !assume_safe_walk {
                state.set_input(Input::Sneak, true);
            }
            match placed {
                PlaceResult::ReadyToPlace => {
                    if ctx.is_crouching() || assume_safe_walk {
                        state.set_input(Input::ClickRight, true);
                    }
                    return;
                }
                PlaceResult::Attempting => {
                    if dist1 > 0.83 {
                        // Might need to go forward a bit.
                        let yaw = calc_rotation_from_vec3d(ctx.player_head(), block_pos_center(dest), ctx.player_rotations()).yaw;
                        if let Some(t) = state.target.rotation {
                            if (t.yaw - yaw).abs() < 0.1 {
                                // But only if the attempted place is straight ahead.
                                state.set_input(Input::MoveForward, true);
                                return;
                            }
                        }
                    } else if let Some(t) = state.target.rotation {
                        if ctx.player_rotations().is_really_close_to(t) {
                            // Something is in the way, it seems.
                            state.set_input(Input::ClickLeft, true);
                        }
                    }
                    return;
                }
                PlaceResult::NoOption => {}
            }
            if feet == dest {
                // In the block we are trying to get to: we are sneaking over air and have to place a block
                // beneath us against the one we just walked off of.
                let face_x = (f64::from(dest[0]) + f64::from(src[0]) + 1.0) * 0.5;
                let face_y = (f64::from(dest[1]) + f64::from(src[1]) - 1.0) * 0.5;
                let face_z = (f64::from(dest[2]) + f64::from(src[2]) + 1.0) * 0.5;
                // faceX, faceY, faceZ is the middle of the face between from and to.
                let goal_look = above(src, -1); // the block just stood on, and the one to place against
                let back_to_face = calc_rotation_from_vec3d(ctx.player_head(), [face_x, face_y, face_z], ctx.player_rotations());
                let pitch = back_to_face.pitch;
                let dist2 = (p[0] - face_x).abs().max((p[2] - face_z).abs());
                if dist2 < 0.29 {
                    // See Baritone issue 208.
                    let yaw = calc_rotation_from_vec3d(block_pos_center(dest), ctx.player_head(), ctx.player_rotations()).yaw;
                    state.set_target(MovementTarget::new(Rotation::new(yaw, pitch), true));
                    state.set_input(Input::MoveBack, true);
                } else {
                    state.set_target(MovementTarget::new(back_to_face, true));
                }
                if ctx.is_looking_at(goal_look) {
                    state.set_input(Input::ClickRight, true); // wait to right click until we can place
                    return;
                }
                if let Some(t) = state.target.rotation {
                    if ctx.player_rotations().is_really_close_to(t) {
                        state.set_input(Input::ClickLeft, true);
                    }
                }
                return;
            }
            move_towards_with_slight_rotation(env, state, dest);
        }
    }

    // ---- MovementAscend

    fn run_ascend(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        let feet = ctx.player_feet();
        let dir = self.direction();
        if feet == dest || feet == [dest[0] + dir.0, dest[1], dest[2] + dir.1] {
            state.set_status(MovementStatus::Success);
            return;
        }
        if !open_doors(env, state, above(src, 1), dest) {
            return;
        }
        let place = above(dest, -1);
        let jumping_onto = block_at(env, place);
        if !mh::can_walk_on(env.calc, place[0], place[1], place[2], &jumping_onto) {
            self.ticks_without_placement += 1;
            if attempt_to_place_a_block(env, state, above(dest, -1), false, true) == PlaceResult::ReadyToPlace {
                state.set_input(Input::Sneak, true);
                if ctx.is_crouching() {
                    state.set_input(Input::ClickRight, true);
                }
            }
            if self.ticks_without_placement > 10 {
                // After 10 ticks without placement we might be standing in the way: move back.
                state.set_input(Input::MoveBack, true);
            }
            return;
        }
        move_towards(env, state, dest);
        state.set_input(Input::Sneak, env.settings.flag("allowWalkOnMagmaBlocks") && jumping_onto.is_block("magma_block"));
        if mh::is_bottom_slab(&jumping_onto) && !mh::is_bottom_slab(&block_at(env, above(src, -1))) {
            return; // do not jump while walking from a non double slab into a bottom slab
        }
        if env.settings.flag("assumeStep") || feet == above(src, 1) {
            return; // no need to hit space if already jumping
        }
        let x_axis = f64::from((src[0] - dest[0]).abs());
        let z_axis = f64::from((src[2] - dest[2]).abs());
        let p = ctx.position();
        let dm = ctx.delta_movement();
        let flat_dist_to_next = x_axis * ((f64::from(dest[0]) + 0.5) - p[0]).abs() + z_axis * ((f64::from(dest[2]) + 0.5) - p[2]).abs();
        let side_dist = z_axis * ((f64::from(dest[0]) + 0.5) - p[0]).abs() + x_axis * ((f64::from(dest[2]) + 0.5) - p[2]).abs();
        let lateral_motion = x_axis * dm[2] + z_axis * dm[0];
        if lateral_motion.abs() > 0.1 {
            return;
        }
        if self.head_bonk_clear(env) {
            state.set_input(Input::Jump, true);
            return;
        }
        if flat_dist_to_next > 1.2 || side_dist > 0.2 {
            return;
        }
        // Once pointing the right way and moving, start jumping. Slightly more efficient: otherwise jumping
        // might start before moving, and fall back without moving onto the block. Also wait until close,
        // because we might jump and hit our head on an adjacent block.
        state.set_input(Input::Jump, true);
    }

    fn head_bonk_clear(&self, env: &Env) -> bool {
        let start_up = above(self.src, 2);
        for (dx, dz) in [(0, 1), (-1, 0), (0, -1), (1, 0)] {
            if !walk_through(env, [start_up[0] + dx, start_up[1], start_up[2] + dz]) {
                return false; // we might bonk our head
            }
        }
        true
    }

    // ---- MovementDescend

    fn into(&self) -> Pos {
        // dest - src.below() + dest
        let (src, dest) = (self.src, self.dest);
        [dest[0] - src[0] + dest[0], dest[1] - (src[1] - 1) + dest[1], dest[2] - src[2] + dest[2]]
    }

    pub fn skip_to_ascend(&self, env: &Env) -> bool {
        let into = self.into();
        !walk_through(env, into) && walk_through(env, above(into, 1)) && walk_through(env, above(into, 2))
    }

    pub fn safe_mode(&self, env: &Env) -> bool {
        if self.force_safe_mode {
            return true;
        }
        // (dest - src) + dest is offset one more in the same direction: the block we would run into if we
        // sprinted straight through this descend.
        let into = self.into();
        if self.skip_to_ascend(env) {
            // If dest extends into something that cannot be walked through but the two above can, we can
            // overshoot and glitch in that weird way.
            return true;
        }
        for y in 0..=2 {
            // We could hit any of the three blocks.
            if mh::avoid_walking_into(env.calc, &block_at(env, above(into, y))) {
                return true;
            }
        }
        false
    }

    fn run_descend(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        let player_feet = ctx.player_feet();
        let fake_dest = [dest[0] * 2 - src[0], dest[1], dest[2] * 2 - src[2]];
        if (player_feet == dest || player_feet == fake_dest) && (mh::is_liquid(&block_at(env, dest)) || ctx.position()[1] - f64::from(dest[1]) < 0.5) {
            // Wait until we are really on the ground before saying we are done: sometimes we continue to fall
            // if the next action starts immediately (lilypads).
            state.set_status(MovementStatus::Success);
            return;
        }
        if !open_doors(env, state, src, above(dest, 1)) {
            return;
        }
        if self.safe_mode(env) {
            let dx = (f64::from(src[0]) + 0.5) * 0.17 + (f64::from(dest[0]) + 0.5) * 0.83;
            let dz = (f64::from(src[2]) + 0.5) * 0.17 + (f64::from(dest[2]) + 0.5) * 0.83;
            let rot = calc_rotation_from_vec3d(ctx.player_head(), [dx, f64::from(dest[1]), dz], ctx.player_rotations()).with_pitch(ctx.player_rotations().pitch);
            state.set_target(MovementTarget::new(rot, false)).set_input(Input::MoveForward, true);
            return;
        }
        let p = ctx.position();
        let (diff_x, diff_z) = (p[0] - (f64::from(dest[0]) + 0.5), p[2] - (f64::from(dest[2]) + 0.5));
        let ab = diff_x.hypot(diff_z);
        let (x, z) = (p[0] - (f64::from(src[0]) + 0.5), p[2] - (f64::from(src[2]) + 0.5));
        let from_start = x.hypot(z);
        state.set_input(Input::Sneak, env.settings.flag("allowWalkOnMagmaBlocks") && block_at(env, above(player_feet, -1)).is_block("magma_block"));
        if player_feet != dest || ab > 0.25 {
            self.num_ticks += 1;
            if self.num_ticks - 1 < 20 && from_start < 1.25 {
                move_towards(env, state, fake_dest);
            } else {
                move_towards(env, state, dest);
            }
        }
    }

    // ---- MovementFall

    /// Whether this fall plants a water bucket (MovementFall.willPlaceBucket).
    fn will_place_bucket(&self, env: &Env) -> bool {
        let mut res = super::baritone_calculation_context::MutableMoveResult::default();
        let below = env.calc.get(self.dest[0], self.src[1] - 2, self.dest[2]);
        super::baritone_movement_descend::dynamic_fall_cost(env.calc, self.src[0], self.src[1], self.src[2], self.dest[0], self.dest[2], 0.0, &below, &mut res)
    }

    fn run_fall(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        let player_feet = ctx.player_feet();
        let to_dest = calc_rotation_from_vec3d(ctx.player_head(), block_pos_center(dest), ctx.player_rotations());
        let mut target_rotation = None;
        let dest_state = block_at(env, dest);
        if block_at(env, above(dest, -1)).is_block("magma_block") && stepping_on_blocks(env).iter().all(|b| walk_through(env, *b)) {
            state.set_input(Input::Sneak, true);
        }
        let is_water = matches!(dest_state.fluid, super::baritone_block_state::FluidKind::Water | super::baritone_block_state::FluidKind::FlowingWater);
        if !is_water && self.will_place_bucket(env) && player_feet != dest {
            if ctx.water_bucket_hotbar_slot().is_none() || ctx.is_nether() {
                state.set_status(MovementStatus::Unreachable);
                return;
            }
            if ctx.position()[1] - f64::from(dest[1]) < ctx.block_reach_distance() && !ctx.on_ground() {
                if let Some(slot) = ctx.water_bucket_hotbar_slot() {
                    ctx.select_hotbar_slot(slot);
                }
                target_rotation = Some(Rotation::new(to_dest.yaw, 90.0));
                if ctx.is_looking_at(dest) || ctx.is_looking_at(above(dest, -1)) {
                    state.set_input(Input::ClickRight, true);
                }
            }
        }
        if !open_doors(env, state, src, [dest[0], src[1], dest[2]]) {
            return;
        }
        match target_rotation {
            Some(r) => {
                state.set_target(MovementTarget::new(r, true));
            }
            None => {
                state.set_target(MovementTarget::new(to_dest, false));
            }
        }
        if player_feet == dest && (ctx.position()[1] - f64::from(player_feet[1]) < 0.094 || is_water) {
            // 0.094 because of lilypads.
            if is_water {
                // Only still water, not flowing water (which a bucket cannot pick up).
                if let Some(slot) = ctx.empty_bucket_hotbar_slot() {
                    ctx.select_hotbar_slot(slot);
                    if ctx.delta_movement()[1] >= 0.0 {
                        state.set_input(Input::ClickRight, true);
                    }
                    return;
                } else if ctx.delta_movement()[1] >= 0.0 {
                    state.set_status(MovementStatus::Success);
                    return;
                } // do not return otherwise: stay centered, because this water might be flowing under the surface
            } else {
                state.set_status(MovementStatus::Success);
                return;
            }
        }
        let dest_center = block_pos_center(dest); // moving to the 0.5 center, not the edge (as when falling on a ladder)
        let (p, dm) = (ctx.position(), ctx.delta_movement());
        if (p[0] + dm[0] - dest_center[0]).abs() > 0.1 || (p[2] + dm[2] - dest_center[2]).abs() > 0.1 {
            if !ctx.on_ground() && dm[1].abs() > 0.4 {
                state.set_input(Input::Sneak, true);
            }
            state.set_input(Input::MoveForward, true);
        }
        let mut avoid = self.fall_avoid(env);
        if avoid.is_none() {
            avoid = Some((src[0] - dest[0], src[2] - dest[2]));
        } else if let Some(a) = avoid {
            let dist = f64::from(a.0).abs() * (dest_center[0] - f64::from(a.0) / 2.0 - p[0]).abs() + f64::from(a.1).abs() * (dest_center[2] - f64::from(a.1) / 2.0 - p[2]).abs();
            if dist < 0.6 {
                state.set_input(Input::MoveForward, true);
            } else if !ctx.on_ground() {
                state.set_input(Input::Sneak, false);
            }
        }
        if target_rotation.is_none() {
            let a = avoid.unwrap_or((0, 0));
            let dest_center_offset = [dest_center[0] + 0.125 * f64::from(a.0), dest_center[1], dest_center[2] + 0.125 * f64::from(a.1)];
            state.set_target(MovementTarget::new(calc_rotation_from_vec3d(ctx.player_head(), dest_center_offset, ctx.player_rotations()), false));
        }
    }

    /// MovementFall.avoid: the way a ladder below faces, as (dx, dz).
    fn fall_avoid(&self, env: &Env) -> Option<(i32, i32)> {
        let feet = env.ctx.player_feet();
        for i in 0..15 {
            let state = block_at(env, above(feet, -i));
            if state.is_block("ladder") {
                return Some(diagonal::facing_step(state.prop("facing").unwrap_or("north")));
            }
        }
        None
    }

    // ---- MovementDiagonal

    fn diagonal_sprint(&self, env: &Env) -> bool {
        if mh::is_liquid(&block_at(env, env.ctx.player_feet())) && !env.settings.flag("sprintInWater") {
            return false;
        }
        for i in 0..4 {
            if !walk_through(env, self.positions_to_break[i]) {
                return false;
            }
        }
        true
    }

    fn run_diagonal(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        let feet = ctx.player_feet();
        if feet == dest {
            state.set_status(MovementStatus::Success);
            return;
        } else if !self.player_in_valid_position(env, src)
            && !(mh::is_liquid(&block_at(env, src)) && self.valid_positions().contains(&above(feet, 1)))
        {
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        if !diagonal::is_blocking_door(&block_at(env, [src[0], src[1], dest[2]]), src[0], dest[2], dest[0], src[2])
            && (!open_doors(env, state, src, [src[0], src[1], dest[2]]) || !open_doors(env, state, [src[0], dest[1], dest[2]], dest))
        {
            return;
        }
        if !diagonal::is_blocking_door(&block_at(env, [dest[0], src[1], src[2]]), dest[0], src[2], src[0], dest[2])
            && (!open_doors(env, state, src, [dest[0], src[1], src[2]]) || !open_doors(env, state, [dest[0], dest[1], src[2]], dest))
        {
            return;
        }
        if dest[1] > src[1] && ctx.position()[1] < f64::from(src[1]) + 0.1 && ctx.horizontal_collision() {
            state.set_input(Input::Jump, true);
        }
        if self.diagonal_sprint(env) {
            state.set_input(Input::Sprint, true);
        }
        let on_magma = env.settings.flag("allowWalkOnMagmaBlocks") && stepping_on_blocks(env).iter().any(|b| block_at(env, *b).is_block("magma_block"));
        state.set_input(Input::Sneak, on_magma);
        move_towards(env, state, dest);
    }

    // ---- MovementDownward

    fn run_downward(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        if ctx.player_feet() == dest {
            state.set_status(MovementStatus::Success);
            return;
        } else if !self.player_in_valid_position(env, src) {
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        let p = ctx.position();
        let (diff_x, diff_z) = (p[0] - (f64::from(dest[0]) + 0.5), p[2] - (f64::from(dest[2]) + 0.5));
        let ab = (diff_x * diff_x + diff_z * diff_z).sqrt();
        self.num_ticks += 1;
        if self.num_ticks - 1 < 10 && ab < 0.2 {
            return;
        }
        move_towards(env, state, self.positions_to_break[0]);
    }

    // ---- MovementPillar

    fn run_pillar(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        if ctx.player_feet()[1] < src[1] {
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        let from_down = block_at(env, src);
        if mh::is_water(&from_down) && mh::is_water(&block_at(env, dest)) {
            let rot = calc_rotation_from_vec3d(ctx.player_head(), block_pos_center(dest), ctx.player_rotations());
            state.set_target(MovementTarget::new(rot, false));
            let dest_center = block_pos_center(dest);
            let p = ctx.position();
            if (p[0] - dest_center[0]).abs() > 0.2 || (p[2] - dest_center[2]).abs() > 0.2 {
                state.set_input(Input::MoveForward, true);
            }
            if ctx.player_feet() == dest {
                state.set_status(MovementStatus::Success);
            }
            return;
        }
        let ladder = mh::is_climbable(&from_down);
        let place = self.position_to_place.unwrap_or(src);
        let rotation = calc_rotation_from_vec3d(ctx.player_head(), block_pos_center(place), ctx.player_rotations());
        if !ladder {
            state.set_target(MovementTarget::new(ctx.player_rotations().with_pitch(rotation.pitch), true));
        }
        let mut block_is_there = walk_on(env, src) || ladder;
        if ladder {
            if ctx.player_feet() == dest {
                state.set_status(MovementStatus::Success);
                return;
            }
            move_towards(env, state, dest);
            state.set_input(Input::Jump, true);
            return;
        }
        if !ctx.select_throwaway_for_location(true, src[0], src[1], src[2]) {
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        state.set_input(Input::Sneak, true);
        let p = ctx.position();
        let dm = ctx.delta_movement();
        let (diff_x, diff_z) = (p[0] - (f64::from(dest[0]) + 0.5), p[2] - (f64::from(dest[2]) + 0.5));
        let dist = (diff_x * diff_x + diff_z * diff_z).sqrt();
        let flat_motion = (dm[0] * dm[0] + dm[2] * dm[2]).sqrt();
        if dist > 0.17 {
            // Why 0.17? Because it seemed like a good number.
            state.set_input(Input::MoveForward, true);
            state.set_target(MovementTarget::new(rotation, true));
        } else if flat_motion < 0.05 {
            state.set_input(Input::Jump, p[1] < f64::from(dest[1]));
        }
        if !block_is_there {
            let fr_state = block_at(env, src);
            if !(fr_state.is(Class::Air) || fr_state.replaceable) {
                if let Some(rot) = ctx.reachable(src, ctx.block_reach_distance(), false) {
                    state.set_target(MovementTarget::new(rot, true));
                }
                state.set_input(Input::Jump, false); // breaking is about 5x slower when jumping
                state.set_input(Input::ClickLeft, true);
                block_is_there = false;
            } else if ctx.is_crouching() && (ctx.is_looking_at(above(src, -1)) || ctx.is_looking_at(src)) && p[1] > f64::from(dest[1]) + 0.1 {
                state.set_input(Input::ClickRight, true);
            }
        }
        if ctx.player_feet() == dest && block_is_there {
            state.set_status(MovementStatus::Success);
        }
    }

    // ---- MovementParkour

    fn run_parkour(&mut self, env: &Env, state: &mut MovementState) {
        let ctx = env.ctx;
        let (src, dest) = (self.src, self.dest);
        let Kind::Parkour { dir, dist, ascend } = self.kind else { return };
        if ctx.player_feet()[1] < src[1] {
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        let rel = |pos: Pos, n: i32| [pos[0] + dir.0 * n, pos[1], pos[2] + dir.1 * n];
        if !open_doors(env, state, src, rel(src, 1)) {
            return;
        }
        if dist >= 4 || ascend {
            state.set_input(Input::Sprint, true);
        }
        if env.settings.flag("allowWalkOnMagmaBlocks") && block_at(env, above(ctx.player_feet(), -1)).is_block("magma_block") {
            state.set_input(Input::Sneak, true);
        }
        move_towards(env, state, dest);
        let feet = ctx.player_feet();
        if feet == dest {
            let d = block_at(env, dest);
            if d.is_block("vine") || d.is_block("ladder") {
                state.set_status(MovementStatus::Success);
                return;
            }
            if ctx.position()[1] - f64::from(feet[1]) < 0.094 {
                // Lilypads.
                state.set_status(MovementStatus::Success);
            }
        } else if feet != src {
            if feet == rel(src, 1) || ctx.position()[1] - f64::from(src[1]) > 0.0001 {
                if env.settings.flag("allowPlace") // see Baritone PR 3775
                    && ctx.has_generic_throwaway()
                    && !walk_on(env, above(dest, -1))
                    && !ctx.on_ground()
                    && attempt_to_place_a_block(env, state, above(dest, -1), true, false) == PlaceResult::ReadyToPlace
                {
                    state.set_input(Input::ClickRight, true);
                }
                if dist == 3 && !ascend {
                    // This is a 2 block gap, dest = src + direction * 3.
                    let p = ctx.position();
                    let (x_diff, z_diff) = ((f64::from(src[0]) + 0.5) - p[0], (f64::from(src[2]) + 0.5) - p[2]);
                    let dist_from_start = x_diff.abs().max(z_diff.abs());
                    if dist_from_start < 0.7 {
                        return;
                    }
                }
                state.set_input(Input::Jump, true);
            } else if feet != rel(dest, -1) {
                state.set_input(Input::Sprint, false);
                if feet == rel(src, -1) {
                    move_towards(env, state, src);
                } else {
                    move_towards(env, state, rel(src, -1));
                }
            }
        }
    }

    /// The cost of this movement right now (`recalculateCost`), for the executor.
    #[allow(dead_code)]
    pub fn current_cost(&mut self, calc: &CalculationContext) -> f64 {
        self.recalculate_cost(calc)
    }

    #[allow(dead_code)]
    pub fn is_impossible(cost: f64) -> bool {
        cost >= COST_INF
    }
}
