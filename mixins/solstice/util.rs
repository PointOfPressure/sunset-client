// Shared helpers for Solstice's modules, ported from Meteor's BlockUtils,
// InvUtils, PlayerUtils and TargetUtils: placing blocks, finding items in the
// hotbar, picking targets, seeing a block through the world.

use super::solstice::{Camera, Module, Shape, hold, look_at, outline, say, toggle, with};
use super::{Game, Screen};
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};

// Block placements count up from here; the game's own are small.
static SEQUENCE: AtomicI32 = AtomicI32::new(1_500_000_000);

pub(super) fn sequence() -> i32 {
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

// Boxes the modules want drawn this frame: where they will place or break.
static SHOW: Mutex<Vec<([i32; 3], u32)>> = Mutex::new(Vec::new());

pub(super) fn show_clear() {
    SHOW.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

pub(super) fn show(pos: [i32; 3], argb: u32) {
    let mut v = SHOW.lock().unwrap_or_else(|e| e.into_inner());
    if v.len() < 600 {
        v.push((pos, argb));
    }
}

pub(super) fn draw_shown(cam: &Camera, out: &mut Vec<Shape>, lw: f32) {
    let v = SHOW.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for (p, c) in v {
        let lo = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
        if let Some(r) = cam.rect(lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0]) {
            outline(out, r, c, lw, true);
        }
    }
}

// ---- names and lists

pub(super) fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

// Whether `name` is in a comma separated list (`obsidian, minecraft:anvil`).
pub(super) fn listed(list: &str, name: &str) -> bool {
    let name = short(name);
    list.split(',').any(|x| short(x.trim()) == name)
}

pub(super) fn toggle_off(game: &mut Game<'_>, name: &str) {
    if let Some(i) = with(|s| s.find(name)) {
        if with(|s| s.modules[i].on) {
            toggle(game, i, false);
        }
    }
}

// ---- blocks

pub(super) fn facts<'a>(game: &'a Game<'_>, p: [i32; 3]) -> &'a horizon_items::blocks::StateFacts {
    game.res.facts().get(game.world.block(p[0], p[1], p[2]))
}

pub(super) fn is_air(game: &Game<'_>, p: [i32; 3]) -> bool {
    game.res.facts().is_air(game.world.block(p[0], p[1], p[2]))
}

// `BlockState.canBeReplaced`: air, fluids, plants.
pub(super) fn replaceable(game: &Game<'_>, p: [i32; 3]) -> bool {
    is_air(game, p) || facts(game, p).replaceable
}

pub(super) fn center_of(p: [i32; 3]) -> [f64; 3] {
    [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5]
}

pub(super) fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub(super) fn player_pos(game: &Game<'_>) -> Option<[f64; 3]> {
    let p = game.player.as_ref()?;
    Some([p.position.x, p.position.y, p.position.z])
}

pub(super) fn block_pos(game: &Game<'_>) -> Option<[i32; 3]> {
    let p = player_pos(game)?;
    Some([p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32])
}

// `PlayerUtils.centerPlayer`: on the middle of its block. The move packet of
// the tick carries it (the server takes one a tick).
pub(super) fn center_player(game: &mut Game<'_>) {
    if let Some(p) = game.player.as_mut() {
        p.position.x = p.position.x.floor() + 0.5;
        p.position.z = p.position.z.floor() + 0.5;
    }
}

// A solid-ish block in the way between the eyes and `to`, sampled along the line.
pub(super) fn clear_line(game: &Game<'_>, to: [f64; 3], except: [i32; 3]) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let d = dist(eye, to);
    let steps = (d * 8.0).ceil().max(1.0) as i32;
    for i in 1..steps {
        let t = f64::from(i) / f64::from(steps);
        let at = [
            (eye[0] + (to[0] - eye[0]) * t).floor() as i32,
            (eye[1] + (to[1] - eye[1]) * t).floor() as i32,
            (eye[2] + (to[2] - eye[2]) * t).floor() as i32,
        ];
        if at != except && !is_air(game, at) && facts(game, at).full_collision {
            return false;
        }
    }
    true
}

// Meteor's `isOutOfRange`, the other way round: within `range` of the eyes,
// and either in plain sight or within `walls`.
pub(super) fn in_reach(game: &Game<'_>, p: [i32; 3], range: f64, walls: f64) -> bool {
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let c = center_of(p);
    let d = dist(eye, c);
    if d > range {
        return false;
    }
    clear_line(game, c, p) || d <= walls
}

// ---- placing

// The directions a block can be clicked from: where the neighbour is, and the
// face of it that points back.
const SIDES: [([i32; 3], i32); 6] = [
    ([0, -1, 0], 1),
    ([0, 1, 0], 0),
    ([0, 0, -1], 3),
    ([0, 0, 1], 2),
    ([-1, 0, 0], 5),
    ([1, 0, 0], 4),
];

// `BlockUtils.getPlaceSide`: a neighbour to click, the one the eyes face most.
fn place_side(game: &Game<'_>, p: [i32; 3]) -> Option<([i32; 3], i32)> {
    let eye = game.eye_position()?;
    let c = center_of(p);
    let look = [c[0] - eye[0], c[1] - eye[1], c[2] - eye[2]];
    let mut best: Option<(f64, usize)> = None;
    for (i, (d, _)) in SIDES.iter().enumerate() {
        let n = [p[0] + d[0], p[1] + d[1], p[2] + d[2]];
        if is_air(game, n) {
            continue;
        }
        let f = facts(game, n);
        // A block you would open instead of placing against, or a fluid.
        if f.block_entity || f.fluid_level > 0 {
            continue;
        }
        let relevancy = look[0] * f64::from(d[0]) + look[1] * f64::from(d[1]) + look[2] * f64::from(d[2]);
        if best.is_none_or(|(b, _)| relevancy > b) {
            best = Some((relevancy, i));
        }
    }
    best.map(|(_, i)| {
        let (d, face) = SIDES[i];
        ([p[0] + d[0], p[1] + d[1], p[2] + d[2]], face)
    })
}

pub(super) fn has_support(game: &Game<'_>, p: [i32; 3]) -> bool {
    place_side(game, p).is_some()
}

// Entities that stop a block going in: the player and the living things.
fn obstructed(game: &Game<'_>, p: [i32; 3]) -> bool {
    let lo = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
    let hi = [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0];
    let hits = |a: horizon_core::geometry::Aabb| {
        let (l, h) = (a.min_position(), a.max_position());
        l.x < hi[0] && h.x > lo[0] && l.y < hi[1] && h.y > lo[1] && l.z < hi[2] && h.z > lo[2]
    };
    if game.player.as_ref().is_some_and(|pl| hits(pl.bounds())) {
        return true;
    }
    game.mobs.iter().any(|m| m.alive() && !m.removed && hits(m.bounds()))
}

// `BlockUtils.canPlaceBlock`.
pub(super) fn can_place(game: &Game<'_>, p: [i32; 3], check_entities: bool) -> bool {
    p[1] >= -64 && p[1] < 320 && replaceable(game, p) && (!check_entities || !obstructed(game, p))
}

// `BlockUtils.place`: puts the block in hotbar `slot` at `p`, clicking a
// neighbour (or the position itself when there is none, an air place).
pub(super) fn place_block(game: &mut Game<'_>, p: [i32; 3], slot: usize, rotate: bool, check_entities: bool) -> bool {
    if slot > 8 || game.remote.is_none() || !can_place(game, p, check_entities) {
        return false;
    }
    let Some(eye) = game.eye_position() else {
        return false;
    };
    let (neighbor, face) = place_side(game, p).unwrap_or((p, 1));
    let hit = if neighbor == p {
        let c = center_of(p);
        [c[0], c[1] + 0.5, c[2]]
    } else {
        let c = center_of(p);
        let d = [neighbor[0] - p[0], neighbor[1] - p[1], neighbor[2] - p[2]];
        [c[0] + f64::from(d[0]) * 0.5, c[1] + f64::from(d[1]) * 0.5, c[2] + f64::from(d[2]) * 0.5]
    };
    let cursor = [
        (hit[0] - f64::from(neighbor[0])) as f32,
        (hit[1] - f64::from(neighbor[1])) as f32,
        (hit[2] - f64::from(neighbor[2])) as f32,
    ];
    if rotate {
        let (yaw, pitch) = look_at(eye, hit);
        game.send(S::MovePlayerRot(play::MovePlayerRot {
            yaw,
            pitch,
            flags: u8::from(game.player.as_ref().is_some_and(|pl| pl.ground.on_ground)),
        }));
    }
    let before = game.selected;
    hold(game, slot);
    game.send(S::UseItemOn(play::UseItemOn {
        hand: 0,
        pos: neighbor,
        face,
        cursor,
        inside: false,
        world_border: false,
        sequence: sequence(),
    }));
    game.swing_arm();
    game.send(S::Punch(Empty));
    hold(game, before);
    true
}

// ---- items

// The first hotbar slot whose item matches.
pub(super) fn find_hotbar(game: &Game<'_>, want: impl Fn(&str) -> bool) -> Option<usize> {
    (0..9).find(|&i| {
        game.inventory
            .get(i)
            .and_then(|s| s.as_ref())
            .is_some_and(|s| want(short(&s.item)))
    })
}

// Anywhere in the inventory (0 to 35): hotbar first.
pub(super) fn find_inventory(game: &Game<'_>, want: impl Fn(&str) -> bool) -> Option<usize> {
    (0..36).find(|&i| {
        game.inventory
            .get(i)
            .and_then(|s| s.as_ref())
            .is_some_and(|s| want(short(&s.item)))
    })
}

pub(super) fn count_items(game: &Game<'_>, want: impl Fn(&str) -> bool) -> u32 {
    let own: u32 = game
        .inventory
        .iter()
        .flatten()
        .filter(|s| want(short(&s.item)))
        .map(|s| s.count)
        .sum();
    own + game.offhand.as_ref().filter(|s| want(short(&s.item))).map_or(0, |s| s.count)
}

// ---- targets

#[derive(Clone, Debug)]
pub(super) struct Target {
    pub id: u32,
    pub name: String,
    pub pos: [f64; 3],
    pub prev: [f64; 3],
    pub health: f32,
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub player: bool,
    pub kind: String,
}

pub(super) fn mob_target(m: &horizon_client::mobs::Mob, name: Option<String>) -> Target {
    let b = m.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    Target {
        id: m.id,
        name: name.unwrap_or_else(|| short(&m.kind.id).replace('_', " ")),
        pos: [m.position.x, m.position.y, m.position.z],
        prev: [m.previous.x, m.previous.y, m.previous.z],
        health: m.health,
        lo: [lo.x, lo.y, lo.z],
        hi: [hi.x, hi.y, hi.z],
        player: m.kind.id == "minecraft:player",
        kind: m.kind.id.to_string(),
    }
}

// Every other player the client knows of, friends left out.
pub(super) fn other_players(game: &Game<'_>) -> Vec<Target> {
    let names = game.remote_player_names();
    let friends = with(|s| s.friends.clone());
    game.mobs
        .iter()
        .filter(|m| m.kind.id == "minecraft:player" && m.alive() && !m.removed)
        .filter_map(|m| {
            let name = names.iter().find(|(i, _)| *i == m.id).map(|(_, n)| n.clone())?;
            if friends.iter().any(|f| f.eq_ignore_ascii_case(&name)) {
                return None;
            }
            Some(mob_target(m, Some(name)))
        })
        .collect()
}

pub(super) fn every_living(game: &Game<'_>) -> Vec<Target> {
    let names = game.remote_player_names();
    game.mobs
        .iter()
        .filter(|m| m.alive() && !m.removed)
        .map(|m| mob_target(m, names.iter().find(|(i, _)| *i == m.id).map(|(_, n)| n.clone())))
        .collect()
}

pub(super) const PRIORITIES: &[&str] = &["Lowest distance", "Highest distance", "Lowest health", "Closest angle"];

// `TargetUtils.getPlayerTarget`: the best player within `range` by priority.
pub(super) fn player_target(game: &Game<'_>, range: f64, priority: &str) -> Option<Target> {
    let eye = game.eye_position()?;
    let mut list: Vec<Target> = other_players(game)
        .into_iter()
        .filter(|t| t.health > 0.0 && dist(eye, t.pos) <= range)
        .collect();
    sort_targets(game, &mut list, priority);
    list.into_iter().next()
}

pub(super) fn sort_targets(game: &Game<'_>, list: &mut [Target], priority: &str) {
    let Some(eye) = game.eye_position() else {
        return;
    };
    let angle = |t: &Target| {
        let (yaw, pitch) = look_at(eye, [t.pos[0], t.pos[1] + (t.hi[1] - t.lo[1]) / 2.0, t.pos[2]]);
        let dy = ((yaw - game.yaw + 180.0).rem_euclid(360.0) - 180.0).abs();
        f64::from(dy + (pitch - game.pitch).abs())
    };
    match priority {
        "Highest distance" => list.sort_by(|a, b| dist(eye, b.pos).total_cmp(&dist(eye, a.pos))),
        "Lowest health" => list.sort_by(|a, b| a.health.total_cmp(&b.health)),
        "Closest angle" => list.sort_by(|a, b| angle(a).total_cmp(&angle(b))),
        _ => list.sort_by(|a, b| dist(eye, a.pos).total_cmp(&dist(eye, b.pos))),
    }
}

// Whether a player stands in a hole: solid all around its feet.
pub(super) fn is_surrounded(game: &Game<'_>, t: &Target) -> bool {
    let p = [t.pos[0].floor() as i32, t.pos[1].floor() as i32, t.pos[2].floor() as i32];
    [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]]
        .iter()
        .all(|d| !replaceable(game, [p[0] + d[0], p[1] + d[1], p[2] + d[2]]))
}

// A comma list's choices as Pick values need 'static: these are the shared ones.
pub(super) const TRAP_BLOCKS: &str = "obsidian,crying_obsidian,netherite_block";

pub(super) fn report(game: &mut Game<'_>, msg: &str) {
    say(game, msg);
}

pub(super) fn inventory_idle(game: &Game<'_>) -> bool {
    super::solstice::inventory_idle(game)
}

pub(super) fn in_world(game: &Game<'_>) -> bool {
    game.native_playing() && game.remote.is_some() && game.player.is_some() && game.screen == Screen::Playing
}

// Keeps `Module` in use for the category files that only take it by reference.
pub(super) fn enabled(mods: &[Module], name: &str) -> Option<Module> {
    mods.iter().find(|m| m.name == name && m.on).cloned()
}
