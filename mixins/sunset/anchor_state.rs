// Original, world-independent ordering rules for Anchor Aura.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Place,
    Charge,
    Detonate,
}

// An observed low-health target may be finished below the configured normal
// threshold. Missing/non-finite health cannot relax that threshold.
pub(super) fn target_min(configured: f64, health: f64) -> Option<f64> {
    (configured.is_finite() && configured >= 0.0 && health.is_finite() && health > 0.0)
        .then_some(configured.min(health))
}

pub(super) fn target_available(health: f64, hurt_immunity: bool) -> bool {
    health.is_finite() && health > 0.0 && !hurt_immunity
}

// A current detonation has priority over further setup. Among equally ready
// opportunities, prefer a possible finish, then damage retained after self cost.
// This ranks observed health only; it makes no claim about hidden totems.
pub(super) fn opportunity(
    damage: f64,
    health: f64,
    own: f64,
    attack: bool,
) -> Option<(bool, bool, f64)> {
    if ![damage, health, own]
        .iter()
        .all(|n| n.is_finite() && *n >= 0.0)
        || health == 0.0
    {
        return None;
    }
    Some((attack, damage >= health, damage.min(health) - own))
}

pub(super) fn better(next: (bool, bool, f64), old: (bool, bool, f64)) -> bool {
    (next.0, next.1)
        .cmp(&(old.0, old.1))
        .then_with(|| next.2.total_cmp(&old.2))
        .is_gt()
}

// Restrict automation to inert, full solid supports. Some interactive blocks
// (crafting tables, anvils) have no block entity; clicking them opens a screen.
pub(super) fn inert_support(name: &str) -> bool {
    matches!(
        name.strip_prefix("minecraft:").unwrap_or(name),
        "obsidian"
            | "crying_obsidian"
            | "bedrock"
            | "stone"
            | "cobblestone"
            | "deepslate"
            | "cobbled_deepslate"
            | "dirt"
            | "grass_block"
            | "netherrack"
            | "end_stone"
            | "sandstone"
            | "red_sandstone"
            | "bricks"
            | "stone_bricks"
            | "nether_bricks"
            | "blackstone"
            | "polished_blackstone"
            | "basalt"
            | "andesite"
            | "diorite"
            | "granite"
            | "iron_block"
            | "gold_block"
            | "diamond_block"
            | "netherite_block"
    )
}

// The LOS ray must stop outside the selected face, leaving the entire support
// block available to occlude a hidden-face click.
pub(super) fn face_ray_end(hit: [f64; 3], face: i32) -> Option<[f64; 3]> {
    let normal = match face {
        0 => [0.0, -1.0, 0.0],
        1 => [0.0, 1.0, 0.0],
        2 => [0.0, 0.0, -1.0],
        3 => [0.0, 0.0, 1.0],
        4 => [-1.0, 0.0, 0.0],
        5 => [1.0, 0.0, 0.0],
        _ => return None,
    };
    if !hit.iter().all(|n| n.is_finite()) {
        return None;
    }
    Some(std::array::from_fn(|i| hit[i] + normal[i] * 1.0e-5))
}

// `None` means the observed block is not an anchor. Unknown/malformed charges
// must be rejected by the caller, rather than treated as an empty anchor.
pub(super) fn next(charges: Option<u8>, offhand_fuel: bool) -> Step {
    match charges {
        None => Step::Place,
        Some(0) => Step::Charge,
        Some(1..=3) if offhand_fuel => Step::Charge,
        Some(_) => Step::Detonate,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wait {
    Pending,
    Changed,
    Expired,
}

pub(super) fn observed(before: u32, now: u32, sent: u64, tick: u64, timeout: u64) -> Wait {
    if before != now {
        Wait::Changed
    } else if tick < sent || tick.saturating_sub(sent) >= timeout {
        Wait::Expired
    } else {
        Wait::Pending
    }
}

// A completely filled charge bar suppresses the offhand-fuel PASS branch.
// Detonation always uses the main hand and a non-glowstone selected item.
pub(super) fn can_detonate(charges: u8, main_fuel: bool, offhand_fuel: bool) -> bool {
    (1..=4).contains(&charges) && !main_fuel && (charges == 4 || !offhand_fuel)
}
