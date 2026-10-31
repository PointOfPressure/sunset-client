// ORIGINAL NATIVE DRAFT. Keep outside active mixins until integration is approved.
// Controller context owns only selected Kill Aura settings; hand actions use
// the separate short-lived combat lease so walking never blocks inventory work.
use super::sunset::{self, Category, Module, Value, hold, with};
use super::sunset_combat_control as control;
use super::sunset_sword_policy as policy;
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::{collisions::SceneCollisions};
use horizon_core::geometry::Vec3;
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
    Module::new(NAME,Category::Combat,"Controls native movement and Kill Aura for nearby enemy players. Patient gives space while charging; Aggressive closes sooner. Stops for physical movement input and unsafe steps.")
        .pick("Style",&["Patient","Aggressive"]).num("Fight range",16.0,6.0,32.0,1.0)
        .num("Turn speed",1.0,0.5,1.5,0.05).bool("Strafe",true).bool("Jump crits",true)
        .bool("Jump reset",true).num("Gapple below HP",10.0,0.0,20.0,1.0)
]
}

struct AuraLease {
    on: policy::Owned<bool>,
    fields: Vec<(&'static str, policy::Owned<Value>)>,
}
impl AuraLease {
    fn new(m: &mut Module) -> Self {
        let on = policy::Owned::new(m.on, true);
        m.on = true;
        let desired = [
            (
                "Attack when holding",
                Value::Pick {
                    i: 0,
                    of: &["Weapons", "All"],
                },
            ),
            (
                "Selected weapon types",
                Value::Text("diamond_sword,diamond_axe".to_owned()),
            ),
            ("Entities", Value::Text("player".to_owned())),
            ("Auto switch", Value::Bool(true)),
            ("Swap back", Value::Bool(true)),
            ("Only on click", Value::Bool(false)),
            ("Only on look", Value::Bool(false)),
        ];
        let mut fields = Vec::new();
        for (name, next) in desired {
            if let Some(s) = m.settings.iter_mut().find(|s| s.name == name) {
                let old = s.value.clone();
                s.value = next.clone();
                fields.push((name, policy::Owned::new(old, next)));
            }
        }
        for (name, want) in [("Rotate", "On hit"), ("Crits", "Off")] {
            if let Some(s) = m.settings.iter_mut().find(|s| s.name == name) {
                if let Value::Pick { i, of } = &mut s.value {
                    if let Some(index) = of.iter().position(|x| *x == want) {
                        let old = Value::Pick { i: *i, of: *of };
                        *i = index;
                        fields.push((name, policy::Owned::new(old, s.value.clone())));
                    }
                }
            }
        }
        if let Some(s) = m.settings.iter_mut().find(|s| s.name == "Range") {
            let old = s.value.clone();
            if let Value::Num { v, .. } = &mut s.value {
                *v = 3.0;
                fields.push(("Range", policy::Owned::new(old, s.value.clone())));
            }
        }
        Self { on, fields }
    }
    fn apply(&mut self, m: &mut Module, attack: bool) -> bool {
        let mut owned = self.on.update(&mut m.on, attack);
        for (name, lease) in &mut self.fields {
            if let Some(s) = m.settings.iter_mut().find(|s| s.name == *name) {
                let next = lease.applied.clone();
                owned &= lease.update(&mut s.value, next);
            } else {
                owned = false;
            }
        }
        owned
    }
    fn restore(self, m: &mut Module) {
        self.on.restore(&mut m.on);
        for (name, lease) in self.fields {
            if let Some(s) = m.settings.iter_mut().find(|s| s.name == name) {
                lease.restore(&mut s.value);
            }
        }
    }
}
#[derive(Clone, Copy)]
struct InputSave {
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
    aura: Option<AuraLease>,
    input: Option<InputSave>,
    food: Option<Food>,
    target: Option<u32>,
    side: i8,
    flip_at: u64,
    previous_distance: Option<f64>,
    closing: f64,
    charge: f32,
    tap_until: u64,
    reset_until: u64,
    crit_jump: bool,
    yielded: bool,
    mouse_until: Option<Instant>,
    last_food: u64,
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

fn restore_aura() {
    if let Some(lease) = state(|s| s.aura.take()) {
        with(|s| {
            if let Some(m) = s.modules.iter_mut().find(|m| m.name == "Kill Aura") {
                lease.restore(m);
            }
        });
    }
}
fn own_aura(attack: bool) -> bool {
    let mut lease = state(|s| s.aura.take());
    let okay = with(|s| {
        let Some(m) = s.modules.iter_mut().find(|m| m.name == "Kill Aura") else {
            return false;
        };
        let own = lease.get_or_insert_with(|| AuraLease::new(m));
        own.apply(m, attack)
    });
    state(|s| s.aura = lease);
    okay
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
pub(super) fn cancel(game: &mut Game<'_>) {
    post_input(game);
    let current = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    stop_food(game, current);
    restore_aura();
    state(|s| {
        s.target = None;
        s.crit_jump = false;
        s.previous_distance = None;
        s.tap_until = 0;
        s.reset_until = 0;
    });
}
pub(super) fn diagnostics() -> String {
    state(|s| {
        format!(
            "SwordBot reason={} target={:?} side={} food={} context={} yielded={}",
            s.reason,
            s.target,
            s.side,
            s.food.is_some(),
            s.aura.is_some(),
            s.yielded
        )
    })
}
pub(super) fn mouse_input() {
    state(|s| s.mouse_until = Some(Instant::now() + Duration::from_millis(250)));
}
pub(super) fn note_velocity(game: &Game<'_>, id: i32, v: [f64; 3]) {
    if module().is_some()
        && game.remote.as_ref().is_some_and(|r| r.entity_id == id)
        && v.iter().all(|n| n.is_finite())
        && v[1] > 0.0
        && v[0].hypot(v[2]) >= 0.1
    {
        state(|s| s.reset_until = game.ticks.saturating_add(3));
    }
}
fn enemy(game: &Game<'_>, mob: &horizon_client::mobs::Mob) -> bool {
    if mob.kind.id != "minecraft:player"
        || !mob.alive()
        || mob.removed
        || !mob.health.is_finite()
        || mob.health <= 0.0
        || game
            .remote
            .as_ref()
            .is_some_and(|r| mob.id == r.entity_id as u32)
    {
        return false;
    }
    if game
        .remote_player_name(mob.id)
        .is_some_and(|n| with(|s| s.friends.iter().any(|f| f.eq_ignore_ascii_case(n))))
    {
        return false;
    }
    !horizon_client::scoreboard::teams_allied(
        game.score.board.players_team(&game.options.player_name()),
        game.mob_team(mob.id),
    )
}
fn pick(game: &Game<'_>, m: &Module) -> Option<u32> {
    if game.mobs.len() > policy::POPULATION_LIMIT {
        return None;
    }
    let eye = game.eye_position()?;
    let old = state(|s| s.target);
    let mut best = None;
    for mob in &game.mobs {
        if !enemy(game, mob) {
            continue;
        }
        let t = util::mob_target(mob);
        let Some(d) = super::sunset_melee_policy::box_distance(eye, t.lo, t.hi) else {
            return None;
        };
        if d > m.n("Fight range").clamp(6.0, 32.0) {
            continue;
        }
        if old == Some(mob.id) {
            return old;
        }
        if best.is_none_or(|(_, old)| d < old) {
            best = Some((mob.id, d));
        }
    }
    best.map(|(id, _)| id)
}
// A short native collision sweep and five actual support rays. This is a
// local step check, not pathfinding; unsupported/unknown terrain stops input.
fn safe_step(game: &Game<'_>, direction: [f64; 2]) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let length = direction[0].hypot(direction[1]);
    if !length.is_finite() || length < 1e-5 {
        return false;
    }
    let dx = direction[0] / length * 0.5;
    let dz = direction[1] / length * 0.5;
    let b = p.bounds();
    let lo = b.min_position();
    let hi = b.max_position();
    if ![lo.x, lo.y, lo.z, hi.x, hi.y, hi.z]
        .iter()
        .all(|v| v.is_finite())
        || hi.x - lo.x > 2.0
        || hi.z - lo.z > 2.0
        || hi.y - lo.y > 4.0
    {
        return false;
    }
    let swept = b.expand_towards(Vec3::new(dx, 0.0, dz));
    let body_clear = SceneCollisions {
        world: &game.world,
        binding: game.collision,
        subject: None,
    }
    .unobstructed(swept)
    .unwrap_or(false);
    if !body_clear {
        return false;
    }
    let x = p.position.x + dx;
    let z = p.position.z + dz;
    for (a, c) in [
        (0.0, 0.0),
        (-0.2, -0.2),
        (-0.2, 0.2),
        (0.2, -0.2),
        (0.2, 0.2),
    ] {
        let from = Vec3::new(x + a, p.position.y + 0.05, z + c);
        let to = Vec3::new(from.x, p.position.y - 1.25, from.z);
        let support = super::spear::sunset_combat_collision::clip(game, from, to).map(|hit| p.position.y - hit.y);
        let mut hazard = false;
        for down in 0..=2 {
            let pos = [
                from.x.floor() as i32,
                (p.position.y + 0.1).floor() as i32 - down,
                from.z.floor() as i32,
            ];
            let f = util::facts(game, pos);
            hazard |= f.fluid_level > 0
                || [
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
        if !policy::support_safe(body_clear, support, hazard) {
            return false;
        }
    }
    true
}

fn urgent_totem(game: &Game<'_>) -> bool {
    game.offhand
        .as_ref()
        .is_none_or(|s| &*s.item != "minecraft:totem_of_undying")
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
            state(|s| s.last_food = game.ticks);
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
    if !want
        || game.using.is_some()
        || game.ticks.saturating_sub(state(|s| s.last_food)) < 20
        || !control::claim(FOOD)
    {
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

pub(super) fn pre_input(game: &mut Game<'_>) {
    post_input(game);
    let Some(m) = module() else {
        cancel(game);
        state(|s| {
            s.yielded = false;
            s.session = None;
        });
        return;
    };
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    if !connected {
        cancel(game);
        state(|s| {
            s.session = session(game);
            s.yielded = false;
            s.side = 1;
        });
    }
    let live = game.native_window()
        && game.native_on(sunset::ID)
        && game.remote.is_some()
        && game.native_playing()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && !game.spectator()
        && game.riding.is_none()
        && game.player.as_ref().is_some_and(|p| {
            p.health.health().is_finite() && p.health.health() > 0.0 && !p.flying && !p.fall_flying
        })
        && game.mobs.len() <= policy::POPULATION_LIMIT;
    let manual = KEYS.iter().any(|k| game.physical_keys.contains(k))
        || game.physical_buttons.contains(&MouseButton::Left)
        || game.physical_buttons.contains(&MouseButton::Right)
        || state(|s| s.mouse_until.is_some_and(|until| Instant::now() < until));
    if !live || manual || game.mining.is_some() || state(|s| s.yielded) {
        cancel(game);
        state(|s| s.reason = "paused for world, physical input or user override");
        return;
    }
    // Other hand owners may refill inventory while this controller waits.
    if control::owner().is_some_and(|owner| owner != "Kill Aura" && owner != FOOD) {
        restore_aura();
        return;
    }
    let Some(id) = pick(game, &m) else {
        cancel(game);
        state(|s| s.reason = "no enemy player in fight range");
        return;
    };
    if state(|s| s.target) != Some(id) {
        state(|s| {
            s.target = Some(id);
            s.previous_distance = None;
            s.crit_jump = false;
            s.tap_until = 0;
        });
    }
    let Some(target) = game.mobs.iter().find(|t| t.id == id) else {
        return;
    };
    let t = util::mob_target(target);
    let Some(eye) = game.eye_position() else {
        return;
    };
    let Some(distance) = super::sunset_melee_policy::box_distance(eye, t.lo, t.hi) else {
        return;
    };
    let target_eye = [t.pos[0], t.hi[1] - 0.15, t.pos[2]];
    let p = game.player.as_ref().unwrap();
    let own = p.bounds();
    let a = own.min_position();
    let b = own.max_position();
    let Some(enemy_distance) =
        super::sunset_melee_policy::box_distance(target_eye, [a.x, a.y, a.z], [b.x, b.y, b.z])
    else {
        return;
    };
    let desired = super::sunset::look_at(
        eye,
        [
            (t.lo[0] + t.hi[0]) * 0.5,
            t.lo[1] + (t.hi[1] - t.lo[1]) * 0.7,
            (t.lo[2] + t.hi[2]) * 0.5,
        ],
    );
    let facing =
        super::sunset::look_at(target_eye, [p.position.x, p.position.y + 1.0, p.position.z]).0;
    let enemy_facing =
        f64::from((facing - target.head_yaw + 180.0).rem_euclid(360.0) - 180.0).abs();
    let charge = game.hand_strength.scale(0.5, game.main_hand_attack_speed());
    let grounded = p.ground.on_ground;
    let velocity_y = p.velocity.y;
    let target_hurt = target.hurt_time > 1;
    let low = p.health.health() as f64 <= m.n("Gapple below HP") && p.health.absorption() < 4.0;
    let health = if low { p.health.health() } else { 21.0 };
    let can_jump = !p.swimming
        && !p.fluids.was_touching_water
        && !p.is_in_lava()
        && p.last_climbable.is_none();
    state(|s| {
        if let Some(old) = s.previous_distance {
            s.closing = s.closing * 0.6 + (old - enemy_distance) * 0.4;
        }
        s.previous_distance = Some(enemy_distance);
        if game.ticks >= s.flip_at {
            s.side = -s.side;
            s.flip_at = game.ticks.saturating_add(16);
        }
        if charge < s.charge - 0.2 {
            s.tap_until = game.ticks.saturating_add(1);
        }
        s.charge = charge;
        if grounded {
            s.crit_jump = false;
        }
    });
    let view = policy::View {
        distance,
        enemy_distance,
        closing: state(|s| s.closing),
        enemy_facing,
        charge,
        health,
        eating: state(|s| s.food.is_some()),
        hurt: game.ticks < state(|s| s.reset_until),
        immune: target_hurt,
        shielded: target.extra.blocking,
        grounded,
        can_jump,
        aggressive: m.p("Style") == "Aggressive",
        strafe: m.b("Strafe"),
        jump_crits: m.b("Jump crits"),
        jump_reset: m.b("Jump reset"),
        eat_below: m.n("Gapple below HP"),
        side: state(|s| s.side),
    };
    let Some(mut decision) = policy::decide(view) else {
        return;
    };
    let eating = food_tick(game, decision.eat);
    if game.using.is_some() && !eating {
        restore_aura();
        return;
    }
    if decision.jump {
        state(|s| s.crit_jump = true);
    }
    if game.ticks < state(|s| s.tap_until) {
        decision.forward = 0;
        decision.sprint = false;
    }
    let crit_air = state(|s| s.crit_jump) && !grounded;
    if crit_air {
        decision.sprint = false;
        if velocity_y < 0.0 {
            decision.forward = 0;
        }
    }
    let attack = decision.attack && !eating && (!crit_air || velocity_y < 0.0);
    if !own_aura(attack) {
        cancel(game);
        state(|s| {
            s.yielded = true;
            s.reason = "Kill Aura was edited by user";
        });
        return;
    }
    if let Some(step) = policy::camera_step(
        [game.yaw, game.pitch],
        [desired.0, desired.1],
        m.n("Turn speed"),
    ) {
        game.turn_player(step[0] / 0.15, step[1] / 0.15);
    }
    let angle = f64::from(game.yaw).to_radians();
    let ahead = [-angle.sin(), angle.cos()];
    let left = [angle.cos(), angle.sin()];
    if decision.forward != 0
        && !safe_step(
            game,
            [
                ahead[0] * f64::from(decision.forward),
                ahead[1] * f64::from(decision.forward),
            ],
        )
    {
        decision.forward = 0;
        decision.sprint = false;
    }
    if decision.side != 0
        && !safe_step(
            game,
            [
                left[0] * f64::from(decision.side),
                left[1] * f64::from(decision.side),
            ],
        )
    {
        state(|s| s.side = -s.side);
        decision.side = 0;
    }
    let before = KEYS.map(|k| game.keys.contains(&k));
    let physical = KEYS.map(|k| game.physical_keys.contains(&k));
    let right = game.right;
    for key in KEYS {
        game.keys.remove(&key);
    }
    if decision.forward > 0 {
        game.keys.insert(KeyCode::KeyW);
    } else if decision.forward < 0 {
        game.keys.insert(KeyCode::KeyS);
    }
    if decision.side > 0 {
        game.keys.insert(KeyCode::KeyA);
    } else if decision.side < 0 {
        game.keys.insert(KeyCode::KeyD);
    }
    if decision.sprint {
        game.keys.insert(KeyCode::ControlLeft);
    }
    if decision.jump {
        game.keys.insert(KeyCode::Space);
    }
    game.right = eating;
    let applied = KEYS.map(|k| game.keys.contains(&k));
    state(|s| {
        s.input = Some(InputSave {
            before,
            physical,
            applied,
            right,
            applied_right: eating,
        });
        s.reason = if eating {
            "retreat and native food use"
        } else if attack {
            "native melee controller"
        } else {
            "spacing while charging"
        };
    });
}
pub(super) fn post_input(game: &mut Game<'_>) {
    if let Some(save) = state(|s| s.input.take()) {
        for (i, key) in KEYS.iter().enumerate() {
            if game.keys.contains(key) == save.applied[i] {
                let physical = game.physical_keys.contains(key);
                let restore = physical || (save.before[i] && !save.physical[i]);
                if restore {
                    game.keys.insert(*key);
                } else {
                    game.keys.remove(key);
                }
            }
        }
        if game.right == save.applied_right {
            game.right = game.physical_buttons.contains(&MouseButton::Right) || save.right;
        }
    }
}
