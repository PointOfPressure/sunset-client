// Original native Crystal PvP controller.
use super::sunset::{self, Category, Module, Value, hold, with};
use super::sunset_bot_tactics_policy as tactics;
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
    Module::new(NAME,Category::Combat,"Native tactical crystal fighter with launch follow-ups, anchor cover, hole and terrain mining, armor mending, verified hotbar refill and collision-checked pearls. Keeps observed equipment, friend and lethal safety checks.")
        .num("Fight range",16.0,8.0,32.0,1.0).bool("Use anchors",true)
        .bool("Knock-up hits",true).bool("Break holes",true).bool("Guard anchors",true)
        .bool("Surround self",true).num("Gapple below HP",14.0,0.0,20.0,1.0)
        .bool("Mend armor",true).bool("Escape pearl",true).bool("Chase pearls",true)
        .bool("Refill hotbar",true).bool("Learn rivals",true).bool("Strafe",true)
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
        } else if m.name == "Auto Inventory Totem" {
            Vec::new()
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
    tactics: Tactics,
    contexts: Vec<ModuleLease>,
    input: Option<InputSave>,
    food: Option<Food>,
    target: Option<u32>,
    side: i8,
    flip_at: u64,
    intent: policy::Intent,
    mouse_pending: bool,
    last_food: u64,
    pearl_counts: [u32; 9],
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
        let protection = if inventory || !s.is_on("Auto Totem") {
            "Auto Inventory Totem"
        } else {
            "Auto Totem"
        };
        let mut owned = true;
        for (name, want) in [
            ("Crystal Aura", combat),
            ("Anchor Aura", combat && anchors),
            (protection, totem),
        ] {
            let Some(m) = s.modules.iter_mut().find(|m| m.name == name) else {
                if name != "Auto Totem" && name != "Auto Inventory Totem" {
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
    tactical_suspend(game);
    release_pause_lease();
    save_rivals(game, true, true);
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    stop_food(game, connected);
    restore_contexts();
    state(|s| {
        s.target = None;
        s.phase = None;
    });
}
// The caller must release the runtime mutex and invoke this before mutating
// settings. Only modules with an actual retained lease trigger a user yield.
pub(super) fn editing(game: &mut Game<'_>, module_name: &str) {
    let own = module_name == NAME;
    let cancel_context = state(|s| {
        let controlled = s.contexts.iter().any(|lease| lease.name == module_name);
        s.intent.editing(own, controlled)
    });
    if cancel_context {
        cancel(game);
        state(|s| {
            s.reason = if own {
                "controller settings edited; context released"
            } else {
                "explicit controlled-module edit; waiting for toggle"
            }
        });
    }
}

pub(super) fn disabled(game: &mut Game<'_>) {
    cancel(game);
    state(|s| s.intent.disabled());
}

fn pause(game: &mut Game<'_>, keep_totem: bool) {
    tactical_suspend(game);
    post_input(game);
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    stop_food(game, connected);
    // Keep an owned false field distinct from having no lease. This preserves
    // the last-applied values while chat/settings are open, so a user edit is
    // detected rather than adopted as a fresh controller baseline on resume.
    if state(|s| !s.contexts.is_empty()) && !coordinate(false, false, keep_totem) {
        restore_contexts();
        state(|s| {
            s.intent.yielded = true;
            s.reason = "coordinated module edited while paused";
        });
    }
    state(|s| s.phase = None);
}
pub(super) fn diagnostics() -> String {
    state(|s| {
        format!(
            "CrystalBot reason={} phase={:?} target={:?} food={} contexts={} sampled_threat={:?} threat_complete={} anchor_cursor={} yielded={} pearl_counts={:?}",
            s.reason,
            s.phase,
            s.target,
            s.food.is_some(),
            s.contexts.len(),
            s.threat,
            s.threat_complete,
            s.cursor,
            s.intent.yielded,
            s.pearl_counts
        )
    })
}
pub(super) fn note_totem(game: &Game<'_>, id: i32) {
    if module().is_none() {
        return;
    }
    state(|s| {
        if game.remote.as_ref().is_some_and(|r| r.entity_id == id) {
            s.tactics.fight.own_pop(game.ticks);
        } else if s.target == Some(id as u32) {
            s.tactics.fight.their_pop = Some(game.ticks);
            if let Some((_, _, _, _, pops)) = s.tactics.fight.opener.as_mut() {
                *pops = pops.saturating_add(1);
            }
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
    let mut worst = Some(0.0f64);
    let mut budget = Budget::new(policy::THREAT_PREDICTIONS);
    for blast in sources.into_iter().take(policy::THREAT_PREDICTIONS) {
        let damage = local_damage(game, blast, &mut budget).map(|p| p.estimate);
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



pub(super) fn pre_input(game: &mut Game<'_>) {
    post_input(game);
    let Some(m) = module() else {
        cancel(game);
        state(|s| *s = State::default());
        return;
    };
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    if !connected {
        let prior = state(|s| s.session.as_ref().map(|old| old.1));
        cancel(game);
        state(|s| {
            let mut pause = std::mem::take(&mut s.tactics.pause);
            let rivals = std::mem::take(&mut s.tactics.rivals);
            if let Some(old) = prior {
                pause.context(game.ticks, old != game.dim);
            }
            *s = State::default();
            s.session = session(game);
            s.tactics.pause = pause;
            s.tactics.rivals = rivals;
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
        || state(|s| std::mem::take(&mut s.mouse_pending));
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
    let paused =
        state(|s| s.intent.yielded || s.pause_until.is_some_and(|until| Instant::now() < until));
    if !live || conflict || state(|s| s.intent.yielded) {
        cancel(game);
        state(|s| s.reason = "paused for world, input, controller conflict or correction");
        return;
    }
    refresh_pause_lease(game);
    if !game.native_playing()
        || game.screen != Screen::Playing
        || sunset::gui_open()
        || super::sunset_render::camera_takes_mouse()
        || physical
        || competing
        || paused
        || game.mining.is_some() && state(|s| s.tactics.mine.is_none())
    {
        if physical {
            state(|s| s.tactics.pause.manual(game.ticks));
        }
        pause(game, true);
        state(|s| {
            if !s.intent.yielded {
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
            BOT_HAND,
            BOT_MINE,
            BOT_KIT,
            BOT_PAUSE,
        ]
        .contains(&owner)
    }) {
        pause(game, true);
        state(|s| s.reason = "yielding to another hand owner");
        return;
    }
    // Establish supply protection first. This is module state ownership only,
    // never a long-held combat hand lease.
    if !coordinate(false, false, true) {
        cancel(game);
        state(|s| {
            s.intent.yielded = true;
            s.reason = "coordinated module edited; waiting for toggle";
        });
        return;
    }
    let Some((mut decision, eating)) = tactical_tick(game, &m) else {
        return;
    };
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
    let drop_floor = state(|s| s.tactics.drop_y).filter(|floor| {
        floor.is_finite()
            && p.position.y - *floor > 1.25
            && p.position.y - *floor <= 6.0
            && p.health.health() + p.health.absorption() > 8.0
    });
    let support_depth = drop_floor.map_or(1.25, |floor| p.position.y - floor + 0.25);
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
        let to = Vec3::new(from.x, p.position.y - support_depth, from.z);
        let support = super::spear::sunset_combat_collision::clip(game, from, to)
            .map(|hit| p.position.y - hit.y);
        let mut hazard = false;
        for down in 0..=(support_depth.ceil() as i32).max(2) {
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
        let supported = if drop_floor.is_some() {
            body_clear
                && !hazard
                && support.is_some_and(|d| d.is_finite() && d >= -0.1 && d <= support_depth)
        } else {
            policy::support_safe(body_clear, support, hazard)
        };
        if !supported {
            return false;
        }
    }
    true
}

fn enemy(game: &Game<'_>, mob: &horizon_client::mobs::Mob) -> bool {
    util::player_enemy(game, mob)
}

// The runtime supplies genuine raw mouse deltas before any bot camera turn.
pub(super) fn mouse_delta(game: &Game<'_>, delta: [f64; 2]) {
    if game.screen != Screen::Playing || sunset::gui_open() || !delta.iter().all(|v| v.is_finite())
    {
        return;
    }
    state(|s| {
        let t = &mut s.tactics;
        if t.mouse_tick != game.ticks {
            t.mouse_tick = game.ticks;
            t.mouse_sum = [0.; 2];
        }
        for i in 0..2 {
            t.mouse_sum[i] += delta[i];
        }
        if t.mouse_sum.iter().any(|v| v.abs() > 1.5) {
            t.pause.manual(game.ticks);
            s.mouse_pending = true;
        }
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

// This fragment is assembled into crystal_bot.rs by assemble.py. It shares the
// existing controller's exact module, connection, input and food ownership.
const BOT_HAND: &str = "Crystal Bot hands";
const BOT_PAUSE: &str = "Crystal Bot pause";
const BOT_MINE: &str = "Crystal Bot mining";
const BOT_KIT: &str = "Crystal Bot kit";
static BOOK_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const FACES: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];
const SIDES: [[i32; 3]; 4] = [[-1, 0, 0], [1, 0, 0], [0, 0, -1], [0, 0, 1]];
type BotClick = super::remote::sunset_bot_remote::BotClick;
struct BotMine {
    done: control::Finish,
    cell: [i32; 3],
    slot: usize,
    previous: usize,
    began: u64,
    applied: bool,
}
#[derive(Clone, Copy)]
struct PearlPlan {
    look: (f32, f32),
    spot: Option<[f64; 3]>,
    away: f64,
    until: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct ObservedPearlFlight {
    target: u32,
    peer: u32,
    began: u64,
}
#[derive(Clone, Copy)]
struct PearlSearch {
    escape: bool,
    observed_flight: Option<ObservedPearlFlight>,
    spot: [f64; 3],
    began: u64,
    cursor: u32,
    best: Option<(f64, PearlPlan)>,
}
#[derive(Clone, Copy)]
struct DigAnchor {
    cell: [i32; 3],
    until: u64,
    guards: u8,
    observed: bool,
}
#[derive(Default)]
struct Tactics {
    mouse_tick: u64,
    mouse_sum: [f64; 2],
    choice: tactics::TargetChoice,
    fight: tactics::Fight,
    pause: tactics::PauseClock,
    air: tactics::Air,
    rivals: Vec<(u128, tactics::Rival)>,
    rival: Option<u128>,
    learn: bool,
    book_at: u64,
    budget_tick: u64,
    predictions: u8,
    forecast_steps: u16,
    blocks: tactics::Blocks,
    camp: tactics::CampLedger,
    hold_guard: Option<control::Finish>,
    mine: Option<BotMine>,
    queued: Option<Instant>,
    generation: u64,
    steer: Option<tactics::Steer>,
    flight_tick: Option<u64>,
    pearl: Option<PearlPlan>,
    pearl_search: Option<PearlSearch>,
    pearl_at: Option<u64>,
    action: &'static str,
    last_xp: u64,
    kit_after: u64,
    spot_after: u64,
    stand: Option<[f64; 3]>,
    drop_y: Option<f64>,
    anchor_cursor: usize,
    anchor_origin: Option<[i32; 3]>,
    anchor_cells: Vec<[i32; 3]>,
    dig: Option<DigAnchor>,
    dig_blows: u8,
    dig_after: u64,
    dig_walk: u64,
    guard_after: u64,
    attack_wait: Option<(u32, u64, f64, bool)>,
    attacked_by: Option<(u32, u64)>,
    last_pearl: Option<u32>,
    box_until: u64,
    box_origin: Option<[i32; 3]>,
    skipped: Vec<([i32; 3], u64)>,
}
pub(super) fn steer(game: &Game<'_>) -> Option<tactics::Steer> {
    if module().is_none() || !state(|s| s.session.as_ref().is_some_and(|old| same(game, old))) {
        return None;
    }
    state(|s| s.tactics.steer)
}
pub(super) fn flight_offset(game: &Game<'_>, id: u32, ahead: u32) -> Option<[f64; 3]> {
    if ahead == 0
        || module().is_none()
        || !state(|s| s.session.as_ref().is_some_and(|old| same(game, old)))
    {
        return None;
    }
    let req = state(|s| {
        let t = &s.tactics;
        if t.flight_tick
            .is_none_or(|at| game.ticks < at || game.ticks - at > 2)
            || t.fight.target != Some(id)
        {
            return None;
        }
        let r = t
            .rivals
            .iter()
            .find(|(u, _)| Some(*u) == t.rival)
            .map(|(_, r)| r)
            .cloned()
            .unwrap_or_default();
        t.air.request(game.ticks, ahead, &r)
    })?;
    let target = game
        .mobs
        .iter()
        .find(|m| m.id == id && m.alive() && !m.removed)
        .map(util::mob_target)?;
    let stop = body_forecast(game, &target, req)?;
    Some(tactics::sub(stop.at, target.pos))
}
fn body_forecast(
    game: &Game<'_>,
    target: &util::Target,
    req: ([f64; 3], [f64; 3], [f64; 3], bool, u32),
) -> Option<tactics::Stop> {
    if !state(|s| {
        let t = &mut s.tactics;
        if t.budget_tick != game.ticks {
            t.budget_tick = game.ticks;
            t.predictions = 0;
            t.forecast_steps = 0;
        }
        if t.forecast_steps.saturating_add(req.4 as u16) > 80 {
            return false;
        }
        t.forecast_steps += req.4 as u16;
        true
    }) {
        return None;
    }
    tactics::body_path(
        req.0,
        req.1,
        req.2,
        req.3,
        f64::from(
            util::facts(
                game,
                [
                    req.0[0].floor() as i32,
                    (req.0[1] - 0.5000001).floor() as i32,
                    req.0[2].floor() as i32,
                ],
            )
            .friction,
        ) * 0.91,
        req.4,
        |from, step| super::sunset_prediction::clip_body(game, target, from, step),
    )
}
fn tactical_suspend(game: &mut Game<'_>) {
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    sunset::cancel_turns(BOT_HAND);
    sunset::cancel_turns(BOT_MINE);
    let mine = state(|s| {
        let t = &mut s.tactics;
        t.generation = t.generation.wrapping_add(1);
        t.queued = None;
        t.steer = None;
        t.flight_tick = None;
        t.pearl = None;
        t.pearl_search = None;
        t.drop_y = None;
        t.fight.launching_until = None;
        t.attack_wait = None;
        t.mine.take()
    });
    if let Some(mine) = mine {
        if connected && mine.done.current() {
            super::remote::sunset_city_mining::stop(game, mine.cell);
            if mine.applied && game.selected == mine.slot {
                hold(game, mine.previous);
            }
        }
    }
    control::release(BOT_HAND);
}
// Must run after final packet forwarding, not when a prospective attack is
// queued. A canceled/friend-filtered hit cannot start a launch prediction.
pub(super) fn attack_sent(game: &Game<'_>, id: u32) {
    let Some(mob) = game.mobs.iter().find(|m| m.id == id) else {
        return;
    };
    let at = super::remote::sunset_bot_remote::server_position(game, id).unwrap_or([
        mob.position.x,
        mob.position.y,
        mob.position.z,
    ]);
    let own = game
        .player
        .as_ref()
        .map(|p| [p.position.x, p.position.y, p.position.z]);
    let resistance = observed_knockback_resistance(game, mob);
    state(|s| {
        let t = &mut s.tactics;
        let Some((_, _, yaw, sprint)) =
            tactics::take_forwarded_attack(&mut t.attack_wait, id, game.ticks)
        else {
            return;
        };
        t.fight.struck(game.ticks, at[1]);
        t.flight_tick = Some(game.ticks);
        if let (Some(own), Some(resistance)) = (own, resistance) {
            t.air.kicked(
                game.ticks,
                at,
                own,
                yaw,
                sprint,
                resistance,
                f64::from(
                    util::facts(
                        game,
                        [
                            at[0].floor() as i32,
                            (at[1] - 0.5000001).floor() as i32,
                            at[2].floor() as i32,
                        ],
                    )
                    .friction,
                ) * 0.91,
            );
        }
    });
    let kicked = state(|s| s.tactics.air.kick);
    if let Some((_, from, velocity, way)) = kicked {
        let target = util::mob_target(mob);
        let bare = body_forecast(game, &target, (from, velocity, [0.; 3], true, 40));
        let nudged = body_forecast(
            game,
            &target,
            (from, velocity, tactics::mul(way, 0.01), true, 40),
        );
        if let (Some(a), Some(b)) = (bare, nudged) {
            if a.landed && b.landed {
                let d = tactics::sub(b.at, a.at);
                let gain = (d[0] * way[0] + d[2] * way[2]) / 0.01;
                state(|s| {
                    s.tactics.air.bare = Some(a);
                    s.tactics.air.gain = gain;
                });
            }
        }
    }
}
fn observed_knockback_resistance(game: &Game<'_>, mob: &horizon_client::mobs::Mob) -> Option<f64> {
    let mut result = 0.;
    for stack in mob.extra.armor.iter().flatten() {
        if stack.extra.as_ref().is_some_and(|e| !e.other.is_empty()) || stack.count != 1 {
            return None;
        }
        if let Some(patch) = stack.wire_patch() {
            if !known_bot_equipment_patch(&patch) {
                return None;
            }
        }
        let info = game.items().get(&stack.item)?;
        for (attribute, _, amount, operation, slot) in &info.modifiers {
            if attribute == "minecraft:knockback_resistance"
                && (slot == "armor" || info.equip_slot.as_ref() == Some(slot))
            {
                if operation != "add_value" || !amount.is_finite() {
                    return None;
                }
                result += amount;
            }
        }
    }
    Some(result.clamp(0., 1.))
}
fn known_bot_equipment_patch(patch: &[u8]) -> bool {
    use horizon_protocol::{buf::Reader, item::component as c};
    let mut r = Reader::new(patch);
    let Ok(n) = r.count(64) else {
        return false;
    };
    if r.count(64).ok() != Some(0) {
        return false;
    }
    for _ in 0..n {
        let Ok(k) = r.varint() else {
            return false;
        };
        if !matches!(
            k,
            c::DAMAGE | c::ENCHANTMENTS | c::CUSTOM_NAME | c::REPAIR_COST | c::TRIM | c::DYED_COLOR
        ) {
            return false;
        }
        let Ok(n) = r.count(r.remaining()) else {
            return false;
        };
        if r.bytes(n).is_err() {
            return false;
        }
    }
    r.remaining() == 0
}
pub(super) fn packet(game: &Game<'_>, packet: &horizon_protocol::packets::play::Clientbound) {
    use horizon_protocol::packets::play::Clientbound as C;
    if module().is_none() {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    if let Some(id) = super::sunset_bot_packet_policy::totem(packet) {
        note_totem(game, id);
    }
    match packet {
        C::Respawn(p) => state(|s| {
            s.tactics
                .pause
                .context(game.ticks, game.dim != Dim::from_id(&p.spawn.dimension))
        }),
        C::AddEntity(p) => {
            if let Some(base) = super::remote::sunset_hurt_remote::spawned_base(game, packet) {
                if let Ok(id) = u32::try_from(p.id) {
                    state(|s| s.tactics.camp.spawned(base, id, game.ticks));
                }
            }
        }
        C::RemoveEntities(p) => {
            let ids = p
                .ids
                .iter()
                .filter_map(|id| u32::try_from(*id).ok())
                .take(4096)
                .collect::<Vec<_>>();
            state(|s| s.tactics.camp.removed(&ids));
        }
        C::BlockUpdate(p) => {
            if let Ok(id) = u32::try_from(p.state) {
                state(|s| s.tactics.blocks.payload(p.pos, id));
            }
        }
        C::SectionBlocksUpdate(p) => {
            for change in p.changes.iter().take(4096) {
                if let Some((cell, id)) =
                    super::sunset_crystal_setup_policy::unpack_section(p.section, *change)
                {
                    state(|s| s.tactics.blocks.payload(cell, id));
                }
            }
        }
        C::Explode(_) => {
            if let Some(center) = super::sunset_bot_packet_policy::blast(packet) {
                state(|s| s.tactics.air.blast(center));
            }
        }
        C::SetTime(_) => {
            if let Some(time) = super::sunset_bot_packet_policy::time(packet) {
                state(|s| s.tactics.pause.time(time, now));
            }
        }
        C::DamageEvent(_) => {
            if let Some(id) = game.remote.as_ref().and_then(|r| {
                super::sunset_bot_packet_policy::own_damage_cause(packet, r.entity_id)
            }) {
                state(|s| s.tactics.attacked_by = Some((id, game.ticks)));
            }
        }
        C::PlayerPosition(_p) => {
            if let Some(own) = game.player.as_ref() {
                let from = [own.position.x, own.position.y, own.position.z];
                let Some(to) = super::sunset_bot_packet_policy::correction(packet, from) else {
                    return;
                };
                let distance = util::dist(from, to);
                state(|s| {
                    let t = &mut s.tactics;
                    t.pause.correction(
                        game.ticks,
                        now,
                        distance,
                        t.pearl_at
                            .map_or(u64::MAX, |at| game.ticks.saturating_sub(at)),
                    );
                });
            }
        }
        _ => {}
    }
}
fn tactical_rival(game: &Game<'_>, id: u32, learn: bool) {
    state(|s| s.tactics.learn = learn);
    let uuid = if learn {
        super::remote::sunset_bot_remote::uuid(game, id)
    } else {
        None
    };
    if state(|s| s.tactics.rival == uuid) {
        return;
    }
    let loaded = if learn {
        uuid.and_then(|uuid| read_rival(game, uuid))
    } else {
        None
    };
    state(|s| {
        let t = &mut s.tactics;
        t.rival = uuid;
        if let Some(uuid) = uuid {
            if !t.rivals.iter().any(|(u, _)| *u == uuid) {
                if t.rivals.len() == 64 {
                    t.rivals.remove(0);
                }
                t.rivals.push((uuid, loaded.unwrap_or_default()));
            }
            if let Some((_, r)) = t.rivals.iter_mut().find(|(u, _)| *u == uuid) {
                r.fights = r.fights.saturating_add(1);
                r.dirty = learn;
            }
        }
    });
}
fn rival() -> tactics::Rival {
    state(|s| {
        s.tactics
            .rivals
            .iter()
            .find(|(u, _)| Some(*u) == s.tactics.rival)
            .map(|(_, r)| r.clone())
            .unwrap_or_default()
    })
}
fn read_rival(game: &Game<'_>, uuid: u128) -> Option<tactics::Rival> {
    let path = game
        .native_data_dir("sunset")
        .join("rivals")
        .join(format!("{uuid:032x}.json"));
    if std::fs::metadata(&path).ok()?.len() > 16384 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if v["version"].as_u64() != Some(1) {
        return None;
    }
    let mut r = tactics::Rival::default();
    for (key, out) in [
        ("fights", &mut r.fights),
        ("launches", &mut r.launches),
        ("flights", &mut r.flights),
        ("retotems", &mut r.retotems),
        ("holes", &mut r.holes),
        ("low_spells", &mut r.low_spells),
    ] {
        *out = u32::try_from(v[key].as_u64()?).ok()?;
    }
    for (key, out, lo, hi) in [
        ("launch_rate", &mut r.launch_rate, 0., 1.),
        ("retotem", &mut r.retotem, 0., 100.),
        ("hole_ticks", &mut r.hole_ticks, 0., 600.),
        ("pearl_rate", &mut r.pearl_rate, 0., 1.),
    ] {
        let n = v[key].as_f64()?;
        if !n.is_finite() || n < lo || n > hi {
            return None;
        }
        *out = n;
    }
    for i in 0..3 {
        r.push[i] = v["push"][i].as_f64()?;
    }
    if !tactics::finite(r.push) || tactics::horizontal(r.push) > 0.15 {
        return None;
    }
    for i in 0..6 {
        r.rewards[i] = v["rewards"][i].as_f64()?;
        r.tries[i] = v["tries"][i].as_f64()?;
        if !r.rewards[i].is_finite()
            || r.rewards[i] < 0.
            || !r.tries[i].is_finite()
            || r.tries[i] < 0.
            || r.tries[i] > 1e6
        {
            return None;
        }
    }
    Some(r)
}
fn save_rivals(game: &Game<'_>, enabled: bool, force: bool) {
    if !enabled
        || !state(|s| s.tactics.learn)
        || !force && state(|s| game.ticks.saturating_sub(s.tactics.book_at) < 600)
    {
        return;
    }
    if BOOK_BUSY
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    let changed = state(|s| {
        s.tactics.book_at = game.ticks;
        s.tactics
            .rivals
            .iter_mut()
            .filter_map(|(u, r)| {
                if !r.dirty {
                    return None;
                }
                r.dirty = false;
                Some((*u, r.clone()))
            })
            .take(64)
            .collect::<Vec<_>>()
    });
    if changed.is_empty() {
        BOOK_BUSY.store(false, std::sync::atomic::Ordering::Release);
        return;
    }
    let folder = game.native_data_dir("sunset").join("rivals");
    // Filesystem work stays off the render/input tick. A bounded snapshot owns
    // only its serialized values; no Game/reference reaches the worker.
    std::thread::spawn(move || {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                BOOK_BUSY.store(false, std::sync::atomic::Ordering::Release);
            }
        }
        let _reset = Reset;
        if std::fs::create_dir_all(&folder).is_err() {
            return;
        }
        for (u, r) in changed {
            let value = serde_json::json!({"version":1,"fights":r.fights,"launches":r.launches,"launch_rate":r.launch_rate,"flights":r.flights,"push":r.push,"retotems":r.retotems,"retotem":r.retotem,"holes":r.holes,"hole_ticks":r.hole_ticks,"low_spells":r.low_spells,"pearl_rate":r.pearl_rate,"rewards":r.rewards,"tries":r.tries});
            let file = folder.join(format!("{u:032x}.json"));
            let temp = file.with_extension("tmp");
            if std::fs::write(&temp, value.to_string()).is_ok() {
                let _ = std::fs::rename(temp, file);
            }
        }
        let mut files = std::fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.path().extension().is_some_and(|x| x == "json")
                    && e.file_name().to_str().is_some_and(|n| {
                        n.len() == 37 && n[..32].bytes().all(|b| b.is_ascii_hexdigit())
                    })
            })
            .take(256)
            .collect::<Vec<_>>();
        files.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
        for old in files.into_iter().skip(64) {
            let _ = std::fs::remove_file(old.path());
        }
    });
}
fn tactical_pick(game: &Game<'_>, m: &Module) -> (Option<util::Target>, usize) {
    let Some(eye) = game.eye_position() else {
        return (None, 0);
    };
    let retaliation = state(|s| s.tactics.attacked_by);
    let mut all = Vec::with_capacity(16);
    let mut targets = Vec::with_capacity(16);
    let mut near = 0;
    for mob in game.mobs.iter().take(512).filter(|m| enemy(game, m)) {
        let t = util::mob_target(mob);
        let Some(d) = super::sunset_melee_policy::box_distance(eye, t.lo, t.hi) else {
            continue;
        };
        if d <= 8. {
            near += 1;
        }
        let has_totem = mob.extra.off_hand.as_deref() == Some("minecraft:totem_of_undying")
            || mob.extra.main_hand.as_deref() == Some("minecraft:totem_of_undying");
        let low = mob.extra.armor.iter().flatten().any(|s| {
            game.items().get(&s.item).is_some_and(|i| {
                i.max_damage > 0
                    && f64::from(i.max_damage.saturating_sub(s.damage)) / f64::from(i.max_damage)
                        < 0.3
            })
        });
        all.push(tactics::Candidate {
            id: t.id,
            distance: d,
            life: super::sunset_prediction::effective_health(game, &t).unwrap_or(f64::INFINITY),
            totem: has_totem,
            low_armor: low,
            retaliate: retaliation
                .is_some_and(|(id, at)| id == t.id && game.ticks.saturating_sub(at) <= 60),
        });
        targets.push(t);
    }
    let id = state(|s| {
        let regroup = s.tactics.fight.needs_food
            || s.food.is_some()
            || game.ticks < s.tactics.fight.regroup_until;
        s.tactics
            .choice
            .choose(game.ticks, m.n("Fight range").clamp(8., 32.), regroup, &all)
    });
    (
        id.and_then(|id| targets.into_iter().find(|t| t.id == id)),
        near,
    )
}
fn tactical_tick(game: &mut Game<'_>, m: &Module) -> Option<(policy::Decision, bool)> {
    if state(|s| {
        s.tactics
            .queued
            .is_some_and(|at| at.elapsed() > Duration::from_millis(250))
    }) {
        tactical_suspend(game);
    }
    if state(|s| {
        s.tactics.mine.as_ref().is_some_and(|mine| {
            !mine.done.current()
                || util::facts(game, mine.cell).replaceable
                || game.ticks.saturating_sub(mine.began) > 80
        })
    }) {
        let timed_out = state(|s| {
            s.tactics
                .mine
                .as_ref()
                .is_some_and(|mine| game.ticks.saturating_sub(mine.began) > 80)
        });
        stop_mine(game);
        if timed_out {
            state(|s| {
                s.tactics.fight.city_fails = s.tactics.fight.city_fails.saturating_add(1);
                s.tactics.fight.next_breach = game.ticks.saturating_add(100);
            });
        }
    }
    let pause_state = state(|s| s.tactics.pause.state(game.ticks));
    if pause_state != tactics::Pause::Go {
        if pause_state == tactics::Pause::Stop {
            util::toggle_off(game, NAME);
        }
        pause(game, true);
        return None;
    }
    save_rivals(game, m.b("Learn rivals"), false);
    scan_anchors(game);
    scan_all_anchors(game);
    let (target, near) = tactical_pick(game, m);
    let id = target.as_ref().map(|t| t.id);
    if state(|s| s.target != id) {
        tactical_suspend(game);
        state(|s| {
            s.target = id;
            s.tactics.stand = None;
            s.tactics.air = tactics::Air::default();
            s.tactics.dig = None;
            s.tactics.dig_blows = 0;
            s.tactics.blocks = tactics::Blocks::default();
            if let Some(id) = id {
                s.tactics.fight.start(id, game.ticks);
            }
        });
    }
    let life = game
        .player
        .as_ref()
        .map(|p| f64::from(p.health.health() + p.health.absorption()))?;
    let Some(target) = target else {
        coordinate(false, false, true);
        let eating = food_tick(
            game,
            game.player.as_ref().is_some_and(|p| {
                f64::from(p.health.health()) <= m.n("Gapple below HP") - 2.
                    && p.health.absorption() < 4.
            }),
        );
        if !eating {
            if m.b("Refill hotbar") {
                refill(game, m);
            }
            if m.b("Mend armor") {
                mend(game, m, true);
            }
        }
        return Some((
            bot_decision(policy::Phase::Search, 0, 0, false, false, eating),
            eating,
        ));
    };
    tactical_rival(game, target.id, m.b("Learn rivals"));
    let mob = game.mobs.iter().find(|m| m.id == target.id)?;
    let target_ground = mob.on_ground;
    let floating = mob.extra.fall_flying
        || mob.fluids.in_water
        || mob.fluids.in_lava
        || mob.climbing
        || mob.no_gravity
        || mob.extra.mount.is_some();
    let target_totem = mob.extra.off_hand.as_deref() == Some("minecraft:totem_of_undying");
    let using = mob.extra.using_item > 0;
    let own = util::player_pos(game)?;
    let server =
        super::remote::sunset_bot_remote::server_position(game, target.id).unwrap_or(target.pos);
    let eye = game.eye_position()?;
    let distance = super::sunset_melee_policy::box_distance(eye, target.lo, target.hi)?;
    let hole = in_hole(game, target.pos.map(|v| v.floor() as i32));
    let (theat, _complete) = sampled_threat(game);
    let risk = theat.map_or(1., |d| d / life.max(1.));
    let p = game.player.as_ref()?;
    let view = tactics::FightView {
        tick: game.ticks,
        health: f64::from(p.health.health()),
        absorption: f64::from(p.health.absorption()),
        totems: count(game, "totem_of_undying"),
        offhand_totem: game
            .offhand
            .as_ref()
            .is_some_and(|s| util::short(&s.item) == "totem_of_undying"),
        enemies: near,
        risk,
        target_life: super::sunset_prediction::effective_health(game, &target)
            .unwrap_or(f64::INFINITY),
        target_totem,
        distance,
        hole,
        target_y: server[1],
        eat_below: m.n("Gapple below HP"),
    };
    let air_land = state(|s| s.tactics.air.airborne && target_ground);
    if air_land {
        state(|s| {
            let t = &mut s.tactics;
            if let Some((_, r)) = t.rivals.iter_mut().find(|(u, _)| Some(*u) == t.rival) {
                t.air.landing_learn(server, r);
            }
        });
    }
    state(|s| {
        let t = &mut s.tactics;
        t.air
            .note(target.id, game.ticks, server, target_ground, floating);
        t.flight_tick = Some(game.ticks);
        if let Some((_, r)) = t.rivals.iter_mut().find(|(u, _)| Some(*u) == t.rival) {
            t.fight.observe(view, r);
        } else {
            // Combat progress remains live when persistence/learning is off.
            t.fight.observe_unlearned(view);
        }
    });
    let r = rival();
    let stance = state(|s| s.tactics.fight.stance(view, &r));
    let finishing = state(|s| s.tactics.fight.finishing(view, &r));
    let steering = stance.steer(
        target.id,
        life,
        view.offhand_totem,
        finishing,
        own[1].floor() >= target.pos[1].floor() && !hole,
        own[1].floor() as i32,
    );
    state(|s| {
        s.tactics.steer = Some(steering);
        if game.ticks >= s.flip_at {
            s.side = -s.side;
            s.flip_at = game.ticks.saturating_add(18);
        }
    });
    // Food and critical retreat use the same entity-feet distance. Mixing
    // eye-to-box reach here with feet distance can oscillate before use starts.
    let food_distance = util::dist(own, target.pos);
    let needs_food = state(|s| s.tactics.fight.needs_food);
    let mut eating = food_tick(
        game,
        (needs_food || life <= m.n("Gapple below HP") - 2.)
            && (!using || view.totems == 0 || life <= 8.)
            && (food_distance >= 8. || state(|s| s.food.is_some())),
    );
    let critical_retreat = !eating && tactics::critical_food_retreat(tactics::CriticalFoodView {
        life,
        distance: food_distance,
        needs_food,
        eating: state(|s| s.food.is_some()),
        using_food: game.using.is_some(),
        gapple_hotbar: hand_has(game, "golden_apple", false)
            || hand_has(game, "enchanted_golden_apple", false),
        offhand_totem: view.offhand_totem,
    });
    if critical_retreat {
        stop_food(game, true);
        eating = false;
    }
    if game.using.is_some() && !eating {
        pause(game, true);
        return None;
    }
    let anchors = m.b("Use anchors") && game.dim != Dim::Nether;
    let mut decision = bot_decision(
        match stance {
            tactics::Stance::Run => policy::Phase::Retreat,
            tactics::Stance::Guard => policy::Phase::Guard,
            _ => policy::Phase::Press,
        },
        if distance > 4. {
            1
        } else if distance < 2.4 {
            -1
        } else {
            0
        },
        if m.b("Strafe") && distance <= 4.5 {
            state(|s| s.side)
        } else {
            0
        },
        distance > 4.,
        true,
        eating,
    );
    if eating {
        decision = bot_decision(
            policy::Phase::Eat,
            if food_distance < 8. { -1 } else { 0 },
            0,
            false,
            false,
            true,
        );
    }
    let mut occupied = eating
        || state(|s| s.tactics.queued.is_some())
        || super::sunset_combat_inventory::busy_for(BOT_KIT);
    if !occupied && state(|s| s.tactics.pearl.is_some()) {
        occupied = continue_pearl(game, m, &target);
    }
    if !occupied && stance == tactics::Stance::Run {
        occupied = m.b("Escape pearl") && plan_pearl(game, m, &target, true);
        decision.forward = if distance < 12. { -1 } else { 0 };
        decision.side = 0;
        decision.sprint = false;
    }
    if critical_retreat {
        if !occupied && m.b("Escape pearl") {
            occupied = plan_pearl(game, m, &target, true);
        }
        decision.phase = policy::Phase::Retreat;
        decision.forward = -1;
        decision.side = 0;
        decision.sprint = false;
        decision.auras = false;
        state(|s| {
            s.tactics.fight.last_retreat = game.ticks;
            s.tactics.fight.regroup_until = game.ticks.saturating_add(300);
            s.tactics.stand = None;
            s.tactics.drop_y = None;
        });
    }
    if !occupied && !critical_retreat && stance != tactics::Stance::Run && !eating {
        if own[1] - target.pos[1] >= 3. && tactics::horizontal(tactics::sub(own, target.pos)) <= 8.
        {
            occupied = lower_enemy(game, m, &target, steering.self_cap);
        }
        if !occupied && m.b("Guard anchors") {
            occupied = anchor_defense(game, m, &target, steering.self_cap);
        }
        if !occupied && state(|s| s.tactics.fight.launching_until.is_some()) {
            occupied = launch(game, m, &target, stance, &r);
        }
        if !occupied
            && state(|s| game.ticks <= s.tactics.fight.follow_until && !s.tactics.fight.follow_laid)
        {
            occupied = follow_air(game, m, &target, steering.self_cap);
        }
        if !occupied && m.b("Surround self") && (life <= 8. || stance == tactics::Stance::Guard) {
            occupied = surround(game, m);
        }
        if !occupied && stance == tactics::Stance::Guard {
            occupied = close_danger(game, m, &target);
        }
        if !occupied && m.b("Break holes") {
            occupied = breach(game, m, &target, &r, stance, steering.self_cap);
        }
        if !occupied && m.b("Mend armor") && distance >= 12. {
            occupied = mend(game, m, false);
        }
        if !occupied && m.b("Chase pearls") {
            occupied = plan_pearl(game, m, &target, false);
        }
        if !occupied
            && m.b("Refill hotbar")
            && (distance >= 8. || !hand_has(game, "end_crystal", false))
        {
            occupied = refill(game, m);
        }
        if !occupied
            && finishing
            && f64::from(target.health) <= 3.
            && !state(|s| s.tactics.pause.slow())
        {
            occupied = queue_attack(game, m, &target, false);
        }
        if !occupied && m.b("Knock-up hits") {
            occupied = start_launch(game, m, &target, stance, &r);
        }
        if !occupied {
            occupied = lower_terrain(game, m, &target, stance);
        }
    }
    if occupied && !eating && !critical_retreat {
        decision.forward = 0;
        decision.side = 0;
        decision.sprint = false;
        decision.auras = false;
    }
    // Movement tasks can override that hold without bypassing safe_step.
    if let Some(spot) = state(|s| s.tactics.stand) {
        let d = tactics::horizontal(tactics::sub(spot, own));
        if d > 0.35 {
            face_spot(game, spot, 1.);
            decision.forward = 1;
            decision.side = 0;
            decision.sprint = d > 4.;
        } else {
            state(|s| {
                s.tactics.stand = None;
                s.tactics.drop_y = None;
            });
        }
    } else {
        face_spot(
            game,
            [
                target.pos[0],
                target.lo[1] + (target.hi[1] - target.lo[1]) * 0.65,
                target.pos[2],
            ],
            1.,
        );
        if !occupied && game.ticks >= state(|s| s.tactics.spot_after) {
            pick_stand(game, &target, stance);
        }
    }
    if !coordinate(decision.auras && !eating, anchors, true) {
        cancel(game);
        state(|s| s.intent.yielded = true);
        return None;
    }

    Some((decision, eating))
}
fn bot_decision(
    phase: policy::Phase,
    forward: i8,
    side: i8,
    sprint: bool,
    auras: bool,
    eat: bool,
) -> policy::Decision {
    policy::Decision {
        phase,
        forward,
        side,
        sprint,
        auras,
        eat,
        ideal: 3.2,
    }
}
fn face_spot(game: &mut Game<'_>, spot: [f64; 3], speed: f64) {
    if let Some(eye) = game.eye_position() {
        let (y, p) = sunset::look_at(eye, spot);
        if let Some(step) = policy::camera_step([game.yaw, game.pitch], [y, p], speed) {
            game.turn_player(step[0] / 0.15, step[1] / 0.15);
        }
    }
}

#[derive(Clone)]
enum BotAction {
    Place {
        cell: [i32; 3],
        before: u32,
        click: BotClick,
        purpose: tactics::Purpose,
        blast: Option<(Blast, f64, f64)>,
    },
    Click {
        cell: [i32; 3],
        before: u32,
        click: BotClick,
        blast: Option<(Blast, f64, f64)>,
    },
    Crystal {
        base: [i32; 3],
        blast: Blast,
        cap: f64,
    },
    Attack {
        id: u32,
        launch: bool,
    },
    Xp,
    Pearl(PearlPlan),
}
fn action_ready(game: &Game<'_>) -> bool {
    module().is_some()
        && game.native_on(sunset::ID)
        && game.native_playing()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && game.using.is_none()
        && game.carried.is_none()
        && util::inventory_idle(game)
        && !urgent_totem(game)
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health() > 0. && !p.flying && !p.fall_flying)
        && !game
            .physical_buttons
            .iter()
            .any(|b| matches!(b, MouseButton::Left | MouseButton::Right))
        && !game.physical_keys.iter().any(|k| KEYS.contains(k))
        && game.mobs.len() <= 512
        && game.crystals.len() <= 128
}
fn safe_blast(game: &mut Game<'_>, blast: Blast, target: u32, min: f64, cap: f64) -> bool {
    let friend_cost = with(|s| s.friends.len().min(8)) as u8;
    if !prediction_budget(game, 2 + friend_cost) {
        return false;
    }
    if !min.is_finite()
        || !cap.is_finite()
        || cap < 0.
        || min < 0.
        || !super::sunset_loot_guard::allows(game, blast, true)
    {
        return false;
    }
    let mut budget = Budget::new(10);
    let Some(local) = local_damage(game, blast, &mut budget) else {
        return false;
    };
    let own = local.estimate;
    let life = game
        .player
        .as_ref()
        .map_or(0., |p| f64::from(p.health.health() + p.health.absorption()));
    if own > cap || super::sunset_explosive_tactics::endangered(own, life, game.creative) {
        return false;
    }
    let Some(victim) = game
        .mobs
        .iter()
        .find(|m| m.id == target && enemy(game, m))
        .map(util::mob_target)
    else {
        return false;
    };
    let Some(theirs) =
        super::sunset_explosive_damage::target_damage(game, &victim, blast, &mut budget)
    else {
        return false;
    };
    if !theirs.estimate.is_finite() || theirs.estimate < min {
        return false;
    }
    let friends = with(|s| s.friends.clone());
    let mut checked = 0;
    for mob in game
        .mobs
        .iter()
        .take(512)
        .filter(|m| m.kind.id == "minecraft:player" && m.alive() && !m.removed)
    {
        let name = game.remote_player_name(mob.id);
        if !friends.is_empty() && name.is_none() {
            return false;
        }
        if !name.is_some_and(|n| friends.iter().any(|f| f.eq_ignore_ascii_case(n))) {
            continue;
        }
        checked += 1;
        if checked > 8 {
            return false;
        }
        let friend = util::mob_target(mob);
        let Some(d) =
            super::sunset_explosive_damage::target_damage(game, &friend, blast, &mut budget)
        else {
            return false;
        };
        // Friends retain the unmitigated upper bound and a zero-damage veto.
        if !d.upper.is_finite() || d.upper > 0. {
            return false;
        }
    }
    true
}
fn queue_action(
    game: &mut Game<'_>,
    m: &Module,
    slot: usize,
    look: (f32, f32),
    action: BotAction,
) -> bool {
    if slot >= 9
        || !action_ready(game)
        || state(|s| s.tactics.queued.is_some())
        || !control::claim(BOT_HAND)
    {
        return false;
    }
    let done = control::finish(BOT_HAND);
    if !done.current() {
        return false;
    }
    let Some(connection) = session(game) else {
        return false;
    };
    let snapshot = m.clone();
    let Some(stack) = game
        .inventory
        .get(slot)
        .and_then(Option::as_ref)
        .filter(|s| s.count > 0)
        .cloned()
    else {
        return false;
    };
    let queued = Instant::now();
    let generation = state(|s| {
        s.tactics.queued = Some(queued);
        s.tactics.generation
    });
    let accepted = sunset::try_turn_then(BOT_HAND, look.0, look.1, move |game| {
        let _done = done;
        if !_done.current() || state(|s| s.tactics.generation != generation) {
            return;
        }
        state(|s| s.tactics.queued = None);
        if queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !action_ready(game)
            || !module().is_some_and(|now| now == snapshot)
            || game.inventory.get(slot).and_then(Option::as_ref) != Some(&stack)
        {
            return;
        }
        let target = state(|s| s.target);
        let prior = game.selected;
        hold(game, slot);
        match action {
            BotAction::Crystal { base, blast, cap } => {
                if crystal_site(game, base)
                    && game
                        .eye_position()
                        .and_then(|eye| {
                            game.clip_ray(eye, super::view_vector(look.0, look.1), game.reach())
                        })
                        .is_some_and(|hit| hit.pos == base && hit.face == [0, 1, 0])
                    && target.is_some_and(|id| safe_blast(game, blast, id, 0., cap))
                    && state(|s| s.tactics.camp.sent(base, game.ticks))
                {
                    let sequence = util::sequence(game);
                    game.send(S::UseItemOn(play::UseItemOn {
                        hand: 0,
                        pos: base,
                        face: 1,
                        cursor: [0.5, 1., 0.5],
                        inside: false,
                        world_border: false,
                        sequence,
                    }));
                    game.swing_arm();
                    game.send(S::Punch(horizon_protocol::packets::Empty));
                }
            }
            BotAction::Place {
                cell,
                before,
                click,
                purpose,
                blast,
            } => {
                let mut rays = 8;
                let okay = game.world.block(cell[0], cell[1], cell[2]).0 == before
                    && util::can_place(game, cell, true)
                    && exact_click(game, click, look)
                    && super::remote::sunset_bot_remote::valid(
                        game,
                        cell,
                        slot,
                        click,
                        game.reach(),
                        &mut rays,
                    )
                    && blast.is_none_or(|(blast, min, cap)| {
                        target.is_some_and(|id| safe_blast(game, blast, id, min, cap))
                    })
                    && state(|s| s.tactics.blocks.sent(cell, before, game.ticks, purpose));
                if okay {
                    send_block_click(game, click);
                    state(|s| s.tactics.action = "block sent; awaiting payload/world agreement");
                }
            }
            BotAction::Click {
                cell,
                before,
                click,
                blast,
            } => {
                let valid = game.world.block(cell[0], cell[1], cell[2]).0 == before
                    && exact_click(game, click, look)
                    && blast.is_none_or(|(b, min, cap)| {
                        target.is_some_and(|id| safe_blast(game, b, id, min, cap))
                    });
                if valid {
                    send_block_click(game, click);
                }
            }
            BotAction::Attack { id, launch } => {
                let fresh = game.mobs.iter().find(|m| m.id == id && enemy(game, m));
                let charge = super::sunset_melee_assist::slot_strength(game, slot, 0.5);
                if fresh.is_some()
                    && charge.is_some_and(|c| c >= if launch { 0.95 } else { 0.9 })
                    && super::sunset_melee_assist::hits_target(
                        game, id, look.0, look.1, false, 2.95,
                    )
                {
                    if launch {
                        state(|s| {
                            s.tactics.attack_wait = Some((
                                id,
                                game.ticks,
                                f64::from(look.0),
                                game.player.as_ref().is_some_and(|p| p.sprinting),
                            ))
                        });
                    }
                    if !super::sunset_melee_assist::send_hit_at(game, id, look.0, look.1) {
                        state(|s| s.tactics.attack_wait = None);
                    }
                }
            }
            BotAction::Xp => {
                if game.mobs.iter().filter(|mob| enemy(game, mob)).all(|mob| {
                    game.player.as_ref().is_some_and(|p| {
                        util::dist(
                            [p.position.x, p.position.y, p.position.z],
                            [mob.position.x, mob.position.y, mob.position.z],
                        ) >= 12.
                    })
                }) && armor_damaged(game, false)
                    && look.1 > 70.
                    && game.ticks.saturating_sub(state(|s| s.tactics.last_xp)) >= 1
                {
                    if super::remote::sunset_combat_remote::use_main_at(game,look.0,look.1) {
                        state(|s| s.tactics.last_xp = game.ticks);
                    }
                }
            }
            BotAction::Pearl(plan) => {
                let still = game.player.as_ref().is_some_and(|p| {
                    p.ground.on_ground
                        && p.velocity.x * p.velocity.x + p.velocity.z * p.velocity.z <= 1e-4
                });
                if still
                    && pearl_ready(game)
                    && game.ticks <= plan.until
                    && pearl_approved(game, plan)
                {
                    if super::remote::sunset_combat_remote::use_main_at(game,look.0,look.1) {
                        pearl_note(7, 1);
                        state(|s| {
                            s.tactics.pearl = None;
                            s.tactics.pearl_at = Some(game.ticks);
                            s.tactics.fight.next_pearl = game.ticks.saturating_add(30);
                        });
                    }
                }
            }
        }
        if game.selected == slot {
            hold(game, prior);
        }
    });
    if !accepted {
        state(|s| s.tactics.queued = None);
    }
    accepted
}
fn send_block_click(game: &mut Game<'_>, click: BotClick) {
    let sequence = util::sequence(game);
    game.send(S::UseItemOn(play::UseItemOn {
        hand: 0,
        pos: click.clicked,
        face: super::remote::sunset_bot_remote::face(click),
        cursor: std::array::from_fn(|i| (click.point[i] - f64::from(click.clicked[i])) as f32),
        inside: false,
        world_border: false,
        sequence,
    }));
    game.swing_arm();
    game.send(S::Punch(horizon_protocol::packets::Empty));
}
fn exact_click(game: &Game<'_>, click: BotClick, look: (f32, f32)) -> bool {
    game.eye_position()
        .and_then(|eye| game.clip_ray(eye, super::view_vector(look.0, look.1), game.reach()))
        .is_some_and(|hit| hit.pos == click.clicked && hit.face == click.normal)
}
fn face(game: &Game<'_>, cell: [i32; 3], rays: &mut usize) -> Option<BotClick> {
    face_from(game, cell, game.eye_position()?, rays)
}
fn face_from(game: &Game<'_>, cell: [i32; 3], eye: [f64; 3], rays: &mut usize) -> Option<BotClick> {
    for bounds in game.outline_boxes(cell).into_iter().take(8) {
        for normal in FACES {
            if *rays == 0 {
                return None;
            }
            let point = std::array::from_fn(|i| {
                if normal[i] > 0 {
                    bounds[i + 3]
                } else if normal[i] < 0 {
                    bounds[i]
                } else {
                    (bounds[i] + bounds[i + 3]) * 0.5
                }
            });
            let d = tactics::sub(point, eye);
            let len = tactics::length(d);
            if len > game.reach() || len <= 1e-8 {
                continue;
            }
            *rays -= 1;
            if let Some(hit) = game.clip_ray(eye, tactics::mul(d, 1. / len), len + 0.025) {
                if hit.pos == cell && hit.face == normal {
                    return Some(BotClick {
                        clicked: cell,
                        normal,
                        point: hit.point,
                    });
                }
            }
        }
    }
    None
}
fn place(
    game: &mut Game<'_>,
    m: &Module,
    cell: [i32; 3],
    item: &str,
    purpose: tactics::Purpose,
    blast: Option<(Blast, f64, f64)>,
) -> bool {
    if !util::can_place(game, cell, true) {
        return false;
    }
    let before = game.world.block(cell[0], cell[1], cell[2]).0;
    if !state(|s| s.tactics.blocks.can_send(cell, before, game.ticks)) {
        return false;
    }
    let Some(slot) = util::find_hotbar(game, |s| s == item) else {
        return false;
    };
    let Some(click) =
        super::remote::sunset_bot_remote::plan(game, cell, slot, game.reach(), &mut 24)
    else {
        return false;
    };
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let look = sunset::look_at(eye, click.point);
    queue_action(
        game,
        m,
        slot,
        look,
        BotAction::Place {
            cell,
            before,
            click,
            purpose,
            blast,
        },
    )
}
fn armor_damaged(game: &Game<'_>, low: bool) -> bool {
    game.armor.iter().flatten().any(|s| {
        game.items().get(&s.item).is_some_and(|i| {
            i.max_damage > 0
                && s.damage > 0
                && (!low
                    || f64::from(i.max_damage.saturating_sub(s.damage)) / f64::from(i.max_damage)
                        < 0.3)
        })
    })
}
fn mend(game: &mut Game<'_>, m: &Module, idle: bool) -> bool {
    if !armor_damaged(game, !idle) {
        return false;
    }
    let Some(slot) = util::find_hotbar(game, |s| s == "experience_bottle") else {
        return false;
    };
    queue_action(game, m, slot, (game.yaw, 86.), BotAction::Xp)
}
fn kit_kind(game: &Game<'_>, stack: &horizon_client::items::ItemStack) -> Option<usize> {
    let name = util::short(&stack.item);
    if name == "end_crystal" {
        Some(0)
    } else if name == "obsidian" {
        Some(1)
    } else if super::sunset_melee_assist::melee_item(game, Some(stack)) {
        Some(2)
    } else if matches!(name, "golden_apple" | "enchanted_golden_apple") {
        Some(3)
    } else if name == "ender_pearl" {
        Some(4)
    } else if name.ends_with("_pickaxe") {
        Some(5)
    } else if name == "experience_bottle" {
        Some(6)
    } else if name == "respawn_anchor" {
        Some(7)
    } else if name == "glowstone" {
        Some(8)
    } else {
        None
    }
}
fn refill(game: &mut Game<'_>, m: &Module) -> bool {
    if game.ticks < state(|s| s.tactics.kit_after)
        || !action_ready(game)
        || super::sunset_combat_inventory::busy_for(BOT_KIT)
    {
        return false;
    }
    for kind in 0..if m.b("Use anchors") { 9 } else { 7 } {
        let (low, top) = match kind {
            0 => (16, 32),
            1 => (8, 16),
            7 | 8 => (2, 4),
            _ => (1, 1),
        };
        let mut total = 0;
        let mut smallest = None;
        for slot in 0..9 {
            if let Some(s) = game.inventory[slot]
                .as_ref()
                .filter(|s| kit_kind(game, s) == Some(kind))
            {
                total += s.count;
                if smallest.is_none_or(|(_, n)| s.count < n) {
                    smallest = Some((slot, s.count));
                }
            }
        }
        if total >= low {
            continue;
        }
        let source = (9..36)
            .filter_map(|slot| {
                game.inventory
                    .get(slot)?
                    .as_ref()
                    .filter(|s| kit_kind(game, s) == Some(kind))
                    .map(|s| (slot, s.count))
            })
            .max_by_key(|(_, n)| *n);
        let Some((from, count)) = source else {
            continue;
        };
        if count <= total || total >= top && count < total + 8 {
            continue;
        }
        let to = smallest
            .map(|(slot, _)| slot)
            .or_else(|| {
                (0..9).find(|slot| game.inventory[*slot].as_ref().is_none_or(|s| s.count == 0))
            })
            .or_else(|| {
                (0..9).rev().find(|slot| {
                    game.inventory[*slot].as_ref().is_some_and(|s| {
                        let name = util::short(&s.item);
                        name != "totem_of_undying"
                            && (kit_kind(game, s).is_none()
                                || kit_kind(game, s).is_some_and(|k| {
                                    (0..*slot).any(|other| {
                                        game.inventory[other]
                                            .as_ref()
                                            .is_some_and(|x| kit_kind(game, x) == Some(k))
                                    })
                                }))
                    })
                })
            });
        if let Some(to) = to {
            if super::sunset_combat_inventory::submit_refill(game, m, BOT_KIT, from, to) {
                state(|s| {
                    s.tactics.kit_after = game.ticks.saturating_add(4);
                    s.tactics.action = "refill waiting for real two-slot ACK";
                });
                return true;
            }
        }
    }
    false
}
fn scan_all_anchors(game: &Game<'_>) {
    let Some(origin) = util::block_pos(game) else {
        return;
    };
    state(|s| {
        let t = &mut s.tactics;
        if t.anchor_origin.is_none_or(|p| p != origin) {
            t.anchor_origin = Some(origin);
            t.anchor_cursor = 0;
            t.anchor_cells.clear();
        }
        t.anchor_cells
            .retain(|p| game.is_anchor(game.world.block(p[0], p[1], p[2])));
        for _ in 0..64 {
            let n = t.anchor_cursor;
            t.anchor_cursor = (n + 1) % 726;
            let p = [
                origin[0] + (n % 11) as i32 - 5,
                origin[1] + (n / 121) as i32 - 2,
                origin[2] + ((n / 11) % 11) as i32 - 5,
            ];
            if game.world.loaded(p[0], p[2])
                && game.is_anchor(game.world.block(p[0], p[1], p[2]))
                && !t.anchor_cells.contains(&p)
                && t.anchor_cells.len() < 64
            {
                t.anchor_cells.push(p);
            }
        }
    });
}
fn in_hole(game: &Game<'_>, cell: [i32; 3]) -> bool {
    SIDES.iter().all(|d| {
        let p = std::array::from_fn(|i| cell[i] + d[i]);
        game.world.loaded(p[0], p[2]) && util::facts(game, p).explosion_resistance >= 600.
    })
}

fn hazard(game: &Game<'_>, cell: [i32; 3]) -> bool {
    let f = util::facts(game, cell);
    f.fluid_level > 0
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
        .contains(&util::short(&f.name))
}
fn standable(game: &Game<'_>, cell: [i32; 3]) -> Option<[f64; 3]> {
    for dy in [0, -1, 1] {
        let cell = [cell[0], cell[1] + dy, cell[2]];
        if !game.world.loaded(cell[0], cell[2]) {
            return None;
        }
        let floor = [cell[0], cell[1] - 1, cell[2]];
        let head = [cell[0], cell[1] + 1, cell[2]];
        if hazard(game, cell)
            || hazard(game, head)
            || hazard(game, floor)
            || !util::facts(game, floor).full_collision
        {
            continue;
        }
        let lo = Vec3::new(
            f64::from(cell[0]) + 0.2,
            f64::from(cell[1]),
            f64::from(cell[2]) + 0.2,
        );
        let hi = Vec3::new(lo.x + 0.6, lo.y + 1.8, lo.z + 0.6);
        if (SceneCollisions {
            world: &game.world,
            binding: game.collision,
            subject: None,
        })
        .unobstructed(horizon_core::geometry::Aabb::new(lo, hi))
        .unwrap_or(false)
        {
            return Some([
                f64::from(cell[0]) + 0.5,
                f64::from(cell[1]),
                f64::from(cell[2]) + 0.5,
            ]);
        }
    }
    None
}
fn walking_path(game: &Game<'_>, from: [f64; 3], to: [f64; 3], allow_drop: bool) -> bool {
    let d = tactics::sub(to, from);
    let n = (tactics::horizontal(d) / 0.5).ceil().max(1.) as usize;
    if n > 40 {
        return false;
    }
    let mut y = from[1].floor() as i32;
    for i in 1..=n {
        let at = tactics::add(from, tactics::mul(d, i as f64 / n as f64));
        let cell = [at[0].floor() as i32, y, at[2].floor() as i32];
        let p = standable(game, cell).or_else(|| {
            if allow_drop {
                (2..=6).find_map(|drop| standable(game, [cell[0], cell[1] - drop, cell[2]]))
            } else {
                None
            }
        });
        let Some(p) = p else {
            return false;
        };
        if (p[1] - f64::from(y)).abs() > if allow_drop { 3. } else { 1. } {
            return false;
        }
        y = p[1] as i32;
    }
    true
}
fn pick_stand(game: &Game<'_>, target: &util::Target, stance: tactics::Stance) {
    state(|s| s.tactics.spot_after = game.ticks.saturating_add(10));
    let Some(own) = util::player_pos(game) else {
        return;
    };
    let base = own.map(|v| v.floor() as i32);
    let mut candidates = Vec::with_capacity(34);
    candidates.push(own);
    for radius in [1.5, 3., 4.] {
        for k in 0..8 {
            let a = k as f64 * std::f64::consts::FRAC_PI_4;
            if let Some(at) = standable(
                game,
                [
                    (own[0] + a.cos() * radius).floor() as i32,
                    base[1],
                    (own[2] + a.sin() * radius).floor() as i32,
                ],
            ) {
                candidates.push(at);
            }
        }
    }
    let mut holes = Vec::with_capacity(6);
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in -1..=1 {
                let cell = [base[0] + dx, base[1] + dy, base[2] + dz];
                if in_hole(game, cell) {
                    if let Some(at) = standable(game, cell) {
                        holes.push(at);
                    }
                }
            }
        }
    }
    holes.sort_by(|a, b| util::dist(own, *a).total_cmp(&util::dist(own, *b)));
    candidates.extend(holes.into_iter().take(6));
    let cover = |at: [f64; 3]| {
        let cell = at.map(|x| x.floor() as i32);
        let closed = SIDES
            .iter()
            .filter(|d| {
                let p = std::array::from_fn(|i| cell[i] + d[i]);
                util::facts(game, p).explosion_resistance >= 600.
            })
            .count();
        if closed == 4 {
            1.
        } else if closed == 3 {
            0.4
        } else {
            0.
        }
    };
    let stay =
        tactics::stand_score(own, own, target.pos, cover(own), stance).unwrap_or(f64::NEG_INFINITY);
    let best = candidates
        .into_iter()
        .filter(|at| walking_path(game, own, *at, false))
        .filter_map(|at| {
            Some((
                at,
                tactics::stand_score(at, own, target.pos, cover(at), stance)?,
            ))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((at, score)) = best {
        if score >= stay + 0.5 && tactics::horizontal(tactics::sub(at, own)) > 0.4 {
            state(|s| s.tactics.stand = Some(at));
        }
    }
}
fn pearl_land(game: &Game<'_>, look: (f32, f32)) -> Option<tactics::PearlLanding> {
    let eye = game.eye_position()?;
    let p = game.player.as_ref()?;
    let motion = [p.velocity.x, p.velocity.y, p.velocity.z];
    let velocity = tactics::pearl_velocity(
        f64::from(look.0),
        f64::from(look.1),
        motion,
        p.ground.on_ground,
    );
    pearl_from(game, [eye[0], eye[1] - 0.1, eye[2]], velocity)
}
fn pearl_from(
    game: &Game<'_>,
    from: [f64; 3],
    velocity: [f64; 3],
) -> Option<tactics::PearlLanding> {
    tactics::pearl_path(from, velocity, |a, b| {
        let cell = a.map(|x| x.floor() as i32);
        if !game.world.loaded(cell[0], cell[2])
            || !game.world.loaded(b[0].floor() as i32, b[2].floor() as i32)
        {
            return None;
        }
        let water = util::facts(game, cell).name.contains("water");
        if a == b {
            return Some((None, water));
        }
        let hit = super::spear::sunset_combat_collision::clip(
            game,
            Vec3::new(a[0], a[1], a[2]),
            Vec3::new(b[0], b[1], b[2]),
        )
        .map(|v| [v.x, v.y, v.z]);
        Some((hit, water))
    })
}
fn safe_pearl_landing(game: &Game<'_>, land: tactics::PearlLanding) -> bool {
    let base = land.at.map(|x| x.floor() as i32);
    let mut floor = None;
    for down in 0..=3 {
        let cell = [base[0], base[1] - down, base[2]];
        if util::facts(game, cell).full_collision {
            floor = Some(cell);
            break;
        }
        if hazard(game, cell) {
            return false;
        }
    }
    let Some(floor) = floor else {
        return false;
    };
    let feet = [floor[0], floor[1] + 1, floor[2]];
    if standable(game, feet).is_none() {
        return false;
    }
    // Keep the landing clear of ledges, lava and low ceilings at every corner.
    for dx in [-0.8, 0.8] {
        for dz in [-0.8, 0.8] {
            let cell = [
                (land.at[0] + dx).floor() as i32,
                feet[1],
                (land.at[2] + dz).floor() as i32,
            ];
            if standable(game, cell).is_none() {
                return false;
            }
        }
    }
    true
}
fn pearl_approved(game: &Game<'_>, plan: PearlPlan) -> bool {
    let Some(land) = pearl_land(game, plan.look) else {
        return false;
    };
    if !safe_pearl_landing(game, land) {
        return false;
    }
    if let Some(spot) = plan.spot {
        let d = tactics::horizontal(tactics::sub(land.at, spot));
        d <= 4. && d >= 0. && (land.at[1] - spot[1]).abs() <= 3.
    } else {
        game.mobs
            .iter()
            .take(512)
            .filter(|m| enemy(game, m))
            .all(|m| util::dist(land.at, [m.position.x, m.position.y, m.position.z]) >= plan.away)
    }
}
fn pearl_ready(game: &Game<'_>) -> bool {
    let Some(slot) = util::find_hotbar(game, |s| s == "ender_pearl") else {
        return false;
    };
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let life = f64::from(p.health.health() + p.health.absorption());
    let totem = game
        .offhand
        .as_ref()
        .is_some_and(|s| util::short(&s.item) == "totem_of_undying");
    let stack = game.inventory[slot].as_ref().unwrap();
    let info = game.items().get(&stack.item);
    let group = horizon_client::cooldowns::group(stack, info.and_then(|i| i.use_cooldown.as_ref()));
    (state(|s| s.tactics.pearl.is_some()) || game.ticks >= state(|s| s.tactics.fight.next_pearl))
        && !game.cooldowns.is_on_cooldown(&group)
        && tactics::pearl_health_ready(life, totem)
}
// Diagnostic only: [entered, not_ready, peer_match, peer_forecast,
// retained_resume, candidates, plans, native_use_true, no_generic_trigger].
// No formatting or I/O until the explicit existing diagnostics() call.
fn pearl_note(index: usize, amount: u32) {
    state(|s| {
        if let Some(counter) = s.pearl_counts.get_mut(index) {
            *counter = counter.saturating_add(amount);
        }
    });
}
fn plan_pearl(game: &mut Game<'_>, m: &Module, target: &util::Target, escape: bool) -> bool {
    pearl_note(0, 1);
    // An incremental search is meaningful only in its exact current bot scene.
    if !state(|s| {
        s.target == Some(target.id)
            && s.session.as_ref().is_some_and(|scene| same(game, scene))
            && s.tactics.pause.state(game.ticks) == tactics::Pause::Go
    }) {
        state(|s| s.tactics.pearl_search = None);
        return false;
    }
    let retained_flight = state(|s| {
        let search = s.tactics.pearl_search?;
        let flight = search.observed_flight?;
        if !search.escape
            && flight.target == target.id
            && game.ticks >= flight.began
            && game.ticks - flight.began <= 10
            && game.ticks >= search.began
            && game.ticks - search.began <= 10
        {
            Some(search)
        } else {
            s.tactics.pearl_search = None;
            None
        }
    });
    if !pearl_ready(game) {
        pearl_note(1, 1);
        return false;
    }
    let own = util::player_pos(game).unwrap_or([0.; 3]);
    let mut spot = target.pos;
    let mut observed_flight = None;
    let r = rival();
    if !escape {
        let health = game
            .player
            .as_ref()
            .map_or(0., |p| p.health.health());
        let d = tactics::horizontal(tactics::sub(spot, own));
        let actual = game
            .creatures
            .projectiles
            .iter()
            .take(128)
            .filter(|p| {
                matches!(
                    p.kind,
                    horizon_client::projectiles::ProjectileKind::EnderPearl
                ) && p.owner == Some(horizon_client::mobs::Who::Mob(target.id))
                    && p.age <= 2
                    && state(|s| s.tactics.last_pearl != Some(p.id))
            })
            .find_map(|p| {
                let pos = [p.position.x, p.position.y, p.position.z];
                let vel = [p.velocity.x, p.velocity.y, p.velocity.z];
                pearl_note(2, 1);
                let landing = pearl_from(game, pos, vel)?;
                pearl_note(3, 1);
                Some((p.id, landing))
            });
        let following = actual.is_some() || retained_flight.is_some();
        let keep_one = count(game, "totem_of_undying") == 0 && health <= 12.
            || !following && r.pearl_low()
                && super::sunset_prediction::effective_health(game, target)
                    .is_none_or(|health| health <= 8.);
        if count(game, "ender_pearl") <= if keep_one { 1 } else { 0 } {
            return false;
        }
        if let Some((id, landing)) = actual {
            state(|s| {
                s.tactics.last_pearl = Some(id);
                if let Some((_, p)) = s.tactics.fight.low_since.as_mut() {
                    *p = true;
                }
            });
            if util::dist(own, landing.at) > 8. {
                spot = landing.at;
                observed_flight = Some(ObservedPearlFlight {
                    target: target.id,
                    peer: id,
                    began: game.ticks,
                });
            }
        } else if let Some(search) = retained_flight {
            pearl_note(4, 1);
            // The observed peer flight was consumed once. Finish its bounded
            // 8-candidate slices without requiring another physical trigger.
            spot = search.spot;
            observed_flight = search.observed_flight;
        } else {
            let motion = tactics::sub(target.pos, target.prev);
            let away = tactics::sub(target.pos, own);
            let running = d >= 12. && motion[0] * away[0] + motion[2] * away[2] > 0.12;
            let elevation = (target.pos[1] - own[1]).abs() >= 3.;
            let stuck = d > 6.
                && game
                    .ticks
                    .saturating_sub(state(|s| s.tactics.fight.closing_at))
                    >= 60;
            if !running && !elevation && !stuck {
                pearl_note(8, 1);
                return false;
            }
            let ticks = (5. + ((d - 6.).max(0.) * 0.8)).min(40.);
            spot = tactics::add(spot, tactics::mul(motion, ticks));
        }
    }
    let mut search = state(|s| s.tactics.pearl_search)
        .filter(|s| {
            s.escape == escape
                && s.observed_flight == observed_flight
                && game.ticks >= s.began
                && game.ticks - s.began <= 10
                && util::dist(s.spot, spot) <= 3.
        })
        .unwrap_or(PearlSearch {
            escape,
            observed_flight,
            spot,
            began: game.ticks,
            cursor: 0,
            best: None,
        });
    let eye = game.eye_position().unwrap_or(own);
    let direction = if escape {
        let mut middle = [0.; 3];
        let mut n = 0.;
        for mob in game.mobs.iter().take(512).filter(|m| enemy(game, m)) {
            middle = tactics::add(middle, [mob.position.x, mob.position.y, mob.position.z]);
            n += 1.;
        }
        tactics::sub(own, tactics::mul(middle, 1. / f64::max(n, 1.)))
    } else {
        tactics::sub(spot, eye)
    };
    let yaw = (-direction[0]).atan2(direction[2]).to_degrees();
    let max = if escape { 72 } else { 60 };
    let before_cursor = search.cursor;
    for _ in 0..8 {
        if search.cursor >= max {
            break;
        }
        let i = search.cursor;
        search.cursor += 1;
        let (off, pitch) = if escape {
            (((i / 8) as f64 - 4.) * 22.5, -45. + (i % 8) as f64 * 7.)
        } else {
            (
                [0., -1.5, 1.5, -3., 3.][(i / 12) as usize],
                15. - (i % 12) as f64 * 8.,
            )
        };
        let look = ((yaw + off) as f32, pitch as f32);
        let Some(land) = pearl_land(game, look) else {
            continue;
        };
        if !safe_pearl_landing(game, land) {
            continue;
        }
        let nearest = game
            .mobs
            .iter()
            .take(512)
            .filter(|m| enemy(game, m))
            .map(|m| util::dist(land.at, [m.position.x, m.position.y, m.position.z]))
            .fold(f64::INFINITY, f64::min);
        let d = tactics::horizontal(tactics::sub(land.at, spot));
        let good = if escape {
            nearest >= 12.
        } else {
            d <= 4. && (land.at[1] - spot[1]).abs() <= 3.
        };
        if !good {
            continue;
        }
        let score = if escape {
            nearest - 0.05 * f64::from(land.ticks)
        } else {
            -d - 0.01 * f64::from(land.ticks)
        };
        let plan = PearlPlan {
            look,
            spot: if escape { None } else { Some(spot) },
            away: if nearest >= 20. { 20. } else { 12. },
            until: game.ticks.saturating_add(14),
        };
        if search.best.is_none_or(|(old, _)| score > old) {
            search.best = Some((score, plan));
        }
    }
    pearl_note(5, search.cursor.saturating_sub(before_cursor));
    if search.cursor >= max || search.best.is_some_and(|(_, p)| p.away >= 20.) {
        state(|s| {
            s.tactics.pearl_search = None;
            s.tactics.fight.next_pearl = game.ticks.saturating_add(10);
            s.tactics.pearl = search.best.map(|(_, mut p)| {
                p.until = game.ticks.saturating_add(14);
                p
            });
        });
        if state(|s| s.tactics.pearl.is_some()) {
            pearl_note(6, 1);
            return continue_pearl(game, m, target);
        }
    } else {
        state(|s| s.tactics.pearl_search = Some(search));
    }
    false
}
fn continue_pearl(game: &mut Game<'_>, m: &Module, _target: &util::Target) -> bool {
    let Some(plan) = state(|s| s.tactics.pearl) else {
        return false;
    };
    if game.ticks > plan.until {
        state(|s| {
            s.tactics.pearl = None;
            s.tactics.fight.next_pearl = game.ticks.saturating_add(15);
        });
        return false;
    }
    let Some(slot) = util::find_hotbar(game, |s| s == "ender_pearl") else {
        return false;
    };
    queue_action(game, m, slot, plan.look, BotAction::Pearl(plan));
    true
}

// The reference's broad selector includes spears, but its actual ordinary
// attack path rejects PIERCING_WEAPON. Select a hand the native path can use.
fn ordinary_launch_hand(game: &Game<'_>, stack: &horizon_client::items::ItemStack) -> bool {
    if stack.count == 0
        || &*stack.item == "minecraft:mace"
        || !super::sunset_melee_assist::melee_item(game, Some(stack))
        || horizon_client::spear::Kinetic::of(&stack.item).is_some()
    {
        return false;
    }
    let Some(info) = game.items().get(&stack.item) else {
        return false;
    };
    let patch = stack.wire_patch();
    // A changed opaque stack loses its exact preserved component patch. The
    // native engine cannot establish an ordinary attack for that unknown hand.
    if patch.is_none() && stack.extra.is_some() {
        return false;
    }
    let piercing = info
        .components
        .iter()
        .any(|name| name == "minecraft:piercing_weapon")
        || stack.extra.as_ref().is_some_and(|data| {
            data.other
                .iter()
                .any(|(name, _)| name == "minecraft:piercing_weapon")
        });
    super::sunset_bot_packet_policy::ordinary_component_hand(piercing, patch.as_deref())
        == Some(true)
}
fn weapon(game: &Game<'_>, shield: bool) -> Option<usize> {
    (0..9)
        .filter_map(|slot| {
            let s = game.inventory.get(slot)?.as_ref()?;
            if !ordinary_launch_hand(game, s)
                || shield
                    && !game
                        .items()
                        .item_tags
                        .get("minecraft:axes")
                        .is_some_and(|axes| axes.contains(&*s.item))
            {
                return None;
            }
            let info = game.items().get(&s.item)?;
            if info.max_damage > 0
                && s.damage.saturating_add(info.weapon_damage.unwrap_or(1)) >= info.max_damage
            {
                return None;
            }
            Some((slot, info.attack_damage))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(s, _)| s)
}
fn queue_attack(game: &mut Game<'_>, m: &Module, target: &util::Target, launching: bool) -> bool {
    let Some(mob) = game
        .mobs
        .iter()
        .find(|mob| mob.id == target.id && enemy(game, mob))
    else {
        return false;
    };
    if mob.hurt_time > 0 && !launching {
        return false;
    }
    let Some(slot) = weapon(game, mob.extra.blocking) else {
        return false;
    };
    if super::sunset_melee_assist::slot_strength(game, slot, 0.5)
        .is_none_or(|c| c < if launching { 0.95 } else { 0.9 })
    {
        return false;
    }
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let point = std::array::from_fn(|i| eye[i].clamp(target.lo[i] + 0.01, target.hi[i] - 0.01));
    let look = sunset::look_at(eye, point);
    if !super::sunset_melee_assist::hits_target(game, target.id, look.0, look.1, false, 2.95) {
        return false;
    }
    queue_action(
        game,
        m,
        slot,
        look,
        BotAction::Attack {
            id: target.id,
            launch: launching,
        },
    )
}
fn start_launch(
    game: &mut Game<'_>,
    m: &Module,
    target: &util::Target,
    stance: tactics::Stance,
    r: &tactics::Rival,
) -> bool {
    if !matches!(stance, tactics::Stance::Press | tactics::Stance::Hold)
        || r.shrugs()
        || state(|s| {
            s.tactics.pause.slow()
                || game.ticks < s.tactics.fight.next_launch
                || s.tactics.mine.is_some()
        })
    {
        return false;
    }
    let Some(mob) = game.mobs.iter().find(|mob| mob.id == target.id) else {
        return false;
    };
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let reach = super::sunset_melee_policy::box_distance(eye, target.lo, target.hi)
        .unwrap_or(f64::INFINITY);
    if !mob.on_ground
        || in_hole(game, target.pos.map(|x| x.floor() as i32))
        || reach > 3.9
        || weapon(game, mob.extra.blocking).is_none()
        || game
            .crystals
            .iter()
            .any(|c| util::dist([c.position.x, c.position.y, c.position.z], target.pos) <= 3.)
    {
        return false;
    }
    let armed = game.held().is_some_and(|s| ordinary_launch_hand(game, s))
        && super::sunset_melee_assist::slot_strength(game, game.selected, 0.5)
            .is_some_and(|c| c >= 0.5);
    let arm = if armed { 0 } else { r.opener(reach, 0.) };
    if !armed && state(|s| s.tactics.fight.opener.is_none()) {
        state(|s| {
            s.tactics.fight.opener = Some((
                arm,
                reach,
                game.ticks,
                super::sunset_prediction::effective_health(game, target).unwrap_or(f64::INFINITY),
                0,
            ))
        });
    }
    if arm == 1 {
        state(|s| s.tactics.fight.next_launch = game.ticks.saturating_add(40));
        return false;
    }
    state(|s| s.tactics.fight.launching_until = Some(game.ticks.saturating_add(16)));
    launch(game, m, target, stance, r)
}
fn launch(
    game: &mut Game<'_>,
    m: &Module,
    target: &util::Target,
    stance: tactics::Stance,
    _r: &tactics::Rival,
) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let d = super::sunset_melee_policy::box_distance(eye, target.lo, target.hi)
        .unwrap_or(f64::INFINITY);
    if !matches!(stance, tactics::Stance::Press | tactics::Stance::Hold)
        || d > 4.1
        || in_hole(game, target.pos.map(|v| v.floor() as i32))
        || state(|s| {
            s.tactics
                .fight
                .launching_until
                .is_none_or(|until| game.ticks > until)
        })
    {
        state(|s| s.tactics.fight.cancel_launch(game.ticks));
        return false;
    }
    face_spot(
        game,
        [
            target.pos[0],
            target.lo[1] + (target.hi[1] - target.lo[1]) * 0.65,
            target.pos[2],
        ],
        1.,
    );
    if queue_attack(game, m, target, true) {
        return true;
    }
    if d > 2.9 {
        state(|s| s.tactics.stand = Some(target.pos));
    }
    true
}
fn follow_air(game: &mut Game<'_>, m: &Module, target: &util::Target, cap: f64) -> bool {
    let r = rival();
    let req = state(|s| s.tactics.air.request(game.ticks, 40, &r));
    let landing = req
        .and_then(|req| body_forecast(game, target, req))
        .filter(|s| s.landed)
        .map_or(target.pos, |s| s.at);
    let feet = [
        landing[0].floor() as i32,
        (landing[1] + 0.2).floor() as i32,
        landing[2].floor() as i32,
    ];
    if m.b("Use anchors")
        && game.dim != Dim::Nether
        && hand_has(game, "glowstone", false)
        && game
            .mobs
            .iter()
            .find(|m| m.id == target.id)
            .is_some_and(|m| !m.on_ground)
    {
        let f = util::facts(game, [feet[0], feet[1] - 1, feet[2]]);
        if f.full_collision
            && safe_blast(game, Blast::anchor(feet), target.id, 6., cap.min(11.))
            && place(
                game,
                m,
                feet,
                "respawn_anchor",
                tactics::Purpose::LandingAnchor,
                Some((Blast::anchor(feet), 6., cap.min(11.))),
            )
        {
            state(|s| s.tactics.fight.follow_laid = true);
            return true;
        }
    }
    let mut candidates = Vec::with_capacity(16);
    for dx in -1..=1 {
        for dz in -1..=1 {
            if dx == 0 && dz == 0 {
                continue;
            }
            for dy in [-1, 0] {
                let cell = [feet[0] + dx, feet[1] + dy, feet[2] + dz];
                if util::can_place(game, cell, true)
                    && util::facts(game, [cell[0], cell[1] - 1, cell[2]]).full_collision
                {
                    candidates.push(cell);
                }
            }
        }
    }
    for cell in candidates.into_iter().take(16) {
        let center = [
            f64::from(cell[0]) + 0.5,
            f64::from(cell[1]) + 1.,
            f64::from(cell[2]) + 0.5,
        ];
        let blast = Blast::explosion(center, 6.);
        if safe_blast(game, blast, target.id, 4., cap)
            && place(
                game,
                m,
                cell,
                "obsidian",
                tactics::Purpose::LandingBase,
                Some((blast, 4., cap)),
            )
        {
            state(|s| s.tactics.fight.follow_laid = true);
            return true;
        }
    }
    state(|s| s.tactics.fight.follow_laid = true);
    false
}
fn tool_for(game: &Game<'_>, cell: [i32; 3], max_ticks: u64) -> Option<usize> {
    let facts = util::facts(game, cell);
    if !facts.hardness.is_finite() || facts.hardness < 0. || hazard(game, cell) {
        return None;
    }
    (0..9)
        .filter_map(|slot| {
            let s = game.inventory.get(slot)?.as_ref()?;
            if s.count == 0
                || !s.item.ends_with("_pickaxe")
                || !game.items().correct_for_drops(Some(s), &facts.name)
            {
                return None;
            }
            let info = game.items().get(&s.item)?;
            if info.max_damage > 0
                && s.damage
                    .saturating_add(info.tool_damage_per_block.unwrap_or(1))
                    >= info.max_damage
            {
                return None;
            }
            let mut speed = game.items().mining_speed(Some(s), &facts.name);
            if speed > 1. {
                speed += game
                    .stations()
                    .enchantments
                    .attribute_add(Some(s), "minecraft:mining_efficiency")
                    as f32;
            }
            speed *= game.potions.effects.mining_speed_factor();
            let ticks = if facts.hardness == 0. {
                1.
            } else {
                facts.hardness * 30. / speed
            };
            if !ticks.is_finite() || ticks > max_ticks as f32 {
                return None;
            }
            Some((slot, speed))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(s, _)| s)
}
// Keep native mining alive only for this exact live bot scene and owned cell.
// Physical input, a different tool, pause, stale lease or scene keeps vanilla's abort.
pub(super) fn suppress_idle_abort(game: &Game<'_>, down: bool) -> bool {
    if down || !action_ready(game) {
        return false;
    }
    state(|s| {
        s.session.as_ref().is_some_and(|scene| same(game, scene))
            && s.tactics.pause.state(game.ticks) == tactics::Pause::Go
            && s.tactics.mine.as_ref().is_some_and(|mine| {
                mine.done.current()
                    && mine.applied
                    && game.selected == mine.slot
                    && game.ticks >= mine.began
                    && game.ticks - mine.began <= 80
                    && super::remote::sunset_city_mining::progress(game, mine.cell).is_some()
            })
    })
}
fn stop_mine(game: &mut Game<'_>) {
    sunset::cancel_turns(BOT_MINE);
    let mine = state(|s| {
        s.tactics.generation = s.tactics.generation.wrapping_add(1);
        s.tactics.queued = None;
        s.tactics.mine.take()
    });
    if let Some(mine) = mine {
        if mine.done.current() {
            super::remote::sunset_city_mining::stop(game, mine.cell);
            if mine.applied && game.selected == mine.slot {
                hold(game, mine.previous);
            }
        }
    }
}
fn mine(
    game: &mut Game<'_>,
    m: &Module,
    cell: [i32; 3],
    limit: u64,
    guard: Option<(Blast, f64, f64)>,
) -> bool {
    if state(|s| {
        s.tactics
            .mine
            .as_ref()
            .is_some_and(|mine| mine.cell != cell)
    }) {
        stop_mine(game);
    }
    if util::facts(game, cell).replaceable {
        stop_mine(game);
        return false;
    }
    let Some(slot) = tool_for(game, cell, limit) else {
        stop_mine(game);
        return false;
    };
    let Some(click) = face(game, cell, &mut 48) else {
        return false;
    };
    if state(|s| {
        s.tactics.mine.as_ref().is_some_and(|mine| {
            !mine.done.current()
                || mine.applied && game.selected != mine.slot
                || game.ticks.saturating_sub(mine.began) > 80
        })
    }) {
        stop_mine(game);
        return false;
    }
    if !control::claim(BOT_MINE) {
        return false;
    }
    if state(|s| s.tactics.mine.is_none()) {
        let done = control::finish(BOT_MINE);
        if !done.current() {
            return false;
        }
        state(|s| {
            s.tactics.mine = Some(BotMine {
                done,
                cell,
                slot,
                previous: game.selected,
                began: game.ticks,
                applied: false,
            })
        });
    }
    if state(|s| s.tactics.queued.is_some()) {
        return true;
    }
    let Some(connection) = session(game) else {
        return false;
    };
    let snapshot = m.clone();
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let look = sunset::look_at(eye, click.point);
    let queued = Instant::now();
    let ticket_generation = state(|s| {
        s.tactics.queued = Some(queued);
        s.tactics.generation
    });
    let accepted = sunset::try_turn_then(BOT_MINE, look.0, look.1, move |game| {
        if !state(|s| {
            s.tactics.generation == ticket_generation
                && s.tactics
                    .mine
                    .as_ref()
                    .is_some_and(|m| m.done.current() && m.cell == cell)
        }) {
            return;
        }
        state(|s| s.tactics.queued = None);
        if queued.elapsed() > Duration::from_millis(200)
            || !same(game, &connection)
            || !module().is_some_and(|now| now == snapshot)
            || !action_ready(game)
            || tool_for(game, cell, limit) != Some(slot)
            || !exact_click(game, click, look)
        {
            return;
        }
        let id = state(|s| s.target);
        if guard.is_some_and(|(b, min, cap)| id.is_none_or(|id| !safe_blast(game, b, id, min, cap)))
        {
            stop_mine(game);
            return;
        }
        hold(game, slot);
        state(|s| {
            if let Some(mine) = s.tactics.mine.as_mut() {
                mine.applied = true;
            }
        });
        super::remote::sunset_city_mining::step(game, cell, click.normal, look.0, look.1);
    });
    if !accepted {
        state(|s| s.tactics.queued = None);
    }
    true
}
fn breach(
    game: &mut Game<'_>,
    m: &Module,
    target: &util::Target,
    r: &tactics::Rival,
    stance: tactics::Stance,
    cap: f64,
) -> bool {
    if stance == tactics::Stance::Run
        || game
            .player
            .as_ref()
            .is_none_or(|p| p.health.health() + p.health.absorption() < 8.)
    {
        return false;
    }
    let feet = target.pos.map(|x| x.floor() as i32);
    if !in_hole(game, feet) {
        if state(|s| s.tactics.mine.is_some()) {
            stop_mine(game);
            state(|s| s.tactics.fight.next_breach = game.ticks.saturating_add(100));
        }
        return false;
    }
    if game.ticks < state(|s| s.tactics.fight.next_breach)
        || game
            .ticks
            .saturating_sub(state(|s| s.tactics.fight.progress_at))
            < if r.camper() { 15 } else { 40 }
    {
        return false;
    }
    if let Some(cell) = state(|s| s.tactics.mine.as_ref().map(|m| m.cell)) {
        if mine(
            game,
            m,
            cell,
            80,
            Some((
                Blast::explosion(
                    [
                        f64::from(cell[0]) + 0.5,
                        f64::from(cell[1]),
                        f64::from(cell[2]) + 0.5,
                    ],
                    6.,
                )
                .ignoring(cell),
                0.,
                cap,
            )),
        ) {
            return true;
        }
        state(|s| {
            s.tactics.fight.city_fails = s.tactics.fight.city_fails.saturating_add(1);
            s.tactics.fight.next_breach = game.ticks.saturating_add(100);
        });
        return false;
    }
    let mut choices = SIDES
        .map(|d| std::array::from_fn(|i| feet[i] + d[i]))
        .into_iter()
        .collect::<Vec<_>>();
    if state(|s| s.tactics.fight.city_fails >= 2) {
        choices.insert(0, [feet[0], feet[1] + 2, feet[2]]);
    }
    choices.sort_by(|a, b| {
        game.eye_position()
            .map_or(0., |eye| util::dist(eye, util::center_of(*a)))
            .total_cmp(
                &game
                    .eye_position()
                    .map_or(0., |eye| util::dist(eye, util::center_of(*b))),
            )
    });
    for cell in choices {
        let center = [
            f64::from(cell[0]) + 0.5,
            f64::from(cell[1]),
            f64::from(cell[2]) + 0.5,
        ];
        let blast = Blast::explosion(center, 6.).ignoring(cell);
        if safe_blast(game, blast, target.id, 2.5, cap)
            && mine(game, m, cell, 80, Some((blast, 2.5, cap)))
        {
            return true;
        }
    }
    if state(|s| s.tactics.fight.city_fails >= 2)
        || util::find_hotbar(game, |n| n.ends_with("_pickaxe")).is_none()
    {
        if camp(game, m, target, cap) {
            return true;
        }
    }
    // Roof access remains a native reachable-face problem. Walk to a bounded
    // reachable vantage before retrying, rather than mining through a wall.
    let roof = [feet[0], feet[1] + 2, feet[2]];
    if !util::facts(game, roof).replaceable && tool_for(game, roof, 80).is_some() {
        let own = util::player_pos(game).unwrap_or(target.pos);
        let mut best = None;
        for dx in -4..=4 {
            for dz in -4..=4 {
                let Some(at) = standable(
                    game,
                    [
                        (own[0] as i32) + dx,
                        own[1].floor() as i32,
                        (own[2] as i32) + dz,
                    ],
                ) else {
                    continue;
                };
                if walking_path(game, own, at, false)
                    && util::dist([at[0], at[1] + 1.62, at[2]], util::center_of(roof))
                        < game.reach() - 0.2
                    && face_from(game, roof, [at[0], at[1] + 1.62, at[2]], &mut 24).is_some()
                    && best.is_none_or(|(_, d)| util::dist(own, at) < d)
                {
                    best = Some((at, util::dist(own, at)));
                }
            }
        }
        if let Some((at, _)) = best {
            state(|s| s.tactics.stand = Some(at));
            return true;
        }
    }
    false
}

fn detonator(game: &Game<'_>) -> Option<usize> {
    (0..9).find(|slot| {
        game.inventory[*slot].as_ref().is_some_and(|s| {
            s.count > 0
                && util::short(&s.item) != "glowstone"
                && game.items().get(&s.item).is_some_and(|i| {
                    i.block.is_none() && i.food.is_none() && i.blocks_attacks.is_none()
                })
        })
    })
}
fn click_anchor(
    game: &mut Game<'_>,
    m: &Module,
    cell: [i32; 3],
    slot: usize,
    guard: Option<(Blast, f64, f64)>,
) -> bool {
    let Some(click) = face(game, cell, &mut 24) else {
        return false;
    };
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let before = game.world.block(cell[0], cell[1], cell[2]).0;
    queue_action(
        game,
        m,
        slot,
        sunset::look_at(eye, click.point),
        BotAction::Click {
            cell,
            before,
            click,
            blast: guard,
        },
    )
}
fn anchor_defense(game: &mut Game<'_>, m: &Module, target: &util::Target, cap: f64) -> bool {
    if game.dim == Dim::Nether || game.ticks < state(|s| s.tactics.guard_after) {
        return false;
    }
    let cells = state(|s| s.tactics.anchor_cells.clone());
    let life = game
        .player
        .as_ref()
        .map_or(0., |p| f64::from(p.health.health() + p.health.absorption()));
    let mut dangerous = None;
    for cell in cells.into_iter().take(64) {
        if state(|s| s.tactics.dig.is_some_and(|d| d.cell == cell))
            || util::dist(util::center_of(cell), target.pos) > 5.5
        {
            continue;
        }
        let blast = Blast::anchor(cell);
        if charged(game, cell)
            && safe_blast(
                game,
                blast,
                target.id,
                0.,
                f64::max(1., 0.15 * life).min(cap),
            )
        {
            if let Some(slot) = detonator(game) {
                if click_anchor(
                    game,
                    m,
                    cell,
                    slot,
                    Some((blast, 0., f64::max(1., 0.15 * life).min(cap))),
                ) {
                    state(|s| s.tactics.guard_after = game.ticks.saturating_add(10));
                    return true;
                }
            }
        }
        let own = if prediction_budget(game, 1) {
            local_damage(game, blast, &mut Budget::new(1))
        } else {
            None
        }
        .map(|p| safety_damage(p, "Observed equipment"));
        if own.is_none_or(|d| d >= f64::max(4., 0.3 * life) || d >= life - 1.) {
            dangerous = Some(cell);
            break;
        }
    }
    let Some(anchor) = dangerous else {
        return false;
    };
    let Some(own) = util::player_pos(game) else {
        return false;
    };
    let center = util::center_of(anchor);
    let direction = tactics::unit([own[0] - center[0], 0., own[2] - center[2]]);
    let shield = [
        (center[0] + direction[0]).floor() as i32,
        own[1].floor() as i32,
        (center[2] + direction[2]).floor() as i32,
    ];
    let guard_item = guard_item(game);
    for dy in [0, 1] {
        let cell = [shield[0], shield[1] + dy, shield[2]];
        if guard_item
            .as_ref()
            .is_some_and(|item| place(game, m, cell, item, tactics::Purpose::AnchorCover, None))
        {
            state(|s| s.tactics.guard_after = game.ticks.saturating_add(20));
            return true;
        }
    }
    // A missing real support face or block is a flee decision, not simulated
    // protective geometry credited by a damage estimate.
    let flee = tactics::add(own, tactics::mul(direction, 3.));
    if walking_path(game, own, flee, false) {
        state(|s| s.tactics.stand = Some(flee));
        return true;
    }
    false
}
fn surround_cells(game: &Game<'_>) -> Vec<[i32; 3]> {
    let Some(p) = game.player.as_ref() else {
        return Vec::new();
    };
    let b = p.bounds();
    let lo = b.min_position();
    let hi = b.max_position();
    let y = (lo.y + 0.01).floor() as i32;
    let mut ring = Vec::with_capacity(12);
    for x in lo.x.floor() as i32..=(hi.x - 1e-6).floor() as i32 {
        for z in lo.z.floor() as i32..=(hi.z - 1e-6).floor() as i32 {
            for d in SIDES {
                let cell = [x + d[0], y, z + d[2]];
                let inside = f64::from(cell[0]) + 1. > lo.x
                    && f64::from(cell[0]) < hi.x
                    && f64::from(cell[2]) + 1. > lo.z
                    && f64::from(cell[2]) < hi.z;
                if !inside && !ring.contains(&cell) && ring.len() < 12 {
                    ring.push(cell);
                }
            }
        }
    }
    ring
}
fn surround(game: &mut Game<'_>, m: &Module) -> bool {
    if !game.player.as_ref().is_some_and(|p| p.ground.on_ground) {
        return false;
    }
    let Some(origin) = util::block_pos(game) else {
        return false;
    };
    state(|s| {
        if s.tactics.box_origin != Some(origin) || game.ticks > s.tactics.box_until {
            s.tactics.box_origin = Some(origin);
            s.tactics.box_until = game.ticks.saturating_add(60);
        }
        s.tactics.skipped.retain(|(_, until)| game.ticks < *until);
    });
    let eye = game.eye_position().unwrap_or([0.; 3]);
    let mut ring = surround_cells(game);
    ring.sort_by(|a, b| {
        util::dist(eye, util::center_of(*b)).total_cmp(&util::dist(eye, util::center_of(*a)))
    });
    for cell in ring {
        if state(|s| s.tactics.skipped.iter().any(|(p, _)| *p == cell)) {
            continue;
        }
        if place(game, m, cell, "obsidian", tactics::Purpose::Surround, None) {
            return true;
        }
    }
    false
}
fn close_danger(game: &mut Game<'_>, m: &Module, target: &util::Target) -> bool {
    if game.ticks < state(|s| s.tactics.spot_after) {
        return false;
    }
    state(|s| s.tactics.spot_after = game.ticks.saturating_add(5));
    let Some(origin) = util::block_pos(game) else {
        return false;
    };
    let mut best = None;
    let mut second = 0.0f64;
    let mut evaluated = 0;
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in -2..=1 {
                let base = [origin[0] + dx, origin[1] + dy, origin[2] + dz];
                let f = util::facts(game, base);
                if !matches!(util::short(&f.name), "obsidian" | "bedrock") {
                    continue;
                }
                let cell = [base[0], base[1] + 1, base[2]];
                if !util::can_place(game, cell, true)
                    || util::dist(target.pos, util::center_of(cell)) > 4.6
                {
                    continue;
                }
                if evaluated >= 4 {
                    break;
                }
                evaluated += 1;
                let blast = Blast::explosion(
                    [
                        f64::from(base[0]) + 0.5,
                        f64::from(base[1]) + 1.,
                        f64::from(base[2]) + 0.5,
                    ],
                    6.,
                );
                if !prediction_budget(game, 2) {
                    continue;
                }
                let mut budget = Budget::new(2);
                let own = local_damage(game, blast, &mut budget)
                    .map(|p| safety_damage(p, "Observed equipment"));
                let theirs =
                    super::sunset_explosive_damage::target_damage(game, target, blast, &mut budget);
                if let (Some(own), Some(theirs)) = (own, theirs) {
                    if own >= 8. && theirs.estimate < own + 2. {
                        if best.is_none_or(|(_, old)| own > old) {
                            second = best.map_or(second, |(_, old)| old);
                            best = Some((cell, own));
                        } else {
                            second = second.max(own);
                        }
                    }
                }
            }
        }
    }
    if let Some((cell, damage)) = best {
        if second <= damage / 2. {
            return place(
                game,
                m,
                cell,
                "obsidian",
                tactics::Purpose::CloseDanger,
                None,
            );
        }
    }
    false
}
fn lower_enemy(game: &mut Game<'_>, m: &Module, target: &util::Target, cap: f64) -> bool {
    let Some(own) = util::player_pos(game) else {
        return false;
    };
    let both_ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground)
        && game
            .mobs
            .iter()
            .find(|m| m.id == target.id)
            .is_some_and(|m| m.on_ground);
    if !both_ground {
        state(|s| s.tactics.fight.below_since = None);
        return false;
    }
    let since = state(|s| *s.tactics.fight.below_since.get_or_insert(game.ticks));
    if game.ticks.saturating_sub(since) < 20 {
        return false;
    }
    if let Some(dig) = state(|s| s.tactics.dig) {
        if game.ticks > dig.until {
            state(|s| {
                s.tactics.dig = None;
                s.tactics.dig_after = game.ticks.saturating_add(100);
            });
            return false;
        }
        let is_anchor = game.is_anchor(game.world.block(dig.cell[0], dig.cell[1], dig.cell[2]));
        let acknowledged = state(|s| {
            s.tactics.blocks.acknowledged(
                dig.cell,
                game.world.block(dig.cell[0], dig.cell[1], dig.cell[2]).0,
            )
        });
        if !is_anchor {
            if dig.observed {
                state(|s| {
                    s.tactics.dig = None;
                    s.tactics.dig_blows = s.tactics.dig_blows.saturating_add(1);
                    s.tactics.dig_walk = game.ticks.saturating_add(40);
                    s.tactics.dig_after = game.ticks.saturating_add(100);
                    s.tactics.stand = Some(target.pos);
                });
                return true;
            }
            return true;
        }
        if !acknowledged && !dig.observed {
            return true;
        }
        state(|s| {
            if let Some(d) = s.tactics.dig.as_mut() {
                d.observed = true;
            }
        });
        let blast = Blast::anchor(dig.cell);
        if !charged(game, dig.cell) {
            if let Some(slot) = util::find_hotbar(game, |s| s == "glowstone") {
                return click_anchor(game, m, dig.cell, slot, Some((blast, 0., cap.min(10.))));
            }
            return true;
        }
        if safe_blast(game, blast, target.id, 0., cap.min(10.)) {
            if let Some(slot) = detonator(game) {
                return click_anchor(game, m, dig.cell, slot, Some((blast, 0., cap.min(10.))));
            }
        }
        if dig.guards < 2 {
            let way = tactics::unit(tactics::sub(own, util::center_of(dig.cell)));
            let cell = [
                dig.cell[0] + way[0].round() as i32,
                own[1].floor() as i32 + i32::from(dig.guards),
                dig.cell[2] + way[2].round() as i32,
            ];
            if place(
                game,
                m,
                cell,
                "obsidian",
                tactics::Purpose::AnchorCover,
                None,
            ) {
                state(|s| {
                    if let Some(d) = s.tactics.dig.as_mut() {
                        d.guards += 1;
                    }
                });
                return true;
            }
        }
        return true;
    }
    // An actual clear descent is preferable to an explosion. Native walking
    // still owns the movement and gravity; the bot never fabricates a fall.
    for dx in -2..=2 {
        for dz in -2..=2 {
            for dy in 2..=6 {
                let cell = [
                    own[0].floor() as i32 + dx,
                    own[1].floor() as i32 - dy,
                    own[2].floor() as i32 + dz,
                ];
                if let Some(at) = standable(game, cell) {
                    if at[1] <= target.pos[1] + 1.
                        && walking_path(game, own, at, true)
                        && game
                            .player
                            .as_ref()
                            .is_some_and(|p| p.health.health() + p.health.absorption() > 8.)
                    {
                        state(|s| {
                            s.tactics.stand = Some(at);
                            s.tactics.drop_y = Some(at[1]);
                        });
                        return true;
                    }
                }
            }
        }
    }
    if m.b("Use anchors")
        && game.dim != Dim::Nether
        && own[1] - target.pos[1] <= 6.
        && state(|s| s.tactics.dig_blows < 2 && game.ticks >= s.tactics.dig_after)
        && hand_has(game, "respawn_anchor", false)
        && hand_has(game, "glowstone", false)
    {
        let mut cells = Vec::with_capacity(81);
        for dx in -4..=4 {
            for dz in -4..=4 {
                for dy in -2..=0 {
                    let cell = [
                        own[0].floor() as i32 + dx,
                        own[1].floor() as i32 + dy,
                        own[2].floor() as i32 + dz,
                    ];
                    if util::can_place(game, cell, true)
                        && util::dist(own, util::center_of(cell)) <= game.reach()
                    {
                        cells.push(cell);
                    }
                }
            }
        }
        cells.sort_by(|a, b| {
            util::dist(target.pos, util::center_of(*a))
                .total_cmp(&util::dist(target.pos, util::center_of(*b)))
        });
        for cell in cells.into_iter().take(8) {
            let blast = Blast::anchor(cell);
            if safe_blast(game, blast, target.id, 0., cap.min(10.))
                && place(
                    game,
                    m,
                    cell,
                    "respawn_anchor",
                    tactics::Purpose::DigAnchor,
                    Some((blast, 0., cap.min(10.))),
                )
            {
                state(|s| {
                    s.tactics.dig = Some(DigAnchor {
                        cell,
                        until: game.ticks.saturating_add(80),
                        guards: 0,
                        observed: false,
                    })
                });
                return true;
            }
        }
    }
    m.b("Chase pearls") && plan_pearl(game, m, target, false)
}
fn lower_terrain(
    game: &mut Game<'_>,
    m: &Module,
    target: &util::Target,
    stance: tactics::Stance,
) -> bool {
    if stance == tactics::Stance::Run
        || state(|s| {
            s.tactics.fight.launching_until.is_some()
                || game.ticks <= s.tactics.fight.follow_until.saturating_add(20)
        })
    {
        return false;
    }
    let Some(own) = util::player_pos(game) else {
        return false;
    };
    if !game.player.as_ref().is_some_and(|p| p.ground.on_ground)
        || !game
            .mobs
            .iter()
            .find(|m| m.id == target.id)
            .is_some_and(|m| m.on_ground)
        || in_hole(game, target.pos.map(|x| x.floor() as i32))
        || util::dist(own, target.pos) > 5.5
    {
        return false;
    }
    let y = (own[1] + 0.01).floor() as i32;
    let theirs = (target.pos[1] + 0.01).floor() as i32;
    if y == theirs {
        for dx in -2..=2 {
            for dz in -2..=2 {
                let cell = [
                    own[0].floor() as i32 + dx,
                    y - 1,
                    own[2].floor() as i32 + dz,
                ];
                if let Some(at) = standable(game, cell) {
                    let d = tactics::horizontal(tactics::sub(at, target.pos));
                    if at[1] == f64::from(y - 1)
                        && (2.4..=4.5).contains(&d)
                        && walking_path(game, own, at, true)
                    {
                        state(|s| s.tactics.stand = Some(at));
                        return true;
                    }
                }
            }
        }
        if game.ticks >= state(|s| s.tactics.fight.next_dig) {
            let under = [own[0].floor() as i32, y - 1, own[2].floor() as i32];
            let below = [under[0], under[1] - 1, under[2]];
            let safe_floor = util::facts(game, below).full_collision
                && !FACES
                    .iter()
                    .any(|d| hazard(game, std::array::from_fn(|i| under[i] + d[i])));
            if safe_floor
                && (hand_has(game, "obsidian", false)
                    || matches!(
                        util::short(&util::facts(game, under).name),
                        "obsidian" | "bedrock"
                    ))
                && mine(game, m, under, 50, None)
            {
                return true;
            }
            state(|s| s.tactics.fight.next_dig = game.ticks.saturating_add(100));
        }
    }
    if y == theirs - 1
        && hand_has(game, "obsidian", false)
        && game.ticks >= state(|s| s.tactics.fight.next_base_dig)
    {
        let feet = target.pos.map(|x| x.floor() as i32);
        for dx in -2..=2 {
            for dz in -2..=2 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let cell = [feet[0] + dx, feet[1] - 1, feet[2] + dz];
                let top = [
                    f64::from(cell[0]) + 0.5,
                    f64::from(cell[1]) + 1.,
                    f64::from(cell[2]) + 0.5,
                ];
                if util::dist(own, top) < 2.
                    || util::dist(top, target.pos) > 2.5
                    || !util::facts(game, [cell[0], cell[1] + 1, cell[2]]).replaceable
                    || FACES
                        .iter()
                        .any(|d| hazard(game, std::array::from_fn(|i| cell[i] + d[i])))
                {
                    continue;
                }
                if mine(game, m, cell, 25, None) {
                    return true;
                }
            }
        }
        state(|s| s.tactics.fight.next_base_dig = game.ticks.saturating_add(100));
    }
    false
}
fn crystal_site(game: &Game<'_>, base: [i32; 3]) -> bool {
    if !game.world.loaded(base[0], base[2])
        || !matches!(
            util::short(&util::facts(game, base).name),
            "obsidian" | "bedrock"
        )
        || !util::is_air(game, [base[0], base[1] + 1, base[2]])
    {
        return false;
    }
    let low = Vec3::new(
        f64::from(base[0]),
        f64::from(base[1]) + 1.,
        f64::from(base[2]),
    );
    let high = Vec3::new(low.x + 1., low.y + 2., low.z + 1.);
    let volume = horizon_core::geometry::Aabb::new(low, high);
    !game
        .player
        .as_ref()
        .is_some_and(|p| p.bounds().intersects(volume))
        && !game
            .mobs
            .iter()
            .any(|m| !m.removed && m.bounds().intersects(volume))
        && !game.crystals.iter().any(|c| {
            c.position.x - 1. < high.x
                && c.position.x + 1. > low.x
                && c.position.y < high.y
                && c.position.y + 2. > low.y
                && c.position.z - 1. < high.z
                && c.position.z + 1. > low.z
        })
}
fn camp(game: &mut Game<'_>, m: &Module, target: &util::Target, cap: f64) -> bool {
    if game
        .crystals
        .iter()
        .filter(|c| util::dist([c.position.x, c.position.y, c.position.z], target.pos) <= 3.)
        .count()
        >= 2
    {
        return false;
    }
    let Some(slot) = util::find_hotbar(game, |s| s == "end_crystal") else {
        return false;
    };
    let Some(own) = util::player_pos(game) else {
        return false;
    };
    let feet = target.pos.map(|x| x.floor() as i32);
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            if dx.abs().max(dz.abs()) != 2 {
                continue;
            }
            let base = [feet[0] + dx, feet[1] - 1, feet[2] + dz];
            if f64::from(base[1]) < own[1].floor()
                || !crystal_site(game, base)
                || !state(|s| s.tactics.camp.can_send(base, game.ticks))
            {
                continue;
            }
            let point = [
                f64::from(base[0]) + 0.5,
                f64::from(base[1]) + 1.,
                f64::from(base[2]) + 0.5,
            ];
            let blast = Blast::explosion(point, 6.);
            if !safe_blast(game, blast, target.id, 0., (cap - 2.).max(0.)) {
                continue;
            }
            let Some(click) = face(game, base, &mut 24).filter(|c| c.normal == [0, 1, 0]) else {
                continue;
            };
            let Some(eye) = game.eye_position() else {
                return false;
            };
            if queue_action(
                game,
                m,
                slot,
                sunset::look_at(eye, click.point),
                BotAction::Crystal {
                    base,
                    blast,
                    cap: (cap - 2.).max(0.),
                },
            ) {
                return true;
            }
        }
    }
    false
}

fn prediction_budget(game: &Game<'_>, cost: u8) -> bool {
    state(|s| {
        let t = &mut s.tactics;
        if t.budget_tick != game.ticks {
            t.budget_tick = game.ticks;
            t.predictions = 0;
            t.forecast_steps = 0;
        }
        if t.predictions.saturating_add(cost) > 24 {
            return false;
        }
        t.predictions += cost;
        true
    })
}

fn guard_item(game: &Game<'_>) -> Option<String> {
    let slot = util::find_hotbar(game, |n| n == "obsidian").or_else(|| {
        (0..9).find(|slot| {
            game.inventory[*slot].as_ref().is_some_and(|s| {
                game.items()
                    .get(&s.item)
                    .and_then(|i| i.block)
                    .is_some_and(|id| {
                        let f = game.res.facts().get(id);
                        f.full_collision
                            && !f.block_entity
                            && !f.has_class("FallingBlock")
                            && !f.has_class("TntBlock")
                            && !matches!(
                                util::short(&f.name),
                                "glowstone" | "respawn_anchor" | "slime_block" | "honey_block"
                            )
                    })
            })
        })
    })?;
    Some(util::short(&game.inventory[slot].as_ref()?.item).to_owned())
}

fn release_pause_lease() {
    let guard = state(|s| s.tactics.hold_guard.take());
    drop(guard);
}
fn refresh_pause_lease(game: &Game<'_>) {
    if state(|s| s.tactics.pause.state(game.ticks)) != tactics::Pause::Hold {
        release_pause_lease();
        return;
    }
    if !control::claim(BOT_PAUSE) {
        return;
    }
    if state(|s| {
        s.tactics
            .hold_guard
            .as_ref()
            .is_some_and(|guard| guard.current())
    }) {
        return;
    }
    let guard = control::finish(BOT_PAUSE);
    if guard.current() {
        state(|s| s.tactics.hold_guard = Some(guard));
    }
}
