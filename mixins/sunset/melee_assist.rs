// Original native melee helpers. References describe behavior; all actions use
// Horizon's actual inventory, movement, item components, packets and spear ray.
use super::sunset::{Category, Module, gui_open, hold, try_turn_then, with};
use super::sunset_combat_control as control;
use super::sunset_mace_policy as mace_policy;
use super::sunset_melee_policy::{self as policy, SlotLease};
use super::sunset_trigger_policy as trigger_policy;
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
    any::Any,
    sync::{Arc, Mutex},
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
            .pick("Mode",&["Passive","Auto"]).bool("Switch back",true),
        Module::new(MACE,Category::Combat,"Selects a mace using observed equipment and native enchantment effects. Optional assist attacks a target under your crosshair during a real fall.")
            .bool("Auto swap",true).pick("Pick",&["Damage","Wind Burst"]).bool("Breach swap",false)
            .bool("Swap back",true).bool("Smash assist",false).pick("Smash timing",&["Delayed","Direct"])
            .num("Min fall",3.0,1.5,30.0,0.5).bool("Mobs",false),
        Module::new(SPEAR,Category::Combat,"Buffers a Lunge key and selects a usable enchanted spear. Reach poke optionally jabs an enemy along your view beyond ordinary melee reach.")
            .text("Lunge key","None").bool("Reach poke",false).bool("Switch back",true),
        Module::new(TAP,Category::Combat,"Briefly releases forward input after a charged sprint hit. S Tap presses backward for the chosen number of ticks.")
            .pick("Mode",&["W Tap","S Tap"]).num("Hold",1.0,1.0,3.0,1.0),
        Module::new(JUMP,Category::Combat,"Requests the native grounded jump after an observed upward knockback packet while sprinting forward."),
        Module::new(FRIEND,Category::Combat,"Blocks direct attacks and piercing spear paths that include a known friend."),
    ]
}

pub(super) fn augment_defaults(modules: &mut Vec<Module>) {
    if let Some(m) = modules.iter_mut().find(|m| m.name == "Trigger Bot") {
        *m=Module::new("Trigger Bot",Category::Combat,"Attacks eligible enemies under your actual crosshair after reaction and charge timing. Smart waits only for an actionable critical stance; Only requires a real critical.")
            .num("Reaction",3.,0.,10.,1.).pick("Crits",&["Smart","Off","Only"])
            .bool("Hold attack",false).bool("Players",true).bool("Hostiles",false)
            .bool("Invisible",false).bool("Weapons only",true);
    }
    for m in defaults() {
        if !modules.iter().any(|old| old.name == m.name) {
            modules.push(m);
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
    action_tick: Option<u64>,
    action_scene: Option<(Arc<dyn Any + Send + Sync>, super::Dim, i32)>,
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
    recent_hit: Option<(u32, Instant)>,
    shield_ready: u64,
    mace: mace_policy::Smash,
    broken_shield: Option<(u32, u64)>,
    random: Option<LegacyRandom>,
    sent: u64,
    blocked: u64,
    reason: &'static str,
    trigger: trigger_policy::Trigger,
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
            s.mace.spent()
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
        && !acted(game)
        && game.clicks.miss_time <= 0
        && control::available(owner)
        && super::sunset::CRIT_STAGE.load(std::sync::atomic::Ordering::Relaxed) == 0
}
pub(super) fn note_action(game: &Game<'_>) {
    let Some(r) = game.remote.as_ref() else {
        return;
    };
    state(|s| {
        s.action_tick = Some(game.ticks);
        s.action_scene = Some((
            r.session.counters.clone() as Arc<dyn Any + Send + Sync>,
            game.dim,
            r.entity_id,
        ));
    });
}
pub(super) fn acted(game: &Game<'_>) -> bool {
    state(|s| {
        s.action_tick == Some(game.ticks)
            && s.action_scene.as_ref().is_some_and(|old| {
                old.1 == game.dim
                    && game.remote.as_ref().is_some_and(|r| {
                        r.entity_id == old.2
                            && Arc::as_ptr(&old.0) as *const ()
                                == Arc::as_ptr(&r.session.counters) as *const ()
                    })
            })
    })
}
pub(super) fn drop_sprint(game: &mut Game<'_>) {
    if let Some(player) = game.player.as_mut() {
        let _ = player.set_sprinting(false);
    }
    if super::sunset_combat_wire::sent_sprinting(game)
        && let Some(id) = game.remote.as_ref().map(|remote| remote.entity_id)
    {
        game.send(S::PlayerCommand(play::PlayerCommand {
            id,
            action: 2,
            data: 0,
        }));
    }
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
                .any(|(i, _)| known_friend(game, game.mobs[*i].id, false));
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
        game.send(S::Attack(play::Attack { id: id as i32 }));
        game.swing_arm();
        game.send(S::Punch(Empty));
        game.reset_attack_strength();
    }
    claim_tick(game);
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
    if !alive(game) || !game.native_playing() || gui_open() {
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
        let other = [
            game.armor_stand_hit().map(|h| h.1),
            game.boat_hit().map(|h| h.1),
            game.cart_hit().map(|h| h.1),
            game.painting_hit().map(|h| h.1),
            game.frame_hit().map(|h| h.1),
        ];
        if other.iter().flatten().any(|other| *other <= d) {
            return None;
        }
        game.mobs[i].id
    };
    let target = game
        .mobs
        .iter()
        .find(|t| t.id == id && t.alive() && !t.removed)?;
    if !m.b("Invisible") && target.invisible {
        return None;
    }
    let wanted = if target.kind.id == "minecraft:player" {
        m.b("Players") && util::player_enemy(game, target)
    } else {
        m.b("Hostiles") && super::sunset_aura::hostile_mob(target)
    };
    wanted.then_some(id)
}

pub(super) fn trigger_tick(game: &mut Game<'_>, m: &Module) {
    if !alive(game) || !game.native_playing() || gui_open() {
        super::sunset::cancel_turns("Trigger Bot");
        state(|s| {
            s.trigger_pending = None;
            s.trigger.no_context();
        });
        return;
    }
    let charge = f64::from(slot_strength(game, game.selected, 0.5).unwrap_or(0.));
    let charge_spread = if charge >= 1. && !state(|s| s.trigger.fully_charged_seen()) {
        random_spread(game, 1)
    } else {
        0
    };
    state(|s| s.trigger.observe_charge(game.ticks, charge, charge_spread));
    let id = trigger_target(game, m);
    let spread = if id.is_some_and(|id| state(|s| s.trigger.needs_acquisition(id))) {
        random_spread(game, 2)
    } else {
        0
    };
    state(|s| {
        s.trigger
            .observe_target(game.ticks, id, m.n("Reaction").clamp(0., 10.) as u8, spread)
    });
    let miss = game.mob_hit().is_none() && game.pick().is_none();
    if state(|s| s.trigger.swallow_air_clicks(m.b("Hold attack"), miss)) {
        game.attack_clicks = 0;
    }
    if let Some(queued) = state(|s| s.trigger_pending) {
        if queued.elapsed() > Duration::from_millis(200) || !allowed(game) {
            super::sunset::cancel_turns("Trigger Bot");
            state(|s| s.trigger_pending = None);
        } else {
            let _ = control::claim("Trigger Bot");
        }
        return;
    }
    let Some(id) = id else {
        return;
    };
    let smash = smash_ready(game, id);
    if !state(|s| s.trigger.timing_allows(game.ticks, id, smash))
        || !attack_allowed(game, "Trigger Bot")
        || game.attack_clicks > 0
        || game.clicks.miss_time > 0
        || m.b("Hold attack")
            && !game.left
            && !game
                .physical_buttons
                .contains(&winit::event::MouseButton::Left)
        || m.b("Weapons only") && !game.held().is_some_and(|s| melee_item(game, Some(s)))
        || wasted_on_shield(game, id)
    {
        return;
    }
    let held_spear = game.held().is_some_and(|s| Kinetic::of(&s.item).is_some());
    let plan = if held_spear {
        None
    } else {
        preferred_weapon(game, id)
    };
    let slot = plan.map_or(game.selected, |p| p.slot);
    let Some(scale) = slot_strength(game, slot, 0.5) else {
        return;
    };
    let spear = available_item(game, slot).is_some_and(|s| Kinetic::of(&s.item).is_some());
    if !smash && (!spear && scale < 1. || !spear && !trigger_crit(game, m, id, scale)) {
        return;
    }
    if spear
        && (game.cannot_attack_with_item(game.hand_strength.ticker, 0)
            || !trigger_spear_safe(game, m, id))
    {
        return;
    }
    if !control::claim("Trigger Bot") {
        return;
    }
    let done = control::finish("Trigger Bot");
    let expected = m.clone();
    let original = game.selected;
    let original_item = game.held().cloned();
    let dimension = game.dim;
    let Some(connection) = game.remote.as_ref().map(|r| r.session.counters.clone()) else {
        return;
    };
    let queued = Instant::now();
    let (yaw, pitch) = (game.yaw, game.pitch);
    state(|s| s.trigger_pending = Some(queued));
    let accepted = try_turn_then("Trigger Bot", yaw, pitch, move |game| {
        let _done = done;
        if state(|s| s.trigger_pending) == Some(queued) {
            state(|s| s.trigger_pending = None);
        }
        if !_done.current()
            || queued.elapsed() > Duration::from_millis(200)
            || dimension != game.dim
            || !game
                .remote
                .as_ref()
                .is_some_and(|r| std::sync::Arc::ptr_eq(&connection, &r.session.counters))
            || !module("Trigger Bot").is_some_and(|now| now == expected)
            || !attack_allowed(game, "Trigger Bot")
            || game.selected != original
            || game.held() != original_item.as_ref()
            || trigger_target(game, &expected) != Some(id)
            || wasted_on_shield(game, id)
            || !super::remote::sunset_combat_remote::sprint_synced(game)
        {
            return;
        }
        let now = if held_spear {
            None
        } else {
            preferred_weapon(game, id)
        };
        if now.map_or(game.selected, |p| p.slot) != slot {
            return;
        }
        let Some(strength) = slot_strength(game, slot, 0.5) else {
            return;
        };
        if !smash_ready(game, id)
            && (!spear && strength < 1. || !spear && !trigger_crit(game, &expected, id, strength))
        {
            return;
        }
        if spear
            && (game.cannot_attack_with_item(game.hand_strength.ticker, 0)
                || !trigger_spear_safe(game, &expected, id))
        {
            return;
        }
        if let Some(plan) = now {
            hold(game, plan.slot);
        }
        let sent = send_hit_at(game, id, yaw, pitch);
        if now.is_some_and(|p| p.restore) && game.selected == slot {
            hold(game, original);
        }
        if sent {
            if smash_ready(game, id) {
                note_smash();
            }
            state(|s| {
                s.trigger.accepted();
                s.sent = s.sent.saturating_add(1);
            });
        }
    });
    if !accepted {
        state(|s| s.trigger_pending = None);
    }
}
fn available_item<'a>(game: &'a Game<'_>, slot: usize) -> Option<&'a ItemStack> {
    let stack = game.inventory.get(slot)?.as_ref()?;
    let info = game.res.items().get(&stack.item)?;
    policy::usable(stack.count, stack.damage, info.max_damage).then_some(stack)
}
pub(super) fn melee_item(game: &Game<'_>, stack: Option<&ItemStack>) -> bool {
    let Some(stack) = stack else {
        return false;
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
    if mob.kind.id == "minecraft:player" {
        return util::player_enemy(game, mob);
    }
    mobs && mob.alive()
        && !mob.removed
        && mob.health.is_finite()
        && mob.kind.id != "minecraft:armor_stand"
        && game.riding != Some(mob.id)
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
    let lead = super::remote::sunset_combat_remote::latency(game)
        .map_or(0., |ms| (f64::from(ms) / 50.).round().clamp(0., 4.)) as f32;
    if (mob.extra.using_item as f32) + lead < (component.delay_seconds * 20.0).round() {
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
    if !allowed(game) {
        return None;
    }
    let target = game
        .mobs
        .iter()
        .find(|m| m.id == id && m.alive() && !m.removed)?;
    if let Some(m) = module(SHIELD) {
        if (target.kind.id == "minecraft:player" || m.b("Mobs"))
            && (game.held().is_none() || melee_item(game, game.held()))
            && observed_shield(game, id)
            && !broken_shield(game, id)
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
    if !m.b("Auto swap") || observed_shield(game, id) && !broken_shield(game, id) {
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
pub(super) fn borrow_slot(
    game: &mut Game<'_>,
    owner: &'static str,
    slot: usize,
    restore: bool,
    ticks: u64,
    done: control::Finish,
) -> bool {
    take_slot_inner(
        game,
        WeaponPlan {
            slot,
            owner,
            restore,
        },
        ticks,
        Some(done),
    )
}
pub(super) fn cancel_owner(game: &mut Game<'_>, owner: &str) {
    super::sunset::cancel_turns(owner);
    if state(|s| s.lease.is_some_and(|lease| lease.owner == owner)) {
        release_lease(game, true);
    }
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

pub(super) fn cancel(game: &mut Game<'_>) {
    post_input(game);
    // The retained ticket, not an owner name, authorizes restoration. A stale
    // ticket drops without a slot packet or release of a replacement lease.
    release_lease(game, true);
    for owner in [SHIELD, MACE, SPEAR, "Trigger Bot"] {
        super::sunset::cancel_turns(owner);
    }
    state(|s| {
        let action_tick = s.action_tick;
        let action_scene = s.action_scene.take();
        let sent = s.sent;
        let blocked = s.blocked;
        *s = State {
            action_tick,
            action_scene,
            sent,
            blocked,
            tick: game.ticks,
            aim_before: [game.yaw, game.pitch],
            reason: "cancelled transient melee state",
            ..State::default()
        };
    });
}

// Kill Aura's Jump mode uses this same temporary native input path. The
// source module must still be enabled when the input is consumed.
pub(super) fn request_jump(owner: &'static str) {
    state(|s| s.jump_owner = Some(owner));
}

pub(super) fn note_attack(game: &Game<'_>, id: u32) {
    note_action(game);
    state(|s| s.recent_hit = Some((id, Instant::now())));
    if observed_shield(game, id)
        && game
            .held()
            .and_then(|s| game.res.items().get(&s.item))
            .is_some_and(|i| i.disable_blocking_seconds > 0.)
    {
        state(|s| s.broken_shield = Some((id, game.ticks.saturating_add(20))));
    }
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
pub(super) fn recent_hit() -> Option<u32> {
    state(|s| {
        s.recent_hit
            .filter(|(_, at)| at.elapsed() <= Duration::from_millis(1500))
            .map(|(id, _)| id)
    })
}
pub(super) fn note_stab(game: &Game<'_>) {
    note_action(game);
    let Some(m) = module(TAP) else {
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    if p.sprinting
        && game.keys.contains(&KeyCode::KeyW)
        && spear_hits(game).is_some_and(|hits| !hits.is_empty())
    {
        state(|s| {
            s.last_attack = Some(game.ticks);
            s.tap_until = game.ticks.saturating_add(m.n("Hold").clamp(1., 3.) as u64);
        });
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
    let assist_jump = state(|s| s.jump_owner.take())
        .is_some_and(|owner| module(owner).is_some() && control::available(owner));
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
    let Some(wanted_key) = super::sunset::key_from_text(wanted) else {
        return false;
    };
    if wanted.eq_ignore_ascii_case("None")
        || wanted.is_empty()
        || super::sunset::key_from_text(key).as_deref() != Some(wanted_key.as_str())
    {
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
                .then_with(|| {
                    if lunge {
                        (a.0 == game.selected).cmp(&(b.0 == game.selected))
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .then_with(|| b.0.cmp(&a.0))
        })
        .map(|v| v.0)
}

fn poke_slot(game: &mut Game<'_>, m: &Module) -> Option<usize> {
    if !m.b("Reach poke")
        || game.held().is_some_and(|s| Kinetic::of(&s.item).is_some())
        || game.mob_hit().is_some()
    {
        return None;
    }
    let eye = game.eye_position()?;
    let hits = spear_hits(game)?;
    let look = super::view_vector(game.yaw, game.pitch);
    let start = Vec3::new(eye[0], eye[1], eye[2]);
    let end = start.add_vector(Vec3::new(look[0], look[1], look[2]).scale(4.4));
    let ordinary = game
        .player
        .as_mut()?
        .attribute("minecraft:entity_interaction_range")
        .ok()?;
    if hits
        .iter()
        .any(|&i| known_friend(game, game.mobs[i].id, false))
    {
        return None;
    }
    let past = hits.iter().any(|&i| {
        enemy(game, &game.mobs[i], true)
            && game.mobs[i]
                .bounds()
                .inflate(Vec3::new(0.125, 0.125, 0.125))
                .clip(start, end)
                .is_some_and(|at| start.distance_squared(at).sqrt() > ordinary)
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
        && slot_strength(game, slot, 0.).is_some_and(|s| s >= 1.)
        && game.clicks.miss_time <= 0
        && game.mining.is_none()
}

#[derive(Clone, Copy)]
enum Automatic {
    Shield(u32),
    Smash(u32),
    SmashShield(u32),
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
    let original = game.selected;
    let original_item = game.held().cloned();
    let dimension = game.dim;
    let queued = Instant::now();
    let (yaw, pitch) = (game.yaw, game.pitch);
    state(|s| {
        s.queued = Some(owner);
        s.queued_since = Some(queued);
    });
    let accepted = try_turn_then(owner, yaw, pitch, move |game| {
        let done = done;
        if state(|s| s.queued_since) == Some(queued) {
            state(|s| s.queued = None);
        }
        if !done.current() {
            return;
        }
        let valid = allowed(game)
            && queued.elapsed() <= Duration::from_millis(200)
            && game.dim == dimension
            && game.selected == original
            && game.held() == original_item.as_ref()
            && !acted(game)
            && super::remote::sunset_combat_remote::sprint_synced(game)
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
            Automatic::SmashShield(id) => {
                if let mace_policy::Intent::BreakShield { slot } = smash_intent(game, &m, id) {
                    Some(WeaponPlan {
                        slot,
                        owner,
                        restore: m.b("Swap back"),
                    })
                } else {
                    None
                }
            }
            Automatic::Smash(id) => {
                let hit = game
                    .mob_hit()
                    .is_some_and(|(i, d)| game.mobs[i].id == id && game.before_block(d));
                if !hit
                    || smash_intent(game, &m, id) != mace_policy::Intent::Smash
                    || guard_attack(game, id)
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
        let hold_ticks = if matches!(action, Automatic::Shield(_)) {
            2
        } else {
            1
        };
        if !take_slot_inner(game, plan, hold_ticks, Some(done)) {
            return;
        }
        match action {
            Automatic::Shield(id) | Automatic::Smash(id) | Automatic::SmashShield(id) => {
                game.send(S::Attack(play::Attack { id: id as i32 }));
                game.swing_arm();
                game.send(S::Punch(Empty));
                game.reset_attack_strength();
                state(|s| match action {
                    Automatic::Shield(_) => s.shield_ready = game.ticks.saturating_add(10),
                    Automatic::Smash(_) => {
                        s.mace.accepted(mace_policy::Intent::Smash, id, game.ticks)
                    }
                    Automatic::SmashShield(_) => s.mace.accepted(
                        mace_policy::Intent::BreakShield { slot: plan.slot },
                        id,
                        game.ticks,
                    ),
                    Automatic::Jab { .. } => {}
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
        claim_tick(game);
        state(|s| s.sent = s.sent.saturating_add(1));
        note("native melee action sent; slot lease active");
    });
    if !accepted {
        if state(|s| s.queued_since) == Some(queued) {
            state(|s| s.queued = None);
        }
    }
    accepted
}

pub(super) fn tick(game: &mut Game<'_>, _modules: &[Module], ticks: u64) {
    state(|s| s.tick = ticks);
    let _ = stop_guarded_use(game);
    let grounded = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
    state(|s| s.mace.observe_grounded(grounded));
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
    let disabled = state(|s| s.lease).is_some_and(|l| module(l.owner).is_none());
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
    if !allowed(game)
        || acted(game)
        || game.attack_clicks > 0
        || game.clicks.miss_time > 0
        || state(|s| s.lease.is_some())
    {
        return;
    }
    if let Some(m) = module(SPEAR) {
        if state(|s| s.lunge.active(game.ticks)) {
            let can_lunge = game.player.as_ref().is_some_and(|p| {
                !p.fall_flying
                    && !p.fluids.was_touching_water
                    && (game.creative || p.health.food.food >= 7)
            }) && game.riding.is_none();
            if can_lunge {
                if let Some(slot) = best_spear(game, true).filter(|&i| jab_ready(game, i)) {
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
            && enemy(game, &game.mobs[index], false)
            && observed_shield(game, id)
            && !broken_shield(game, id)
            && (game.held().is_none() || melee_item(game, game.held()))
            && best_axe(game).is_some()
            && queue_action(game, m, Automatic::Shield(id))
        {
            return;
        }
    }
    if let Some(m) = module(MACE).filter(|m| m.b("Smash assist")) {
        match smash_intent(game, &m, id) {
            mace_policy::Intent::BreakShield { .. } => {
                let _ = queue_action(game, m, Automatic::SmashShield(id));
            }
            mace_policy::Intent::Hold(_) => {
                drop_sprint(game);
                game.clicks.miss_time = game.clicks.miss_time.max(1);
            }
            mace_policy::Intent::Smash => {
                let _ = queue_action(game, m, Automatic::Smash(id));
            }
            mace_policy::Intent::Idle => {}
        }
    }
}

fn claim_tick(game: &mut Game<'_>) {
    game.attack_clicks = 0;
    game.clicks.miss_time = game.clicks.miss_time.max(1);
    game.clicks.right_click_delay = game.clicks.right_click_delay.max(2);
    game.use_delay = game.use_delay.max(2);
}

fn broken_shield(game: &Game<'_>, id: u32) -> bool {
    state(|s| {
        s.broken_shield
            .is_some_and(|(target, until)| target == id && game.ticks < until)
            || s.mace.shield_attempted(id, game.ticks)
    })
}

pub(super) fn wasted_on_shield(game: &Game<'_>, id: u32) -> bool {
    if !observed_shield(game, id) || broken_shield(game, id) {
        return false;
    }
    if game
        .held()
        .and_then(|s| game.res.items().get(&s.item))
        .is_some_and(|info| info.disable_blocking_seconds > 0.)
    {
        return false;
    }
    !(module(SHIELD).is_some() && best_axe(game).is_some())
}

pub(super) fn smash_ready(game: &Game<'_>, id: u32) -> bool {
    let Some(m) = module(MACE) else {
        return false;
    };
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    if state(|s| s.mace.spent())
        || !p.fall_distance.is_finite()
        || p.fall_distance <= 1.5
        || p.fall_distance < m.n("Min fall")
        || p.fall_flying
    {
        return false;
    }
    available_item(game, game.selected).is_some_and(|s| &*s.item == "minecraft:mace")
        || m.b("Auto swap")
            && game
                .mobs
                .iter()
                .find(|t| t.id == id)
                .is_some_and(|target| best_mace(game, target, &m).is_some())
}

fn can_hold_smash(game: &mut Game<'_>, id: u32) -> bool {
    let (yaw, pitch) = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    super::sunset_mace_native::continuation(game, id, yaw, pitch, 0.).unwrap_or(false)
}

pub(super) fn holds_smash(game: &mut Game<'_>, id: u32) -> bool {
    let Some(m) = module(MACE) else {
        return false;
    };
    if !smash_ready(game, id) || !can_hold_smash(game, id) {
        return false;
    }
    let charge = slot_strength(game, game.selected, 0.5).unwrap_or(0.);
    let future = slot_strength(game, game.selected, 1.5).unwrap_or(0.);
    m.p("Smash timing") == "Delayed"
        || charge <= 0.9 && future > 0.9
        || game.mobs.iter().any(|t| t.id == id && t.hurt_time > 0)
        || game.player.as_ref().is_some_and(|p| p.sprinting)
}

pub(super) fn note_smash() {
    state(|s| s.mace.accepted(mace_policy::Intent::Smash, 0, 0));
}

fn smash_intent(game: &mut Game<'_>, m: &Module, id: u32) -> mace_policy::Intent {
    let Some(target) = game.mobs.iter().find(|t| t.id == id) else {
        return mace_policy::Intent::Idle;
    };
    let target_allowed = enemy(game, target, m.b("Mobs"));
    let shield_blocks = observed_shield(game, id) && !broken_shield(game, id);
    let immune = target.hurt_time > 0;
    let mace_available = smash_ready(game, id);
    let can_continue = can_hold_smash(game, id);
    let Some(p) = game.player.as_ref() else {
        return mace_policy::Intent::Idle;
    };
    let frame = mace_policy::Frame {
        tick: game.ticks,
        target: id,
        grounded: p.ground.on_ground,
        descending: p.velocity.y < 0.,
        native_smash: p.fall_distance > 1.5 && !p.fall_flying,
        fall: p.fall_distance,
        min_fall: m.n("Min fall"),
        target_allowed,
        mace_available,
        shield_blocks,
        axe: best_axe(game),
        auto_swap: m.b("Auto swap"),
        swap_back: m.b("Swap back"),
        can_continue,
        timing: if m.p("Smash timing") == "Delayed" {
            mace_policy::Timing::Delayed
        } else {
            mace_policy::Timing::Direct
        },
        charge: f64::from(slot_strength(game, game.selected, 0.5).unwrap_or(0.)),
        charge_next: f64::from(slot_strength(game, game.selected, 1.5).unwrap_or(0.)),
        target_immune: immune,
        sprinting: p.sprinting,
    };
    state(|s| s.mace.plan(frame))
}

fn trigger_crit(game: &mut Game<'_>, m: &Module, id: u32, scale: f32) -> bool {
    let Some(p) = game.player.as_ref() else {
        return false;
    };
    let blind = game.potions.effects.get("blindness").is_some();
    let stance = !p.ground.on_ground
        && p.last_climbable.is_none()
        && !p.fluids.was_touching_water
        && !blind
        && game.riding.is_none()
        && !p.sprinting;
    let f = trigger_policy::CritFrame {
        mode: match m.p("Crits") {
            "Only" => trigger_policy::Crits::Only,
            "Off" => trigger_policy::Crits::Off,
            _ => trigger_policy::Crits::Smart,
        },
        tick: game.ticks,
        would_crit: scale > 0.9 && p.fall_distance > 0. && stance,
        crit_stance: stance,
        grounded_jumping_without_sprint: p.ground.on_ground
            && game.keys.contains(&KeyCode::Space)
            && !p.sprinting,
        airborne: !p.ground.on_ground
            && p.last_climbable.is_none()
            && !p.fluids.was_touching_water
            && !blind
            && game.riding.is_none()
            && !p.flying
            && !p.fall_flying,
        sprinting: p.sprinting,
        fall_positive: p.fall_distance > 0.,
    };
    let decision = state(|s| s.trigger.critical(f));
    if decision.drop_sprint {
        drop_sprint(game);
    }
    let _ = id;
    decision.allow
}

fn trigger_spear_safe(game: &Game<'_>, m: &Module, id: u32) -> bool {
    let Some(hits) = spear_hits(game) else {
        return false;
    };
    hits.iter().any(|&i| game.mobs[i].id == id)
        && hits.iter().all(|&i| {
            let target = &game.mobs[i];
            if target.kind.id == "minecraft:player" {
                m.b("Players") && util::player_enemy(game, target)
            } else {
                m.b("Hostiles") && super::sunset_aura::hostile_mob(target)
            }
        })
}

fn random_spread(game: &Game<'_>, max: u8) -> u8 {
    use horizon_core::random::RandomSource;
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
            .next_int_bound(i32::from(max) + 1)
            .unwrap_or(0) as u8
    })
}
