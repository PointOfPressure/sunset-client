// Original native sword controller.
// Own melee intentions use short action leases; movement never borrows
// Kill Aura settings or blocks emergency inventory work.
use super::sunset::{self, Category, Module, hold, with};
use super::sunset_combat_control as control;
use super::sunset_sword_controller_policy as fight;
use super::sunset_sword_policy as policy;
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::spear::Kinetic;
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::{
    any::Any,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{event::MouseButton, keyboard::KeyCode};

const NAME: &str = "Sword PvP Bot";
const FOOD: &str = "Sword Bot food";
const KEYS: [KeyCode; 7] = [
    KeyCode::KeyW,
    KeyCode::KeyS,
    KeyCode::KeyA,
    KeyCode::KeyD,
    KeyCode::ControlLeft,
    KeyCode::ShiftLeft,
    KeyCode::Space,
];
type Session = (i32, super::Dim, Arc<dyn Any + Send + Sync>);
pub(super) fn defaults() -> Vec<Module> {
    vec![
    Module::new(NAME,Category::Combat,"Controls its own native melee, spacing, critical jumps and food use against enemy players. Patient and Aggressive change combat decisions; physical input and unsafe terrain yield.")
        .pick("Style",&["Patient","Aggressive"]).num("Fight range",16.0,6.0,32.0,1.0)
        .num("Turn speed",1.0,0.5,1.5,0.05).bool("Strafe",true).bool("Jump crits",true)
        .bool("Jump reset",true).num("Gapple below HP",10.0,0.0,20.0,1.0)
]
}

#[derive(Clone)]
struct InputSave {
    scene: Option<Session>,
    before: [bool; 7],
    physical: [bool; 7],
    applied: [bool; 7],
    right: bool,
    applied_right: bool,
}
struct Food {
    done: control::Finish,
    previous: usize,
    selected: usize,
    began: Instant,
    item: String,
    started: bool,
}
#[derive(Default)]
struct State {
    session: Option<Session>,
    input: Option<InputSave>,
    food: Option<Food>,
    target: Option<u32>,
    brain: fight::Brain,
    side: i8,
    side_carry: f64,
    flip_at: u64,
    flipped_at: u64,
    mouse_until: Option<Instant>,
    pending: Option<Instant>,
    random: Option<LegacyRandom>,
    reason: &'static str,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on).cloned())
}

fn same(game: &Game<'_>, old: &Session) -> bool {
    game.dim == old.1
        && game.remote.as_ref().is_some_and(|r| {
            r.entity_id == old.0
                && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ()
        })
}

fn session(game: &Game<'_>) -> Option<Session> {
    game.remote.as_ref().map(|r| {
        (
            r.entity_id,
            game.dim,
            r.session.counters.clone() as Arc<dyn Any + Send + Sync>,
        )
    })
}

fn stop_food(game: &mut Game<'_>, same_session: bool) {
    if let Some(food) = state(|s| s.food.take()) {
        if same_session && food.done.current() {
            if food.started
                && game.selected == food.selected
                && game.using.is_some_and(|(slot, _)| slot == food.selected)
            {
                game.send(S::PlayerAction(play::PlayerAction {
                    action: 6,
                    pos: [0; 3],
                    direction: 0,
                    sequence: 0,
                }));
                game.using = None;
            }
            if game.selected == food.selected && food.previous < 9 {
                hold(game, food.previous);
            }
        }
    }
}

fn urgent_totem(game: &Game<'_>) -> bool {
    game.offhand.as_ref().is_none_or(|s| s.count == 0)
        && game
            .inventory
            .iter()
            .take(36)
            .filter_map(Option::as_ref)
            .any(|s| &*s.item == "minecraft:totem_of_undying" && s.count > 0)
        && with(|s| s.is_on("Auto Totem") || s.is_on("Auto Inventory Totem"))
}

fn food_tick(game: &mut Game<'_>, want: bool) -> bool {
    if urgent_totem(game) {
        stop_food(game, true);
        return false;
    }
    if let Some(mut food) = state(|s| s.food.take()) {
        let ours = food.done.current()
            && game.selected == food.selected
            && food.began.elapsed() < Duration::from_secs(4)
            && game.held().is_some_and(|s| &*s.item == food.item);
        if !ours || (!want && !food.started) {
            state(|s| s.food = Some(food));
            stop_food(game, true);
            return false;
        }
        if food.started && game.using.is_none() {
            state(|s| s.food = Some(food));
            stop_food(game, true);
            return false;
        }
        if !control::claim(FOOD) {
            state(|s| s.food = Some(food));
            stop_food(game, true);
            return false;
        }
        food.started = true;
        state(|s| s.food = Some(food));
        return true;
    }
    if !want || game.using.is_some() || !control::claim(FOOD) {
        return false;
    }
    let done = control::finish(FOOD);
    let slot = (0..9).find(|&i| {
        game.inventory
            .get(i)
            .and_then(Option::as_ref)
            .is_some_and(|s| {
                s.count > 0
                    && matches!(
                        &*s.item,
                        "minecraft:golden_apple" | "minecraft:enchanted_golden_apple"
                    )
            })
    });
    let Some(slot) = slot else {
        return false;
    };
    let previous = game.selected;
    hold(game, slot);
    let item = game.held().map(|s| s.item.to_string()).unwrap_or_default();
    // Native use predicts the consumption animation and sends its usual item
    // packet. The server alone changes health, effects and inventory count.
    if !super::remote::sunset_combat_remote::use_main(game) {
        if game.selected == slot {
            hold(game, previous);
        }
        return false;
    }
    state(|s| {
        s.food = Some(Food {
            done,
            previous,
            selected: slot,
            began: Instant::now(),
            item,
            started: true,
        })
    });
    true
}

pub(super) fn post_input(game: &mut Game<'_>) {
    if let Some(save) = state(|s| s.input.take()) {
        let current_scene = save.scene.as_ref().is_some_and(|old| same(game, old));
        for (i, key) in KEYS.iter().enumerate() {
            if game.keys.contains(key) == save.applied[i] {
                let physical = game.physical_keys.contains(key);
                let restore = physical || (current_scene && save.before[i] && !save.physical[i]);
                if restore {
                    game.keys.insert(*key);
                } else {
                    game.keys.remove(key);
                }
            }
        }
        if game.right == save.applied_right {
            game.right = game.physical_buttons.contains(&MouseButton::Right)
                || (current_scene && save.right);
        }
    }
}

// Sword does not borrow another module's settings. Keep the runtime API so
// all persistence call sites can project every controller uniformly.
pub(super) fn baseline(_modules: &mut [Module]) {}
pub(super) fn cancel(game: &mut Game<'_>) {
    post_input(game);
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    stop_food(game, connected);
    super::sunset::cancel_turns(NAME);
    state(|s| {
        s.target = None;
        s.brain.cancel();
        s.pending = None;
        s.side_carry = 0.;
    });
}
pub(super) fn editing(game: &mut Game<'_>, name: &str) {
    if name == NAME {
        cancel(game);
    }
}
pub(super) fn disabled(game: &mut Game<'_>) {
    cancel(game);
}
fn pause(game: &mut Game<'_>) {
    cancel(game);
}
pub(super) fn diagnostics() -> String {
    state(|s| {
        format!(
            "SwordBot reason={} target={:?} food={} pending={} air={}",
            s.reason,
            s.target,
            s.food.is_some(),
            s.pending.is_some(),
            s.brain.is_air_sequence()
        )
    })
}
pub(super) fn mouse_input() {
    state(|s| s.mouse_until = Some(Instant::now() + Duration::from_millis(250)));
}
fn random_unit(game: &Game<'_>) -> f64 {
    state(|s| {
        s.random
            .get_or_insert_with(|| {
                LegacyRandom::new(
                    game.ticks as i64
                        ^ std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_nanos() as i64),
                )
            })
            .next_double()
    })
}
fn enemy(game: &Game<'_>, mob: &horizon_client::mobs::Mob) -> bool {
    util::player_enemy(game, mob)
}
fn pick(game: &Game<'_>, m: &Module) -> Option<u32> {
    if game.mobs.len() > policy::POPULATION_LIMIT {
        return None;
    }
    let p = game.player.as_ref()?;
    let old = state(|s| s.target);
    let range = m.n("Fight range").clamp(6., 32.);
    let mut nearest = None;
    for mob in &game.mobs {
        if !enemy(game, mob) {
            continue;
        }
        let distance = p.position.distance_squared(mob.position).sqrt();
        if old == Some(mob.id) && distance <= range + 8. {
            return old;
        }
        if distance <= range && nearest.is_none_or(|(_, d)| distance < d) {
            nearest = Some((mob.id, distance));
        }
    }
    nearest.map(|(id, _)| id)
}
fn melee(game: &Game<'_>, slot: usize) -> bool {
    let Some(stack) = game.inventory.get(slot).and_then(Option::as_ref) else {
        return false;
    };
    let Some(info) = game.res.items().get(&stack.item) else {
        return false;
    };
    if !super::sunset_melee_policy::usable(stack.count, stack.damage, info.max_damage) {
        return false;
    }
    super::sunset_melee_assist::melee_item(game, Some(stack))
}
fn hold_weapon(game: &mut Game<'_>) -> bool {
    if melee(game, game.selected) {
        return true;
    }
    let slot = (0..9).filter(|i| melee(game, *i)).max_by(|a, b| {
        let damage = |i: usize| {
            game.inventory[i]
                .as_ref()
                .and_then(|s| game.res.items().get(&s.item))
                .map_or(0., |i| i.attack_damage)
        };
        damage(*a).total_cmp(&damage(*b)).then_with(|| b.cmp(a))
    });
    if let Some(slot) = slot {
        if !control::claim(NAME) {
            return false;
        }
        let _done = control::finish(NAME);
        hold(game, slot);
        return true;
    }
    false
}
fn physical(game: &Game<'_>) -> bool {
    game.left
        || game.right
        || !game.physical_keys.is_empty()
        || game.physical_buttons.contains(&MouseButton::Left)
        || game.physical_buttons.contains(&MouseButton::Right)
        || state(|s| s.mouse_until.is_some_and(|until| Instant::now() < until))
}
// A bounded native support probe rather than a pathfinder. Ordinary native
// collision still resolves a wall; a grounded collision can request a jump.
fn safe_step(game: &Game<'_>, direction: [f64; 2]) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let length = direction[0].hypot(direction[1]);
    if !length.is_finite() || length == 0. {
        return false;
    }
    let x = p.position.x + direction[0] / length * 0.8;
    let z = p.position.z + direction[1] / length * 0.8;
    for (dx, dz) in [(0., 0.), (-0.2, -0.2), (-0.2, 0.2), (0.2, -0.2), (0.2, 0.2)] {
        let from = Vec3::new(x + dx, p.position.y + 0.1, z + dz);
        if !game
            .world
            .loaded(from.x.floor() as i32, from.z.floor() as i32)
        {
            return false;
        }
        let to = Vec3::new(from.x, p.position.y - 3., from.z);
        let distance = super::spear::sunset_combat_collision::clip(game, from, to)
            .map(|at| p.position.y - at.y);
        let mut hazard = false;
        for down in 0..=3 {
            let pos = [
                from.x.floor() as i32,
                (p.position.y + 0.1).floor() as i32 - down,
                from.z.floor() as i32,
            ];
            let f = util::facts(game, pos);
            hazard |= [
                "lava",
                "fire",
                "soul_fire",
                "magma_block",
                "cactus",
                "sweet_berry_bush",
                "powder_snow",
                "campfire",
                "soul_campfire",
                "wither_rose",
            ]
            .contains(&util::short(&f.name));
        }
        if !fight::ground_safe(distance, hazard) {
            return false;
        }
    }
    true
}
fn available(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && game.remote.is_some()
        && game.native_playing()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && !game.spectator()
        && !physical(game)
        && !super::sunset_render::camera_takes_mouse()
        && game.mining.is_none()
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health() > 0. && !p.fall_flying)
        && game.mobs.len() <= policy::POPULATION_LIMIT
        && !with(|s| s.is_on("Crystal PvP Bot") || s.is_on("Kill Aura") || s.is_on("Trigger Bot"))
}
fn queue_attack(game: &mut Game<'_>, m: &Module, id: u32, attack: fight::Attack, reach: f64) {
    if state(|s| s.pending.is_some())
        || !super::sunset_melee_assist::attack_allowed(game, NAME)
        || !control::claim(NAME)
    {
        return;
    }
    let done = control::finish(NAME);
    let Some(connection) = session(game) else {
        return;
    };
    let selected = game.selected;
    let held = game.held().cloned();
    let snapshot = m.clone();
    let queued = Instant::now();
    let (yaw, pitch) = (game.yaw, game.pitch);
    state(|s| s.pending = Some(queued));
    let accepted = sunset::try_turn_then(NAME, yaw, pitch, move |game| {
        let _done = done;
        // Do not let an old canceled queue clear a newer pending intention.
        if state(|s| s.pending) == Some(queued) {
            state(|s| s.pending = None);
        }
        if !_done.current()
            || queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !available(game)
            || game.selected != selected
            || !super::remote::sunset_combat_remote::sprint_synced(game)
            || game.held() != held.as_ref()
            || !module().is_some_and(|m| m == snapshot)
            || state(|s| s.target) != Some(id)
            || !game.mobs.iter().any(|t| t.id == id && enemy(game, t))
        {
            return;
        }
        let plan = super::sunset_melee_assist::preferred_weapon(game, id);
        let slot = plan.map_or(game.selected, |p| p.slot);
        if !super::sunset_melee_assist::slot_strength(game, slot, 0.5)
            .is_some_and(|s| s >= attack.minimum_charge)
        {
            return;
        }
        if attack.falling_critical
            && (!game.player.as_ref().is_some_and(|p| p.velocity.y < 0.)
                || !game
                    .critical_attack(game.hand_strength.scale(0.5, game.main_hand_attack_speed())))
        {
            return;
        }
        if !super::sunset_melee_assist::hits_target(game, id, yaw, pitch, false, 3.) {
            return;
        }
        if let Some(plan) = plan {
            hold(game, plan.slot);
        }
        // The reference Sword attack is an ordinary melee attack; a held
        // piercing weapon must not be silently converted into a spear jab.
        let sent = !game.held().is_some_and(|s| Kinetic::of(&s.item).is_some())
            && super::sunset_melee_assist::send_hit_at(game, id, yaw, pitch);
        if plan.is_some_and(|p| p.restore) && game.selected == slot {
            hold(game, selected);
        }
        if sent {
            let jitter = random_unit(game) * 0.12 - 0.06;
            state(|s| s.brain.accepted(reach, jitter));
        }
    });
    if !accepted {
        state(|s| s.pending = None);
    }
}
fn face(game: &mut Game<'_>, id: u32, speed: f64) {
    let Some(mob) = game.mobs.iter().find(|t| t.id == id) else {
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let drift = mob.position.subtract(mob.previous);
    let raw = mob.bounds().move_by(drift);
    let lo = raw.min_position();
    let hi = raw.max_position();
    let min = [
        lo.x + (hi.x - lo.x) * 0.2,
        lo.y + (hi.y - lo.y) * 0.15,
        lo.z + (hi.z - lo.z) * 0.2,
    ];
    let max = [
        hi.x - (hi.x - lo.x) * 0.2,
        hi.y - (hi.y - lo.y) * 0.15,
        hi.z - (hi.z - lo.z) * 0.2,
    ];
    let at = std::array::from_fn(|i| eye[i].clamp(min[i], max[i]));
    let wanted = sunset::look_at(eye, at);
    let yaw = f64::from((wanted.0 - game.yaw + 180.).rem_euclid(360.) - 180.);
    let pitch = f64::from(wanted.1 - game.pitch);
    let cap = if yaw.abs() > 40. { 27. } else { 22. } * speed.clamp(0.5, 1.5);
    let scale = (cap / yaw.hypot(pitch).max(cap)).min(1.);
    game.turn_player(yaw * scale / 0.15, pitch * scale / 0.15);
}
pub(super) fn pre_input(game: &mut Game<'_>) {
    post_input(game);
    let Some(m) = module() else {
        cancel(game);
        state(|s| *s = State::default());
        return;
    };
    if !state(|s| s.session.as_ref().is_some_and(|old| same(game, old))) {
        cancel(game);
        state(|s| {
            *s = State::default();
            s.session = session(game);
            s.side = 1;
        });
    }
    if !available(game) {
        pause(game);
        return;
    }
    if control::owner().is_some_and(|o| o != NAME && o != FOOD) {
        pause(game);
        return;
    }
    if let Some(at) = state(|s| s.pending) {
        if at.elapsed() > Duration::from_millis(200) {
            cancel(game);
        } else {
            let _ = control::claim(NAME);
        }
        return;
    }
    let Some(id) = pick(game, &m) else {
        pause(game);
        return;
    };
    if state(|s| s.target) != Some(id) {
        let jitter = random_unit(game) * 0.12 - 0.06;
        state(|s| {
            s.target = Some(id);
            s.brain.set_fight(id, jitter);
        });
    }
    face(game, id, m.n("Turn speed"));
    let p = game.player.as_ref().unwrap();
    let low = f64::from(p.health.health()) <= m.n("Gapple below HP") && p.health.absorption() < 4.;
    let eating = food_tick(game, low || state(|s| s.food.is_some()));
    if game.using.is_some() && !eating {
        pause(game);
        return;
    }
    if eating {
        apply_drive(
            game,
            fight::Drive {
                forward: -1,
                ..fight::Drive::default()
            },
            true,
        );
        return;
    }
    if !hold_weapon(game) {
        return;
    }
    let Some(t) = game.mobs.iter().find(|t| t.id == id) else {
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let box_ = t.bounds();
    let lo = box_.min_position();
    let hi = box_.max_position();
    let Some(reach) =
        super::sunset_melee_policy::box_distance(eye, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
    else {
        return;
    };
    let p = game.player.as_ref().unwrap();
    let own = p.bounds();
    let a = own.min_position();
    let b = own.max_position();
    let their_eye = Vec3::new(t.position.x, t.position.y + t.eye_height(), t.position.z);
    let Some(their_reach) = super::sunset_melee_policy::box_distance(
        [their_eye.x, their_eye.y, their_eye.z],
        [a.x, a.y, a.z],
        [b.x, b.y, b.z],
    ) else {
        return;
    };
    let to_self = Vec3::new(p.position.x, p.position.y + 1.1, p.position.z).subtract(their_eye);
    let dir = super::view_vector(t.head_yaw, t.head_pitch);
    let len = to_self.distance_squared(Vec3::ZERO).sqrt();
    if len <= 0. {
        return;
    }
    let facing = ((to_self.x * dir[0] + to_self.y * dir[1] + to_self.z * dir[2]) / len)
        .clamp(-1., 1.)
        .acos()
        .to_degrees();
    let away = t.position.subtract(p.position);
    let length = away.x.hypot(away.z);
    let motion = t.position.subtract(t.previous);
    let fleeing = if length > 0. {
        (motion.x * away.x + motion.z * away.z) / length
    } else {
        0.
    };
    let speed = game.main_hand_attack_speed();
    let charge = game.hand_strength.scale(0.5, speed);
    let ready_in = ((1. - charge) * horizon_client::attack_strength::delay(speed))
        .ceil()
        .max(0.) as i32;
    let view = fight::View {
        tick: game.ticks,
        target: id,
        reach,
        their_reach,
        height_difference: t.position.y - p.position.y,
        ready_in,
        charge,
        facing,
        fleeing,
        latency_ms: super::remote::sunset_combat_remote::latency(game).unwrap_or(0),
        own_hurt: p.health.hurt.hurt_time,
        immune: t.hurt_time > 1,
        shield: t.extra.blocking,
        grounded: p.ground.on_ground,
        water: p.fluids.was_touching_water,
        vertical_speed: p.velocity.y,
        aggressive: m.p("Style") == "Aggressive",
        strafe: m.b("Strafe"),
        jump_crits: m.b("Jump crits"),
        jump_reset: m.b("Jump reset"),
    };
    let Some(drive) = state(|s| s.brain.plan(view)) else {
        return;
    };
    if game.ticks >= state(|s| s.flip_at) {
        let gap = 10 + (random_unit(game) * 13.).floor() as u64;
        state(|s| {
            s.side = -s.side;
            s.flip_at = game.ticks.saturating_add(gap);
        });
    }
    if drive.flip_side && game.ticks.saturating_sub(state(|s| s.flipped_at)) >= 8 {
        let gap = 10 + (random_unit(game) * 13.).floor() as u64;
        state(|s| {
            s.side = -s.side;
            s.flipped_at = game.ticks;
            s.flip_at = game.ticks.saturating_add(gap);
        });
    }
    apply_drive(game, drive, false);
    if let Some(attack) = drive.attack {
        queue_attack(game, &m, id, attack, reach);
    }
    state(|s| s.reason = "independent native melee and spacing");
}
fn apply_drive(game: &mut Game<'_>, mut drive: fight::Drive, eating: bool) {
    let yaw = f64::from(game.yaw).to_radians();
    let ahead = [-yaw.sin(), yaw.cos()];
    let left = [yaw.cos(), yaw.sin()];
    if drive.forward != 0
        && !safe_step(
            game,
            [
                ahead[0] * f64::from(drive.forward),
                ahead[1] * f64::from(drive.forward),
            ],
        )
    {
        drive.forward = 0;
        drive.sprint = false;
    }
    if drive.forward > 0
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.ground.on_ground && p.collision.horizontal)
    {
        drive.jump = true;
    }
    let side = state(|s| {
        s.side_carry += drive.strafe.clamp(0., 1.);
        if s.side_carry >= 1. {
            s.side_carry -= 1.;
            s.side
        } else {
            0
        }
    });
    let mut side = side;
    if side != 0 && !safe_step(game, [left[0] * f64::from(side), left[1] * f64::from(side)]) {
        side = -side;
        state(|s| s.side = side);
        if !safe_step(game, [left[0] * f64::from(side), left[1] * f64::from(side)]) {
            side = 0;
        }
    }
    let before = KEYS.map(|k| game.keys.contains(&k));
    let physical = KEYS.map(|k| game.physical_keys.contains(&k));
    let right = game.right;
    for k in KEYS {
        game.keys.remove(&k);
    }
    if drive.forward > 0 {
        game.keys.insert(KeyCode::KeyW);
    } else if drive.forward < 0 {
        game.keys.insert(KeyCode::KeyS);
    }
    if side > 0 {
        game.keys.insert(KeyCode::KeyA);
    } else if side < 0 {
        game.keys.insert(KeyCode::KeyD);
    }
    if drive.sprint && !drive.drop_sprint {
        game.keys.insert(KeyCode::ControlLeft);
    }
    if drive.drop_sprint {
        super::sunset_melee_assist::drop_sprint(game);
    }
    if drive.jump
        && game.player.as_ref().is_some_and(|p| {
            p.ground.on_ground
                && !p.fluids.was_touching_water
                && !p.is_in_lava()
                && p.last_climbable.is_none()
        })
    {
        game.keys.insert(KeyCode::Space);
    }
    game.right = eating;
    let applied = KEYS.map(|k| game.keys.contains(&k));
    let scene = session(game);
    state(|s| {
        s.input = Some(InputSave {
            scene,
            before,
            physical,
            applied,
            right,
            applied_right: eating,
        })
    });
}
