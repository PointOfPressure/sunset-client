// Kill Aura's Target strafe: while Kill Aura has a target and a move key is held (Strafe when: always, while jump is
// held, or while Speed is on), the move across the ground goes round the target at Strafe radius instead of where the
// keys point, at the speed the move has anyway (walking, sprinting, or Speed's). Each tick it heads for the point of
// the circle a tick's move ahead, so from further out it closes in and from inside it backs out onto the circle. A or
// D picks the way round (to your left or right as you face the target); a wall turns it the other way, and with Avoid
// edges so does a drop of more than three blocks, lava, fire or the like where it is heading (neither way safe: no
// strafe). Riding, gliding or flying it leaves alone.
use super::Game;
use super::sunset::Module;
use super::sunset_util as util;
use std::sync::Mutex;
use winit::keyboard::KeyCode;

// Blocks it keeps you out of with Avoid edges.
const HAZARDS: [&str; 10] = [
    "lava", "fire", "soul_fire", "magma_block", "cactus", "campfire", "soul_campfire", "sweet_berry_bush", "wither_rose", "powder_snow",
];
// The deepest ground looked for under you and ahead, and the drop it lets you take.
const DEEPEST: i32 = 8;
const DROP: i32 = 3;
// Ticks between two turns at walls.
const TURN_TICKS: u64 = 10;

// The way round (1: to your left as you face the target, that is anticlockwise seen from above; -1: to your right)
// and the tick it last turned at a wall.
struct Way {
    side: f64,
    turned: u64,
}

static WAY: Mutex<Way> = Mutex::new(Way { side: 1.0, turned: 0 });

fn way<R>(f: impl FnOnce(&mut Way) -> R) -> R {
    f(&mut WAY.lock().unwrap_or_else(|e| e.into_inner()))
}

// The way (x, z, length 1) this tick's move goes, or None to leave it as it is. `speed_on`: Speed is on.
pub(super) fn direction(game: &Game<'_>, m: &Module, speed_on: bool) -> Option<[f64; 2]> {
    let id = super::sunset_aura::target_id()?;
    let target = game.mobs.iter().find(|t| t.id == id && t.alive() && !t.removed)?;
    let p = game.player.as_ref()?;
    if super::sunset::move_input(game) == (0.0, 0.0) || game.riding.is_some() || p.fall_flying || p.flying {
        return None;
    }
    let wanted = match m.p("Strafe when") {
        "Jump held" => game.physical_keys.contains(&KeyCode::Space),
        "Speed on" => speed_on,
        _ => true,
    };
    if !wanted {
        return None;
    }
    let (left, right) = (game.keys.contains(&KeyCode::KeyA), game.keys.contains(&KeyCode::KeyD));
    let ticks = game.ticks;
    let wall = p.collision.horizontal;
    let side = way(|w| {
        if left != right {
            w.side = if left { 1.0 } else { -1.0 };
        } else if wall && ticks.saturating_sub(w.turned) >= TURN_TICKS {
            w.side = -w.side;
            w.turned = ticks;
        }
        w.side
    });
    let here = [p.position.x, p.position.z];
    let centre = [target.position.x, target.position.z];
    let step = (p.position.x - game.previous_position.x).hypot(p.position.z - game.previous_position.z).clamp(0.1, 1.5);
    let radius = m.n("Strafe radius").max(0.1);
    let first = aim(here, centre, radius, step, side, game.yaw);
    if !m.b("Avoid edges") || safe(game, here, first, step, p.position.y) {
        return Some(first);
    }
    let other = aim(here, centre, radius, step, -side, game.yaw);
    if safe(game, here, other, step, p.position.y) {
        way(|w| w.side = -side);
        return Some(other);
    }
    None
}

// Toward the point of the circle round `centre` that is `step` along it from where you are, `side` round.
fn aim(here: [f64; 2], centre: [f64; 2], radius: f64, step: f64, side: f64, yaw: f32) -> [f64; 2] {
    let off = [here[0] - centre[0], here[1] - centre[1]];
    // Right on the target: out the way you face first.
    let angle = if off[0].hypot(off[1]) < 1e-3 {
        let yaw = f64::from(yaw).to_radians();
        (yaw.cos()).atan2(-yaw.sin())
    } else {
        off[1].atan2(off[0])
    };
    let next = angle + side * (step / radius).min(1.0);
    let point = [centre[0] + radius * next.cos(), centre[1] + radius * next.sin()];
    let to = [point[0] - here[0], point[1] - here[1]];
    let len = to[0].hypot(to[1]);
    if len < 1e-6 {
        // On the point already: along the circle.
        let tangent = [-next.sin() * side, next.cos() * side];
        return tangent;
    }
    [to[0] / len, to[1] / len]
}

// Where a few ticks' move along `dir` takes you is no hazard and no drop of more than three blocks below the ground
// under you now (over no ground already, any way is as good).
fn safe(game: &Game<'_>, here: [f64; 2], dir: [f64; 2], step: f64, y: f64) -> bool {
    let ahead = (step * 3.0).clamp(0.6, 2.0);
    let at = [here[0] + dir[0] * ahead, here[1] + dir[1] * ahead];
    let feet = y.floor() as i32;
    let column = |x: f64, z: f64| [x.floor() as i32, z.floor() as i32];
    let [x, z] = column(at[0], at[1]);
    let hazard = |p: [i32; 3]| HAZARDS.contains(&util::short(&util::facts(game, p).name));
    if (-1..=1).any(|dy| hazard([x, feet + dy, z])) {
        return false;
    }
    let depth = |[cx, cz]: [i32; 2]| (1..=DEEPEST).find(|d| !util::is_air(game, [cx, feet - d, cz]));
    let Some(now) = depth(column(here[0], here[1])) else {
        return true;
    };
    depth([x, z]).is_some_and(|d| d <= now + DROP && !hazard([x, feet - d, z]))
}
