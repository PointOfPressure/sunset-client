// baritone.pathing.path.SprintJump: one sprint jump along a straight run of path, simulated tick by tick the
// same way LivingEntity does it, to pick between sprinting flat out and braking to land on a chosen cell.
use super::baritone_calc_path::Path;
use super::baritone_executor::ExecEnv;
use super::baritone_movement::Kind;
use super::baritone_movement_helper as mh;
use super::baritone_rotation::Rotation;
use super::baritone_utils::Pos;

// Straight out of 1.21.4 LivingEntity.travelInAir / jumpFromGround / Player.getFlyingSpeed; aiStep scales the
// move input by 0.98.
const GROUND_FRICTION: f64 = 0.6 * 0.91;
const AIR_FRICTION: f64 = 0.91;
const AIR_ACCEL: f64 = 0.026 * 0.98;
const JUMP_BOOST: f64 = 0.2;
const JUMP: f64 = 0.42;
const GRAVITY: f64 = 0.08;
const SAFE_FALL: f64 = 3.0;
const EPS: f64 = 1e-6;
// A braked hop that lands short is a slow way to walk: ground sprint is ~0.28 a tick, going down steps or
// bonking one is way worse.
const FLAT_PACE: f64 = 0.28;
const SLOW_PACE: f64 = 0.15;
/// ~4 blocks a jump, plus enough past that to see whether the next hop can pop a step.
const MAX_RUNWAY: usize = 10;
pub const MAX_TICKS: i32 = 40;
/// We are exact about vanilla, not about what the server does with our yaw.
const MARGINS: [f64; 3] = [0.85, 1.0, 1.15];
/// Not a spot on the line, a throttle policy.
const BRAKE_THEN_PUSH: f64 = f64::NEG_INFINITY;

#[derive(Clone, Copy)]
struct Landing {
    s: f64,
    y: f64,
    apex: f64,
    v: f64,
    ticks: i32,
}

#[derive(Clone)]
pub struct SprintJump {
    /// Relative to the takeoff floor, cell 0 is the one we take off from.
    pub floors: Vec<f64>,
    spacing: f64,
    half_width: f64,
    ground_accel: f64,
    origin_x: f64,
    origin_z: f64,
    takeoff_y: f64,
    ux: f64,
    uz: f64,
    target: f64,
    apex: f64,
    pushing: bool,
    pub ticks: i32,
}

/// `MovementHelper.fullyPassable` over a column.
fn clear(e: &ExecEnv, x: i32, y: i32, z: i32, top: i32) -> bool {
    (y..=top).all(|yy| mh::fully_passable(e.calc, x, yy, z))
}

/// Ice and slime change friction, soul sand and honey are slow, farmland is not full height, and hopping on
/// magma without sneaking hurts.
fn solid_floor(e: &ExecEnv, p: Pos) -> bool {
    let s = e.ctx.bsi().get0(p[0], p[1], p[2]);
    let name = s.bare_name();
    let slippery = matches!(name, "ice" | "packed_ice" | "blue_ice" | "frosted_ice" | "slime_block" | "soul_sand" | "honey_block" | "magma_block");
    !slippery && s.full_collision
}

fn side(e: &ExecEnv, x: i32, y: i32, z: i32, top: i32) -> bool {
    solid_floor(e, [x, y - 1, z]) && clear(e, x, y, z, top)
}

impl SprintJump {
    fn new(floors: Vec<f64>, spacing: f64, ground_accel: f64) -> Self {
        Self {
            floors,
            spacing,
            half_width: 0.3 * spacing, // an axis aligned box projected onto a diagonal reaches sqrt 2 times further
            ground_accel,
            origin_x: 0.0,
            origin_z: 0.0,
            takeoff_y: 0.0,
            ux: 0.0,
            uz: 0.0,
            target: 0.0,
            apex: 0.0,
            pushing: false,
            ticks: 0,
        }
    }

    pub fn plan(e: &ExecEnv, path: &Path, path_position: usize, env: &super::baritone_movement_exec::Env) -> Option<SprintJump> {
        let ctx = e.ctx;
        let start = path.positions[path_position];
        let m0 = &path.movements[path_position];
        let dir = (m0.dest[0] - m0.src[0], m0.dest[1] - m0.src[1], m0.dest[2] - m0.src[2]);
        let diagonal = dir.0 != 0 && dir.2 != 0;
        let p = ctx.position();
        // The sim knows nothing about potions or modded physics, and jump boost off a hill is how you break your
        // legs. The head tops out at y + 3.05, so y + 3 has to be clear. A ceiling at y + 2 is a head hitter.
        if !ctx.on_ground()
            || !ctx.is_sprinting()
            || ctx.is_in_water()
            || ctx.is_in_lava()
            || ctx.on_climbable()
            || ctx.player_feet() != start
            || (p[1] - f64::from(start[1])).abs() > 1e-3
            || (diagonal && !e.settings.flag("sprintJumpingDiagonals"))
            || ctx.has_unsupported_motion_effects()
            || !solid_floor(e, [start[0], start[1] - 1, start[2]])
            || !clear(e, start[0], start[1], start[2], start[1] + 3)
        {
            return None;
        }
        let mut floors = vec![0.0; MAX_RUNWAY + 1];
        let mut n = 1;
        let mut i = path_position;
        while i < path.movements.len() && n <= MAX_RUNWAY {
            let m = &path.movements[i];
            let (src, dest) = (m.src, m.dest);
            let top = (dest[1] + 2).max(start[1] + 3); // we fly over lower cells at takeoff height
            let walking = if diagonal { matches!(m.kind, Kind::Diagonal { .. }) && dest[1] == src[1] } else { matches!(m.kind, Kind::Traverse | Kind::Ascend | Kind::Descend) };
            // Breaking is like 5x slower when you're jumping, and momentum doesn't wait for a block to be placed. On
            // a diagonal the hitbox corners sweep through both side cells, and drift can land us in one.
            if !walking
                || dest[0] - src[0] != dir.0
                || dest[2] - src[2] != dir.2
                || !m.to_break(env).is_empty()
                || !m.to_place(env).is_empty()
                || !solid_floor(e, [dest[0], dest[1] - 1, dest[2]])
                || !clear(e, dest[0], dest[1], dest[2], top)
                || (diagonal && !(side(e, src[0] + dir.0, src[1], src[2], top) && side(e, src[0], src[1], src[2] + dir.2, top)))
            {
                break;
            }
            floors[n] = f64::from(dest[1] - start[1]);
            i += 1;
            n += 1;
        }
        floors.truncate(n);
        let spacing = if diagonal { std::f64::consts::SQRT_2 } else { 1.0 };
        // solidFloor pins friction to 0.6, so the 0.216 / f^3 in the ground acceleration is 1.
        let mut jump = SprintJump::new(floors, spacing, ctx.movement_speed() * 0.98);
        jump.origin_x = f64::from(start[0]) + 0.5;
        jump.origin_z = f64::from(start[2]) + 0.5;
        jump.takeoff_y = f64::from(start[1]);
        jump.ux = f64::from(dir.0) / spacing;
        jump.uz = f64::from(dir.2) / spacing;
        let pos = [p[0] - jump.origin_x, p[1], p[2] - jump.origin_z];
        let vel = ctx.delta_movement();
        // Drifting sideways is how you clip a corner.
        if n < 3 || jump.across(pos[0], pos[2]).abs() > 0.2 || jump.across(vel[0], vel[2]).abs() > 0.08 {
            return None;
        }
        let target = jump.plan_hop(jump.along(pos[0], pos[2]), jump.along(vel[0], vel[2]))?;
        jump.target = target;
        Some(jump)
    }

    fn along(&self, x: f64, z: f64) -> f64 {
        x * self.ux + z * self.uz
    }

    fn across(&self, x: f64, z: f64) -> f64 {
        z * self.ux - x * self.uz
    }

    /// Full send (NaN), brake to land around a distance along the line, or don't jump (None).
    fn plan_hop(&self, s: f64, v: f64) -> Option<f64> {
        // A hop that lands right in front of a step leaves the next one nothing to do but bonk the face, lose
        // sprint, and hand it to MovementAscend. So with a step ahead, pick a hop the next one can pop it from, or
        // sprint a tick or two more if that sets one up.
        let climb = self.step_up_ahead(0);
        let chained = if climb { self.choose(s, v, &MARGINS, true) } else { None };
        if chained.is_some() || (climb && self.can_hop(s, v, &MARGINS, true, 1)) {
            chained
        } else {
            self.choose(s, v, &MARGINS, false)
        }
    }

    fn choose(&self, s: f64, v: f64, margins: &[f64], chain: bool) -> Option<f64> {
        if self.works(s, v, margins, f64::NAN, chain) {
            return Some(f64::NAN);
        }
        // Full send flies too far (over the first step of a hill and down the second) or sets up a bonk. Braking
        // only ever shortens a hop, so nothing past where full send lands.
        let full = self.jump(s, v, f64::NAN);
        for k in (1..self.floors.len()).rev() {
            let t = k as f64 * self.spacing;
            if full.is_none_or(|l| t <= l.s + self.spacing / 2.0) && self.works(s, v, margins, t, chain) {
                return Some(t);
            }
        }
        None
    }

    fn works(&self, s: f64, v: f64, margins: &[f64], target: f64, chain: bool) -> bool {
        for margin in margins {
            let l = self.jump(s, v * margin, target);
            let Some(land) = l else { return false };
            if !self.safe(Some(land)) {
                return false;
            }
            if !target.is_nan() {
                let pace = if land.y < -EPS || chain { SLOW_PACE } else { FLAT_PACE };
                if land.s - s < pace * f64::from(land.ticks) {
                    return false;
                }
            }
        }
        !chain || self.jump(s, v, target).is_some_and(|l| self.chains(l))
    }

    fn chains(&self, l: Landing) -> bool {
        let cell = (l.s / self.spacing).round() as i64;
        if cell < 0 || !self.step_up_ahead(cell as usize) {
            return true;
        }
        let cell = cell as usize;
        let next: Vec<f64> = (0..self.floors.len() - cell.min(self.floors.len())).map(|i| self.floors[cell + i] - l.y).collect();
        SprintJump::new(next, self.spacing, self.ground_accel).can_hop(l.s - cell as f64 * self.spacing, l.v, &[1.0], false, 0)
    }

    fn can_hop(&self, mut s: f64, mut v: f64, margins: &[f64], chain: bool, from: i32) -> bool {
        let mut wait = 0;
        while wait <= 2 && self.highest(s) <= EPS {
            if wait >= from && self.choose(s, v, margins, chain).is_some() {
                return true;
            }
            s += v + self.ground_accel;
            v = (v + self.ground_accel) * GROUND_FRICTION;
            wait += 1;
        }
        false
    }

    fn step_up_ahead(&self, cell: usize) -> bool {
        (cell + 1..self.floors.len()).any(|k| self.floors[k] > self.floors[cell] + EPS)
    }

    /// None if we hit a wall, leave the runway, or never come down.
    fn jump(&self, s: f64, v: f64, target: f64) -> Option<Landing> {
        self.fly(s, v + JUMP_BOOST, 0.0, JUMP, 0.0, true, target)
    }

    #[allow(clippy::too_many_arguments)]
    fn fly(&self, mut s: f64, mut v: f64, mut y: f64, mut vy: f64, mut apex: f64, jumping: bool, target: f64) -> Option<Landing> {
        let mut committed = false;
        for tick in 0..MAX_TICKS {
            // onGround is still true on the jump tick, so it gets ground acceleration and ground friction.
            let jump_tick = jumping && tick == 0;
            let throttle = if jump_tick {
                0
            } else if target == BRAKE_THEN_PUSH {
                if tick == 0 { -1 } else { 1 }
            } else {
                self.throttle(s, v, y, vy, apex, target, committed)
            };
            committed = throttle > 0;
            let disp = v + if jump_tick { self.ground_accel } else { AIR_ACCEL * f64::from(throttle) };
            // Vanilla collides y before x/z, so we're already as high as we're going to get this tick when we move
            // forward.
            y += vy;
            apex = apex.max(y);
            let landed = vy < 0.0 && y <= self.highest(s) + EPS;
            if landed {
                y = self.highest(s);
            }
            s += disp;
            if self.highest(s) > y + EPS {
                return None; // face first into a step, sprint gets cancelled and we just fall back down
            }
            if landed {
                return Some(Landing { s, y, apex, v: disp * AIR_FRICTION, ticks: tick + 1 });
            }
            v = disp * if jump_tick { GROUND_FRICTION } else { AIR_FRICTION };
            vy = (vy - GRAVITY) * 0.98;
        }
        None
    }

    /// Hold W facing forward (1) or backward (-1, brakes without losing sprint).
    #[allow(clippy::too_many_arguments)]
    fn throttle(&self, s: f64, v: f64, y: f64, vy: f64, apex: f64, target: f64, committed: bool) -> i32 {
        if target.is_nan() || committed {
            return 1;
        }
        // Brake first, push last. Same landing spot, but we come down moving, which is what the next hop needs to
        // pop a step. Brake while braking now and pushing after still gets there, then push for good: one flip, no
        // chatter.
        match self.fly(s, v, y, vy, apex, false, BRAKE_THEN_PUSH) {
            Some(late) if late.s < target => 1,
            _ => -1,
        }
    }

    fn safe(&self, l: Option<Landing>) -> bool {
        let Some(l) = l else { return false };
        if l.apex - l.y >= SAFE_FALL {
            return false; // small hills only
        }
        let cell = (l.s / self.spacing).round() as i64;
        // The lip of a step puts our feet in a block that isn't on the path. And we slide after landing
        // (v / (1 - 0.546) with W let go), which has to end over the runway too in case the path turns right after it.
        cell >= 0 && (cell as usize) < self.floors.len() && (self.floors[cell as usize] - l.y).abs() < EPS && self.highest(l.s + l.v.max(0.0) / (1.0 - GROUND_FRICTION)) != f64::INFINITY
    }

    /// Tallest floor under a hitbox centered at s. Past the runway is a wall so the whole flight stays over
    /// checked cells; behind it is where we came from (a hard braked prediction can drift back there).
    fn highest(&self, s: f64) -> f64 {
        let mut top = f64::NEG_INFINITY;
        let from = ((s - self.half_width) / self.spacing - 0.5).floor() as i64 + 1;
        let to = ((s + self.half_width) / self.spacing + 0.5).ceil() as i64 - 1;
        for k in from..=to {
            if k >= self.floors.len() as i64 {
                return f64::INFINITY;
            }
            top = top.max(self.floors[k.max(0) as usize]);
        }
        top
    }

    /// Which way to face this tick: forward on takeoff so the jump boost goes down the line, then push or brake,
    /// tilted to kill sideways drift.
    pub fn steer(&mut self, pos: [f64; 3], vel: [f64; 3], pitch: f32) -> Rotation {
        let rel = [pos[0] - self.origin_x, pos[1], pos[2] - self.origin_z];
        let mut throttle = 1;
        let mut accel = JUMP_BOOST + self.ground_accel;
        let first = self.ticks == 0;
        self.ticks += 1;
        if !first {
            self.apex = self.apex.max(rel[1] - self.takeoff_y);
            throttle = self.throttle(self.along(rel[0], rel[2]), self.along(vel[0], vel[2]), rel[1] - self.takeoff_y, vel[1], self.apex, self.target, self.pushing);
            self.pushing = throttle > 0;
            accel = AIR_ACCEL;
        }
        // Aiming at a point on the line ignores how fast we're already drifting toward it, so we'd sail past and
        // swing back forever (the diagonal wiggle). Ask for a sideways speed that shrinks with the offset instead.
        let side = ((-self.across(rel[0], rel[2]) * 0.3 - self.across(vel[0], vel[2])) / accel).clamp(-0.5, 0.5);
        let forward = f64::from(throttle) * (1.0 - side * side).sqrt();
        let yaw = (self.uz * forward + self.ux * side).atan2(self.ux * forward - self.uz * side).to_degrees() as f32 - 90.0;
        Rotation::new(yaw, pitch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_send_hop_over_flat_ground_lands_about_four_blocks_on() {
        let hop = SprintJump::new(vec![0.0; MAX_RUNWAY + 1], 1.0, 0.13 * 0.98);
        let l = hop.jump(0.0, 0.28, f64::NAN).expect("lands");
        assert!(l.s > 3.5 && l.s < 5.5, "{}", l.s);
        assert!(l.ticks > 8 && l.ticks < 15, "{}", l.ticks);
        assert!(hop.plan_hop(0.0, 0.28).is_some());
    }

    #[test]
    fn a_wall_right_ahead_is_no_place_to_jump() {
        let mut floors = vec![0.0; 4];
        floors[2] = 1.0;
        floors[3] = 1.0;
        let hop = SprintJump::new(floors, 1.0, 0.13 * 0.98);
        assert!(hop.jump(0.0, 0.28, f64::NAN).is_none() || !hop.safe(hop.jump(0.0, 0.28, f64::NAN)));
    }
}
