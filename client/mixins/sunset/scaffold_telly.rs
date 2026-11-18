// Telly's jump cycle is separate from Normal's aiming presets. Movement, placement and returning all follow the
// look actually forwarded by the connection; the camera never supplies an unforwarded placement ray.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
enum Phase {
    #[default]
    Ground,
    Takeoff,
    Air,
    Place,
    Return,
    Landing,
}

struct Run {
    phase: Phase,
    anchor: Option<i32>,
    keep: bool,
    towering: bool,
    air: u32,
    rotation_tick: u32,
}

impl Default for Run {
    fn default() -> Self {
        Self { phase: Phase::Ground, anchor: None, keep: false, towering: false, air: 0, rotation_tick: 3 }
    }
}

impl Run {
    fn advance(
        &mut self,
        ground: bool,
        y: f64,
        forward: bool,
        jump: bool,
        keep: bool,
        tower: bool,
        headroom: bool,
    ) -> bool {
        let kept = self.keep;
        self.keep = keep && forward && !jump;
        if ground {
            let landed = self.air > 0;
            self.air = 0;
            if !self.keep || !kept || self.anchor.is_none() {
                self.anchor = Some(y.floor() as i32);
            }
            self.towering = forward && headroom && (self.keep || (tower && jump));
            let automatic = self.keep && headroom;
            self.phase = if automatic || self.towering {
                Phase::Takeoff
            } else if landed {
                Phase::Landing
            } else {
                Phase::Ground
            };
            self.rotation_tick = 3;
            automatic
        } else {
            self.air = self.air.saturating_add(1);
            self.rotation_tick = self.rotation_tick.saturating_sub(1);
            if matches!(self.phase, Phase::Ground | Phase::Takeoff | Phase::Landing) {
                self.phase = Phase::Air;
            }
            false
        }
    }

    fn row(&self, y: f64) -> i32 {
        let feet = y.floor() as i32;
        if self.keep {
            feet.min(self.anchor.unwrap_or(feet)) - 1
        } else {
            feet - 1
        }
    }

    fn may_place(&self, ground: bool) -> bool {
        !ground && self.rotation_tick == 0 && !matches!(self.phase, Phase::Return)
    }
}

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

fn random_speed(a: f32, b: f32, random: f64) -> f32 {
    a.min(b) + (a - b).abs() * random as f32
}

// Planning horizon only: native physics still owns every jump and every velocity update.
fn landing_ticks(mut y: f64, mut up: f64, top: f64) -> u32 {
    for tick in 0..20 {
        if y <= top && up <= 0.0 { return tick; }
        y += up;
        up = (up - 0.08) * 0.98;
    }
    20
}

fn back_yaws(mode: &str, walk: f32) -> [f32; 4] {
    let back = walk + 180.0;
    let diagonal = if (walk.rem_euclid(90.0) - 45.0).abs() < 25.0 {
        back
    } else if (walk + 180.0).rem_euclid(90.0) < 45.0 {
        walk - 135.0
    } else {
        walk + 135.0
    };
    match mode {
        "Backwards" => [back; 4],
        "Sideways" => [diagonal; 4],
        _ => [diagonal, back, diagonal + 90.0, diagonal - 90.0],
    }
}

mod native {
    use super::super::remote::sunset_remote as remote;
    use super::super::sunset::{Module, hold, move_input, scaffold_drawn, scaffold_slot};
    use super::super::{
        Game, Screen, sunset_rotation as rotation, sunset_scaffold_normal as normal,
        sunset_util as util,
    };
    use super::{Phase, Run, back_yaws, landing_ticks, random_speed, wrap};
    use std::cell::Cell;
    use std::fmt::Write as _;
    use std::sync::Mutex;
    use winit::event::MouseButton;
    use winit::keyboard::KeyCode;

    thread_local! {
        static BLOCK_USE: Cell<bool> = const { Cell::new(false) };
        static PLACING: Cell<bool> = const { Cell::new(false) };
    }

    #[derive(Default)]
    struct Telly {
        active: bool,
        run: Run,
        origin: Option<[f64; 2]>,
        planner: normal::Planner,
        rng: Option<rotation::Rng>,
        slot_before: Option<usize>,
        slot_used: Option<usize>,
        swap_back: bool,
        keys_before: Option<[bool; 4]>,
        jump_before: Option<bool>,
        sprint_before: Option<bool>,
        watch: [Option<([i32; 3], u64)>; 8],
        watch_at: usize,
        cooled: [Option<([i32; 3], u64)>; 8],
        cooled_at: usize,
        last_place: Option<u64>,
        cooldown: u64,
        look: Option<(f32, f32)>,
        clicks: u64,
        rejected: u64,
        place_turn: Option<f32>,
    }

    static TELLY: Mutex<Option<Telly>> = Mutex::new(None);

    fn with<R>(f: impl FnOnce(&mut Telly) -> R) -> R {
        let mut guard = TELLY.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(Telly::default))
    }

    pub(crate) fn manual_use_blocked(game: &Game<'_>) -> bool {
        game.native_window() && BLOCK_USE.with(Cell::get) && !PLACING.with(Cell::get)
    }

    fn set_key(game: &mut Game<'_>, key: KeyCode, on: bool, before: &mut Option<bool>) {
        let held = game.keys.contains(&key);
        if held != on {
            before.get_or_insert(held);
            if on {
                game.keys.insert(key);
            } else {
                game.keys.remove(&key);
            }
        }
    }

    fn restore_key(game: &mut Game<'_>, key: KeyCode, before: &mut Option<bool>) {
        if let Some(held) = before.take() {
            if held {
                game.keys.insert(key);
            } else {
                game.keys.remove(&key);
            }
        }
    }

    // Query exactly the normal player's collision context, including contextual shapes and entities.
    fn headroom(game: &mut Game<'_>) -> bool {
        let entities = game.player_entity_collisions();
        let Some(player) = game.player.as_mut() else {
            return false;
        };
        let mut at = player.position;
        at.y += 1.0;
        let bounds = player.dimensions.bounds(at);
        horizon_client::motion::MotionWorld {
            entities: &entities,
            player,
            scene: &mut game.world,
            assets: &game.pack.assets,
            collision_binding: game.collision,
            resources: game.motion,
            health: &game.health_resources,
        }
        .no_player_collision(bounds)
        .unwrap_or(false)
    }

    impl Telly {
        fn random(&mut self, tick: u64) -> f64 {
            self.rng
                .get_or_insert_with(|| rotation::Rng::new(tick.wrapping_mul(0x9e37_79b9) | 1))
                .next()
        }

        fn restore_slot(&mut self, game: &mut Game<'_>) {
            if let Some(slot) = self.slot_before.take()
                && self.swap_back
                && self.slot_used == Some(game.selected)
                && game.player.is_some()
            {
                hold(game, slot);
            }
            self.slot_used = None;
        }

        fn is_cooled(&self, cell: [i32; 3], tick: u64) -> bool {
            self.cooled
                .iter()
                .flatten()
                .any(|&(at, until)| cell == at && tick < until)
        }

        fn watch_results(&mut self, game: &Game<'_>, wait: u64) {
            let tick = game.ticks;
            let Some(eye) = game.eye_position() else {
                return;
            };
            for i in 0..self.watch.len() {
                let Some((cell, at)) = self.watch[i] else {
                    continue;
                };
                let close = (0..3)
                    .map(|a| (f64::from(cell[a]) + 0.5 - eye[a]).powi(2))
                    .sum::<f64>()
                    <= 36.0;
                if tick > at + 20 || !close {
                    self.watch[i] = None;
                } else if !remote::prediction_pending(game, cell) && util::replaceable(game, cell) {
                    self.watch[i] = None;
                    self.cooldown = self.cooldown.max(tick + wait);
                    self.cooled[self.cooled_at] = Some((cell, tick + wait));
                    self.cooled_at = (self.cooled_at + 1) % self.cooled.len();
                    self.rejected += 1;
                }
            }
        }
    }

    // The row's cells a line from the take-off to the landing crosses that a block can still go in, in order.
    fn collect_targets(
        game: &Game<'_>,
        t: &Telly,
        from: [f64; 2],
        end: [f64; 2],
        row: i32,
    ) -> ([[i32; 3]; 24], usize) {
        let mut targets = [[0; 3]; 24];
        let mut count = 0;
        let mut visited = 0;
        let mut add = |cell: [i32; 3]| {
            if count < targets.len()
                && !targets[..count].contains(&cell)
                && util::can_place(game, cell, true)
                && !t.is_cooled(cell, game.ticks)
            {
                targets[count] = cell;
                count += 1;
            }
        };
        normal::line_cells(from, end, |at| {
            let cell = [at[0], row, at[1]];
            add(cell);
            // A line through a corner takes one of the two cells beside it; on a diagonal the other one is often the
            // only one whose side faces the eyes. Either can carry the bridge, so both are aimed at, next to a block.
            for d in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
                let side = [at[0] + d[0], row, at[1] + d[1]];
                let held = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, -1, 0]]
                    .iter()
                    .any(|b| !util::replaceable(game, [side[0] + b[0], side[1] + b[1], side[2] + b[2]]));
                if held {
                    add(side);
                }
            }
            visited += 1;
            visited < 12
        });
        (targets, count)
    }

    pub(crate) fn pre_tick(game: &mut Game<'_>, module: Option<&Module>) {
        // Restoration belongs before every new action, never after a placement in the same tick.
        with(|t| {
            if let Some(m) = module { t.swap_back = m.b("Swap back"); }
            t.restore_slot(game);
        });
        let Some(m) = module else {
            if with(|t| t.active) {
                stop(game);
            }
            return;
        };
        let valid = game.screen == Screen::Playing
            && !game.spectator()
            && game.riding.is_none()
            && game
                .player
                .as_ref()
                .is_some_and(|p| p.health.health() > 0.0 && !p.flying);
        let clicking = game.right || game.physical_buttons.contains(&MouseButton::Right);
        if !valid {
            stop(game);
            return;
        }
        let slot = scaffold_slot(game, m);
        let Some(slot) = slot else {
            stop(game);
            return;
        };
        let room = game.player.as_ref().is_some_and(|p| p.ground.on_ground) && headroom(game);
        BLOCK_USE.with(|c| c.set(true));
        with(|t| bridge(game, m, t, slot, clicking, room));
    }

    fn bridge(
        game: &mut Game<'_>,
        m: &Module,
        t: &mut Telly,
        slot: usize,
        clicking: bool,
        room: bool,
    ) {
        t.active = true;
        t.watch_results(game, m.n("Reject wait (ticks)") as u64);
        let (Some(p), Some(eye)) = (game.player.as_ref(), game.eye_position()) else {
            return;
        };
        let (pos, velocity, ground) = (
            [p.position.x, p.position.y, p.position.z],
            [p.velocity.x, p.velocity.y, p.velocity.z],
            p.ground.on_ground,
        );
        let camera = (game.yaw, game.pitch);
        let forwarded = super::super::sunset_combat_wire::sent_look(game);
        let sent = forwarded.unwrap_or(camera);
        // Your move keys come from the keyboard itself: a key movement correction turned must never read as yours.
        let moves = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD];
        if moves.iter().any(|k| game.physical_keys.contains(k)) {
            for k in moves {
                if game.physical_keys.contains(&k) {
                    game.keys.insert(k);
                } else {
                    game.keys.remove(&k);
                }
            }
        }
        let input = move_input(game);
        // On the ground the look follows your walk, so movement correction turns any move keys into forward: Telly
        // bridges whichever way you walk (strafing too), and sprints doing it.
        let forward = input != (0.0, 0.0);
        let walk = normal::walk_yaw(camera.0, input);
        let jump = game.keys.contains(&KeyCode::Space);
        let keep = m.p("Keep Y") == "Telly" && (!m.b("Keep Y on press") || clicking);
        let automatic = t.run.advance(
            ground,
            pos[1],
            forward,
            jump,
            keep,
            m.p("Tower") == "Telly",
            room,
        );
        // Off an edge without a jump (a low ceiling, a let-go tower) the feet are inside the row by the third tick:
        // the click goes as soon as the turned look has been sent, while they are still above it.
        if !ground && t.run.air <= 2 && velocity[1] < 0.0 {
            t.run.rotation_tick = 0;
        }
        let mode = m.p("Telly rotations");
        let rotate = mode != "None";
        let degrees = rotation::degrees_per_count(f64::from(game.settings.sensitivity));
        let start = t.run.air <= 2 || ground;
        let prefix = if start { "Start speed" } else { "Normal speed" };
        let (a, b) = if prefix == "Start speed" {
            (m.n("Start speed min"), m.n("Start speed max"))
        } else {
            (m.n("Normal speed min"), m.n("Normal speed max"))
        };
        let speed = random_speed(a as f32, b as f32, t.random(game.ticks));

        let speed_effect = game.potions.effects.amplifier("speed").is_some();
        let motion = if !ground {
            "Air motion (%)"
        } else if speed_effect {
            "Speed motion (%)"
        } else {
            "Ground motion (%)"
        };
        // Only below 100%: a scale turns the keys into an analog input, which a full one has no reason to be.
        let scale = (m.n(motion) as f32 / 100.0).clamp(0.01, 1.0);
        horizon_client::motion::sunset_motion::set_input_scale((scale < 1.0).then_some(scale));

        let row = t.run.row(pos[1]);
        if ground { t.origin = Some([pos[0], pos[2]]); }
        let remaining = landing_ticks(pos[1], velocity[1], f64::from(row + 1));
        let horizon = f64::from(remaining.max(2));
        let end = [pos[0] + velocity[0] * horizon, pos[2] + velocity[2] * horizon];
        let landing = [end[0].floor() as i32, row, end[1].floor() as i32];
        let from = t.origin.unwrap_or([pos[0], pos[2]]);
        let (mut targets, mut target_count) = collect_targets(game, t, from, end, row);

        let reach = game.reach();
        let back = rotate.then(|| back_yaws(mode, walk));
        // The planned look clicks only next tick, once it has been forwarded: by then this tick's move has carried the
        // eye by the velocity it starts with, so the look is planned from there.
        let next_eye = [eye[0] + velocity[0], eye[1] + velocity[1], eye[2] + velocity[2]];
        let delay = m.n("Delay (ticks)") as u64;
        let ready = t.run.may_place(ground)
            && game.ticks >= t.cooldown
            && t.last_place.is_none_or(|at| game.ticks >= at + 1 + delay)
            && game.using.is_none()
            && !game.left
            && !game.physical_buttons.contains(&MouseButton::Left);
        let mut placed = false;
        let mut clicked = false;
        if ready && let Some(sent) = forwarded {
            let count = if m.b("Multi place") { 4 } else { 1 };
            for _ in 0..count {
                let Some(click) = normal::placing_ray(game, eye, sent, reach) else {
                    break;
                };
                let hit = super::super::Hit {
                    pos: click.pos,
                    face: click.face,
                    point: click.point,
                };
                let Some(cell) = remote::places_into(game, &hit, slot) else {
                    break;
                };
                if !targets[..target_count].contains(&cell) || t.is_cooled(cell, game.ticks) {
                    break;
                }
                if game.selected != slot {
                    t.slot_before.get_or_insert(game.selected);
                    hold(game, slot);
                }
                t.slot_used = Some(slot);
                PLACING.with(|c| c.set(true));
                remote::use_block(game, hit, sent, m.b("Swing"));
                clicked = true;
                PLACING.with(|c| c.set(false));
                t.clicks += 1;
                if util::replaceable(game, cell) {
                    break;
                }
                t.watch[t.watch_at] = Some((cell, game.ticks));
                t.watch_at = (t.watch_at + 1) % t.watch.len();
                t.last_place = Some(game.ticks);
                if m.b("Render") {
                    scaffold_drawn(cell);
                }
                placed = true;
                // Every extra click recasts exactly the already-forwarded look after native prediction.
            }
        }
        // The look for next tick aims at what is still missing: a cell clicked just now is never aimed at again.
        if placed {
            (targets, target_count) = collect_targets(game, t, from, end, row);
        }
        let targets = &targets[..target_count];
        let found = normal::plan_cells(&mut t.planner, game, targets, next_eye, sent, back, Some(walk), reach);
        if placed && !util::replaceable(game, landing) {
            t.run.phase = Phase::Return;
        }
        if !ground && !util::replaceable(game, landing) {
            t.run.phase = Phase::Return;
        } else if !ground && t.run.phase == Phase::Return {
            // Steering or releasing Keep Y can change the landing row while returning.
            t.run.phase = Phase::Air;
        }
        let forward_look = (walk, camera.1);
        let target = if ground || t.run.phase == Phase::Return {
            forward_look
        } else if let Some(plan) = found {
            t.run.phase = Phase::Place;
            plan.look
        } else {
            (back_yaws(mode, walk)[0], 85.0)
        };
        let mut look_now = camera;
        if rotate {
            // The turn speeds hold only while towering. Bridging goes to the planned look at once (still in whole mouse
            // counts): a capped turn in the air misses the side for ticks and leaves the bridge one block a jump.
            // Up to 300 a tick: unwinding back to a camera turned far round must still land in one move (a whole 360 in
            // one packet is Grim's AimModulo360, so never that).
            let speed = if t.run.towering && jump { speed } else { 300.0 };
            let mut next = if ground || t.run.phase == Phase::Return {
                normal::turn_back(sent, target, speed, degrees)
            } else {
                normal::turn(sent, target, speed, degrees)
            };
            // Grim's DuplicateRotPlace: the yaw turn a place packet rides with (over 2 degrees) must never repeat the last
            // place's exactly. One mouse count more tells them apart.
            if clicked && next != sent {
                let turn = (next.0 - sent.0).abs();
                if turn > 2.0 && t.place_turn.is_some_and(|last| (last - turn).abs() < 0.01) {
                    next.0 += degrees.copysign(next.0 - sent.0);
                }
                t.place_turn = Some((next.0 - sent.0).abs());
            }
            t.look = Some(next);
            look_now = next;
            rotation::scaffold_look(Some(next));
            normal::turn_input(game, &mut t.keys_before, camera.0, next.0);
            horizon_client::motion::sunset_motion::set_move_yaw(Some(next.0));
        } else {
            t.look = None;
            rotation::scaffold_look(None);
            horizon_client::motion::sunset_motion::set_move_yaw(None);
        }
        // The jump and its sprint push go the way of the look this tick's move carries (the movement yaw), so the
        // take-off is judged by that look, not the last one sent: a camera turning faster than the check would never
        // jump and walk off the edge.
        let facing_forward = !rotate || wrap(look_now.0 - walk).abs() <= 4.0;
        let takeoff = ground && automatic && facing_forward;
        if ground && t.run.towering && !facing_forward {
            set_key(game, KeyCode::Space, false, &mut t.jump_before);
        }
        if takeoff {
            set_key(game, KeyCode::Space, true, &mut t.jump_before);
        }
        let sprint = m.p("Telly sprint");
        let force_sprint = takeoff || (t.run.towering && ground) || (sprint == "Legit" && forward);
        if force_sprint {
            set_key(game, KeyCode::ControlLeft, true, &mut t.sprint_before);
        }
        let block_sprint = sprint == "None" && !force_sprint;
        horizon_client::motion::sunset_motion::set_no_sprint(block_sprint);
        if block_sprint {
            set_key(game, KeyCode::ControlLeft, false, &mut t.sprint_before);
        }
    }

    pub(crate) fn post_tick(game: &mut Game<'_>) {
        if !game.native_window() {
            return;
        }
        BLOCK_USE.with(|c| c.set(false));
        with(|t| {
            if let Some(before) = t.keys_before.take() {
                for (key, held) in [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD]
                    .into_iter()
                    .zip(before)
                {
                    if held {
                        game.keys.insert(key);
                    } else {
                        game.keys.remove(&key);
                    }
                }
            }
            restore_key(game, KeyCode::Space, &mut t.jump_before);
            restore_key(game, KeyCode::ControlLeft, &mut t.sprint_before);
            if t.active {
                rotation::scaffold_look(None);
            }
        });
        horizon_client::motion::sunset_motion::set_input_scale(None);
    }

    pub(crate) fn stop(game: &mut Game<'_>) {
        post_tick(game);
        with(|t| {
            t.restore_slot(game);
            let planner = std::mem::take(&mut t.planner);
            *t = Telly {
                planner,
                rng: t.rng,
                clicks: t.clicks,
                rejected: t.rejected,
                ..Telly::default()
            };
        });
        rotation::scaffold_look(None);
        horizon_client::motion::sunset_motion::set_move_yaw(None);
        horizon_client::motion::sunset_motion::set_no_sprint(false);
    }

    pub(crate) fn correction() {
        with(|t| {
            t.run = Run::default();
            t.origin = None;
            t.look = None;
            t.watch = [None; 8];
            t.cooled = [None; 8];
            t.cooldown = 0;
        });
        rotation::scaffold_look(None);
    }

    pub(crate) fn scene_changed(game: &mut Game<'_>) {
        // A selection from another life must never be restored into the new inventory.
        with(|t| { t.slot_before = None; t.slot_used = None; });
        stop(game);
    }

    pub(crate) fn report(out: &mut String) {
        with(|t| {
            if t.clicks > 0 {
                let _ = writeln!(
                    out,
                    "\n## Scaffold (Telly)\n- Native clicks: {}; rejected blocks: {}; phase: {:?}; air ticks: {}; row: {:?}; tower: {}; look: {:?}",
                    t.clicks, t.rejected, t.run.phase, t.run.air, t.run.anchor, t.run.towering, t.look
                );
            }
        });
    }
}

pub(super) use native::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tower_and_bridge_row_survive_the_airborne_ticks() {
        let mut run = Run::default();
        assert!(run.advance(true, 64.0, true, false, true, true, true));
        assert_eq!(run.row(64.0), 63);
        for y in [64.42, 64.75, 65.0, 64.8] {
            run.advance(false, y, true, false, true, true, true);
            assert!(run.towering && run.keep);
            assert_eq!(run.row(y), 63);
        }
        run.advance(true, 64.0, true, false, false, true, true);
        assert!(!run.keep && !run.towering);
    }

    #[test]
    fn keep_on_press_release_releases_the_row_without_clearing_tower_in_air() {
        let mut run = Run::default();
        run.advance(true, 64.0, true, true, false, true, true);
        run.advance(false, 65.1, true, true, false, true, true);
        assert!(run.towering);
        assert_eq!(run.row(65.1), 64);
        run.advance(true, 65.0, false, false, false, true, true);
        assert!(!run.towering);
        run.advance(true, 65.0, true, false, true, true, true);
        run.advance(false, 66.1, true, false, false, true, true);
        assert!(!run.keep);
        assert_eq!(run.row(66.1), 65);
    }

    #[test]
    fn look_limits_use_forwarded_rotation_and_sorted_speeds() {
        assert_eq!(random_speed(107.5, 95.0, 0.0), 95.0);
        assert_eq!(random_speed(107.5, 95.0, 1.0), 107.5);
        assert!((wrap(179.0 - -179.0) + 2.0).abs() < 0.001);
        assert_eq!(back_yaws("Backwards", 0.0)[0], 180.0);
        assert_eq!(back_yaws("Sideways", 0.0)[0].abs(), 135.0);
    }

    #[test]
    fn landing_horizon_plans_through_the_jump_instead_of_only_under_its_midpoint() {
        assert!(landing_ticks(64.42, 0.3332, 64.0) >= 9);
        assert!(landing_ticks(64.8, -0.2, 64.0) <= 4);
        assert_eq!(landing_ticks(64.0, 0.0, 64.0), 0);
    }

    #[test]
    fn placement_waits_for_airborne_rotation_and_never_a_newly_scheduled_look() {
        let mut run = Run::default();
        run.advance(true, 64.0, true, false, true, true, true);
        assert!(!run.may_place(true));
        run.advance(false, 64.42, true, false, true, true, true);
        assert!(!run.may_place(false));
        run.advance(false, 64.75, true, false, true, true, true);
        assert!(!run.may_place(false));
        run.advance(false, 65.0, true, false, true, true, true);
        assert!(run.may_place(false));
        run.phase = Phase::Place;
        assert!(run.may_place(false));
        run.phase = Phase::Return;
        assert!(!run.may_place(false));
    }
}
