// What the server's light gives away that its blocks don't. The server works out light from its real blocks and sends
// it with the chunk; anti-xray (or a plugin hiding bases or geodes) changes the blocks it sends but not that light. In
// the server's world a block's light falls by at least one per block away from whatever gives it, so a cell no side
// of which is brighter gives that light itself: when the block the client is shown there gives less, the real block
// is another one, hidden. The scans read only chunks already received and never ask the server for anything.
//
// Geode Finder finds amethyst geodes three ways: Palette, a section whose palette still names amethyst buds (a bud
// entry the server's block list keeps after the buds it sent were changed to stone); Light, a cell lit 1, 2, 4 or 5
// (the light of a small, medium or large bud and a cluster) brighter than its sides, against amethyst, by a block
// that gives none: a hidden bud; Light edges, air lit 0 beside air lit 4, which no light falling a level a block
// makes: the edge left where a plugin cut the light round a geode it hides. Light ESP shows light underground (below
// Max Y, brighter than the sky there): torches and lamps of bases dug into the deepslate. Hidden sources keeps only
// the light no shown block explains, which leaves out lava and glow lichen. Base Finder's Hidden light sign is the
// same test.
//
// Behaviour after Donut Client 3.0's Amethyst Scanner and Light Debug (Spectre 26.2's AmethystBypass and LightEsp),
// written anew here; the hidden-source test is Sunset's own.

use super::sunset::{Camera, Canvas, Category, Module, line};
use super::sunset_chunk_feed::Feed;
use super::sunset_util::*;
use super::sunset_visual::{Look, draw_box};
use super::Game;
use horizon_client::chunk::{Chunk, Light, Section};
use horizon_world::registry::BlockStateId;
use std::collections::HashMap;
use std::sync::Mutex;

pub(super) const GEODE: &str = "Geode Finder";
pub(super) const LIGHT: &str = "Light ESP";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(GEODE, Category::World, "Finds geodes the server hides.")
            .pick("Method", &["Palette", "Light", "Light edges"])
            .num("Min cells", 12.0, 1.0, 100.0, 1.0)
            .bool("Chat", false)
            .bool("Tracers", false)
            .colour("Color", "Purple"),
        Module::new(LIGHT, Category::Render, "Shows light the server sends underground.")
            .pick("Show", &["All light", "Hidden sources"])
            .num("Max Y", 0.0, -64.0, 320.0, 1.0)
            .num("Minimum light", 5.0, 1.0, 15.0, 1.0)
            .bool("Denoise", true)
            .num("Radius", 8.0, 1.0, 32.0, 1.0)
            .num("Max", 4096.0, 256.0, 16384.0, 256.0)
            .colour("Color", "Red")
            .colour("Source color", "Yellow"),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        "Min cells" => m.p("Method") == "Light edges",
        _ => true,
    }
}

// ---- what the scans know of each block state

pub(super) const AMETHYST: u8 = 1;
pub(super) const BUDDING: u8 = 2;
pub(super) const CLUSTER: u8 = 3;
pub(super) const BUD: u8 = 4;
const AIR: u8 = 5;

// The six sides, in the order of a facing's index (down, up, north, south, west, east).
pub(super) const SIDES: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
pub(super) const NO_FACING: u8 = 6;

// Each block state's amethyst kind (0 for others), the side a bud or cluster faces (it grows on the block behind it),
// and the light the block gives.
pub(super) struct Table {
    pub(super) kind: Vec<u8>,
    pub(super) facing: Vec<u8>,
    pub(super) emission: Vec<u8>,
}

impl Table {
    pub(super) fn of(game: &Game<'_>) -> Self {
        let registry = &game.pack.assets.registry;
        let states = &game.res.facts().states;
        let mut t = Table { kind: Vec::with_capacity(states.len()), facing: Vec::with_capacity(states.len()), emission: Vec::with_capacity(states.len()) };
        for (id, s) in states.iter().enumerate() {
            let kind = match short(&s.name) {
                "amethyst_block" => AMETHYST,
                "budding_amethyst" => BUDDING,
                "amethyst_cluster" => CLUSTER,
                "small_amethyst_bud" | "medium_amethyst_bud" | "large_amethyst_bud" => BUD,
                "air" | "cave_air" | "void_air" => AIR,
                _ => 0,
            };
            let facing = if matches!(kind, CLUSTER | BUD) {
                let side = registry.state(BlockStateId(id as u32)).and_then(|b| b.properties.get("facing").cloned());
                match side.as_deref() {
                    Some("down") => 0,
                    Some("up") => 1,
                    Some("north") => 2,
                    Some("south") => 3,
                    Some("west") => 4,
                    Some("east") => 5,
                    _ => NO_FACING,
                }
            } else {
                NO_FACING
            };
            t.kind.push(kind);
            t.facing.push(facing);
            t.emission.push(s.emission);
        }
        t
    }
    pub(super) fn kind(&self, state: u32) -> u8 {
        self.kind.get(state as usize).copied().unwrap_or(0)
    }
    fn emits(&self, state: u32) -> u8 {
        self.emission.get(state as usize).copied().unwrap_or(0)
    }
}

// ---- light

pub(super) fn block_light(chunk: &Chunk, x: i32, y: i32, z: i32) -> u8 {
    chunk.light(x, y, z) & 15
}

// A section with no block light anywhere in it.
pub(super) fn dark(section: &Section) -> bool {
    match &section.light {
        Light::Uniform(v) => v & 15 == 0,
        Light::Sky(_, block) => *block == 0,
        _ => false,
    }
}

// Whether the cell at `x y z` (inside the chunk: 1..15 across) is lit `l` (more than 0) by a hidden source: no side is
// brighter and the block shown there gives less light.
pub(super) fn hidden_source(chunk: &Chunk, t: &Table, x: i32, y: i32, z: i32, l: u8) -> bool {
    l > 0 && t.emits(chunk.get(x, y, z)) < l && SIDES.iter().all(|d| block_light(chunk, x + d[0], y + d[1], z + d[2]) <= l)
}

// The first hidden source of at least `min` light in a chunk: where (in the world) and how bright.
pub(super) fn first_hidden(chunk: &Chunk, t: &Table, min: u8) -> Option<([i32; 3], u8)> {
    for (i, section) in chunk.sections.iter().enumerate() {
        if dark(section) {
            continue;
        }
        let y0 = chunk.min_y() + i as i32 * 16;
        for j in 0..4096 {
            let (x, y, z) = (j as i32 & 15, y0 + (j as i32 >> 8), j as i32 >> 4 & 15);
            if !(1..15).contains(&x) || !(1..15).contains(&z) {
                continue;
            }
            let l = section.light(j) & 15;
            if l >= min && hidden_source(chunk, t, x, y, z, l) {
                return Some(([chunk.x * 16 + x, y, chunk.z * 16 + z], l));
            }
        }
    }
    None
}

// ---- Geode Finder

// A spread-out pick among a section's candidate cells, the same every scan.
fn scatter(x: i32, y: i32, z: i32) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^ h >> 32
}

fn touches(chunk: &Chunk, t: &Table, x: i32, y: i32, z: i32, kinds: &[u8]) -> bool {
    SIDES.iter().any(|d| {
        let (nx, nz) = (x + d[0], z + d[2]);
        (0..16).contains(&nx) && (0..16).contains(&nz) && kinds.contains(&t.kind(chunk.get(nx, y + d[1], nz)))
    })
}

// Palette: the first section whose palette names a bud; its mark is an air cell against the geode's blocks, else the
// section's middle.
fn geode_palette(chunk: &Chunk, t: &Table, states: &mut [u32; 4096]) -> Vec<[i32; 3]> {
    for (i, section) in chunk.sections.iter().enumerate() {
        let Some(palette) = section.blocks.palette() else {
            continue;
        };
        if !palette.iter().any(|&v| t.kind(v) == BUD) {
            continue;
        }
        let y0 = chunk.min_y() + i as i32 * 16;
        section.unpack(states);
        let mut best: Option<(u64, [i32; 3])> = None;
        for (j, &v) in states.iter().enumerate() {
            let (x, y, z) = (j as i32 & 15, y0 + (j as i32 >> 8), j as i32 >> 4 & 15);
            if t.kind(v) == AIR && touches(chunk, t, x, y, z, &[AMETHYST, BUDDING]) {
                let w = [chunk.x * 16 + x, y, chunk.z * 16 + z];
                let h = scatter(w[0], w[1], w[2]);
                if best.is_none_or(|(b, _)| h < b) {
                    best = Some((h, w));
                }
            }
        }
        return vec![best.map_or([chunk.x * 16 + 8, y0 + 8, chunk.z * 16 + 8], |(_, w)| w)];
    }
    Vec::new()
}

// Light: hidden buds, a cell lit as a bud or cluster is, brighter than its sides, against amethyst.
fn geode_light(chunk: &Chunk, t: &Table) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for (i, section) in chunk.sections.iter().enumerate() {
        if dark(section) || !section.blocks.maybe_has(|v| matches!(t.kind(v), AMETHYST | BUDDING)) {
            continue;
        }
        let y0 = chunk.min_y() + i as i32 * 16;
        for j in 0..4096 {
            let (x, y, z) = (j as i32 & 15, y0 + (j as i32 >> 8), j as i32 >> 4 & 15);
            let l = section.light(j) & 15;
            if !matches!(l, 1 | 2 | 4 | 5) || !(1..15).contains(&x) || !(1..15).contains(&z) || t.emits(section.get(j)) > 0 {
                continue;
            }
            if SIDES.iter().all(|d| block_light(chunk, x + d[0], y + d[1], z + d[2]) < l) && touches(chunk, t, x, y, z, &[AMETHYST, BUDDING]) {
                out.push([chunk.x * 16 + x, y, chunk.z * 16 + z]);
            }
        }
    }
    out
}

// Light edges: open cells (air or a cluster) lit 0 whose brightest side is lit 4 and open. A chunk with fewer than
// `min` of them is left out; one with more shows its first 200.
fn geode_edges(chunk: &Chunk, t: &Table, min: usize) -> Vec<[i32; 3]> {
    let open = |x: i32, y: i32, z: i32| matches!(t.kind(chunk.get(x, y, z)), AIR | CLUSTER);
    let mut out = Vec::new();
    let mut count = 0;
    for (i, section) in chunk.sections.iter().enumerate() {
        let y0 = chunk.min_y() + i as i32 * 16;
        if y0 + 15 < -58 || y0 > 30 {
            continue;
        }
        // Lit cells are needed around: this section or the ones above and below.
        let near = |k: usize| chunk.sections.get(k).is_some_and(|s| !dark(s));
        if !(near(i) || i.checked_sub(1).is_some_and(near) || near(i + 1)) {
            continue;
        }
        for j in 0..4096 {
            let (x, y, z) = (j as i32 & 15, y0 + (j as i32 >> 8), j as i32 >> 4 & 15);
            if !(-58..=30).contains(&y) || !(1..15).contains(&x) || !(1..15).contains(&z) || section.light(j) & 15 != 0 || !open(x, y, z) {
                continue;
            }
            let mut brightest = 0;
            let mut lit_open = false;
            for d in SIDES {
                let (nx, ny, nz) = (x + d[0], y + d[1], z + d[2]);
                let l = block_light(chunk, nx, ny, nz);
                brightest = brightest.max(l);
                lit_open |= l == 4 && open(nx, ny, nz);
            }
            if brightest == 4 && lit_open {
                count += 1;
                if out.len() < 200 {
                    out.push([chunk.x * 16 + x, y, chunk.z * 16 + z]);
                }
            }
        }
    }
    if count >= min { out } else { Vec::new() }
}

// ---- Light ESP

// A lit cell shown: where, how bright, and whether it is a hidden source.
#[derive(Clone, Copy)]
struct Lit {
    at: [i32; 3],
    light: u8,
    source: bool,
}

struct LightPlan {
    table: Table,
    max_y: i32,
    min: u8,
    denoise: bool,
    hidden_only: bool,
}

// On the scan thread: a chunk's lit cells below Max Y, joined into fields (cells lit touching each other). With
// Denoise a field whose brightest cell is under Minimum light + 3 goes (dim leaks, glow lichen); with Hidden sources
// only fields holding a hidden source stay. Of a field only the cells at its surface are drawn.
fn light_look(plan: &LightPlan, chunk: &Chunk, lit: &mut Vec<u8>, seen: &mut Vec<bool>, out: &mut Vec<Lit>) {
    let min_y = chunk.min_y();
    let top = plan.max_y.min(min_y + chunk.sections.len() as i32 * 16);
    if top <= min_y {
        return;
    }
    let height = (top - min_y) as usize;
    lit.clear();
    lit.resize(height * 256, 0);
    let mut any = false;
    for (i, section) in chunk.sections.iter().enumerate() {
        let y0 = min_y + i as i32 * 16;
        if y0 >= top || dark(section) {
            continue;
        }
        for j in 0..4096 {
            let y = y0 + (j as i32 >> 8);
            if y >= top {
                break;
            }
            let v = section.light(j);
            let (block, sky) = (v & 15, v >> 4);
            if block >= plan.min && block > sky {
                lit[(y - min_y) as usize * 256 + (j & 255)] = block;
                any = true;
            }
        }
    }
    if !any {
        return;
    }
    seen.clear();
    seen.resize(lit.len(), false);
    let at = |k: usize| [(k & 15) as i32, min_y + (k / 256) as i32, (k >> 4 & 15) as i32];
    let mut field = Vec::new();
    let mut todo = Vec::new();
    for start in 0..lit.len() {
        if lit[start] == 0 || seen[start] {
            continue;
        }
        field.clear();
        todo.push(start);
        seen[start] = true;
        let (mut brightest, mut source) = (0u8, false);
        while let Some(k) = todo.pop() {
            field.push(k);
            let [x, y, z] = at(k);
            brightest = brightest.max(lit[k]);
            if (1..15).contains(&x) && (1..15).contains(&z) && hidden_source(chunk, &plan.table, x, y, z, lit[k]) {
                source = true;
            }
            for d in SIDES {
                let (nx, ny, nz) = (x + d[0], y + d[1], z + d[2]);
                if !(0..16).contains(&nx) || !(0..16).contains(&nz) || ny < min_y || ny >= top {
                    continue;
                }
                let n = (ny - min_y) as usize * 256 + (nz as usize) * 16 + nx as usize;
                if lit[n] > 0 && !seen[n] {
                    seen[n] = true;
                    todo.push(n);
                }
            }
        }
        if (plan.denoise && brightest < plan.min.saturating_add(3)) || (plan.hidden_only && !source) {
            continue;
        }
        for &k in &field {
            let [x, y, z] = at(k);
            let inner = SIDES.iter().all(|d| {
                let (nx, ny, nz) = (x + d[0], y + d[1], z + d[2]);
                (0..16).contains(&nx) && (0..16).contains(&nz) && ny >= min_y && ny < top && lit[(ny - min_y) as usize * 256 + (nz as usize) * 16 + nx as usize] > 0
            });
            if inner || out.len() >= 4096 {
                continue;
            }
            let src = (1..15).contains(&x) && (1..15).contains(&z) && hidden_source(chunk, &plan.table, x, y, z, lit[k]);
            out.push(Lit { at: [chunk.x * 16 + x, y, chunk.z * 16 + z], light: lit[k], source: src });
        }
    }
}

// ---- state and ticks

#[derive(Default)]
struct Ls {
    geode: Option<Feed<((i32, i32), Vec<[i32; 3]>)>>,
    geodes: HashMap<(i32, i32), Vec<[i32; 3]>>,
    light: Option<Feed<((i32, i32), Vec<Lit>)>>,
    lit: HashMap<(i32, i32), Vec<Lit>>,
    revision: u64,
}

static LS: Mutex<Option<Ls>> = Mutex::new(None);

fn ls<R>(f: impl FnOnce(&mut Ls) -> R) -> R {
    f(LS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Ls::default))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on).filter(|_| in_world(game));
    let (geode, light) = (get(GEODE), get(LIGHT));
    if geode.is_none() && light.is_none() {
        ls(|s| *s = Ls::default());
        return;
    }
    // Chunks gone from the world take their finds with them.
    let revision = game.world.chunks.membership_revision();
    ls(|s| {
        if s.revision != revision {
            s.revision = revision;
            s.geodes.retain(|c, _| game.world.chunks.contains_key(c));
            s.lit.retain(|c, _| game.world.chunks.contains_key(c));
        }
    });
    let world = world_hash(game).unwrap_or(0);
    match geode {
        Some(m) => {
            let (method, min) = (m.p("Method"), m.n("Min cells") as usize);
            let key = format!("{world}|{}|{method}|{min}", game.dim.id());
            if ls(|s| s.geode.as_ref().is_none_or(|f| f.key != key)) {
                let table = Table::of(game);
                let mut states = [0u32; 4096];
                let feed = Feed::start("geode-finder", key, move |chunk: &Chunk, out: &mut Vec<((i32, i32), Vec<[i32; 3]>)>| {
                    let found = match method {
                        "Light" => geode_light(chunk, &table),
                        "Light edges" => geode_edges(chunk, &table, min),
                        _ => geode_palette(chunk, &table, &mut states),
                    };
                    if !found.is_empty() {
                        out.push(((chunk.x, chunk.z), found));
                    }
                });
                ls(|s| {
                    s.geode = feed;
                    s.geodes.clear();
                });
            }
            let fresh = ls(|s| {
                let found = s.geode.as_mut().map(|f| f.pump(game)).unwrap_or_default();
                let mut fresh = Vec::new();
                for (c, cells) in found {
                    if let Some(&first) = cells.first() {
                        fresh.push((c, first, cells.len()));
                    }
                    s.geodes.insert(c, cells);
                }
                fresh
            });
            if m.b("Chat") {
                for ((cx, cz), [x, y, z], n) in fresh {
                    let cells = if n > 1 { format!(" ({n} cells)") } else { String::new() };
                    report(game, &format!("Geode in chunk {cx} {cz} at {x} {y} {z}{cells}"));
                }
            }
        }
        None => ls(|s| {
            s.geode = None;
            s.geodes.clear();
        }),
    }
    match light {
        Some(m) => {
            let (max_y, min, denoise, hidden_only) = (m.n("Max Y") as i32, m.n("Minimum light") as u8, m.b("Denoise"), m.p("Show") == "Hidden sources");
            let key = format!("{world}|{}|{max_y}|{min}|{denoise}|{hidden_only}", game.dim.id());
            if ls(|s| s.light.as_ref().is_none_or(|f| f.key != key)) {
                let plan = LightPlan { table: Table::of(game), max_y, min, denoise, hidden_only };
                let (mut lit, mut seen) = (Vec::new(), Vec::new());
                let feed = Feed::start("light-esp", key, move |chunk: &Chunk, out: &mut Vec<((i32, i32), Vec<Lit>)>| {
                    let mut cells = Vec::new();
                    light_look(&plan, chunk, &mut lit, &mut seen, &mut cells);
                    if !cells.is_empty() {
                        out.push(((chunk.x, chunk.z), cells));
                    }
                });
                ls(|s| {
                    s.light = feed;
                    s.lit.clear();
                });
            }
            ls(|s| {
                for (c, cells) in s.light.as_mut().map(|f| f.pump(game)).unwrap_or_default() {
                    s.lit.insert(c, cells);
                }
            });
        }
        None => ls(|s| {
            s.light = None;
            s.lit.clear();
        }),
    }
}

// ---- drawing

// Brighter light draws whiter and more solid.
fn graded(argb: u32, light: u8) -> u32 {
    let t = f64::from(light) / 15.0;
    let ch = |shift: u32| {
        let c = f64::from(argb >> shift & 0xFF);
        ((c + (255.0 - c) * t * 0.7).round() as u32) << shift
    };
    let alpha = ((f64::from(argb >> 24) * (0.5 + 0.5 * t)).round() as u32) << 24;
    alpha | ch(16) | ch(8) | ch(0)
}

pub(super) fn draw(cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let (geode, light) = (get(GEODE), get(LIGHT));
    if geode.is_none() && light.is_none() {
        return;
    }
    let eye = cam.eye();
    let cube = |p: [i32; 3]| {
        let lo = p.map(f64::from);
        (lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0])
    };
    let middle = [cam.w as f32 / 2.0, cam.h as f32 / 2.0];
    ls(|s| {
        if let Some(m) = geode {
            let (colour, tracers) = (m.c("Color"), m.b("Tracers"));
            for p in s.geodes.values().flatten() {
                let (lo, hi) = cube(*p);
                draw_box(cam, out, &Look::BOXED, lo, hi, colour, lw);
                if tracers && let Some(to) = cam.project(center_of(*p)) {
                    line(out, middle, to, colour, lw);
                }
            }
        }
        if let Some(m) = light {
            let (colour, source, max) = (m.c("Color"), m.c("Source color"), m.n("Max") as usize);
            let range = m.n("Radius") * 16.0;
            let mut drawn = 0;
            'all: for cells in s.lit.values() {
                for c in cells {
                    let mid = center_of(c.at);
                    if (mid[0] - eye[0]).hypot(mid[2] - eye[2]) > range {
                        continue;
                    }
                    let (lo, hi) = cube(c.at);
                    let argb = if c.source { source } else { graded(colour, c.light) };
                    if draw_box(cam, out, &Look::BOXED, lo, hi, argb, lw) {
                        drawn += 1;
                        if drawn >= max {
                            break 'all;
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::graded;

    #[test]
    fn brighter_light_draws_whiter_and_more_solid() {
        let dim = graded(0x80FF_0000, 1);
        let bright = graded(0x80FF_0000, 15);
        assert!(bright >> 24 > dim >> 24);
        assert!((bright >> 8 & 0xFF) > (dim >> 8 & 0xFF));
        assert_eq!(bright >> 16 & 0xFF, 0xFF);
    }
}
