// New Chunks: which chunks the server has just generated and which were there before, the way Trouser-Streak's
// NewerNewChunks tells them apart (behaviour only, written fresh). The palette method reads the order of each section's
// palette as the server sent it: a section just generated starts as all air and gains its blocks one by one, so its
// palette begins with air whatever its first block is; a section read from a save was packed in the order its blocks
// come, so its palette begins with its first block. The liquid method: liquid already flowing when a chunk comes was
// there before; liquid starting to flow beside its source afterwards is a new chunk's first tick. New chunks show red
// and old ones green on a flat overlay, kept in the mod's data folder per world or server and per dimension. Horizon's
// own server (single player too) packs every palette in block order, so there only the liquid method can tell.
//
// Player Activity Detector (Trouser-Streak's OnlinePlayerActivityDetector): an old chunk whose palette still names
// blocks none of its blocks are any more has changed since the server loaded it, so it was kept loaded: a player is
// (or was just) near it. Chunks you had yourself within Recent visit don't count (you may have kept those loaded).

use super::sunset::{Camera, Canvas, Category, Module};
use super::sunset_util::*;
use super::Game;
use horizon_protocol::chunk::{Section, storage_get};
use horizon_world::registry::BlockStateId;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("New Chunks", Category::World, "Shows new and old chunks.")
            .bool("Palette method", true)
            .bool("Liquid method", true)
            .bool("Save", true)
            .bool("Show old", true)
            .num("Render distance", 32.0, 4.0, 128.0, 4.0)
            .num("Render height", 0.0, -64.0, 320.0, 1.0)
            .colour("New color", "Red")
            .colour("Old color", "Green")
            .num("Fill opacity", 0.25, 0.0, 1.0, 0.05),
        Module::new("Player Activity Detector", Category::World, "Finds chunks players keep loaded.")
            .num("Recent visit (s)", 60.0, 0.0, 600.0, 5.0)
            .bool("Chat", true)
            .colour("Color", "Purple"),
    ]
}

const NEW: u8 = 1;
const OLD: u8 = 2;

// Either module on: the hooks pass chunks and block changes on only then.
static ON: AtomicBool = AtomicBool::new(false);
// The game tick now, and Player Activity Detector's Recent visit in ticks: for the decoder thread.
static NOW: AtomicU64 = AtomicU64::new(0);
static RECENT: AtomicU64 = AtomicU64::new(0);

// A chunk packet as it came: its sections' bytes and how to read them.
struct Arrival {
    chunk: (i32, i32),
    buffer: Vec<u8>,
    sections: usize,
    biome_bits: u8,
    // Look for blocks gone from the palettes too: not had within Recent visit.
    stale: bool,
}

// What the thread made of a chunk: the palette method's answer (NEW, OLD or 0), liquid flowing in it, and how many
// sections name blocks no longer there.
struct Verdict {
    chunk: (i32, i32),
    palette: u8,
    flowing: bool,
    stale: u32,
}

#[derive(Default)]
struct Nc {
    // The world and dimension shown and the file they are kept in.
    key: String,
    file: Option<std::fs::File>,
    chunks: HashMap<(i32, i32), u8>,
    jobs: Option<Sender<Arrival>>,
    verdicts: Option<Receiver<Verdict>>,
    // Chunks with liquid that started to flow beside its source (from the block update hook).
    flows: Vec<(i32, i32)>,
    // When each chunk was last loaded here (in ticks), and those Player Activity Detector has found.
    had: HashMap<(i32, i32), u64>,
    active: Vec<(i32, i32)>,
}

static NC: Mutex<Option<Nc>> = Mutex::new(None);

fn nc<R>(f: impl FnOnce(&mut Nc) -> R) -> R {
    f(NC.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Nc::default))
}

// From the chunk packet hook: the packet's sections, to the thread.
pub(crate) fn arrived(x: i32, z: i32, buffer: &[u8], sections: usize, biome_bits: u8) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let (now, recent) = (NOW.load(Ordering::Relaxed), RECENT.load(Ordering::Relaxed));
    nc(|n| {
        let stale = n.had.get(&(x, z)).is_none_or(|&t| now.saturating_sub(t) > recent);
        if let Some(jobs) = &n.jobs {
            let _ = jobs.send(Arrival { chunk: (x, z), buffer: buffer.to_vec(), sections, biome_bits, stale });
        }
    });
}

// From the block update hook: liquid that starts to flow beside a source.
pub(super) fn block_update(game: &Game<'_>, pos: [i32; 3], state: i32) {
    if !ON.load(Ordering::Relaxed) || state < 0 {
        return;
    }
    let facts = game.res.facts();
    let s = facts.get(BlockStateId(state as u32));
    if s.fluid_level == 0 || (s.fluid_level == 8 && !s.fluid_falling) {
        return;
    }
    let source = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, 0, 1], [0, 0, -1]].iter().any(|d| {
        let n = facts.get(game.world.block(pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]));
        n.fluid_level == 8 && !n.fluid_falling
    });
    if source {
        nc(|n| n.flows.push((pos[0] >> 4, pos[2] >> 4)));
    }
}

// On the thread: one chunk's sections read and judged.
fn judge(a: &Arrival, air: u32, flowing: &[bool]) -> Option<Verdict> {
    let mut r = horizon_protocol::buf::Reader::new(&a.buffer);
    let sections: Vec<Section> = (0..a.sections).map(|_| Section::read(&mut r, 16, a.biome_bits)).collect::<Result<_, _>>().ok()?;
    let (mut new, mut old, mut stale, mut flows) = (0u32, 0u32, 0u32, false);
    let is_flowing = |id: i32| usize::try_from(id).ok().and_then(|i| flowing.get(i)).copied().unwrap_or(false);
    for s in &sections {
        let p = &s.states;
        if p.bits == 0 {
            flows |= p.palette.first().is_some_and(|&id| is_flowing(id));
            continue;
        }
        if p.palette.is_empty() {
            // The global palette: no order to read, the liquid looked for block by block.
            flows |= (0..4096).any(|i| is_flowing(storage_get(&p.data, p.bits, i) as i32));
            continue;
        }
        flows |= p.palette.iter().any(|&id| is_flowing(id));
        let first = p.palette.get(storage_get(&p.data, p.bits, 0) as usize).copied().unwrap_or(0);
        if p.palette[0] == air as i32 && first != air as i32 {
            new += 1;
        } else if p.palette[0] != air as i32 {
            old += 1;
        }
        if a.stale {
            let mut used = vec![false; p.palette.len()];
            for i in 0..4096 {
                if let Some(u) = used.get_mut(storage_get(&p.data, p.bits, i) as usize) {
                    *u = true;
                }
            }
            stale += u32::from(used.contains(&false));
        }
    }
    let palette = match (new, old) {
        (0, 0) => 0,
        (n, o) if n >= o => NEW,
        _ => OLD,
    };
    Some(Verdict { chunk: a.chunk, palette, flowing: flows, stale })
}

// The file a world's and dimension's chunks are kept in.
fn path(game: &Game<'_>) -> Option<std::path::PathBuf> {
    let dim = short(game.dim.id()).replace(['/', ':'], "_");
    Some(game.native_data_dir(super::sunset::ID).join("newchunks").join(format!("{:016x}-{dim}.txt", world_hash(game)?)))
}

// "x z n" or "x z o" per line, a later line over an earlier.
fn load(path: &std::path::Path) -> HashMap<(i32, i32), u8> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let (x, z) = (w.next()?.parse().ok()?, w.next()?.parse().ok()?);
            let v = match w.next()? {
                "n" => NEW,
                "o" => OLD,
                _ => return None,
            };
            Some(((x, z), v))
        })
        .collect()
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let (chunks, activity) = (get("New Chunks"), get("Player Activity Detector"));
    let on = (chunks.is_some() || activity.is_some()) && game.remote.is_some();
    ON.store(on, Ordering::Relaxed);
    NOW.store(game.ticks, Ordering::Relaxed);
    RECENT.store(activity.map_or(0, |m| (m.n("Recent visit (s)") * 20.0) as u64), Ordering::Relaxed);
    if !on {
        nc(|n| *n = Nc::default());
        return;
    }
    // Another world or dimension (or Save switched): its own chunks, and a fresh thread.
    let save = chunks.is_some_and(|m| m.b("Save"));
    let file = path(game).filter(|_| save);
    let key = format!("{}|{}|{save}", world_hash(game).unwrap_or(0), game.dim.id());
    if nc(|n| n.key != key) {
        let loaded = file.as_deref().map(load).unwrap_or_default();
        let opened = file.and_then(|p| {
            std::fs::create_dir_all(p.parent()?).ok()?;
            std::fs::OpenOptions::new().create(true).append(true).open(p).ok()
        });
        let facts = game.res.facts();
        let air = facts.states.iter().position(|s| short(&s.name) == "air").unwrap_or(0) as u32;
        let flowing: Vec<bool> = facts.states.iter().map(|s| s.fluid_level > 0 && (s.fluid_level < 8 || s.fluid_falling)).collect();
        let (jobs, rx) = channel::<Arrival>();
        let (tx, verdicts) = channel();
        let thread = move || {
            for a in rx {
                if let Some(v) = judge(&a, air, &flowing) && tx.send(v).is_err() {
                    return;
                }
            }
        };
        let started = std::thread::Builder::new().name("sunset-new-chunks".into()).spawn(thread).is_ok();
        nc(|n| {
            *n = Nc { key, file: opened, chunks: loaded, ..Nc::default() };
            if started {
                n.jobs = Some(jobs);
                n.verdicts = Some(verdicts);
            }
        });
    }
    if game.ticks % 20 == 0 {
        let now = game.ticks;
        nc(|n| {
            for (c, _) in game.world.chunks.iter() {
                n.had.insert(*c, now);
            }
        });
    }
    // Horizon's own server packs every palette in block order, new chunks too, and sends new chunks with their liquid
    // already flowing: only liquid starting to flow tells there.
    let horizon = game.remote.as_ref().is_some_and(|r| r.brand.to_ascii_lowercase().contains("horizon"));
    let (palette, liquid) = chunks.map_or((true, false), |m| (m.b("Palette method") && !horizon, m.b("Liquid method")));
    let found = nc(|n| {
        let verdicts: Vec<Verdict> = n.verdicts.as_ref().map(|r| r.try_iter().collect()).unwrap_or_default();
        let mut decided = Vec::new();
        let mut active = Vec::new();
        for v in verdicts {
            // The palette's answer first; liquid already flowing says old when it has none.
            let verdict = if palette && v.palette != 0 { v.palette } else if liquid && v.flowing && !horizon { OLD } else { 0 };
            if verdict != 0 && !n.chunks.contains_key(&v.chunk) {
                decided.push((v.chunk, verdict));
            }
            if !horizon && v.palette == OLD && v.stale > 0 && !n.active.contains(&v.chunk) {
                active.push((v.chunk, v.stale));
            }
        }
        for c in std::mem::take(&mut n.flows) {
            if liquid && !n.chunks.contains_key(&c) && !decided.iter().any(|d| d.0 == c) {
                decided.push((c, NEW));
            }
        }
        for &(c, v) in &decided {
            n.chunks.insert(c, v);
            if let Some(f) = n.file.as_mut() {
                let _ = writeln!(f, "{} {} {}", c.0, c.1, if v == NEW { "n" } else { "o" });
            }
        }
        if activity.is_none() {
            active.clear();
        }
        n.active.extend(active.iter().map(|a| a.0));
        active
    });
    if activity.is_some_and(|m| m.b("Chat")) {
        for ((x, z), stale) in found {
            let s = if stale == 1 { "" } else { "s" };
            report(game, &format!("Player activity? chunk {x} {z} ({} {}): {stale} section{s} changed since it loaded", x * 16 + 8, z * 16 + 8));
        }
    }
}

// The chunks around the camera as squares at Render height, and Player Activity Detector's in its colour.
pub(super) fn draw(cam: &Camera, mods: &[Module], out: &mut impl Canvas) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let (chunks, activity) = (get("New Chunks"), get("Player Activity Detector"));
    if chunks.is_none() && activity.is_none() {
        return;
    }
    let eye = cam.eye();
    let (cx, cz) = ((eye[0].floor() as i32) >> 4, (eye[2].floor() as i32) >> 4);
    nc(|n| {
        if let Some(m) = chunks {
            let r = m.n("Render distance") as i32;
            let alpha = |argb: u32| ((((argb >> 24) as f64 * m.n("Fill opacity")).round() as u32) << 24) | (argb & 0x00FF_FFFF);
            let (new, old) = (alpha(m.c("New color")), alpha(m.c("Old color")));
            let y = m.n("Render height");
            for x in cx - r..=cx + r {
                for z in cz - r..=cz + r {
                    let argb = match n.chunks.get(&(x, z)) {
                        Some(&NEW) => new,
                        Some(_) if m.b("Show old") => old,
                        _ => continue,
                    };
                    square(cam, out, x, z, y, argb);
                }
            }
        }
        if let Some(m) = activity {
            let argb = (m.c("Color") & 0x00FF_FFFF) | 0x6000_0000;
            for &(x, z) in &n.active {
                square(cam, out, x, z, eye[1] - 2.0, argb);
            }
        }
    });
}

// A chunk's square at height `y`: whole when all its corners are in front of the camera, else in 4 by 4 parts.
fn square(cam: &Camera, out: &mut impl Canvas, x: i32, z: i32, y: f64, argb: u32) {
    let (x0, z0) = (f64::from(x * 16), f64::from(z * 16));
    if !cam.sees([x0 + 8.0, y, z0 + 8.0], 12.0) {
        return;
    }
    let part = |x0: f64, z0: f64, s: f64| {
        let c = [[x0, y, z0], [x0 + s, y, z0], [x0 + s, y, z0 + s], [x0, y, z0 + s]].map(|p| cam.project(p));
        match c {
            [Some(a), Some(b), Some(c), Some(d)] => Some([a, b, c, d]),
            _ => None,
        }
    };
    if let Some(q) = part(x0, z0, 16.0) {
        out.quad(q, argb);
        return;
    }
    for i in 0..4 {
        for j in 0..4 {
            if let Some(q) = part(x0 + f64::from(i) * 4.0, z0 + f64::from(j) * 4.0, 4.0) {
                out.quad(q, argb);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_protocol::chunk::Paletted;

    // A section of blocks `values` written with `palette` in that order, as a server sends it.
    fn section(palette: Vec<i32>, values: &[u32]) -> Section {
        let mut data = vec![0i64; 4096 / 16];
        for (i, &v) in values.iter().enumerate() {
            data[i / 16] |= (i64::from(v)) << ((i % 16) * 4);
        }
        Section { non_empty: 4096, fluids: 0, states: Paletted { bits: 4, palette, data }, biomes: Paletted { bits: 0, palette: vec![0], data: vec![] } }
    }

    fn judged(sections: &[Section]) -> Verdict {
        let buffer = horizon_protocol::chunk::ChunkData::from_sections(sections);
        judge(&Arrival { chunk: (0, 0), buffer, sections: sections.len(), biome_bits: 6, stale: true }, 0, &[false, false, false, false]).unwrap()
    }

    #[test]
    fn a_palette_that_starts_with_air_under_stone_is_new() {
        // Stone (1) at the first block, with air (0) first in the palette: generated just now.
        let mut values = vec![1u32; 4096];
        values[4095] = 0;
        let v = judged(&[section(vec![0, 1], &values)]);
        assert_eq!(v.palette, NEW);
        // The same blocks packed in the order they come: stone first.
        let packed: Vec<u32> = values.iter().map(|&b| 1 - b).collect();
        let v = judged(&[section(vec![1, 0], &packed)]);
        assert_eq!((v.palette, v.stale), (OLD, 0));
        // A palette still naming a block none are (3): changed since it was loaded.
        let v = judged(&[section(vec![1, 0, 3], &packed)]);
        assert_eq!((v.palette, v.stale), (OLD, 1));
    }
}
