// Shared helpers for Sunset's modules, ported from Meteor's BlockUtils,
// InvUtils, PlayerUtils and TargetUtils: placing blocks, finding items in the
// hotbar, picking targets, seeing a block through the world.

use super::sunset::{Camera, Canvas, Module, hold, look_at, say, toggle, turn_then, with};
use super::sunset_visual::{Look, draw_box};
use super::{Game, Screen};
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use std::sync::Mutex;

// The next block-action sequence number, from the game's own counter.
pub(super) fn sequence(game: &mut Game<'_>) -> i32 {
    super::remote::sunset_remote::next(game)
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

pub(super) fn draw_shown(cam: &Camera, out: &mut impl Canvas, lw: f32) {
    for &(p, c) in SHOW.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        let lo = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
        draw_box(cam, out, &Look::BOXED, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], c, lw);
    }
}

// ---- block searches spread over ticks

// About how many blocks a search reads in one tick: Block ESP's whole cube (65 blocks a side) takes a second instead
// of one long tick.
const SWEEP_BUDGET: usize = 12_000;
// What a search keeps, nearest first; the drawing stops at the module's Max long before.
const SWEEP_KEEP: usize = 16_384;

// A search of the blocks round the player (Block ESP, Hole ESP, Tunnel ESP, Void ESP, Light Overlay), a slice of
// columns (one x) at a time over as many ticks as it takes; each whole pass replaces what the last one found.
pub(super) struct Sweep<V> {
    // The last whole pass: where, and what the search noted there, nearest its centre first.
    pub(super) found: Vec<([i32; 3], V)>,
    next: Vec<([i32; 3], V)>,
    centre: [i32; 3],
    radius: i32,
    // The next slice while a pass runs.
    x: Option<i32>,
    // The settings the search is for, and the tick its last pass ended (0: none yet).
    key: u64,
    done: u64,
}

impl<V> Default for Sweep<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Sweep<V> {
    pub(super) const fn new() -> Self {
        Self { found: Vec::new(), next: Vec::new(), centre: [0; 3], radius: 0, x: None, key: 0, done: 0 }
    }

    // One tick of the search round `at`, `radius` slices each way. A pass starts `every` ticks after the last one
    // ended, when the player is `moved` blocks from where it was made, or at once when `key` (what it looks for)
    // changes. `slice(centre, x, out)` searches the slice at `x` and says how many blocks it read. True when a pass
    // ended this tick.
    pub(super) fn tick(
        &mut self,
        ticks: u64,
        at: [i32; 3],
        radius: i32,
        key: u64,
        every: u64,
        moved: i32,
        mut slice: impl FnMut([i32; 3], i32, &mut Vec<([i32; 3], V)>) -> usize,
    ) -> bool {
        if key != self.key {
            self.found.clear();
            self.x = None;
        }
        if self.x.is_none() {
            let far = (0..3).any(|i| (at[i] - self.centre[i]).abs() >= moved);
            if self.done != 0 && key == self.key && ticks.saturating_sub(self.done) < every && !far {
                return false;
            }
            (self.key, self.centre, self.radius, self.x) = (key, at, radius, Some(at[0] - radius));
            self.next.clear();
        }
        let mut read = 0;
        while let Some(x) = self.x.filter(|x| *x <= self.centre[0] + self.radius) {
            read += slice(self.centre, x, &mut self.next);
            self.x = Some(x + 1);
            if self.next.len() > 2 * SWEEP_KEEP {
                self.nearest();
            }
            if read >= SWEEP_BUDGET {
                return false;
            }
        }
        self.nearest();
        std::mem::swap(&mut self.found, &mut self.next);
        self.next.clear();
        self.done = ticks.max(1);
        self.x = None;
        true
    }

    // The pass so far, nearest first and cut to what is kept.
    fn nearest(&mut self) {
        let c = self.centre;
        let far = |p: &[i32; 3]| (0..3).map(|i| i64::from(p[i] - c[i]).pow(2)).sum::<i64>();
        self.next.sort_unstable_by_key(|(p, _)| far(p));
        self.next.truncate(SWEEP_KEEP);
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

// The block the feet are in: on a slab, soul sand, a path, carpet or snow, the block of the position is the one stood
// on, so it is the one above.
pub(super) fn feet_block(game: &Game<'_>) -> Option<[i32; 3]> {
    let p = block_pos(game)?;
    Some(if replaceable(game, p) { p } else { [p[0], p[1] + 1, p[2]] })
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
pub(super) fn place_side(game: &Game<'_>, p: [i32; 3]) -> Option<([i32; 3], i32)> {
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

// `BlockUtils.place`: puts the block in hotbar `slot` (or the offhand's, slot OFFHAND) at `p`, clicking a
// neighbour (or the position itself when there is none, an air place).
pub(super) fn place_block(game: &mut Game<'_>, p: [i32; 3], slot: usize, rotate: bool, check_entities: bool) -> bool {
    if (slot > 8 && slot != OFFHAND) || game.remote.is_none() || !can_place(game, p, check_entities) {
        return false;
    }
    if game.eye_position().is_none() {
        return false;
    }
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
    turned_to(game, "Place", rotate, hit, move |game| {
        let before = game.selected;
        let offhand = slot == OFFHAND;
        if !offhand {
            hold(game, slot);
        }
        let seq = sequence(game);
        game.send(S::UseItemOn(play::UseItemOn {
            hand: i32::from(offhand),
            pos: neighbor,
            face,
            cursor,
            inside: false,
            world_border: false,
            sequence: seq,
        }));
        game.swing_arm();
        game.send(S::Punch(Empty));
        if !offhand {
            hold(game, before);
        }
    });
    true
}

// Does `deed`, after the server's look has turned to `at` when `rotate` says so (see turn_then).
pub(super) fn turned_to(
    game: &mut Game<'_>,
    who: &'static str,
    rotate: bool,
    at: [f64; 3],
    deed: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static,
) {
    match game.eye_position().filter(|_| rotate) {
        Some(eye) => {
            let (yaw, pitch) = look_at(eye, at);
            turn_then(who, yaw, pitch, deed);
        }
        None => deed(game),
    }
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

// The offhand, as a slot place_block takes.
pub(super) const OFFHAND: usize = 40;

// `InvUtils.findInHotbar`: the offhand first, then the hand, then the rest of the hotbar.
pub(super) fn find_held(game: &Game<'_>, want: impl Fn(&str) -> bool) -> Option<usize> {
    if game.offhand.as_ref().is_some_and(|s| want(short(&s.item))) {
        return Some(OFFHAND);
    }
    let hand = game.inventory.get(game.selected).and_then(|s| s.as_ref()).is_some_and(|s| want(short(&s.item)));
    if hand { Some(game.selected) } else { find_hotbar(game, want) }
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

// ---- targets

#[derive(Clone, Debug)]
pub(super) struct Target {
    pub id: u32,
    pub pos: [f64; 3],
    pub prev: [f64; 3],
    pub health: f32,
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub player: bool,
    pub kind: String,
}

pub(super) fn mob_target(m: &horizon_client::mobs::Mob) -> Target {
    let b = m.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    Target {
        id: m.id,
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
            Some(mob_target(m))
        })
        .collect()
}

// Shared enemy predicate for the native combat controllers. A retained profile
// name is insufficient: require the current player-list entry and its mode.
pub(super) fn player_enemy(game: &Game<'_>, mob: &horizon_client::mobs::Mob) -> bool {
    if mob.kind.id != "minecraft:player" || !mob.health.is_finite() || !mob.alive()
        || mob.removed || Some(mob.id) == super::sunset::own_id(game) || game.riding == Some(mob.id)
        || !matches!(super::remote::sunset_combat_remote::player_mode(game, mob.id), Some(0 | 2)) {
        return false;
    }
    let Some(name) = game.remote_player_name(mob.id) else { return false; };
    if with(|s| s.friends.iter().any(|friend| friend.eq_ignore_ascii_case(name))) {
        return false;
    }
    horizon_client::scoreboard::teams_allow_damage(
        game.score.board.players_team(&game.options.player_name()), game.mob_team(mob.id),
    )
}
pub(super) fn setting_key_down(game:&Game<'_>,module:&Module,setting:&str)->bool {
    let Some(name)=super::sunset::key_from_text(module.t(setting)) else {return false;};
    game.physical_keys.iter().chain(game.keys.iter())
        .any(|key|horizon_client::keybinds::key_name(*key)==Some(name.as_str()))
        || game.physical_buttons.iter().any(|button|horizon_client::keybinds::mouse_name(*button)==name)
}
pub(super) fn block_contact(game:&Game<'_>,look:(f32,f32),clicked:[i32;3],face:i32,range:f64)->Option<[f64;3]> {
    if !look.0.is_finite() || !look.1.is_finite() || !range.is_finite(){return None;}
    let yaw=f64::from(look.0).to_radians();let pitch=f64::from(look.1).to_radians();
    let direction=[-yaw.sin()*pitch.cos(),-pitch.sin(),yaw.cos()*pitch.cos()];
    let normal=match face {0=>[0,-1,0],1=>[0,1,0],2=>[0,0,-1],3=>[0,0,1],4=>[-1,0,0],5=>[1,0,0],_=>return None};
    game.clip_ray(game.eye_position()?,direction,range.min(game.reach()))
        .filter(|hit|hit.pos==clicked&&hit.face==normal).map(|hit|hit.point)
}

pub(super) fn every_living(game: &Game<'_>) -> Vec<Target> {
    game.mobs.iter().filter(|m| m.alive() && !m.removed).map(mob_target).collect()
}

// What to call an entity: a player's name, a custom name, or what it is.
pub(super) fn display_name(game: &Game<'_>, mob: &horizon_client::mobs::Mob) -> String {
    let mut name = String::new();
    name_into(game, mob, &mut name);
    name
}

// The same, written over `out`: Nametags reuse one buffer for every mob of every frame.
pub(super) fn name_into(game: &Game<'_>, mob: &horizon_client::mobs::Mob, out: &mut String) {
    out.clear();
    if let Some(n) = game.remote_player_name(mob.id).or(mob.custom_name.as_deref()) {
        out.push_str(n);
        return;
    }
    // "minecraft:zombie_villager" as "Zombie Villager".
    for (i, part) in short(&mob.kind.id).split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut c = part.chars();
        if let Some(f) = c.next() {
            out.extend(f.to_uppercase());
            out.push_str(c.as_str());
        }
    }
}

// Who the targeting modules leave alone: with Anti bot, a player the server's player list does not show; with
// Ignore teams, anyone on your own scoreboard team.
pub(super) fn spared(game: &Game<'_>, m: &Module, id: u32, player: bool) -> bool {
    if player
        && m.b("Anti bot")
        && let Some(online) = game.remote_online_names()
        && game.remote_player_name(id).is_none_or(|n| !online.iter().any(|o| o == n))
    {
        return true;
    }
    m.b("Ignore teams")
        && horizon_client::scoreboard::teams_allied(game.score.board.players_team(&game.options.player_name()), game.mob_team(id))
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

// A comma list's choices as Pick values need 'static: these are the shared ones.
pub(super) const TRAP_BLOCKS: &str = "obsidian,crying_obsidian,netherite_block";

pub(super) fn report(game: &mut Game<'_>, msg: &str) {
    say(game, msg);
}

pub(super) fn inventory_idle(game: &Game<'_>) -> bool {
    super::sunset::inventory_idle(game)
}

pub(super) fn in_world(game: &Game<'_>) -> bool {
    game.native_playing() && game.remote.is_some() && game.player.is_some() && game.screen == Screen::Playing
}

// The most damage the nearest end crystal would do to something standing at `at`
// (a power 6 blast with nothing in the way and no armour: Meteor's DamageUtils
// without the raycast).
pub(super) fn crystal_damage(game: &Game<'_>, at: [f64; 3]) -> f64 {
    let mut worst: f64 = 0.0;
    for c in &game.crystals {
        let from = [c.position.x, c.position.y, c.position.z];
        let d = dist(from, at) / 12.0;
        if d > 1.0 {
            continue;
        }
        let impact = 1.0 - d;
        worst = worst.max(((impact * impact + impact) / 2.0 * 7.0 * 12.0 + 1.0).floor());
    }
    worst
}

// The highest point of the fall so far.
static FALL_PEAK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// How far you have dropped from your highest point since you last stood on something (0 when you are on the
// ground, flying or gliding). Called once a tick by whoever needs it.
pub(super) fn fallen(game: &Game<'_>) -> f64 {
    use std::sync::atomic::Ordering::Relaxed;
    let Some(p) = game.player.as_ref() else {
        return 0.0;
    };
    let y = p.position.y;
    let mut peak = f64::from_bits(FALL_PEAK.load(Relaxed));
    if p.ground.on_ground || p.flying || p.fall_flying || peak < y {
        peak = y;
    }
    FALL_PEAK.store(peak.to_bits(), Relaxed);
    peak - y
}

// What falling to the ground below would cost you (Meteor's PlayerUtils.possibleHealthReductions, fall part):
// the drop so far plus the drop still to come, less three blocks.
pub(super) fn fall_damage(game: &Game<'_>) -> f64 {
    let so_far = fallen(game);
    let Some(p) = game.player.as_ref() else {
        return 0.0;
    };
    if p.velocity.y > -0.2 || p.flying {
        return 0.0;
    }
    let y = p.position.y;
    let (x, z) = (p.position.x.floor() as i32, p.position.z.floor() as i32);
    let top = y.floor() as i32;
    let ground = (0..256).find(|d| super::sunset::solid_at(game, x, top - 1 - d, z));
    let to_come = ground.map_or(256.0, |d| y - f64::from(top - 1 - d + 1));
    (so_far + to_come - 3.0).max(0.0).ceil()
}
