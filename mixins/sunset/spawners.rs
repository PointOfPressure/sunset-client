// Native spawner metadata, bounded discovery and projected labels.
use super::Game;
use super::sunset::{Camera, Canvas, Category, Module, say};
use horizon_client::chunk_map::{sunset_spawner_data as data, sunset_spawner_index as index};
use horizon_client::containers::BlockEntity;
use horizon_client::mobs::spawner::SpawnerExt;
use horizon_world::nbt::{JavaString, Tag};
use std::fmt::Write as _;
use std::sync::OnceLock;
use std::sync::Mutex;

#[derive(Default)]
struct Session {
    tracker: data::Tracker,
    // SpawnInfo.dimension is the actual dimension identifier, including custom
    // dimensions sharing a vanilla dimension type.
    dimension: Option<String>,
    candidates: Vec<data::Pos>,
    draw_text: String,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new("Spawner Alerts", Category::World,
        "Reports newly discovered loaded spawners with coordinates, type and distance.")
        .num("Range", 64.0, 8.0, 256.0, 8.0)
        .bool("Chat", true)
        .bool("Labels", true)
        .num("Max labels", 8.0, 1.0, 32.0, 1.0)]
}

pub(super) fn reset() {
    *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub(super) fn dimension(id: &str) {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    guard.get_or_insert_with(Session::default).dimension = Some(id.to_owned());
}

// Both the actual block state and the current block entity must agree. An
// unloaded or replaced block cannot keep a stale label or produce an alert.
fn valid_entity<'g>(game: &'g Game<'_>, pos: data::Pos) -> Option<&'g BlockEntity> {
    let state = game.world.chunks.get(&(pos.0 >> 4, pos.2 >> 4))?
        .block(pos.0 & 15, pos.1, pos.2 & 15);
    let facts = game.res.facts();
    let name = facts.get(state).name.as_ref();
    let entity = game.world.block_entities.get(&pos)?;
    match entity {
        BlockEntity::Spawner(_) if name == "minecraft:spawner" => Some(entity),
        BlockEntity::TrialSpawner(_) if name == "minecraft:trial_spawner" => Some(entity),
        _ => None,
    }
}

fn metadata(entity: &BlockEntity) -> (Option<&[u16]>, bool) {
    let (spawn, trial) = match entity {
        BlockEntity::Spawner(s) => (s.next_data(), false),
        BlockEntity::TrialSpawner(s) => (s.next_spawn_data.as_ref(), true),
        _ => (None, false),
    };
    // Native NBT stores UTF-16. Borrow it directly; decode only when a cached
    // label's identifier changes. The static lookup key allocates just once.
    static ID: OnceLock<JavaString> = OnceLock::new();
    let id = spawn.and_then(|data| data.entity.get(ID.get_or_init(|| "id".into())));
    let id = match id {
        Some(Tag::String(id)) if id.0.len() <= 128 => Some(id.0.as_slice()),
        _ => None,
    };
    (id, trial)
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module], ticks: u64) {
    let Some(player) = game.player.as_ref() else {
        if game.remote.is_none() { reset(); }
        return;
    };
    let at = [player.position.x, player.position.y, player.position.z];
    let module = mods.iter().find(|m| m.name == "Spawner Alerts" && m.on);
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let session = guard.get_or_insert_with(Session::default);
    session.tracker.context(session.dimension.as_deref().unwrap_or(game.dim.id()), module.is_some());
    let Some(module) = module else { return; };
    let tracker = &mut session.tracker;
    let saturated = index::scan(&game.world.chunks, tracker.cursor, &mut session.candidates);
    let end = session.candidates.len() < data::SCAN_BUDGET;
    for pos in session.candidates.drain(..) {
        tracker.cursor = Some(pos);
        let Some(entity) = valid_entity(game, pos) else {
            index::remove(&mut game.world.chunks, pos);
            continue;
        };
        if data::within(pos, at, module.n("Range")) {
            let distance = data::distance_squared(pos, at).sqrt();
            if tracker.needs(pos, distance) {
                let (mob, trial) = metadata(entity);
                tracker.observe(data::Found::new(pos, mob, trial, distance), module.b("Chat"));
            }
        }
    }
    if end { tracker.finish_pass(); }
    if !module.b("Chat") { tracker.silence(); }
    let message = module.b("Chat").then(|| tracker.chat(ticks)).flatten();
    let warning = (saturated || tracker.seen_saturated) && !tracker.warned;
    tracker.warned |= warning;
    drop(guard);
    if let Some(message) = message { say(game, &message); }
    if warning {
        say(game, "Spawner Alerts capacity reached (16,384 indexed / 65,536 discoveries); additional spawners may be omitted until the world or connection is reloaded.");
    }
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas) {
    let Some(module) = mods.iter().find(|m| m.name == "Spawner Alerts" && m.on && m.b("Labels")) else { return; };
    let Some(player) = game.player.as_ref() else { return; };
    let at = [player.position.x, player.position.y, player.position.z];
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let Some(session) = guard.as_mut() else { return; };
    let mut drawn = 0;
    // The last complete scan's nearest 32, never the historic discovery set.
    let mut text = std::mem::take(&mut session.draw_text);
    for found in &mut session.tracker.labels {
        let pos = found.pos;
        if !data::within(pos, at, module.n("Range")) { continue; }
        let point = [f64::from(pos.0) + 0.5, f64::from(pos.1) + 1.25, f64::from(pos.2) + 0.5];
        if !cam.sees(point, 0.0) { continue; }
        let Some(projected) = cam.project(point).filter(|p| cam.on_screen(*p, 0.0)) else { continue; };
        let Some(entity) = valid_entity(game, pos) else { continue; };
        let (mob, trial) = metadata(entity);
        found.refresh(mob, trial);
        text.clear();
        let _ = write!(text, "{} ({:.0}m)", found.name, data::distance_squared(pos, at).sqrt());
        let width = game.native_text_width(&text);
        out.text(&text, [projected[0] - width / 2.0, projected[1] - 4.0], 0xFFFFFF);
        drawn += 1;
        if drawn >= module.n("Max labels") as usize { break; }
    }
    session.draw_text = text;
}
