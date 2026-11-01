// Original local crystal ghost prediction. Renderer and native special hitboxes
// omit a ghost; server entities and aura acknowledgements stay authoritative.
use super::sunset::{self, Category, Module};
use super::sunset_city_policy::{self as policy, History, Masks};
use super::Game;
use horizon_protocol::packets::play::{Clientbound as C, Serverbound as S};
use std::{
    any::Any,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

const NAME: &str = "Crystal Optimizer";
type Session = (i32, super::Dim, Arc<dyn Any + Send + Sync>);
#[derive(Default)]
struct State {
    session: Option<Session>,
    history: History,
    masks: Masks,
    tick: u64,
    hidden_total: u64,
    confirmed: u64,
    entity_range:Option<f64>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
static HAS_MASKS: AtomicBool = AtomicBool::new(false);
fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}

pub(super) fn defaults() -> Module {
    Module::new(NAME,Category::Combat,"Predicts a broken crystal after a forwarded hit. A rejected prediction restores its rendering and hitbox after the measured confirmation window.")
}
fn module() -> Option<Module> {
    sunset::with(|s| {
        s.module(NAME)
            .filter(|m| m.on)
            .cloned()
    })
}
fn same(game: &Game<'_>, old: &Session) -> bool {
    game.remote.as_ref().is_some_and(|r| {
        r.entity_id == old.0
            && game.dim == old.1
            && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ()
    })
}
fn session(game: &Game<'_>) -> Option<Session> {
    let r = game.remote.as_ref()?;
    Some((r.entity_id, game.dim, r.session.counters.clone()))
}
fn window(game:&Game<'_>)->u64 {u64::from(super::sunset_prediction::rtt(game).saturating_add(2).clamp(4,20))}
fn active(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && game.remote.is_some()
        && !game.spectator()
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
}

pub(super) fn reset() {
    state(|s| *s = State::default());
    HAS_MASKS.store(false, Ordering::Release);
}
fn known_patch(patch: &[u8]) -> bool {
    use horizon_protocol::{buf::Reader, item::component as c};
    let mut r = Reader::new(patch);
    let Ok(added) = r.count(64) else {
        return false;
    };
    if r.count(64).ok() != Some(0) {
        return false;
    }
    for _ in 0..added {
        let Ok(kind) = r.varint() else {
            return false;
        };
        if !matches!(
            kind,
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
fn nonnegative(effect: &serde_json::Value, level: u32, depth: u8) -> bool {
    if depth > 4 {
        return false;
    }
    match effect["type"].as_str() {
        Some("minecraft:add") => {
            let n = horizon_client::enchant::level_value(&effect["value"], level);
            n.is_finite() && n >= 0.0
        }
        Some("minecraft:multiply") => {
            let n = horizon_client::enchant::level_value(&effect["factor"], level);
            n.is_finite() && n >= 1.0
        }
        Some("minecraft:all_of") => effect["effects"].as_array().is_some_and(|list| {
            list.len() <= 8 && list.iter().all(|e| nonnegative(e, level, depth + 1))
        }),
        _ => false,
    }
}
// A conservative lower base, without consuming the engine's enchantment RNG.
// Current effect strength is added when the entire recent history is checked.
fn hand_base(game: &Game<'_>, slot: usize) -> Option<f32> {
    let item = game.inventory.get(slot)?;
    let Some(item) = item else {
        return Some(1.0);
    };
    let info = game.items().get(&item.item)?;
    if item.count == 0
        || (info.max_damage > 0 && item.damage >= info.max_damage)
        || item.extra.as_ref().is_some_and(|e| !e.other.is_empty())
    {
        return None;
    }
    if item.extra.is_some() {
        let patch = item.wire_patch()?;
        if !known_patch(&patch) {
            return None;
        }
    }
    for (id, level) in item.enchantments() {
        let definition = game.stations().enchantments.get(id)?;
        if let Some(effects) = definition.effects.get("minecraft:damage") {
            let effects = effects.as_array()?;
            if effects.len() > 8 || !effects.iter().all(|e| nonnegative(&e["effect"], *level, 0)) {
                return None;
            }
        }
    }
    let base = 1.0 + info.attack_damage;
    (base.is_finite() && base >= 1.0).then_some(base)
}

pub(super) fn tick(game: &mut Game<'_>, tick: u64) {
    let Some(_m) = module() else {
        reset();
        return;
    };
    if !active(game) || game.crystals.len() > 512 {
        reset();
        return;
    }
    let entity_range=game.player.as_mut().and_then(|player|player.attribute("minecraft:entity_interaction_range").ok())
        .filter(|range|range.is_finite()&&*range>0.0&&*range<=64.0);
    let base = hand_base(game, game.selected);
    state(|s| {
        if s.session.as_ref().is_none_or(|old| !same(game, old)) || tick < s.tick {
            *s = State::default();
            s.session = session(game);
        }
        s.tick = tick;s.entity_range=entity_range;
        s.history.sample(tick, base);
        s.masks.expire_after_window(tick, window(game));
        HAS_MASKS.store(s.masks.count() > 0, Ordering::Release);
    });
}

// Call only after final packet filters accept the packet for forwarding. This
// module never constructs an Attack and never replaces native attack input.
// Mirrors the native swing/cooldown prerequisites for an observed hit.
// A ghost is a bounded local visual prediction; it never removes Game entities.
fn swinging_hand(game:&Game<'_>)->bool {
    let Some(stack)=game.held() else{return true;};
    let Some(info)=game.items().get(&stack.item) else{return false;};
    stack.count>0 && horizon_client::spear::Kinetic::of(&stack.item).is_none()
        && !stack.extra.as_ref().is_some_and(|extra|!extra.other.is_empty())
        && !game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(stack,info.use_cooldown.as_ref()))
}

pub(super) fn sent(game: &Game<'_>, packet: &S) {
    let Some(_m) = module() else {
        return;
    };
    if !active(game) || game.crystals.len() > 512 {
        return;
    }
    let connected = state(|s| s.session.as_ref().is_some_and(|old| same(game, old)));
    if !connected {
        return;
    }
    if let S::SetCarriedItem(p) = packet {
        let base = usize::try_from(p.slot)
            .ok()
            .filter(|s| *s < 9)
            .and_then(|slot| hand_base(game, slot));
        state(|s| s.history.sample(game.ticks, base));
        return;
    }
    let S::Attack(attack) = packet else {
        return;
    };
    let Ok(id) = u32::try_from(attack.id) else {
        return;
    };
    if game.selected>=9 || game.player.as_ref().is_none_or(|p|p.auto_spin_attack_ticks>0) || !swinging_hand(game) {
        return;
    }
    let Some(crystal) = game
        .crystals
        .iter()
        .find(|c| c.id == id && id != 0)
    else {
        return;
    };
    let position = [crystal.position.x, crystal.position.y, crystal.position.z];
    if !policy::valid(position)
        || game
            .crystals
            .iter()
            .filter(|c| [c.position.x, c.position.y, c.position.z] == position)
            .take(2)
            .count()
            != 1
    {
        return;
    }
    let Some(eye) = game.eye_position() else {
        return;
    };
    let lo = [position[0] - 1.0, position[1], position[2] - 1.0];
    let hi = [position[0] + 1.0, position[1] + 2.0, position[2] + 1.0];
    let Some(range)=state(|s|s.entity_range.filter(|_|s.tick==game.ticks&&s.session.as_ref().is_some_and(|old|same(game,old)))) else{return;};
    if policy::box_distance(eye,lo,hi).is_none_or(|distance|distance>range)
        || !game.border.contains(position[0],position[2]) {return;}
    // Only the final forwarded Attack reaches this observer. Re-aiming at
    // the crystal center would reject a legitimate first-box-contact hit.
    let enchanted=game.held().is_some_and(|stack|super::sunset_combat_macros::enchant_damage(game,stack));
    let current = hand_base(game, game.selected);
    let effect = game.effect_attack_bonus();
    state(|s| {
        s.history.sample(game.ticks, current);
        if (enchanted || s.history.positive_with_empty(game.ticks, window(game), effect))
            && s.masks.pending(id, position, game.ticks)
        {
            s.hidden_total = s.hidden_total.saturating_add(1);
            HAS_MASKS.store(true, Ordering::Release);
        }
    });
}

pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if matches!(packet,C::Login(_)|C::Respawn(_)|C::Disconnect(_)){reset();return;}
    if !state(|s| s.session.as_ref().is_some_and(|old| same(game, old))) {
        return;
    }
    if let C::RemoveEntities(p) = packet {
        state(|s| {
            for id in p.ids.iter().take(4096) {
                if let Ok(id) = u32::try_from(*id) {
                    if s.masks.confirm(id) {
                        s.confirmed = s.confirmed.saturating_add(1);
                    }
                }
                HAS_MASKS.store(s.masks.count() > 0, Ordering::Release);
            }
        });
    }
}

// Used by a single MODIFY_EXPRESSION_VALUE on native crystal distance culling.
// The original renderer's continue skips only this body and its beam. No
// authoritative entity is deleted and no native renderer is reimplemented.
// Object picking has the actual entity ID. Position-only rendering cannot
// distinguish a later co-located crystal from its already predicted neighbor.
pub(super) fn hidden_id(game:&Game<'_>,id:u32,position:[f64;3])->bool {
    if !HAS_MASKS.load(Ordering::Acquire)||!active(game)||game.crystals.len()>512
        || !sunset::with(|s|s.module(NAME).is_some_and(|m|m.on)){return false;}
    state(|s|s.session.as_ref().is_some_and(|session|same(game,session))
        && s.masks.visible_id(id,position,game.ticks))
}
pub(super) fn hidden_position(game: &Game<'_>, position: [f64; 3]) -> bool {
    if !HAS_MASKS.load(Ordering::Acquire) || !active(game) || game.crystals.len() > 512 {
        return false;
    }
    let Some(id) = state(|s| {
        s.session
            .as_ref()
            .filter(|old| same(game, old))
            .and_then(|_| s.masks.visible_at(position,game.ticks))
    }) else {
        return false;
    };
    if !sunset::with(|s| s.module(NAME).is_some_and(|m| m.on)) {
        return false;
    }
    let mut matches = game
        .crystals
        .iter()
        .filter(|c| [c.position.x, c.position.y, c.position.z] == position);
    let Some(crystal) = matches.next() else {
        return false;
    };
    if matches.next().is_some() {
        return false;
    }
    crystal.id == id
}
pub(super) fn has_predictions(game:&Game<'_>)->bool {
    HAS_MASKS.load(Ordering::Acquire) && active(game) && module().is_some()
        && state(|s|s.session.as_ref().is_some_and(|session|same(game,session)))
}
pub(super) fn duplicate(game: &Game<'_>, packet: &S) -> bool {
    let S::Attack(p) = packet else {
        return false;
    };
    if !HAS_MASKS.load(Ordering::Acquire) || game.crystals.len() > 512 {
        return false;
    }
    let Ok(id) = u32::try_from(p.id) else {
        return false;
    };
    let Some(crystal) = game.crystals.iter().find(|c| c.id == id) else {
        return false;
    };
    hidden_position(
        game,
        [crystal.position.x, crystal.position.y, crystal.position.z],
    )
}
pub(super) fn debug() -> serde_json::Value {
    state(|s| {
        serde_json::json!({"authoritative_entities_retained":true,"native_hitboxes_filtered":true,
    "hidden":s.masks.count(),"stubborn":s.masks.stubborn(),"total_hidden":s.hidden_total,"server_remove_events":s.confirmed})
    })
}
