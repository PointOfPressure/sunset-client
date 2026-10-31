// Original native Crystal PvP controller.
use super::sunset::{self, Category, Module, Value, hold, with};
use super::sunset_combat_control as control;
use super::sunset_crystal_bot_policy as policy;
use super::sunset_explosive_damage::{Blast, local_damage, safety_damage};
use super::sunset_explosive_policy::Budget;
use super::sunset_util as util;
use super::{Dim, Game, Screen};
use horizon_client::collisions::SceneCollisions;
use horizon_core::geometry::Vec3;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::{
    any::Any,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{event::MouseButton, keyboard::KeyCode};
const NAME: &str = "Crystal PvP Bot";
const FOOD: &str = "Crystal Bot food";
const KEYS: [KeyCode; 7] = [
    KeyCode::KeyW,
    KeyCode::KeyS,
    KeyCode::KeyA,
    KeyCode::KeyD,
    KeyCode::ControlLeft,
    KeyCode::ShiftLeft,
    KeyCode::Space,
];
type Session = (i32, Dim, Arc<dyn Any + Send + Sync>);
pub(super) fn defaults() -> Vec<Module> {
    vec![
    Module::new(NAME,Category::Combat,"Coordinates native Crystal/Anchor Aura with safe local movement, observed threat retreat, totem utility and real gapple use. Keeps the auras' configured damage safety limits.")
        .num("Fight range",16.0,8.0,32.0,1.0).bool("Use anchors",true).bool("Keep totem",true)
        .bool("Strafe",true).num("Turn speed",1.0,0.5,1.5,0.05)
        .num("Gapple below HP",14.0,0.0,20.0,1.0)
]
}

struct ModuleLease {
    name: &'static str,
    on: policy::Owned<bool>,
    fields: Vec<(&'static str, policy::Owned<Value>)>,
}
impl ModuleLease {
    fn acquire(m: &mut Module, enabled: bool) -> Self {
        let on = policy::Owned::new(m.on, enabled);
        m.on = enabled;
        let mut fields = Vec::new();
        let desired = if m.name == "Auto Totem" {
            vec![("Use hotbar totems", Value::Bool(true))]
        } else {
            vec![("Entities", Value::Text("player".to_owned()))]
        };
        for (name, next) in desired {
            if let Some(s) = m.settings.iter_mut().find(|s| s.name == name) {
                let old = s.value.clone();
                s.value = next.clone();
                fields.push((name, policy::Owned::new(old, next)));
            }
        }
        Self {
            name: m.name,
            on,
            fields,
        }
    }
    fn apply(&mut self, m: &mut Module, enabled: bool) -> bool {
        let mut owned = self.on.update(&mut m.on, enabled);
        for (name, lease) in &mut self.fields {
            if let Some(s) = m.settings.iter_mut().find(|s| s.name == *name) {
                owned &= lease.update(&mut s.value, lease.applied.clone());
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
    fn baseline(&self, m: &mut Module) {
        self.on.baseline(&mut m.on);
        for (name, lease) in &self.fields {
            if let Some(s) = m.settings.iter_mut().find(|s| s.name == *name) {
                lease.baseline(&mut s.value);
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
    contexts: Vec<ModuleLease>,
    input: Option<InputSave>,
    food: Option<Food>,
    target: Option<u32>,
    side: i8,
    flip_at: u64,
    yielded: bool,
    mouse_until: Option<Instant>,
    last_food: u64,
    brain: policy::Brain,
    phase: Option<policy::Phase>,
    reason: &'static str,
    scan_origin: Option<[i32; 3]>,
    cursor: usize,
    scan_good: bool,
    scan_complete: bool,
    anchors: Vec<[i32; 3]>,
    threat_at: u64,
    threat_origin: Option<[f64; 3]>,
    threat_signature: u64,
    threat: Option<f64>,
    threat_complete: bool,
    corrections: Vec<Instant>,
    pause_until: Option<Instant>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}
fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on).cloned())
}

// No runtime mutex is acquired here. Pass a detached clone, never the live
// module slice, so saving cannot modify gameplay or consume context leases.
pub(super) fn baseline(modules: &mut [Module]) {
    state(|s| {
        for lease in &s.contexts {
            if let Some(m) = modules.iter_mut().find(|m| m.name == lease.name) {
                lease.baseline(m);
            }
        }
    });
}
fn restore_contexts() {
    let contexts = state(|s| std::mem::take(&mut s.contexts));
    with(|s| {
        for lease in contexts {
            if let Some(m) = s.modules.iter_mut().find(|m| m.name == lease.name) {
                lease.restore(m);
            }
        }
    });
}
fn coordinate(combat: bool, anchors: bool, totem: bool) -> bool {
    let mut contexts = state(|s| std::mem::take(&mut s.contexts));
    let okay = with(|s| {
        let inventory = s.is_on("Auto Inventory Totem");
        let mut owned = true;
        for (name, want) in [
            ("Crystal Aura", combat),
            ("Anchor Aura", combat && anchors),
            ("Auto Totem", totem && !inventory),
        ] {
            let Some(m) = s.modules.iter_mut().find(|m| m.name == name) else {
                if name != "Auto Totem" {
                    owned = false;
                }
                continue;
            };
            if name == "Auto Totem" && !totem && !contexts.iter().any(|c| c.name == name) {
                continue;
            }
            if let Some(lease) = contexts.iter_mut().find(|c| c.name == name) {
                let enabled = if name == "Auto Totem" && !totem {
                    lease.on.before
                } else {
                    want
                };
                owned &= lease.apply(m, enabled);
            } else {
                contexts.push(ModuleLease::acquire(m, want));
            }
        }
        owned
    });
    state(|s| s.contexts = contexts);
    okay
}
pub(super) fn cancel(game: &mut Game<'_>) {
    post_input(game);
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    stop_food(game, connected);
    restore_contexts();
    state(|s| {
        s.target = None;
        s.phase = None;
    });
}
fn pause(game: &mut Game<'_>, keep_totem: bool) {
    post_input(game);
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    stop_food(game, connected);
    // Keep an owned false field distinct from having no lease. This preserves
    // the last-applied values while chat/settings are open, so a user edit is
    // detected rather than adopted as a fresh controller baseline on resume.
    if state(|s| !s.contexts.is_empty()) && !coordinate(false, false, keep_totem) {
        restore_contexts();
        state(|s| {
            s.yielded = true;
            s.reason = "coordinated module edited while paused";
        });
    }
    state(|s| s.phase = None);
}
pub(super) fn diagnostics() -> String {
    state(|s| {
        format!(
            "CrystalBot reason={} phase={:?} target={:?} food={} contexts={} sampled_threat={:?} threat_complete={} anchor_cursor={} yielded={}",
            s.reason,
            s.phase,
            s.target,
            s.food.is_some(),
            s.contexts.len(),
            s.threat,
            s.threat_complete,
            s.cursor,
            s.yielded
        )
    })
}
pub(super) fn note_totem(game: &Game<'_>, id: i32) {
    if module().is_some() && game.remote.as_ref().is_some_and(|r| r.entity_id == id) {
        state(|s| s.brain.pop(game.ticks));
    }
}
pub(super) fn correction() {
    if module().is_none() {
        return;
    }
    let now = Instant::now();
    state(|s| {
        s.corrections
            .retain(|at| now.duration_since(*at) < Duration::from_secs(10));
        if s.corrections.len() < 6 {
            s.corrections.push(now);
        }
        s.pause_until = Some(now + Duration::from_millis(500));
        if s.corrections.len() >= 6 {
            s.yielded = true;
            s.reason = "repeated applied position corrections; waiting for toggle";
        }
    });
}
fn count(game: &Game<'_>, item: &str) -> u32 {
    game.inventory
        .iter()
        .take(36)
        .filter_map(Option::as_ref)
        .chain(game.offhand.as_ref())
        .filter(|s| util::short(&s.item) == item)
        .fold(0u32, |n, s| n.saturating_add(s.count))
}
fn hand_has(game: &Game<'_>, item: &str, offhand: bool) -> bool {
    game.inventory
        .iter()
        .take(9)
        .filter_map(Option::as_ref)
        .any(|s| s.count > 0 && util::short(&s.item) == item)
        || offhand
            && game
                .offhand
                .as_ref()
                .is_some_and(|s| s.count > 0 && util::short(&s.item) == item)
}
fn empty_offhand(game: &Game<'_>) -> bool {
    game.offhand.as_ref().is_none_or(|s| s.count == 0)
}
fn urgent_totem(game: &Game<'_>) -> bool {
    empty_offhand(game)
        && count(game, "totem_of_undying") > 0
        && with(|s| s.is_on("Auto Totem") || s.is_on("Auto Inventory Totem"))
}
fn charged(game: &Game<'_>, p: [i32; 3]) -> bool {
    let block = game.world.block(p[0], p[1], p[2]);
    game.is_anchor(block)
        && game
            .pack
            .assets
            .registry
            .state(block)
            .and_then(|s| s.properties.get("charges"))
            .and_then(|v| v.parse::<u8>().ok())
            .is_some_and(|n| n > 0 && n <= 4)
}
fn scan_anchors(game: &Game<'_>) {
    let Some(at) = util::block_pos(game) else {
        return;
    };
    state(|s| {
        if s.scan_origin.is_none_or(|old| {
            (0..3)
                .map(|i| (i64::from(at[i]) - i64::from(old[i])).pow(2))
                .sum::<i64>()
                > 9
        }) {
            s.scan_origin = Some(at);
            s.cursor = 0;
            s.scan_good = true;
            s.scan_complete = false;
            s.anchors.clear();
        }
        s.anchors.retain(|p| charged(game, *p));
        let origin = s.scan_origin.unwrap_or(at);
        for _ in 0..policy::ANCHOR_CELLS {
            if s.cursor == 0 {
                s.scan_good = true;
            }
            let index = s.cursor;
            let dx = (index % 11) as i32 - 5;
            let dz = ((index / 11) % 11) as i32 - 5;
            let dy = (index / 121) as i32 - 2;
            let pos = [
                origin[0].saturating_add(dx),
                origin[1].saturating_add(dy),
                origin[2].saturating_add(dz),
            ];
            if !game.world.loaded(pos[0], pos[2]) {
                s.scan_good = false;
            } else if charged(game, pos) && !s.anchors.contains(&pos) {
                if s.anchors.len() < 24 {
                    s.anchors.push(pos);
                } else {
                    s.scan_good = false;
                }
            }
            s.cursor += 1;
            if s.cursor >= 605 {
                s.cursor = 0;
                s.scan_complete = s.scan_good;
            }
        }
    });
}
fn sampled_threat(game: &mut Game<'_>) -> (Option<f64>, bool) {
    let Some(at) = util::player_pos(game) else {
        return (None, false);
    };
    if game.crystals.len() > policy::CRYSTAL_LIMIT {
        return (None, false);
    }
    if !at.iter().all(|v| v.is_finite())
        || game.crystals.iter().any(|c| {
            ![c.position.x, c.position.y, c.position.z]
                .iter()
                .all(|v| v.is_finite())
        })
    {
        return (None, false);
    }
    let mut sources: Vec<Blast> = game
        .crystals
        .iter()
        .filter(|c| c.id != 0 && !c.invulnerable)
        .map(|c| Blast::explosion([c.position.x, c.position.y, c.position.z], 6.0))
        .filter(|b| util::dist(at, b.center) <= 12.0)
        .collect();
    if game.dim != Dim::Nether {
        for p in state(|s| s.anchors.clone()) {
            let center = util::center_of(p);
            if util::dist(at, center) <= 10.0 {
                sources.push(Blast::anchor(p));
            }
        }
    }
    sources.sort_by(|a, b| util::dist(at, a.center).total_cmp(&util::dist(at, b.center)));
    let mut signature = 0xcbf29ce484222325u64;
    for b in &sources {
        for value in b.center {
            signature = (signature ^ value.to_bits()).wrapping_mul(0x100000001b3);
        }
        signature = (signature ^ u64::from(b.power.to_bits())).wrapping_mul(0x100000001b3);
    }
    let fresh = state(|s| {
        s.threat_signature == signature
            && game.ticks.saturating_sub(s.threat_at) < 4
            && s.threat_origin
                .is_some_and(|old| util::dist(old, at) <= 0.75)
    });
    if fresh {
        return state(|s| (s.threat, s.threat_complete));
    }
    let complete = sources.len() <= policy::THREAT_PREDICTIONS && state(|s| s.scan_complete);
    let mode = with(|s| {
        if ["Crystal Aura", "Anchor Aura"].iter().any(|name| {
            s.module(name)
                .is_some_and(|m| m.p("Damage model") == "Unmitigated")
        }) {
            "Unmitigated"
        } else {
            "Observed equipment"
        }
    });
    let mut worst = Some(0.0f64);
    let mut budget = Budget::new(policy::THREAT_PREDICTIONS);
    for blast in sources.into_iter().take(policy::THREAT_PREDICTIONS) {
        let damage = local_damage(game, blast, &mut budget).map(|p| safety_damage(p, mode));
        match damage {
            Some(d) if d.is_finite() && d >= 0.0 => {
                worst = worst.map(|old| old.max(d));
            }
            _ => {
                worst = None;
                break;
            }
        }
    }
    state(|s| {
        s.threat = worst;
        s.threat_complete = complete;
        s.threat_origin = Some(at);
        s.threat_signature = signature;
        s.threat_at = game.ticks;
    });
    (worst, complete)
}

fn target(game: &Game<'_>, m: &Module) -> (Option<util::Target>, usize) {
    let Some(eye) = game.eye_position() else {
        return (None, 0);
    };
    let old = state(|s| s.target);
    let mut kept = None;
    let mut best = None;
    let mut near = 0usize;
    for mob in &game.mobs {
        if !enemy(game, mob) {
            continue;
        }
        let t = util::mob_target(mob);
        let Some(distance) = super::sunset_melee_policy::box_distance(eye, t.lo, t.hi) else {
            return (None, 0);
        };
        if distance <= 8.0 {
            near = near.saturating_add(1);
        }
        if Some(t.id) == old && distance <= m.n("Fight range").clamp(8.0, 32.0) + 4.0 {
            kept = Some(t.clone());
        }
        if distance > m.n("Fight range").clamp(8.0, 32.0) {
            continue;
        }
        let score = -distance + (20.0 - f64::from(t.health).min(20.0)) * 0.05;
        if best.as_ref().is_none_or(|(_, old)| score > *old) {
            best = Some((t, score));
        }
    }
    (kept.or_else(|| best.map(|(t, _)| t)), near)
}

pub(super) fn pre_input(game: &mut Game<'_>) {
    post_input(game);
    let Some(m) = module() else {
        cancel(game);
        state(|s| *s = State::default());
        return;
    };
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    if !connected {
        cancel(game);
        state(|s| {
            *s = State::default();
            s.session = session(game);
            s.side = 1;
        });
    }
    let live = game.native_window()
        && game.native_on(sunset::ID)
        && game.remote.is_some()
        && !game.spectator()
        && !game.creative
        && game.riding.is_none()
        && game.player.as_ref().is_some_and(|p| {
            p.health.health().is_finite() && p.health.health() > 0.0 && !p.flying && !p.fall_flying
        })
        && game.mobs.len() <= policy::POPULATION_LIMIT
        && game.crystals.len() <= policy::CRYSTAL_LIMIT;
    let hotbar = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let physical = game.left
        || game.right
        || KEYS
            .iter()
            .chain(hotbar.iter())
            .any(|k| game.physical_keys.contains(k))
        || game.physical_buttons.contains(&MouseButton::Left)
        || game.physical_buttons.contains(&MouseButton::Right)
        || state(|s| s.mouse_until.is_some_and(|until| Instant::now() < until));
    let conflict = with(|s| s.is_on("Sword PvP Bot"));
    let competing = with(|s| {
        [
            "Kill Aura",
            "Trigger Bot",
            "Nuker",
            "Scaffold",
            "Auto EXP",
            "Auto Gap",
        ]
        .iter()
        .any(|name| s.is_on(name))
    });
    let paused = state(|s| s.yielded || s.pause_until.is_some_and(|until| Instant::now() < until));
    if !live || conflict || state(|s| s.yielded) {
        cancel(game);
        state(|s| s.reason = "paused for world, input, controller conflict or correction");
        return;
    }
    if !game.native_playing()
        || game.screen != Screen::Playing
        || sunset::gui_open()
        || super::sunset_render::camera_takes_mouse()
        || physical
        || competing
        || paused
        || game.mining.is_some()
    {
        pause(game, m.b("Keep totem"));
        state(|s| {
            if !s.yielded {
                s.reason = "temporary input/GUI/correction pause; ownership retained";
            }
        });
        return;
    }
    if control::owner().is_some_and(|owner| {
        ![
            "Crystal Aura",
            "Anchor Aura",
            "Auto Totem",
            "Auto Inventory Totem",
            FOOD,
        ]
        .contains(&owner)
    }) {
        pause(game, m.b("Keep totem"));
        state(|s| s.reason = "yielding to another hand owner");
        return;
    }
    // Establish supply protection first. This is module state ownership only,
    // never a long-held combat hand lease.
    if !coordinate(false, false, m.b("Keep totem")) {
        cancel(game);
        state(|s| {
            s.yielded = true;
            s.reason = "coordinated module edited; waiting for toggle";
        });
        return;
    }
    scan_anchors(game);
    let (threat, complete) = sampled_threat(game);
    let (picked, near) = target(game, &m);
    let id = picked.as_ref().map(|t| t.id);
    state(|s| {
        if s.target != id {
            s.target = id;
        }
        if game.ticks >= s.flip_at {
            s.side = -s.side;
            s.flip_at = game.ticks.saturating_add(18);
        }
    });
    let Some(eye) = game.eye_position() else {
        return;
    };
    let distance = picked
        .as_ref()
        .and_then(|t| super::sunset_melee_policy::box_distance(eye, t.lo, t.hi));
    let p = game.player.as_ref().unwrap();
    let health = f64::from(p.health.health());
    let absorption = f64::from(p.health.absorption());
    let food =
        hand_has(game, "golden_apple", false) || hand_has(game, "enchanted_golden_apple", false);
    let anchors = m.b("Use anchors") && game.dim != Dim::Nether;
    let supplied = hand_has(game, "end_crystal", true)
        || game.crystals.iter().any(|c| {
            c.id != 0
                && !c.invulnerable
                && util::dist(eye, [c.position.x, c.position.y, c.position.z]) <= 4.5
        })
        || anchors
            && (hand_has(game, "respawn_anchor", true) && hand_has(game, "glowstone", true)
                || state(|s| {
                    s.anchors
                        .iter()
                        .any(|p| util::dist(eye, util::center_of(*p)) <= 4.5)
                }));
    let view = policy::View {
        tick: game.ticks,
        health,
        absorption,
        distance,
        enemies_near: near,
        totems: count(game, "totem_of_undying"),
        empty_offhand: empty_offhand(game),
        refill_possible: urgent_totem(game),
        supplied,
        food,
        using_food: state(|s| s.food.is_some()),
        eat_below: m.n("Gapple below HP"),
        threat,
        threat_complete: complete,
        strafe: m.b("Strafe"),
        side: state(|s| s.side),
    };
    let Some(mut decision) = state(|s| s.brain.decide(view)) else {
        cancel(game);
        return;
    };
    let eating = food_tick(game, decision.eat);
    if game.using.is_some() && !eating {
        pause(game, m.b("Keep totem"));
        state(|s| s.reason = "manual native item use");
        return;
    }
    if !coordinate(decision.auras && !eating, anchors, m.b("Keep totem")) {
        cancel(game);
        state(|s| {
            s.yielded = true;
            s.reason = "coordinated module edited; waiting for toggle";
        });
        return;
    }
    if let Some(t) = picked.as_ref() {
        let wanted = sunset::look_at(
            eye,
            [
                (t.lo[0] + t.hi[0]) * 0.5,
                t.lo[1] + (t.hi[1] - t.lo[1]) * 0.65,
                (t.lo[2] + t.hi[2]) * 0.5,
            ],
        );
        if let Some(step) = policy::camera_step(
            [game.yaw, game.pitch],
            [wanted.0, wanted.1],
            m.n("Turn speed"),
        ) {
            game.turn_player(step[0] / 0.15, step[1] / 0.15);
        }
    }
    let yaw = f64::from(game.yaw).to_radians();
    let ahead = [-yaw.sin(), yaw.cos()];
    let left = [yaw.cos(), yaw.sin()];
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
    // Stay grounded around explosives; no fabricated hops or airborne inputs.
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
        s.phase = Some(decision.phase);
        s.reason = match decision.phase {
            policy::Phase::Search => "waiting for enemy",
            policy::Phase::Supply => "waiting for supplied native utilities",
            policy::Phase::Press => "closing for native explosive planners",
            policy::Phase::Guard => "guard spacing; bounded threat samples",
            policy::Phase::Retreat => "retreating from observed risk",
            policy::Phase::Eat => "retreat and native food use",
        };
    });
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
        let support = super::spear::sunset_combat_collision::clip(game, from, to)
            .map(|hit| p.position.y - hit.y);
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

fn enemy(game: &Game<'_>, mob: &horizon_client::mobs::Mob) -> bool {
    if game.remote_player_name(mob.id).is_none() {
        return false;
    }
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

pub(super) fn mouse_input() {
    state(|s| s.mouse_until = Some(Instant::now() + Duration::from_millis(250)));
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
