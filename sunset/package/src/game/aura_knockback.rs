// Kill Aura's Knockback aim: which way a hit's knockback would hurt the target most, so the look's yaw can point it
// there while the hit is due (aura.rs). The knockback a sprint or a Knockback enchantment adds goes along the
// attacker's yaw on 1.8 and 1.9+ alike, the base knockback along the line between the two of you; the pitch still
// looks at the target.
//
// Around the target, 32 directions are tried, each from 0.8 to 5 blocks in 0.35 steps until a wall stops it. Where the
// target's box would end up is scored by what is there: lava 150, a web 125, fire 100, cactus 96, the void 145, a drop
// with no floor within 24 blocks 108, a ditch of 4 blocks or more 88 and up, water 58; less a few points a block
// away. The best of at least 45 is the plan.
use super::Game;
use super::sunset_util::{Target, facts, replaceable, short};

// What a block cell holds, for the knockback's landing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cell {
    Air,
    Solid,
    Water,
    Lava,
    Web,
    Fire,
    Cactus,
}

impl Cell {
    // A box can't move into it (a cactus's box is a little smaller than a block, but stops a slide all the same).
    fn blocks(self) -> bool {
        matches!(self, Cell::Solid | Cell::Cactus)
    }
}

pub(super) fn cell(game: &Game<'_>, p: [i32; 3]) -> Cell {
    let f = facts(game, p);
    match short(&f.name) {
        "cobweb" => return Cell::Web,
        "fire" | "soul_fire" => return Cell::Fire,
        "cactus" => return Cell::Cactus,
        _ => {}
    }
    match f.fluid {
        crate::game::Fluid::Lava => Cell::Lava,
        crate::game::Fluid::Water => Cell::Water,
        _ if f.waterlogged => Cell::Water,
        _ if replaceable(game, p) => Cell::Air,
        _ => Cell::Solid,
    }
}

// The plan: the yaw that pushes the target toward the worst place found, its score and what is there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Plan {
    pub yaw: f32,
    pub score: f64,
    pub hazard: &'static str,
}

const DIRECTIONS: u32 = 32;
const NEAREST: f64 = 0.8;
const FARTHEST: f64 = 5.0;
const STEP: f64 = 0.35;
// A plan below this is not worth turning the look for.
const WORTH: f64 = 45.0;
// How far below a landing a floor is looked for.
const FLOOR_SEARCH: i32 = 24;

type Box3 = ([f64; 3], [f64; 3]);

fn any_in(b: Box3, at: &impl Fn([i32; 3]) -> Cell, want: impl Fn(Cell) -> bool) -> bool {
    let (lo, hi) = b;
    let from = |v: f64| (v + 1e-4).floor() as i32;
    let to = |v: f64| (v - 1e-4).floor() as i32;
    for x in from(lo[0])..=to(hi[0]) {
        for y in from(lo[1])..=to(hi[1]) {
            for z in from(lo[2])..=to(hi[2]) {
                if want(at([x, y, z])) {
                    return true;
                }
            }
        }
    }
    false
}

// How far the box would drop to a floor below it: the least drop over the columns under it; None when one column has
// no floor within the search (the void, or a drop too deep to matter).
fn drop_below(b: Box3, min_y: i32, at: &impl Fn([i32; 3]) -> Cell) -> Option<f64> {
    let (lo, hi) = b;
    // From the cell under the box's bottom (the floor it stands on, when it stands).
    let top = (lo[1] - 1e-4).floor() as i32;
    let bottom = (top - FLOOR_SEARCH).max(min_y);
    let mut least = f64::INFINITY;
    for x in (lo[0] + 1e-4).floor() as i32..=(hi[0] - 1e-4).floor() as i32 {
        for z in (lo[2] + 1e-4).floor() as i32..=(hi[2] - 1e-4).floor() as i32 {
            let floor = (bottom..=top).rev().find(|&y| at([x, y, z]).blocks())?;
            least = least.min((lo[1] - f64::from(floor + 1)).max(0.0));
        }
    }
    least.is_finite().then_some(least)
}

// What the target's box would meet at `b`, `away` blocks out: its name and score.
fn landing(b: Box3, away: f64, min_y: i32, at: &impl Fn([i32; 3]) -> Cell) -> Option<(&'static str, f64)> {
    let (lo, hi) = b;
    let core = ([lo[0] + 0.05, lo[1], lo[2] + 0.05], [hi[0] - 0.05, hi[1], hi[2] - 0.05]);
    let feet = ([core.0[0], core.0[1] - 0.35, core.0[2]], [core.1[0], core.1[1] - 0.35, core.1[2]]);
    let has = |c: Cell| any_in(core, at, |x| x == c) || any_in(feet, at, |x| x == c);
    if has(Cell::Lava) {
        return Some(("Lava", 150.0 - away * 8.0));
    }
    if has(Cell::Web) {
        return Some(("Web", 125.0 - away * 7.0));
    }
    if has(Cell::Fire) {
        return Some(("Fire", 100.0 - away * 7.0));
    }
    if has(Cell::Cactus) {
        return Some(("Cactus", 96.0 - away * 6.0));
    }
    match drop_below(core, min_y, at) {
        None if core.0[1] <= f64::from(min_y) + 8.0 => Some(("Void", 145.0 - away * 7.0)),
        None => Some(("Deep drop", 108.0 - away * 7.0)),
        Some(d) if d >= 4.0 => Some(("Ditch", 88.0 + d.min(10.0) * 3.5 - away * 6.0)),
        Some(d) if has(Cell::Water) => Some(("Water", 58.0 + d * 2.0 - away * 5.0)),
        Some(_) => None,
    }
}

// The best plan around a box (lo, hi), with `at` telling what each cell holds and `min_y` the world's bottom.
pub(super) fn plan(lo: [f64; 3], hi: [f64; 3], min_y: i32, at: impl Fn([i32; 3]) -> Cell) -> Option<Plan> {
    let mut best: Option<Plan> = None;
    for i in 0..DIRECTIONS {
        let a = std::f64::consts::TAU * f64::from(i) / f64::from(DIRECTIONS);
        // A look of yaw `a` points along (-sin a, cos a): the way its knockback pushes.
        let (dx, dz) = (-a.sin(), a.cos());
        let mut away = NEAREST;
        while away <= FARTHEST + 1e-9 {
            let moved = ([lo[0] + dx * away, lo[1], lo[2] + dz * away], [hi[0] + dx * away, hi[1], hi[2] + dz * away]);
            if let Some((hazard, score)) = landing(moved, away, min_y, &at)
                && best.is_none_or(|b| score > b.score)
            {
                let yaw = (a.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
                best = Some(Plan { yaw: yaw as f32, score, hazard });
            }
            let snug = ([moved.0[0] + 0.02, moved.0[1], moved.0[2] + 0.02], [moved.1[0] - 0.02, moved.1[1], moved.1[2] - 0.02]);
            if any_in(snug, &at, Cell::blocks) {
                break;
            }
            away += STEP;
        }
    }
    best.filter(|b| b.score >= WORTH)
}

// The plan for a target in the world as it is.
pub(super) fn plan_for(game: &Game<'_>, t: &Target) -> Option<Plan> {
    plan(t.lo, t.hi, game.world.shape.min_y, |p| cell(game, p))
}

// The pitch that still looks at the target while the yaw is the plan's: down the line of that yaw to the target's
// upper body.
pub(super) fn pitch_for(eye: [f64; 3], t: &Target, yaw: f32) -> f32 {
    let at = [(t.lo[0] + t.hi[0]) / 2.0, t.lo[1] + (t.hi[1] - t.lo[1]) * 0.77, (t.lo[2] + t.hi[2]) / 2.0];
    let (dx, dy, dz) = (at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]);
    let r = f64::from(yaw).to_radians();
    let forward = (-dx * r.sin() + dz * r.cos()).max(dx.hypot(dz) * 0.25).max(0.05);
    (-dy).atan2(forward).to_degrees().clamp(-90.0, 90.0) as f32
}

// ---- Displace (your own clicks; after Echo's and Celeste's Displace, behaviour only): a hit whose knockback goes
// along your yaw (a sprinting hit at full charge, or a Knockback weapon) is held back a tick so the server first has
// the yaw Direction asks for: Hazard turns it toward the plan above (lava, fire, a web, the void, a drop; with none
// found the hit goes as it is), Left and Right turn your own yaw Angle that way, Back turns it round (the target
// comes toward you). The pitch still looks at the target. The look goes with the next move packet and the hit right
// after it, before the move packet after that one brings your own look back: the hit lands a tick late, and nothing
// but your look changes (Horizon and vanilla check a hit's distance to the box, not the look).
pub(super) fn displace_yaw(direction: &str, yaw: f32, angle: f32, hazard: Option<f32>) -> Option<f32> {
    match direction {
        "Left" => Some(yaw - angle),
        "Right" => Some(yaw + angle),
        "Back" => Some(yaw + 180.0),
        _ => hazard,
    }
}

pub(super) fn displace_click(game: &mut Game<'_>) -> Option<bool> {
    let m = super::sunset::with(|s| s.module("Displace").filter(|m| m.on).cloned())?;
    if game.remote.is_none() || game.spectator() || game.clicks.miss_time > 0 || game.using.is_some() {
        return None;
    }
    if super::sunset::strike_waiting() || super::sunset::turn_pending() || super::sunset::CRIT_STAGE.load(std::sync::atomic::Ordering::Relaxed) != 0 {
        return None;
    }
    if game.held().is_some_and(|s| horizon_client::spear::Kinetic::of(&s.item).is_some()) {
        return None;
    }
    let (i, distance, _) = game.mob_hit()?;
    let projectile = game.redirectable_hit().is_some_and(|(_, d)| d <= distance);
    if projectile || !game.before_block(distance) {
        return None;
    }
    let mob = &game.mobs[i];
    if (m.b("Players only") && mob.kind.id != "minecraft:player") || mob.attrs.knockback_resistance >= 1.0 {
        return None;
    }
    let enchanted = game.held().is_some_and(|s| s.enchantment_level("minecraft:knockback") > 0);
    let charged = game.hand_strength.scale(0.5, game.main_hand_attack_speed()) > 0.9;
    if !enchanted && !(charged && super::sunset_combat_wire::sent_sprinting(game)) {
        return None;
    }
    let t = super::sunset_util::mob_target(mob);
    let eye = game.eye_position()?;
    let direction = m.p("Direction");
    let hazard = if direction == "Hazard" { plan_for(game, &t).map(|p| p.yaw) } else { None };
    let yaw = displace_yaw(direction, game.yaw, m.n("Angle") as f32, hazard)?;
    let pitch = pitch_for(eye, &t, yaw);
    super::sunset::attack(game, t.id, true, Some((yaw, pitch)));
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A flat floor at y = -1 with lava, a wall and a pit placed by hand.
    fn world(extra: &'static [([i32; 3], Cell)]) -> impl Fn([i32; 3]) -> Cell {
        move |p| {
            if let Some((_, c)) = extra.iter().find(|(at, _)| *at == p) {
                return *c;
            }
            if p[1] == -1 { Cell::Solid } else { Cell::Air }
        }
    }
    const PLAYER: ([f64; 3], [f64; 3]) = ([-0.3, 0.0, -0.3], [0.3, 1.8, 0.3]);

    #[test]
    fn displace_turns_the_yaw_its_way() {
        assert_eq!(displace_yaw("Left", 10.0, 90.0, None), Some(-80.0));
        assert_eq!(displace_yaw("Right", 10.0, 45.0, Some(3.0)), Some(55.0));
        assert_eq!(displace_yaw("Back", 10.0, 90.0, None), Some(190.0));
        assert_eq!(displace_yaw("Hazard", 10.0, 90.0, Some(-30.0)), Some(-30.0));
        // Hazard with nothing found: the hit goes as it is.
        assert_eq!(displace_yaw("Hazard", 10.0, 90.0, None), None);
    }

    #[test]
    fn flat_ground_has_no_plan() {
        assert_eq!(plan(PLAYER.0, PLAYER.1, -64, world(&[])), None);
    }

    #[test]
    fn lava_to_the_east_turns_the_yaw_east() {
        // East is +x: a yaw of -90.
        let p = plan(PLAYER.0, PLAYER.1, -64, world(&[([2, 0, 0], Cell::Lava)])).unwrap();
        assert_eq!(p.hazard, "Lava");
        assert!((p.yaw + 90.0).abs() <= 360.0 / 32.0 + 1e-3, "{p:?}");
    }

    #[test]
    fn a_wall_hides_what_is_behind_it() {
        const WALL: [([i32; 3], Cell); 3] = [([1, 0, 0], Cell::Solid), ([1, 1, 0], Cell::Solid), ([3, 0, 0], Cell::Lava)];
        let p = plan(PLAYER.0, PLAYER.1, -64, world(&WALL));
        assert!(p.is_none_or(|p| p.hazard != "Lava"), "{p:?}");
    }

    #[test]
    fn a_missing_floor_near_the_bottom_is_the_void() {
        // No floor anywhere: the void at the bottom of the world, a deep drop far above it.
        let open = |_: [i32; 3]| Cell::Air;
        assert_eq!(plan(PLAYER.0, PLAYER.1, -2, open).map(|p| p.hazard), Some("Void"));
        assert_eq!(plan(PLAYER.0, PLAYER.1, -64, open).map(|p| p.hazard), Some("Deep drop"));
    }

    #[test]
    fn the_pitch_still_looks_at_the_target() {
        let t = Target { id: 1, pos: [3.0, 0.0, 0.0], prev: [3.0, 0.0, 0.0], health: 20.0, lo: [2.7, 0.0, -0.3], hi: [3.3, 1.8, 0.3], player: true, kind: "minecraft:player".into() };
        let level = pitch_for([0.0, 1.62, 0.0], &t, -90.0);
        assert!(level.abs() < 5.0, "{level}");
    }
}
