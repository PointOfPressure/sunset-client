// baritone.process.elytra.ElytraBehavior's solver: where to aim for the next tick of flight. For each path node near the
// player, from the furthest back, it asks whether the player's hitbox can fly there (rays through the chunk octrees),
// then simulates the elytra's physics for a range of pitches and keeps the pitch whose flight points best at the node
// and never meets a block. The world, the path and the player are inputs, so all of it runs and is tested without a
// game. Everything the Java does around it (the locks, the threads, the firework entity, the inventory) is the glue's.
use super::baritone_aim::{AimProcessor, AimSettings};
use super::baritone_rotation::{calc_look_direction_from_rotation, calc_rotation_from_vec3d, mth_cos, mth_sin, Rotation};
use std::ops::{Add, Sub};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl V3 {
    pub const ZERO: V3 = V3 { x: 0.0, y: 0.0, z: 0.0 };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn from_array(a: [f64; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }

    pub fn from_block(p: [i32; 3]) -> Self {
        Self::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
    }

    pub fn to_array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    pub fn scale(self, f: f64) -> Self {
        Self::new(self.x * f, self.y * f, self.z * f)
    }

    pub fn dot(self, o: V3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn length_sqr(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn length(self) -> f64 {
        self.length_sqr().sqrt()
    }

    /// `Vec3.normalize`: the zero vector for anything shorter than 1e-5.
    pub fn normalize(self) -> Self {
        let d = self.length();
        if d < 1.0e-5 { V3::ZERO } else { Self::new(self.x / d, self.y / d, self.z / d) }
    }

    pub fn distance_to(self, o: V3) -> f64 {
        (self - o).length()
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

/// `AABB`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    /// `expandTowards`: grows the box along the vector, in the direction the vector points.
    pub fn expand_towards(self, v: V3) -> Aabb {
        let mut b = self;
        if v.x < 0.0 {
            b.min_x += v.x;
        } else if v.x > 0.0 {
            b.max_x += v.x;
        }
        if v.y < 0.0 {
            b.min_y += v.y;
        } else if v.y > 0.0 {
            b.max_y += v.y;
        }
        if v.z < 0.0 {
            b.min_z += v.z;
        } else if v.z > 0.0 {
            b.max_z += v.z;
        }
        b
    }

    pub fn inflate(self, v: f64) -> Aabb {
        Aabb { min_x: self.min_x - v, min_y: self.min_y - v, min_z: self.min_z - v, max_x: self.max_x + v, max_y: self.max_y + v, max_z: self.max_z + v }
    }

    /// `move`.
    pub fn translate(self, v: V3) -> Aabb {
        Aabb { min_x: self.min_x + v.x, min_y: self.min_y + v.y, min_z: self.min_z + v.z, max_x: self.max_x + v.x, max_y: self.max_y + v.y, max_z: self.max_z + v.z }
    }
}

/// `BaritoneMath.fastFloor`.
pub fn fast_floor(d: f64) -> i32 {
    let i = d as i32;
    if d < f64::from(i) { i - 1 } else { i }
}

/// `BaritoneMath.fastCeil`.
pub fn fast_ceil(d: f64) -> i32 {
    let i = d as i32;
    if d > f64::from(i) { i + 1 } else { i }
}

/// What the solver asks of the world. A position the pathfinder has no chunk for counts as solid.
pub trait FlightWorld {
    /// `ElytraBehavior.passable(x, y, z, ignoreLava)`.
    fn passable(&self, x: i32, y: i32, z: i32, ignore_lava: bool) -> bool;
    /// `ElytraBehavior.clearView`: whether a ray from `start` reaches `dest`.
    fn clear_view(&self, start: V3, dest: V3, ignore_lava: bool) -> bool;
    /// `ElytraBehavior.raytrace(8, src, dst, Visibility.ALL)`: whether every ray of the batch reaches its end. Both arrays
    /// hold x, y, z of eight points.
    fn all_clear(&self, src: &[f64; 24], dst: &[f64; 24]) -> bool;
}

pub struct FlightSettings {
    pub minimum_avoidance: f64,
    pub pitch_range: i32,
    pub simulation_ticks: i32,
    pub render_hitbox_raytraces: bool,
}

/// The `ITickableAimProcessor` fork the simulation aims with: the aim processor and the rotation it last returned.
#[derive(Clone)]
pub struct TickableAim {
    processor: AimProcessor,
    prev: Rotation,
    settings: AimSettings,
}

impl TickableAim {
    pub fn new(processor: AimProcessor, prev: Rotation, settings: AimSettings) -> Self {
        Self { processor, prev, settings }
    }

    #[allow(dead_code)]
    pub fn advance(&mut self, ticks: i32) {
        self.processor.advance(ticks, &self.settings);
    }

    pub fn next_rotation(&mut self, rotation: Rotation) -> Rotation {
        let actual = self.processor.next_rotation(rotation, self.prev, &self.settings);
        self.prev = actual;
        actual
    }

    pub fn fork(&self) -> Self {
        self.clone()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FireworkBoost {
    /// The ticks the attached firework entity has existed, if there is one.
    firework_ticks_existed: Option<i32>,
    minimum_boost_ticks: i32,
    maximum_boost_ticks: i32,
}

impl FireworkBoost {
    pub fn new(firework_ticks_existed: Option<i32>, minimum_boost_ticks: i32) -> Self {
        // this.lifetime = 10 * i + this.rand.nextInt(6) + this.rand.nextInt(7);
        Self { firework_ticks_existed, minimum_boost_ticks, maximum_boost_ticks: minimum_boost_ticks + 11 }
    }

    pub fn is_boosted(&self) -> bool {
        self.firework_ticks_existed.is_some()
    }

    /// The guaranteed number of remaining ticks with boost.
    pub fn guaranteed_boost_ticks(&self) -> i32 {
        self.firework_ticks_existed.map_or(0, |t| (self.minimum_boost_ticks - t).max(0))
    }

    /// The maximum number of remaining ticks with boost.
    pub fn maximum_boost_ticks(&self) -> i32 {
        self.firework_ticks_existed.map_or(0, |t| (self.maximum_boost_ticks - t).max(0))
    }
}

impl PartialEq for FireworkBoost {
    fn eq(&self, other: &Self) -> bool {
        if !self.is_boosted() && !other.is_boosted() {
            return true;
        }
        self.firework_ticks_existed == other.firework_ticks_existed && self.minimum_boost_ticks == other.minimum_boost_ticks && self.maximum_boost_ticks == other.maximum_boost_ticks
    }
}

/// `SolverContext`: the path, player and firework boost as they were when it was made.
#[derive(Clone)]
pub struct SolverContext {
    pub path: Arc<Vec<[i32; 3]>>,
    pub player_near: usize,
    pub start: V3,
    pub motion: V3,
    pub bounding_box: Aabb,
    pub ignore_lava: bool,
    pub boost: FireworkBoost,
    pub aim: TickableAim,
    /// `ctx.playerRotations()`, which the yaw of a new aim is wrapped around.
    pub player_rotations: Rotation,
}

impl PartialEq for SolverContext {
    fn eq(&self, other: &Self) -> bool {
        // The contents of the path aren't modified, so they are compared by reference.
        Arc::ptr_eq(&self.path, &other.path)
            && self.player_near == other.player_near
            && self.start == other.start
            && self.motion == other.motion
            && self.bounding_box == other.bounding_box
            && self.ignore_lava == other.ignore_lava
            && self.boost == other.boost
    }
}

impl SolverContext {
    fn path_vec(&self, i: usize) -> V3 {
        V3::from_block(self.path[i])
    }
}

pub struct Solution {
    pub context: SolverContext,
    pub rotation: Rotation,
    pub going_to: Option<V3>,
    pub solved_pitch: bool,
    pub force_use_firework: bool,
    /// The flight that was simulated for the pitch chosen, as offsets from the start.
    pub simulation_line: Option<Vec<V3>>,
}

struct PitchResult {
    pitch: f32,
    dot: f64,
    steps: Vec<V3>,
}

const DEG_TO_RAD_F: f32 = (std::f64::consts::PI / 180.0) as f32;

/// `step`: one tick of elytra flight (`LivingEntity.travel` while fall flying) on a motion vector.
pub fn step(motion: V3, look: V3, pitch: f32) -> V3 {
    let (mut motion_x, mut motion_y, mut motion_z) = (motion.x, motion.y, motion.z);

    let pitch_radians = pitch * DEG_TO_RAD_F;
    let pitch_base2 = (look.x * look.x + look.z * look.z).sqrt();
    let flat_motion = (motion_x * motion_x + motion_z * motion_z).sqrt();
    let this_is_always_one = look.length();
    let mut pitch_base3 = mth_cos(pitch_radians);
    pitch_base3 = (f64::from(pitch_base3) * f64::from(pitch_base3) * f64::min(1.0, this_is_always_one / 0.4)) as f32;
    motion_y += -0.08 + f64::from(pitch_base3) * 0.06;
    if motion_y < 0.0 && pitch_base2 > 0.0 {
        let speed_modifier = motion_y * -0.1 * f64::from(pitch_base3);
        motion_y += speed_modifier;
        motion_x += look.x * speed_modifier / pitch_base2;
        motion_z += look.z * speed_modifier / pitch_base2;
    }
    if pitch_radians < 0.0 {
        // If you are looking up (above level).
        let another_speed_modifier = flat_motion * f64::from(-mth_sin(pitch_radians)) * 0.04;
        motion_y += another_speed_modifier * 3.2;
        motion_x -= look.x * another_speed_modifier / pitch_base2;
        motion_z -= look.z * another_speed_modifier / pitch_base2;
    }
    if pitch_base2 > 0.0 {
        // This is always true unless you are looking literally straight up.
        motion_x += (look.x / pitch_base2 * flat_motion - motion_x) * 0.1;
        motion_z += (look.z / pitch_base2 * flat_motion - motion_z) * 0.1;
    }
    motion_x *= f64::from(0.99f32);
    motion_y *= f64::from(0.98f32);
    motion_z *= f64::from(0.99f32);

    V3::new(motion_x, motion_y, motion_z)
}

/// `pitchesToSolveFor`: the good pitch first, then one step further up, one step further down, and so on.
fn pitches_to_solve_for(good_pitch: f32, desperate: bool, pitch_range: i32) -> Vec<f32> {
    let range = pitch_range as f32;
    let min_pitch = if desperate { -90.0 } else { f32::max(good_pitch - range, -89.0) };
    let max_pitch = if desperate { 90.0 } else { f32::min(good_pitch + range, 89.0) };

    let mut pitch_values = Vec::with_capacity((fast_ceil(f64::from(max_pitch - min_pitch)) + 1).max(0) as usize);
    let mut pitch = good_pitch;
    while pitch <= max_pitch {
        pitch_values.push(pitch);
        pitch += 1.0;
    }
    let mut pitch = good_pitch - 1.0;
    while pitch >= min_pitch {
        pitch_values.push(pitch);
        pitch -= 1.0;
    }
    pitch_values
}

pub struct Solver<'a> {
    pub world: &'a dyn FlightWorld,
    pub settings: &'a FlightSettings,
    pub landing_mode: bool,
    /// `Thread.interrupted()`: the game thread gave up on this solve.
    pub cancelled: &'a dyn Fn() -> bool,
}

impl Solver<'_> {
    /// `solveAngles`.
    pub fn solve_angles(&self, context: &SolverContext) -> Option<Solution> {
        let path = &context.path;
        let player_near = if self.landing_mode { path.len() - 1 } else { context.player_near };
        let start = context.start;
        let mut solution: Option<Solution> = None;

        // Try for a strict solution first, then relax more and more (in a corner or near some blocks it has to relax its
        // constraints a bit).
        for relaxation in 0..3 {
            // Attempt to gain height, if we can, so as not to waste the boost.
            let heights: &[i32] = if context.boost.is_boosted() { &[20, 10, 5, 0] } else { &[0] };
            let lookahead = if relaxation == 0 { 2 } else { 3 }; // ideally a distance in blocks, rather than voxel steps
            let min_step = player_near;

            let mut i = (player_near + 20).min(path.len() - 1);
            loop {
                if i < min_step {
                    break;
                }
                if (self.cancelled)() {
                    return None;
                }
                let mut candidates: Vec<(V3, i32)> = Vec::new();
                for &dy in heights {
                    if relaxation == 0 || i == min_step {
                        // No interp.
                        candidates.push((context.path_vec(i), dy));
                    } else if relaxation == 1 {
                        for interp in [1.0, 0.75, 0.5, 0.25] {
                            let dest = if interp == 1.0 {
                                context.path_vec(i)
                            } else {
                                context.path_vec(i).scale(interp) + context.path_vec(i - 1).scale(1.0 - interp)
                            };
                            candidates.push((dest, dy));
                        }
                    } else {
                        // Create a point along the segment every block.
                        let delta = context.path_vec(i) - context.path_vec(i - 1);
                        let steps = fast_floor(delta.length());
                        let step = delta.normalize();
                        let mut stepped = context.path_vec(i);
                        for _ in 0..steps {
                            candidates.push((stepped, dy));
                            stepped = stepped - step;
                        }
                    }
                }

                for (point, augment) in candidates {
                    let mut dest = point + V3::new(0.0, f64::from(augment), 0.0);
                    if self.landing_mode {
                        dest = dest + V3::new(0.5, 0.5, 0.5);
                    }

                    if augment != 0 {
                        if i + lookahead >= path.len() {
                            continue;
                        }
                        let up = V3::new(0.0, f64::from(augment), 0.0);
                        if start.distance_to(dest) < 40.0 {
                            if !self.world.clear_view(dest, context.path_vec(i + lookahead) + up, false) || !self.world.clear_view(dest, context.path_vec(i + lookahead), false) {
                                // Don't go upwards if doing so would prevent us from being able to see the next position
                                // OR the modified next position.
                                continue;
                            }
                        } else if !self.world.clear_view(dest, context.path_vec(i), false) {
                            // But if it's far away, allow gaining altitude if we could lose it again by the time we get
                            // there.
                            continue;
                        }
                    }

                    let min_avoidance = self.settings.minimum_avoidance;
                    let growth = match relaxation {
                        2 => None,
                        0 => Some(2.0 * min_avoidance),
                        _ => Some(min_avoidance),
                    };

                    if self.is_hitbox_clear(context, dest, growth) {
                        // Yaw is trivial, just calculate the rotation required to face the destination.
                        let yaw = calc_rotation_from_vec3d(start.to_array(), dest.to_array(), context.player_rotations).yaw;

                        match self.solve_pitch_for(context, dest, relaxation) {
                            None => {
                                solution = Some(Solution {
                                    context: context.clone(),
                                    rotation: Rotation::new(yaw, context.player_rotations.pitch),
                                    going_to: None,
                                    solved_pitch: false,
                                    force_use_firework: false,
                                    simulation_line: None,
                                });
                                continue;
                            }
                            Some((pitch, force, steps)) => {
                                // A solution was found with yaw AND pitch, so just immediately return it.
                                return Some(Solution {
                                    context: context.clone(),
                                    rotation: Rotation::new(yaw, pitch),
                                    going_to: Some(dest),
                                    solved_pitch: true,
                                    force_use_firework: force,
                                    simulation_line: Some(steps),
                                });
                            }
                        }
                    }
                }
                if i == 0 {
                    break;
                }
                i -= 1;
            }
        }
        solution
    }

    /// `isHitboxClear`: the player's hitbox, grown a little, can fly from the start to the destination.
    fn is_hitbox_clear(&self, context: &SolverContext, dest: V3, grow_amount: Option<f64>) -> bool {
        let start = context.start;
        let ignore_lava = context.ignore_lava;

        if !self.world.clear_view(start, dest, ignore_lava) {
            return false;
        }
        let Some(grow) = grow_amount else { return true };

        let bb = context.bounding_box.inflate(grow);
        let (ox, oy, oz) = (dest.x - start.x, dest.y - start.y, dest.z - start.z);

        let src = [
            bb.min_x, bb.min_y, bb.min_z, bb.min_x, bb.min_y, bb.max_z, bb.min_x, bb.max_y, bb.min_z, bb.min_x, bb.max_y, bb.max_z, bb.max_x, bb.min_y, bb.min_z, bb.max_x, bb.min_y, bb.max_z, bb.max_x,
            bb.max_y, bb.min_z, bb.max_x, bb.max_y, bb.max_z,
        ];
        let dst = [
            bb.min_x + ox, bb.min_y + oy, bb.min_z + oz, bb.min_x + ox, bb.min_y + oy, bb.max_z + oz, bb.min_x + ox, bb.max_y + oy, bb.min_z + oz, bb.min_x + ox, bb.max_y + oy, bb.max_z + oz,
            bb.max_x + ox, bb.min_y + oy, bb.min_z + oz, bb.max_x + ox, bb.min_y + oy, bb.max_z + oz, bb.max_x + ox, bb.max_y + oy, bb.min_z + oz, bb.max_x + ox, bb.max_y + oy, bb.max_z + oz,
        ];

        // Use the non-batching method without early failure.
        if self.settings.render_hitbox_raytraces {
            let mut clear = true;
            for i in 0..8 {
                let s = V3::new(src[i * 3], src[i * 3 + 1], src[i * 3 + 2]);
                let d = V3::new(dst[i * 3], dst[i * 3 + 1], dst[i * 3 + 2]);
                // Don't forward ignoreLava since the batch call doesn't care about it.
                if !self.world.clear_view(s, d, false) {
                    clear = false;
                }
            }
            return clear;
        }

        self.world.all_clear(&src, &dst)
    }

    /// `solvePitch(context, goal, relaxation)`: the pitch to fly at, whether it takes a firework to get out of the
    /// current situation, and the flight that was simulated.
    fn solve_pitch_for(&self, context: &SolverContext, goal: V3, relaxation: i32) -> Option<(f32, bool, Vec<V3>)> {
        let desperate = relaxation == 2;
        let good_pitch = calc_rotation_from_vec3d(context.start.to_array(), goal.to_array(), context.player_rotations).pitch;
        let pitches = pitches_to_solve_for(good_pitch, desperate, self.settings.pitch_range);

        let solve = |ticks: i32, ticks_boosted: i32, ticks_boost_delay: i32| self.solve_pitch(context, goal, relaxation, &pitches, ticks, ticks_boosted, ticks_boost_delay);

        let mut tests: Vec<(i32, i32, i32)> = Vec::new();

        if context.boost.is_boosted() {
            let guaranteed = context.boost.guaranteed_boost_ticks();
            if guaranteed == 0 {
                // Uncertain when boost will run out.
                let lookahead = i32::max(4, 10 - context.boost.maximum_boost_ticks());
                tests.push((lookahead, 1, 0));
            } else if guaranteed <= 5 {
                // Boost will run out within 5 ticks.
                tests.push((guaranteed + 5, guaranteed, 0));
            } else {
                // There's plenty of guaranteed boost.
                tests.push((guaranteed + 1, guaranteed, 0));
            }
        }

        // Standard test, assume (not) boosted for entire duration.
        let ticks = if desperate {
            3
        } else if context.boost.is_boosted() {
            i32::max(5, context.boost.guaranteed_boost_ticks())
        } else {
            self.settings.simulation_ticks
        };
        tests.push((ticks, if context.boost.is_boosted() { ticks } else { 0 }, 0));

        for (a, b, c) in tests {
            if let Some(result) = solve(a, b, c) {
                return Some((result.pitch, false, result.steps));
            }
        }

        // If we used a firework would we be able to get out of the current situation??? perhaps.
        if desperate {
            for delay in [3, 2, 1] {
                if let Some(result) = solve(ticks, 10, delay) {
                    return Some((result.pitch, true, result.steps));
                }
            }
        }

        None
    }

    #[allow(clippy::too_many_arguments)]
    fn solve_pitch(&self, context: &SolverContext, goal: V3, relaxation: i32, pitches: &[f32], ticks: i32, ticks_boosted: i32, ticks_boost_delay: i32) -> Option<PitchResult> {
        // We are at a certain velocity, but we have a target velocity: what pitch would get us closest to our target
        // velocity? Yaw is easy so we only care about pitch.
        let goal_delta = goal - context.start;
        let goal_direction = goal_delta.normalize();

        // Each result that beat the one before it, the best last.
        let mut best_results: Vec<PitchResult> = Vec::new();

        for &pitch in pitches {
            let Some(displacement) = self.simulate(context, goal_delta, pitch, ticks, ticks_boosted, ticks_boost_delay) else { continue };
            let last = *displacement.last().expect("the start is always in it");
            let mut goodness = goal_direction.dot(last.normalize());
            if self.landing_mode {
                goodness = -(goal_delta - last).length();
            }
            if best_results.last().is_none_or(|best| goodness > best.dot) {
                best_results.push(PitchResult { pitch, dot: goodness, steps: displacement });
            }
        }

        'outer: for result in best_results.into_iter().rev() {
            if relaxation < 2 {
                // Ensure that the goal is visible along the entire simulated path. Reverse order iteration since the last
                // position is most likely to fail.
                for i in (1..result.steps.len()).rev() {
                    if !self.world.clear_view(context.start + result.steps[i], goal, context.ignore_lava) {
                        continue 'outer;
                    }
                }
            } else if !self.world.clear_view(context.start + *result.steps.last().expect("the start is always in it"), goal, context.ignore_lava) {
                // Ensure that the goal is visible from the final position.
                continue;
            }
            return Some(result);
        }
        None
    }

    /// `simulate`: the player's flight for some ticks at a pitch, as offsets from the start; None if it meets a block.
    fn simulate(&self, context: &SolverContext, goal_delta: V3, pitch: f32, ticks: i32, ticks_boosted: i32, ticks_boost_delay: i32) -> Option<Vec<V3>> {
        let mut aim = context.aim.fork();
        let mut delta = goal_delta;
        let mut motion = context.motion;
        let mut hitbox = context.bounding_box;
        let mut displacement = Vec::with_capacity(ticks.max(0) as usize + 1);
        displacement.push(V3::ZERO);
        let mut remaining_ticks_boosted = ticks_boosted;

        for i in 0..ticks {
            if delta.length_sqr() < 1.0 {
                break;
            }
            let rotation = aim.next_rotation(calc_rotation_from_vec3d([0.0; 3], delta.to_array(), context.player_rotations).with_pitch(pitch));
            let look_direction = V3::from_array(calc_look_direction_from_rotation(rotation));

            motion = step(motion, look_direction, rotation.pitch);
            delta = delta - motion;

            // Collision box while the player is in motion, with additional padding for safety. expandTowards gives the
            // directional swept volume (it handles negative vectors correctly, unlike inflate, and covers the whole sweep,
            // unlike move).
            let in_motion = hitbox.expand_towards(motion).inflate(0.01);

            let (xmin, xmax) = (fast_floor(in_motion.min_x), fast_ceil(in_motion.max_x));
            let (ymin, ymax) = (fast_floor(in_motion.min_y), fast_ceil(in_motion.max_y));
            let (zmin, zmax) = (fast_floor(in_motion.min_z), fast_ceil(in_motion.max_z));
            for x in xmin..xmax {
                for y in ymin..ymax {
                    for z in zmin..zmax {
                        if !self.world.passable(x, y, z, context.ignore_lava) {
                            return None;
                        }
                    }
                }
            }

            hitbox = hitbox.translate(motion);
            let last = *displacement.last().expect("pushed above");
            displacement.push(last + motion);

            if i >= ticks_boost_delay {
                let boosted = remaining_ticks_boosted > 0;
                remaining_ticks_boosted -= 1;
                if boosted {
                    // See EntityFireworkRocket.
                    motion = motion
                        + V3::new(
                            look_direction.x * 0.1 + (look_direction.x * 1.5 - motion.x) * 0.5,
                            look_direction.y * 0.1 + (look_direction.y * 1.5 - motion.y) * 0.5,
                            look_direction.z * 0.1 + (look_direction.z * 1.5 - motion.z) * 0.5,
                        );
                }
            }
        }

        Some(displacement)
    }
}

/// `tickUseFireworks`: whether a firework is to be used this tick. The cool-down and setback delay are the caller's to
/// check.
#[allow(clippy::too_many_arguments)]
pub fn wants_firework(start: V3, going_to: V3, is_boosted: bool, force_use_firework: bool, conserve_fireworks: bool, firework_speed: f64, player_y: f64, motion: V3) -> bool {
    let use_on_descend = !conserve_fireworks || player_y < going_to.y + 5.0;
    let current_speed = V3::new(
        motion.x,
        // Ignore the y component if we are BOTH below where we want to be AND descending.
        if player_y < going_to.y { f64::max(0.0, motion.y) } else { motion.y },
        motion.z,
    )
    .length_sqr();

    force_use_firework
        || (!is_boosted
            && use_on_descend
            && (player_y < going_to.y - 5.0 || start.distance_to(V3::new(going_to.x + 0.5, player_y, going_to.z + 0.5)) > 5.0)
            && current_speed < firework_speed * firework_speed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baritone_elytra_chunk::Chunk;
    use crate::baritone_elytra_pathfinder::{CacheMiss, Dimension, NetherPathfinder};
    use crate::baritone_elytra_raytracer::raytrace;

    /// A world of octree chunks; any chunk it does not hold is solid, as in the pathfinder's passable().
    struct Octree {
        npf: NetherPathfinder,
    }

    impl Octree {
        fn open_sky() -> Self {
            let npf = NetherPathfinder::new(1, None, Dimension::Overworld, 384).unwrap();
            for x in -3..16 {
                for z in -3..4 {
                    npf.insert_chunk(x, z, Chunk::new());
                }
            }
            Self { npf }
        }

        /// Makes these blocks solid. The chunks they are in were empty, and are replaced.
        fn fill(&self, blocks: &[(i32, i32, i32)]) {
            let mut chunks: std::collections::HashMap<(i32, i32), Chunk> = std::collections::HashMap::new();
            for &(x, y, z) in blocks {
                chunks.entry((x >> 4, z >> 4)).or_default().set_block(x & 15, y, z & 15, true);
            }
            for ((cx, cz), chunk) in chunks {
                self.npf.insert_chunk(cx, cz, chunk);
            }
        }
    }

    impl FlightWorld for Octree {
        fn passable(&self, x: i32, y: i32, z: i32, _ignore_lava: bool) -> bool {
            if !(0..384).contains(&y) {
                return true;
            }
            !self.npf.get_chunk_or_default(x >> 4, z >> 4, true).is_solid(x & 15, y, z & 15)
        }

        fn clear_view(&self, start: V3, dest: V3, _ignore_lava: bool) -> bool {
            start == dest || raytrace(&self.npf, start.to_array(), dest.to_array(), CacheMiss::Solid).unwrap().is_none()
        }

        fn all_clear(&self, src: &[f64; 24], dst: &[f64; 24]) -> bool {
            (0..8).all(|i| {
                let o = i * 3;
                raytrace(&self.npf, [src[o], src[o + 1], src[o + 2]], [dst[o], dst[o + 1], dst[o + 2]], CacheMiss::Solid).unwrap().is_none()
            })
        }
    }

    fn settings() -> FlightSettings {
        FlightSettings { minimum_avoidance: 0.2, pitch_range: 25, simulation_ticks: 20, render_hitbox_raytraces: false }
    }

    fn context(path: Vec<[i32; 3]>, start: V3, motion: V3, boosted: bool) -> SolverContext {
        let (w, h) = (0.3, 1.8);
        let rotation = Rotation::new(-90.0, 0.0);
        SolverContext {
            path: Arc::new(path),
            player_near: 0,
            start,
            motion,
            bounding_box: Aabb { min_x: start.x - w, min_y: start.y, min_z: start.z - w, max_x: start.x + w, max_y: start.y + h, max_z: start.z + w },
            ignore_lava: false,
            boost: FireworkBoost::new(if boosted { Some(0) } else { None }, 10),
            aim: TickableAim::new(AimProcessor::new(7), rotation, AimSettings { random_looking: 0.0, random_looking_113: 0.0, sensitivity: 0.5 }),
            player_rotations: rotation,
        }
    }

    fn solver<'a>(world: &'a Octree, settings: &'a FlightSettings, never: &'a dyn Fn() -> bool) -> Solver<'a> {
        Solver { world, settings, landing_mode: false, cancelled: never }
    }

    #[test]
    fn a_tick_of_level_flight_from_rest_follows_the_formula() {
        // pitchBase3 = 1, motionY = -0.08 + 0.06 = -0.02; the speed modifier is -0.02 * -0.1 = 0.002, which y and z gain.
        // The flat motion was zero when the tick began, so the last pull of z towards the look direction takes a tenth
        // of the gain back: z = (0.002 - 0.0002) * 0.99.
        let m = step(V3::ZERO, V3::new(0.0, 0.0, 1.0), 0.0);
        assert!((m.y - (-0.018 * f64::from(0.98f32))).abs() < 1e-12, "{m:?}");
        assert!((m.z - 0.0018 * f64::from(0.99f32)).abs() < 1e-12, "{m:?}");
        assert_eq!(m.x, 0.0);
    }

    #[test]
    fn climbing_trades_speed_for_height() {
        // Looking up at 45 degrees at speed 1, the player gains height and loses forward speed.
        let look = V3::from_array(calc_look_direction_from_rotation(Rotation::new(0.0, -45.0)));
        let m = step(V3::new(0.0, 0.0, 1.0), look, -45.0);
        assert!(m.y > 0.0, "{m:?}");
        assert!(m.z < 1.0, "{m:?}");
    }

    #[test]
    fn firework_boost_counts_down_from_the_ticks_the_entity_has_lived() {
        let b = FireworkBoost::new(Some(4), 10);
        assert!(b.is_boosted());
        assert_eq!(b.guaranteed_boost_ticks(), 6);
        assert_eq!(b.maximum_boost_ticks(), 17);
        let late = FireworkBoost::new(Some(30), 10);
        assert_eq!((late.guaranteed_boost_ticks(), late.maximum_boost_ticks()), (0, 0));
        assert_eq!(FireworkBoost::new(None, 10).guaranteed_boost_ticks(), 0);
        // Two unboosted states are equal whatever the rest says.
        assert_eq!(FireworkBoost::new(None, 10), FireworkBoost::new(None, 30));
        assert_ne!(FireworkBoost::new(Some(1), 10), FireworkBoost::new(Some(2), 10));
    }

    #[test]
    fn pitches_come_good_first_then_alternate_up_and_down() {
        let p = pitches_to_solve_for(10.0, false, 3);
        assert_eq!(p, vec![10.0, 11.0, 12.0, 13.0, 9.0, 8.0, 7.0]);
        // A pitch near the limit is clamped to +-89.
        assert_eq!(*pitches_to_solve_for(88.0, false, 5).last().unwrap(), 83.0);
        assert_eq!(pitches_to_solve_for(0.0, true, 25).len(), 181);
    }

    #[test]
    fn open_sky_gives_a_solution_towards_the_next_node() {
        let world = Octree::open_sky();
        let s = settings();
        let never = || false;
        // Flying east at y 100 with speed, the path ahead along x.
        let path: Vec<[i32; 3]> = (0..6).map(|i| [10 + i * 16, 100, 0]).collect();
        let ctx = context(path, V3::new(0.5, 100.0, 0.5), V3::new(1.2, -0.05, 0.0), false);
        let solution = solver(&world, &s, &never).solve_angles(&ctx).expect("a solution");
        assert!(solution.solved_pitch);
        let to = solution.going_to.expect("somewhere to go");
        assert!(to.x > 10.0, "{to:?}");
        // East is yaw -90.
        assert!((solution.rotation.yaw + 90.0).abs() < 20.0, "yaw {}", solution.rotation.yaw);
        assert!(solution.rotation.pitch.abs() <= 90.0);
        assert!(solution.simulation_line.as_ref().unwrap().len() > 1);
    }

    #[test]
    fn a_wall_with_a_gap_is_flown_through_the_gap() {
        let world = Octree::open_sky();
        // A wall at x = 40 across the whole width, except a gap at y 96..=108, z -3..=3.
        let mut wall = Vec::new();
        for y in 60..140 {
            for z in -12..12 {
                if !((96..=108).contains(&y) && (-3..=3).contains(&z)) {
                    wall.push((40, y, z));
                }
            }
        }
        world.fill(&wall);
        let s = settings();
        let never = || false;
        let path: Vec<[i32; 3]> = vec![[10, 100, 0], [25, 100, 0], [40, 100, 0], [55, 100, 0], [70, 100, 0]];
        let ctx = context(path, V3::new(0.5, 100.0, 0.5), V3::new(1.2, -0.02, 0.0), false);
        let solution = solver(&world, &s, &never).solve_angles(&ctx).expect("a solution");
        let to = solution.going_to.expect("somewhere to go");
        // The destination is reachable in a straight line, the hitbox rays included.
        assert!(world.clear_view(ctx.start, to, false), "{to:?}");
        assert!(solution.solved_pitch);
    }

    #[test]
    fn a_cancelled_solve_gives_up() {
        let world = Octree::open_sky();
        let s = settings();
        let yes = || true;
        let path: Vec<[i32; 3]> = (0..6).map(|i| [10 + i * 16, 100, 0]).collect();
        let ctx = context(path, V3::new(0.5, 100.0, 0.5), V3::new(1.2, -0.05, 0.0), false);
        assert!(solver(&world, &s, &yes).solve_angles(&ctx).is_none());
    }

    #[test]
    fn enclosed_in_stone_there_is_no_pitch() {
        let world = Octree::open_sky();
        // A box of solid blocks around the start: nothing can be flown through.
        let mut shell = Vec::new();
        for x in -3..=4 {
            for y in 94..=106 {
                for z in -3..=4 {
                    if x == -3 || x == 4 || y == 94 || y == 106 || z == -3 || z == 4 {
                        shell.push((x, y, z));
                    }
                }
            }
        }
        world.fill(&shell);
        let s = settings();
        let never = || false;
        let path: Vec<[i32; 3]> = (0..4).map(|i| [10 + i * 16, 100, 0]).collect();
        let ctx = context(path, V3::new(0.5, 100.0, 0.5), V3::new(0.3, -0.05, 0.0), false);
        let solution = solver(&world, &s, &never).solve_angles(&ctx);
        assert!(solution.is_none_or(|s| !s.solved_pitch));
    }

    #[test]
    fn a_firework_is_wanted_when_slow_and_not_already_boosted() {
        let start = V3::new(0.5, 100.0, 0.5);
        let to = V3::new(100.0, 100.0, 0.0);
        let slow = V3::new(0.5, 0.0, 0.0);
        let fast = V3::new(1.6, 0.0, 0.0);
        assert!(wants_firework(start, to, false, false, false, 1.2, 100.0, slow));
        assert!(!wants_firework(start, to, false, false, false, 1.2, 100.0, fast));
        assert!(!wants_firework(start, to, true, false, false, 1.2, 100.0, slow));
        // Forced always wins.
        assert!(wants_firework(start, to, true, true, false, 1.2, 100.0, fast));
        // Conserving fireworks, one is not used while well above where it is going.
        assert!(!wants_firework(start, V3::new(100.0, 50.0, 0.0), false, false, true, 1.2, 100.0, slow));
    }
}
