// baritone.api.pathing.movement.ActionCosts: what a move costs, in game ticks. The A* adds these up,
// so the cheapest path is the one that takes the fewest ticks.
use std::sync::OnceLock;

pub const WALK_ONE_BLOCK_COST: f64 = 20.0 / 4.317;
pub const WALK_ONE_IN_WATER_COST: f64 = 20.0 / 2.2;
pub const WALK_ONE_OVER_SOUL_SAND_COST: f64 = WALK_ONE_BLOCK_COST * 2.0;
pub const LADDER_UP_ONE_COST: f64 = 20.0 / 2.35;
pub const LADDER_DOWN_ONE_COST: f64 = 20.0 / 3.0;
/// Sprint swimming, roughly wading speed times the 1.3 sprint multiplier.
pub const SWIM_ONE_BLOCK_COST: f64 = 20.0 / 2.86;
pub const SNEAK_ONE_BLOCK_COST: f64 = 20.0 / 1.3;
pub const SPRINT_ONE_BLOCK_COST: f64 = 20.0 / 5.612;
pub const SPRINT_MULTIPLIER: f64 = SPRINT_ONE_BLOCK_COST / WALK_ONE_BLOCK_COST;
/// Stepping off an edge takes most of a walked block, and the rest is centring after the fall.
pub const WALK_OFF_BLOCK_COST: f64 = WALK_ONE_BLOCK_COST * 0.8;
pub const CENTER_AFTER_FALL_COST: f64 = WALK_ONE_BLOCK_COST - WALK_OFF_BLOCK_COST;
/// A move that cannot be made.
pub const COST_INF: f64 = 1_000_000.0;

/// The distance fallen in the `ticks`th tick of a fall: gravity 0.08 a tick, drag 0.98.
fn velocity(ticks: i32) -> f64 {
    (0.98f64.powi(ticks) - 1.0) * -3.92
}

/// How many ticks it takes to fall `distance` blocks.
pub fn distance_to_ticks(distance: f64) -> f64 {
    if distance == 0.0 {
        return 0.0;
    }
    let mut remaining = distance;
    let mut tick = 0;
    loop {
        let fallen = velocity(tick);
        if remaining <= fallen {
            return f64::from(tick) + remaining / fallen;
        }
        remaining -= fallen;
        tick += 1;
    }
}

fn table() -> &'static Vec<f64> {
    static TABLE: OnceLock<Vec<f64>> = OnceLock::new();
    TABLE.get_or_init(|| (0..=4096).map(|n| distance_to_ticks(f64::from(n))).collect())
}

/// The ticks a fall of `n` blocks takes.
pub fn fall_cost(n: i32) -> f64 {
    table().get(n.max(0) as usize).copied().unwrap_or(COST_INF)
}

/// The ticks a jump up one block takes: the fall from its peak, less the fall from where a walk
/// leaves the ground.
pub fn jump_one_block_cost() -> f64 {
    distance_to_ticks(1.25) - distance_to_ticks(0.25)
}
