// One tick's classification, shared by combat paths. Network lifecycle hooks
// invalidate identities when the world actually applies a spawn or removal.
use super::sunset::Module;
use super::sunset_antibot_state::{self as state, Class, Observation, Sensitivity, Tracker};
pub(super) use state::Token;
use super::{Dim, Game};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

type Session = Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>;
struct State {
    tracker: Tracker,
    session: Option<Session>,
    dim: Option<Dim>,
    entity: Option<i32>,
    changed: bool,
    active: bool,
    start: Instant,
    uuids: HashMap<u128, u16>,
    names: HashMap<u64, u16>,
    modes: HashMap<u128, i32>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            tracker: Tracker::default(), session: None, dim: None, entity: None,
            changed: false, active: false,
            start: Instant::now(), uuids: HashMap::new(), names: HashMap::new(), modes: HashMap::new(),
        }
    }
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}
fn sensitivity(module: &Module) -> Sensitivity {
    match module.p("Anti bot sensitivity") {
        "Conservative" => Sensitivity::Conservative, "Sensitive" => Sensitivity::Sensitive, _ => Sensitivity::Balanced,
    }
}
fn bind(game: &Game<'_>, s: &mut State) {
    let active = game.native_on(super::sunset::ID) && game.player.is_some() && game.remote.is_some();
    let same_session = match (&s.session, &game.remote) {
        (Some(old), Some(remote)) => Arc::ptr_eq(old, &remote.session.counters),
        (None, None) => true,
        _ => false,
    };
    let entity = game.remote.as_ref().map(|r| r.entity_id);
    if !same_session || s.dim != Some(game.dim) || s.entity != entity || s.active != active {
        s.tracker.clear();
        if !same_session { s.modes.clear(); }
        s.session = game.remote.as_ref().map(|r| r.session.counters.clone());
        s.dim = Some(game.dim);
        s.entity = entity;
        s.active = active;
        s.changed = true;
    }
}
// O(1), including on the title screen. No entity scans in the frame path.
pub(super) fn frame(game: &mut Game<'_>) {
    let changed = with(|s| { bind(game, s); std::mem::take(&mut s.changed) });
    if changed { super::sunset::reset_targets(game); }
}
pub(super) fn world_changed() {
    with(|s| { s.tracker.clear(); s.changed = true; });
}
// TabPlayer::new defaults mode to survival even when no mode was sent. Keep
// explicit presence, then require it to agree with the applied engine state.
// The poll tap may run before world application; it cannot establish position
// or spawn lifetime, and a missing/overflowed update is treated as unknown.
pub(super) fn packets(game: &Game<'_>, packets: &[horizon_protocol::packets::play::Clientbound]) {
    use horizon_protocol::packets::play::Clientbound as C;
    with(|s| {
        bind(game, s);
        for packet in packets {
            match packet {
                C::Login(_) | C::Disconnect(_) => s.modes.clear(),
                C::PlayerInfoRemove(p) => { for id in &p.uuids { s.modes.remove(id); } }
                C::PlayerInfoUpdate(p) => for entry in &p.entries {
                    if let Some(mode) = entry.game_mode
                        && (s.modes.len() < 4096 || s.modes.contains_key(&entry.uuid))
                    { s.modes.insert(entry.uuid, mode); }
                },
                _ => {}
            }
        }
    });
}
pub(super) fn invalidate(game: &Game<'_>, id: i32) {
    with(|s| {
        bind(game, s);
        s.tracker.invalidate(id as u32, s.start.elapsed().as_secs_f64());
    });
}
fn name_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b.to_ascii_lowercase())).wrapping_mul(0x100_0000_01b3))
}
fn invalid_name(name: &str) -> bool {
    let core = name.strip_prefix(['.', '*']).unwrap_or(name);
    !(3..=16).contains(&core.len()) || !core.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
// Run before deferred attacks, at most one sample per entity per 40 ms. The
// tracker uses monotonic time, independent of Timer and the world's tick rate.
pub(super) fn tick(game: &mut Game<'_>) {
    frame(game);
    let Some(local) = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]) else { return; };
    with(|s| {
        s.uuids.clear();
        s.names.clear();
        for mob in game.mobs.iter().filter(|m| m.kind.id == "minecraft:player" && m.alive() && !m.removed) {
            if let Some(uuid) = super::remote::sunset_combat_remote::player_uuid(game, mob.id) {
                *s.uuids.entry(uuid).or_default() += 1;
            }
            if let Some(name) = game.remote_player_name(mob.id) {
                *s.names.entry(name_hash(name)).or_default() += 1;
            }
        }
        let now = s.start.elapsed().as_secs_f64();
        let own = s.entity;
        for mob in game.mobs.iter().filter(|m| m.alive() && !m.removed && Some(m.id as i32) != own) {
            let uuid = game.persist.uuids.get(&mob.id).map(|parts| horizon_client::lan::playerdata::ints_uuid(*parts));
            s.tracker.identify(now, mob.id, uuid, mob.age);
            let position = super::remote::sunset_combat_remote::entity_position(game, mob.id)
                .unwrap_or([mob.position.x, mob.position.y, mob.position.z]);
            if position.iter().chain(local.iter()).any(|v| !v.is_finite())
                || super::sunset_util::dist(position, local) > 32.0
            { continue; }
            let player = mob.kind.id == "minecraft:player";
            let name = game.remote_player_name(mob.id);
            let b = mob.bounds();
            let (lo, hi) = (b.min_position(), b.max_position());
            let mode = super::remote::sunset_combat_remote::player_mode(game, mob.id);
            let mode = uuid.and_then(|id| s.modes.get(&id).copied()).filter(|known| Some(*known) == mode);
            let duplicate = player && (uuid.is_some_and(|u| s.uuids.get(&u).copied().unwrap_or(0) > 1)
                || name.is_some_and(|n| s.names.get(&name_hash(n)).copied().unwrap_or(0) > 1));
            s.tracker.observe(now, Observation {
                id: mob.id, uuid, player, info: super::remote::sunset_combat_remote::player_has_info(game, mob.id),
                invalid_name: name.is_some_and(invalid_name), duplicate,
                mode,
                position, local, local_yaw: f64::from(game.yaw),
                scale: f64::from(mob.extra.model_scale), size: [hi.x - lo.x, hi.y - lo.y, hi.z - lo.z],
                on_ground: mob.on_ground, flying: mob.extra.fall_flying,
                age: mob.age, invisible: mob.invisible,
            }, Sensitivity::Balanced);
        }
        s.tracker.finish(now);
    });
}
pub(super) fn token(id: u32) -> Option<Token> { with(|s| s.tracker.token(id)) }
pub(super) fn current(token: Token) -> bool { with(|s| s.tracker.current(token)) }
pub(super) fn blocked(id: u32) -> bool {
    with(|s| s.tracker.decision(id).class == Class::Rejected)
}
pub(super) fn blocked_for(id: u32, module: &Module) -> bool {
    if !module.b("Anti bot") { return false; }
    match sensitivity(module) {
        Sensitivity::Balanced => blocked(id),
        sensitivity => with(|s| s.tracker.decision_for(id, sensitivity).class == Class::Rejected),
    }
}
pub(super) fn diagnostics(game: &Game<'_>, id: Option<u32>) -> Vec<String> {
    let local = game.eye_position().unwrap_or([0.0; 3]);
    let targets = (super::sunset_aura::target_id(), super::sunset_combat::aim_target_id(), super::sunset_click_fix::pending_target_id());
    let policies = super::sunset::with(|s| ["Kill Aura", "Aim Assist", "Trigger Bot"].map(|name| {
        s.module(name).map(|m| (name, m.b("Anti bot"), sensitivity(m)))
    }));
    with(|s| {
        let mut out = vec!["AntiBot is configured inside each combat module; confidence is evidence, not a measured probability.".to_owned()];
        for (name, enabled, sensitivity) in policies.into_iter().flatten() {
            out.push(format!("{name}: Anti bot={enabled}, sensitivity={sensitivity:?}"));
        }
        out.push(format!("Targets aura={:?} aim={:?} pending={:?}", targets.0, targets.1, targets.2));
        for mob in game.mobs.iter().filter(|m| id.map_or_else(
            || super::sunset_util::dist(local, [m.position.x, m.position.y, m.position.z]) < 12.0,
            |id| id == m.id,
        )).take(8) {
            let d = s.tracker.decision(mob.id);
            let reasons = [
                (state::NAME, "unusual name"), (state::INFO, "missing current player-info"),
                (state::DUPLICATE, "duplicate identity"), (state::SHAPE, "unusual scale/bounds"),
                (state::INVISIBLE, "invisible"), (state::ORBIT, "persistent orbit"),
                (state::TETHER, "look-relative tether"), (state::HOVER, "sustained hover"),
                (state::JUMPS, "repeated position jumps"), (state::CHURN, "nearby short-lived spawns"),
            ].into_iter().filter_map(|(bit, label)| (d.reasons & bit != 0).then_some(label)).collect::<Vec<_>>().join(", ");
            let mode = super::remote::sunset_combat_remote::player_mode(game, mob.id);
            let mode_known = super::remote::sunset_combat_remote::player_uuid(game, mob.id)
                .and_then(|uuid| s.modes.get(&uuid)).is_some_and(|known| Some(*known) == mode);
            out.push(format!("id={} {} {:?} observed={} confidence={} scale={:.3} info={} mode={:?} known={}: {}",
                mob.id, game.remote_player_name(mob.id).unwrap_or(&mob.kind.id), d.class, d.observed, d.confidence, mob.extra.model_scale,
                super::remote::sunset_combat_remote::player_has_info(game, mob.id),
                mode, mode_known,
                if reasons.is_empty() { "no corroborated decoy evidence" } else { &reasons }));
        }
        out
    })
}

#[cfg(test)]
mod mode_tests {
    use super::*;
    #[test]
    fn world_reset_retains_current_player_info_presence() {
        with(|s| { s.modes.insert(42, 0); });
        world_changed();
        assert_eq!(with(|s| s.modes.get(&42).copied()), Some(0));
        with(|s| { s.modes.clear(); });
    }
}
