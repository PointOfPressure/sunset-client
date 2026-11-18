// Server-advertised block entities can outlive a server's visual block substitution.
// These records extend Storage ESP without changing world blocks or collision.
use super::Game;
use super::sunset::{Camera, Canvas, Module};
use super::sunset_util::{center_of, dist, listed};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct State {
    world: Option<(u64, &'static str)>,
    records: HashMap<[i32; 3], String>,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

fn world(game: &Game<'_>) -> Option<(u64, &'static str)> {
    super::sunset_cyanite_utilities::world_key(game).map(|w| (w, game.dim.id()))
}

fn same_world(game: &Game<'_>, s: &mut State) {
    let world = world(game);
    if s.world != world { *s = State { world, ..State::default() }; }
}

pub(super) fn chunk(game: &Game<'_>, x: i32, z: i32) {
    state(|s| {
        same_world(game, s);
        s.records.retain(|p, _| (p[0] >> 4, p[2] >> 4) != (x, z));
    });
}

pub(super) fn record(game: &Game<'_>, pos: [i32; 3], kind: i32) {
    if !game.native_window() || !game.native_on(super::sunset::ID) { return; }
    let Some(name) = game.remote.as_ref()
        .and_then(|r| r.builtin.get("minecraft:block_entity_type")?.name(kind)) else { return };
    state(|s| {
        same_world(game, s);
        if s.records.len() < 65_536 || s.records.contains_key(&pos) {
            s.records.insert(pos, name.to_owned());
        }
    });
}

pub(super) fn tick(game: &Game<'_>) {
    state(|s| {
        same_world(game, s);
        if game.ticks % 20 == 0 {
            s.records.retain(|pos, _| game.world.chunks.contains_key(&(pos[0] >> 4, pos[2] >> 4)));
        }
    });
}

// A subsequent authoritative block change retires the old metadata at that cell.
// A new block-entity packet, if any, will advertise its new identity separately.
pub(super) fn packet(game: &Game<'_>, packet: &horizon_protocol::packets::play::Clientbound) {
    use horizon_protocol::packets::play::Clientbound as C;
    if !game.native_window() { return; }
    state(|s| {
        same_world(game, s);
        if s.records.is_empty() { return; }
        match packet {
            C::BlockUpdate(p) => { s.records.remove(&p.pos); }
            C::SectionBlocksUpdate(p) => {
                for &change in p.changes.iter().take(4096) {
                    let v = change as u64;
                    s.records.remove(&[p.section[0] * 16 + ((v >> 8) & 15) as i32,
                        p.section[1] * 16 + (v & 15) as i32, p.section[2] * 16 + ((v >> 4) & 15) as i32]);
                }
            }
            _ => {}
        }
    });
}

pub(super) fn shutdown() {
    state(|s| *s = State::default());
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let Some(m) = mods.iter().find(|m| m.name == "Storage ESP" && m.on && m.b("Hidden metadata")) else { return };
    let records = state(|s| {
        if s.world == world(game) { s.records.iter().map(|(p, n)| (*p, n.clone())).collect::<Vec<_>>() }
        else { Vec::new() }
    });
    for (pos, name) in records {
        let at = center_of(pos);
        if dist(cam.pos, at) > m.n("Metadata range") { continue; }
        let state = game.world.block(pos[0], pos[1], pos[2]);
        if game.res.facts().is_air(state) { continue; }
        let shown = &game.res.facts().get(state).name;
        let spawner = name.ends_with("spawner");
        let storage = listed("chest,trapped_chest,barrel,shulker_box,ender_chest,hopper,furnace,blast_furnace,smoker,dispenser,dropper", &name);
        if !(spawner && m.b("Hidden spawners") || storage && m.b("Hidden storage")) { continue; }
        let same = shown.as_ref() == name
            || (spawner && shown.ends_with(":spawner"))
            || (name.ends_with(":chest") && listed("chest,trapped_chest", shown))
            || (name.ends_with(":shulker_box") && shown.ends_with("shulker_box"));
        if same { continue; }
        let lo = pos.map(f64::from);
        super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED,
            lo, lo.map(|v| v + 1.0), m.c("Metadata color"), lw);
        if m.b("Metadata labels") && let Some([x, y]) = cam.project(at) {
            let label = format!("{} metadata", super::sunset_util::short(&name));
            out.text(&label, [x - game.native_text_width(&label) / 2.0, y], m.c("Metadata color"));
        }
    }
}
