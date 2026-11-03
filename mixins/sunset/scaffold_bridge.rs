// Scaffold's Legit and Godbridge modes, after vape-rs's (github.com/rdbtCVS/vape-rs, MIT). Both work before the game's
// tick: the sneak, the look and the right click they give are the game's own for that tick, so a block goes with the
// game's sequence number and prediction, before the tick's one move packet.
use super::Game;
use super::sunset::{Module, legit_sneak, scaffold_item, scaffold_slot};
use super::sunset_util as util;
use horizon_core::geometry::{Aabb, Vec3};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

#[derive(Default)]
struct Bridge {
    // Legit: when you were last at an edge, and how long sneak stays once you are past it (picked as sneak starts).
    edge_at: Option<Instant>,
    delay: Duration,
    random: u64,
    // Godbridge, before it takes over: the next block you place by hand behind you, and how many you have placed.
    place: Option<[i32; 3]>,
    placed: u32,
    // Godbridge, bridging.
    line: Option<Line>,
    // When you stopped at the edge (the look goes 2 degrees lower after 300 ms there).
    stalled: Option<Instant>,
    // The block the last right click was for, and the tick: one click a block, half a second for the server.
    clicked: Option<([i32; 3], u64)>,
    // The move keys and right click this tick are Godbridge's: given back after it.
    took: bool,
}

// The bridge: the way it goes (x and z, one of them 1 or -1), on which side of it you walk (and so which strafe key),
// where across the blocks you keep to, and the slot in hand before it took over.
#[derive(Clone, Copy)]
struct Line {
    step: [i32; 2],
    reversed: bool,
    across: f64,
    slot: usize,
}

static BRIDGE: Mutex<Option<Bridge>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Bridge) -> R) -> R {
    let mut guard = BRIDGE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Bridge::default))
}

// The way behind you, to the nearest of the four: yaw 0 faces +z, 90 faces -x.
fn behind(yaw: f32) -> [i32; 2] {
    match (yaw + 180.0).rem_euclid(360.0) {
        a if !(45.0..315.0).contains(&a) => [0, 1],
        a if a < 135.0 => [-1, 0],
        a if a < 225.0 => [0, -1],
        _ => [1, 0],
    }
}

// The yaw that faces back along the bridge (to where it started).
fn back_yaw(step: [i32; 2]) -> f32 {
    (step[0] as f32).atan2(-step[1] as f32).to_degrees()
}

// Turning the look one way turns the walk the same way: +1 when a larger yaw walks you toward +across.
fn across_sign(step: [i32; 2]) -> f64 {
    if step[0] != 0 { f64::from(step[0]) } else { -f64::from(step[1]) }
}

// You walk the bridge back and a strafe key at 45 degrees to it, on the side the look's ray leans away from.
fn across_frac(step: [i32; 2], reversed: bool) -> f64 {
    if reversed == (across_sign(step) > 0.0) { 0.8 } else { 0.2 }
}

// The look for the bridge, turned 20 degrees a block toward the line when you are off it.
fn bridge_yaw(step: [i32; 2], reversed: bool, off_line: f64) -> f32 {
    back_yaw(step) + if reversed { 45.0 } else { -45.0 } + (20.0 * across_sign(step) * off_line) as f32
}

fn angle_between(a: f32, b: f32) -> f32 {
    ((a - b + 180.0).rem_euclid(360.0) - 180.0).abs()
}

// The pitch whose ray, from `past` blocks beyond the last block's edge, meets that block's side `drop` below the
// eyes, with `toward` the share of the look that faces back along the bridge.
fn face_pitch(past: f64, toward: f64, drop: f64) -> f32 {
    (toward * drop).atan2(past).to_degrees().min(89.9) as f32
}

// Before the edge, walk on only while the next tick (this speed and a walk's push of 0.1) still stops short of where
// the look can no longer reach the side: else let go and slide up to it.
fn walk_on(past: f64, speed: f64) -> bool {
    past + speed + 0.1 <= 0.26
}

// The block height you stand on (a slab's own block on half a block).
fn under_y(y: f64) -> i32 {
    if (y - y.trunc()).abs() == 0.5 { y.floor() as i32 } else { (y - 1.0).floor() as i32 }
}

fn next_random(b: &mut Bridge) -> u64 {
    if b.random == 0 {
        b.random = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
    }
    b.random = b.random.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut n = b.random;
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    n ^ (n >> 31)
}

// Nothing under the middle of your feet, 0.2 in from each side, where this tick's motion takes them.
fn at_edge(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else { return false; };
    let feet = p.bounds().deflate(Vec3::new(0.2, 0.0, 0.2)).move_by(Vec3::new(p.velocity.x, 0.0, p.velocity.z));
    let (lo, hi) = (feet.min_position(), feet.max_position());
    let below = Aabb::new(Vec3::new(lo.x, lo.y - 0.5, lo.z), Vec3::new(hi.x, lo.y - 0.001, hi.z));
    horizon_client::collisions::SceneCollisions { world: &game.world, binding: game.collision, subject: None }
        .unobstructed(below)
        .unwrap_or(false)
}

fn climbing(game: &Game<'_>) -> bool {
    let Some(p) = game.player.as_ref() else { return false; };
    let at = [p.position.x.floor() as i32, p.position.y.floor() as i32, p.position.z.floor() as i32];
    let name = &util::facts(game, at).name;
    ["ladder", "vine", "scaffolding"].iter().any(|e| name.ends_with(e))
}

// Legit (vape-rs's eagle): sneak while not walking forward, on the ground, at the edge of the block under you; once past
// it, sneak stays a moment longer (from Shortest to Longest sneak delay, picked anew as sneak starts). You place.
fn edge_sneak(game: &mut Game<'_>, b: &mut Bridge, delay: (f64, f64)) {
    let Some(p) = game.player.as_ref() else { return; };
    let ground = p.ground.on_ground;
    if !ground || p.flying || climbing(game) {
        legit_sneak(game, false);
        return;
    }
    let forward = super::sunset::move_input(game).0 > 0.0;
    let now = Instant::now();
    let edge = !forward && at_edge(game);
    let lingering = !edge && b.delay > Duration::from_millis(30) && b.edge_at.is_some_and(|at| now - at < b.delay);
    if !(edge || lingering) {
        legit_sneak(game, false);
        return;
    }
    if !game.keys.contains(&KeyCode::ShiftLeft) {
        let (lo, hi) = (delay.0.min(delay.1), delay.0.max(delay.1));
        let span = (hi - lo) as u64 + 1;
        b.delay = Duration::from_millis(lo as u64 + next_random(b) % span);
    }
    if edge {
        b.edge_at = Some(now);
    }
    legit_sneak(game, true);
}

// Before Godbridge takes over: the blocks you place by hand along the way behind you, counted up to Activation blocks.
// Returns the bridge once there are enough and you walk back.
fn count_placed(game: &Game<'_>, m: &Module, b: &mut Bridge) -> Option<Line> {
    let p = game.player.as_ref()?;
    if !scaffold_item(game, m, game.selected) {
        (b.place, b.placed) = (None, 0);
        return None;
    }
    let step = behind(game.yaw);
    let feet = [p.position.x.floor() as i32, under_y(p.position.y), p.position.z.floor() as i32];
    let along = |at: [i32; 3], n: i32| [at[0] + step[0] * n, at[1], at[2] + step[1] * n];
    let air = |at: [i32; 3]| util::replaceable(game, at);
    let Some(place) = b.place else {
        if p.ground.on_ground {
            b.place = (0..3).map(|n| along(feet, n)).find(|&at| air(at));
            b.placed = 0;
        }
        return None;
    };
    let enough = m.n("Activation blocks");
    if f64::from(b.placed) >= enough {
        if !game.physical_keys.contains(&KeyCode::KeyS) || game.keys.contains(&KeyCode::Space) {
            return None;
        }
        (b.place, b.placed) = (None, 0);
        let reversed = ((game.yaw - back_yaw(step) + 180.0).rem_euclid(360.0) - 180.0) >= 0.0;
        let cell = f64::from(if step[0] != 0 { place[2] } else { place[0] });
        return Some(Line { step, reversed, across: cell + across_frac(step, reversed), slot: game.selected });
    }
    if !air(place) {
        b.placed += 1;
        let next = along(place, 1);
        if !air(next) {
            (b.place, b.placed) = (None, 0);
        } else if f64::from(b.placed) < enough {
            b.place = Some(next);
        }
        return None;
    }
    // Still air, but you went off its row, to another height or too far from where the counting began: again.
    let across = if step[0] != 0 { 2 } else { 0 };
    let start = (0..3).map(|i| f64::from(place[i] - step_at(step, i) * b.placed as i32 - feet[i]).powi(2)).sum::<f64>().sqrt();
    if place[across] != feet[across] || place[1] != feet[1] || start > enough + 2.0 {
        (b.place, b.placed) = (None, 0);
    }
    None
}

fn step_at(step: [i32; 2], axis: usize) -> i32 {
    match axis {
        0 => step[0],
        2 => step[1],
        _ => 0,
    }
}

// Godbridge (vape-rs's GodBridge, straight bridges): no sneak. The look turns to face back along the bridge, 45 degrees
// to one side; S and that side's strafe key walk you along it. At the edge it lets go and slides up to it, and once
// you are past the last block's edge (still on it) the look lowers until the crosshair is on that block's side, and
// a right click puts the next one there. Lasts while S is held and there are blocks.
fn godbridge(game: &mut Game<'_>, m: &Module, b: &mut Bridge) {
    let Some(line) = b.line else {
        edge_sneak(game, b, (100.0, 200.0));
        if let Some(line) = count_placed(game, m, b) {
            b.line = Some(line);
            b.stalled = None;
            legit_sneak(game, false);
        }
        return;
    };
    let slot = scaffold_slot(game, m);
    let Some((p, slot)) = game.player.as_ref().filter(|p| !p.flying).zip(slot) else {
        end(game, b);
        return;
    };
    if !game.physical_keys.contains(&KeyCode::KeyS) {
        end(game, b);
        return;
    }
    let (pos, vel, rising) = ([p.position.x, p.position.y, p.position.z], [p.velocity.x, p.velocity.z], p.velocity.y);
    let step = line.step;
    let (along, across) = if step[0] != 0 { (0, 2) } else { (2, 0) };
    let dir = f64::from(step[0] + step[1]);
    legit_sneak(game, false);
    game.selected = slot;
    // The look along the bridge, back onto the line when off it.
    let yaw = bridge_yaw(step, line.reversed, line.across - pos[across]);
    let rate = (2.0 + angle_between(game.yaw, yaw) / 15.0).min(12.0);
    game.yaw = super::sunset_movement::turn_toward(game.yaw, yaw, rate);
    let turning = angle_between(game.yaw, yaw) > 4.0;
    let ticks = game.ticks;
    // Jumping (vape-rs's GodBridge): while you go up or down, the crosshair on a block's top counts as well as on its
    // side, so the bridge's own look puts a block under you as you clear it, and you land one up.
    let jumped = !(-0.1..=0.1).contains(&rising) && !turning && game.pick().is_some_and(|hit| {
        let at = std::array::from_fn(|i| hit.pos[i] + hit.face[i]);
        let fresh = b.clicked.is_none_or(|(was, tick)| was != at || ticks >= tick + 10);
        hit.face[1] >= 0 && fresh && util::can_place(game, at, true)
    });
    if jumped && let Some(hit) = game.pick() {
        super::remote::sunset_remote::press_use(game);
        b.clicked = Some((std::array::from_fn(|i| hit.pos[i] + hit.face[i]), ticks));
    }
    let y = under_y(pos[1]);
    let cell = |n: i32| {
        let mut at = [pos[0].floor() as i32, y, pos[2].floor() as i32];
        at[along] += step_at(step, along) * n;
        at
    };
    // The last block: the one under you, or the one you just went past.
    let solid = |at: [i32; 3]| !util::replaceable(game, at);
    let last = if solid(cell(0)) { Some(cell(0)) } else { Some(cell(-1)).filter(|&at| solid(at)) };
    let mut walk = !turning;
    let mut pitch = if b.stalled.is_some_and(|at| at.elapsed() >= Duration::from_millis(300)) { 80.0 } else { 78.0 };
    match last {
        _ if jumped => {}
        Some(last) if !solid(cell_after(last, step, along)) => {
            b.stalled.get_or_insert_with(Instant::now);
            // How far your middle is past the last block's leading side.
            let edge = f64::from(last[along]) + if dir > 0.0 { 1.0 } else { 0.0 };
            let past = (pos[along] - edge) * dir;
            if past > 0.0 && past < 0.29 && !turning {
                let toward = f64::from(game.yaw.to_radians().sin()) * f64::from(step[0]) - f64::from(game.yaw.to_radians().cos()) * f64::from(step[1]);
                let drop = game.eye_position().map_or(1.62, |eye| eye[1] - f64::from(last[1]) - 0.5);
                pitch = face_pitch(past, toward.max(0.1), drop);
                game.pitch = pitch;
                let next = cell_after(last, step, along);
                let face = [step[0], 0, step[1]];
                let fresh = b.clicked.is_none_or(|(at, tick)| at != next || ticks >= tick + 10);
                walk = game.pick().is_some_and(|hit| hit.pos == last && hit.face == face) && fresh && util::can_place(game, next, true);
                if walk {
                    super::remote::sunset_remote::press_use(game);
                    b.clicked = Some((next, ticks));
                }
            } else if past <= 0.0 {
                walk &= walk_on(past, vel[along / 2] * dir);
            } else {
                walk = false;
            }
        }
        Some(_) => b.stalled = None,
        None => walk = false,
    }
    if game.pitch != pitch {
        game.pitch = super::sunset_movement::turn_toward(game.pitch, pitch, 12.0);
    }
    let strafe = if line.reversed { KeyCode::KeyD } else { KeyCode::KeyA };
    for k in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD] {
        game.keys.remove(&k);
    }
    if walk {
        game.keys.insert(KeyCode::KeyS);
        game.keys.insert(strafe);
    }
    game.right = false;
    b.took = true;
}

fn cell_after(last: [i32; 3], step: [i32; 2], along: usize) -> [i32; 3] {
    let mut at = last;
    at[along] += step_at(step, along);
    at
}

// Back to waiting for blocks placed by hand, with the slot you had in hand.
fn end(game: &mut Game<'_>, b: &mut Bridge) {
    if let Some(line) = b.line.take() {
        game.selected = line.slot;
    }
    (b.place, b.placed, b.stalled) = (None, 0, None);
}

// Each tick before the game's: Scaffold when Legit or Godbridge is the mode and it may work now, else None.
pub(super) fn pre_tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        stop(game);
        return;
    };
    with(|b| match m.p("Mode") {
        "Legit" => {
            if b.line.is_some() {
                end(game, b);
            }
            if scaffold_item(game, m, game.selected) {
                edge_sneak(game, b, (m.n("Shortest sneak delay (ms)"), m.n("Longest sneak delay (ms)")));
            } else {
                legit_sneak(game, false);
            }
        }
        "Godbridge" => godbridge(game, m, b),
        _ => {}
    });
}

// After the game's tick: the keys and right click Godbridge took go back to what you hold.
pub(super) fn post_tick(game: &mut Game<'_>) {
    if !with(|b| std::mem::take(&mut b.took)) {
        return;
    }
    for k in [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD] {
        if game.physical_keys.contains(&k) {
            game.keys.insert(k);
        } else {
            game.keys.remove(&k);
        }
    }
    game.right = game.physical_buttons.contains(&MouseButton::Right);
}

// Scaffold off, another mode, Panic or no world: sneak, slot and keys back.
pub(super) fn stop(game: &mut Game<'_>) {
    let active = with(|b| b.line.is_some() || b.place.is_some() || b.edge_at.is_some() || b.took);
    if !active {
        return;
    }
    with(|b| {
        if game.player.is_some() {
            end(game, b);
        }
        *b = Bridge { random: b.random, took: b.took, ..Bridge::default() };
    });
    legit_sneak(game, false);
    post_tick(game);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_goes_behind_you_and_the_look_faces_back_along_it() {
        assert_eq!(behind(180.0), [0, 1]);
        assert_eq!(behind(0.0), [0, -1]);
        assert_eq!(behind(90.0), [1, 0]);
        assert_eq!(behind(-90.0), [-1, 0]);
        assert_eq!(behind(130.0), [1, 0]);
        for step in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            let yaw = back_yaw(step).to_radians();
            // Facing back: the look's flat direction is the bridge's, reversed.
            assert!((f64::from(-yaw.sin()) + f64::from(step[0])).abs() < 1e-6, "{step:?}");
            assert!((f64::from(yaw.cos()) + f64::from(step[1])).abs() < 1e-6, "{step:?}");
        }
    }

    #[test]
    fn back_and_strafe_at_the_bridge_look_walk_along_it_and_turn_toward_the_line() {
        // The walk of S and a strafe key at yaw `yaw` (A: left, D: right), as the game moves you.
        let walk = |yaw: f32, right: bool| {
            let (s, c) = (f64::from(yaw.to_radians().sin()), f64::from(yaw.to_radians().cos()));
            let strafe = if right { -1.0 } else { 1.0 };
            [s + c * strafe, -c + s * strafe]
        };
        for step in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            for reversed in [false, true] {
                let w = walk(bridge_yaw(step, reversed, 0.0), reversed);
                let len = (w[0] * w[0] + w[1] * w[1]).sqrt();
                assert!((w[0] / len - f64::from(step[0])).abs() < 1e-6 && (w[1] / len - f64::from(step[1])).abs() < 1e-6, "{step:?} {reversed}");
                // Off the line toward -across: the turned walk leans toward +across.
                let w = walk(bridge_yaw(step, reversed, 0.2), reversed);
                let across = if step[0] != 0 { w[1] } else { w[0] };
                assert!(across > 0.0, "{step:?} {reversed}");
            }
        }
        assert_eq!(across_frac([0, -1], true), 0.8);
        assert_eq!(across_frac([0, 1], true), 0.2);
    }

    #[test]
    fn the_crosshair_meets_the_side_only_past_the_edge_and_the_walk_stops_short() {
        // From 0.1 past the edge, 2.12 above the side's middle, half the look facing back: steep, under 90.
        let pitch = face_pitch(0.1, 0.707, 2.12);
        assert!(pitch > 85.0 && pitch < 89.9);
        let reach = 0.707 * 2.12 / f64::from(pitch.to_radians().tan());
        assert!((reach - 0.1).abs() < 1e-3);
        assert!(face_pitch(0.0, 0.707, 2.12) <= 89.9);
        // Walking (0.2 a tick) half a block before the edge goes on; at it, let go and slide.
        assert!(walk_on(-0.5, 0.2));
        assert!(walk_on(-0.05, 0.2));
        assert!(!walk_on(0.0, 0.2));
        assert!(walk_on(0.0, 0.0));
        assert_eq!(under_y(65.0), 64);
        assert_eq!(under_y(64.5), 64);
    }
}
