// baritone.api.pathing.goals: where a path is going. A goal says when the search is done and how far
// from done a position is. The heuristic must not be more than the real cost left, or the first
// path found is not the cheapest one. Every formula and every name here is the Java's.
use super::baritone_costs::{fall_cost, jump_one_block_cost};
use super::baritone_utils::{maybe_censor, Pos};
use std::cell::Cell;
use std::rc::Rc;

thread_local! {
    /// `Settings.costHeuristic`, which the goals read whenever they work out a distance. Core keeps it in step
    /// with the setting.
    static COST_HEURISTIC: Cell<f64> = const { Cell::new(3.563) };
}

pub fn set_cost_heuristic(value: f64) {
    COST_HEURISTIC.with(|c| c.set(value));
}

fn cost_heuristic() -> f64 {
    COST_HEURISTIC.with(Cell::get)
}

pub trait Goal {
    fn is_in_goal(&self, p: Pos) -> bool;
    /// An estimate of the ticks from `p` to the goal.
    fn heuristic(&self, p: Pos) -> f64;
    /// `heuristic()` with no position: what the estimate is at the goal itself, which `#eta` needs.
    fn heuristic0(&self) -> f64 {
        0.0
    }
    /// A short description for chat (toString).
    fn label(&self) -> String;
    /// The block drawn for this goal, if it has one.
    fn render_pos(&self) -> Option<Pos> {
        None
    }
    /// Where #elytra flies for this goal: a GoalBlock's block, a GoalXZ's column at y 64; no other goal can be flown to.
    fn elytra_target(&self) -> Option<Pos> {
        None
    }
    /// The goal this one turns around, if it is an inversion.
    fn inverted_origin(&self) -> Option<GoalRef> {
        None
    }
    /// How PathRenderer draws it.
    fn render_kind(&self) -> GoalRender {
        match self.render_pos() {
            Some(pos) => GoalRender::Pos { pos, halved: false },
            None => GoalRender::Nothing,
        }
    }
}

/// What PathRenderer does for a goal, in the order of its instanceof ladder.
pub enum GoalRender {
    /// `IGoalRenderPos`: the box at a block; GoalGetToBlock and GoalTwoBlocks draw it half as tall (`halved`).
    Pos { pos: Pos, halved: bool },
    /// `GoalXZ`.
    Xz(i32, i32),
    /// `GoalComposite`: each of its goals.
    Composite(Vec<GoalRef>),
    /// `GoalInverted`: its origin, in the inverted color.
    Inverted(GoalRef),
    /// `GoalYLevel`.
    YLevel(i32),
    /// Every other goal draws nothing.
    Nothing,
}

pub type GoalRef = Rc<dyn Goal>;

/// `GoalXZ.calculate`: the cost of covering (x, z) blocks on the flat, diagonals first, then straight.
pub fn xz_heuristic(dx: f64, dz: f64) -> f64 {
    let (x, z) = (dx.abs(), dz.abs());
    let (straight, diagonal) = if x < z { (z - x, x) } else { (x - z, z) };
    (diagonal * std::f64::consts::SQRT_2 + straight) * cost_heuristic()
}

/// `GoalYLevel.calculate`: the cost of changing height from `current` to `goal`.
pub fn y_heuristic(goal: i32, current: i32) -> f64 {
    if current > goal {
        // Need to descend.
        fall_cost(2) / 2.0 * f64::from(current - goal)
    } else if current < goal {
        // Need to ascend.
        f64::from(goal - current) * jump_one_block_cost()
    } else {
        0.0
    }
}

/// `GoalBlock.calculate`: `yd` is how far the position is above the goal's height.
pub fn block_heuristic(dx: i32, yd: i32, dz: i32) -> f64 {
    y_heuristic(0, yd) + xz_heuristic(f64::from(dx), f64::from(dz))
}

/// `yDiff < 0 ? yDiff + 1 : yDiff`, which GoalGetToBlock and GoalTwoBlocks use: the head being in the block is as
/// good as the feet.
fn head_shift(y_diff: i32) -> i32 {
    if y_diff < 0 { y_diff + 1 } else { y_diff }
}

/// One block.
pub struct GoalBlock(pub Pos);

impl Goal for GoalBlock {
    fn is_in_goal(&self, p: Pos) -> bool {
        p == self.0
    }
    fn heuristic(&self, p: Pos) -> f64 {
        block_heuristic(p[0] - self.0[0], p[1] - self.0[1], p[2] - self.0[2])
    }
    fn label(&self) -> String {
        format!("GoalBlock{{x={},y={},z={}}}", maybe_censor(self.0[0]), maybe_censor(self.0[1]), maybe_censor(self.0[2]))
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
    fn elytra_target(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// A column, any height.
pub struct GoalXZ(pub i32, pub i32);

impl GoalXZ {
    /// `GoalXZ.fromDirection`: the column `distance` blocks from `origin` along a yaw.
    pub fn from_direction(origin: [f64; 3], yaw: f32, distance: f64) -> GoalXZ {
        let theta = f64::from(yaw.to_radians());
        let x = origin[0] - theta.sin() * distance;
        let z = origin[2] + theta.cos() * distance;
        GoalXZ(x.floor() as i32, z.floor() as i32)
    }
}

impl Goal for GoalXZ {
    fn is_in_goal(&self, p: Pos) -> bool {
        p[0] == self.0 && p[2] == self.1
    }
    fn heuristic(&self, p: Pos) -> f64 {
        xz_heuristic(f64::from(p[0] - self.0), f64::from(p[2] - self.1))
    }
    fn label(&self) -> String {
        format!("GoalXZ{{x={},z={}}}", maybe_censor(self.0), maybe_censor(self.1))
    }
    fn elytra_target(&self) -> Option<Pos> {
        // ElytraBehavior changes the destination height by itself, depending on whether the player is above or below the roof.
        Some([self.0, 64, self.1])
    }
    fn render_kind(&self) -> GoalRender {
        GoalRender::Xz(self.0, self.1)
    }
}

/// A height, anywhere.
pub struct GoalYLevel(pub i32);

impl Goal for GoalYLevel {
    fn is_in_goal(&self, p: Pos) -> bool {
        p[1] == self.0
    }
    fn heuristic(&self, p: Pos) -> f64 {
        y_heuristic(self.0, p[1])
    }
    fn label(&self) -> String {
        format!("GoalYLevel{{y={}}}", maybe_censor(self.0))
    }
    fn render_kind(&self) -> GoalRender {
        GoalRender::YLevel(self.0)
    }
}

/// The shared `heuristic()` of GoalNear and GoalRunAway: the dearest estimate that is still inside the goal, over
/// a box around it, provided it is below every estimate outside it (see Baritone pull request 1978).
fn heuristic_at_goal(goal: &dyn Goal, min: Pos, max: Pos) -> f64 {
    let mut maybe_always_inside: Vec<f64> = Vec::new();
    let mut min_outside = f64::INFINITY;
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                let h = goal.heuristic([x, y, z]);
                if h < min_outside && goal.is_in_goal([x, y, z]) {
                    maybe_always_inside.push(h);
                } else {
                    min_outside = min_outside.min(h);
                }
            }
        }
    }
    let mut max_inside = f64::NEG_INFINITY;
    for inside in maybe_always_inside {
        if inside < min_outside {
            max_inside = max_inside.max(inside);
        }
    }
    max_inside
}

/// Within a sphere of `range` blocks around a position.
pub struct GoalNear(pub Pos, pub i32);

impl Goal for GoalNear {
    fn is_in_goal(&self, p: Pos) -> bool {
        let d = [p[0] - self.0[0], p[1] - self.0[1], p[2] - self.0[2]];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= self.1 * self.1
    }
    fn heuristic(&self, p: Pos) -> f64 {
        block_heuristic(p[0] - self.0[0], p[1] - self.0[1], p[2] - self.0[2])
    }
    fn heuristic0(&self) -> f64 {
        let range = f64::from(self.1 * self.1).sqrt().ceil() as i32;
        let [x, y, z] = self.0;
        heuristic_at_goal(self, [x - range, y - range, z - range], [x + range, y + range, z + range])
    }
    fn label(&self) -> String {
        format!("GoalNear{{x={}, y={}, z={}, rangeSq={}}}", maybe_censor(self.0[0]), maybe_censor(self.0[1]), maybe_censor(self.0[2]), self.1 * self.1)
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// Next to a block, or in its column: where you stand to use or break it.
pub struct GoalGetToBlock(pub Pos);

impl Goal for GoalGetToBlock {
    fn is_in_goal(&self, p: Pos) -> bool {
        let (dx, dy, dz) = (p[0] - self.0[0], p[1] - self.0[1], p[2] - self.0[2]);
        dx.abs() + head_shift(dy).abs() + dz.abs() <= 1
    }
    fn heuristic(&self, p: Pos) -> f64 {
        block_heuristic(p[0] - self.0[0], head_shift(p[1] - self.0[1]), p[2] - self.0[2])
    }
    fn label(&self) -> String {
        format!("GoalGetToBlock{{x={},y={},z={}}}", maybe_censor(self.0[0]), maybe_censor(self.0[1]), maybe_censor(self.0[2]))
    }
    fn render_kind(&self) -> GoalRender {
        GoalRender::Pos { pos: self.0, halved: true }
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// A two high column: stand at the block or the one under it, to break something above your head.
pub struct GoalTwoBlocks(pub Pos);

impl Goal for GoalTwoBlocks {
    fn is_in_goal(&self, p: Pos) -> bool {
        p[0] == self.0[0] && p[2] == self.0[2] && (p[1] == self.0[1] || p[1] == self.0[1] - 1)
    }
    fn heuristic(&self, p: Pos) -> f64 {
        block_heuristic(p[0] - self.0[0], head_shift(p[1] - self.0[1]), p[2] - self.0[2])
    }
    fn label(&self) -> String {
        format!("GoalTwoBlocks{{x={},y={},z={}}}", maybe_censor(self.0[0]), maybe_censor(self.0[1]), maybe_censor(self.0[2]))
    }
    fn render_kind(&self) -> GoalRender {
        GoalRender::Pos { pos: self.0, halved: true }
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// MineProcess.GoalThreeBlocks: the block, the one under it, or the one two under it.
pub struct GoalThreeBlocks(pub Pos);

impl Goal for GoalThreeBlocks {
    fn is_in_goal(&self, p: Pos) -> bool {
        p[0] == self.0[0] && p[2] == self.0[2] && (self.0[1] - 2..=self.0[1]).contains(&p[1])
    }
    fn heuristic(&self, p: Pos) -> f64 {
        let y_diff = p[1] - self.0[1];
        let shifted = if y_diff < -1 {
            y_diff + 2
        } else if y_diff == -1 {
            0
        } else {
            y_diff
        };
        block_heuristic(p[0] - self.0[0], shifted, p[2] - self.0[2])
    }
    fn label(&self) -> String {
        format!("GoalThreeBlocks{{x={},y={},z={}}}", self.0[0], self.0[1], self.0[2])
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// BuilderProcess.GoalPlace: stand on top of the block to place.
pub struct GoalPlace(pub Pos);

impl GoalPlace {
    pub fn new(place_at: Pos) -> Self {
        Self([place_at[0], place_at[1] + 1, place_at[2]])
    }
}

impl Goal for GoalPlace {
    fn is_in_goal(&self, p: Pos) -> bool {
        p == self.0
    }
    fn heuristic(&self, p: Pos) -> f64 {
        // Prioritize lower y coordinates.
        f64::from(self.0[1]) * 100.0 + block_heuristic(p[0] - self.0[0], p[1] - self.0[1], p[2] - self.0[2])
    }
    fn label(&self) -> String {
        format!("GoalPlace{{x={},y={},z={}}}", self.0[0], self.0[1], self.0[2])
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// BuilderProcess.GoalAdjacent: next to the block to place, but not on the side `no` and not in the block itself.
pub struct GoalAdjacent {
    pos: Pos,
    no: Pos,
    allow_same_level: bool,
}

impl GoalAdjacent {
    pub fn new(pos: Pos, no: Pos, allow_same_level: bool) -> Self {
        Self { pos, no, allow_same_level }
    }
}

impl Goal for GoalAdjacent {
    fn is_in_goal(&self, p: Pos) -> bool {
        if p == self.pos || p == self.no {
            return false;
        }
        if !self.allow_same_level && p[1] == self.pos[1] - 1 {
            return false;
        }
        if p[1] < self.pos[1] - 1 {
            return false;
        }
        GoalGetToBlock(self.pos).is_in_goal(p)
    }
    fn heuristic(&self, p: Pos) -> f64 {
        // Prioritize lower y coordinates.
        f64::from(self.pos[1]) * 100.0 + GoalGetToBlock(self.pos).heuristic(p)
    }
    fn label(&self) -> String {
        format!("GoalAdjacent{{x={},y={},z={}}}", self.pos[0], self.pos[1], self.pos[2])
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.pos)
    }
}

/// BuilderProcess.GoalBreak: anywhere to break a block from but right on top of it.
pub struct GoalBreak(pub Pos);

impl Goal for GoalBreak {
    fn is_in_goal(&self, p: Pos) -> bool {
        // Can't stand right on top of a block, that might not work (what if it's unsupported, can't break then),
        // but any other adjacent works for breaking, including inside or below.
        if p[1] > self.0[1] {
            return false;
        }
        GoalGetToBlock(self.0).is_in_goal(p)
    }
    fn heuristic(&self, p: Pos) -> f64 {
        GoalGetToBlock(self.0).heuristic(p)
    }
    fn label(&self) -> String {
        format!("GoalBreak{{x={},y={},z={}}}", self.0[0], self.0[1], self.0[2])
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// The GoalGetToBlock over a block to break from above: it may not be in the block itself, nor above it.
pub struct GoalBreakFromAbove(pub Pos);

impl Goal for GoalBreakFromAbove {
    fn is_in_goal(&self, p: Pos) -> bool {
        if p[1] > self.0[1] || p == self.0 {
            return false;
        }
        GoalGetToBlock(self.0).is_in_goal(p)
    }
    fn heuristic(&self, p: Pos) -> f64 {
        GoalGetToBlock(self.0).heuristic(p)
    }
    fn label(&self) -> String {
        GoalGetToBlock(self.0).label()
    }
    fn render_pos(&self) -> Option<Pos> {
        Some(self.0)
    }
}

/// BuilderProcess.JankyGoalComposite: done at either, but how far from done is the primary's business.
pub struct JankyGoalComposite {
    pub primary: GoalRef,
    pub fallback: GoalRef,
}

impl Goal for JankyGoalComposite {
    fn is_in_goal(&self, p: Pos) -> bool {
        self.primary.is_in_goal(p) || self.fallback.is_in_goal(p)
    }
    fn heuristic(&self, p: Pos) -> f64 {
        self.primary.heuristic(p)
    }
    fn label(&self) -> String {
        format!("JankyComposite Primary: {} Fallback: {}", self.primary.label(), self.fallback.label())
    }
    fn render_pos(&self) -> Option<Pos> {
        self.primary.render_pos()
    }
}

/// Any of several goals: done at the first, as near as the nearest.
pub struct GoalComposite(pub Vec<GoalRef>);

impl Goal for GoalComposite {
    fn is_in_goal(&self, p: Pos) -> bool {
        self.0.iter().any(|g| g.is_in_goal(p))
    }
    fn heuristic(&self, p: Pos) -> f64 {
        self.0.iter().map(|g| g.heuristic(p)).fold(f64::MAX, f64::min)
    }
    fn heuristic0(&self) -> f64 {
        self.0.iter().map(|g| g.heuristic0()).fold(f64::MAX, f64::min)
    }
    fn label(&self) -> String {
        format!("GoalComposite[{}]", self.0.iter().map(|g| g.label()).collect::<Vec<_>>().join(", "))
    }
    fn render_pos(&self) -> Option<Pos> {
        self.0.first().and_then(|g| g.render_pos())
    }
    fn render_kind(&self) -> GoalRender {
        GoalRender::Composite(self.0.clone())
    }
}

/// Anywhere but the goal: the heuristic climbs as the position nears it.
pub struct GoalInverted(pub GoalRef);

impl Goal for GoalInverted {
    fn is_in_goal(&self, _p: Pos) -> bool {
        false
    }
    fn heuristic(&self, p: Pos) -> f64 {
        -self.0.heuristic(p)
    }
    fn heuristic0(&self) -> f64 {
        f64::NEG_INFINITY
    }
    fn label(&self) -> String {
        format!("GoalInverted{{{}}}", self.0.label())
    }
    fn inverted_origin(&self) -> Option<GoalRef> {
        Some(self.0.clone())
    }
    fn render_kind(&self) -> GoalRender {
        GoalRender::Inverted(self.0.clone())
    }
}

/// Away from some positions: done once at least `distance` from every one of them.
pub struct GoalRunAway {
    pub from: Vec<Pos>,
    pub distance: i32,
    pub maintain_y: Option<i32>,
}

impl GoalRunAway {
    /// `distanceSq = (int) (distance * distance)`.
    fn distance_sq(&self) -> i64 {
        i64::from(self.distance) * i64::from(self.distance)
    }
}

impl Goal for GoalRunAway {
    fn is_in_goal(&self, p: Pos) -> bool {
        if let Some(y) = self.maintain_y {
            if p[1] != y {
                return false;
            }
        }
        let want = self.distance_sq();
        self.from.iter().all(|f| {
            let (dx, dz) = (i64::from(p[0] - f[0]), i64::from(p[2] - f[2]));
            dx * dx + dz * dz >= want
        })
    }
    fn heuristic(&self, p: Pos) -> f64 {
        let mut min = f64::MAX;
        for f in &self.from {
            let h = xz_heuristic(f64::from(f[0] - p[0]), f64::from(f[2] - p[2]));
            if h < min {
                min = h;
            }
        }
        min = -min;
        if let Some(y) = self.maintain_y {
            min = min * 0.6 + y_heuristic(y, p[1]) * 1.5;
        }
        min
    }
    fn heuristic0(&self) -> f64 {
        let distance = (self.distance_sq() as f64).sqrt().ceil() as i32;
        let (mut min_x, mut min_y, mut min_z) = (i32::MAX, i32::MAX, i32::MAX);
        let (mut max_x, mut max_y, mut max_z) = (i32::MIN, i32::MIN, i32::MIN);
        for p in &self.from {
            min_x = min_x.min(p[0] - distance);
            min_y = min_y.min(p[1] - distance);
            min_z = min_z.min(p[2] - distance);
            // As the Java has it: the maxima are taken against the minima.
            max_x = min_x.max(p[0] + distance);
            max_y = min_y.max(p[1] + distance);
            max_z = min_z.max(p[2] + distance);
        }
        heuristic_at_goal(self, [min_x, min_y, min_z], [max_x, max_y, max_z])
    }
    fn label(&self) -> String {
        let from = self.from.iter().map(|f| format!("BlockPos{{x={}, y={}, z={}}}", f[0], f[1], f[2])).collect::<Vec<_>>().join(", ");
        match self.maintain_y {
            Some(y) => format!("GoalRunAwayFromMaintainY y={y}, [{from}]"),
            None => format!("GoalRunAwayFrom[{from}]"),
        }
    }
}

/// Keep going in one direction (dx, dz) from an origin, in a straight line.
pub struct GoalStrictDirection {
    pub origin: Pos,
    pub dx: i32,
    pub dz: i32,
}

impl Goal for GoalStrictDirection {
    fn is_in_goal(&self, _p: Pos) -> bool {
        false
    }
    fn heuristic(&self, p: Pos) -> f64 {
        let (dx, dz) = (p[0] - self.origin[0], p[2] - self.origin[2]);
        let along = dx * self.dx + dz * self.dz;
        let across = (dx * self.dz).abs() + (dz * self.dx).abs();
        let vertical = (p[1] - self.origin[1]).abs();
        -f64::from(along) * 100.0 + f64::from(across) * 1000.0 + f64::from(vertical) * 1000.0
    }
    fn heuristic0(&self) -> f64 {
        f64::NEG_INFINITY
    }
    fn label(&self) -> String {
        format!("GoalStrictDirection{{x={}, y={}, z={}, dx={}, dz={}}}", maybe_censor(self.origin[0]), maybe_censor(self.origin[1]), maybe_censor(self.origin[2]), maybe_censor(self.dx), maybe_censor(self.dz))
    }
}

/// The highways of the Nether: a height, and the axes and diagonals through the origin.
pub struct GoalAxis(pub i32);

impl Goal for GoalAxis {
    fn is_in_goal(&self, p: Pos) -> bool {
        p[1] == self.0 && (p[0] == 0 || p[2] == 0 || p[0].abs() == p[2].abs())
    }
    fn heuristic(&self, p: Pos) -> f64 {
        let (x, z) = (p[0].abs(), p[2].abs());
        let (shrt, lng) = (x.min(z), x.max(z));
        let diff = lng - shrt;
        let flat_axis_distance = f64::from(x).min(f64::from(z).min(f64::from(diff) * std::f64::consts::FRAC_1_SQRT_2));
        flat_axis_distance * cost_heuristic() + y_heuristic(self.0, p[1])
    }
    fn label(&self) -> String {
        "GoalAxis".to_owned()
    }
}

/// A goal in a shared pointer: what the searches and the processes hold.
pub fn rc<G: Goal + 'static>(goal: G) -> GoalRef {
    Rc::new(goal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_to_block_counts_the_head_in_the_block_as_good_as_the_feet() {
        let g = GoalGetToBlock([5, 64, 5]);
        // Standing two under, the head is in the block's column: the heuristic of a block 1 higher than that.
        assert_eq!(g.heuristic([5, 63, 5]), 0.0);
        assert!(g.heuristic([5, 62, 5]) > 0.0);
        assert_eq!(GoalTwoBlocks([5, 64, 5]).heuristic([5, 63, 5]), 0.0);
    }

    #[test]
    fn three_blocks_has_the_shaft_in_it() {
        let g = GoalThreeBlocks([5, 64, 5]);
        assert_eq!(g.heuristic([5, 63, 5]), 0.0);
        assert_eq!(g.heuristic([5, 62, 5]), 0.0);
        assert!(g.heuristic([5, 61, 5]) > 0.0);
        assert!(g.is_in_goal([5, 62, 5]) && !g.is_in_goal([5, 61, 5]));
    }

    #[test]
    fn run_away_nearest_source_decides_and_maintain_y_scales() {
        let g = GoalRunAway { from: vec![[0, 64, 0], [100, 64, 0]], distance: 10, maintain_y: None };
        // The nearest source is 5 away, not the farthest.
        assert!((g.heuristic([5, 64, 0]) + xz_heuristic(5.0, 0.0)).abs() < 1e-9);
        let y = GoalRunAway { from: vec![[0, 64, 0]], distance: 10, maintain_y: Some(11) };
        let expected = -xz_heuristic(5.0, 0.0) * 0.6 + y_heuristic(11, 64) * 1.5;
        assert!((y.heuristic([5, 64, 0]) - expected).abs() < 1e-9);
    }

    #[test]
    fn labels_are_the_javas() {
        assert_eq!(GoalYLevel(70).label(), "GoalYLevel{y=70}");
        assert_eq!(GoalNear([1, 2, 3], 4).label(), "GoalNear{x=1, y=2, z=3, rangeSq=16}");
        assert_eq!(GoalComposite(vec![rc(GoalBlock([1, 2, 3])), rc(GoalXZ(4, 5))]).label(), "GoalComposite[GoalBlock{x=1,y=2,z=3}, GoalXZ{x=4,z=5}]");
        assert_eq!(GoalRunAway { from: vec![[1, 2, 3]], distance: 5, maintain_y: None }.label(), "GoalRunAwayFrom[BlockPos{x=1, y=2, z=3}]");
        assert_eq!(GoalStrictDirection { origin: [1, 2, 3], dx: 1, dz: 0 }.label(), "GoalStrictDirection{x=1, y=2, z=3, dx=1, dz=0}");
    }

    #[test]
    fn the_axis_heuristic_is_flat_axis_distance_with_the_diagonal_scaled() {
        let g = GoalAxis(120);
        // x = 10, z = 4: diff 6, diff * sqrt(2)/2 = 4.24, so min(10, 4, 4.24) = 4.
        let want = 4.0 * cost_heuristic();
        assert!((g.heuristic([10, 120, 4]) - want).abs() < 1e-9);
    }

    #[test]
    fn near_has_a_heuristic_at_its_own_centre() {
        let g = GoalNear([0, 64, 0], 2);
        assert!(g.heuristic0().is_finite());
        assert_eq!(GoalInverted(rc(GoalBlock([0, 0, 0]))).heuristic0(), f64::NEG_INFINITY);
    }

    #[test]
    fn adjacent_place_and_break() {
        let place = GoalPlace::new([3, 63, 3]);
        assert!(place.is_in_goal([3, 64, 3]));
        let adj = GoalAdjacent::new([3, 64, 3], [4, 64, 3], true);
        assert!(!adj.is_in_goal([3, 64, 3]) && !adj.is_in_goal([4, 64, 3]) && adj.is_in_goal([2, 64, 3]));
        let brk = GoalBreak([3, 64, 3]);
        assert!(!brk.is_in_goal([3, 65, 3]) && brk.is_in_goal([2, 64, 3]));
    }
}
