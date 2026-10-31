// Original native melee helpers. References describe behavior; all actions use
// Horizon's actual inventory, movement, item components, packets and spear ray.
use super::sunset::{Category, Module, gui_open, hold, try_turn_then, with};
use super::sunset_combat_control as control;
use super::sunset_melee_policy::{self as policy, SlotLease};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::{
    enchant::EffectContext,
    items::ItemStack,
    mobs::Mob,
    spear::{Kinetic, REACH, hits_in_reach},
};
use horizon_core::{geometry::Vec3, random::LegacyRandom};
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use horizon_world::combat_rules::{self, ArmorWorld};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use winit::keyboard::KeyCode;

const SHIELD: &str = "Shield Breaker";
const MACE: &str = "Mace";
const SPEAR: &str = "Spear Swap";
const TAP: &str = "W Tap";
const JUMP: &str = "Jump Reset";
const FRIEND: &str = "Friend Guard";
const INPUT_KEYS: [KeyCode; 4] = [
    KeyCode::KeyW,
    KeyCode::KeyS,
    KeyCode::ControlLeft,
    KeyCode::Space,
];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(SHIELD,Category::Combat,"Selects a working shield-disabling weapon for a hit into an observed raised shield. Auto also attacks the shield under your crosshair.")
            .pick("Mode",&["Passive","Auto"]).bool("Switch back",true).bool("Mobs",false),
        Module::new(MACE,Category::Combat,"Selects a mace using observed equipment and native enchantment effects. Optional assist attacks a target under your crosshair during a real fall.")
            .bool("Auto swap",true).pick("Pick",&["Damage","Wind Burst"]).bool("Breach swap",false)
            .bool("Swap back",true).bool("Smash assist",false).pick("Smash timing",&["Delayed","Direct"])
            .num("Min fall",3.0,1.5,30.0,0.5).bool("Mobs",false),
        Module::new(SPEAR,Category::Combat,"Buffers a Lunge key and selects a usable enchanted spear. Reach poke optionally jabs an enemy along your view beyond ordinary melee reach.")
            .text("Lunge key","None").bool("Reach poke",false).bool("Switch back",true).bool("Mobs",false),
        Module::new(TAP,Category::Combat,"Briefly releases forward input after a charged sprint hit. S Tap presses backward for the chosen number of ticks.")
            .pick("Mode",&["W Tap","S Tap"]).num("Hold",1.0,1.0,3.0,1.0),
        Module::new(JUMP,Category::Combat,"Requests the native grounded jump after an observed upward knockback packet while sprinting forward."),
        Module::new(FRIEND,Category::Combat,"Blocks direct attacks and piercing spear paths that include a known friend.").bool("Teams",false),
    ]
}

pub(super) fn augment_defaults(modules: &mut Vec<Module>) {
    if let Some(m) = modules.iter_mut().find(|m| m.name == "Trigger Bot") {
        *m = m
            .clone()
            .num("Reaction", 3.0, 0.0, 10.0, 1.0)
            .pick("Crits", &["Smart", "Off", "Only"])
            .bool("Hold attack", false)
            .bool("Weapons only", true)
            .bool("Invisible", false);
    }
    for module in defaults() {
        if !modules.iter().any(|m| m.name == module.name) {
            modules.push(module);
        }
    }
}

#[derive(Clone, Copy)]
struct InputSave {
    before: [bool; 4],
    physical: [bool; 4],
    changed: [bool; 4],
}
#[derive(Default)]
struct State {
    lease: Option<SlotLease>,
    lease_end: Option<control::Finish>,
    queued: Option<&'static str>,
    queued_since: Option<Instant>,
    tick: u64,
    lunge: policy::Request,
    tap_until: u64,
    jump_pending: bool,
    input: Option<InputSave>,
    last_attack: Option<u64>,
    shield_ready: u64,
    smash_sent: bool,
    smash_delayed: bool,
    sent: u64,
    blocked: u64,
    reason: &'static str,
    trigger_target: Option<u32>,
    trigger_seen: u64,
    trigger_next: u64,
    trigger_pending: Option<Instant>,
    aim_before: [f32; 2],
    aim_mouse: Option<Instant>,
    aim_mining: Option<Instant>,
    aim_sign: [f64; 2],
    jump_owner: Option<&'static str>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}
fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}
fn note(why: &'static str) {
    state(|s| s.reason = why);
}

pub(super) fn diagnostics() -> String {
    state(|s| {
        format!(
            "Melee reason={} sent={} blocked={} lease={:?} queued={:?} tap_until={} jump_pending={} smash_sent={}",
            s.reason,
            s.sent,
            s.blocked,
            s.lease,
            s.queued,
            s.tap_until,
            s.jump_pending,
            s.smash_sent
        )
    })
}

fn alive(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(super::sunset::ID)
        && game.remote.is_some()
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && !game.spectator()
        && game.selected < 9
}
fn allowed(game: &Game<'_>) -> bool {
    alive(game)
        && game.native_playing()
        && game.screen == Screen::Playing
        && !gui_open()
        && game.using.is_none()
        && game.mining.is_none()
        && util::inventory_idle(game)
        && !game.right
        && !with(|s| s.is_on("Blink"))
        && game.mobs.len() <= policy::POPULATION_LIMIT
}

pub(super) fn attack_allowed(game: &Game<'_>, owner: &str) -> bool {
    allowed(game)
        && control::available(owner)
        && super::sunset::CRIT_STAGE.load(std::sync::atomic::Ordering::Relaxed) == 0
}

pub(super) fn slot_strength(game: &Game<'_>, slot: usize, partial: f32) -> Option<f32> {
    if slot >= 9 || !partial.is_finite() {
        return None;
    }
    let modifier = match game.inventory.get(slot)? {
        Some(stack) => {
            let info = game.res.items().get(&stack.item)?;
            if !policy::usable(stack.count, stack.damage, info.max_damage) {
                return None;
            }
            f64::from(info.attack_speed)
        }
        None => 0.0,
    };
    let modifiers = game
        .potions
        .effects
        .modifiers()
        .into_iter()
        .filter(|m| m.0 == "minecraft:attack_speed")
        .map(|m| m.2);
    let speed = policy::attack_speed(modifier, modifiers)?;
    (speed > 0.0).then(|| game.hand_strength.scale(partial, speed))
}

// Explicit look, native first contact and collider clipping. The temporary
// server rotation need not move the user's camera to validate the action.
pub(super) fn hits_target(
    game: &Game<'_>,
    id: u32,
    yaw: f32,
    pitch: f32,
    spear: bool,
    range: f64,
) -> bool {
    if game.mobs.len() > policy::POPULATION_LIMIT
        || !yaw.is_finite()
        || !pitch.is_finite()
        || !range.is_finite()
    {
        return false;
    }
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let look = super::view_vector(yaw, pitch);
    if !eye.iter().chain(look.iter()).all(|v| v.is_finite()) {
        return false;
    }
    if spear {
        let boxes: Vec<_> = game
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.alive() && !m.removed && game.riding != Some(m.id))
            .map(|(i, m)| (i, m.bounds()))
            .collect();
        if boxes.iter().any(|(_, b)| {
            let a = b.min_position();
            let z = b.max_position();
            policy::box_distance(eye, [a.x, a.y, a.z], [z.x, z.y, z.z]).is_none()
        }) {
            return false;
        }
        let max = if game.creative {
            REACH.max_creative
        } else {
            REACH.max
        };
        let min = if game.creative {
            REACH.min_creative
        } else {
            REACH.min
        };
        let hits = hits_in_reach(
            &boxes,
            Vec3::new(eye[0], eye[1], eye[2]),
            Vec3::new(look[0], look[1], look[2]),
            game.known_player_movement(),
            (min, max.min(range as f32), REACH.hitbox_margin),
            |a, b| super::spear::sunset_combat_collision::clip(game, a, b),
        );
        return hits.iter().any(|(i, _)| game.mobs[*i].id == id)
            && !hits
                .iter()
                .any(|(i, _)| known_friend(game, game.mobs[*i].id, true));
    }
    let range = range.min(if game.creative { 5.0 } else { 3.0 });
    let mut nearest = None;
    for mob in &game.mobs {
        if !mob.alive() || mob.removed || game.riding == Some(mob.id) {
            continue;
        }
        let t = util::mob_target(mob);
        if policy::box_distance(eye, t.lo, t.hi).is_none() {
            return false;
        }
        if let Some(d) = super::ray_box(eye, look, range, mob.bounds()) {
            if nearest.is_none_or(|(_, old)| d < old) {
                nearest = Some((mob.id, d));
            }
        }
    }
    let Some((hit, d)) = nearest else {
        return false;
    };
    if hit != id {
        return false;
    }
    let end = Vec3::new(
        eye[0] + look[0] * d,
        eye[1] + look[1] * d,
        eye[2] + look[2] * d,
    );
    super::spear::sunset_combat_collision::clip(game, Vec3::new(eye[0], eye[1], eye[2]), end)
        .is_none()
}

// The caller owns and validates the deed. No queued critical packet hop is
// introduced here: the actual fall/cooldown determines the native outcome.
pub(super) fn send_hit(game: &mut Game<'_>, id: u32) -> bool {
    send_hit_at(game, id, game.yaw, game.pitch)
}
pub(super) fn send_hit_at(game: &mut Game<'_>, id: u32, yaw: f32, pitch: f32) -> bool {
    if guard_attack(game, id) {
        return false;
    }
    if let Some(kinetic) = game.held().and_then(|s| Kinetic::of(&s.item)) {
        if guard_stab_at(game, yaw, pitch) {
            return false;
        }
        note_attack(game, id);
        game.send(S::PlayerAction(play::PlayerAction {
            action: 8,
            pos: [0; 3],
            direction: 0,
            sequence: 0,
        }));
        game.hand_strength.reset_attack();
        game.swing_hand(
            horizon_client::first_person::Hand::Main,
            kinetic.attack_animation(),
        );
        game.play_at_feet(kinetic.attack_sound);
    } else {
        note_attack(game, id);
        game.send(S::Attack(play::Attack { id: id as i32 }));
        game.swing_arm();
        game.send(S::Punch(Empty));
        game.reset_attack_strength();
    }
    true
}

pub(super) fn aim_before(game: &Game<'_>) {
    state(|s| s.aim_before = [game.yaw, game.pitch]);
}
pub(super) fn aim_gate(game: &Game<'_>) -> bool {
    if game.mining.is_some() {
        state(|s| s.aim_mining = Some(Instant::now()));
    }
    attack_allowed(game, "Aim Assist")
        && control::owner().is_none()
        && !state(|s| {
            s.aim_mining
                .is_some_and(|t| t.elapsed() < Duration::from_millis(250))
        })
}
pub(super) fn aim_delta(game: &Game<'_>, turn: [f64; 2]) -> [f64; 2] {
    let before = state(|s| s.aim_before);
    let user = [
        f64::from((game.yaw - before[0] + 180.0).rem_euclid(360.0) - 180.0),
        f64::from(game.pitch - before[1]),
    ];
    state(|s| {
        for i in 0..2 {
            if user[i].is_finite() && user[i].abs() > 1.0e-4 {
                s.aim_sign[i] = user[i].signum();
                s.aim_mouse = Some(Instant::now());
            }
        }
    });
    if !aim_gate(game)
        || !state(|s| {
            s.aim_mouse
                .is_some_and(|t| t.elapsed() <= Duration::from_millis(150))
        })
    {
        return [0.0; 2];
    }
    let signs = state(|s| s.aim_sign);
    [
        policy::assisted_axis(turn[0], signs[0]),
        policy::assisted_axis(turn[1], signs[1]),
    ]
}

fn trigger_target(game: &Game<'_>, m: &Module) -> Option<u32> {
    if !attack_allowed(game, "Trigger Bot")
        || (m.b("Hold attack")
            && !game.left
            && !game
                .physical_buttons
                .contains(&winit::event::MouseButton::Left))
        || (m.b("Weapons only") && (game.held().is_none() || !melee_item(game, game.held())))
    {
        return None;
    }
    let spear = game.held().is_some_and(|s| Kinetic::of(&s.item).is_some());
    let id = if spear {
        spear_hits(game)?.first().map(|&i| game.mobs[i].id)?
    } else {
        let (i, d) = game.mob_hit()?;
        if !game.before_block(d) {
            return None;
        }
        game.mobs[i].id
    };
    let target = game.mobs.iter().find(|t| t.id == id)?;
    if !target.alive()
        || target.removed
        || (!m.b("Invisible") && target.invisible)
        || known_friend(game, id, true)
        || !super::sunset::wanted(m, super::sunset::kind_of(target))
        || util::spared(game, m, id, target.kind.id == "minecraft:player")
        || observed_shield(game, id) && best_axe(game).is_none()
    {
        return None;
    }
    Some(id)
}

pub(super) fn trigger_tick(game: &mut Game<'_>, m: &Module) {
    if let Some(queued) = state(|s| s.trigger_pending) {
        if queued.elapsed() > Duration::from_millis(200) || !attack_allowed(game, "Trigger Bot") {
            super::sunset::cancel_turns("Trigger Bot");
            state(|s| s.trigger_pending = None);
        } else {
            let _ = control::claim("Trigger Bot");
        }
        return;
    }
    let Some(id) = trigger_target(game, m) else {
        state(|s| s.trigger_target = None);
        return;
    };
    state(|s| {
        if s.trigger_target != Some(id) {
            s.trigger_target = Some(id);
            s.trigger_seen = game.ticks;
        }
    });
    let seen = state(|s| s.trigger_seen);
    if game.ticks < state(|s| s.trigger_next)
        || game.ticks.saturating_sub(seen) < m.n("Reaction").clamp(0.0, 10.0) as u64
    {
        return;
    }
    let planned_slot = preferred_weapon(game, id).map_or(game.selected, |p| p.slot);
    let Some(scale) = slot_strength(game, planned_slot, 0.5) else {
        return;
    };
    if m.b("Wait for cooldown") && scale < 1.0 {
        return;
    }
    if !policy::critical_ready(
        m.p("Crits"),
        game.critical_attack(scale),
        game.ticks.saturating_sub(seen),
        12,
    ) {
        return;
    }
    if !control::claim("Trigger Bot") {
        return;
    }
    let done = control::finish("Trigger Bot");
    let snapshot = m.clone();
    let Some(session) = game.remote.as_ref().map(|r| r.session.counters.clone()) else {
        return;
    };
    let dim = game.dim;
    let queued = Instant::now();
    let slot = game.selected;
    state(|s| s.trigger_pending = Some(queued));
    let accepted = try_turn_then("Trigger Bot", game.yaw, game.pitch, move |game| {
        let _done = done;
        state(|s| s.trigger_pending = None);
        if !_done.current()
            || game.selected != slot
            || game.dim != dim
            || queued.elapsed() > Duration::from_millis(200)
            || !game
                .remote
                .as_ref()
                .is_some_and(|r| std::sync::Arc::ptr_eq(&session, &r.session.counters))
            || !module("Trigger Bot").is_some_and(|now| now == snapshot)
            || trigger_target(game, &snapshot) != Some(id)
        {
            return;
        }
        let plan = preferred_weapon(game, id);
        if plan.map_or(game.selected, |p| p.slot) != planned_slot {
            return;
        }
        let Some(scale) = slot_strength(game, planned_slot, 0.5) else {
            return;
        };
        if snapshot.b("Wait for cooldown") && scale < 1.0
            || !policy::critical_ready(
                snapshot.p("Crits"),
                game.critical_attack(scale),
                game.ticks.saturating_sub(seen),
                12,
            )
        {
            return;
        }
        let original = game.selected;
        if let Some(plan) = plan {
            hold(game, plan.slot);
        }
        let sent = send_hit(game, id);
        if let Some(plan) = plan {
            if plan.restore && game.selected == plan.slot {
                hold(game, original);
            }
        }
        if sent {
            state(|s| {
                s.trigger_next = game
                    .ticks
                    .saturating_add(if snapshot.b("Wait for cooldown") {
                        1
                    } else {
                        snapshot.n("Delay (ticks)").clamp(0.0, 20.0) as u64
                    });
                s.trigger_seen = game.ticks;
                s.sent = s.sent.saturating_add(1);
            });
        }
    });
    if accepted {
        note("trigger action queued");
    } else {
        state(|s| s.trigger_pending = None);
    }
}
fn available_item<'a>(game: &'a Game<'_>, slot: usize) -> Option<&'a ItemStack> {
    let stack = game.inventory.get(slot)?.as_ref()?;
    let info = game.res.items().get(&stack.item)?;
    policy::usable(stack.count, stack.damage, info.max_damage).then_some(stack)
}
fn melee_item(game: &Game<'_>, stack: Option<&ItemStack>) -> bool {
    let Some(stack) = stack else {
        return true;
    };
    let name = &*stack.item;
    name == "minecraft:mace"
        || name == "minecraft:trident"
        || Kinetic::of(name).is_some()
        || ["minecraft:swords", "minecraft:axes"].iter().any(|tag| {
            game.res
                .items()
                .item_tags
                .get(*tag)
                .is_some_and(|items| items.contains(name))
        })
}
fn known_friend(game: &Game<'_>, id: u32, teams: bool) -> bool {
    let Some(mob) = game
        .mobs
        .iter()
        .find(|m| m.id == id && m.kind.id == "minecraft:player")
    else {
        return false;
    };
    let explicit = game
        .remote_player_name(id)
        .is_some_and(|name| with(|s| s.friends.iter().any(|f| f.eq_ignore_ascii_case(name))));
    explicit
        || (teams
            && horizon_client::scoreboard::teams_allied(
                game.score.board.players_team(&game.options.player_name()),
                game.mob_team(mob.id),
            ))
}
fn enemy(game: &Game<'_>, mob: &Mob, mobs: bool) -> bool {
    mob.alive()
        && !mob.removed
        && (mob.kind.id == "minecraft:player" || mobs)
        && !known_friend(game, mob.id, true)
        && game
            .remote
            .as_ref()
            .is_none_or(|r| mob.id != r.entity_id as u32)
}

// TRUE means block. Root routes outgoing Attack through this as well as the
// native attack entry, so direct/manual clicks receive the same protection.
pub(super) fn guard_attack(game: &Game<'_>, id: u32) -> bool {
    let Some(m) = module(FRIEND) else {
        return false;
    };
    let blocked = alive(game) && known_friend(game, id, m.b("Teams"));
    if blocked {
        state(|s| s.blocked = s.blocked.saturating_add(1));
        note("friend attack blocked");
    }
    blocked
}

fn spear_hits(game: &Game<'_>) -> Option<Vec<usize>> {
    spear_hits_at(game, game.yaw, game.pitch)
}
fn spear_hits_at(game: &Game<'_>, yaw: f32, pitch: f32) -> Option<Vec<usize>> {
    if game.mobs.len() > policy::POPULATION_LIMIT {
        return None;
    }
    let eye = game.eye_position()?;
    if !eye.iter().all(|v| v.is_finite()) || !yaw.is_finite() || !pitch.is_finite() {
        return None;
    }
    if game.mobs.iter().any(|m| {
        let b = m.bounds();
        let a = b.min_position();
        let z = b.max_position();
        policy::box_distance([eye[0], eye[1], eye[2]], [a.x, a.y, a.z], [z.x, z.y, z.z]).is_none()
    }) {
        return None;
    }
    let eye = Vec3::new(eye[0], eye[1], eye[2]);
    let look = super::view_vector(yaw, pitch);
    let look = Vec3::new(look[0], look[1], look[2]);
    let boxes: Vec<_> = game
        .mobs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.alive() && !m.removed && game.riding != Some(m.id))
        .map(|(i, m)| (i, m.bounds()))
        .collect();
    let reach = if game.creative {
        (REACH.min_creative, REACH.max_creative, REACH.hitbox_margin)
    } else {
        (REACH.min, REACH.max, REACH.hitbox_margin)
    };
    Some(
        hits_in_reach(
            &boxes,
            eye,
            look,
            game.known_player_movement(),
            reach,
            |from, to| super::spear::sunset_combat_collision::clip(game, from, to),
        )
        .into_iter()
        .map(|(i, _)| i)
        .collect(),
    )
}

// Spears pierce several entities: checking only the crosshair victim would
// allow a friend behind the first victim to be stabbed by the same action.
pub(super) fn block_stab(game: &Game<'_>) -> bool {
    guard_stab_at(game, game.yaw, game.pitch)
}
pub(super) fn guard_stab_at(game: &Game<'_>, yaw: f32, pitch: f32) -> bool {
    let Some(m) = module(FRIEND) else {
        return false;
    };
    if !alive(game) {
        return false;
    }
    let blocked = spear_hits_at(game, yaw, pitch).is_none_or(|hits| {
        hits.iter()
            .any(|&i| known_friend(game, game.mobs[i].id, m.b("Teams")))
    });
    if blocked {
        state(|s| s.blocked = s.blocked.saturating_add(1));
        note("friend in piercing spear path");
    }
    blocked
}
pub(super) fn stop_guarded_use(game: &mut Game<'_>) -> bool {
    let spear = game
        .using
        .and_then(|(slot, _)| game.stack_in(slot))
        .is_some_and(|s| Kinetic::of(&s.item).is_some());
    if !spear || !block_stab(game) {
        return false;
    }
    // Native MultiPlayerGameMode.releaseUsingItem packet; no kinetic damage is
    // applied here. The authoritative server stops its continuing use.
    game.send(S::PlayerAction(play::PlayerAction {
        action: 6,
        pos: [0; 3],
        direction: 0,
        sequence: 0,
    }));
    game.using = None;
    note("stopped spear use to protect friend");
    true
}

fn observed_shield(game: &Game<'_>, id: u32) -> bool {
    let Some(mob) = game
        .mobs
        .iter()
        .find(|m| m.id == id && m.alive() && !m.removed)
    else {
        return false;
    };
    if !mob.extra.blocking || mob.extra.using_item <= 0 {
        return false;
    }
    let used = if mob.extra.using_off_hand {
        mob.extra.off_hand.as_ref()
    } else {
        mob.extra.main_hand.as_ref()
    };
    let Some(component) = used
        .and_then(|item| game.res.items().get(item))
        .and_then(|i| i.blocks_attacks.as_ref())
    else {
        return false;
    };
    if (mob.extra.using_item as f32) < (component.delay_seconds * 20.0).round() {
        return false;
    }
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    component.reductions.iter().any(|r| {
        (r.factor > 0.0 || r.base > 0.0)
            && policy::facing_attack(
                [mob.position.x, mob.position.y, mob.position.z],
                mob.head_yaw,
                [p.position.x, p.position.y, p.position.z],
                r.horizontal_angle,
            )
    })
}
fn best_axe(game: &Game<'_>) -> Option<usize> {
    (0..9)
        .filter_map(|slot| {
            let stack = available_item(game, slot)?;
            let info = game.res.items().get(&stack.item)?;
            (info.disable_blocking_seconds > 0.0).then_some((slot, info.attack_damage))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(slot, _)| slot)
}

struct ArmorEstimate<'a, 'r> {
    game: &'a Game<'r>,
    stack: &'a ItemStack,
    victim: &'a str,
    random: LegacyRandom,
}
impl ArmorWorld for ArmorEstimate<'_, '_> {
    type Error = std::convert::Infallible;
    type Weapon = ();
    type ServerLevel = ();
    fn weapon(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(Some(()))
    }
    fn server_level(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(Some(()))
    }
    fn modify_armor_effectiveness(&mut self, _: (), _: (), value: f32) -> Result<f32, Self::Error> {
        let ctx = EffectContext {
            this: Some(self.victim),
            attacker: Some("minecraft:player"),
            direct_attacker: Some("minecraft:player"),
            is_direct: true,
            damage_type: Some("minecraft:mace_smash"),
            items: Some(self.game.res.items()),
            ..Default::default()
        };
        Ok(self.game.res.items().enchantments.modify_value(
            Some(self.stack),
            "minecraft:armor_effectiveness",
            &ctx,
            &mut self.random,
            value,
        ))
    }
}

fn estimate_weapon(game: &Game<'_>, stack: &ItemStack, target: &Mob) -> Option<f32> {
    let info = game.res.items().get(&stack.item)?;
    let p = game.player.as_ref()?;
    let smashing = &*stack.item == "minecraft:mace" && p.fall_distance > 1.5 && !p.fall_flying;
    let source = if smashing {
        "minecraft:mace_smash"
    } else {
        "minecraft:player_attack"
    };
    let ctx = EffectContext {
        this: Some(&target.kind.id),
        attacker: Some("minecraft:player"),
        direct_attacker: Some("minecraft:player"),
        is_direct: true,
        damage_type: Some(source),
        items: Some(game.res.items()),
        ..Default::default()
    };
    // Use a private random stream for evaluation. Looking through the hotbar
    // must not advance gameplay RNG, wear equipment, or alter selected items.
    let mut random = LegacyRandom::new(0);
    let mut damage = 1.0 + info.attack_damage + game.effect_attack_bonus();
    damage = game.res.items().enchantments.modify_value(
        Some(stack),
        "minecraft:damage",
        &ctx,
        &mut random,
        damage,
    );
    if smashing {
        let per = game.res.items().enchantments.modify_value(
            Some(stack),
            "minecraft:smash_damage_per_fallen_block",
            &ctx,
            &mut random,
            0.0,
        );
        damage += (policy::smash_bonus(p.fall_distance)? + f64::from(per) * p.fall_distance) as f32;
    }
    let (armor, tough) = target.worn_armor();
    let mut prediction = ArmorEstimate {
        game,
        stack,
        victim: &target.kind.id,
        random,
    };
    let damage = combat_rules::after_armor(
        &mut prediction,
        damage,
        target.kind.armor + armor,
        target.kind.armor_toughness + tough,
    )
    .ok()?;
    (damage.is_finite() && damage >= 0.0).then_some(damage)
}
fn best_mace(game: &Game<'_>, target: &Mob, m: &Module) -> Option<usize> {
    let smash = game
        .player
        .as_ref()
        .is_some_and(|p| p.fall_distance > 1.5 && !p.fall_flying);
    let wind = smash && m.p("Pick") == "Wind Burst";
    (0..9)
        .filter_map(|slot| {
            let stack = available_item(game, slot)?;
            if &*stack.item != "minecraft:mace"
                || (!smash && stack.enchantment_level("minecraft:breach") == 0)
            {
                return None;
            }
            Some((
                slot,
                wind && stack.enchantment_level("minecraft:wind_burst") > 0,
                estimate_weapon(game, stack, target)?,
            ))
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.total_cmp(&b.2)))
        .map(|(slot, _, _)| slot)
}

#[derive(Clone, Copy)]
pub(super) struct WeaponPlan {
    pub slot: usize,
    pub owner: &'static str,
    pub restore: bool,
}
// Pure inventory selection for attacks already owned by another combat
// module. Its caller applies and restores the slot inside that accepted deed.
pub(super) fn preferred_weapon(game: &Game<'_>, id: u32) -> Option<WeaponPlan> {
    if !allowed(game) || !melee_item(game, game.held()) {
        return None;
    }
    let target = game
        .mobs
        .iter()
        .find(|m| m.id == id && m.alive() && !m.removed)?;
    if known_friend(game, id, true) {
        return None;
    }
    if let Some(m) = module(SHIELD) {
        if (target.kind.id == "minecraft:player" || m.b("Mobs"))
            && observed_shield(game, id)
            && !game
                .held()
                .and_then(|s| game.res.items().get(&s.item))
                .is_some_and(|i| i.disable_blocking_seconds > 0.0)
        {
            return best_axe(game).map(|slot| WeaponPlan {
                slot,
                owner: SHIELD,
                restore: m.b("Switch back"),
            });
        }
    }
    let m = module(MACE)?;
    if !m.b("Auto swap")
        || observed_shield(game, id)
        || (target.kind.id != "minecraft:player" && !m.b("Mobs"))
    {
        return None;
    }
    let smash = game
        .player
        .as_ref()
        .is_some_and(|p| p.fall_distance > 1.5 && !p.fall_flying);
    if !smash && !m.b("Breach swap") {
        return None;
    }
    let slot = best_mace(game, target, &m)?;
    if !smash {
        let old = game
            .held()
            .and_then(|s| estimate_weapon(game, s, target))
            .unwrap_or(1.0);
        let next = estimate_weapon(game, available_item(game, slot)?, target)?;
        if next < old * 1.15 {
            return None;
        }
    }
    Some(WeaponPlan {
        slot,
        owner: MACE,
        restore: m.b("Swap back"),
    })
}

fn take_slot(game: &mut Game<'_>, plan: WeaponPlan, hold_ticks: u64) -> bool {
    take_slot_inner(game, plan, hold_ticks, None)
}
fn take_slot_inner(
    game: &mut Game<'_>,
    plan: WeaponPlan,
    hold_ticks: u64,
    queued_end: Option<control::Finish>,
) -> bool {
    if state(|s| s.lease.is_some() && !s.lease_end.as_ref().is_some_and(|end| end.current())) {
        state(|s| {
            s.lease = None;
            s.lease_end.take();
        });
    }
    let old = state(|s| s.lease);
    if old.is_some_and(|l| l.owner != plan.owner)
        || available_item(game, plan.slot).is_none()
        || !control::claim(plan.owner)
    {
        return false;
    }
    let end = if old.is_none() {
        queued_end.or_else(|| Some(control::finish(plan.owner)))
    } else {
        None
    };
    let previous = old.map_or(game.selected, |l| l.previous);
    hold(game, plan.slot);
    state(|s| {
        if let Some(end) = end {
            s.lease_end = Some(end);
        }
        s.lease = Some(SlotLease {
            owner: plan.owner,
            previous,
            selected: plan.slot,
            until: game.ticks.saturating_add(hold_ticks),
            return_enabled: plan.restore,
        })
    });
    true
}

// TRUE permits the original native attack; FALSE suppresses it. Passive
// weapon selection does not turn a manual click into an extra attack.
pub(super) fn before_attack(game: &mut Game<'_>, id: u32) -> bool {
    if guard_attack(game, id) {
        return false;
    }
    if let Some(plan) = preferred_weapon(game, id) {
        let _ = take_slot(game, plan, 2);
    }
    true
}

fn release_lease(game: &mut Game<'_>, force: bool) {
    let Some(lease) = state(|s| s.lease) else {
        return;
    };
    let owned = state(|s| s.lease_end.as_ref().is_some_and(|end| end.current()));
    if force || lease.expired(game.selected, game.ticks, owned) {
        if let Some(slot) = lease.restore(
            game.selected,
            if force { u64::MAX } else { game.ticks },
            owned,
        ) {
            hold(game, slot);
        }
        state(|s| {
            s.lease = None;
            s.lease_end.take();
        });
    } else {
        let _ = control::claim(lease.owner);
    }
}

// Kill Aura's Jump mode uses this same temporary native input path. The
// source module must still be enabled when the input is consumed.
pub(super) fn request_jump(owner: &'static str) {
    state(|s| s.jump_owner = Some(owner));
}

pub(super) fn note_attack(game: &Game<'_>, id: u32) {
    let Some(m) = module(TAP) else {
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let living = game
        .mobs
        .iter()
        .any(|t| t.id == id && t.alive() && !t.removed);
    if policy::should_tap(
        p.sprinting,
        game.keys.contains(&KeyCode::KeyW),
        slot_strength(game, game.selected, 0.5).unwrap_or(0.0),
        living,
    ) && state(|s| s.last_attack != Some(game.ticks))
    {
        state(|s| {
            s.last_attack = Some(game.ticks);
            s.tap_until = game
                .ticks
                .saturating_add(m.n("Hold").clamp(1.0, 3.0) as u64);
        });
    }
}
pub(super) fn note_stab(game: &Game<'_>) {
    if let Some(id) = spear_hits(game).and_then(|v| v.first().map(|&i| game.mobs[i].id)) {
        note_attack(game, id);
    }
}
pub(super) fn note_velocity(game: &Game<'_>, entity: i32, velocity: [f64; 3]) {
    if module(JUMP).is_some()
        && game.remote.as_ref().is_some_and(|r| r.entity_id == entity)
        && policy::jump_knockback(velocity)
    {
        state(|s| s.jump_pending = true);
    }
}

pub(super) fn pre_input(game: &mut Game<'_>, _modules: &[Module], ticks: u64) {
    post_input(game);
    state(|s| s.tick = ticks);
    if !alive(game) || !game.native_playing() || gui_open() {
        state(|s| {
            s.tap_until = 0;
            s.jump_pending = false;
            s.jump_owner = None;
        });
        return;
    }
    let tap = module(TAP).filter(|_| state(|s| game.ticks < s.tap_until));
    let assist_jump = state(|s| s.jump_owner.take()).is_some_and(|owner| module(owner).is_some() && control::available(owner));
    let jump = (state(|s| std::mem::take(&mut s.jump_pending))
        && module(JUMP).is_some()
        && game.keys.contains(&KeyCode::KeyW)
        && game.player.as_ref().is_some_and(|p| p.sprinting)
        || assist_jump)
        && game.player.as_ref().is_some_and(|p| {
            p.ground.on_ground
                && !p.flying
                && !p.fall_flying
                && !p.fluids.was_touching_water
                && !p.is_in_lava()
                && p.last_climbable.is_none()
        })
        && game.using.is_none()
        && game.riding.is_none();
    if tap.is_none() && !jump {
        return;
    }
    let before = INPUT_KEYS.map(|key| game.keys.contains(&key));
    let physical = INPUT_KEYS.map(|key| game.physical_keys.contains(&key));
    if let Some(m) = tap {
        game.keys.remove(&KeyCode::KeyW);
        game.keys.remove(&KeyCode::ControlLeft);
        if m.p("Mode") == "S Tap" {
            game.keys.insert(KeyCode::KeyS);
        }
    }
    if jump {
        game.keys.insert(KeyCode::Space);
        note("native knockback jump requested");
    }
    let changed = std::array::from_fn(|i| before[i] != game.keys.contains(&INPUT_KEYS[i]));
    state(|s| {
        s.input = Some(InputSave {
            before,
            physical,
            changed,
        })
    });
}
pub(super) fn post_input(game: &mut Game<'_>) {
    if let Some(saved) = state(|s| s.input.take()) {
        for (i, key) in INPUT_KEYS
            .iter()
            .enumerate()
            .filter(|(i, _)| saved.changed[*i])
        {
            let physical = game.physical_keys.contains(key);
            let restore = physical || (saved.before[i] && !saved.physical[i]);
            if restore {
                game.keys.insert(*key);
            } else {
                game.keys.remove(key);
            }
        }
    }
}

pub(super) fn request_lunge(game: &Game<'_>) {
    if module(SPEAR).is_some() && alive(game) {
        state(|s| s.lunge.start(game.ticks, 10));
        note("lunge request buffered");
    }
}
pub(super) fn key(game: &Game<'_>, key: &str, pressed: bool) -> bool {
    if !pressed || !game.native_playing() || gui_open() {
        return false;
    }
    let Some(m) = module(SPEAR) else {
        return false;
    };
    let wanted = m.t("Lunge key").trim();
    let Some(wanted_key) = super::sunset::key_from_text(wanted) else { return false; };
    if wanted.eq_ignore_ascii_case("None") || wanted.is_empty()
        || super::sunset::key_from_text(key).as_deref() != Some(wanted_key.as_str()) {
        return false;
    }
    request_lunge(game);
    true
}

fn best_spear(game: &Game<'_>, lunge: bool) -> Option<usize> {
    (0..9)
        .filter_map(|slot| {
            let stack = available_item(game, slot)?;
            Kinetic::of(&stack.item)?;
            let level = stack.enchantment_level("minecraft:lunge");
            if lunge && level == 0 {
                return None;
            }
            Some((
                slot,
                level,
                game.res.items().get(&stack.item)?.attack_damage,
            ))
        })
        .max_by(|a, b| {
            (if lunge { a.1.cmp(&b.1) } else { b.1.cmp(&a.1) })
                .then_with(|| (a.0 == game.selected).cmp(&(b.0 == game.selected)))
                .then_with(|| a.2.total_cmp(&b.2))
        })
        .map(|v| v.0)
}

fn poke_slot(game: &Game<'_>, m: &Module) -> Option<usize> {
    if !m.b("Reach poke")
        || game.held().is_some_and(|s| Kinetic::of(&s.item).is_some())
        || game.mob_hit().is_some()
    {
        return None;
    }
    let eye = game.eye_position()?;
    let hits = spear_hits(game)?;
    if hits
        .iter()
        .any(|&i| known_friend(game, game.mobs[i].id, true))
    {
        return None;
    }
    let past = hits.iter().any(|&i| {
        let target = util::mob_target(&game.mobs[i]);
        enemy(game, &game.mobs[i], m.b("Mobs"))
            && policy::box_distance(eye, target.lo, target.hi).is_some_and(|d| d > 3.0 && d <= 4.4)
    });
    if !past {
        return None;
    }
    best_spear(game, false).filter(|&slot| jab_ready(game, slot))
}

// Native remote_start_attack hook. Reach Poke responds to an actual attack
// click; merely enabling Spear Swap never creates an unsolicited jab.
pub(super) fn before_click(game: &mut Game<'_>) -> Option<bool> {
    if !allowed(game) {
        return None;
    }
    if game.held().is_some_and(|s| Kinetic::of(&s.item).is_some()) {
        return block_stab(game).then_some(false);
    }
    if let Some((index, distance)) = game.mob_hit() {
        if game.before_block(distance) {
            let id = game.mobs[index].id;
            if !before_attack(game, id) {
                return Some(false);
            }
        }
        return None;
    }
    let m = module(SPEAR)?;
    let slot = poke_slot(game, &m)?;
    queue_action(game, m, Automatic::Jab { slot, lunge: false }).then_some(false)
}
fn jab_ready(game: &Game<'_>, slot: usize) -> bool {
    available_item(game, slot).is_some()
        && slot_strength(game, slot, 0.0).is_some_and(|scale| scale >= 1.0)
}

#[derive(Clone, Copy)]
enum Automatic {
    Shield(u32),
    Smash(u32),
    Jab { slot: usize, lunge: bool },
}
fn queue_action(game: &mut Game<'_>, m: Module, action: Automatic) -> bool {
    let owner = m.name;
    if state(|s| s.lease.is_some() || s.queued.is_some()) || !control::claim(owner) {
        return false;
    }
    let done = control::finish(owner);
    let Some(connection) = game.remote.as_ref().map(|r| r.session.counters.clone()) else {
        return false;
    };
    let dimension = game.dim;
    let queued = Instant::now();
    let (yaw, pitch) = (game.yaw, game.pitch);
    state(|s| {
        s.queued = Some(owner);
        s.queued_since = Some(queued);
    });
    let accepted = try_turn_then(owner, yaw, pitch, move |game| {
        let done = done;
        state(|s| s.queued = None);
        if !done.current() {
            return;
        }
        let valid = allowed(game)
            && queued.elapsed() <= Duration::from_millis(200)
            && game.dim == dimension
            && game
                .remote
                .as_ref()
                .is_some_and(|r| std::sync::Arc::ptr_eq(&connection, &r.session.counters))
            && module(owner).is_some_and(|now| now == m);
        if !valid {
            note("deferred melee action cancelled");
            return;
        }
        let prepared = match action {
            Automatic::Shield(id) => {
                let current = game.mob_hit().map(|(i, d)| (game.mobs[i].id, d));
                if !current.is_some_and(|(hit, d)| hit == id && game.before_block(d))
                    || !observed_shield(game, id)
                    || guard_attack(game, id)
                {
                    None
                } else {
                    best_axe(game).map(|slot| WeaponPlan {
                        slot,
                        owner,
                        restore: m.b("Switch back"),
                    })
                }
            }
            Automatic::Smash(id) => {
                let hit = game.mob_hit().map(|(i, d)| (game.mobs[i].id, d));
                if !hit.is_some_and(|(hit, d)| hit == id && game.before_block(d))
                    || observed_shield(game, id)
                    || guard_attack(game, id)
                    || !game.player.as_ref().is_some_and(|p| {
                        policy::smash_eligible(
                            p.fall_distance,
                            p.velocity.y,
                            p.fall_flying,
                            m.n("Min fall"),
                        )
                    })
                {
                    None
                } else {
                    game.mobs
                        .iter()
                        .find(|t| t.id == id)
                        .and_then(|target| {
                            if m.b("Auto swap") {
                                best_mace(game, target, &m)
                            } else {
                                available_item(game, game.selected)
                                    .filter(|s| &*s.item == "minecraft:mace")
                                    .map(|_| game.selected)
                            }
                        })
                        .map(|slot| WeaponPlan {
                            slot,
                            owner,
                            restore: m.b("Swap back"),
                        })
                }
            }
            Automatic::Jab { slot, lunge } => {
                let valid = available_item(game, slot).is_some_and(|s| {
                    Kinetic::of(&s.item).is_some()
                        && (!lunge || s.enchantment_level("minecraft:lunge") > 0)
                }) && jab_ready(game, slot)
                    && !block_stab(game)
                    && (lunge || poke_slot(game, &m) == Some(slot));
                valid.then_some(WeaponPlan {
                    slot,
                    owner,
                    restore: m.b("Switch back"),
                })
            }
        };
        let Some(plan) = prepared else {
            note("fresh melee target/material check failed");
            return;
        };
        if !take_slot_inner(game, plan, 2, Some(done)) {
            return;
        }
        match action {
            Automatic::Shield(id) | Automatic::Smash(id) => {
                game.send(S::Attack(play::Attack { id: id as i32 }));
                game.swing_arm();
                game.send(S::Punch(Empty));
                game.reset_attack_strength();
                state(|s| {
                    s.shield_ready = game.ticks.saturating_add(10);
                    if matches!(action, Automatic::Smash(_)) {
                        s.smash_sent = true;
                    }
                });
            }
            Automatic::Jab { slot, .. } => {
                let kinetic = available_item(game, slot).and_then(|s| Kinetic::of(&s.item));
                game.send(S::PlayerAction(play::PlayerAction {
                    action: 8,
                    pos: [0; 3],
                    direction: 0,
                    sequence: 0,
                }));
                game.hand_strength.reset_attack();
                if let Some(kinetic) = kinetic {
                    game.swing_hand(
                        horizon_client::first_person::Hand::Main,
                        kinetic.attack_animation(),
                    );
                    game.play_at_feet(kinetic.attack_sound);
                }
                state(|s| s.lunge.clear());
            }
        }
        state(|s| s.sent = s.sent.saturating_add(1));
        note("native melee action sent; slot lease active");
    });
    if !accepted {
        state(|s| s.queued = None);
    }
    accepted
}

pub(super) fn tick(game: &mut Game<'_>, _modules: &[Module], ticks: u64) {
    state(|s| s.tick = ticks);
    let _ = stop_guarded_use(game);
    if state(|s| s.trigger_pending.is_some()) && (module("Trigger Bot").is_none() || !allowed(game))
    {
        super::sunset::cancel_turns("Trigger Bot");
        state(|s| s.trigger_pending = None);
    }
    if !alive(game) {
        release_lease(game, true);
        state(|s| {
            s.lunge.clear();
            s.jump_pending = false;
            s.tap_until = 0;
        });
        return;
    }
    let lease = state(|s| s.lease);
    let disabled = lease.is_some_and(|l| module(l.owner).is_none());
    release_lease(game, disabled || !game.native_playing() || gui_open());
    if let Some(owner) = state(|s| s.queued) {
        if !allowed(game)
            || module(owner).is_none()
            || state(|s| {
                s.queued_since
                    .is_none_or(|t| t.elapsed() > Duration::from_millis(200))
            })
        {
            super::sunset::cancel_turns(owner);
            state(|s| s.queued = None);
        }
        return;
    }
    if !allowed(game) || game.left || state(|s| s.lease.is_some()) {
        return;
    }
    if game
        .player
        .as_ref()
        .is_some_and(|p| p.ground.on_ground || p.velocity.y >= 0.0)
    {
        state(|s| {
            s.smash_sent = false;
            s.smash_delayed = false;
        });
    }
    if let Some(m) = module(SPEAR) {
        let requested = state(|s| s.lunge.active(game.ticks));
        if requested {
            let can_lunge = game.player.as_ref().is_some_and(|p| {
                !p.fall_flying
                    && !p.fluids.was_touching_water
                    && !p.is_in_lava()
                    && (game.creative || p.health.food.food >= 7)
            }) && game.riding.is_none();
            if can_lunge {
                if let Some(slot) = best_spear(game, true).filter(|&slot| jab_ready(game, slot)) {
                    if !block_stab(game)
                        && queue_action(game, m.clone(), Automatic::Jab { slot, lunge: true })
                    {
                        return;
                    }
                }
            }
        }
    }
    let Some((index, distance)) = game.mob_hit() else {
        return;
    };
    if !game.before_block(distance) {
        return;
    }
    let id = game.mobs[index].id;
    if let Some(m) = module(SHIELD).filter(|m| m.p("Mode") == "Auto") {
        if game.ticks >= state(|s| s.shield_ready)
            && enemy(game, &game.mobs[index], m.b("Mobs"))
            && observed_shield(game, id)
            && melee_item(game, game.held())
            && best_axe(game).is_some()
            && queue_action(game, m, Automatic::Shield(id))
        {
            return;
        }
    }
    if let Some(m) = module(MACE).filter(|m| m.b("Smash assist")) {
        if state(|s| s.smash_sent)
            || !enemy(game, &game.mobs[index], m.b("Mobs"))
            || observed_shield(game, id)
        {
            return;
        }
        let falling = game.player.as_ref().is_some_and(|p| {
            policy::smash_eligible(
                p.fall_distance,
                p.velocity.y,
                p.fall_flying,
                m.n("Min fall"),
            )
        });
        if !falling {
            return;
        }
        if m.p("Smash timing") == "Delayed" && !state(|s| s.smash_delayed) {
            // Delay once only while the next downward eye still reaches this
            // same target; otherwise make the current real falling hit.
            let p = game.player.as_ref().unwrap();
            let mut eye = game.eye_position().unwrap_or_default();
            eye[1] += p.velocity.y;
            let target = util::mob_target(&game.mobs[index]);
            let next = policy::box_distance(eye, target.lo, target.hi).is_some_and(|d| d < 2.65);
            if next {
                state(|s| s.smash_delayed = true);
                note("one-tick smash timing hold");
                return;
            }
        }
        let _ = queue_action(game, m, Automatic::Smash(id));
    }
}
