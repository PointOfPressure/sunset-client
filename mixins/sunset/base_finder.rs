// Base Finder: chunks that show players were there, looked at the way Trouser-Streak's BaseFinder does (behaviour
// only, written fresh): block lists in tiers, each with the count it takes (one shulker box is enough, glass takes
// twenty), and single signs no world makes: written signs, open portals, soul sand bubble columns, builds above the
// highest terrain, bedrock away from the floor and roof, spawners with none of their structure around them, anything
// but mushrooms on the nether roof; and entities: item frames without an elytra, resting ender pearls, name-tagged
// mobs, villagers that have traded, boats and crowds. Each chunk is said once (one line per cooldown) with a waypoint
// that goes when Base Finder turns off.

use super::sunset::{Category, Module};
use super::sunset_util::*;
use super::Game;
use horizon_client::chunk::Chunk;
use horizon_client::containers::BlockEntity;
use horizon_world::registry::BlockStateId;
use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

const NAME: &str = "Base Finder";

// The tiers: what each list holds and how many of its blocks in one chunk make a base. Left out is what structures
// are made of: wool, glass panes, lamps, comparators and note blocks (ancient cities), glazed terracotta (trail
// ruins), and oak, spruce, birch, acacia, jungle and dark oak planks (villages, mineshafts, mansions, shipwrecks).
const LISTS: [(&str, &str, &str, f64); 7] = [
    (
        "List 1",
        "List 1 count",
        "shulker_box,*_shulker_box,*_bed,ender_chest,beacon,respawn_anchor,lodestone,conduit,crafter,*_concrete,*_concrete_powder,netherite_block,diamond_block",
        1.0,
    ),
    ("List 2", "List 2 count", "enchanting_table,anvil,chipped_anvil,damaged_anvil,brewing_stand,jukebox,observer,target,beehive,honey_block,slime_block,scaffolding,tinted_glass,daylight_detector", 2.0),
    ("List 3", "List 3 count", "hopper,dropper,dispenser,repeater,*_banner", 6.0),
    ("List 4", "List 4 count", "chest,trapped_chest,barrel,furnace,blast_furnace,smoker,crafting_table,*_sign", 12.0),
    ("List 5", "List 5 count", "glass,*_stained_glass,*_stained_glass_pane", 20.0),
    ("List 6", "List 6 count", "cherry_planks,bamboo_planks,bamboo_mosaic,mangrove_planks,crimson_planks,warped_planks,pale_oak_planks", 20.0),
    ("List 7", "List 7 count", "", 12.0),
];

pub(super) fn defaults() -> Vec<Module> {
    let mut m = Module::new(NAME, Category::World, "Finds player bases.")
        .bool("Waypoints", true)
        .num("Message cooldown", 20.0, 0.0, 200.0, 5.0)
        .num("Ignore near spawn", 0.0, 0.0, 10000.0, 100.0)
        .bool("Written signs", true)
        .bool("Open portals", true)
        .bool("Bubble columns", true)
        .bool("Sky builds", true)
        .num("Sky build Y", 260.0, 64.0, 319.0, 1.0)
        .bool("Bedrock", true)
        .num("Bedrock layers", 5.0, 1.0, 16.0, 1.0)
        .bool("Lone spawners", true)
        .bool("Nether roof", true)
        .num("Entity scan delay", 20.0, 1.0, 200.0, 1.0)
        .bool("Item frames", true)
        .bool("Ender pearls", true)
        .bool("Name tags", true)
        .bool("Traded villagers", true)
        .bool("Boats", true)
        .bool("Entity clusters", true)
        .num("Cluster size", 12.0, 3.0, 100.0, 1.0)
        .text("Cluster entities", "cow,sheep,pig,chicken,rabbit,horse,donkey,mule,llama,goat,villager,iron_golem");
    for (list, count, blocks, n) in LISTS {
        m = m.text(list, blocks).num(count, n, 1.0, 500.0, 1.0);
    }
    vec![m]
}

// Which settings show: a check's own numbers with it on.
pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        "Sky build Y" => m.b("Sky builds"),
        "Bedrock layers" => m.b("Bedrock"),
        "Cluster size" | "Cluster entities" => m.b("Entity clusters"),
        _ => true,
    }
}

// What a block state is to the scan: on which list (0 for none) as which kind, and the signs it can be.
#[derive(Clone, Copy, Default)]
struct Look {
    list: u8,
    kind: u16,
    sign: u8,
}

const AIR: u8 = 1;
const PORTAL: u8 = 2;
const BUBBLE: u8 = 3;
const BEDROCK: u8 = 4;
const MUSHROOM: u8 = 5;
const SPAWNER: u8 = 6;
const COMPANION: u8 = 7;
const END_PORTAL: u8 = 8;

// What the scan thread is told once: the table, the lists' names and counts, and which signs to look for where.
struct Plan {
    looks: Vec<Look>,
    names: Vec<String>,
    counts: [u32; 7],
    dim: &'static str,
    portals: bool,
    bubbles: bool,
    sky: Option<i32>,
    bedrock: Option<i32>,
    spawners: bool,
    roof: bool,
}

// A chunk with a sign of a base: why, where to look, and the most common listed kinds with their counts.
struct Found {
    chunk: (i32, i32),
    why: String,
    at: [i32; 3],
    kinds: Vec<(String, u32)>,
}

// A scan for one dimension and one set of settings: the chunks sent to the thread (the loaded ones), the ones found,
// those just found, the chat lines waiting for the cooldown, and the thread's way in and out. Dropping it ends the
// thread.
struct Scan {
    key: String,
    revision: u64,
    sent: HashSet<(i32, i32)>,
    said: HashSet<(i32, i32)>,
    fresh: Vec<Found>,
    lines: VecDeque<String>,
    cooldown: u32,
    entity_timer: u32,
    jobs: Sender<Arc<Chunk>>,
    found: Receiver<Found>,
}

static SCAN: Mutex<Option<Scan>> = Mutex::new(None);

fn scan<R>(f: impl FnOnce(&mut Option<Scan>) -> R) -> R {
    f(&mut SCAN.lock().unwrap_or_else(|e| e.into_inner()))
}

// The scan thread: each chunk's blocks counted by list and kind, and the signs looked for; a chunk with any sent back.
fn thread(plan: Plan, jobs: Receiver<Arc<Chunk>>, found: Sender<Found>) {
    let mut states = [0u32; 4096];
    let nether = plan.dim == "minecraft:the_nether";
    let overworld = plan.dim == "minecraft:overworld";
    for chunk in jobs {
        let mut per_list = [0u32; 7];
        let mut per_kind = vec![0u32; plan.names.len()];
        let mut sums = [[0i64; 3]; 7];
        let mut sign: Option<(&str, [i32; 3])> = None;
        let mut spawner: Option<[i32; 3]> = None;
        let mut companion = false;
        for (i, section) in chunk.sections.iter().enumerate() {
            let y0 = chunk.min_y() + i as i32 * 16;
            // A section all of one state: air, stone, water... only its first block needs looking at.
            let first = section.get(0);
            if section.is_uniform(first) && plan.looks.get(first as usize).is_none_or(|l| l.list == 0 && l.sign <= AIR) {
                if overworld && plan.sky.is_some_and(|y| y0 + 15 > y) && plan.looks.get(first as usize).is_none_or(|l| l.sign != AIR) {
                    sign = sign.or(Some(("a build above the terrain", [chunk.x * 16, y0, chunk.z * 16])));
                }
                if nether && plan.roof && y0 >= 128 && plan.looks.get(first as usize).is_none_or(|l| l.sign != AIR) {
                    sign = sign.or(Some(("a build on the nether roof", [chunk.x * 16, y0, chunk.z * 16])));
                }
                continue;
            }
            section.unpack(&mut states);
            for (j, &v) in states.iter().enumerate() {
                let look = plan.looks.get(v as usize).copied().unwrap_or_default();
                let (x, y, z) = (chunk.x * 16 + (j as i32 & 15), y0 + (j as i32 >> 8), chunk.z * 16 + (j as i32 >> 4 & 15));
                if look.list > 0 {
                    let l = usize::from(look.list - 1);
                    per_list[l] += 1;
                    per_kind[usize::from(look.kind)] += 1;
                    sums[l] = [sums[l][0] + i64::from(x), sums[l][1] + i64::from(y), sums[l][2] + i64::from(z)];
                }
                if sign.is_some() && look.sign != SPAWNER && look.sign != COMPANION {
                    continue;
                }
                let why = match look.sign {
                    PORTAL if plan.portals => Some("an open nether portal"),
                    END_PORTAL if plan.portals && overworld => Some("an open end portal"),
                    BUBBLE if plan.bubbles => Some("a soul sand bubble column"),
                    BEDROCK if plan.bedrock.is_some_and(|n| {
                        if overworld {
                            y >= chunk.min_y() + n
                        } else {
                            nether && (n..128 - n).contains(&y)
                        }
                    }) => Some("bedrock where none grows"),
                    SPAWNER => {
                        spawner = spawner.or(Some([x, y, z]));
                        None
                    }
                    COMPANION => {
                        companion = true;
                        None
                    }
                    AIR => None,
                    // Anything else up high: a build over the terrain in the overworld, on the nether's roof.
                    s => {
                        if overworld && plan.sky.is_some_and(|top| y > top) {
                            Some("a build above the terrain")
                        } else if nether && plan.roof && y >= 128 && s != MUSHROOM {
                            Some("a build on the nether roof")
                        } else {
                            None
                        }
                    }
                };
                if let Some(why) = why {
                    sign = Some((why, [x, y, z]));
                }
            }
        }
        if plan.spawners && !companion && sign.is_none() && let Some(at) = spawner {
            sign = Some(("a spawner with none of its structure", at));
        }
        let tier = (0..7).find(|&l| plan.counts[l] > 0 && per_list[l] >= plan.counts[l]);
        let (why, at) = match (tier, sign) {
            (Some(l), _) => {
                let n = i64::from(per_list[l]);
                let total: u32 = per_list.iter().sum();
                (format!("{total} placed block{}", if total == 1 { "" } else { "s" }), [(sums[l][0] / n) as i32, (sums[l][1] / n) as i32, (sums[l][2] / n) as i32])
            }
            (None, Some((why, at))) => (why.to_owned(), at),
            (None, None) => continue,
        };
        let mut kinds: Vec<(String, u32)> = plan.names.iter().cloned().zip(per_kind).filter(|(_, c)| *c > 0).collect();
        kinds.sort_by(|a, b| b.1.cmp(&a.1));
        kinds.truncate(4);
        if found.send(Found { chunk: (chunk.x, chunk.z), why, at, kinds }).is_err() {
            return;
        }
    }
}

// Base Finder's own waypoints, "Base x z": a waypoint of yours called "Base" or "Base camp" is not one.
pub(super) fn base_point(name: &str) -> bool {
    name.strip_prefix("Base ")
        .and_then(|xz| xz.split_once(' '))
        .is_some_and(|(x, z)| x.parse::<i32>().is_ok() && z.parse::<i32>().is_ok())
}

// A block's short name on a list: by name, or a "*_bed" entry by its end (so wall and hanging signs are signs).
fn entry(list: &[&str], name: &str) -> Option<usize> {
    let name = short(name);
    list.iter().position(|x| x.strip_prefix('*').map_or(*x == name, |end| name.ends_with(end)))
}

// The scan's plan from the settings: every block state looked up once.
fn plan(game: &Game<'_>, m: &Module) -> Plan {
    let states = &game.res.facts().states;
    let registry = &game.pack.assets.registry;
    let lists: Vec<Vec<&str>> =
        LISTS.iter().map(|(list, ..)| m.t(list).split(',').map(str::trim).filter(|x| !x.is_empty()).collect()).collect();
    let mut names: Vec<String> = Vec::new();
    let looks = states
        .iter()
        .enumerate()
        .map(|(id, s)| {
            let name = short(&s.name);
            let sign = match name {
                "air" | "cave_air" | "void_air" => AIR,
                "nether_portal" => PORTAL,
                "end_portal" => END_PORTAL,
                "bubble_column"
                    if registry.state(BlockStateId(id as u32)).and_then(|b| b.properties.get("drag")).is_some_and(|d| d == "false") =>
                {
                    BUBBLE
                }
                "bedrock" => BEDROCK,
                "red_mushroom" | "brown_mushroom" => MUSHROOM,
                "spawner" => SPAWNER,
                // What a dungeon, mineshaft, fortress, stronghold or bastion spawner has around it.
                "mossy_cobblestone" | "cobweb" | "nether_bricks" | "nether_brick_fence" | "stone_bricks" | "mossy_stone_bricks"
                | "cracked_stone_bricks" | "end_portal_frame" | "polished_blackstone_bricks" | "cracked_polished_blackstone_bricks"
                | "blackstone" | "gilded_blackstone" => COMPANION,
                _ => 0,
            };
            let Some((l, k)) = lists.iter().enumerate().find_map(|(l, list)| entry(list, name).map(|k| (l, k))) else {
                return Look { sign, ..Look::default() };
            };
            let label = lists[l][k].trim_start_matches("*_").replace('_', " ");
            let kind = names.iter().position(|n| *n == label).unwrap_or_else(|| {
                names.push(label);
                names.len() - 1
            });
            Look { list: l as u8 + 1, kind: kind as u16, sign }
        })
        .collect();
    let mut counts = [0u32; 7];
    for (c, (_, count, ..)) in counts.iter_mut().zip(LISTS) {
        *c = m.n(count) as u32;
    }
    Plan {
        looks,
        names,
        counts,
        dim: game.dim.id(),
        portals: m.b("Open portals"),
        bubbles: m.b("Bubble columns"),
        sky: m.b("Sky builds").then(|| m.n("Sky build Y") as i32),
        bedrock: m.b("Bedrock").then(|| m.n("Bedrock layers") as i32),
        spawners: m.b("Lone spawners"),
        roof: m.b("Nether roof"),
    }
}

// What the scan depends on: the dimension and the settings the thread reads (the lists and the block signs).
const BLOCK_SIGNS: [&str; 8] = ["Open portals", "Bubble columns", "Sky builds", "Sky build Y", "Bedrock", "Bedrock layers", "Lone spawners", "Nether roof"];

fn key(game: &Game<'_>, m: &Module) -> String {
    let mut k = game.dim.id().to_owned();
    for s in m.settings.iter().filter(|s| s.name.starts_with("List ") || BLOCK_SIGNS.contains(&s.name)) {
        k.push_str(&format!("|{:?}", s.value));
    }
    k
}

// Each tick, with Base Finder on (`m`) or off (None).
pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m.filter(|_| in_world(game)) else {
        if m.is_none() {
            // Off: stop scanning and take its waypoints away, saved ones from earlier sessions included.
            scan(|s| *s = None);
            super::sunset::remove_waypoints(game, base_point);
        }
        return;
    };
    let key = key(game, m);
    if scan(|s| s.as_ref().is_none_or(|s| s.key != key)) {
        let plan = plan(game, m);
        let (jobs, rx) = channel();
        let (tx, found) = channel();
        if std::thread::Builder::new().name("sunset-base-finder".into()).spawn(move || thread(plan, rx, tx)).is_err() {
            return;
        }
        scan(|s| {
            *s = Some(Scan { key, revision: 0, sent: HashSet::new(), said: HashSet::new(), fresh: Vec::new(), lines: VecDeque::new(), cooldown: 0, entity_timer: 0, jobs, found })
        });
    }
    let revision = game.world.chunks.membership_revision();
    let entity_scan = scan(|s| {
        let Some(s) = s.as_mut() else {
            return false;
        };
        if s.revision != revision {
            s.revision = revision;
            s.sent.retain(|c| game.world.chunks.contains_key(c));
            for (c, chunk) in game.world.chunks.iter() {
                if s.sent.insert(*c) {
                    let _ = s.jobs.send(chunk.clone());
                }
            }
        }
        let found: Vec<Found> = s.found.try_iter().collect();
        for f in found {
            if s.said.insert(f.chunk) {
                s.fresh.push(f);
            }
        }
        s.entity_timer = s.entity_timer.saturating_sub(1);
        s.entity_timer == 0
    });
    if entity_scan {
        let found = entity_signs(game, m);
        scan(|s| {
            if let Some(s) = s.as_mut() {
                s.entity_timer = m.n("Entity scan delay") as u32;
                for f in found {
                    if s.said.insert(f.chunk) {
                        s.fresh.push(f);
                    }
                }
            }
        });
    }
    // What is new and not near spawn gets its waypoint at once; its chat line waits for the cooldown.
    let near = m.n("Ignore near spawn");
    let spawn = game.spawn.filter(|_| game.dim.id() == "minecraft:overworld").map(|s| [s.x, s.z]);
    let fresh: Vec<Found> = scan(|s| s.as_mut().map(|s| s.fresh.drain(..).collect()).unwrap_or_default());
    for f in fresh {
        if spawn.is_some_and(|[sx, sz]| (f64::from(f.at[0]) - sx).hypot(f64::from(f.at[2]) - sz) < near) {
            continue;
        }
        let [x, y, z] = f.at;
        if m.b("Waypoints") {
            super::sunset::add_waypoint(game, format!("Base {x} {z}"), [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5]);
        }
        let what: Vec<String> = f.kinds.iter().map(|(n, c)| format!("{c} {n}")).collect();
        let list = if what.is_empty() { String::new() } else { format!(": {}", what.join(", ")) };
        let line = format!("Base? {} in chunk {} {} around {x} {y} {z}{list}", f.why, f.chunk.0, f.chunk.1);
        scan(|s| s.as_mut().map(|s| s.lines.push_back(line)));
    }
    let line = scan(|s| {
        let s = s.as_mut()?;
        s.cooldown = s.cooldown.saturating_sub(1);
        if s.cooldown > 0 {
            return None;
        }
        s.cooldown = m.n("Message cooldown") as u32;
        s.lines.pop_front()
    });
    if let Some(line) = line {
        report(game, &line);
    }
}

// The signs that are not blocks: written signs (block entities), and the entities no world spawns.
fn entity_signs(game: &Game<'_>, m: &Module) -> Vec<Found> {
    let mut out = Vec::new();
    let mut add = |why: &str, at: [f64; 3]| {
        let at = at.map(|v| v.floor() as i32);
        out.push(Found { chunk: (at[0] >> 4, at[2] >> 4), why: why.to_owned(), at, kinds: Vec::new() });
    };
    if m.b("Written signs") {
        for (&(x, y, z), entity) in &game.world.block_entities {
            let BlockEntity::Sign(sign) = entity else {
                continue;
            };
            if sign.front.lines.iter().chain(&sign.back.lines).any(|l| !l.trim().is_empty()) {
                add("a written sign", [f64::from(x), f64::from(y), f64::from(z)]);
            }
        }
    }
    if m.b("Item frames") {
        // End ships hang an elytra in one; any other framed (or empty) frame was put up.
        for f in game.item_frames.iter().filter(|f| f.item.as_ref().is_none_or(|i| short(&i.item) != "elytra")) {
            add("an item frame", f.pos.map(f64::from));
        }
    }
    if m.b("Ender pearls") {
        // A pearl at rest (a stasis chamber's); thrown ones fly.
        for p in &game.creatures.projectiles {
            let v = p.velocity;
            if p.kind == horizon_client::projectiles::ProjectileKind::EnderPearl && (v.x * v.x + v.y * v.y + v.z * v.z) < 0.01 {
                add("a resting ender pearl", [p.position.x, p.position.y, p.position.z]);
            }
        }
    }
    if m.b("Boats") {
        for b in game.boats.iter().filter(|b| Some(b.id) != game.riding) {
            let bb = b.bounds();
            let (lo, hi) = (bb.min_position(), bb.max_position());
            add("a boat", [(lo.x + hi.x) / 2.0, lo.y, (lo.z + hi.z) / 2.0]);
        }
    }
    let me = super::sunset::own_id(game);
    let crowd = m.b("Entity clusters").then(|| m.t("Cluster entities"));
    let mut crowds: Vec<((i32, i32), u32, [f64; 3])> = Vec::new();
    for mob in game.mobs.iter().filter(|x| x.alive() && !x.removed && Some(x.id) != me) {
        let at = [mob.position.x, mob.position.y, mob.position.z];
        let kind = short(&mob.kind.id);
        if kind == "player" {
            continue;
        }
        if m.b("Name tags") && mob.custom_name.is_some() {
            add("a name-tagged mob", at);
        }
        if m.b("Traded villagers") && kind == "villager" && mob.extra.variant3 > 1 {
            add("a villager that has traded", at);
        }
        if crowd.is_some_and(|list| listed(list, kind)) {
            let c = ((at[0].floor() as i32) >> 4, (at[2].floor() as i32) >> 4);
            match crowds.iter_mut().find(|(k, ..)| *k == c) {
                Some(e) => e.1 += 1,
                None => crowds.push((c, 1, at)),
            }
        }
    }
    for (_, n, at) in crowds {
        if f64::from(n) >= m.n("Cluster size") {
            add(&format!("{n} animals together"), at);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{base_point, entry};

    #[test]
    fn base_finder_claims_only_its_own_waypoints() {
        assert!(base_point("Base 376 15") && base_point("Base -21 -13"));
        assert!(!base_point("Base") && !base_point("Base camp") && !base_point("Base 1 2 3") && !base_point("Home 1 2"));
    }

    #[test]
    fn list_entries_match_by_name_or_end() {
        let list = ["chest", "*_bed", "*_sign"];
        assert_eq!(entry(&list, "minecraft:chest"), Some(0));
        assert_eq!(entry(&list, "minecraft:trapped_chest"), None);
        assert_eq!(entry(&list, "minecraft:red_bed"), Some(1));
        assert_eq!(entry(&list, "minecraft:oak_wall_sign"), Some(2));
    }
}
