// Base-hunting modules beside Base Finder, written fresh from what Meteor's Stash Finder and Trouser-Streak's hunting
// modules do (behaviour only): Stash Finder (chunks full of storage), Cave Disturbance Detector (a plain air block
// in a cave's cave air: someone dug there), Portal Pattern Finder (a portal's shape left in plain air inside cave
// air), Activated Spawner Detector (spawners a player has been near) and Mob Gear ESP (mobs wearing what players
// lose).

use super::sunset::{Camera, Canvas, Category, Module};
use super::sunset_chunk_feed::Feed;
use super::sunset_util::*;
use super::sunset_visual::{Look, draw_box};
use super::Game;
use horizon_client::chunk::Chunk;
use horizon_client::containers::BlockEntity;
use std::collections::HashSet;
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Stash Finder", Category::World, "Finds chunks full of storage.")
            .text("Storage blocks", "chest,trapped_chest,barrel,shulker_box,*_shulker_box,ender_chest,hopper,dispenser,dropper,furnace,blast_furnace,smoker")
            .num("Minimum storage count", 4.0, 1.0, 100.0, 1.0)
            .num("Minimum distance", 0.0, 0.0, 10000.0, 100.0)
            .bool("Waypoints", false),
        Module::new("Cave Disturbance Detector", Category::World, "Finds dug air in caves.")
            .bool("Chat", true)
            .colour("Color", "Orange"),
        Module::new("Portal Pattern Finder", Category::World, "Finds where portals were.")
            .bool("Chat", true)
            .colour("Color", "Purple"),
        Module::new("Activated Spawner Detector", Category::World, "Finds spawners players were near.")
            .bool("Trial spawners", true)
            .bool("Dungeons", true)
            .bool("Mineshafts", true)
            .bool("Fortresses", true)
            .bool("Bastions", true)
            .bool("Strongholds", true)
            .bool("Others", true)
            .colour("Color", "Red"),
        Module::new("Mob Gear ESP", Category::Render, "Shows mobs wearing player gear.")
            .text("Items", "netherite_*,diamond_sword,diamond_axe,diamond_pickaxe,diamond_shovel,diamond_hoe,mace,elytra,totem_of_undying,shulker_box,*_shulker_box,enchanted_golden_apple,ender_pearl")
            .text("Enchantments", "mending,frost_walker,soul_speed,swift_sneak,binding_curse,vanishing_curse")
            .bool("Named items", true)
            .bool("Chat", true)
            .colour("Color", "Pink")
            .num("Fill opacity", 0.25, 0.0, 1.0, 0.05),
    ]
}

// What a cave scan sees in a block state.
const OTHER: u8 = 0;
const AIR: u8 = 1;
const CAVE_AIR: u8 = 2;

// A find of a chunk scan: the chunk, where (and how big), and for Stash Finder its storage by kind.
struct Find {
    chunk: (i32, i32),
    at: [i32; 3],
    size: [i32; 3],
    kinds: Vec<(String, u32)>,
}

#[derive(Default)]
struct Hs {
    stash: Option<Feed<Find>>,
    cave: Option<Feed<Find>>,
    portal: Option<Feed<Find>>,
    // What each has found in this world and dimension (boxes drawn for the last three).
    stashes: HashSet<(i32, i32)>,
    caves: Vec<([i32; 3], [i32; 3])>,
    portals: Vec<([i32; 3], [i32; 3])>,
    spawners: Vec<[i32; 3]>,
    spawners_seen: HashSet<[i32; 3]>,
    gear: Vec<u32>,
    gear_said: HashSet<u32>,
    world: (u64, &'static str),
}

static HS: Mutex<Option<Hs>> = Mutex::new(None);

fn hs<R>(f: impl FnOnce(&mut Hs) -> R) -> R {
    f(HS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Hs::default))
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    // A new world or dimension starts over.
    let world = (world_hash(game).unwrap_or(0), game.dim.id());
    if hs(|h| h.world != world) {
        hs(|h| *h = Hs { world, ..Hs::default() });
    }
    stash_finder(game, get("Stash Finder"));
    cave_scan(game, get("Cave Disturbance Detector"), false);
    cave_scan(game, get("Portal Pattern Finder"), true);
    activated_spawners(game, get("Activated Spawner Detector"));
    mob_gear(game, get("Mob Gear ESP"));
}

// ---- Stash Finder

fn stash_finder(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        hs(|h| h.stash = None);
        return;
    };
    let min = m.n("Minimum storage count") as u32;
    let key = format!("{}|{}|{min}", game.dim.id(), m.t("Storage blocks"));
    if hs(|h| h.stash.as_ref().is_none_or(|f| f.key != key)) {
        let list: Vec<&str> = m.t("Storage blocks").split(',').map(str::trim).filter(|x| !x.is_empty()).collect();
        let mut names: Vec<String> = Vec::new();
        let kinds: Vec<u16> = game
            .res
            .facts()
            .states
            .iter()
            .map(|s| {
                let name = short(&s.name);
                let Some(k) = list.iter().position(|x| x.strip_prefix('*').map_or(*x == name, |end| name.ends_with(end))) else {
                    return u16::MAX;
                };
                let label = list[k].trim_start_matches("*_").replace('_', " ");
                (names.iter().position(|n| *n == label).unwrap_or_else(|| {
                    names.push(label);
                    names.len() - 1
                })) as u16
            })
            .collect();
        let mut states = [0u32; 4096];
        let feed = Feed::start("stash-finder", key, move |chunk: &Chunk, out: &mut Vec<Find>| {
            let mut counts = vec![0u32; names.len()];
            let (mut total, mut sum) = (0u32, [0i64; 3]);
            for (i, section) in chunk.sections.iter().enumerate() {
                if section.is_uniform(section.get(0)) && kinds.get(section.get(0) as usize).is_none_or(|k| *k == u16::MAX) {
                    continue;
                }
                section.unpack(&mut states);
                let y0 = chunk.min_y() + i as i32 * 16;
                for (j, &v) in states.iter().enumerate() {
                    if let Some(&k) = kinds.get(v as usize).filter(|k| **k != u16::MAX) {
                        counts[usize::from(k)] += 1;
                        total += 1;
                        sum = [sum[0] + (j as i64 & 15), sum[1] + (j as i64 >> 8) + i64::from(y0), sum[2] + (j as i64 >> 4 & 15)];
                    }
                }
            }
            if total >= min {
                let n = i64::from(total);
                let mut kinds: Vec<(String, u32)> = names.iter().cloned().zip(counts).filter(|(_, c)| *c > 0).collect();
                kinds.sort_by(|a, b| b.1.cmp(&a.1));
                let at = [chunk.x * 16 + (sum[0] / n) as i32, (sum[1] / n) as i32, chunk.z * 16 + (sum[2] / n) as i32];
                out.push(Find { chunk: (chunk.x, chunk.z), at, size: [16, 1, 16], kinds });
            }
        });
        hs(|h| h.stash = feed);
    }
    let found = hs(|h| {
        let f = h.stash.as_mut().map(|f| f.pump(game)).unwrap_or_default();
        f.into_iter().filter(|f| h.stashes.insert(f.chunk)).collect::<Vec<_>>()
    });
    let spawn = game.spawn.map(|s| [s.x, s.z]);
    for f in found {
        let [x, y, z] = f.at;
        if spawn.is_some_and(|[sx, sz]| (f64::from(x) - sx).hypot(f64::from(z) - sz) < m.n("Minimum distance")) {
            continue;
        }
        let total: u32 = f.kinds.iter().map(|k| k.1).sum();
        let what: Vec<String> = f.kinds.iter().map(|(n, c)| format!("{c} {n}")).collect();
        report(game, &format!("Stash? {total} storage blocks in chunk {} {} around {x} {y} {z}: {}", f.chunk.0, f.chunk.1, what.join(", ")));
        if m.b("Waypoints") {
            super::sunset::add_waypoint(game, format!("Stash {x} {z}"), [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5]);
        }
    }
}

// ---- Cave Disturbance Detector and Portal Pattern Finder

// A chunk's state at a place inside it (`x`, `z` 0..16), as the cave scan sees it.
fn at(codes: &[u8], chunk: &Chunk, x: i32, y: i32, z: i32) -> u8 {
    codes.get(chunk.get(x, y, z) as usize).copied().unwrap_or(OTHER)
}

// A plain air block with cave air on all six sides: dug out, or a block put there and taken away.
fn disturbance(codes: &[u8], chunk: &Chunk, x: i32, y: i32, z: i32) -> bool {
    [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]]
        .iter()
        .all(|[dx, dy, dz]| at(codes, chunk, x + dx, y + dy, z + dz) == CAVE_AIR)
}

// A nether portal's imprint with `(x, y, z)` its frame's lower corner, along x (`along_x`) or z: the frame (4 wide,
// 5 high, corners left out) and its inside all plain air, a thin slab of it, with cave air around the frame and in
// front of and behind the inside (so a dug room is not one).
fn imprint(codes: &[u8], chunk: &Chunk, x: i32, y: i32, z: i32, along_x: bool) -> bool {
    let cell = |u: i32, v: i32, w: i32| {
        if along_x { at(codes, chunk, x + u, y + v, z + w) } else { at(codes, chunk, x + w, y + v, z + u) }
    };
    let corner = |u: i32, v: i32| (u == 0 || u == 3) && (v == 0 || v == 4);
    if !(0..4).all(|u| (0..5).all(|v| corner(u, v) || cell(u, v, 0) == AIR)) {
        return false;
    }
    let ring = (-1..5).flat_map(|u| (-1..6).map(move |v| (u, v))).filter(|&(u, v)| u == -1 || u == 4 || v == -1 || v == 5);
    let around = ring.filter(|&(u, v)| cell(u, v, 0) == CAVE_AIR).count();
    let sides = (1..3).flat_map(|u| (1..4).flat_map(move |v| [(u, v, -1), (u, v, 1)])).filter(|&(u, v, w)| cell(u, v, w) == CAVE_AIR).count();
    around >= 12 && sides >= 8
}

// On the scan thread: a chunk's first disturbance (or portal imprint), where both kinds of air meet: where and how
// big.
fn cave_look(codes: &[u8], states: &mut [u32; 4096], chunk: &Chunk, portals: bool) -> Option<([i32; 3], [i32; 3])> {
    for (i, section) in chunk.sections.iter().enumerate() {
        if section.is_uniform(section.get(0)) {
            continue;
        }
        section.unpack(states);
        let y0 = chunk.min_y() + i as i32 * 16;
        if !states.iter().any(|&v| codes.get(v as usize) == Some(&CAVE_AIR)) {
            continue;
        }
        for (j, &v) in states.iter().enumerate() {
            if codes.get(v as usize) != Some(&AIR) {
                continue;
            }
            let (x, y, z) = (j as i32 & 15, y0 + (j as i32 >> 8), j as i32 >> 4 & 15);
            // The block and its sides inside this chunk; or this block as the frame's bottom left (its corner is
            // one along), with the frame and a block around it inside this chunk.
            let found = if !portals {
                ((1..15).contains(&x) && (1..15).contains(&z) && disturbance(codes, chunk, x, y, z)).then_some(([x, y, z], [1, 1, 1]))
            } else if (2..=12).contains(&x) && (1..=14).contains(&z) && imprint(codes, chunk, x - 1, y, z, true) {
                Some(([x - 1, y, z], [4, 5, 1]))
            } else {
                ((1..=14).contains(&x) && (2..=12).contains(&z) && imprint(codes, chunk, x, y, z - 1, false)).then_some(([x, y, z - 1], [1, 5, 4]))
            };
            if let Some(([x, y, z], size)) = found {
                return Some(([chunk.x * 16 + x, y, chunk.z * 16 + z], size));
            }
        }
    }
    None
}

// The cave scan's feed and finds: Portal Pattern Finder's, or Cave Disturbance Detector's.
fn cave_parts(h: &mut Hs, portals: bool) -> (&mut Option<Feed<Find>>, &mut Vec<([i32; 3], [i32; 3])>) {
    if portals { (&mut h.portal, &mut h.portals) } else { (&mut h.cave, &mut h.caves) }
}

fn cave_scan(game: &mut Game<'_>, m: Option<&Module>, portals: bool) {
    let Some(m) = m else {
        hs(|h| {
            let (feed, finds) = cave_parts(h, portals);
            *feed = None;
            finds.clear();
        });
        return;
    };
    let key = game.dim.id().to_owned();
    if hs(|h| cave_parts(h, portals).0.as_ref().is_none_or(|f| f.key != key)) {
        let codes: Vec<u8> = game
            .res
            .facts()
            .states
            .iter()
            .map(|s| match short(&s.name) {
                "air" => AIR,
                "cave_air" => CAVE_AIR,
                _ => OTHER,
            })
            .collect();
        let mut states = [0u32; 4096];
        let name = if portals { "portal-patterns" } else { "cave-disturbance" };
        let started = Feed::start(name, key, move |chunk: &Chunk, out: &mut Vec<Find>| {
            if let Some((at, size)) = cave_look(&codes, &mut states, chunk, portals) {
                out.push(Find { chunk: (chunk.x, chunk.z), at, size, kinds: Vec::new() });
            }
        });
        hs(|h| *cave_parts(h, portals).0 = started);
    }
    let found = hs(|h| {
        let (feed, finds) = cave_parts(h, portals);
        let mut fresh = Vec::new();
        for f in feed.as_mut().map(|f| f.pump(game)).unwrap_or_default() {
            if !finds.contains(&(f.at, f.size)) {
                finds.push((f.at, f.size));
                fresh.push(f.at);
            }
        }
        fresh
    });
    if m.b("Chat") {
        let what = if portals { "Portal imprint" } else { "Cave disturbance" };
        for [x, y, z] in found {
            report(game, &format!("{what} at {x} {y} {z}"));
        }
    }
}

// ---- Activated Spawner Detector

// The structure a spawner belongs to, from the blocks around it (5 each way): None for one it is not looked for in.
fn spawner_place(game: &Game<'_>, m: &Module, p: [i32; 3]) -> Option<&'static str> {
    let facts = game.res.facts();
    let mut found = "Others";
    'look: for dx in -5..=5 {
        for dy in -3..=3 {
            for dz in -5..=5 {
                let s = game.world.block(p[0] + dx, p[1] + dy, p[2] + dz);
                found = match short(&facts.get(s).name) {
                    "mossy_cobblestone" => "Dungeons",
                    "cobweb" | "rail" => "Mineshafts",
                    "nether_bricks" | "nether_brick_fence" => "Fortresses",
                    "polished_blackstone_bricks" | "gilded_blackstone" | "cracked_polished_blackstone_bricks" => "Bastions",
                    "stone_bricks" | "mossy_stone_bricks" | "cracked_stone_bricks" | "end_portal_frame" => "Strongholds",
                    _ => continue,
                };
                break 'look;
            }
        }
    }
    m.b(found).then_some(found)
}

// Twice a second: each spawner whose countdown has moved off its first 20 ticks (a player came within its range),
// and each trial spawner not waiting for players, said once.
fn activated_spawners(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        hs(|h| {
            h.spawners.clear();
            h.spawners_seen.clear();
        });
        return;
    };
    if game.ticks % 10 != 0 {
        return;
    }
    let registry = &game.pack.assets.registry;
    let mut found = Vec::new();
    for (&(x, y, z), entity) in &game.world.block_entities {
        let p = [x, y, z];
        if hs(|h| h.spawners_seen.contains(&p)) {
            continue;
        }
        let what = match entity {
            BlockEntity::Spawner(s) if s.spawn_delay != 20 => spawner_place(game, m, p).map(|place| (format!("{} spawner", place.trim_end_matches('s').to_lowercase()), s.spawn_delay)),
            BlockEntity::TrialSpawner(_) if m.b("Trial spawners") => {
                let state = registry.state(game.world.block(x, y, z)).and_then(|b| b.properties.get("trial_spawner_state").cloned());
                state.filter(|s| s != "waiting_for_players" && s != "inactive").map(|s| (format!("trial spawner ({})", s.replace('_', " ")), 0))
            }
            _ => None,
        };
        if let Some((label, delay)) = what {
            found.push((p, label, delay));
        }
    }
    for (p, label, delay) in found {
        hs(|h| {
            h.spawners_seen.insert(p);
            h.spawners.push(p);
        });
        let [x, y, z] = p;
        let delay = if delay > 0 { format!(", next spawn in {delay} ticks") } else { String::new() };
        report(game, &format!("Activated {label} at {x} {y} {z}{delay}"));
    }
}

// ---- Mob Gear ESP

// What on a mob gives it away: the first listed item it wears or holds, a listed enchantment, or a named piece.
fn player_gear(m: &Module, mob: &horizon_client::mobs::Mob) -> Option<String> {
    let listed_item = |id: &str| {
        let id = short(id);
        m.t("Items").split(',').map(str::trim).filter(|x| !x.is_empty()).any(|x| x.strip_prefix('*').map_or(x == id, |end| id.ends_with(end)))
            || m.t("Items").split(',').map(str::trim).any(|x| x.strip_suffix('*').is_some_and(|start| !start.is_empty() && id.starts_with(start)))
    };
    let e = &mob.extra;
    for id in [&e.main_hand, &e.off_hand, &e.head, &e.chest, &e.legs, &e.feet].into_iter().flatten() {
        if listed_item(id) {
            return Some(short(id).replace('_', " "));
        }
    }
    for stack in e.armor.iter().flatten() {
        if let Some((name, _)) = stack.enchantments().iter().find(|(n, _)| listed(m.t("Enchantments"), n)) {
            return Some(format!("{} with {}", short(&stack.item).replace('_', " "), short(name).replace('_', " ")));
        }
        if m.b("Named items") && let Some(name) = stack.custom_name() {
            return Some(format!("{} named {name}", short(&stack.item).replace('_', " ")));
        }
    }
    None
}

fn mob_gear(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        hs(|h| {
            h.gear.clear();
            h.gear_said.clear();
        });
        return;
    };
    if game.ticks % 10 != 0 {
        return;
    }
    let found: Vec<(u32, String, [f64; 3], String)> = game
        .mobs
        .iter()
        .filter(|x| x.alive() && !x.removed && short(&x.kind.id) != "player")
        .filter_map(|x| player_gear(m, x).map(|g| (x.id, g, [x.position.x, x.position.y, x.position.z], short(&x.kind.id).replace('_', " "))))
        .collect();
    let fresh: Vec<_> = hs(|h| {
        h.gear = found.iter().map(|f| f.0).collect();
        found.into_iter().filter(|f| h.gear_said.insert(f.0)).collect()
    });
    if m.b("Chat") {
        for (_, gear, p, kind) in fresh {
            report(game, &format!("A {kind} with {gear} at {:.0} {:.0} {:.0}", p[0], p[1], p[2]));
        }
    }
}

// ---- Drawing

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let block = |p: [i32; 3], size: [i32; 3]| {
        let lo = p.map(f64::from);
        (lo, [lo[0] + f64::from(size[0]), lo[1] + f64::from(size[1]), lo[2] + f64::from(size[2])])
    };
    hs(|h| {
        for (name, list) in [("Cave Disturbance Detector", &h.caves), ("Portal Pattern Finder", &h.portals)] {
            if let Some(m) = get(name) {
                for &(p, size) in list {
                    let (lo, hi) = block(p, size);
                    draw_box(cam, out, &Look::BOXED, lo, hi, m.c("Color"), lw);
                }
            }
        }
        if let Some(m) = get("Activated Spawner Detector") {
            for &p in &h.spawners {
                let (lo, hi) = block(p, [1, 1, 1]);
                draw_box(cam, out, &Look::BOXED, lo, hi, m.c("Color"), lw);
            }
        }
        if let Some(m) = get("Mob Gear ESP") {
            let look = Look::of(m);
            for mob in game.mobs.iter().filter(|x| h.gear.contains(&x.id)) {
                let b = mob.bounds();
                let (lo, hi) = (b.min_position(), b.max_position());
                draw_box(cam, out, &look, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z], m.c("Color"), lw);
            }
        }
    });
}
