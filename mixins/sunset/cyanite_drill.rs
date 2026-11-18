// Observes a drill's local action and nearby server-confirmed removals.
use super::Game;
use super::sunset::{Category, Module, with, say};
use horizon_protocol::packets::play::{Clientbound as C, Serverbound as S};
use std::collections::BTreeSet;
use std::io::Write;
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new("Drill Logger", Category::World, "Records nearby block removals after each pickaxe swing.")
        .num("Radius", 4.0, 1.0, 8.0, 1.0)
        .num("Observe ticks", 10.0, 2.0, 40.0, 1.0)
        .bool("Notify", true)]
}

struct Sample {
    pos: [i32; 3],
    yaw: f32,
    pitch: f32,
    at: u64,
    until: u64,
    radius: i32,
    solid: BTreeSet<[i32; 3]>,
    removed: BTreeSet<[i32; 3]>,
}
#[derive(Default)]
struct State {
    world: Option<(u64, &'static str)>,
    samples: Vec<Sample>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

pub(super) fn sent(game: &Game<'_>, packet: &S) {
    let S::PlayerAction(action) = packet else { return };
    if action.action != 0 || !game.native_window() || !game.native_on(super::sunset::ID) { return; }
    let Some(m) = with(|s| s.module("Drill Logger").filter(|m| m.on).cloned()) else { return };
    if !game.held().is_some_and(|s| s.item.ends_with("_pickaxe")) { return; }
    let radius = m.n("Radius") as i32;
    let mut solid = BTreeSet::new();
    for x in -radius..=radius {
        for y in -radius..=radius {
            for z in -radius..=radius {
                let pos = [action.pos[0] + x, action.pos[1] + y, action.pos[2] + z];
                if game.world.chunks.contains_key(&(pos[0] >> 4, pos[2] >> 4))
                    && !game.res.facts().is_air(game.world.block(pos[0], pos[1], pos[2]))
                {
                    solid.insert(pos);
                }
            }
        }
    }
    state(|s| {
        if s.samples.len() < 32 {
            s.samples.push(Sample { pos: action.pos, yaw: game.yaw, pitch: game.pitch,
                at: game.ticks, until: game.ticks + m.n("Observe ticks") as u64,
                radius, solid, removed: BTreeSet::new() });
        }
    });
}

fn removed(game: &Game<'_>, pos: [i32; 3], id: i32) {
    let Ok(id) = u32::try_from(id) else { return };
    if !game.res.facts().is_air(horizon_world::registry::BlockStateId(id)) { return; }
    state(|s| {
        for sample in &mut s.samples {
            if game.ticks <= sample.until && sample.solid.contains(&pos)
                && (0..3).all(|i| (pos[i] - sample.pos[i]).abs() <= sample.radius)
            {
                sample.removed.insert(pos);
            }
        }
    });
}

pub(super) fn shutdown() {
    state(|s| *s = State::default());
}

pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if !game.native_window() || !game.native_on(super::sunset::ID)
        || !with(|s| s.module("Drill Logger").is_some_and(|m| m.on))
    { return; }
    match packet {
        C::BlockUpdate(p) => removed(game, p.pos, p.state),
        C::SectionBlocksUpdate(p) => {
            let chunk = p.section;
            for &change in p.changes.iter().take(4096) {
                let v = change as u64;
                let pos = [chunk[0] * 16 + ((v >> 8) & 15) as i32,
                    chunk[1] * 16 + (v & 15) as i32, chunk[2] * 16 + ((v >> 4) & 15) as i32];
                removed(game, pos, (v >> 12) as i32);
            }
        }
        _ => {}
    }
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let world = super::sunset_cyanite_utilities::world_key(game).map(|w| (w, game.dim.id()));
    let module = mods.iter().find(|m| m.name == "Drill Logger" && m.on);
    if module.is_none() || state(|s| s.world != world) {
        state(|s| *s = State { world, ..State::default() });
        return;
    }
    let complete = state(|s| {
        let mut complete = Vec::new();
        let mut i = 0;
        while i < s.samples.len() {
            if game.ticks >= s.samples[i].until { complete.push(s.samples.remove(i)); } else { i += 1; }
        }
        complete
    });
    for sample in complete {
        let offsets: Vec<_> = sample.removed.iter().map(|p| std::array::from_fn::<_, 3, _>(|i| p[i] - sample.pos[i])).collect();
        let record = serde_json::json!({"at":chrono::Local::now().to_rfc3339(), "tick":sample.at,
            "dimension":game.dim.id(), "origin":sample.pos, "yaw":sample.yaw, "pitch":sample.pitch,
            "observed_removals":offsets});
        let dir = game.native_data_dir(super::sunset::ID).join("drill");
        let path = dir.join(format!("{:016x}.jsonl", world.map_or(0, |w| w.0)));
        let result = std::fs::create_dir_all(dir).and_then(|()| {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
            writeln!(file, "{record}")
        });
        match result {
            Ok(()) if module.is_some_and(|m| m.b("Notify")) =>
                say(game, &format!("Drill Logger: {} nearby removals recorded.", sample.removed.len())),
            Err(error) => say(game, &format!("Drill Logger could not save: {error}")),
            _ => {}
        }
    }
}
