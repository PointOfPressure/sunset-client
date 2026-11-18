// Test-only incoming entities. This module is staged with the probe, never with Sunset releases.
use super::{Game, C, play};
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};

static PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
static STATE: Mutex<Option<Fixture>> = Mutex::new(None);

#[derive(Clone)]
struct Entity {
    id: i32,
    generation: u64,
    data: Value,
    origin: [f64; 3],
    started: u64,
}
impl Entity {
    fn uuid(&self) -> u128 {
        self.data["uuid"].as_str().and_then(|v| v.parse().ok())
            .unwrap_or((u128::from(self.generation) << 64) | self.id as u128)
    }
    fn number(&self, key: &str, default: f64) -> f64 {
        self.data[key].as_f64().unwrap_or(default)
    }
    fn motion(&self) -> &str { self.data["motion"].as_str().unwrap_or("plain") }
    fn position(&self, game: &Game<'_>) -> [f64; 3] {
        let mut at = self.origin;
        if self.motion() == "orbit" {
            let a = game.ticks.saturating_sub(self.started) as f64 * 0.25;
            let local = game.player.as_ref().map_or(self.origin, |p| [p.position.x, p.position.y, p.position.z]);
            return [local[0] + a.sin() * 2.0, local[1] + 1.4, local[2] + a.cos() * 2.0];
        }
        at[0] += self.number("x", 0.0);
        at[1] += self.number("y", 0.0);
        at[2] += self.number("z", 2.0);
        at
    }
}
#[derive(Default)]
struct Fixture {
    command: String,
    revision: u64,
    entities: Vec<Entity>,
    error: Option<String>,
    replacements: u32,
    evasions: u32,
}

fn packet(game: &mut Game<'_>, packet: C, errors: &mut Option<String>) {
    crate::game::sunset_antibot::packets(game, std::slice::from_ref(&packet));
    if let Err(error) = game.handle_packet(packet) { *errors = Some(error.to_string()); }
}

fn remove(game: &mut Game<'_>, entity: &Entity, errors: &mut Option<String>) {
    packet(game, C::RemoveEntities(play::RemoveEntities { ids: vec![entity.id] }), errors);
    packet(game, C::PlayerInfoRemove(play::PlayerInfoRemove { uuids: vec![entity.uuid()] }), errors);
}

fn presentation(game: &mut Game<'_>, entity: &Entity, errors: &mut Option<String>) {
    let info = play::PlayerInfo {
        uuid: entity.uuid(), name: Some(entity.data["name"].as_str().unwrap_or("FixturePlayer").to_owned()),
        game_mode: Some(entity.data["mode"].as_i64().unwrap_or(0) as i32),
        listed: Some(entity.data["listed"].as_bool().unwrap_or(true)), latency: Some(35),
        ..play::PlayerInfo::default()
    };
    packet(game, C::PlayerInfoUpdate(play::PlayerInfoUpdate { actions: 1 | 4 | 8 | 16, entries: vec![info] }), errors);
    let flags = if entity.data["invisible"].as_bool().unwrap_or(false) { 0x20 } else { 0 };
    packet(game, C::SetEntityData(play::SetEntityData {
        id: entity.id, values: vec![(0, 0, horizon_protocol::entity_data::Value::Byte(flags))],
    }), errors);
    if let Some(attribute) = game.motion.attributes.id("minecraft:scale") {
        packet(game, C::UpdateAttributes(play::UpdateAttributes {
            id: entity.id, values: vec![play::AttributeSnapshot {
                attribute: attribute.registry_id(), base: entity.number("scale", 1.0), modifiers: vec![],
            }],
        }), errors);
    }
}

fn spawn(game: &mut Game<'_>, entity: &Entity, errors: &mut Option<String>) {
    let kind = game.remote.as_ref().and_then(|r| r.builtin.get("minecraft:entity_type"))
        .and_then(|registry| registry.id("minecraft:player"));
    if let Some(kind) = kind {
        presentation(game, entity, errors);
        packet(game, C::AddEntity(play::AddEntity {
            id: entity.id, uuid: entity.uuid(), kind, position: entity.position(game),
            velocity: [0.0; 3], pitch: 0, yaw: 0, head_yaw: 0, data: 0,
        }), errors);
    } else { *errors = Some("player entity type unavailable".to_owned()); }
}

// Replace an armed entity after a look was queued but before its deed runs.
// This exercises actual spawn hooks and final validation, not a fake queue.
pub(super) fn replace_on_turn(game: &mut Game<'_>) {
    if !crate::game::sunset::turn_pending() { return; }
    let Ok(mut lock) = STATE.lock() else { return; };
    let Some(state) = lock.as_mut() else { return; };
    let Some(index) = state.entities.iter().position(|e| e.data["replace_on_turn"].as_bool() == Some(true)) else { return; };
    let mut entity = state.entities[index].clone();
    remove(game, &entity, &mut state.error);
    entity.generation += 1;
    entity.data["replace_on_turn"] = json!(false);
    spawn(game, &entity, &mut state.error);
    state.entities[index] = entity;
    state.replacements += 1;
}

pub(crate) fn evade_pending(game: &mut Game<'_>) {
    if !crate::game::sunset::strike_waiting() { return; }
    let Ok(mut lock) = STATE.lock() else { return; };
    let Some(state) = lock.as_mut() else { return; };
    for entity in &mut state.entities {
        if entity.data["evade_pending"].as_bool() != Some(true) { continue; }
        entity.data["evade_pending"] = json!(false);
        entity.data["z"] = json!(100.0);
        state.evasions += 1;
        packet(game, C::EntityPositionSync(play::EntityPositionSync {
            id: entity.id, path: play::PositionPath::Linear(entity.position(game)), yaw: 0.0, pitch: 0.0, on_ground: true,
        }), &mut state.error);
    }
}

pub(super) fn tick(game: &mut Game<'_>) {
    let Some(path) = PATH.get_or_init(|| std::env::var_os("SUNSET_ANTIBOT_FIXTURE").map(Into::into)) else { return; };
    if game.player.is_none() || game.remote.is_none() { return; }
    let Ok(mut lock) = STATE.lock() else { return; };
    let state = lock.get_or_insert_with(Fixture::default);
    if let Ok(command) = std::fs::read_to_string(path)
        && command != state.command
        && let Ok(value) = serde_json::from_str::<Value>(&command)
        && let Some(entries) = value["entities"].as_array()
    {
        let p = game.player.as_ref().unwrap().position;
        let mut next = Vec::new();
        state.error = None;
        for data in entries {
            let Some(id) = data["id"].as_i64().and_then(|id| i32::try_from(id).ok()).filter(|id| *id > 0) else { continue; };
            let generation = data["generation"].as_u64().unwrap_or(1);
            let old = state.entities.iter().find(|e| e.id == id && e.generation == generation);
            next.push(Entity {
                id, generation, data: data.clone(), origin: old.map_or([p.x, p.y, p.z], |e| e.origin),
                started: old.map_or(game.ticks, |e| e.started),
            });
        }
        for old in &state.entities {
            if !next.iter().any(|e| e.id == old.id && e.generation == old.generation) {
                remove(game, old, &mut state.error);
            }
        }
        for entity in &next {
            if !state.entities.iter().any(|e| e.id == entity.id && e.generation == entity.generation) {
                spawn(game, entity, &mut state.error);
            }
            presentation(game, entity, &mut state.error);
        }
        state.entities = next;
        if value["world_reset"].as_bool() == Some(true) { crate::game::sunset_antibot::world_changed(); }
        state.command = command;
        state.revision = value["revision"].as_u64().unwrap_or(0);
    }
    for entity in &mut state.entities {
        packet(game, C::EntityPositionSync(play::EntityPositionSync {
            id: entity.id, path: play::PositionPath::Linear(entity.position(game)), yaw: 0.0, pitch: 0.0,
            on_ground: entity.motion() == "plain" || entity.data["grounded"].as_bool().unwrap_or(false),
        }), &mut state.error);
    }
    let entities: Vec<_> = state.entities.iter().map(|e| json!({
        "id": e.id,
        "blocked": crate::game::sunset_antibot::blocked(e.id as u32),
        "token": format!("{:?}", crate::game::sunset_antibot::token(e.id as u32)),
        "diagnostics": crate::game::sunset_antibot::diagnostics(game, Some(e.id as u32)),
    })).collect();
    let status = json!({
        "revision": state.revision, "ticks": game.ticks, "error": state.error,
        "aura": crate::game::sunset_aura::target_id(), "aim": crate::game::sunset_combat::aim_target_id(),
        "pending": crate::game::sunset_click_fix::pending_target_id(), "replacements": state.replacements, "evasions": state.evasions,
        "entities": entities,
    });
    let mut output = path.as_os_str().to_os_string();
    output.push(".state");
    let _ = std::fs::write(std::path::PathBuf::from(output), status.to_string());
}
