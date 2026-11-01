// The melee helpers from Rust's PR #16: Shield Breaker, Mace, Spear Swap, W Tap, Jump Reset and Friend Guard, and
// the weapon they pick for Kill Aura's and Trigger Bot's hits. Hits use the game's own inventory, movement, item
// components, packets and spear ray.
use super::sunset::{Category, Module, gui_open, hold, try_turn_then, with};
use super::sunset_combat_control as control;
use super::sunset_mace_policy as mace_policy;
use super::sunset_melee_policy::{self as policy, SlotLease};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::enchant::EffectContext;
use horizon_client::items::ItemStack;
use horizon_client::mobs::Mob;
use horizon_client::spear::{Kinetic, REACH, hits_in_reach};
use horizon_core::geometry::Vec3;
use horizon_core::random::LegacyRandom;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use horizon_world::combat_rules::{self, ArmorWorld};
use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

const SHIELD: &str = "Shield Breaker";
const MACE: &str = "Mace";
const SPEAR: &str = "Spear Swap";
const TAP: &str = "W Tap";
const JUMP: &str = "Jump Reset";
const FRIEND: &str = "Friend Guard";
// The keys W Tap and Jump Reset press or let go for a tick.
const INPUT_KEYS: [KeyCode; 4] = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::ControlLeft, KeyCode::Space];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(SHIELD, Category::Combat, "Hits a player's raised shield with an axe from your hotbar, which disables it. Passive: only when you or Kill Aura hit them; Auto also hits the shield under your crosshair. Shield held: the axe as soon as they hold a shield, not only once it is up (good players raise it as your hit arrives). Stun slam: falling onto the shield, Kill Aura's axe hit is followed by Mace's mace in the same tick. Holding (Duelkit's): what you must hold for it to swap (sword, axe, spear, mace, trident, empty, other). Back after A delay keeps the axe in hand a random Shortest to Longest delay ticks before your slot comes back.")
            .pick("Mode", &["Passive", "Auto"])
            .text("Holding", "sword,axe,spear,mace,trident,empty")
            .bool("Switch back", true)
            .pick("Back after", &["Same tick", "A delay"])
            .num("Shortest delay", 2.0, 1.0, 20.0, 1.0)
            .num("Longest delay", 4.0, 1.0, 20.0, 1.0)
            .bool("Shield held", false)
            .bool("Stun slam", true),
        Module::new(MACE, Category::Combat, "Takes the mace that hits hardest (its enchantments and the target's armour counted) as you fall onto a target, or for Breach. Smash assist hits the target under your crosshair during a real fall. Holding (Duelkit's): what you must hold for it to swap (sword, axe, spear, mace, trident, empty, other). Back after A delay keeps the mace in hand a random Shortest to Longest delay ticks before your slot comes back.")
            .bool("Auto swap", true)
            .pick("Pick", &["Damage", "Wind Burst"])
            .bool("Breach swap", false)
            .text("Holding", "sword,axe,spear,mace,trident,empty,other")
            .bool("Swap back", true)
            .pick("Back after", &["Same tick", "A delay"])
            .num("Shortest delay", 2.0, 1.0, 20.0, 1.0)
            .num("Longest delay", 4.0, 1.0, 20.0, 1.0)
            .bool("Smash assist", false)
            .pick("Smash timing", &["Delayed", "Direct"])
            .num("Min fall", 3.0, 1.5, 30.0, 0.5)
            .bool("Mobs", false),
        Module::new(SPEAR, Category::Combat, "The Lunge key lunges with the spear from your hotbar with the most Lunge. Reach poke jabs an enemy along your look past ordinary reach when you click. Spear reach (Duelkit's): holding what Holding lists (a sword or axe), the crosshair reaches as far as the spear in your hotbar, and the spear is selected and put back right before the hit.")
            .text("Lunge key", "None")
            .bool("Reach poke", false)
            .pick("Spear reach", &["Off", "When out of reach", "Every hit"])
            .text("Holding", "sword,axe")
            .bool("Switch back", true),
        Module::new(TAP, Category::Combat, "Lets go of forward for a tick or more after a charged sprint hit, so the next hit knocks back as a sprint hit again. S Tap presses back instead.")
            .pick("Mode", &["W Tap", "S Tap"])
            .num("Hold", 1.0, 1.0, 3.0, 1.0),
        Module::new(JUMP, Category::Combat, "Jumps as knockback hits you while you sprint forward, which takes some of it off."),
        Module::new(FRIEND, Category::Combat, "Stops your hits (and spear stabs passing through them) on friends.")
            .bool("Notifications", true),
    ]
}

#[derive(Clone, Copy)]
struct InputSave {
    before: [bool; 4],
    physical: [bool; 4],
    changed: [bool; 4],
}

#[derive(Default)]
struct State {
    // The tick a hit or use went out, and in which world: one action a tick.
    action_tick: Option<u64>,
    action_scene: Option<(Arc<dyn Any + Send + Sync>, super::Dim, i32)>,
    lease: Option<SlotLease>,
    lease_end: Option<control::Finish>,
    queued: Option<&'static str>,
    queued_since: Option<Instant>,
    lunge: policy::Request,
    tap_until: u64,
    jump_pending: bool,
    input: Option<InputSave>,
    last_attack: Option<u64>,
    shield_ready: u64,
    mace: mace_policy::Smash,
    broken_shield: Option<(u32, u64)>,
    friend_hint_at: Option<Instant>,
    friend_hint: Option<u32>,
    jump_owner: Option<&'static str>,
    // A hit whose look the server has: it goes at the start of the next tick, before that tick's move packet, as the
    // game's own attack does.
    due: Option<Box<dyn for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(s.get_or_insert_with(State::default))
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

fn alive(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(super::sunset::ID)
        && game.remote.is_some()
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && !game.spectator()
        && game.selected < 9
}

fn allowed(game: &Game<'_>) -> bool {
    alive(game)
        && game.native_playing()
        && game.screen == Screen::Playing
        && !gui_open()
        && (super::sunset_hands::shielding(game) || (game.using.is_none() && !game.right))
        && game.mining.is_none()
        && util::inventory_idle(game)
        && !with(|s| s.is_on("Blink"))
        && game.mobs.len() <= policy::POPULATION_LIMIT
}

pub(super) fn note_action(game: &Game<'_>) {
    let Some(r) = game.remote.as_ref() else {
        return;
    };
    state(|s| {
        s.action_tick = Some(game.ticks);
        s.action_scene = Some((r.session.counters.clone() as Arc<dyn Any + Send + Sync>, game.dim, r.entity_id));
    });
}

fn acted(game: &Game<'_>) -> bool {
    state(|s| {
        s.action_tick == Some(game.ticks)
            && s.action_scene.as_ref().is_some_and(|old| {
                old.1 == game.dim
                    && game.remote.as_ref().is_some_and(|r| {
                        r.entity_id == old.2 && Arc::as_ptr(&old.0) as *const () == Arc::as_ptr(&r.session.counters) as *const ()
                    })
            })
    })
}

// Stops sprinting, and tells the server if it had you sprinting.
pub(super) fn drop_sprint(game: &mut Game<'_>) {
    if let Some(player) = game.player.as_mut() {
        let _ = player.set_sprinting(false);
    }
    if super::sunset_combat_wire::sent_sprinting(game)
        && let Some(id) = game.remote.as_ref().map(|remote| remote.entity_id)
    {
        game.send(S::PlayerCommand(play::PlayerCommand { id, action: 2, data: 0 }));
    }
}

// The attack cooldown (0 to 1) a hit with the item in hotbar `slot` would have, `partial` ticks on.
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
    let modifiers =
        game.potions.effects.modifiers().into_iter().filter(|m| m.0 == "minecraft:attack_speed").map(|m| m.2);
    let speed = policy::attack_speed(modifier, modifiers)?;
    (speed > 0.0).then(|| game.hand_strength.scale(partial, speed))
}

// A spear's stab: the game's PlayerAction 8, the swing and the sound.
fn stab(game: &mut Game<'_>, kinetic: Kinetic) {
    game.send(S::PlayerAction(play::PlayerAction { action: 8, pos: [0; 3], direction: 0, sequence: 0 }));
    game.hand_strength.reset_attack();
    game.swing_hand(horizon_client::first_person::Hand::Main, kinetic.attack_animation());
    game.play_at_feet(kinetic.attack_sound);
}

fn available_item<'a>(game: &'a Game<'_>, slot: usize) -> Option<&'a ItemStack> {
    let stack = game.inventory.get(slot)?.as_ref()?;
    let info = game.res.items().get(&stack.item)?;
    policy::usable(stack.count, stack.damage, info.max_damage).then_some(stack)
}

fn known_friend(game: &Game<'_>, id: u32) -> bool {
    game.mobs.iter().any(|m| m.id == id && m.kind.id == "minecraft:player")
        && game.remote_player_name(id).is_some_and(|name| with(|s| s.friends.iter().any(|f| f.eq_ignore_ascii_case(name))))
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

pub(super) fn reset_friend_feedback() {
    state(|s| {
        s.friend_hint = None;
        s.friend_hint_at = None;
    });
}

// Friend Guard's action bar line, at most every 1.5 s. The packet checks only read the game, so the line waits for
// the next tick.
fn queue_friend_hint(game: &Game<'_>, id: u32) {
    if !module(FRIEND).is_some_and(|m| m.b("Notifications")) || game.remote_player_name(id).is_none() {
        return;
    }
    let now = Instant::now();
    state(|s| {
        if s.friend_hint_at.is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_millis(1500)) {
            s.friend_hint_at = Some(now);
            s.friend_hint = Some(id);
        }
    });
}

fn flush_friend_hint(game: &mut Game<'_>) {
    if !module(FRIEND).is_some_and(|m| m.b("Notifications")) || !alive(game) {
        reset_friend_feedback();
        return;
    }
    let Some(id) = state(|s| s.friend_hint.take()) else {
        return;
    };
    if !known_friend(game, id) {
        return;
    }
    if let Some(name) = game.remote_player_name(id) {
        let message = horizon_client::text::Text::literal(format!("Held back your hit on {name}"));
        game.titles.set_overlay(message, false);
    }
}

// Friend Guard: true stops a hit on `id` (every Attack packet comes through here, your clicks too).
pub(super) fn guard_attack(game: &Game<'_>, id: u32) -> bool {
    let blocked = module(FRIEND).is_some() && alive(game) && known_friend(game, id);
    if blocked {
        queue_friend_hint(game, id);
    }
    blocked
}

// What a spear stab along (yaw, pitch) would pass through: indices into game.mobs.
fn spear_hits_at(game: &Game<'_>, yaw: f32, pitch: f32) -> Option<Vec<usize>> {
    if game.mobs.len() > policy::POPULATION_LIMIT || !yaw.is_finite() || !pitch.is_finite() {
        return None;
    }
    let eye = game.eye_position()?;
    if !eye.iter().all(|v| v.is_finite()) {
        return None;
    }
    if game.mobs.iter().any(|m| {
        let b = m.bounds();
        let (a, z) = (b.min_position(), b.max_position());
        policy::box_distance(eye, [a.x, a.y, a.z], [z.x, z.y, z.z]).is_none()
    }) {
        return None;
    }
    let look = super::view_vector(yaw, pitch);
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
            Vec3::new(eye[0], eye[1], eye[2]),
            Vec3::new(look[0], look[1], look[2]),
            game.known_player_movement(),
            reach,
            |from, to| super::spear::sunset_combat_collision::clip(game, from, to),
        )
        .into_iter()
        .map(|(i, _)| i)
        .collect(),
    )
}

// Friend Guard for a spear: a stab passes through several, so a friend behind the first counts too. True stops it.
pub(super) fn block_stab(game: &Game<'_>) -> bool {
    guard_stab_at(game, game.yaw, game.pitch)
}

pub(super) fn guard_stab_at(game: &Game<'_>, yaw: f32, pitch: f32) -> bool {
    if module(FRIEND).is_none() || !alive(game) {
        return false;
    }
    let hits = spear_hits_at(game, yaw, pitch);
    let friend = hits.as_ref().and_then(|hits| hits.iter().map(|&i| game.mobs[i].id).find(|&id| known_friend(game, id)));
    if let Some(id) = friend {
        queue_friend_hint(game, id);
    }
    hits.is_none() || friend.is_some()
}

// A held spear charge that would run through a friend is let go (the game's release-use packet).
fn stop_guarded_use(game: &mut Game<'_>) {
    let spear = game.using.and_then(|(slot, _)| game.stack_in(slot)).is_some_and(|s| Kinetic::of(&s.item).is_some());
    if spear && block_stab(game) {
        game.send(S::PlayerAction(play::PlayerAction { action: 6, pos: [0; 3], direction: 0, sequence: 0 }));
        game.using = None;
    }
}

// A raised shield `id` holds up toward you: its blocks_attacks component, raised long enough (its delay, less your
// ping) and facing you within its angle.
fn observed_shield(game: &Game<'_>, id: u32) -> bool {
    let Some(mob) = game.mobs.iter().find(|m| m.id == id && m.alive() && !m.removed) else {
        return false;
    };
    if !mob.extra.blocking || mob.extra.using_item <= 0 {
        return false;
    }
    let used = if mob.extra.using_off_hand { mob.extra.off_hand.as_ref() } else { mob.extra.main_hand.as_ref() };
    let Some(component) = used.and_then(|item| game.res.items().get(item)).and_then(|i| i.blocks_attacks.as_ref()) else {
        return false;
    };
    let lead = super::remote::sunset_combat_remote::latency(game).map_or(0.0, |ms| (f64::from(ms) / 50.0).round().clamp(0.0, 4.0)) as f32;
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

// A shield in either of `id`'s hands, raised or not.
fn holds_shield(game: &Game<'_>, id: u32) -> bool {
    game.mobs.iter().find(|m| m.id == id).is_some_and(|mob| {
        [mob.extra.main_hand.as_ref(), mob.extra.off_hand.as_ref()]
            .into_iter()
            .flatten()
            .any(|item| game.res.items().get(item).is_some_and(|i| i.blocks_attacks.is_some()))
    })
}

// A shield Shield Breaker takes the axe to: raised at you, or with Shield held only held.
fn shield_counts(game: &Game<'_>, id: u32) -> bool {
    observed_shield(game, id) || (module(SHIELD).is_some_and(|m| m.b("Shield held")) && holds_shield(game, id))
}

// The hardest-hitting hotbar item that disables shields (an axe).
fn best_axe(game: &Game<'_>) -> Option<usize> {
    (0..9)
        .filter_map(|slot| {
            let info = game.res.items().get(&available_item(game, slot)?.item)?;
            (info.disable_blocking_seconds > 0.0).then_some((slot, info.attack_damage))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(slot, _)| slot)
}

// The target's armour as the server would weigh it against a mace smash (Breach lowers it).
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

// The damage a hit with `stack` would do to `target` after armour, a smash's fall bonus included. Its own random
// stream: weighing the hotbar changes nothing in the game.
fn estimate_weapon(game: &Game<'_>, stack: &ItemStack, target: &Mob) -> Option<f32> {
    let info = game.res.items().get(&stack.item)?;
    let p = game.player.as_ref()?;
    let smashing = &*stack.item == "minecraft:mace" && p.fall_distance > 1.5 && !p.fall_flying;
    let ctx = EffectContext {
        this: Some(&target.kind.id),
        attacker: Some("minecraft:player"),
        direct_attacker: Some("minecraft:player"),
        is_direct: true,
        damage_type: Some(if smashing { "minecraft:mace_smash" } else { "minecraft:player_attack" }),
        items: Some(game.res.items()),
        ..Default::default()
    };
    let enchantments = &game.res.items().enchantments;
    let mut random = LegacyRandom::new(0);
    let mut damage = enchantments.modify_value(
        Some(stack),
        "minecraft:damage",
        &ctx,
        &mut random,
        1.0 + info.attack_damage + game.effect_attack_bonus(),
    );
    if smashing {
        let per = enchantments.modify_value(Some(stack), "minecraft:smash_damage_per_fallen_block", &ctx, &mut random, 0.0);
        damage += (policy::smash_bonus(p.fall_distance)? + f64::from(per) * p.fall_distance) as f32;
    }
    let (armor, tough) = target.worn_armor();
    let mut estimate = ArmorEstimate { game, stack, victim: &target.kind.id, random };
    let damage =
        combat_rules::after_armor(&mut estimate, damage, target.kind.armor + armor, target.kind.armor_toughness + tough).ok()?;
    (damage.is_finite() && damage >= 0.0).then_some(damage)
}

// The mace that does most to `target` (Wind Burst first with Pick on it); outside a smash only one with Breach.
fn best_mace(game: &Game<'_>, target: &Mob, m: &Module) -> Option<usize> {
    let smash = game.player.as_ref().is_some_and(|p| p.fall_distance > 1.5 && !p.fall_flying);
    let wind = smash && m.p("Pick") == "Wind Burst";
    (0..9)
        .filter_map(|slot| {
            let stack = available_item(game, slot)?;
            if &*stack.item != "minecraft:mace" || (!smash && stack.enchantment_level("minecraft:breach") == 0) {
                return None;
            }
            Some((slot, wind && stack.enchantment_level("minecraft:wind_burst") > 0, estimate_weapon(game, stack, target)?))
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.total_cmp(&b.2)))
        .map(|(slot, _, _)| slot)
}

// A slot to hit `id` with, and whether to go back after.
#[derive(Clone, Copy)]
struct WeaponPlan {
    pub slot: usize,
    pub owner: &'static str,
    pub restore: bool,
    // Ticks more before the slot goes back (Back after A delay).
    pub delay: u64,
}

// What the hand holds, as Duelkit's Holding lists name it.
fn held_kind(game: &Game<'_>) -> &'static str {
    let Some(stack) = game.held() else {
        return "empty";
    };
    let tagged = |tag: &str| game.res.items().item_tags.get(tag).is_some_and(|items| items.contains(&*stack.item));
    match &*stack.item {
        "minecraft:mace" => "mace",
        "minecraft:trident" => "trident",
        name if Kinetic::of(name).is_some() => "spear",
        _ if tagged("minecraft:swords") => "sword",
        _ if tagged("minecraft:axes") => "axe",
        _ => "other",
    }
}

// The swap modules start only from what their Holding lists.
fn holding(game: &Game<'_>, m: &Module) -> bool {
    util::listed(m.t("Holding"), held_kind(game))
}

// Duelkit's After a delay: the swapped weapon stays in hand a few ticks, from Shortest to Longest delay (picked by the
// tick it starts, so it varies), before the slot goes back; 0 with Same tick.
pub(super) fn back_delay(game: &Game<'_>, m: &Module) -> u64 {
    if m.p("Back after") != "A delay" {
        return 0;
    }
    let lo = m.n("Shortest delay") as u64;
    let hi = (m.n("Longest delay") as u64).max(lo);
    lo + game.ticks.wrapping_mul(0x9E37_79B9) % (hi - lo + 1)
}

// The weapon Shield Breaker (an axe at a raised shield) or Mace (a smash, or Breach doing 15% more) wants for a hit
// on `id`. It only chooses: whoever hits switches and goes back.
fn preferred_weapon(game: &Game<'_>, id: u32) -> Option<WeaponPlan> {
    if !allowed(game) {
        return None;
    }
    let target = game.mobs.iter().find(|m| m.id == id && m.alive() && !m.removed)?;
    let shield_up = shield_counts(game, id) && !broken_shield(game, id);
    if let Some(m) = module(SHIELD)
        && target.kind.id == "minecraft:player"
        && holding(game, &m)
        && shield_up
        && !game.held().and_then(|s| game.res.items().get(&s.item)).is_some_and(|i| i.disable_blocking_seconds > 0.0)
    {
        return best_axe(game).map(|slot| WeaponPlan { slot, owner: SHIELD, restore: m.b("Switch back"), delay: back_delay(game, &m) });
    }
    let m = module(MACE).filter(|m| holding(game, m))?;
    let smash = game.player.as_ref().is_some_and(|p| p.fall_distance > 1.5 && !p.fall_flying);
    if !m.b("Auto swap") || shield_up || (!smash && !m.b("Breach swap")) {
        return None;
    }
    let slot = best_mace(game, target, &m)?;
    if !smash {
        let old = game.held().and_then(|s| estimate_weapon(game, s, target)).unwrap_or(1.0);
        if estimate_weapon(game, available_item(game, slot)?, target)? < old * 1.15 {
            return None;
        }
    }
    Some(WeaponPlan { slot, owner: MACE, restore: m.b("Swap back"), delay: back_delay(game, &m) })
}

// `owner` turned off: its look waiting and its borrowed slot go.
pub(super) fn cancel_owner(game: &mut Game<'_>, owner: &str) {
    state(|s| {
        if s.jump_owner == Some(owner) {
            s.jump_owner = None;
        }
    });
    super::sunset::cancel_turns(owner);
    if state(|s| s.lease.is_some_and(|lease| lease.owner == owner)) {
        release_lease(game, true);
    }
}

fn take_slot(game: &mut Game<'_>, plan: WeaponPlan, hold_ticks: u64, queued_end: Option<control::Finish>) -> bool {
    if state(|s| s.lease.is_some() && !s.lease_end.as_ref().is_some_and(|end| end.current())) {
        state(|s| {
            s.lease = None;
            s.lease_end = None;
        });
    }
    let old = state(|s| s.lease);
    if old.is_some_and(|l| l.owner != plan.owner) || available_item(game, plan.slot).is_none() || !control::claim(plan.owner) {
        return false;
    }
    let end = if old.is_none() { queued_end.or_else(|| Some(control::finish(plan.owner))) } else { None };
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
            until: game.ticks.saturating_add(hold_ticks + plan.delay),
            return_enabled: plan.restore,
        })
    });
    true
}

// Your own click on `id`: false stops it (a friend); Shield Breaker or Mace switch the weapon for it.
fn before_attack(game: &mut Game<'_>, id: u32) -> bool {
    if guard_attack(game, id) {
        return false;
    }
    if let Some(plan) = preferred_weapon(game, id) {
        take_slot(game, plan, 2, None);
    }
    true
}

fn release_lease(game: &mut Game<'_>, force: bool) {
    let Some(lease) = state(|s| s.lease) else {
        return;
    };
    let owned = state(|s| s.lease_end.as_ref().is_some_and(|end| end.current()));
    if force || lease.expired(game.selected, game.ticks, owned) {
        if let Some(slot) = lease.restore(game.selected, if force { u64::MAX } else { game.ticks }, owned) {
            hold(game, slot);
        }
        state(|s| {
            s.lease = None;
            s.lease_end = None;
        });
    } else {
        control::claim(lease.owner);
    }
}

// Sunset off, a new world or life: the borrowed slot goes back, keys come back, the rest is dropped.
pub(super) fn cancel(game: &mut Game<'_>) {
    post_input(game);
    release_lease(game, true);
    for owner in [SHIELD, MACE, SPEAR] {
        super::sunset::cancel_turns(owner);
    }
    state(|s| {
        let (action_tick, action_scene) = (s.action_tick, s.action_scene.take());
        *s = State { action_tick, action_scene, ..State::default() };
    });
}

// Key Throw's wind charge jump presses the jump key for the next tick, while `owner` is on.
pub(super) fn request_jump(owner: &'static str) {
    state(|s| s.jump_owner = Some(owner));
}

// An Attack packet went out: Shield Breaker's hit counts for a second, W Tap starts.
fn note_attack(game: &Game<'_>, id: u32) {
    note_action(game);
    if observed_shield(game, id)
        && game.held().and_then(|s| game.res.items().get(&s.item)).is_some_and(|i| i.disable_blocking_seconds > 0.0)
    {
        state(|s| s.broken_shield = Some((id, game.ticks.saturating_add(20))));
    }
    let Some(m) = module(TAP) else {
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let living = game.mobs.iter().any(|t| t.id == id && t.alive() && !t.removed);
    if policy::should_tap(
        p.sprinting,
        game.keys.contains(&KeyCode::KeyW),
        slot_strength(game, game.selected, 0.5).unwrap_or(0.0),
        living,
    ) {
        start_tap(game, &m);
    }
}

fn start_tap(game: &Game<'_>, m: &Module) {
    state(|s| {
        if s.last_attack != Some(game.ticks) {
            s.last_attack = Some(game.ticks);
            s.tap_until = game.ticks.saturating_add(m.n("Hold").clamp(1.0, 3.0) as u64);
        }
    });
}

// A spear stab went out along (yaw, pitch): W Tap starts if it went through someone.
fn note_stab_at(game: &Game<'_>, yaw: f32, pitch: f32) {
    note_action(game);
    if let Some(m) = module(TAP)
        && super::sunset_combat_wire::sent_sprinting(game)
        && game.keys.contains(&KeyCode::KeyW)
        && spear_hits_at(game, yaw, pitch).is_some_and(|hits| !hits.is_empty())
    {
        start_tap(game, &m);
    }
}

// What went on the wire: hits and uses count as the tick's action.
pub(super) fn forwarded(game: &Game<'_>, packet: &S) {
    match packet {
        S::UseItem(_) | S::UseItemOn(_) => note_action(game),
        S::Attack(p) if p.id >= 0 => note_attack(game, p.id as u32),
        S::PlayerAction(p) if p.action == 8 => match super::sunset_combat_wire::sent_look(game) {
            Some((yaw, pitch)) => note_stab_at(game, yaw, pitch),
            None => note_action(game),
        },
        _ => {}
    }
}

// Friend Guard on what is about to go out: false drops it (a hit on a friend, a stab or spear use through one).
pub(super) fn allowed_out(game: &Game<'_>, packet: &S) -> bool {
    let look = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    match packet {
        S::Attack(p) => p.id < 0 || !guard_attack(game, p.id as u32),
        S::PlayerAction(p) if p.action == 8 => !guard_stab_at(game, look.0, look.1),
        S::UseItem(p) => {
            let slot = if p.hand == 1 { util::OFFHAND } else { game.selected };
            !game.stack_in(slot).is_some_and(|s| Kinetic::of(&s.item).is_some() && guard_stab_at(game, p.yaw, p.pitch))
        }
        _ => true,
    }
}

// The server's knockback on you (a velocity packet): Jump Reset jumps next tick.
pub(super) fn note_velocity(game: &Game<'_>, entity: i32, velocity: [f64; 3]) {
    if module(JUMP).is_some()
        && game.remote.as_ref().is_some_and(|r| r.entity_id == entity)
        && policy::jump_knockback(velocity)
    {
        state(|s| s.jump_pending = true);
    }
}

// Before the tick's movement: W Tap lets go of forward (S Tap presses back), Jump Reset and a requested jump press
// jump. post_input puts the keys back.
pub(super) fn pre_input(game: &mut Game<'_>) {
    post_input(game);
    if !alive(game) || !game.native_playing() || gui_open() {
        state(|s| {
            s.tap_until = 0;
            s.jump_pending = false;
            s.jump_owner = None;
        });
        return;
    }
    let tap = module(TAP).filter(|_| state(|s| game.ticks < s.tap_until));
    let asked = state(|s| s.jump_owner.take()).is_some_and(|owner| module(owner).is_some() && control::available(owner));
    let knocked = state(|s| std::mem::take(&mut s.jump_pending))
        && module(JUMP).is_some()
        && game.keys.contains(&KeyCode::KeyW)
        && game.player.as_ref().is_some_and(|p| p.sprinting);
    let jump = (knocked || asked)
        && game.player.as_ref().is_some_and(|p| {
            p.ground.on_ground && !p.flying && !p.fall_flying && !p.fluids.was_touching_water && !p.is_in_lava()
        })
        && super::remote::sunset_prediction_remote::local_motion(game).is_some_and(|motion| !motion.floating)
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
    }
    let changed = std::array::from_fn(|i| before[i] != game.keys.contains(&INPUT_KEYS[i]));
    state(|s| s.input = Some(InputSave { before, physical, changed }));
}

// The keys pre_input changed go back to what you hold.
pub(super) fn post_input(game: &mut Game<'_>) {
    let Some(saved) = state(|s| s.input.take()) else {
        return;
    };
    for (i, key) in INPUT_KEYS.iter().enumerate().filter(|(i, _)| saved.changed[*i]) {
        if game.physical_keys.contains(key) || (saved.before[i] && !saved.physical[i]) {
            game.keys.insert(*key);
        } else {
            game.keys.remove(key);
        }
    }
}

// Spear Swap's Lunge key, pressed: a lunge waits up to 10 ticks for its moment.
pub(super) fn key(game: &Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_playing() || gui_open() || !alive(game) {
        return;
    }
    let Some(m) = module(SPEAR) else {
        return;
    };
    if super::sunset::key_from_text(m.t("Lunge key")).is_some_and(|wanted| super::sunset::key_from_text(key) == Some(wanted)) {
        state(|s| s.lunge.start(game.ticks, 10));
    }
}

// The hotbar spear for a lunge (the most Lunge, the one in hand if level) or a jab (the least).
fn best_spear(game: &Game<'_>, lunge: bool) -> Option<usize> {
    (0..9)
        .filter_map(|slot| {
            let stack = available_item(game, slot)?;
            Kinetic::of(&stack.item)?;
            let level = stack.enchantment_level("minecraft:lunge");
            (!lunge || level > 0).then_some((slot, level))
        })
        .max_by(|a, b| {
            if lunge {
                a.1.cmp(&b.1).then_with(|| (a.0 == game.selected).cmp(&(b.0 == game.selected)))
            } else {
                b.1.cmp(&a.1)
            }
            .then_with(|| b.0.cmp(&a.0))
        })
        .map(|v| v.0)
}

// Reach poke: your click reaches nobody, but a spear's stab along your look would reach an enemy past ordinary
// reach (and no friend): the spear to jab with.
fn poke_slot(game: &mut Game<'_>, m: &Module) -> Option<usize> {
    if !m.b("Reach poke") || game.held().is_some_and(|s| Kinetic::of(&s.item).is_some()) || game.mob_hit().is_some() {
        return None;
    }
    let eye = game.eye_position()?;
    let hits = spear_hits_at(game, game.yaw, game.pitch)?;
    let look = super::view_vector(game.yaw, game.pitch);
    let start = Vec3::new(eye[0], eye[1], eye[2]);
    let end = start.add_vector(Vec3::new(look[0], look[1], look[2]).scale(4.4));
    let ordinary = game.player.as_mut()?.attribute("minecraft:entity_interaction_range").ok()?;
    if hits.iter().any(|&i| known_friend(game, game.mobs[i].id)) {
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

// The attack hook, before your click: Some(false) takes the click (a friend, a reach poke). None: the game's own.
pub(super) fn before_click(game: &mut Game<'_>) -> Option<bool> {
    if !allowed(game) {
        return None;
    }
    if game.held().is_some_and(|s| Kinetic::of(&s.item).is_some()) {
        return block_stab(game).then_some(false);
    }
    if let Some((index, distance)) = game.mob_hit() {
        let id = game.mobs[index].id;
        if game.before_block(distance) && !before_attack(game, id) {
            return Some(false);
        }
        spear_pass(game, id);
        return None;
    }
    let m = module(SPEAR)?;
    let slot = poke_slot(game, &m)?;
    queue_action(game, m, Automatic::Jab { slot, lunge: false }).then_some(false)
}

fn jab_ready(game: &Game<'_>, slot: usize) -> bool {
    available_item(game, slot).is_some()
        && slot_strength(game, slot, 0.0).is_some_and(|s| s >= 1.0)
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

// A hit of queue_action's waits for the start of the next tick.
pub(super) fn due_waiting() -> bool {
    state(|s| s.due.is_some())
}

// The start of a tick: the hit queue_action has ready goes.
pub(super) fn due(game: &mut Game<'_>) {
    if let Some(hit) = state(|s| s.due.take()) {
        hit(game);
    }
}

// A hit of the melee modules' own: once the server has your look, at the start of the next tick, checked again (the
// same world, slot, item and settings, under 200 ms), the slot switched for one hit and the hit sent.
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
    let accepted = try_turn_then(owner, yaw, pitch, move |_| state(|s| s.due = Some(Box::new(move |game: &mut Game<'_>| {
        let done = done;
        if state(|s| s.queued_since) == Some(queued) {
            state(|s| s.queued = None);
        }
        let valid = done.current()
            && allowed(game)
            && queued.elapsed() <= Duration::from_millis(200)
            && game.dim == dimension
            && game.selected == original
            && game.held() == original_item.as_ref()
            && !acted(game)
            && super::remote::sunset_combat_remote::sprint_synced(game)
            && game.remote.as_ref().is_some_and(|r| Arc::ptr_eq(&connection, &r.session.counters))
            && module(owner).is_some_and(|now| now == m);
        if !valid {
            return;
        }
        let under = |game: &Game<'_>, id: u32| game.mob_hit().is_some_and(|(i, d)| game.mobs[i].id == id && game.before_block(d));
        let slot = match action {
            Automatic::Shield(id) => {
                if !under(game, id) || !shield_counts(game, id) || guard_attack(game, id) {
                    None
                } else {
                    best_axe(game)
                }
            }
            Automatic::SmashShield(id) => match smash_intent(game, &m, id) {
                mace_policy::Intent::BreakShield { slot } => Some(slot),
                _ => None,
            },
            Automatic::Smash(id) => {
                if !under(game, id) || smash_intent(game, &m, id) != mace_policy::Intent::Smash || guard_attack(game, id) {
                    None
                } else if m.b("Auto swap") {
                    game.mobs.iter().find(|t| t.id == id).and_then(|target| best_mace(game, target, &m))
                } else {
                    available_item(game, game.selected).filter(|s| &*s.item == "minecraft:mace").map(|_| game.selected)
                }
            }
            Automatic::Jab { slot, lunge } => {
                let valid = available_item(game, slot)
                    .is_some_and(|s| Kinetic::of(&s.item).is_some() && (!lunge || s.enchantment_level("minecraft:lunge") > 0))
                    && jab_ready(game, slot)
                    && !block_stab(game)
                    && (lunge || poke_slot(game, &m) == Some(slot));
                valid.then_some(slot)
            }
        };
        let Some(slot) = slot else {
            return;
        };
        let restore = m.b(if m.name == MACE { "Swap back" } else { "Switch back" });
        let hold_ticks = if matches!(action, Automatic::Shield(_)) { 2 } else { 1 };
        let delay = back_delay(game, &m);
        if !take_slot(game, WeaponPlan { slot, owner, restore, delay }, hold_ticks, Some(done)) {
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
                    Automatic::Smash(_) => s.mace.accepted(mace_policy::Intent::Smash, id, game.ticks),
                    _ => s.mace.accepted(mace_policy::Intent::BreakShield { slot }, id, game.ticks),
                });
            }
            Automatic::Jab { slot, .. } => {
                if let Some(kinetic) = available_item(game, slot).and_then(|s| Kinetic::of(&s.item)) {
                    stab(game, kinetic);
                }
                state(|s| s.lunge.clear());
            }
        }
        claim_tick(game);
    }))));
    if !accepted && state(|s| s.queued_since) == Some(queued) {
        state(|s| s.queued = None);
    }
    accepted
}

pub(super) fn tick(game: &mut Game<'_>) {
    flush_friend_hint(game);
    stop_guarded_use(game);
    let grounded = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
    state(|s| s.mace.observe_grounded(grounded));
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
            || state(|s| s.queued_since.is_none_or(|t| t.elapsed() > Duration::from_millis(200)))
        {
            super::sunset::cancel_turns(owner);
            state(|s| s.queued = None);
        }
        return;
    }
    if !allowed(game) || acted(game) || game.attack_clicks > 0 || game.clicks.miss_time > 0 || state(|s| s.lease.is_some()) {
        return;
    }
    if let Some(m) = module(SPEAR)
        && state(|s| s.lunge.active(game.ticks))
        && game.riding.is_none()
        && game.player.as_ref().is_some_and(|p| !p.fall_flying && !p.fluids.was_touching_water && (game.creative || p.health.food.food >= 7))
        && let Some(slot) = best_spear(game, true).filter(|&i| jab_ready(game, i))
        && !block_stab(game)
        && queue_action(game, m, Automatic::Jab { slot, lunge: true })
    {
        return;
    }
    let Some((index, distance)) = game.mob_hit() else {
        return;
    };
    if !game.before_block(distance) {
        return;
    }
    let id = game.mobs[index].id;
    if let Some(m) = module(SHIELD).filter(|m| m.p("Mode") == "Auto")
        && game.ticks >= state(|s| s.shield_ready)
        && enemy(game, &game.mobs[index], false)
        && shield_counts(game, id)
        && !broken_shield(game, id)
        && holding(game, &m)
        && best_axe(game).is_some()
        && queue_action(game, m, Automatic::Shield(id))
    {
        return;
    }
    if let Some(m) = module(MACE).filter(|m| m.b("Smash assist")) {
        match smash_intent(game, &m, id) {
            mace_policy::Intent::BreakShield { .. } => {
                queue_action(game, m, Automatic::SmashShield(id));
            }
            mace_policy::Intent::Hold => {
                drop_sprint(game);
                game.clicks.miss_time = game.clicks.miss_time.max(1);
            }
            mace_policy::Intent::Smash => {
                queue_action(game, m, Automatic::Smash(id));
            }
            mace_policy::Intent::Idle => {}
        }
    }
}

// A hit went out this tick: the game's own click and use wait.
fn claim_tick(game: &mut Game<'_>) {
    game.attack_clicks = 0;
    game.clicks.miss_time = game.clicks.miss_time.max(1);
    game.clicks.right_click_delay = game.clicks.right_click_delay.max(2);
    game.use_delay = game.use_delay.max(2);
}

fn broken_shield(game: &Game<'_>, id: u32) -> bool {
    state(|s| s.broken_shield.is_some_and(|(target, until)| target == id && game.ticks < until) || s.mace.shield_attempted(id, game.ticks))
}

// Falling far enough onto `id` for Mace's smash, with a mace in hand or to swap to.
fn smash_ready(game: &Game<'_>, id: u32) -> bool {
    let (Some(m), Some(p)) = (module(MACE), game.player.as_ref()) else {
        return false;
    };
    if state(|s| s.mace.spent()) || !p.fall_distance.is_finite() || p.fall_distance <= 1.5 || p.fall_distance < m.n("Min fall") || p.fall_flying {
        return false;
    }
    available_item(game, game.selected).is_some_and(|s| &*s.item == "minecraft:mace")
        || (m.b("Auto swap") && game.mobs.iter().find(|t| t.id == id).is_some_and(|target| best_mace(game, target, &m).is_some()))
}

fn can_hold_smash(game: &mut Game<'_>, id: u32) -> bool {
    let (yaw, pitch) = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    super::sunset_mace_native::continuation(game, id, yaw, pitch).unwrap_or(false)
}

// A hit from Kill Aura or Trigger Bot on `id`, planned for the next tick: Shield Breaker's axe at a raised shield or
// Mace's mace goes in hand for it (a smash uses up the fall). The slot to go back to after, if it is to.
pub(super) fn strike_weapon(game: &mut Game<'_>, id: u32) -> Option<(usize, u64)> {
    let plan = preferred_weapon(game, id)?;
    let previous = game.selected;
    if plan.owner == MACE && game.player.as_ref().is_some_and(|p| p.fall_distance > 1.5 && !p.fall_flying) {
        state(|s| s.mace.accepted(mace_policy::Intent::Smash, 0, 0));
    }
    hold(game, plan.slot);
    (plan.restore && plan.slot != previous).then_some((previous, plan.delay))
}

fn smash_intent(game: &mut Game<'_>, m: &Module, id: u32) -> mace_policy::Intent {
    let Some(target) = game.mobs.iter().find(|t| t.id == id) else {
        return mace_policy::Intent::Idle;
    };
    let target_allowed = enemy(game, target, m.b("Mobs"));
    let target_immune = target.hurt_time > 0;
    let shield_blocks = observed_shield(game, id) && !broken_shield(game, id);
    let mace_available = smash_ready(game, id);
    let can_continue = can_hold_smash(game, id);
    let Some(p) = game.player.as_ref() else {
        return mace_policy::Intent::Idle;
    };
    let frame = mace_policy::Frame {
        tick: game.ticks,
        target: id,
        grounded: p.ground.on_ground,
        descending: p.velocity.y < 0.0,
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
        timing: if m.p("Smash timing") == "Delayed" { mace_policy::Timing::Delayed } else { mace_policy::Timing::Direct },
        charge: f64::from(slot_strength(game, game.selected, 0.5).unwrap_or(0.0)),
        charge_next: f64::from(slot_strength(game, game.selected, 1.5).unwrap_or(0.0)),
        target_immune,
        sprinting: p.sprinting,
    };
    state(|s| s.mace.plan(frame))
}

// Spear reach (Duelkit's): the hotbar spear the crosshair reaches as far as, while what Holding lists is held (a
// sword or axe). None: off.
pub(super) fn spear_reach_slot(game: &Game<'_>) -> Option<usize> {
    module(SPEAR).filter(|m| m.p("Spear reach") != "Off" && holding(game, m))?;
    best_spear(game, false)
}

// Before a hit on `id` with a sword or axe: Spear reach selects the spear and the weapon again (both told to the
// server), as a manual reach swap does; When out of reach only past the reach you have without it.
pub(super) fn spear_pass(game: &mut Game<'_>, id: u32) {
    let Some(spear) = spear_reach_slot(game) else {
        return;
    };
    let every = module(SPEAR).is_some_and(|m| m.p("Spear reach") == "Every hit");
    let (Some(eye), Some(target)) = (game.eye_position(), game.mobs.iter().find(|m| m.id == id)) else {
        return;
    };
    let b = target.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let distance = policy::box_distance(eye, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    let ordinary = game.player.as_mut().and_then(|p| p.attribute("minecraft:entity_interaction_range").ok()).unwrap_or(3.0);
    if every || distance.is_none_or(|d| d > ordinary) {
        let weapon = game.selected;
        hold(game, spear);
        hold(game, weapon);
    }
}

// Shield Breaker's Stun slam, before Kill Aura's axe hit on `id`: falling far enough onto their raised shield, the
// mace to hit with straight after it.
pub(super) fn stun_slam(game: &Game<'_>, id: u32) -> Option<usize> {
    module(SHIELD).filter(|m| m.b("Stun slam"))?;
    let mace = module(MACE)?;
    let p = game.player.as_ref()?;
    let axe = game.held().and_then(|s| game.res.items().get(&s.item)).is_some_and(|i| i.disable_blocking_seconds > 0.0);
    if !axe || !observed_shield(game, id) || p.fall_flying || p.fall_distance <= 1.5 || p.fall_distance < mace.n("Min fall") {
        return None;
    }
    let target = game.mobs.iter().find(|t| t.id == id)?;
    best_mace(game, target, &mace)
}

// Key Throw's Hotbar keys on a Lunge spear (Duelkit's Quick use): a jab with it at the start of the next tick, before
// its move packet (the lunge is the jab's), and the slot you held back in the same tick.
pub(super) fn quick_lunge(game: &mut Game<'_>, slot: usize) -> bool {
    // No charge check: the game's key selected the spear, which restarted this client's charge, while the server's
    // is still the weapon's when the jab arrives.
    let fit = move |game: &Game<'_>| {
        allowed(game)
            && game.clicks.miss_time <= 0
            && game.mining.is_none()
            && !block_stab(game)
            && game.riding.is_none()
            && available_item(game, slot).is_some_and(|s| Kinetic::of(&s.item).is_some() && s.enchantment_level("minecraft:lunge") > 0)
            && game.player.as_ref().is_some_and(|p| !p.fall_flying && !p.fluids.was_touching_water && (game.creative || p.health.food.food >= 7))
    };
    if !fit(game) || state(|s| s.due.is_some()) {
        return false;
    }
    let back = game.selected;
    state(|s| {
        s.due = Some(Box::new(move |game: &mut Game<'_>| {
            let Some(kinetic) = available_item(game, slot).and_then(|s| Kinetic::of(&s.item)) else {
                return;
            };
            // The game's own hotbar key selected the spear meanwhile: that is undone first.
            if ![back, slot].contains(&game.selected) {
                return;
            }
            game.selected = back;
            if !fit(game) || acted(game) {
                return;
            }
            hold(game, slot);
            stab(game, kinetic);
            hold(game, back);
            claim_tick(game);
        }))
    });
    true
}
