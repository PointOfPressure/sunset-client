// The melee helpers from Rust's PR #16: Auto Weapon (its sections Best weapon and Axe on shields (Shield Breaker
// before)), Auto Spear (Spear Swap before, then Auto Weapon's Spear), W Tap, Jump Reset and Friend Guard, and the
// weapon they pick for Kill Aura's and Trigger Bot's hits. Hits use the game's own inventory, movement, item
// components, packets and spear ray. The mace is Auto Mace's (auto_mace.rs).
use super::sunset::{Canvas, Category, Module, gui_open, hold, try_turn_then, with};
use super::sunset_combat_control as control;
use super::sunset_melee_policy::{self as policy, SlotLease};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::items::ItemStack;
use horizon_client::mobs::Mob;
use horizon_client::spear::{Kinetic, REACH, hits_in_reach};
use horizon_core::geometry::Vec3;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

const WEAPON: &str = "Auto Weapon";
// Auto Weapon's sections: each its switch's name, and the owner of what it does (the hand, its look).
const BEST: &str = "Best weapon";
const SHIELD: &str = "Axe on shields";
// Auto Spear: the module, and the owner of its jabs, lunges and reach swaps.
const SPEAR: &str = "Auto Spear";
const TAP: &str = "W Tap";
const JUMP: &str = "Jump Reset";
const FRIEND: &str = "Friend Guard";
// The keys W Tap and Jump Reset press or let go for a tick.
const INPUT_KEYS: [KeyCode; 4] = [KeyCode::KeyW, KeyCode::KeyS, KeyCode::ControlLeft, KeyCode::Space];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        // One weapon module for every hit: each section on its own switch, all of them with the Holding filter and the
        // slot back after the hit.
        Module::new(WEAPON, Category::Combat, "Picks the weapon for each hit.")
            .pick("Mode", &["Custom", "Sword", "Axe"])
            .bool(BEST, true)
            .bool("Prefer axes", false)
            .bool("Totem hit", false)
            .bool(SHIELD, true)
            .pick("Shield mode", &["Passive", "Auto"])
            .bool("Shield held", false)
            .text("Holding", "sword,axe,spear,mace,trident,empty")
            .bool("Swap back", true)
            .pick("Back after", &["Same tick", "A delay"])
            .num("Shortest delay", 2.0, 1.0, 20.0, 1.0)
            .num("Longest delay", 4.0, 1.0, 20.0, 1.0),
        // The spear for your hits: Lunge key, Lunge on hit, Reach poke, Spear reach, Extended reach and the Damage
        // indicator, with the Holding filter and the slot back after a jab, as Auto Weapon has them.
        Module::new(SPEAR, Category::Combat, "Lunges, jabs and reaches with the spear.")
            .text("Lunge key", "None")
            .bool("Lunge on hit", false)
            .bool("Reach poke", false)
            .pick("Spear reach", &["Off", "When out of reach", "Every hit"])
            .bool("Extended reach", false)
            .num("Reach chance (%)", 100.0, 0.0, 100.0, 5.0)
            .text("Reach weapons", "sword,axe,mace")
            .bool("Works with Trigger Bot", true)
            .bool("Damage indicator", false)
            .text("Holding", "sword,axe,spear,mace,trident,empty")
            .bool("Swap back", true)
            .pick("Back after", &["Same tick", "A delay"])
            .num("Shortest delay", 2.0, 1.0, 20.0, 1.0)
            .num("Longest delay", 4.0, 1.0, 20.0, 1.0),
        Module::new(TAP, Category::Combat, "Resets sprint between hits.")
            .pick("Mode", &["W Tap", "S Tap"])
            .num("Hold", 1.0, 1.0, 8.0, 1.0)
            .num("Longest hold", 1.0, 1.0, 8.0, 1.0)
            .num("Reaction (ticks)", 0.0, 0.0, 5.0, 1.0)
            .num("Chance (%)", 100.0, 0.0, 100.0, 5.0),
        Module::new(JUMP, Category::Combat, "Jumps to take less knockback.")
            .num("Chance (%)", 100.0, 0.0, 100.0, 5.0),
        Module::new(FRIEND, Category::Client, "Stops hits on friends.")
            .bool("Notifications", true),
    ]
}

#[derive(Clone, Copy)]
struct InputSave {
    before: [bool; INPUT_KEYS.len()],
    physical: [bool; INPUT_KEYS.len()],
    changed: [bool; INPUT_KEYS.len()],
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
    // W Tap and Sprint's Keep sprint: forward let go from `tap_from` until `tap_until` (S held too with `tap_back`).
    tap_from: u64,
    tap_until: u64,
    tap_back: bool,
    jump_pending: bool,
    input: Option<InputSave>,
    last_attack: Option<u64>,
    shield_ready: u64,
    broken_shield: Option<(u32, u64)>,
    friend_hint_at: Option<Instant>,
    friend_hint: Option<u32>,
    jump_owner: Option<&'static str>,
    // Auto Spear's Extended reach: the slot to go back to, the ticks before it may (a missed Chance roll's lockout too),
    // and Trigger Bot's target for a stab at the start of the next tick.
    reach_back: Option<usize>,
    reach_wait: u32,
    reach_strike: Option<super::sunset::AttackGuard>,
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

// Auto Weapon, on with the section `owner` names on; any other owner's module of that name (Auto Spear's own).
fn section(owner: &str) -> Option<Module> {
    match owner {
        BEST | SHIELD => module(WEAPON).filter(|m| weapon_on(m, owner)),
        _ => module(owner),
    }
}

// Auto Weapon's Mode: Custom runs the sections switched on; each other mode one weapon's. Axe brings Axe on shields.
pub(super) fn weapon_on(m: &Module, owner: &str) -> bool {
    match m.p("Mode") {
        "Sword" => owner == BEST,
        "Axe" => owner == BEST || owner == SHIELD,
        _ => m.b(owner),
    }
}

// Best weapon's pick in the Sword and Axe modes: only that kind; Custom's goes by Prefer axes.
fn best_fits(game: &Game<'_>, m: &Module, slot: usize) -> bool {
    let kind = game.inventory.get(slot).and_then(|s| s.as_ref()).map_or("empty", |s| stack_kind(game, s));
    match m.p("Mode") {
        "Sword" => kind == "sword",
        "Axe" => kind == "axe",
        _ => true,
    }
}

fn alive(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(super::sunset::ID)
        && game.remote.is_some()
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && !game.spectator()
        && game.selected < 9
}

// The hands are free for an automatic hit or swap (Auto Mace's too).
pub(super) fn allowed(game: &Game<'_>) -> bool {
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

// A hit or use went out this tick (anyone's): no other may.
pub(super) fn acted(game: &Game<'_>) -> bool {
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

// A lunge may go: not riding, gliding or in water, and food enough to sprint (the game's own lunge rules).
fn lunge_fits(game: &Game<'_>) -> bool {
    game.riding.is_none()
        && game.player.as_ref().is_some_and(|p| !p.fall_flying && !p.fluids.was_touching_water && (game.creative || p.health.food.food >= 7))
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
pub(super) fn observed_shield(game: &Game<'_>, id: u32) -> bool {
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

// A shield Axe on shields takes the axe to: raised at you, or with Shield held only held.
fn shield_counts(game: &Game<'_>, id: u32) -> bool {
    observed_shield(game, id) || (section(SHIELD).is_some_and(|m| m.b("Shield held")) && holds_shield(game, id))
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

// A slot to hit `id` with, and whether to go back after.
#[derive(Clone, Copy)]
struct WeaponPlan {
    pub slot: usize,
    pub owner: &'static str,
    pub restore: bool,
    // Ticks more before the slot goes back (Back after A delay).
    pub delay: u64,
}

// Match what the hand holds against the Holding list.
fn held_kind(game: &Game<'_>) -> &'static str {
    game.held().map_or("empty", |stack| stack_kind(game, stack))
}

fn stack_kind(game: &Game<'_>, stack: &ItemStack) -> &'static str {
    let tagged = |tag: &str| game.res.items().item_tags.get(tag).is_some_and(|items| items.contains(&*stack.item));
    match &*stack.item {
        "minecraft:mace" => "mace",
        "minecraft:trident" => "trident",
        "minecraft:totem_of_undying" => "totem",
        name if Kinetic::of(name).is_some() => "spear",
        _ if tagged("minecraft:swords") => "sword",
        _ if tagged("minecraft:axes") => "axe",
        _ => "other",
    }
}

// The swap modules start only from what their Holding lists; Totem hit adds a totem in hand.
fn holding(game: &Game<'_>, m: &Module) -> bool {
    let kind = held_kind(game);
    util::listed(m.t("Holding"), kind) || (kind == "totem" && m.b("Totem hit"))
}

// A roll from 0 to 9999 for the chance settings, different each tick and for each `salt`.
fn roll(game: &Game<'_>, salt: u64) -> u64 {
    let mut x = game.ticks.wrapping_add(salt.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x % 10_000
}

fn chance(game: &Game<'_>, m: &Module, salt: u64) -> bool {
    (roll(game, salt) as f64) < m.n("Chance (%)") * 100.0
}

// A tick count from `lo` to `hi` (either way round), picked by the tick.
fn between(game: &Game<'_>, lo: f64, hi: f64, salt: u64) -> u64 {
    let (lo, hi) = (lo.min(hi).max(0.0) as u64, lo.max(hi).max(0.0) as u64);
    lo + roll(game, salt) % (hi - lo + 1)
}

// After a delay: the swapped weapon stays in hand a few ticks, from Shortest to Longest delay (picked by the
// tick it starts, so it varies), before the slot goes back; 0 with Same tick.
pub(super) fn back_delay(game: &Game<'_>, m: &Module) -> u64 {
    if m.p("Back after") != "A delay" {
        return 0;
    }
    let lo = m.n("Shortest delay") as u64;
    let hi = (m.n("Longest delay") as u64).max(lo);
    lo + game.ticks.wrapping_mul(0x9E37_79B9) % (hi - lo + 1)
}

// The weapon Auto Weapon wants for a hit on `id`, while what Holding lists is held: an axe at a raised shield (Axe on
// shields), else the best weapon (Best weapon), none while Auto Mace has the hit. It only chooses: whoever hits
// switches and goes back.
fn preferred_weapon(game: &Game<'_>, id: u32) -> Option<WeaponPlan> {
    if !allowed(game) {
        return None;
    }
    let m = module(WEAPON).filter(|m| holding(game, m))?;
    let target = game.mobs.iter().find(|m| m.id == id && m.alive() && !m.removed)?;
    let plan = |slot, owner| WeaponPlan { slot, owner, restore: m.b("Swap back"), delay: back_delay(game, &m) };
    let shield_up = shield_counts(game, id) && !broken_shield(game, id);
    if weapon_on(&m, SHIELD)
        && target.kind.id == "minecraft:player"
        && shield_up
        && !game.held().and_then(|s| game.res.items().get(&s.item)).is_some_and(|i| i.disable_blocking_seconds > 0.0)
    {
        return best_axe(game).map(|slot| plan(slot, SHIELD));
    }
    if super::sunset_auto_mace::claims(game) {
        return None;
    }
    if weapon_on(&m, BEST) {
        let axes = m.p("Mode") == "Axe" || (m.p("Mode") == "Custom" && m.b("Prefer axes"));
        return super::sunset::best_weapon(game, axes)
            .filter(|&slot| slot != game.selected && best_fits(game, &m, slot))
            .map(|slot| plan(slot, BEST));
    }
    None
}

// Auto Weapon's Axe on shields has the hit on `id`: the player's shield counts and a hotbar axe (maybe the one in
// hand) is for it. Auto Mace leaves such a hit alone.
pub(super) fn axe_at_shield(game: &Game<'_>, id: u32) -> bool {
    module(WEAPON).is_some_and(|m| weapon_on(&m, SHIELD) && holding(game, &m))
        && game.mobs.iter().any(|t| t.id == id && t.alive() && !t.removed && t.kind.id == "minecraft:player")
        && shield_counts(game, id)
        && !broken_shield(game, id)
        && best_axe(game).is_some()
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

// Your own click on `id`: false stops it (a friend); Auto Weapon switches the weapon for it.
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
    for owner in [BEST, SHIELD, SPEAR] {
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

// An Attack packet went out: an Axe on shields hit counts for a second, W Tap starts.
fn note_attack(game: &Game<'_>, id: u32) {
    note_action(game);
    super::sunset_swap_keys::hit(game);
    if observed_shield(game, id)
        && game.held().and_then(|s| game.res.items().get(&s.item)).is_some_and(|i| i.disable_blocking_seconds > 0.0)
    {
        state(|s| s.broken_shield = Some((id, game.ticks.saturating_add(20))));
    }
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let living = game.mobs.iter().any(|t| t.id == id && t.alive() && !t.removed);
    let tap = module(TAP);
    if let Some(m) = &tap
        && policy::should_tap(
            p.sprinting,
            game.keys.contains(&KeyCode::KeyW),
            slot_strength(game, game.selected, 0.5).unwrap_or(0.0),
            living,
        )
    {
        start_tap(game, m);
    } else if tap.is_none() {
        keep_sprint(game, id);
    }
}

// W Tap after a hit: by Chance, Reaction ticks later, forward let go for Hold to Longest hold ticks.
fn start_tap(game: &Game<'_>, m: &Module) {
    if state(|s| s.last_attack == Some(game.ticks)) {
        return;
    }
    state(|s| s.last_attack = Some(game.ticks));
    if !chance(game, m, 1) {
        return;
    }
    let from = game.ticks.saturating_add(m.n("Reaction (ticks)").clamp(0.0, 5.0) as u64);
    let hold = between(game, m.n("Hold"), m.n("Longest hold"), 2).clamp(1, 8);
    state(|s| (s.tap_from, s.tap_until, s.tap_back) = (from, from + hold, m.p("Mode") == "S Tap"));
}

// Sprint's Keep sprint, without W Tap: after a sprinting hit on a player with forward held, forward is let go for the
// restore delay's ticks and pressed again, so the server sees the sprint start again (0 ticks: never).
fn keep_sprint(game: &Game<'_>, id: u32) {
    let Some(m) = module("Sprint").filter(|m| m.b("Keep sprint")) else {
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let player = game.mobs.iter().any(|t| t.id == id && t.alive() && !t.removed && t.kind.id == "minecraft:player");
    if !player
        || !p.sprinting
        || !game.keys.contains(&KeyCode::KeyW)
        || game.keys.contains(&KeyCode::ShiftLeft)
        || p.collision.horizontal
        || game.using.is_some()
        || p.health.food.food <= 6
        || state(|s| s.last_attack == Some(game.ticks) || game.ticks < s.tap_until)
    {
        return;
    }
    let ticks = between(game, m.n("Shortest restore delay"), m.n("Longest restore delay"), 3);
    state(|s| {
        s.last_attack = Some(game.ticks);
        if ticks > 0 {
            (s.tap_from, s.tap_until, s.tap_back) = (game.ticks, game.ticks + ticks, false);
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
    if module(JUMP).is_some_and(|m| chance(game, &m, 4))
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
    let tap =state(|s| (s.tap_from..s.tap_until).contains(&game.ticks).then_some(s.tap_back));
    let asked = state(|s| s.jump_owner.take()).is_some_and(|owner| section(owner).is_some() && control::available(owner));
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
    if let Some(back) = tap {
        game.keys.remove(&KeyCode::KeyW);
        game.keys.remove(&KeyCode::ControlLeft);
        if back {
            game.keys.insert(KeyCode::KeyS);
        }
    }
    if jump {
        game.keys.insert(KeyCode::Space);
    }
    let changed = std::array::from_fn(|i| before[i] != game.keys.contains(&INPUT_KEYS[i]));
    state(|s| s.input = Some(InputSave { before, physical, changed }));
}

// Wind Jump: jump held for this tick (after pre_input, before the tick's move and PlayerInput); post_input lets it
// go.
pub(super) fn press_jump(game: &mut Game<'_>) {
    const SPACE: usize = 3;
    if game.keys.contains(&INPUT_KEYS[SPACE]) {
        return;
    }
    let before = INPUT_KEYS.map(|key| game.keys.contains(&key));
    let physical = INPUT_KEYS.map(|key| game.physical_keys.contains(&key));
    state(|s| s.input.get_or_insert(InputSave { before, physical, changed: [false; INPUT_KEYS.len()] }).changed[SPACE] = true);
    game.keys.insert(INPUT_KEYS[SPACE]);
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

// Auto Spear's Lunge key, pressed: a lunge waits up to 10 ticks for its moment.
pub(super) fn key(game: &Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_playing() || gui_open() || !alive(game) {
        return;
    }
    let Some(m) = section(SPEAR) else {
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
    if !m.b("Reach poke") || !holding(game, m) || game.held().is_some_and(|s| Kinetic::of(&s.item).is_some()) || game.mob_hit().is_some() {
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

// How far along your look Extended reach looks for a block (none may be there) and a mob, and how near the mob is.
const REACH_LINE: f64 = 4.75;
const REACH_NEAR: f64 = 6.0;

// Extended reach's spear: the first in the hotbar without Lunge (a Lunge spear's stab would carry you).
fn reach_spear(game: &Game<'_>) -> Option<usize> {
    (0..9).find(|&slot| {
        game.inventory
            .get(slot)
            .and_then(|s| s.as_ref())
            .is_some_and(|s| s.count > 0 && Kinetic::of(&s.item).is_some() && s.enchantment_level("minecraft:lunge") == 0)
    })
}

// Extended reach's target: with such a spear in the hotbar, the hand's attack charged full, and nobody under your
// crosshair, the line along your look meets no block in its first 4.75 blocks, and the first mob in the world's list
// within 6 blocks, in sight, whose box that line crosses.
fn reach_target(game: &Game<'_>) -> Option<u32> {
    reach_spear(game)?;
    if slot_strength(game, game.selected, 0.0).is_none_or(|c| c < 1.0) || game.mob_hit().is_some_and(|(_, d)| game.before_block(d)) {
        return None;
    }
    let eye = game.eye_position()?;
    let look = super::view_vector(game.yaw, game.pitch);
    if game.clip_ray(eye, look, REACH_LINE).is_some() {
        return None;
    }
    let from = Vec3::new(eye[0], eye[1], eye[2]);
    let to = from.add_vector(Vec3::new(look[0], look[1], look[2]).scale(REACH_LINE));
    let p = game.player.as_ref()?;
    game.mobs
        .iter()
        .find(|t| {
            let b = t.bounds();
            let their_eye = [t.position.x, t.position.y + t.eye_height(), t.position.z];
            t.alive()
                && !t.removed
                && p.position.distance_squared(t.position).sqrt() <= REACH_NEAR
                && super::sunset_combat::clip_clear(game, eye, their_eye, util::dist(eye, their_eye))
                && (b.contains(from) || b.clip(from, to).is_some())
        })
        .map(|t| t.id)
}

// Extended reach may swap now, for its target: the Chance roll (a miss locks it out this tick and the next), no swap
// of its own under way, not falling past 1.5 blocks (a mace smash's), what Reach weapons lists (sword, axe, mace) in
// hand, and no friend along the stab (Friend Guard).
fn reach_check(game: &Game<'_>, m: &Module) -> Option<u32> {
    let target = reach_target(game)?;
    if roll(game, 9) as f64 >= m.n("Reach chance (%)") * 100.0 {
        state(|s| s.reach_wait = 1);
    }
    let kind = held_kind(game);
    let fits = matches!(kind, "sword" | "axe" | "mace") && util::listed(m.t("Reach weapons"), kind);
    let falling = game.player.as_ref().is_some_and(|p| p.fall_distance > 1.5);
    (state(|s| s.reach_wait == 0 && s.reach_back.is_none()) && !falling && fits && !block_stab(game)).then_some(target)
}

// Extended reach's stab: the spear in hand, its stab along your look at once (as your click would go with it), and the
// slot you had back two ticks on (reach_tick).
fn reach_go(game: &mut Game<'_>) -> bool {
    let Some((spear, kinetic)) = reach_spear(game).and_then(|slot| Some((slot, Kinetic::of(&game.inventory.get(slot)?.as_ref()?.item)?))) else {
        return false;
    };
    let back = game.selected;
    state(|s| {
        s.reach_back = Some(back);
        s.reach_wait = 1;
    });
    hold(game, spear);
    stab(game, kinetic);
    claim_tick(game);
    true
}

// The start of a tick (`busy`: a hit or slot change went before in it): Extended reach's wait runs down, then its slot
// goes back; or Trigger Bot's stab goes, its target still the one along your look. True if anything was sent.
pub(super) fn reach_tick(game: &mut Game<'_>, busy: bool) -> bool {
    let back = state(|s| {
        if s.reach_wait > 0 {
            s.reach_wait -= 1;
            None
        } else if busy {
            None
        } else {
            s.reach_back.take()
        }
    });
    if let Some(slot) = back {
        let was = game.selected;
        if alive(game) && slot < 9 {
            hold(game, slot);
        }
        return game.selected != was;
    }
    let Some(guard) = state(|s| s.reach_strike.take()) else {
        return false;
    };
    if busy || !guard.valid(game) || acted(game) || !allowed(game) || state(|s| s.reach_back.is_some() || s.reach_wait > 0) || reach_target(game) != Some(guard.id()) {
        return false;
    }
    reach_go(game)
}

// Trigger Bot with nobody under the crosshair: Extended reach's target (Works with Trigger Bot), for reach_strike.
pub(super) fn reach_trigger(game: &Game<'_>) -> Option<u32> {
    let m = section(SPEAR).filter(|m| m.b("Extended reach") && m.b("Works with Trigger Bot"))?;
    if !allowed(game) {
        return None;
    }
    reach_check(game, &m)
}

// Trigger Bot hits `id` by Extended reach: the stab goes at the start of the next tick.
pub(super) fn reach_strike(guard: super::sunset::AttackGuard) {
    state(|s| s.reach_strike = Some(guard));
}

// The attack hook, before your click: Some(false) takes the click (a friend, a reach poke, Extended reach's stab).
// None: the game's own.
pub(super) fn before_click(game: &mut Game<'_>) -> Option<bool> {
    if !allowed(game) {
        return None;
    }
    if game.held().is_some_and(|s| Kinetic::of(&s.item).is_some()) {
        return block_stab(game).then_some(false);
    }
    if let Some(m) = section(SPEAR).filter(|m| m.b("Extended reach"))
        && reach_check(game, &m).is_some()
        && reach_go(game)
    {
        return Some(false);
    }
    if let Some((index, distance)) = game.mob_hit() {
        let id = game.mobs[index].id;
        let held = game.selected;
        if game.before_block(distance)
            && !guard_attack(game, id)
            && let Some(m) = section(SPEAR)
            && let Some(slot) = lunge_on_hit(game, &m, index)
            && queue_action(game, SPEAR, m, Automatic::Jab { slot, lunge: true })
        {
            return Some(false);
        }
        if game.before_block(distance) && !before_attack(game, id) {
            return Some(false);
        }
        // One slot change before a hit at most: none more for Spear reach after Auto Weapon's.
        if game.selected == held {
            spear_pass(game, id);
        }
        return None;
    }
    let m = section(SPEAR)?;
    let slot = poke_slot(game, &m)?;
    queue_action(game, SPEAR, m, Automatic::Jab { slot, lunge: false }).then_some(false)
}

// Lunge on hit: your click on an enemy (the mob at `index`), with what Holding lists in hand, goes as a jab of the
// hotbar's Lunge spear instead, when its stab along your look reaches them and a lunge may go. The jab waits for the
// start of the next tick and the slot goes back the tick after, one slot change a tick.
fn lunge_on_hit(game: &Game<'_>, m: &Module, index: usize) -> Option<usize> {
    if !m.b("Lunge on hit") || !holding(game, m) || held_kind(game) == "spear" || !lunge_fits(game) || block_stab(game) {
        return None;
    }
    if !enemy(game, &game.mobs[index], true) || !spear_hits_at(game, game.yaw, game.pitch)?.contains(&index) {
        return None;
    }
    best_spear(game, true).filter(|&slot| jab_ready(game, slot))
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
    Jab { slot: usize, lunge: bool },
}

// A hit of queue_action's waits for the start of the next tick.
pub(super) fn due_waiting() -> bool {
    state(|s| s.due.is_some())
}

// The start of a tick, before its move packet (a slot change after it flags Grim's Post), after lease_back: the hit
// queue_action has ready goes.
pub(super) fn due(game: &mut Game<'_>) {
    if let Some(hit) = state(|s| s.due.take()) {
        hit(game);
    }
}

// A hit of the melee modules' own: once the server has your look, at the start of the next tick, checked again (the
// same world, slot, item and settings, under 200 ms), the slot switched for one hit and the hit sent.
fn queue_action(game: &mut Game<'_>, owner: &'static str, m: Module, action: Automatic) -> bool {
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
            && section(owner).is_some_and(|now| now == m);
        if !valid {
            return;
        }
        let under =|game: &Game<'_>, id: u32| game.mob_hit().is_some_and(|(i, d)| game.mobs[i].id == id && game.before_block(d));
        let slot = match action {
            Automatic::Shield(id) => {
                if !under(game, id) || !shield_counts(game, id) || guard_attack(game, id) {
                    None
                } else {
                    best_axe(game)
                }
            }
            Automatic::Jab { slot, lunge } => {
                let valid = available_item(game, slot)
                    .is_some_and(|s| Kinetic::of(&s.item).is_some() && (!lunge || s.enchantment_level("minecraft:lunge") > 0))
                    && jab_ready(game, slot)
                    && (!lunge || lunge_fits(game))
                    && !block_stab(game)
                    && (lunge || poke_slot(game, &m) == Some(slot));
                valid.then_some(slot)
            }
        };
        let Some(slot) = slot else {
            return;
        };
        let restore = m.b("Swap back");
        let hold_ticks = if matches!(action, Automatic::Shield(_)) { 2 } else { 1 };
        let delay = back_delay(game, &m);
        if !take_slot(game, WeaponPlan { slot, owner, restore, delay }, hold_ticks, Some(done)) {
            return;
        }
        match action {
            Automatic::Shield(id) => {
                game.send(S::Attack(play::Attack { id: id as i32 }));
                game.swing_arm();
                game.send(S::Punch(Empty));
                game.reset_attack_strength();
                state(|s| s.shield_ready = game.ticks.saturating_add(10));
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
    if !alive(game) {
        release_lease(game, true);
        state(|s| {
            s.lunge.clear();
            s.jump_pending = false;
            s.tap_until = 0;
        });
        return;
    }
    // The borrowed slot goes back at the start of a tick (lease_back), never here, after the tick's move packet.
    if let Some(lease) = state(|s| s.lease) {
        control::claim(lease.owner);
    }
    if let Some(owner) = state(|s| s.queued) {
        if !allowed(game)
            || section(owner).is_none()
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
    if let Some(m) = section(SPEAR)
        && state(|s| s.lunge.active(game.ticks))
        && lunge_fits(game)
        && let Some(slot) = best_spear(game, true).filter(|&i| jab_ready(game, i))
        && !block_stab(game)
        && queue_action(game, SPEAR, m, Automatic::Jab { slot, lunge: true })
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
    if let Some(m) = section(SHIELD).filter(|m| m.p("Shield mode") == "Auto")
        && game.ticks >= state(|s| s.shield_ready)
        && enemy(game, &game.mobs[index], false)
        && shield_counts(game, id)
        && !broken_shield(game, id)
        && holding(game, &m)
        && best_axe(game).is_some()
        && queue_action(game, SHIELD, m, Automatic::Shield(id))
    {
        return;
    }
}

// A hit went out this tick: the game's own click and use wait.
pub(super) fn claim_tick(game: &mut Game<'_>) {
    game.attack_clicks = 0;
    game.clicks.miss_time = game.clicks.miss_time.max(1);
    game.clicks.right_click_delay = game.clicks.right_click_delay.max(2);
    game.use_delay = game.use_delay.max(2);
}

fn broken_shield(game: &Game<'_>, id: u32) -> bool {
    state(|s| s.broken_shield.is_some_and(|(target, until)| target == id && game.ticks < until))
}

// A hit from Kill Aura or Trigger Bot on `id`, planned: the slot of Auto Weapon's axe at a raised shield or best
// weapon for it, when it is not the one in hand, and the ticks after the hit it goes back (None: it stays). The caller
// holds it.
pub(super) fn strike_weapon(game: &Game<'_>, id: u32) -> Option<(usize, Option<u64>)> {
    let plan = preferred_weapon(game, id)?;
    (plan.slot != game.selected).then_some((plan.slot, plan.restore.then_some(plan.delay)))
}

// The start of a tick, before its hits: a borrowed slot whose time is up goes back. (Sent after the tick's move packet
// it is what Grim's Post check flags; after a hit in the same tick, its PacketOrderE.) True if the slot changed.
pub(super) fn lease_back(game: &mut Game<'_>) -> bool {
    if !alive(game) {
        return false;
    }
    let before = game.selected;
    let disabled = state(|s| s.lease).is_some_and(|l| section(l.owner).is_none());
    release_lease(game, disabled || !game.native_playing() || gui_open());
    game.selected != before
}

// Spear reach (Sunset's): the hotbar spear the crosshair reaches as far as, while what Holding lists is held (a
// sword or axe). None: off.
pub(super) fn spear_reach_slot(game: &Game<'_>) -> Option<usize> {
    section(SPEAR).filter(|m| m.p("Spear reach") != "Off" && holding(game, m))?;
    best_spear(game, false)
}

// Before a hit on `id` with a sword or axe: Spear reach selects the spear and the weapon again (both told to the
// server), as a manual reach swap does; When out of reach only past the reach you have without it. Never after
// another slot change for the same hit. The two slot changes in one tick are what Grim's Reach check falsely flags
// (Grim issues #2601 and #2517), so it is left off on Grim servers.
pub(super) fn spear_pass(game: &mut Game<'_>, id: u32) {
    let Some(spear) = spear_reach_slot(game) else {
        return;
    };
    let every = section(SPEAR).is_some_and(|m| m.p("Spear reach") == "Every hit");
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

// Key Throw's Hotbar keys on a Lunge spear (Sunset's Quick use): a jab with it at the start of the next tick, before
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

// Spear's Damage indicator: while you charge a spear, the hit it would do to the nearest player as things stand (the
// game's kinetic damage: a player's base attack damage plus the closing speed along your look, in blocks a second, times the
// spear's damage multiplier, rounded down), under the crosshair; LETHAL from 20.
const BASE_DAMAGE: f64 = 1.0;

pub(super) fn draw_spear(game: &Game<'_>, w: f32, h: f32, out: &mut impl Canvas) {
    if section(SPEAR).is_none_or(|m| !m.b("Damage indicator")) {
        return;
    }
    let Some(kinetic) = game.using.and_then(|(slot, _)| game.stack_in(slot)).and_then(|s| Kinetic::of(&s.item)) else {
        return;
    };
    let Some(p) = game.player.as_ref() else {
        return;
    };
    let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let along = |v: [f64; 3]| (0..3).map(|i| look[i] * v[i] * 20.0).sum::<f64>();
    let theirs = util::player_target(game, 12.0, "Distance").map_or(0.0, |t| along([t.pos[0] - t.prev[0], t.pos[1] - t.prev[1], t.pos[2] - t.prev[2]]));
    let mine = along([p.velocity.x, p.velocity.y, p.velocity.z]);
    let damage = BASE_DAMAGE + ((mine - theirs).max(0.0) * f64::from(kinetic.damage_multiplier)).floor();
    let text = if damage >= 20.0 { "LETHAL".to_owned() } else { format!("Damage: {damage:.0} HP") };
    let (bw, bh) = (64.0_f32, 20.0_f32);
    let (x, y) = (w / 2.0 - bw / 2.0, h / 2.0 + 30.0);
    let share = (damage / 20.0).clamp(0.0, 1.0) as f32;
    out.fill([x, y, bw, bh], 0x960E_0E0E);
    out.text(&text, [x + (bw - game.native_text_width(&text)) / 2.0, y + 3.0], 0xFFFFFF);
    out.fill([x + 7.0, y + bh - 7.0, bw - 14.0, 4.0], 0x5A00_0000);
    let red = ((1.0 - share) * 255.0) as u32;
    let green = (share * 255.0) as u32;
    out.fill([x + 7.0, y + bh - 7.0, (bw - 14.0) * share, 4.0], 0xDC00_0000 | red << 16 | green << 8);
}
