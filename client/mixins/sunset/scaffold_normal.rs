// Scaffold's Normal mode. Each tick before the game's, so every packet it causes goes before the tick's one move
// packet:
// - the cells: those under the line from where you are to where your speed takes you in two ticks (Expand blocks
//   further along your walk), at the height you bridge on (Same Y: the row you left the ground from), in order. The
//   line's cells always share a side, so a diagonal is a staircase whose every cell goes against the one before;
// - the click: for the first cell with a block beside it, a side of that block the eyes are outside of, within reach,
//   on a block a click would not use (a door, a chest), and on it the point whose look turns least (Backwards: the yaw
//   behind your walk on the 45s, and the pitch that meets the side), checked with the game's own ray. Without one, the
//   nearest cell within Closest block range that has one;
// - the place: the game's own right click on what a look meets (its sequence number and its prediction of the block),
//   once Ray cast allows it (Strict: the look the server already has, which the tick's move packet then repeats;
//   Normal: the look this tick's move packet carries, as a player's own click goes; Off: the planned side), and only
//   when the game says that click puts the block in one of the cells. One a tick at most, Delay ticks apart;
// - the look: turned toward the click at a speed between Min rotation speed and Rotation speed, in whole mouse counts,
//   held while it still meets a side to place on, and turned back to your camera when Scaffold stops or has had nothing
//   to place for a while. Silent: your camera stays; the move packets carry it;
// - movement correction: the tick's move goes by the look the server is told (Strict), with the move keys turned
//   first to the ones that walk your way at that look (Silent), so the PlayerInput the server is sent, the look and
//   the move all agree.
// The edge without sneaking: a side can be clicked only once the eyes are past it (the server's PositionPlace), and
// stepping off a block drops you only in the move after (the move collides down first), so the click goes in the
// first tick or two past the edge. Strict's look is aimed one move ahead, from the step this tick's move will take
// (the speed kept plus the push your keys gave the last move), turned toward the edge some ticks before you get there,
// and on the last tick before a fall Late click clicks with the look this tick's move packet carries.
// Written fresh against GrimAC's and NoCheatPlus's place checks. A block the server takes back is retried; a cell taken
// back twice waits two seconds.
use super::Game;
use super::sunset::{Module, legit_sneak, look_at, move_input, scaffold_slot};
use super::sunset_rotation::{Rng, degrees_per_count, direction, step, whole_counts};
use super::sunset_util as util;
use std::fmt::Write as _;
use std::sync::Mutex;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

// How far in from a side's edges a look's point stays.
const INSET: f64 = 0.12;
// Most cells a tick plans for, and most rays it casts to check looks.
const MAX_CELLS: usize = 24;
const MAX_RAYS: usize = 24;
const TELLY_RAYS: usize = 96;
// Ticks with nothing to place before the look goes back to your camera while Scaffold stays on.
const IDLE_TICKS: u32 = 15;
// How many moves ahead the look starts turning toward an edge you walk to.
const AHEAD_TICKS: u32 = 6;
// Half a player's width, a hair in, for what a player stands on.
const HALF_WIDTH: f64 = 0.299;
// A predicted block is watched this long for the server taking it back.
const WATCH_TICKS: u64 = 20;
// A cell whose block was taken back (or refused) this many times waits this long.
const COOL_FAILS: u8 = 2;
const COOL_TICKS: u64 = 40;

// The four blocks around a cell, then the one below: where a block can go against (the one above would be an
// upside-down place).
const BESIDE: [[i32; 3]; 5] = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, -1, 0]];

// Blocks a right click would use instead of placing against (besides those with a block entity, all of which do).
const USED: [&str; 24] = [
    "DoorBlock",
    "TrapDoorBlock",
    "FenceGateBlock",
    "ButtonBlock",
    "LeverBlock",
    "BedBlock",
    "NoteBlock",
    "CakeBlock",
    "CandleCakeBlock",
    "CraftingTableBlock",
    "AnvilBlock",
    "ComposterBlock",
    "FlowerPotBlock",
    "RespawnAnchorBlock",
    "RedStoneOreBlock",
    "RepeaterBlock",
    "ComparatorBlock",
    "BellBlock",
    "LoomBlock",
    "StonecutterBlock",
    "GrindstoneBlock",
    "CartographyTableBlock",
    "SmithingTableBlock",
    "DragonEggBlock",
];

// A right click on a block's side: the block, its side's outward normal, and the point on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Click {
    pub(super) pos: [i32; 3],
    pub(super) face: [i32; 3],
    pub(super) point: [f64; 3],
}

// A way to place a block: the click that puts it there, and the look whose ray makes that click.
#[derive(Clone, Copy, Debug)]
pub(super) struct Plan {
    pub(super) click: Click,
    pub(super) look: (f32, f32),
}

// A look considered for a plan, and what turning to it costs.
#[derive(Clone, Copy, Debug)]
struct Choice {
    plan: Plan,
    cost: f32,
}

// The cells wanted this tick, nearest first.
#[derive(Clone, Copy, Default)]
struct Cells {
    at: [[i32; 3]; MAX_CELLS],
    len: usize,
}

impl Cells {
    fn push(&mut self, cell: [i32; 3]) {
        if self.len < MAX_CELLS && !self.has(cell) {
            self.at[self.len] = cell;
            self.len += 1;
        }
    }
    fn has(&self, cell: [i32; 3]) -> bool {
        self.list().contains(&cell)
    }
    fn list(&self) -> &[[i32; 3]] {
        &self.at[..self.len]
    }
}

// A cell whose block did not stay: how many times, until when it waits, and when it last failed.
#[derive(Clone, Copy, Default)]
struct Failed {
    cell: [i32; 3],
    fails: u8,
    until: u64,
    last: u64,
}

// What Normal did, for Debug Report.
#[derive(Clone, Copy, Default)]
struct Stats {
    clicks: u64,
    predicted: u64,
    refused: u64,
    taken_back: u64,
    cooled: u64,
    late: u64,
    state: &'static str,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Correction {
    #[default]
    Off,
    Strict,
    Silent,
}

#[derive(Default)]
struct Normal {
    // "No blocks" was said, until there are blocks again.
    warned: bool,
    // The look the move packets carry while Normal holds one (your camera's own with Look Visible).
    look: Option<(f32, f32)>,
    visible: bool,
    correction: Correction,
    // This tick's turning speed.
    speed: f32,
    // Ticks with nothing to place, and ticks since you left the ground.
    idle: u32,
    air: u32,
    // Same Y: the row the blocks go in.
    row: Option<i32>,
    // The last tick's position and kept speed, for the step this tick's move takes.
    last_move: Option<(u64, [f64; 3], [f64; 3])>,
    last_place: Option<u64>,
    // The ticks of the last 20 places (ring), for Max per second.
    recent: [u64; 20],
    recent_at: usize,
    // The slot in hand before Scaffold took the blocks, the one it took, and whether it goes back when Scaffold stops.
    slot_before: Option<usize>,
    slot_used: Option<usize>,
    swap_back: bool,
    // Ledge sneak holds sneak.
    sneaking: bool,
    // This tick's: the right click's block and cell, and what is given back after the tick.
    forced: bool,
    pressed: Option<[i32; 3]>,
    render: bool,
    jumped: bool,
    right_taken: bool,
    keys_before: Option<[bool; 4]>,
    // Predicted blocks (cell, tick), and cells that failed.
    watch: [Option<([i32; 3], u64)>; 8],
    failed: [Option<Failed>; 8],
    cells: Cells,
    choices: Vec<Choice>,
    stats: Stats,
    rng: Option<Rng>,
}

static NORMAL: Mutex<Option<Normal>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Normal) -> R) -> R {
    let mut guard = NORMAL.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Normal::default))
}

impl Normal {
    fn random(&mut self, seed: u64) -> f64 {
        self.rng.get_or_insert_with(|| Rng::new(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)).next()
    }

    fn cooled(&self, cell: [i32; 3], tick: u64) -> bool {
        self.failed.iter().flatten().any(|f| f.cell == cell && f.until > tick)
    }

    // A cell's block did not stay: counted, and after COOL_FAILS the cell waits COOL_TICKS.
    fn fail(&mut self, cell: [i32; 3], tick: u64) {
        let i = self.failed.iter().position(|f| f.is_some_and(|f| f.cell == cell)).unwrap_or_else(|| {
            let oldest = |i: &usize| self.failed[*i].map_or(0, |f| f.last.max(f.until));
            (0..self.failed.len()).min_by_key(oldest).unwrap_or(0)
        });
        let entry = self.failed[i].filter(|f| f.cell == cell).unwrap_or(Failed { cell, ..Failed::default() });
        let fails = entry.fails + 1;
        self.failed[i] = Some(if fails >= COOL_FAILS {
            self.stats.cooled += 1;
            Failed { cell, fails: 0, until: tick + COOL_TICKS, last: tick }
        } else {
            Failed { cell, fails, until: entry.until, last: tick }
        });
    }

    // The blocks predicted lately: one gone again is the server taking it back.
    fn watch_results(&mut self, game: &Game<'_>, tick: u64) {
        for i in 0..self.watch.len() {
            let Some((cell, at)) = self.watch[i] else {
                continue;
            };
            if tick > at + WATCH_TICKS {
                self.watch[i] = None;
            } else if util::replaceable(game, cell) {
                self.watch[i] = None;
                self.stats.taken_back += 1;
                self.stats.state = "a block was taken back";
                self.fail(cell, tick);
            }
        }
    }

    fn watch(&mut self, cell: [i32; 3], tick: u64) {
        let i = self.watch.iter().position(Option::is_none).unwrap_or_else(|| {
            (0..self.watch.len()).min_by_key(|&i| self.watch[i].map_or(0, |w| w.1)).unwrap_or(0)
        });
        self.watch[i] = Some((cell, tick));
    }

    fn places_lately(&self, tick: u64) -> usize {
        self.recent.iter().filter(|&&t| t != 0 && t + 20 > tick).count()
    }
}

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

fn angle(a: (f32, f32), b: (f32, f32)) -> f32 {
    wrap(a.0 - b.0).hypot(a.1 - b.1)
}

// The yaw of the way the move keys (forward, left) walk at `yaw`: forward 0, left -90, right +90, back 180.
pub(super) fn walk_yaw(yaw: f32, (forward, left): (f64, f64)) -> f32 {
    if forward == 0.0 && left == 0.0 {
        return yaw;
    }
    yaw + (-left).atan2(forward).to_degrees() as f32
}

// How far `yaw` is from the nearest of the eight ways around `base` (0 to 22.5): at those, the move keys walk exactly
// the way you meant with the look at `yaw`.
fn off_eight(yaw: f32, base: f32) -> f32 {
    let d = wrap(yaw - base).abs() % 45.0;
    d.min(45.0 - d)
}

// Of the eight ways the move keys walk (forward and left, each -1, 0 or 1), the one that at the `look` yaw walks nearest
// the way (`forward`, `left`) walks at the `view` yaw; one key rather than two on a tie. No keys stay none.
fn turned_keys(forward: i8, left: i8, view: f32, look: f32) -> (i8, i8) {
    if forward == 0 && left == 0 {
        return (0, 0);
    }
    let wanted = walk_yaw(view, (f64::from(forward), f64::from(left)));
    let mut best = (f32::MAX, 0, 0);
    for f in -1..=1i8 {
        for l in -1..=1i8 {
            if f == 0 && l == 0 {
                continue;
            }
            let off = wrap(walk_yaw(look, (f64::from(f), f64::from(l))) - wanted).abs();
            let single = f == 0 || l == 0;
            let was_single = best.1 == 0 || best.2 == 0;
            if off < best.0 - 1e-3 || (off < best.0 + 1e-3 && single && !was_single) {
                best = (off, f, l);
            }
        }
    }
    (best.1, best.2)
}

// The cells (x, z) a line from `a` to `b` crosses, in order, each sharing a side with the one before (a line through a
// corner takes one of the two cells beside it as well). `visit` returns false to stop.
pub(super) fn line_cells(a: [f64; 2], b: [f64; 2], mut visit: impl FnMut([i32; 2]) -> bool) {
    let mut cell = [a[0].floor() as i32, a[1].floor() as i32];
    let end = [b[0].floor() as i32, b[1].floor() as i32];
    let d = [b[0] - a[0], b[1] - a[1]];
    let step: [i32; 2] = std::array::from_fn(|i| if d[i] > 0.0 { 1 } else { -1 });
    let stride: [f64; 2] = std::array::from_fn(|i| if d[i] == 0.0 { f64::INFINITY } else { (1.0 / d[i]).abs() });
    let mut next: [f64; 2] = std::array::from_fn(|i| {
        if d[i] > 0.0 {
            (f64::from(cell[i]) + 1.0 - a[i]) * stride[i]
        } else if d[i] < 0.0 {
            (a[i] - f64::from(cell[i])) * stride[i]
        } else {
            f64::INFINITY
        }
    });
    if !visit(cell) {
        return;
    }
    let mut left = (end[0] - cell[0]).abs() + (end[1] - cell[1]).abs();
    while left > 0 {
        let i = usize::from(next[0] >= next[1]);
        cell[i] += step[i];
        next[i] += stride[i];
        left -= 1;
        if !visit(cell) {
            return;
        }
    }
}

// A block's side: the axis it faces along, the plane it is on, and its rectangle on the other two axes, taken in from
// the edges.
#[derive(Clone, Copy, Debug)]
struct Side {
    axis: usize,
    plane: f64,
    lo: [f64; 3],
    hi: [f64; 3],
}

impl Side {
    // The side of a box (min x, y, z, max x, y, z) that `face` points out of.
    fn of(bounds: [f64; 6], face: [i32; 3]) -> Side {
        let axis = (0..3).find(|&i| face[i] != 0).unwrap_or(1);
        let plane = if face[axis] > 0 { bounds[3 + axis] } else { bounds[axis] };
        let mut lo = [bounds[0], bounds[1], bounds[2]];
        let mut hi = [bounds[3], bounds[4], bounds[5]];
        for i in 0..3 {
            if i == axis {
                (lo[i], hi[i]) = (plane, plane);
            } else {
                let inset = INSET.min((hi[i] - lo[i]) * 0.25);
                lo[i] += inset;
                hi[i] -= inset;
            }
        }
        Side { axis, plane, lo, hi }
    }

    // Whether `eye` is outside the side, so it can see it (the server's PositionPlace asks the same).
    fn faces(&self, eye: [f64; 3], face: [i32; 3]) -> bool {
        (eye[self.axis] - self.plane) * f64::from(face[self.axis]) > 1e-6
    }

    fn clamp(&self, p: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| if i == self.axis { self.plane } else { p[i].clamp(self.lo[i], self.hi[i]) })
    }

    // Where a ray from `eye` along `dir` crosses the side's plane, if ahead of it.
    fn crossing(&self, eye: [f64; 3], dir: [f64; 3]) -> Option<[f64; 3]> {
        if dir[self.axis].abs() < 1e-9 {
            return None;
        }
        let t = (self.plane - eye[self.axis]) / dir[self.axis];
        (t > 0.0).then(|| std::array::from_fn(|i| if i == self.axis { self.plane } else { eye[i] + dir[i] * t }))
    }

    // Nine points spread over the side (corners, edges' middles and its middle, all taken in).
    fn grid(&self) -> impl Iterator<Item = [f64; 3]> + '_ {
        let (u, v) = match self.axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let at = |i: usize, k: usize| [self.lo[i], (self.lo[i] + self.hi[i]) / 2.0, self.hi[i]][k];
        (0..9).map(move |k| {
            let mut p = [0.0; 3];
            p[self.axis] = self.plane;
            p[u] = at(u, k / 3);
            p[v] = at(v, k % 3);
            p
        })
    }

    // The pitches (least, most) whose ray from `eye` at `yaw` meets the side within its rectangle, if any.
    fn pitches(&self, eye: [f64; 3], yaw: f32) -> Option<(f32, f32)> {
        let r = f64::from(yaw).to_radians();
        let flat = [-r.sin(), 0.0, r.cos()];
        if self.axis == 1 {
            // A top: the ray meets it at the flat distance d where tan(pitch) = drop / d; d is kept to where the flat
            // point stays inside the rectangle along x and along z.
            let drop = eye[1] - self.plane;
            if drop <= 1e-6 {
                return None;
            }
            let (mut near, mut far) = (0.0f64, f64::INFINITY);
            for i in [0, 2] {
                if flat[i].abs() < 1e-9 {
                    if !(self.lo[i]..=self.hi[i]).contains(&eye[i]) {
                        return None;
                    }
                    continue;
                }
                let (a, b) = ((self.lo[i] - eye[i]) / flat[i], (self.hi[i] - eye[i]) / flat[i]);
                near = near.max(a.min(b));
                far = far.min(a.max(b));
            }
            if near > far {
                return None;
            }
            let pitch = |d: f64| drop.atan2(d).to_degrees() as f32;
            Some((pitch(far), pitch(near)))
        } else {
            // A side: the ray reaches its plane a flat distance t away, having dropped t tan(pitch) by then.
            if flat[self.axis].abs() < 1e-9 {
                return None;
            }
            let t = (self.plane - eye[self.axis]) / flat[self.axis];
            let other = 2 - self.axis;
            if t <= 1e-6 || !(self.lo[other]..=self.hi[other]).contains(&(eye[other] + flat[other] * t)) {
                return None;
            }
            let pitch = |y: f64| ((eye[1] - y) / t).atan().to_degrees() as f32;
            Some((pitch(self.hi[1]), pitch(self.lo[1])))
        }
    }
}

// The distance from `eye` to the nearest point of a box (the server's FarPlace measures so).
fn box_distance(eye: [f64; 3], b: [f64; 6]) -> f64 {
    let d: [f64; 3] = std::array::from_fn(|i| (b[i] - eye[i]).max(eye[i] - b[3 + i]).max(0.0));
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

// The box a block at `at` is clicked on (its whole outline), or None when it is no block to place against: air, a
// plant or a fluid (those are placed into), nothing to click, or a block a click would use.
fn support(game: &Game<'_>, at: [i32; 3]) -> Option<[f64; 6]> {
    if util::replaceable(game, at) {
        return None;
    }
    let f = util::facts(game, at);
    if f.outline.is_empty() || f.block_entity || f.fluid_level > 0 || USED.iter().any(|c| f.has_class(c)) {
        return None;
    }
    let mut b = [f64::MAX, f64::MAX, f64::MAX, f64::MIN, f64::MIN, f64::MIN];
    for o in f.outline.iter() {
        for i in 0..3 {
            b[i] = b[i].min(o[i]);
            b[3 + i] = b[3 + i].max(o[3 + i]);
        }
    }
    Some(std::array::from_fn(|i| b[i] + f64::from(at[i % 3])))
}

// What a look from `eye` clicks within `reach`: the first block side its ray meets.
fn ray(game: &Game<'_>, eye: [f64; 3], look: (f32, f32), reach: f64) -> Option<Click> {
    let hit = game.clip_ray(eye, direction(look), reach)?;
    (hit.face != [0; 3]).then_some(Click { pos: hit.pos, face: hit.face, point: hit.point })
}

// The same, when the block it meets is one to place against.
pub(super) fn placing_ray(game: &Game<'_>, eye: [f64; 3], look: (f32, f32), reach: f64) -> Option<Click> {
    ray(game, eye, look, reach).filter(|c| support(game, c.pos).is_some())
}

// The cell a click would put a block in, as the shape of things says (the game's own word is `places_into`).
pub(super) fn cell_of(click: &Click) -> [i32; 3] {
    std::array::from_fn(|i| click.pos[i] + click.face[i])
}

// How the looks for a plan are found.
#[derive(Clone, Copy)]
struct Aim {
    eye: [f64; 3],
    now: (f32, f32),
    reach: f64,
    // Backwards: the yaws behind your walk.
    back: Option<[f32; 4]>,
    // Silent movement correction: looks on the eight ways around your walk cost less.
    walk: Option<f32>,
}

// The looks that click a side of a block beside `cell`, with what each costs (how far the look turns, and with Silent
// movement correction how far its yaw is off the eight ways around your walk). False when no block beside it can be
// placed against at all (not merely none whose side the eyes see yet).
fn choices(game: &Game<'_>, out: &mut Vec<Choice>, cell: [i32; 3], aim: &Aim) -> bool {
    out.clear();
    let mut beside = false;
    for d in BESIDE {
        let pos = [cell[0] + d[0], cell[1] + d[1], cell[2] + d[2]];
        let Some(bounds) = support(game, pos) else {
            continue;
        };
        beside = true;
        let face = [-d[0], -d[1], -d[2]];
        let side = Side::of(bounds, face);
        if !side.faces(aim.eye, face) || box_distance(aim.eye, bounds) > aim.reach {
            continue;
        }
        let mut add = |look: (f32, f32), point: [f64; 3]| {
            let look = (aim.now.0 + wrap(look.0 - aim.now.0), look.1.clamp(-90.0, 90.0));
            let cost = angle(look, aim.now) + aim.walk.map_or(0.0, |walk| 2.0 * off_eight(look.0, walk));
            out.push(Choice { plan: Plan { click: Click { pos, face, point }, look }, cost });
        };
        // Backwards: the yaws behind you, each with the pitch nearest yours that meets the side. With none of them
        // meeting this side, the looks any player would use.
        let behind = aim.back.map(|yaws| {
            yaws.map(|yaw| {
                let (least, most) = side.pitches(aim.eye, yaw)?;
                // Within the middle half of the window: a step a little off the one foreseen still meets the side.
                let q = (most - least) / 4.0;
                let pitch = aim.now.1.clamp(least + q, most - q);
                let point = side.crossing(aim.eye, direction((yaw, pitch))).map_or_else(|| side.clamp(aim.eye), |p| side.clamp(p));
                Some(((yaw, pitch.min(90.0)), point))
            })
        });
        if let Some(behind) = behind.filter(|b| b.iter().any(Option::is_some)) {
            for (look, point) in behind.into_iter().flatten() {
                add(look, point);
            }
            continue;
        }
        if let Some(p) = side.crossing(aim.eye, direction(aim.now)) {
            let p = side.clamp(p);
            add(look_at(aim.eye, p), p);
        }
        for p in side.grid() {
            add(look_at(aim.eye, p), p);
        }
    }
    out.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    beside
}

// The plan for this tick: for the cells in order, the cheapest look whose ray really clicks its side (any, without
// `checked`). When the first cell has no block beside it at all, the nearest cell around it within `range` that has one
// (added to the cells); one whose side the eyes cannot see yet is waited for.
fn plan(game: &Game<'_>, n: &mut Normal, aim: &Aim, checked: bool, range: f64, tick: u64, budget: usize) -> Option<Plan> {
    let rays = std::cell::Cell::new(0);
    let mut choices = std::mem::take(&mut n.choices);
    let mut found = None;
    let mut bare = None;
    let cells = n.cells;
    let pick = |game: &Game<'_>, choices: &mut Vec<Choice>, cell: [i32; 3]| -> Option<Option<Plan>> {
        if !self::choices(game, choices, cell, aim) {
            return None;
        }
        for c in choices.iter() {
            if !checked {
                return Some(Some(c.plan));
            }
            if rays.get() >= budget {
                return Some(None);
            }
            rays.set(rays.get() + 1);
            if let Some(click) = ray(game, aim.eye, c.plan.look, aim.reach)
                && click.pos == c.plan.click.pos
                && click.face == c.plan.click.face
            {
                return Some(Some(Plan { click, ..c.plan }));
            }
        }
        Some(None)
    };
    for &cell in cells.list() {
        match pick(game, &mut choices, cell) {
            Some(Some(p)) => {
                found = Some(p);
                break;
            }
            Some(None) => {}
            None => {
                bare.get_or_insert(cell);
            }
        }
    }
    if found.is_none()
        && let Some(target) = bare.filter(|&t| Some(t) == cells.list().first().copied())
    {
        let r = range.min(6.0).floor() as i32;
        'search: for radius in 1..=r {
            for dy in [0, -1] {
                for dx in -radius..=radius {
                    for dz in -radius..=radius {
                        if rays.get() >= budget {
                            break 'search;
                        }
                        if dx.abs().max(dz.abs()) != radius {
                            continue;
                        }
                        let cell = [target[0] + dx, target[1] + dy, target[2] + dz];
                        if n.cooled(cell, tick) || !util::can_place(game, cell, true) {
                            continue;
                        }
                        if let Some(Some(p)) = pick(game, &mut choices, cell) {
                            n.cells.push(cell);
                            found = Some(p);
                            break 'search;
                        }
                    }
                }
            }
        }
    }
    n.choices = choices;
    found
}

// Telly reuses the same bounded geometry and ray planner, retaining its scratch allocation between ticks.
#[derive(Default)]
pub(super) struct Planner(Normal);

pub(super) fn plan_cells(
    planner: &mut Planner,
    game: &Game<'_>,
    targets: &[[i32; 3]],
    eye: [f64; 3],
    sent: (f32, f32),
    back: Option<[f32; 4]>,
    walk: Option<f32>,
    reach: f64,
) -> Option<Plan> {
    planner.0.cells = Cells::default();
    for &cell in targets {
        planner.0.cells.push(cell);
    }
    let aim = Aim {
        eye,
        now: sent,
        reach,
        back,
        walk,
    };
    // Telly has many cells in one jump (two to a step on a diagonal), and a third of a second to place them.
    plan(game, &mut planner.0, &aim, true, 0.0, game.ticks, TELLY_RAYS)
}

// A step of the look from `from` toward `to`, at most `speed` degrees a tick each way, in whole mouse counts.
pub(super) fn turn(from: (f32, f32), to: (f32, f32), speed: f32, degrees: f32) -> (f32, f32) {
    whole_counts(from, step("Custom", from, to, speed, speed, 0.0), degrees)
}

// A step of the look back to your camera. The yaw goes the way that unwinds it to your camera's own yaw (not the short
// way round): letting go at a yaw 360 off would be a full turn in one packet to the server (Grim's AimModulo360).
pub(super) fn turn_back(from: (f32, f32), camera: (f32, f32), speed: f32, degrees: f32) -> (f32, f32) {
    let counts = |d: f32| (d.clamp(-speed, speed) / degrees).round() * degrees;
    (from.0 + counts(camera.0 - from.0), (from.1 + counts(camera.1 - from.1)).clamp(-90.0, 90.0))
}

// Silent movement correction: the move keys turned to the ones that walk your way at the look's yaw. What they were is
// kept for after the tick.
fn turn_keys(game: &mut Game<'_>, n: &mut Normal, view: f32, look: f32) {
    turn_input(game, &mut n.keys_before, view, look);
}

pub(super) fn turn_input(game: &mut Game<'_>, before: &mut Option<[bool; 4]>, view: f32, look: f32) {
    let keys = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD];
    let held = keys.map(|k| game.keys.contains(&k));
    let forward = i8::from(held[0]) - i8::from(held[1]);
    let left = i8::from(held[2]) - i8::from(held[3]);
    let (f, l) = turned_keys(forward, left, view, look);
    let want = [f > 0, f < 0, l > 0, l < 0];
    if want == held || (forward == 0 && left == 0) {
        return;
    }
    before.get_or_insert(held);
    for (k, on) in keys.into_iter().zip(want) {
        if on {
            game.keys.insert(k);
        } else {
            game.keys.remove(&k);
        }
    }
}

// The block height you stand on (a slab's own block on half a block).
fn under_y(y: f64) -> i32 {
    if (y - y.trunc()).abs() == 0.5 { y.floor() as i32 } else { (y - 1.0).floor() as i32 }
}

// The step across this tick's move takes: the speed kept from the last move (after friction, only about half a walking
// step) plus the push the keys gave the last move (how far it went beyond the speed it started with), which holds while
// the keys and the ground stay the same. Without the last tick, or after a jolt (a hit, a setback), the kept speed alone.
fn next_step(last: Option<(u64, [f64; 3], [f64; 3])>, tick: u64, pos: [f64; 3], vel: [f64; 3]) -> [f64; 2] {
    let push = last.filter(|l| l.0 + 1 == tick).map_or([0.0; 2], |(_, p, v)| [pos[0] - p[0] - v[0], pos[2] - p[2] - v[2]]);
    if push[0].hypot(push[1]) > 0.3 {
        return [vel[0], vel[2]];
    }
    [vel[0] + push[0], vel[2] + push[1]]
}

// Whether a player at (x, z) stands on something in row `y`: a block that is not air, a plant or a fluid under a corner
// of its box.
fn stands(game: &Game<'_>, x: f64, z: f64, y: i32) -> bool {
    [[-1.0, -1.0], [1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]].iter().any(|c| {
        !util::replaceable(game, [(x + c[0] * HALF_WIDTH).floor() as i32, y, (z + c[1] * HALF_WIDTH).floor() as i32])
    })
}

// The game's own right click on `click`, if the game says it puts a block in one of the cells: the slot to the blocks
// first (the game tells the server before the click), then the click.
fn click(game: &mut Game<'_>, n: &mut Normal, click: Click, slot: usize, render: bool) -> Option<[i32; 3]> {
    let hit = super::Hit { pos: click.pos, face: click.face, point: click.point };
    let cell = super::remote::sunset_remote::places_into(game, &hit, slot)?;
    if !n.cells.has(cell) {
        return None;
    }
    if game.selected != slot {
        n.slot_before.get_or_insert(game.selected);
        game.selected = slot;
    }
    n.slot_used = Some(slot);
    game.forced_hit = Some(hit);
    super::remote::sunset_remote::press_use(game);
    // A held right click would use the forced block a second time this tick.
    if game.right {
        game.right = false;
        n.right_taken = true;
    }
    n.forced = true;
    n.pressed = Some(cell);
    n.render = render;
    n.last_place = Some(game.ticks);
    n.recent[n.recent_at] = game.ticks;
    n.recent_at = (n.recent_at + 1) % n.recent.len();
    n.stats.clicks += 1;
    n.stats.state = "placing";
    Some(cell)
}

// Each tick before the game's: Normal's cells, click, look, keys and jump. None: Scaffold is off, paused or in another
// mode, and the look turns back to your camera.
pub(super) fn pre_tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        release(game);
        return;
    };
    let clicking = game.physical_buttons.contains(&MouseButton::Right) || game.right;
    let slot = scaffold_slot(game, m);
    let flying = game.player.as_ref().is_none_or(|p| p.flying);
    let warn = with(|n| {
        let warn = slot.is_none() && !n.warned;
        n.warned = slot.is_none();
        warn
    });
    if warn {
        super::sunset::say(game, "Scaffold: no blocks to place (none in your hotbar, or the Blocks filter leaves none).");
    }
    let (Some(slot), false, true) = (slot, flying, clicking || !m.b("Only on click")) else {
        with(|n| n.stats.state = if slot.is_none() { "no blocks" } else { "waiting" });
        release(game);
        return;
    };
    with(|n| bridge(game, m, n, slot));
    fast_tower(game, m);
}

fn bridge(game: &mut Game<'_>, m: &Module, n: &mut Normal, slot: usize) {
    let tick = game.ticks;
    n.watch_results(game, tick);
    let (Some(eye), Some(p)) = (game.eye_position(), game.player.as_ref()) else {
        return;
    };
    let (pos, vel, ground) = ([p.position.x, p.position.y, p.position.z], [p.velocity.x, p.velocity.y, p.velocity.z], p.ground.on_ground);
    let step = next_step(n.last_move, tick, pos, vel);
    n.last_move = Some((tick, pos, vel));
    n.air = if ground { 0 } else { n.air.saturating_add(1) };
    let input = move_input(game);
    let moving = input != (0.0, 0.0);
    let space = game.keys.contains(&KeyCode::Space);
    let own_sneak = game.physical_keys.contains(&KeyCode::ShiftLeft) || (game.keys.contains(&KeyCode::ShiftLeft) && !n.sneaking);
    let mode = m.p("Rotations");
    let telly = mode == "Telly";
    let camera = (game.yaw, game.pitch);
    let walk = walk_yaw(camera.0, input);

    // The height: the Same Y row, else under where you will be (one lower while you sneak), never your feet's.
    let keep_row = (m.p("Same Y") != "Off" || telly) && moving && !space;
    let under = under_y(pos[1]);
    if ground || !keep_row || n.row.is_none_or(|r| pos[1] < f64::from(r) + 1.0 - 1e-3) {
        n.row = Some(under);
    }
    let feet = pos[1].floor() as i32;
    let mut y = if keep_row { n.row.unwrap_or(under) } else { (pos[1] + vel[1] - 0.75).floor() as i32 };
    if own_sneak && !space && !keep_row {
        y -= 1;
    }
    y = y.min(feet - 1);

    // The cells: under the line to where two steps take you, and Expand blocks on along your walk.
    let mut end = [pos[0] + step[0] * 2.0, pos[2] + step[1] * 2.0];
    let expand = m.n("Expand");
    if expand > 0.0 && moving {
        let r = f64::from(walk).to_radians();
        end = [end[0] - r.sin() * expand, end[1] + r.cos() * expand];
    }
    n.cells = Cells::default();
    line_cells([pos[0], pos[2]], end, |c| {
        let cell = [c[0], y, c[1]];
        if !n.cooled(cell, tick) && util::can_place(game, cell, true) {
            n.cells.push(cell);
        }
        n.cells.len < MAX_CELLS
    });

    let rotate = m.b("Rotate");
    let visible = rotate && m.p("Look") == "Visible";
    let ray_cast = if rotate { m.p("Ray cast") } else { "Off" };
    let correction = match m.p("Movement correction") {
        "Strict" if !visible => Correction::Strict,
        "Silent" if !visible => Correction::Silent,
        _ => Correction::Off,
    };
    let reach = m.n("Max placement distance").min(game.reach());
    let sent = if visible { camera } else { super::sunset_combat_wire::sent_look(game).unwrap_or(camera) };
    let now = if visible { camera } else { n.look.unwrap_or(sent) };
    // The look picked now is used for the click next tick with Strict (from where this tick's move takes you), this
    // tick's otherwise.
    let aim_eye = if ray_cast == "Strict" { [eye[0] + step[0], eye[1] + if ground { 0.0 } else { vel[1] }, eye[2] + step[1]] } else { eye };
    let back = (mode == "Backwards" || telly).then(|| {
        let behind = walk + 180.0;
        let snap = |a: f32| (wrap(a) / 45.0).round() * 45.0;
        [snap(behind), snap(behind + 45.0), snap(behind - 45.0), snap(camera.0 + 180.0)]
    });
    let aim = Aim {
        eye: aim_eye,
        now,
        reach,
        back,
        walk: (correction == Correction::Silent).then_some(walk),
    };

    // What may click this tick.
    let delay = m.n("Delay (ticks)") as u64;
    let attack = game.left || game.physical_buttons.contains(&MouseButton::Left);
    let wait = if n.cells.len == 0 {
        Some("nothing to place")
    } else if game.using.is_some() {
        Some("using an item")
    } else if attack {
        Some("attack held")
    } else if n.last_place.is_some_and(|t| tick < t + 1 + delay) {
        Some("delay")
    } else if n.places_lately(tick) as f64 >= m.n("Max per second") {
        Some("max per second")
    } else {
        None
    };
    let render = m.b("Render");
    let mut placed = None;
    if wait.is_none() && ray_cast == "Strict" {
        // The look the server already has; this tick's move packet repeats it.
        if let Some(c) = placing_ray(game, eye, sent, reach) {
            placed = click(game, n, c, slot, render);
        }
    }

    let found = if n.cells.len == 0 { None } else { plan(game, n, &aim, ray_cast != "Off", m.n("Closest block range"), tick, MAX_RAYS) };
    // The look still meets a side whose click fills a cell: it stays.
    let holding = placed.is_some()
        || placing_ray(game, aim_eye, now, reach).is_some_and(|c| n.cells.has(cell_of(&c)));
    // Nothing to click yet, walking to an edge: the look the click past it will want, from where the steps ahead take
    // you, so the turn is done by the time you get there.
    let ahead = if found.is_none() && !holding && rotate && moving && ground && !matches!(mode, "Snap" | "Telly") {
        (2..=AHEAD_TICKS).find_map(|k| {
            let k = f64::from(k);
            let cell = [(pos[0] + step[0] * k).floor() as i32, y, (pos[2] + step[1] * k).floor() as i32];
            if !util::can_place(game, cell, false) || n.cooled(cell, tick) {
                return None;
            }
            let there = Aim { eye: [eye[0] + step[0] * k, eye[1], eye[2] + step[1] * k], ..aim };
            let mut list = std::mem::take(&mut n.choices);
            choices(game, &mut list, cell, &there);
            let look = list.first().map(|c| c.plan.look);
            n.choices = list;
            look
        })
    } else {
        None
    };
    if found.is_some() || placed.is_some() || ahead.is_some() {
        n.idle = 0;
    } else {
        n.idle = n.idle.saturating_add(1);
    }
    let (lo, hi) = (m.n("Min rotation speed") as f32, m.n("Rotation speed") as f32);
    n.speed = lo.min(hi) + (hi - lo).abs() * n.random(tick) as f32;
    let degrees = degrees_per_count(f64::from(game.settings.sensitivity));
    let forward = (walk, now.1);
    let target = match mode {
        "Telly" if ground || f64::from(n.air) < m.n("Telly ticks") => Some(forward),
        "Snap" if found.is_none() && !holding => Some(forward),
        _ if holding => Some(now),
        _ => found.map(|p| p.look).or(ahead),
    };
    let mut next = if !rotate {
        None
    } else if n.idle > IDLE_TICKS && !matches!(mode, "Snap" | "Telly") {
        let back = turn_back(now, camera, n.speed.max(30.0), degrees);
        (angle(back, camera) > 1.0 || (back.0 - camera.0).abs() > 1.0).then_some(back)
    } else {
        Some(turn(now, target.unwrap_or(now), n.speed, degrees))
    };

    if wait.is_none() && placed.is_none() {
        match ray_cast {
            "Normal" => {
                // The look this tick's move packet carries, as a player's own click goes.
                if let Some(c) = placing_ray(game, eye, next.unwrap_or(now), reach) {
                    placed = click(game, n, c, slot, render);
                }
            }
            "Off" => {
                if let Some(p) = found {
                    placed = click(game, n, p.click, slot, render);
                } else if m.b("Air place")
                    && let Some(&cell) = n.cells.list().first()
                {
                    // Nothing to place against: the cell itself, clicked on its top.
                    let r = [n.random(tick), n.random(tick + 1)];
                    let point = [f64::from(cell[0]) + 0.3 + 0.4 * r[0], f64::from(cell[1]) + 1.0, f64::from(cell[2]) + 0.3 + 0.4 * r[1]];
                    placed = click(game, n, Click { pos: cell, face: [0, 1, 0], point }, slot, render);
                }
            }
            _ => {}
        }
    }
    // Late click: Strict's look missed and this move starts off the edge, the last one before a fall. The click goes
    // with the look this tick's move packet carries, as Ray cast Normal's does.
    if wait.is_none()
        && placed.is_none()
        && rotate
        && ray_cast == "Strict"
        && m.b("Late click")
        && ground
        && moving
        && !stands(game, pos[0], pos[2], y)
        && let Some(p) = plan(game, n, &Aim { eye, ..aim }, true, m.n("Closest block range"), tick, MAX_RAYS)
    {
        let look = turn(now, p.look, n.speed, degrees);
        if let Some(c) = placing_ray(game, eye, look, reach) {
            placed = click(game, n, c, slot, render);
            if placed.is_some() {
                next = Some(look);
                n.stats.late += 1;
            }
        }
    }
    if placed.is_none() {
        n.stats.state = wait.unwrap_or(if found.is_some() { "turning to the side" } else { "no side to place against" });
    }

    // The look, and the move going by it.
    n.visible = visible;
    n.correction = correction;
    n.swap_back = m.b("Swap back");
    match next {
        Some(look) if visible => {
            super::sunset::scaffold_view(game);
            (game.yaw, game.pitch) = look;
            n.look = Some(look);
        }
        Some(look) => {
            n.look = Some(look);
            super::sunset_rotation::scaffold_look(Some(look));
            correct(game, n, look, camera.0);
        }
        None => {
            n.look = None;
            super::sunset_rotation::scaffold_look(None);
        }
    }
    // Sprint: Keep leaves your sprint be, Off stops it, Vanilla allows it only when vanilla would for the look the
    // server has: your move at least 0.8 forward along it (a silent look behind you allows none).
    let sent_yaw = match next { Some(look) if !visible => look.0, _ => camera.0 };
    let held = |k: KeyCode| f32::from(u8::from(game.keys.contains(&k)));
    let input = (held(KeyCode::KeyW) - held(KeyCode::KeyS), held(KeyCode::KeyA) - held(KeyCode::KeyD));
    let no_sprint = match m.p("Sprint") {
        "Off" => true,
        "Vanilla" => !sprint_allowed(input, camera.0, sent_yaw),
        _ => false,
    };
    horizon_client::motion::sunset_motion::set_no_sprint(no_sprint);

    // Ledge sneak: on the ground, walking where a block is still wanted and none went this tick, sneak (the game then
    // keeps you on the edge, and the server is told you sneak).
    let next_cell = [(pos[0] + step[0]).floor() as i32, y, (pos[2] + step[1]).floor() as i32];
    let jumping = m.p("Same Y") == "Auto jump" || telly;
    let danger = m.b("Ledge sneak") && !jumping && ground && moving && !space && placed.is_none() && n.cells.has(next_cell);
    if danger != n.sneaking {
        legit_sneak(game, danger);
        n.sneaking = danger;
    }
    // Same Y's Auto jump and Telly jump for you while you walk on the ground.
    if jumping && ground && moving && !space && game.keys.insert(KeyCode::Space) {
        n.jumped = true;
    }
}

// Movement correction while the move packets carry `look`: the move goes by its yaw (Strict), the keys turned first
// so they still walk your way (Silent).
fn correct(game: &mut Game<'_>, n: &mut Normal, look: (f32, f32), view: f32) {
    if n.correction == Correction::Off {
        return;
    }
    if n.correction == Correction::Silent {
        turn_keys(game, n, view, look.0);
    }
    horizon_client::motion::sunset_motion::set_move_yaw(Some(look.0));
}

// Scaffold off, paused or with nothing to do: the sneak and slot back, and the look back to your camera, then let go.
fn release(game: &mut Game<'_>) {
    with(|n| {
        n.cells = Cells::default();
        (n.row, n.idle, n.air) = (None, 0, 0);
        if n.sneaking {
            legit_sneak(game, false);
            n.sneaking = false;
        }
        // The slot goes back, but not in the tick after a place (NoCheatPlus's ToolSwitch), nor if you changed it.
        if n.slot_before.is_some() && n.last_place.is_none_or(|t| game.ticks >= t + 2) {
            if let Some(before) = n.slot_before.take()
                && n.swap_back
                && n.slot_used == Some(game.selected)
                && game.player.is_some()
            {
                game.selected = before;
            }
            n.slot_used = None;
        }
        let Some(look) = n.look else {
            return;
        };
        let camera = (game.yaw, game.pitch);
        if n.visible || game.player.is_none() {
            n.look = None;
            super::sunset_rotation::scaffold_look(None);
            return;
        }
        let degrees = degrees_per_count(f64::from(game.settings.sensitivity));
        let back = turn_back(look, camera, n.speed.max(30.0), degrees);
        if (back.0 - camera.0).abs() <= 1.0 && (back.1 - camera.1).abs() <= 1.0 {
            n.look = None;
            super::sunset_rotation::scaffold_look(None);
            return;
        }
        n.look = Some(back);
        super::sunset_rotation::scaffold_look(Some(back));
        correct(game, n, back, camera.0);
    });
}

// What Fast tower does this tick: lift at Tower speed, settle on the block's top under a ceiling, or nothing. Both
// need a block under you: after a missed placement the lift stops, you come down onto the last block and the next
// placement is tried from there, instead of rising (or being told on the ground) over nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tower {
    Lift,
    Cap,
    Wait,
}

pub(super) fn tower_action(room: bool, supported: bool) -> Tower {
    match (supported, room) {
        (false, _) => Tower::Wait,
        (true, true) => Tower::Lift,
        (true, false) => Tower::Cap,
    }
}

// Fast tower (Sunset's): while you hold jump (and stand still, or While moving), lifted at Tower speed while there is
// room above, set on the block's top under a ceiling (tower_action).
fn fast_tower(game: &mut Game<'_>, m: &Module) {
    let moving = move_input(game) != (0.0, 0.0);
    let (space, sneak) = (game.keys.contains(&KeyCode::Space), game.keys.contains(&KeyCode::ShiftLeft));
    if !m.b("Fast tower") || !space || sneak || (moving && !m.b("While moving")) {
        return;
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let feet = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    let room = (0..2).all(|dy| util::is_air(game, [feet[0], feet[1] + 2 + dy, feet[2]]));
    let supported = !util::is_air(game, [feet[0], feet[1] - 1, feet[2]]);
    let Some(p) = game.player.as_mut() else {
        return;
    };
    match tower_action(room, supported) {
        Tower::Lift => p.velocity.y = m.n("Tower speed"),
        Tower::Cap => {
            p.velocity.y = p.position.y.ceil() - p.position.y;
            p.ground.on_ground = true;
        }
        Tower::Wait => {}
    }
}

// After the game's tick: the forced block, the jump, the right click and the move keys Normal gave go back, and the
// click's block is checked (the game predicts it at once; none means the game refused it). Only the window's game:
// single player's own server ticks one too, and what Normal gave is the window's.
pub(super) fn post_tick(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    let (forced, pressed, render, jumped, right, keys) = with(|n| {
        (
            std::mem::take(&mut n.forced),
            n.pressed.take(),
            n.render,
            std::mem::take(&mut n.jumped),
            std::mem::take(&mut n.right_taken),
            n.keys_before.take(),
        )
    });
    if forced {
        game.forced_hit = None;
    }
    if let Some(cell) = pressed {
        let set = !util::replaceable(game, cell);
        let tick = game.ticks;
        with(|n| {
            if set {
                n.stats.predicted += 1;
                n.watch(cell, tick);
            } else {
                n.stats.refused += 1;
                n.stats.state = "the game refused a click";
                n.fail(cell, tick);
            }
        });
        if set && render {
            super::sunset::scaffold_drawn(cell);
        }
    }
    if jumped && !game.physical_keys.contains(&KeyCode::Space) {
        game.keys.remove(&KeyCode::Space);
    }
    if right {
        game.right = game.physical_buttons.contains(&MouseButton::Right);
    }
    if let Some(held) = keys {
        for (k, on) in [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD].into_iter().zip(held) {
            if on {
                game.keys.insert(k);
            } else {
                game.keys.remove(&k);
            }
        }
    }
}

// A server correction (a setback or teleport): the server set the look, so Normal starts again from yours.
pub(super) fn correction() {
    with(|n| {
        (n.look, n.row, n.last_move) = (None, None, None);
        n.watch = Default::default();
    });
    super::sunset_rotation::scaffold_look(None);
}

// Panic or no world: everything back at once (the look too, without turning).
pub(super) fn stop(game: &mut Game<'_>) {
    post_tick(game);
    with(|n| {
        if let Some(before) = n.slot_before.take()
            && n.swap_back
            && n.slot_used == Some(game.selected)
            && game.player.is_some()
        {
            game.selected = before;
        }
        if n.sneaking {
            legit_sneak(game, false);
        }
        *n = Normal { rng: n.rng, stats: n.stats, choices: std::mem::take(&mut n.choices), ..Normal::default() };
    });
    super::sunset_rotation::scaffold_look(None);
    horizon_client::motion::sunset_motion::set_move_yaw(None);
    horizon_client::motion::sunset_motion::set_no_sprint(false);
}

// Debug Report's lines for Normal, once it has clicked.
pub(super) fn report(out: &mut String) {
    let s = with(|n| n.stats);
    if s.clicks == 0 {
        return;
    }
    let _ = writeln!(out, "\n## Scaffold (Normal)");
    let _ = writeln!(
        out,
        "- Clicks: {} (late clicks: {}); blocks the game predicted: {}; refused by the game: {}; taken back by the server: {}; cells set to wait: {}",
        s.clicks, s.late, s.predicted, s.refused, s.taken_back, s.cooled
    );
    let _ = writeln!(out, "- Last state: {}", if s.state.is_empty() { "-" } else { s.state });
}

// Whether vanilla lets you sprint moving by `input` (forward, left: -1 to 1 each, as the keys give it) with your view
// at `view` while the server has your look at `sent` (yaw degrees): the keys' move, seen from the sent look, needs a
// forward impulse of 0.8 or more (vanilla's classic threshold to sprint; W with A is 1 when the looks agree).
pub(super) fn sprint_allowed(input: (f32, f32), view: f32, sent: f32) -> bool {
    let (forward, left) = input;
    if forward == 0.0 && left == 0.0 {
        return false;
    }
    // The way you move, as a yaw: your view turned left by the keys' angle.
    let moving = view - left.atan2(forward).to_degrees();
    forward.hypot(left) * (moving - sent).to_radians().cos() >= 0.8
}

#[cfg(test)]
mod tests {
    #[test]
    fn fast_tower_needs_a_block_under_you() {
        use super::{Tower, tower_action};
        assert_eq!(tower_action(true, true), Tower::Lift);
        assert_eq!(tower_action(false, true), Tower::Cap);
        assert_eq!(tower_action(true, false), Tower::Wait, "a missed block: no lift over nothing");
        assert_eq!(tower_action(false, false), Tower::Wait, "nor on the ground over nothing");
    }

    #[test]
    fn vanilla_sprint_follows_the_sent_look() {
        use super::sprint_allowed;
        assert!(sprint_allowed((1.0, 0.0), 30.0, 30.0), "forward along the look");
        assert!(!sprint_allowed((1.0, 0.0), 30.0, 210.0), "the server sees you walking backwards");
        assert!(!sprint_allowed((1.0, 0.0), 0.0, 60.0), "60 degrees off is under 0.8 forward");
        assert!(sprint_allowed((1.0, 0.0), 0.0, 30.0));
        // W with A: forward 1 while the looks agree, as in vanilla; A alone is no forward impulse.
        assert!(sprint_allowed((1.0, 1.0), 0.0, 0.0));
        assert!(sprint_allowed((1.0, 1.0), 0.0, -45.0));
        assert!(!sprint_allowed((0.0, 1.0), 0.0, 0.0));
        assert!(!sprint_allowed((1.0, 1.0), 0.0, 90.0), "the look turned right: you go sideways to it");
        assert!(!sprint_allowed((0.0, 0.0), 0.0, 0.0), "standing still");
    }

    use super::*;

    #[test]
    fn a_diagonal_line_is_a_staircase_of_cells_beside_each_other() {
        let mut cells = Vec::new();
        line_cells([0.5, 0.5], [3.5, 2.5], |c| {
            cells.push(c);
            true
        });
        assert_eq!(cells.first(), Some(&[0, 0]));
        assert_eq!(cells.last(), Some(&[3, 2]));
        assert_eq!(cells.len(), 6);
        for w in cells.windows(2) {
            assert_eq!((w[0][0] - w[1][0]).abs() + (w[0][1] - w[1][1]).abs(), 1, "{cells:?}");
        }
        // Straight through a corner, both ways, and a line that stays in its cell.
        let mut through = Vec::new();
        line_cells([0.5, 0.5], [1.5, 1.5], |c| {
            through.push(c);
            true
        });
        assert_eq!(through.len(), 3);
        let mut back = Vec::new();
        line_cells([-0.2, 4.7], [-1.9, 3.1], |c| {
            back.push(c);
            true
        });
        assert_eq!(back, [[-1, 4], [-1, 3], [-2, 3]].to_vec());
        let mut still = 0;
        line_cells([2.3, 2.3], [2.3, 2.3], |_| {
            still += 1;
            true
        });
        assert_eq!(still, 1);
        let mut first = 0;
        line_cells([0.5, 0.5], [9.5, 0.5], |_| {
            first += 1;
            false
        });
        assert_eq!(first, 1);
    }

    #[test]
    fn silent_correction_keeps_your_way_within_half_a_step() {
        // W at your camera, the look straight back: S takes you the same way.
        assert_eq!(turned_keys(1, 0, 0.0, 180.0), (-1, 0));
        // W, the look 90 to the right (yaw +90): walking left of it (A) goes your way.
        assert_eq!(turned_keys(1, 0, 0.0, 90.0), (0, 1));
        // Off by 10: still W; off by 30: W and A (15 off) beat W alone (30 off).
        assert_eq!(turned_keys(1, 0, 0.0, 10.0), (1, 0));
        assert_eq!(turned_keys(1, 0, 0.0, 30.0), (1, 1));
        // Exactly between two ways: the single key.
        assert_eq!(turned_keys(1, 0, 0.0, 22.5), (1, 0));
        // S and D at your camera, the look 135 to the left.
        let way = walk_yaw(0.0, (-1.0, -1.0));
        let (f, l) = turned_keys(-1, -1, 0.0, -135.0);
        assert!(wrap(walk_yaw(-135.0, (f64::from(f), f64::from(l))) - way).abs() <= 22.5 + 1e-3);
        assert_eq!(turned_keys(0, 0, 0.0, 120.0), (0, 0));
        for look in (0..72).map(|i| i as f32 * 5.0) {
            for (f, l) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
                let (tf, tl) = turned_keys(f, l, 30.0, look);
                let off = wrap(walk_yaw(look, (f64::from(tf), f64::from(tl))) - walk_yaw(30.0, (f64::from(f), f64::from(l))));
                assert!(off.abs() <= 22.5 + 1e-3, "{look} {f} {l}: {off}");
            }
        }
    }

    #[test]
    fn the_walk_yaw_follows_the_game_s_move_directions() {
        // Yaw 0 walks +z; A (left) walks +x at yaw 0, which is yaw -90; S walks back.
        assert_eq!(walk_yaw(0.0, (1.0, 0.0)), 0.0);
        assert_eq!(walk_yaw(0.0, (0.0, 1.0)), -90.0);
        assert_eq!(walk_yaw(0.0, (0.0, -1.0)), 90.0);
        assert_eq!(wrap(walk_yaw(10.0, (-1.0, 0.0))), -170.0);
        assert_eq!(off_eight(100.0, 10.0), 0.0);
        assert_eq!(off_eight(30.0, 10.0), 20.0);
        assert_eq!(off_eight(-35.0, 10.0), 0.0);
    }

    #[test]
    fn a_side_is_seen_only_from_outside_and_kept_in_from_its_edges() {
        let block = [5.0, 64.0, 6.0, 6.0, 65.0, 7.0];
        let south = Side::of(block, [0, 0, 1]);
        assert_eq!((south.axis, south.plane), (2, 7.0));
        assert!((south.lo[0] - 5.12).abs() < 1e-9 && (south.hi[1] - 64.88).abs() < 1e-9);
        assert!(south.faces([5.5, 66.0, 7.3], [0, 0, 1]));
        assert!(!south.faces([5.5, 66.0, 6.9], [0, 0, 1]));
        let top = Side::of(block, [0, 1, 0]);
        assert_eq!((top.axis, top.plane), (1, 65.0));
        assert!(top.faces([5.5, 66.6, 6.5], [0, 1, 0]));
        // A bottom slab's top is half way up.
        let slab = Side::of([0.0, 0.0, 0.0, 1.0, 0.5, 1.0], [0, 1, 0]);
        assert_eq!(slab.plane, 0.5);
        assert_eq!(top.clamp([9.0, 0.0, 6.5]), [5.88, 65.0, 6.5]);
        assert_eq!(top.grid().count(), 9);
        assert!(top.grid().all(|p| p[1] == 65.0 && (5.12..=5.88).contains(&p[0])));
        assert_eq!(box_distance([5.5, 67.0, 6.5], block), 2.0);
        assert_eq!(box_distance([5.5, 64.5, 6.5], block), 0.0);
    }

    #[test]
    fn a_pitch_from_the_window_meets_the_side_at_that_yaw() {
        // Past the edge of a block's south side, looking back north (yaw 180) and down.
        let block = [0.0, 63.0, 0.0, 1.0, 64.0, 1.0];
        let side = Side::of(block, [0, 0, 1]);
        let eye = [0.5, 65.62, 1.2];
        let (least, most) = side.pitches(eye, 180.0).expect("the side is in front");
        assert!(least < most && least > 70.0 && most < 90.0, "{least} {most}");
        for pitch in [least + 0.01, (least + most) / 2.0, most - 0.01] {
            let p = side.crossing(eye, direction((180.0, pitch))).expect("ahead");
            assert!(p[1] >= side.lo[1] - 1e-6 && p[1] <= side.hi[1] + 1e-6, "{pitch}: {p:?}");
        }
        // Facing away from the side: none.
        assert!(side.pitches(eye, 0.0).is_none());
        // The top of the block below, from above it: straight down meets it, and so do pitches near 90.
        let top = Side::of([0.0, 63.0, 0.0, 1.0, 64.0, 1.0], [0, 1, 0]);
        let (least, most) = top.pitches([0.5, 65.62, 0.5], 90.0).expect("below");
        assert!(most > 89.9 && least > 60.0 && least < 80.0, "{least} {most}");
        let p = top.crossing([0.5, 65.62, 0.5], direction((90.0, least + 0.01))).expect("ahead");
        assert!(p[0] >= top.lo[0] - 1e-6, "{p:?}");
    }

    #[test]
    fn the_next_step_is_the_kept_speed_plus_the_last_push() {
        // Walking from a stand on the ground: each move goes the kept speed plus the keys' push, then friction keeps
        // 0.546 of it.
        let (push, friction) = (0.098, 0.546);
        let (mut pos, mut vel) = ([0.5, 64.0, 0.5], [0.0; 3]);
        let mut last = None;
        for tick in 1..12u64 {
            let step = next_step(last, tick, pos, vel);
            let moved = vel[2] + push;
            if tick > 1 {
                assert!((step[1] - moved).abs() < 1e-9 && step[0].abs() < 1e-9, "tick {tick}: {step:?} for {moved}");
            }
            last = Some((tick, pos, vel));
            pos[2] += moved;
            vel[2] = moved * friction;
        }
        // Near the walking speed by then, about twice the kept speed.
        assert!((vel[2] / friction - 0.2158).abs() < 0.002, "{}", vel[2] / friction);
        // No last tick, or a jolt since: the kept speed alone.
        assert_eq!(next_step(None, 5, pos, vel), [vel[0], vel[2]]);
        assert_eq!(next_step(Some((4, [pos[0] + 3.0, pos[1], pos[2]], vel)), 5, pos, vel), [vel[0], vel[2]]);
        assert_eq!(next_step(Some((3, pos, vel)), 5, pos, vel), [vel[0], vel[2]]);
    }

    #[test]
    fn a_look_aimed_one_step_ahead_still_meets_the_side_a_little_off_it() {
        // Walking back past a block's south side: the look picked for the eyes 0.2 past it is clicked from where the
        // move really took them.
        let side = Side::of([0.0, 63.0, 0.0, 1.0, 64.0, 1.0], [0, 0, 1]);
        let (least, most) = side.pitches([0.5, 65.62, 1.2], 180.0).expect("in front");
        let q = (most - least) / 4.0;
        for now in [0.0, 85.0, 90.0] {
            let pitch = f32::clamp(now, least + q, most - q);
            for off in [-0.01, 0.0, 0.01] {
                let (lo, hi) = side.pitches([0.5, 65.62, 1.2 + off], 180.0).expect("in front");
                assert!(pitch >= lo && pitch <= hi, "from {now}: {pitch} outside {lo}..{hi} at {off}");
            }
        }
    }

    #[test]
    fn the_look_turns_back_the_way_that_unwinds_it() {
        let degrees = degrees_per_count(0.5);
        // Held at 190 past a camera at 0: back the long way to 0, never on to 360.
        let mut look = (190.0, 80.0);
        for _ in 0..20 {
            look = turn_back(look, (0.0, 20.0), 40.0, degrees);
        }
        assert!(look.0.abs() <= degrees && (look.1 - 20.0).abs() <= degrees, "{look:?}");
        // A turn toward a target goes the short way and stops on it.
        let next = turn((170.0, 0.0), (-170.0, 0.0), 60.0, degrees);
        assert!((next.0 - 190.0).abs() <= degrees, "{next:?}");
        assert_eq!(under_y(65.0), 64);
        assert_eq!(under_y(64.5), 64);
    }

    #[test]
    fn a_cell_that_fails_twice_waits() {
        let mut n = Normal::default();
        let cell = [1, 64, 2];
        n.fail(cell, 100);
        assert!(!n.cooled(cell, 101));
        n.fail(cell, 102);
        assert!(n.cooled(cell, 103) && n.cooled(cell, 102 + COOL_TICKS - 1));
        assert!(!n.cooled(cell, 102 + COOL_TICKS));
        assert_eq!(n.stats.cooled, 1);
        // Many cells: the table keeps the latest, without growing.
        for i in 0..20 {
            n.fail([i, 0, 0], 200 + i as u64);
        }
        assert_eq!(n.failed.iter().flatten().count(), n.failed.len());
        let mut cells = Cells::default();
        for i in 0..40 {
            cells.push([i, 0, 0]);
            cells.push([i, 0, 0]);
        }
        assert_eq!(cells.len, MAX_CELLS);
        assert!(cells.has([3, 0, 0]) && !cells.has([30, 0, 0]));
        n.recent = [0; 20];
        n.recent[0] = 90;
        n.recent[1] = 95;
        assert_eq!(n.places_lately(100), 2);
        assert_eq!(n.places_lately(112), 1);
    }
}
