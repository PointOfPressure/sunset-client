// Crystal Aura: one crystal sequence at a time on the nearest enemy. It picks a base next to them (obsidian or
// bedrock already there, or a spot it puts obsidian on), places an end crystal on it, hits it, and places again on the
// same base until the target leaves or the spot stops paying off. The pick scores every base around the target by the
// damage it does (with the target led by its velocity and ping), what it costs you, where the blast pushes them, how
// far the look has to turn and how close it is. Its Macro section is combat_macros.rs's (a right click with a crystal).
use super::sunset::{self, Category, Module, with};
use super::sunset_explosive_damage::{Blast, local_damage_at};
use super::sunset_explosive_policy::Budget;
use super::sunset_explosive_tactics as tactics;
use super::sunset_util::{self as util, Target};
use super::{Game, sunset_prediction as prediction};
use horizon_protocol::packets::{
    Empty,
    play::{self, Serverbound as S},
};
use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Instant;
type SetupScene = (i32, super::Dim, Arc<dyn Any + Send + Sync>);

const NAME: &str = "Crystal Aura";
// The scan around the target: 4 blocks out, 2 up and down, and only cells within 5.25 of the eyes.
const SCAN_SIDE: i32 = 4;
const SCAN_UP: i32 = 2;
const SCAN_EYE_SQ: f64 = 5.25 * 5.25;
// The farthest a click or a support face may be from the eyes.
const CLICK_REACH: f64 = 4.25;
const AIM_LIMIT: f64 = 4.4;
// The most the look turns in one tick while it is silent.
const TURN_STEP: f32 = 90.0;
// One error message every 3 s at most.
const NOTICE_MS: f64 = 3000.0;

// ---------------------------------------------------------------- settings

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Combat, "Places obsidian and crystals by the nearest enemy and breaks them, or on your right click.")
            .bool("Aura", true)
            .bool("Advanced", false)
            .group("Target")
            .num("Target range", 10.0, 1.0, 16.0, 0.5)
            .num("Max target distance", 12.0, 1.0, 20.0, 0.5)
            .num("Movement tolerance", 3.0, 1.0, 8.0, 1.0)
            .group("Damage")
            .num("Min damage", 6.0, 0.0, 36.0, 0.5)
            .num("Max self damage", 8.0, 0.0, 36.0, 0.5)
            .bool("Anti suicide", true)
            .num("Recheck factor", 0.6, 0.0, 1.0, 0.05)
            .group("Actions")
            .pick("Break mode", &["Attack", "Swap attack", "Manual"])
            .bool("Silent rotation", true)
            .num("Silent FOV", 180.0, 0.0, 180.0, 5.0)
            .bool("Fast swap", true)
            .bool("Switch back", true)
            .num("Reach", 2.95, 1.0, 3.0, 0.05)
            .group("Timing")
            .num("Shortest crystal delay (ms)", 60.0, 0.0, 500.0, 10.0)
            .num("Longest crystal delay (ms)", 120.0, 0.0, 500.0, 10.0)
            .num("Shortest break delay (ms)", 60.0, 0.0, 500.0, 10.0)
            .num("Longest break delay (ms)", 140.0, 0.0, 500.0, 10.0)
            .num("Shortest reaction (ms)", 40.0, 0.0, 200.0, 10.0)
            .num("Longest reaction (ms)", 80.0, 0.0, 200.0, 10.0)
            .group("Placement")
            .bool("Prefer adjacent", true)
            .bool("Prefer placed obsidian", true)
            .bool("Placed obsidian only", false)
            .pick("Momentum", &["None", "Toward hole", "Away from you", "Toward center"])
            .num("Momentum weight", 1.0, 0.0, 5.0, 0.1)
            .num("Turn cost", 0.05, 0.0, 1.0, 0.01)
            .num("Turn cost cap", 60.0, 0.0, 180.0, 5.0)
            .num("Path lookahead", 3.0, 0.0, 10.0, 1.0)
            .group("Prediction")
            .bool("Predict target", true)
            .num("Lead ticks", 3.0, 0.0, 10.0, 0.5)
            .num("Ping lead", 1.0, 0.0, 2.0, 0.1)
            .num("Max lead", 3.0, 0.0, 8.0, 0.5)
            .bool("Predict self", true)
            .num("Self lead ticks", 2.0, 0.0, 8.0, 0.5)
            .num("Self max lead", 2.0, 0.0, 6.0, 0.5)
            .group("Render")
            .bool("Render", true)
            .bool("Render target", true)
            .bool("Render placement", true)
            .bool("Render candidates", true)
            .num("Candidates shown", 8.0, 1.0, 30.0, 1.0)
            .text("Target color", "#FFFF5050")
            .text("Obsidian color", "#FF963CDC")
            .text("Crystal color", "#FFE678E6")
            .text("Candidate color", "#FF78C8FF")
            .bool("Debug aborts", false)
            .group("Macro")
            .bool("Macro", false)
            .num("Obsidian delay", 2.0, 0.0, 10.0, 1.0)
            .bool("Perfect timing", false)
            .num("Place delay", 0.0, 0.0, 10.0, 1.0)
            .num("Break delay", 0.0, 0.0, 10.0, 1.0)
            .bool("Auto obsidian", true)
            .bool("Human pace", true)
            .bool("Protect loot", true),
    ]
}

// Which settings the menu shows: the Aura's own with Aura on (its fine tuning with Advanced), the Macro's with Macro
// on, and the two both read with either.
pub(super) fn visible(m: &Module, name: &str) -> bool {
    let (aura, macro_on, advanced) = (m.b("Aura"), m.b("Macro"), m.b("Advanced"));
    let tuned = aura && advanced;
    let render = aura && m.b("Render");
    match name {
        "Aura" | "Macro" => true,
        "Max self damage" | "Switch back" => aura || macro_on,
        "Obsidian delay" => macro_on && m.b("Auto obsidian"),
        "Perfect timing" | "Place delay" | "Break delay" | "Auto obsidian" | "Human pace" | "Protect loot" => macro_on,
        "Advanced" | "Target range" | "Min damage" | "Anti suicide" | "Break mode" | "Silent rotation" | "Render" => aura,
        "Silent FOV" => tuned && m.b("Silent rotation"),
        "Momentum weight" => tuned && m.p("Momentum") != "None",
        "Turn cost cap" => tuned && m.n("Turn cost") > 0.0,
        "Lead ticks" | "Ping lead" | "Max lead" => tuned && m.b("Predict target"),
        "Self lead ticks" | "Self max lead" => tuned && m.b("Predict self"),
        "Render target" | "Render placement" | "Render candidates" => tuned && render,
        "Target color" => tuned && render && m.b("Render target"),
        "Obsidian color" | "Crystal color" => tuned && render && m.b("Render placement"),
        "Candidates shown" | "Candidate color" => tuned && render && m.b("Render candidates"),
        _ => tuned,
    }
}

// ---------------------------------------------------------------- state

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    Idle,
    // Obsidian goes on the base first.
    Base,
    // A crystal goes on the base.
    Place,
    // The crystal on the base is hit.
    Hit,
}
impl Stage {
    fn label(self) -> &'static str {
        match self {
            Stage::Idle => "idle",
            Stage::Base => "obsidian",
            Stage::Place => "crystal",
            Stage::Hit => "hit",
        }
    }
}

// A base the scan found: where the crystal goes on, how obsidian gets there when it isn't yet (the block clicked and
// its face), the point to look at first and the score.
#[derive(Clone, Copy, Debug)]
struct Spot {
    base: [i32; 3],
    support: Option<([i32; 3], i32)>,
    aim: [f64; 3],
    needs_base: bool,
    armed: bool,
    score: f64,
}

// A delay between two settings, drawn again after each use.
#[derive(Clone, Copy)]
struct Roll {
    ms: f64,
}
impl Roll {
    const ZERO: Roll = Roll { ms: 0.0 };
    fn draw(m: &Module, short: &str, long: &str, rng: &mut u64) -> Roll {
        let (a, b) = (m.n(short), m.n(long));
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        Roll { ms: lo + unit(rng) * (hi - lo) }
    }
}

// A stopwatch in milliseconds.
#[derive(Clone, Copy)]
struct Watch(Option<Instant>);
impl Watch {
    const NEW: Watch = Watch(None);
    fn reset(&mut self) {
        self.0 = Some(Instant::now());
    }
    fn ms(&self) -> f64 {
        self.0.map_or(f64::MAX, |t| t.elapsed().as_secs_f64() * 1000.0)
    }
    fn past(&self, ms: f64) -> bool {
        self.ms() >= ms
    }
}

struct State {
    stage: Stage,
    target: Option<u32>,
    spot: Option<Spot>,
    // The target's block when the sequence began.
    origin: Option<[i32; 3]>,
    // A click or hit went out in this sequence.
    acted: bool,
    // Obsidian was just placed: its crystal goes without the crystal delay.
    fresh_base: bool,
    // A hit stage that ran out of time places once more before giving up.
    retried: bool,
    // A crystal on the base is out of reach.
    far: bool,
    // The last crystal hit: ignored until it is gone.
    hit_id: Option<u32>,
    // The slot held before the first action, for Switch back and Swap attack.
    home: Option<usize>,
    stage_clock: Watch,
    place_clock: Watch,
    break_clock: Watch,
    first_clock: Watch,
    notice_clock: Watch,
    place_roll: Roll,
    break_roll: Roll,
    first_roll: Roll,
    // The eyes as of the last tick: where the server last had them.
    last_eye: Option<[f64; 3]>,
    // The tick something went out (a click, a hit, a slot change): one a tick.
    acted_tick: u64,
    // Velocity tracking: whose, where last tick, which tick and the smoothed motion per tick.
    tracked: Option<u32>,
    tracked_pos: Option<[f64; 3]>,
    tracked_tick: u64,
    motion: [f64; 3],
    // First seen tick of each crystal, for its age.
    seen: Vec<(u32, u64)>,
    candidates: Vec<Spot>,
    rng: u64,
    weapon: Option<WeaponLease>,
}
static STATE: Mutex<State> = Mutex::new(State {
    stage: Stage::Idle,
    target: None,
    spot: None,
    origin: None,
    acted: false,
    fresh_base: false,
    retried: false,
    far: false,
    hit_id: None,
    home: None,
    stage_clock: Watch::NEW,
    place_clock: Watch::NEW,
    break_clock: Watch::NEW,
    first_clock: Watch::NEW,
    notice_clock: Watch::NEW,
    place_roll: Roll::ZERO,
    break_roll: Roll::ZERO,
    first_roll: Roll::ZERO,
    last_eye: None,
    acted_tick: u64::MAX,
    tracked: None,
    tracked_pos: None,
    tracked_tick: 0,
    motion: [0.0; 3],
    seen: Vec::new(),
    candidates: Vec::new(),
    rng: 0x9E37_79B9_7F4A_7C15,
    weapon: None,
});
fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}
fn unit(rng: &mut u64) -> f64 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    (*rng >> 11) as f64 / (1u64 << 53) as f64
}

// ---------------------------------------------------------------- the shared hand (Anchor Aura uses it too)

struct WeaponLease {
    scene: SetupScene,
    original: usize,
    owned: usize,
    owner: &'static str,
    until: u64,
}
fn restore_weapon(game: &mut Game<'_>, requested: bool) {
    restore_shared(game, NAME, requested);
}
pub(super) fn restore_shared(game: &mut Game<'_>, owner: &str, requested: bool) {
    // Packet Mine holds its tool for a STOP and gives the hand back next tick: that is not the player changing slot,
    // so the lease stays for then.
    if !st(|s| s.weapon.as_ref().is_some_and(|h| h.owner == owner)) || super::sunset_packet_mine::borrowed() {
        return;
    }
    // A competing queued/native hand action gets to finish before a return.
    // Keep the baseline until ownership becomes available, unless the player
    // already changed the selected slot or this scene is gone.
    let stale =
        st(|s| s.weapon.as_ref().is_some_and(|h| !same_setup_scene(game, &h.scene) || h.owned != game.selected));
    if stale {
        st(|s| s.weapon = None);
        return;
    }
    if requested
        && (game.using.is_some()
            || super::sunset_player::gap_eating()
            || super::sunset_combat_control::owner().is_some_and(|active| active != owner))
    {
        return;
    }
    let Some(lease) = st(|s| s.weapon.take()) else {
        return;
    };
    if requested && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0) {
        if let Some(slot) = super::sunset_macro_policy::restore(lease.original, lease.owned, game.selected, true) {
            weapon_hold(game, slot);
        }
    }
}
pub(super) fn take_shared(game: &mut Game<'_>, slot: usize, owner: &'static str) {
    if slot == util::OFFHAND {
        return;
    }
    let Some(remote) = game.remote.as_ref() else {
        return;
    };
    let scene = (remote.entity_id, game.dim, remote.session.counters.clone() as Arc<dyn Any + Send + Sync>);
    let before = game.selected;
    st(|s| {
        if s.weapon.as_ref().is_some_and(|h| !same_setup_scene(game, &h.scene) || h.owned != before) {
            s.weapon = None;
        }
        if let Some(lease) = s.weapon.as_mut() {
            lease.owned = slot;
            lease.owner = owner;
            lease.until = game.ticks.saturating_add(10);
        } else if before != slot {
            s.weapon =
                Some(WeaponLease { scene, original: before, owned: slot, owner, until: game.ticks.saturating_add(10) });
        }
    });
    weapon_hold(game, slot);
}
fn take_weapon(game: &mut Game<'_>, slot: usize) {
    take_shared(game, slot, NAME);
}
pub(super) fn idle_shared(game: &mut Game<'_>, owner: &str, requested: bool) {
    if st(|s| s.weapon.as_ref().is_some_and(|h| h.owner == owner && game.ticks > h.until)) {
        restore_shared(game, owner, requested);
    }
}
fn idle_weapon(game: &mut Game<'_>, requested: bool) {
    idle_shared(game, NAME, requested);
}
pub(super) fn cleanup_shared(game: &mut Game<'_>) {
    let owner = st(|s| s.weapon.as_ref().map(|h| h.owner));
    if let Some(owner) = owner {
        let requested = owner != "Crystal Aura" || with(|s| s.module(NAME).is_none_or(|m| m.b("Switch back")));
        restore_shared(game, owner, requested);
    }
}
fn weapon_hold(game: &mut Game<'_>, slot: usize) {
    sunset::hold(game, slot);
    super::sunset_combat_macros::note_main(game);
}
fn same_setup_scene(game: &Game<'_>, old: &SetupScene) -> bool {
    game.remote.as_ref().is_some_and(|r| {
        r.entity_id == old.0
            && game.dim == old.1
            && Arc::as_ptr(&r.session.counters) as *const () == Arc::as_ptr(&old.2) as *const ()
    })
}
pub(super) fn owns_selection(game: &Game<'_>) -> bool {
    st(|s| s.weapon.as_ref().is_some_and(|lease| same_setup_scene(game, &lease.scene) && lease.owned == game.selected))
}

// ---------------------------------------------------------------- module life

pub(super) fn reset() {
    sunset::cancel_turns(NAME);
    st(|s| {
        s.stage = Stage::Idle;
        s.target = None;
        s.spot = None;
        s.origin = None;
        s.home = None;
        s.fresh_base = false;
        s.retried = false;
        s.acted = false;
        s.hit_id = None;
        s.last_eye = None;
        s.tracked = None;
        s.tracked_pos = None;
        s.motion = [0.0; 3];
        s.candidates.clear();
        s.seen.clear();
    });
    view(|v| *v = View::NEW);
}
// Turned off (or the Aura section): the slot held before goes back, then everything is forgotten.
pub(super) fn disabled(game: &mut Game<'_>) {
    let home = st(|s| s.home.take());
    if let Some(home) = home
        && with(|s| s.module(NAME).is_none_or(|m| m.b("Switch back")))
        && game.selected != home
    {
        weapon_hold(game, home);
    }
    cleanup_shared(game);
    reset();
}
// The game can't be played right now (a screen, using an item): the sequence waits where it is; its stage clock runs
// on, so one that waits too long ends.
pub(super) fn paused(_game: &mut Game<'_>) {}

// A new world, life or connection.
pub(super) fn discard_scene() {
    super::sunset_combat_control::reset();
    st(|s| s.weapon = None);
    reset();
    super::sunset_anchor_aura::reset();
}
pub(super) fn packet(_game: &Game<'_>, packet: &horizon_protocol::packets::play::Clientbound) {
    use horizon_protocol::packets::play::Clientbound as C;
    if matches!(packet, C::Login(_) | C::Respawn(_) | C::Disconnect(_)) {
        discard_scene();
    }
}
// Anchor Aura leaves the cells of a running sequence alone: the base and the crystal's.
pub(super) fn reserved_cell(cell: [i32; 3]) -> bool {
    st(|s| s.stage != Stage::Idle && s.spot.is_some_and(|p| cell == p.base || cell == up(p.base)))
}

// ---------------------------------------------------------------- world helpers

fn base_block(game: &Game<'_>, p: [i32; 3]) -> bool {
    matches!(&*util::facts(game, p).name, "minecraft:obsidian" | "minecraft:bedrock")
}
// Taken: obsidian can't go there (a block, fire or grass in it).
fn taken(game: &Game<'_>, p: [i32; 3]) -> bool {
    let f = util::facts(game, p);
    !f.replaceable || matches!(&*f.name, "minecraft:fire" | "minecraft:short_grass")
}
fn up(p: [i32; 3]) -> [i32; 3] {
    [p[0], p[1] + 1, p[2]]
}
fn mid(p: [i32; 3]) -> [f64; 3] {
    [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5]
}
// Where the crystal on a base stands.
fn crystal_at(base: [i32; 3]) -> [f64; 3] {
    [f64::from(base[0]) + 0.5, f64::from(base[1]) + 1.0, f64::from(base[2]) + 0.5]
}
fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    util::dist(a, b)
}
fn overlaps(alo: [f64; 3], ahi: [f64; 3], blo: [f64; 3], bhi: [f64; 3]) -> bool {
    (0..3).all(|i| alo[i] < bhi[i] && ahi[i] > blo[i])
}
fn cell_box(p: [i32; 3], height: f64) -> ([f64; 3], [f64; 3]) {
    let lo = p.map(f64::from);
    (lo, [lo[0] + 1.0, lo[1] + height, lo[2] + 1.0])
}
fn crystal_box(c: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    ([c[0] - 1.0, c[1], c[2] - 1.0], [c[0] + 1.0, c[1] + 2.0, c[2] + 1.0])
}
fn bounds(b: horizon_core::geometry::Aabb) -> ([f64; 3], [f64; 3]) {
    let (lo, hi) = (b.min_position(), b.max_position());
    ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
}

// A crystal (not the last one hit) standing in the cell: within 1 block of its middle, as the hit stage looks for it.
// A crystal on a neighbouring base reaches into the cell too (they are 2 wide), but only blocks it (occupied).
fn crystal_in(game: &Game<'_>, cell: [i32; 3]) -> bool {
    let hit = st(|s| {
        if s.hit_id.is_some_and(|id| !game.crystals.iter().any(|c| c.id == id)) {
            s.hit_id = None;
        }
        s.hit_id
    });
    let spot = mid(cell);
    game.crystals.iter().any(|c| Some(c.id) != hit && dist([c.position.x, c.position.y, c.position.z], spot) <= 1.0)
}
// Anything in the cell (and the cells above it up to height) but dropped items and the last crystal hit: what keeps
// a block or a crystal out.
fn occupied(game: &Game<'_>, cell: [i32; 3], height: f64) -> bool {
    let (lo, hi) = cell_box(cell, height);
    let hit = st(|s| s.hit_id);
    let small = |x: f64, y: f64, z: f64| overlaps([x - 0.25, y, z - 0.25], [x + 0.25, y + 0.5, z + 0.25], lo, hi);
    game.player.as_ref().is_some_and(|p| {
        let (a, b) = bounds(p.bounds());
        overlaps(a, b, lo, hi)
    }) || game.mobs.iter().any(|m| {
        let (a, b) = bounds(m.bounds());
        !m.removed && overlaps(a, b, lo, hi)
    }) || game.crystals.iter().any(|c| {
        let (a, b) = crystal_box([c.position.x, c.position.y, c.position.z]);
        Some(c.id) != hit && overlaps(a, b, lo, hi)
    }) || game.orbs.iter().any(|o| small(o.position.x, o.position.y, o.position.z))
        || game.creatures.projectiles.iter().any(|p| small(p.position.x, p.position.y, p.position.z))
}
// occupied, or someone walking into it within Path lookahead ticks: the target at its tracked motion, you at yours.
fn crowded(game: &Game<'_>, m: &Module, cell: [i32; 3], height: f64, target: Option<&Target>) -> bool {
    if occupied(game, cell, height) {
        return true;
    }
    let ticks = m.n("Path lookahead").round().max(0.0) as i32;
    if ticks <= 0 {
        return false;
    }
    let (lo, hi) = cell_box(cell, height);
    let swept = |blo: [f64; 3], bhi: [f64; 3], v: [f64; 3]| {
        v.iter().map(|x| x * x).sum::<f64>() > 1.0e-6
            && (1..=ticks).any(|k| {
                let k = f64::from(k);
                overlaps(std::array::from_fn(|i| blo[i] + v[i] * k), std::array::from_fn(|i| bhi[i] + v[i] * k), lo, hi)
            })
    };
    if let Some(t) = target
        && swept(t.lo, t.hi, motion_of(t.id))
    {
        return true;
    }
    game.player.as_ref().is_some_and(|p| {
        let (a, b) = bounds(p.bounds());
        swept(a, b, [p.velocity.x, p.velocity.y, p.velocity.z])
    })
}

// ---------------------------------------------------------------- looks

fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}
fn angles(eye: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let (dx, dy, dz) = (to[0] - eye[0], to[1] - eye[1], to[2] - eye[2]);
    let flat = (dx * dx + dz * dz).sqrt();
    ((dz.atan2(dx).to_degrees() - 90.0) as f32, (-dy.atan2(flat).to_degrees()) as f32)
}
// A look turned from the last one the server has toward a point, at most TURN_STEP of yaw; true when it was cut short.
fn step_look(game: &Game<'_>, to: [f64; 3]) -> Option<((f32, f32), bool)> {
    let eye = game.eye_position()?;
    let from = super::sunset_combat_wire::sent_look(game).unwrap_or((game.yaw, game.pitch));
    let (yaw, pitch) = angles(eye, to);
    let turn = wrap(yaw - from.0);
    let short = turn.abs() > TURN_STEP;
    Some(((from.0 + turn.clamp(-TURN_STEP, TURN_STEP), from.1 + wrap(pitch - from.1)), short))
}
// The look from the camera straight at a point.
fn camera_look(game: &Game<'_>, to: [f64; 3]) -> Option<(f32, f32)> {
    let (yaw, pitch) = angles(game.eye_position()?, to);
    Some((game.yaw + wrap(yaw - game.yaw), game.pitch + wrap(pitch - game.pitch)))
}
fn off_camera(game: &Game<'_>, look: (f32, f32)) -> f64 {
    f64::from(wrap(look.0 - game.yaw).abs())
}
fn direction(look: (f32, f32)) -> [f64; 3] {
    let (yaw, pitch) = (f64::from(look.0).to_radians(), f64::from(look.1).to_radians());
    [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()]
}
// Where a segment first enters a box (None from inside it).
fn enters(from: [f64; 3], to: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> Option<[f64; 3]> {
    let d: [f64; 3] = std::array::from_fn(|i| to[i] - from[i]);
    let (mut near, mut far) = (0.0f64, 1.0f64);
    let mut entered = false;
    for i in 0..3 {
        if d[i].abs() < 1.0e-12 {
            if from[i] < lo[i] || from[i] > hi[i] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[i] - from[i]) / d[i], (hi[i] - from[i]) / d[i]);
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        if a > near {
            near = a;
            entered = true;
        }
        far = far.min(b);
        if near > far {
            return None;
        }
    }
    entered.then(|| std::array::from_fn(|i| from[i] + d[i] * near))
}
// The look from an eye meets the box within Reach.
fn look_meets(from: [f64; 3], look: (f32, f32), lo: [f64; 3], hi: [f64; 3], reach: f64) -> bool {
    let d = direction(look);
    enters(from, std::array::from_fn(|i| from[i] + d[i] * 6.0), lo, hi).is_some_and(|p| dist(from, p) <= reach)
}
fn box_distance(lo: [f64; 3], hi: [f64; 3], p: [f64; 3]) -> f64 {
    let d: [f64; 3] = std::array::from_fn(|i| (lo[i] - p[i]).max((p[i] - hi[i]).max(0.0)));
    d.iter().map(|v| v * v).sum::<f64>().sqrt()
}
// A clear line from the eyes to a point on a block: the first block it meets is met there, and no one stands on it.
fn clear_to(game: &Game<'_>, eye: [f64; 3], point: [f64; 3]) -> bool {
    let length = dist(eye, point);
    if length < 1.0e-6 {
        return true;
    }
    let d: [f64; 3] = std::array::from_fn(|i| (point[i] - eye[i]) / length);
    if !game.clip_ray(eye, d, length + 0.1).is_some_and(|hit| dist(hit.point, point) < 0.1) {
        return false;
    }
    let own = game.remote.as_ref().map(|r| r.entity_id as u32);
    let blocks = |lo: [f64; 3], hi: [f64; 3]| enters(eye, point, lo.map(|v| v - 0.3), hi.map(|v| v + 0.3)).is_some();
    !game.mobs.iter().any(|m| {
        let (lo, hi) = bounds(m.bounds());
        !m.removed && Some(m.id) != own && m.alive() && blocks(lo, hi)
    }) && !game.crystals.iter().any(|c| {
        let (lo, hi) = crystal_box([c.position.x, c.position.y, c.position.z]);
        blocks(lo, hi)
    })
}
// The point of a block to click it at: the middle of its top when the eyes see it, else the nearest middle of a side
// they see, else the nearest of its corners they see.
fn click_point(game: &Game<'_>, block: [i32; 3]) -> Option<[f64; 3]> {
    const POINTS: [[f64; 3]; 13] = [
        [0.5, 1.0, 0.5],
        [0.5, 0.5, 0.0],
        [0.0, 0.5, 0.5],
        [1.0, 0.5, 0.5],
        [0.5, 0.5, 1.0],
        [1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let eye = game.eye_position()?;
    let at = |o: [f64; 3]| -> [f64; 3] { std::array::from_fn(|i| f64::from(block[i]) + o[i]) };
    let mut best: Option<[f64; 3]> = None;
    for (i, o) in POINTS.iter().take(5).enumerate() {
        let p = at(*o);
        if dist(eye, p) > CLICK_REACH || !clear_to(game, eye, p) {
            continue;
        }
        if i == 0 {
            return Some(p);
        }
        if best.is_none_or(|b| dist(eye, p) < dist(eye, b)) {
            best = Some(p);
        }
    }
    if best.is_some() {
        return best;
    }
    for o in POINTS {
        let p = at(o);
        if dist(eye, p) <= CLICK_REACH && clear_to(game, eye, p) && best.is_none_or(|b| dist(eye, p) < dist(eye, b)) {
            best = Some(p);
        }
    }
    best
}
// A silent look at the point can click the block: within Silent FOV, and either still turning or already on it.
fn silent_clicks(game: &Game<'_>, m: &Module, point: [f64; 3], block: [i32; 3]) -> bool {
    let Some((look, short)) = step_look(game, point) else {
        return false;
    };
    if off_camera(game, look) > m.n("Silent FOV") {
        return false;
    }
    short || util::block_hit(game, look, block, 6.0).is_some()
}

// The same for one face of the block: the click that puts obsidian against it.
fn silent_clicks_face(game: &Game<'_>, m: &Module, point: [f64; 3], block: [i32; 3], face: i32) -> bool {
    let Some((look, short)) = step_look(game, point) else {
        return false;
    };
    if off_camera(game, look) > m.n("Silent FOV") {
        return false;
    }
    short || util::block_contact(game, look, block, face, 6.0).is_some()
}

// ---------------------------------------------------------------- targets and damage

fn ping_of(game: &Game<'_>, id: u32) -> i32 {
    super::remote::sunset_combat_remote::player_uuid(game, id)
        .and_then(|uuid| game.remote.as_ref()?.tab_players.get(&uuid).map(|p| p.latency))
        .unwrap_or(0)
        .max(0)
}
fn own_ping(game: &Game<'_>) -> i32 {
    super::remote::sunset_combat_remote::latency(game).unwrap_or(0).max(0)
}
// Every player that may be attacked (not you, a friend, a bot Antibot caught, a teammate or one in creative), however far.
fn enemies(game: &Game<'_>) -> Vec<Target> {
    game.mobs.iter().filter(|mob| util::player_enemy(game, mob)).map(util::mob_target).collect()
}
fn nearest(game: &Game<'_>, list: &[Target]) -> Option<Target> {
    let feet = util::player_pos(game)?;
    let sq = |t: &Target| (0..3).map(|i| (t.pos[i] - feet[i]).powi(2)).sum::<f64>();
    list.iter().filter(|t| t.health > 0.0).min_by(|a, b| sq(a).total_cmp(&sq(b))).cloned()
}
// The motion per tick of whoever is tracked, each tick's averaged half and half with what it was.
fn track(game: &Game<'_>, who: Option<&Target>) {
    st(|s| match who {
        None => {
            s.tracked = None;
            s.tracked_pos = None;
            s.motion = [0.0; 3];
            s.tracked_tick = game.ticks;
        }
        Some(t) => {
            match s.tracked_pos {
                Some(last) if s.tracked == Some(t.id) && game.ticks == s.tracked_tick.wrapping_add(1) => {
                    s.motion = std::array::from_fn(|i| s.motion[i] * 0.5 + (t.pos[i] - last[i]) * 0.5);
                }
                _ => {
                    s.tracked = Some(t.id);
                    s.motion = [0.0; 3];
                }
            }
            s.tracked_pos = Some(t.pos);
            s.tracked_tick = game.ticks;
        }
    });
}
fn motion_of(id: u32) -> [f64; 3] {
    st(|s| if s.tracked == Some(id) { s.motion } else { [0.0; 3] })
}
fn moving(id: u32) -> bool {
    let v = motion_of(id);
    v[0] * v[0] + v[2] * v[2] > 4.0e-4
}
fn clamp_len(v: [f64; 3], max: f64) -> [f64; 3] {
    let sq: f64 = v.iter().map(|x| x * x).sum();
    if sq <= max * max {
        return v;
    }
    if sq <= 1.0e-6 {
        return [0.0; 3];
    }
    let k = max / sq.sqrt();
    v.map(|x| x * k)
}
// Where the target will be: its feet led by its motion for Lead ticks and its ping, no more than Max lead.
fn led(game: &Game<'_>, m: &Module, t: &Target) -> [f64; 3] {
    let v = motion_of(t.id);
    if !m.b("Predict target") || v.iter().map(|x| x * x).sum::<f64>() < 1.0e-6 {
        return t.pos;
    }
    let ticks = (m.n("Lead ticks") + f64::from(ping_of(game, t.id)) / 50.0 * m.n("Ping lead")).clamp(0.0, 20.0);
    let lead = clamp_len([v[0] * ticks, 0.0, v[2] * ticks], m.n("Max lead"));
    std::array::from_fn(|i| t.pos[i] + lead[i])
}
// Where your eyes will be, led the same way by your own velocity.
fn led_eye(game: &Game<'_>, m: &Module) -> Option<[f64; 3]> {
    let eye = game.eye_position()?;
    let p = game.player.as_ref()?;
    let v = [p.velocity.x, p.velocity.y, p.velocity.z];
    if !m.b("Predict self") || v.iter().map(|x| x * x).sum::<f64>() < 1.0e-6 {
        return Some(eye);
    }
    let ticks = m.n("Self lead ticks");
    let lead = clamp_len([v[0] * ticks, 0.0, v[2] * ticks], m.n("Self max lead"));
    Some(std::array::from_fn(|i| eye[i] + lead[i]))
}
fn blast(game: &Game<'_>, base: [i32; 3]) -> Option<Blast> {
    let obsidian = game.items().get("minecraft:obsidian")?.block?;
    Some(Blast::explosion(crystal_at(base), 6.0).with_block(base, obsidian))
}
// What a crystal on the base does to the target standing with its feet there.
fn damage_to(game: &Game<'_>, t: &Target, feet: [f64; 3], base: [i32; 3]) -> f64 {
    if super::remote::sunset_combat_remote::player_mode(game, t.id) == Some(1) {
        return 0.0;
    }
    let Some(blast) = blast(game, base) else {
        return 0.0;
    };
    let shift: [f64; 3] = std::array::from_fn(|i| feet[i] - t.pos[i]);
    let mut moved = t.clone();
    moved.pos = feet;
    moved.lo = std::array::from_fn(|i| t.lo[i] + shift[i]);
    moved.hi = std::array::from_fn(|i| t.hi[i] + shift[i]);
    tactics::friend_damage(game, &moved, blast, &mut Budget::new(64)).unwrap_or(0.0)
}
// What it does to you where you stand (unknown counts as too much).
fn damage_to_self(game: &mut Game<'_>, base: [i32; 3]) -> f64 {
    let Some(blast) = blast(game, base) else {
        return f64::INFINITY;
    };
    local_damage_at(game, blast, [0.0; 3], &mut Budget::new(64)).map_or(f64::INFINITY, |p| p.estimate)
}
fn own_health(game: &Game<'_>) -> f64 {
    game.player.as_ref().map_or(0.0, |p| f64::from(p.health.health() + p.health.absorption()))
}
fn too_costly(own: f64, health: f64, m: &Module) -> bool {
    own > m.n("Max self damage") || m.b("Anti suicide") && own >= health - 1.0
}

// ---------------------------------------------------------------- the scan

// The nearest hole around the target: an air cell with air above, a block under and on all four sides.
fn nearest_hole(game: &Game<'_>, feet: [i32; 3]) -> Option<[i32; 3]> {
    let mut best: Option<([i32; 3], i32)> = None;
    for dx in -6..=6 {
        for dz in -6..=6 {
            let c = [feet[0] + dx, feet[1], feet[2] + dz];
            if !util::is_air(game, c) || !util::is_air(game, up(c)) || !taken(game, [c[0], c[1] - 1, c[2]]) {
                continue;
            }
            if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(x, z)| !taken(game, [c[0] + x, c[1], c[2] + z])) {
                continue;
            }
            let d = dx * dx + dz * dz;
            if best.is_none_or(|(_, b)| d < b) {
                best = Some((c, d));
            }
        }
    }
    best.map(|(c, _)| c)
}
// Which way the blast should push the target (flat, unit), for Momentum.
fn push_goal(game: &Game<'_>, m: &Module, target: &Target, at: [f64; 3]) -> Option<[f64; 3]> {
    let flat = |v: [f64; 3]| {
        let sq = v[0] * v[0] + v[2] * v[2];
        (sq > 1.0e-4).then(|| {
            let l = sq.sqrt();
            [v[0] / l, 0.0, v[2] / l]
        })
    };
    match m.p("Momentum") {
        "Away from you" => {
            let feet = util::player_pos(game)?;
            flat(std::array::from_fn(|i| at[i] - feet[i]))
        }
        "Toward center" => flat([-at[0], 0.0, -at[2]]),
        "Toward hole" => {
            let hole = mid(nearest_hole(game, target.pos.map(|v| v.floor() as i32))?);
            flat(std::array::from_fn(|i| hole[i] - at[i]))
        }
        _ => None,
    }
}
// Every base around the target that pays off, best first; the best is returned.
fn scan(game: &mut Game<'_>, m: &Module, target: &Target) -> Option<Spot> {
    let feet = target.pos.map(|v| v.floor() as i32);
    let ahead = led(game, m, target);
    let eye = game.eye_position()?;
    let eye_ahead = led_eye(game, m).unwrap_or(eye);
    let goal = push_goal(game, m, target, ahead);
    let health = own_health(game);
    let leading = m.b("Predict target") && moving(target.id);
    let target_health = prediction::effective_health(game, target).unwrap_or(f64::from(target.health));
    let silent = m.b("Silent rotation");
    let reach = m.n("Reach");
    let mut found: Vec<Spot> = Vec::new();
    for dx in -SCAN_SIDE..=SCAN_SIDE {
        for dz in -SCAN_SIDE..=SCAN_SIDE {
            for dy in -SCAN_UP..=SCAN_UP {
                let base = [feet[0] + dx, feet[1] + dy, feet[2] + dz];
                let middle = mid(base);
                let sq = |e: [f64; 3]| (0..3).map(|i| (e[i] - middle[i]).powi(2)).sum::<f64>();
                if sq(eye) > SCAN_EYE_SQ && sq(eye_ahead) > SCAN_EYE_SQ {
                    continue;
                }
                let above = up(base);
                let existing = base_block(game, base);
                let armed = existing && crystal_in(game, above);
                if !armed && (!util::is_air(game, above) || crowded(game, m, above, 2.0, Some(target))) {
                    continue;
                }
                let mut support = None;
                let aim;
                if armed {
                    let c = [middle[0], middle[1] + 1.0, middle[2]];
                    if dist(eye, c) > reach + 0.5 && dist(eye_ahead, c) > reach + 0.5 {
                        continue;
                    }
                    if silent && step_look(game, c).is_none_or(|(look, _)| off_camera(game, look) > m.n("Silent FOV")) {
                        continue;
                    }
                    aim = c;
                } else if existing {
                    let Some(p) = click_point(game, base) else {
                        continue;
                    };
                    if silent && !silent_clicks(game, m, p, base) {
                        continue;
                    }
                    aim = p;
                } else {
                    if m.b("Placed obsidian only") || taken(game, base) || crowded(game, m, base, 1.0, Some(target)) {
                        continue;
                    }
                    // The nearest face of a block beside it the eyes can click (the face toward the base).
                    let mut best: Option<([i32; 3], i32, [f64; 3], f64)> = None;
                    for (face, d) in [(1, [0, -1, 0]), (0, [0, 1, 0]), (3, [0, 0, -1]), (2, [0, 0, 1]), (5, [-1, 0, 0]), (4, [1, 0, 0])] {
                        let by = [base[0] + d[0], base[1] + d[1], base[2] + d[2]];
                        if util::facts(game, by).replaceable {
                            continue;
                        }
                        let c = mid(by);
                        let point: [f64; 3] = std::array::from_fn(|i| c[i] - f64::from(d[i]) * 0.5);
                        let near = dist(eye, point);
                        if near > CLICK_REACH && dist(eye_ahead, point) > CLICK_REACH {
                            continue;
                        }
                        // The face toward the base has to face the eyes: a click on any other face of the support puts
                        // the obsidian somewhere else, so a support seen from its back would never take it.
                        if (0..3).map(|i| (eye[i] - point[i]) * -f64::from(d[i])).sum::<f64>() <= 0.0 {
                            continue;
                        }
                        if silent && !silent_clicks_face(game, m, point, by, face) {
                            continue;
                        }
                        if best.is_none_or(|b| near < b.3) {
                            best = Some((by, face, point, near));
                        }
                    }
                    let Some((by, face, point, _)) = best else {
                        continue;
                    };
                    let top = [middle[0], middle[1] + 0.5, middle[2]];
                    if dist(eye, top) > CLICK_REACH && dist(eye_ahead, top) > CLICK_REACH {
                        continue;
                    }
                    support = Some((by, face));
                    aim = point;
                }
                let now = damage_to(game, target, target.pos, base);
                let mut dealt = now;
                if leading && now < m.n("Min damage") {
                    dealt = now.max(damage_to(game, target, ahead, base));
                }
                if dealt < m.n("Min damage") {
                    continue;
                }
                if too_costly(damage_to_self(game, base), health, m) {
                    continue;
                }
                let mut score = dealt;
                if dealt >= target_health {
                    score += 50.0;
                }
                if let Some(goal) = goal {
                    let c = crystal_at(base);
                    let push = [ahead[0] - c[0], ahead[2] - c[2]];
                    let sq = push[0] * push[0] + push[1] * push[1];
                    if sq > 1.0e-4 {
                        let along = (push[0] * goal[0] + push[1] * goal[2]) / sq.sqrt();
                        score += m.n("Momentum weight") * 8.0 * along.max(0.0);
                    }
                }
                if m.b("Prefer adjacent") && base[1] == feet[1] && (base[0] - feet[0]).abs().max((base[2] - feet[2]).abs()) == 1 {
                    score += 3.0;
                }
                if existing && m.b("Prefer placed obsidian") {
                    score += if armed { 8.0 } else { 4.0 };
                }
                if m.n("Turn cost") > 0.0
                    && let Some((look, _)) = step_look(game, aim)
                {
                    score -= m.n("Turn cost") * off_camera(game, look).min(m.n("Turn cost cap"));
                }
                score -= 0.2 * dist(eye, aim);
                found.push(Spot { base, support, aim, needs_base: !existing, armed, score });
            }
        }
    }
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let best = found.first().copied();
    st(|s| s.candidates = found);
    best
}

// ---------------------------------------------------------------- the sequence

fn notify(kind: super::sunset_toast::Kind, text: &str) {
    super::sunset_toast::push(kind, &format!("{NAME}: {text}"));
}
// The sequence ends (with Debug aborts, saying why).
fn stop(game: &mut Game<'_>, m: &Module, why: &str) {
    if m.b("Debug aborts")
        && let Some(stage) = st(|s| (s.stage != Stage::Idle).then_some(s.stage))
    {
        notify(super::sunset_toast::Kind::Info, &format!("abort: {}, {why}", stage.label()));
    }
    end(game, m);
}
// The slot goes back and everything about the sequence is dropped.
fn end(game: &mut Game<'_>, m: &Module) {
    give_back(game, m);
    st(|s| {
        s.stage = Stage::Idle;
        s.target = None;
        s.spot = None;
        s.origin = None;
        s.fresh_base = false;
        s.retried = false;
        s.candidates.clear();
    });
}
// A hotbar item is missing: said once every 3 s, and the sequence ends.
fn missing(game: &mut Game<'_>, m: &Module, text: &str) {
    let due = st(|s| {
        let due = s.notice_clock.past(NOTICE_MS);
        if due {
            s.notice_clock.reset();
        }
        due
    });
    if due {
        notify(super::sunset_toast::Kind::Error, text);
    }
    end(game, m);
}
// The slot held before the sequence comes back (Switch back), unless something already went out this tick.
fn give_back(game: &mut Game<'_>, m: &Module) {
    let Some(home) = st(|s| s.home) else {
        return;
    };
    if acted_now(game) {
        return;
    }
    if m.b("Switch back") && game.selected != home {
        weapon_hold(game, home);
        mark_acted(game);
    }
    st(|s| s.home = None);
}
fn acted_now(game: &Game<'_>) -> bool {
    st(|s| s.acted_tick == game.ticks)
}
fn mark_acted(game: &Game<'_>) {
    st(|s| s.acted_tick = game.ticks);
}
fn restart_stage() {
    st(|s| s.stage_clock.reset());
}
fn set_stage(stage: Stage) {
    st(|s| {
        s.stage = stage;
        s.stage_clock.reset();
    });
}
// How long a stage may take: a hit waits longer for its crystal than a placement for its block.
fn stage_limit(game: &Game<'_>, stage: Stage) -> f64 {
    let ping = f64::from(own_ping(game));
    if stage == Stage::Hit { (300.0 + ping * 1.5).clamp(500.0, 1500.0) } else { (120.0 + ping * 0.8).clamp(200.0, 600.0) }
}
fn draw_rolls(s: &mut State, m: &Module, reaction: bool, place: bool, hit: bool) {
    let mut rng = s.rng;
    if reaction {
        s.first_roll = Roll::draw(m, "Shortest reaction (ms)", "Longest reaction (ms)", &mut rng);
    }
    if place {
        s.place_roll = Roll::draw(m, "Shortest crystal delay (ms)", "Longest crystal delay (ms)", &mut rng);
    }
    if hit {
        s.break_roll = Roll::draw(m, "Shortest break delay (ms)", "Longest break delay (ms)", &mut rng);
    }
    s.rng = rng;
}
// Idle: the nearest enemy in Target range, and the best base around them.
fn begin(game: &mut Game<'_>, m: &Module) {
    let Some(target) = nearest(game, &enemies(game)) else {
        return;
    };
    if util::player_pos(game).is_none_or(|feet| dist(feet, target.pos) > m.n("Target range")) {
        return;
    }
    st(|s| s.target = Some(target.id));
    let Some(spot) = scan(game, m, &target) else {
        st(|s| s.target = None);
        return;
    };
    st(|s| {
        s.spot = Some(spot);
        s.origin = Some(target.pos.map(|v| v.floor() as i32));
        s.retried = false;
        s.stage = if spot.armed {
            Stage::Hit
        } else if spot.needs_base {
            Stage::Base
        } else {
            Stage::Place
        };
        s.stage_clock.reset();
        s.acted = false;
        s.fresh_base = false;
        s.first_clock.reset();
        s.place_clock.reset();
        s.break_clock.reset();
        draw_rolls(s, m, true, true, true);
    });
}
// The current stage may act now: the reaction delay before the first action, then each stage's own delay.
fn due(m: &Module) -> bool {
    let manual = m.p("Break mode") == "Manual";
    st(|s| {
        if !s.acted {
            return !(s.stage == Stage::Hit && manual) && s.stage != Stage::Idle && s.first_clock.past(s.first_roll.ms);
        }
        match s.stage {
            Stage::Base => true,
            Stage::Place => s.fresh_base || s.place_clock.past(s.place_roll.ms),
            Stage::Hit => !manual && s.break_clock.past(s.break_roll.ms),
            Stage::Idle => false,
        }
    })
}
fn age(game: &Game<'_>, id: u32) -> u64 {
    st(|s| s.seen.iter().find(|(c, _)| *c == id).map_or(0, |(_, at)| game.ticks.saturating_sub(*at)))
}
fn note_crystals(game: &Game<'_>) {
    st(|s| {
        s.seen.retain(|(id, _)| game.crystals.iter().any(|c| c.id == *id));
        for c in game.crystals.iter() {
            if !s.seen.iter().any(|(id, _)| *id == c.id) && s.seen.len() < 1024 {
                s.seen.push((c.id, game.ticks));
            }
        }
    });
}
// The crystal on the base that may be hit: not the last one hit, at least 2 ticks old, within Reach of the eyes (now
// and as the server last had them). One out of Reach marks the stage unreachable.
fn base_crystal(game: &Game<'_>, m: &Module) -> Option<(u32, [f64; 3])> {
    st(|s| s.far = false);
    let base = st(|s| s.spot.map(|p| p.base))?;
    if !crystal_in(game, up(base)) {
        return None;
    }
    let spot = mid(up(base));
    let eye = game.eye_position()?;
    let (last, hit) = st(|s| (s.last_eye, s.hit_id));
    let (zone_lo, zone_hi) = (spot.map(|v| v - 2.0), spot.map(|v| v + 2.0));
    for c in game.crystals.iter() {
        let at = [c.position.x, c.position.y, c.position.z];
        let (lo, hi) = crystal_box(at);
        if !overlaps(lo, hi, zone_lo, zone_hi) || Some(c.id) == hit || c.invulnerable || age(game, c.id) < 2 || dist(at, spot) > 1.0 {
            continue;
        }
        let far = box_distance(lo, hi, eye).max(last.map_or(0.0, |l| box_distance(lo, hi, l)));
        if far <= m.n("Reach") {
            return Some((c.id, at));
        }
        st(|s| s.far = true);
    }
    None
}
// Where to look for the current stage: the support face for obsidian, the base's click point for a crystal, the
// point of the crystal nearest the eyes (0.15 into it) for a hit.
fn stage_aim(game: &Game<'_>, m: &Module) -> Option<[f64; 3]> {
    let (stage, spot) = st(|s| (s.stage, s.spot));
    let spot = spot?;
    match stage {
        Stage::Base => Some(spot.aim),
        Stage::Place => click_point(game, spot.base),
        Stage::Hit => {
            let (_, at) = base_crystal(game, m)?;
            let eye = game.eye_position()?;
            let (lo, hi) = crystal_box(at);
            let near: [f64; 3] = std::array::from_fn(|i| eye[i].clamp(lo[i], hi[i]));
            let centre = [at[0], at[1] + 1.0, at[2]];
            let to: [f64; 3] = std::array::from_fn(|i| centre[i] - near[i]);
            let l = to.iter().map(|v| v * v).sum::<f64>().sqrt();
            Some(if l * l > 1.0e-6 { std::array::from_fn(|i| near[i] + to[i] / l * 0.15) } else { near })
        }
        Stage::Idle => None,
    }
}
fn unreachable(game: &Game<'_>, aim: Option<[f64; 3]>) -> bool {
    if st(|s| s.stage) == Stage::Hit {
        return st(|s| s.far);
    }
    let (Some(aim), Some(eye)) = (aim, game.eye_position()) else {
        return false;
    };
    dist(eye, aim) > AIM_LIMIT || st(|s| s.last_eye).is_some_and(|l| dist(l, aim) > AIM_LIMIT)
}
fn target_now(game: &Game<'_>) -> Option<Target> {
    let id = st(|s| s.target)?;
    enemies(game).into_iter().find(|t| t.id == id)
}

// Runs the Aura for this tick. True while a sequence runs (Anchor Aura waits for it).
pub(super) fn tick(game: &mut Game<'_>, m: &Module) -> bool {
    note_crystals(game);
    let tracked = target_now(game).or_else(|| nearest(game, &enemies(game)));
    track(game, tracked.as_ref());
    if st(|s| s.stage) != Stage::Idle {
        let target = target_now(game);
        let valid = target.as_ref().is_some_and(|t| {
            t.health > 0.0 && util::player_pos(game).is_some_and(|feet| dist(feet, t.pos) <= m.n("Max target distance"))
        });
        if !valid {
            stop(game, m, "invalid target");
        } else if let (Some(t), Some(origin)) = (target.as_ref(), st(|s| s.origin)) {
            let now = t.pos.map(|v| v.floor() as i32);
            let drift = (0..3).map(|i| (now[i] - origin[i]).abs()).max().unwrap_or(0);
            let allowed = if st(|s| s.stage) == Stage::Base { 2 } else { m.n("Movement tolerance").round() as i32 };
            if drift > allowed {
                stop(game, m, &format!("target moved {drift}"));
            }
        }
    }
    let stage = st(|s| s.stage);
    if stage != Stage::Idle && st(|s| s.stage_clock.ms()) >= stage_limit(game, stage) {
        if stage == Stage::Hit && !st(|s| s.retried) {
            st(|s| s.retried = true);
            set_stage(Stage::Place);
        } else {
            stop(game, m, &format!("stage took {} ms", stage_limit(game, stage) as i64));
        }
    }
    let manual = m.p("Break mode") == "Manual";
    match st(|s| s.stage) {
        Stage::Idle => {
            give_back(game, m);
            begin(game, m);
        }
        Stage::Hit if manual => {
            let base = st(|s| s.spot.map(|p| p.base));
            if base.is_some_and(|b| !crystal_in(game, up(b))) && st(|s| s.break_clock.past(s.break_roll.ms)) {
                set_stage(Stage::Place);
            }
        }
        _ if acted_now(game) => {}
        _ if m.b("Silent rotation") => silent(game, m),
        _ => plain(game, m),
    }
    let stage = st(|s| s.stage);
    // While it runs, your own clicks wait (a manual hit on its crystal goes through).
    if stage != Stage::Idle {
        game.right = false;
        if !(manual && stage == Stage::Hit) {
            game.left = false;
        }
    }
    let eye = game.eye_position();
    st(|s| s.last_eye = eye);
    stage != Stage::Idle
}

// Silent: the look turns at most 90 degrees a tick toward the stage's aim (the camera stays), with a little jitter,
// and the stage acts once the look is on it.
fn silent(game: &mut Game<'_>, m: &Module) {
    if !due(m) {
        return;
    }
    let aim = stage_aim(game, m);
    if unreachable(game, aim) {
        stop(game, m, "unreachable");
        return;
    }
    let (Some(aim), Some(eye)) = (aim, game.eye_position()) else {
        return;
    };
    let Some(((yaw, pitch), short)) = step_look(game, aim) else {
        return;
    };
    let wobble = (0.18 / dist(eye, aim).max(1.0)).clamp(0.005, 0.15);
    let (jy, jp) = st(|s| {
        let mut rng = s.rng;
        let a = (unit(&mut rng) * 2.0 - 1.0) * wobble;
        let b = (unit(&mut rng) * 2.0 - 1.0) * wobble;
        s.rng = rng;
        (a as f32, b as f32)
    });
    let look = (yaw + jy, (pitch + jp).clamp(-89.9, 89.9));
    if off_camera(game, look) > m.n("Silent FOV") {
        stop(game, m, "outside Silent FOV");
        return;
    }
    if short {
        sunset::turn(NAME, look.0, look.1);
        return;
    }
    act(game, m, look);
}
// Not silent: the camera turns straight onto the aim and the stage acts.
fn plain(game: &mut Game<'_>, m: &Module) {
    if !due(m) {
        return;
    }
    let aim = stage_aim(game, m);
    if unreachable(game, aim) {
        stop(game, m, "unreachable");
        return;
    }
    let Some(look) = aim.and_then(|a| camera_look(game, a)) else {
        return;
    };
    game.yaw = look.0;
    game.pitch = look.1.clamp(-90.0, 90.0);
    act(game, m, (game.yaw, game.pitch));
}
fn act(game: &mut Game<'_>, m: &Module, look: (f32, f32)) {
    match st(|s| s.stage) {
        Stage::Base => put_base(game, m, look),
        Stage::Place => put_crystal(game, m, look),
        Stage::Hit => hit(game, m, look),
        Stage::Idle => {}
    }
}
// The slot for the next click: the first action notes the slot held before. Without Fast swap the change takes this
// tick's action and the click waits for the next.
fn swap(game: &mut Game<'_>, m: &Module, slot: usize) -> bool {
    let held = game.selected;
    st(|s| {
        s.home.get_or_insert(held);
    });
    if held == slot || m.b("Fast swap") {
        return true;
    }
    weapon_hold(game, slot);
    mark_acted(game);
    restart_stage();
    false
}
fn hotbar(game: &Game<'_>, item: &str) -> Option<usize> {
    util::find_hotbar(game, |name| name == item)
}
// Sends the look, then the deed with it; false when another look already waits this tick.
fn send_with_look(
    game: &mut Game<'_>,
    look: (f32, f32),
    deed: impl for<'a, 'b> FnOnce(&'a mut Game<'b>) + Send + 'static,
) -> bool {
    if !super::sunset_combat_control::claim(NAME) {
        return false;
    }
    let finish = super::sunset_combat_control::finish(NAME);
    let session = game.remote.as_ref().map(|r| r.session.counters.clone());
    let taken = sunset::try_turn_then(NAME, look.0, look.1, move |game: &mut Game<'_>| {
        let finish = finish;
        if !finish.current() {
            return;
        }
        let same = session
            .as_ref()
            .is_some_and(|old| game.remote.as_ref().is_some_and(|r| Arc::ptr_eq(old, &r.session.counters)));
        if same {
            deed(game);
        }
    });
    if !taken {
        super::sunset_combat_control::release(NAME);
        return false;
    }
    mark_acted(game);
    true
}
fn use_on(game: &mut Game<'_>, slot: usize, pos: [i32; 3], face: i32, point: [f64; 3]) {
    if game.selected != slot {
        weapon_hold(game, slot);
    }
    let sequence = util::sequence(game);
    game.send(S::UseItemOn(play::UseItemOn {
        hand: 0,
        pos,
        face,
        cursor: std::array::from_fn(|i| (point[i] - f64::from(pos[i])) as f32),
        inside: false,
        world_border: false,
        sequence,
    }));
    game.swing_arm();
}
fn put_base(game: &mut Game<'_>, m: &Module, look: (f32, f32)) {
    let Some(spot) = st(|s| s.spot) else {
        return;
    };
    if base_block(game, spot.base) {
        set_stage(Stage::Place);
        return;
    }
    if taken(game, spot.base) || occupied(game, spot.base, 1.0) {
        stop(game, m, "base blocked");
        return;
    }
    let Some(slot) = hotbar(game, "obsidian") else {
        missing(game, m, "no obsidian in the hotbar");
        return;
    };
    let Some((support, face)) = spot.support else {
        return;
    };
    // The look has to land on the support's face toward the base.
    let Some(point) = util::block_contact(game, look, support, face, 6.0) else {
        return;
    };
    if !swap(game, m, slot) {
        return;
    }
    if send_with_look(game, look, move |game: &mut Game<'_>| use_on(game, slot, support, face, point)) {
        st(|s| {
            s.acted = true;
            s.fresh_base = true;
        });
        set_stage(Stage::Place);
    }
}
fn put_crystal(game: &mut Game<'_>, m: &Module, look: (f32, f32)) {
    let Some(spot) = st(|s| s.spot) else {
        return;
    };
    if !base_block(game, spot.base) {
        return;
    }
    let above = up(spot.base);
    if crystal_in(game, above) {
        set_stage(Stage::Hit);
        return;
    }
    if !util::is_air(game, above) || occupied(game, above, 2.0) {
        stop(game, m, "crystal spot blocked");
        return;
    }
    let Some(target) = target_now(game) else {
        return;
    };
    let mut dealt = damage_to(game, &target, target.pos, spot.base);
    if m.b("Predict target") && moving(target.id) {
        dealt = dealt.max(damage_to(game, &target, led(game, m, &target), spot.base));
    }
    let own = damage_to_self(game, spot.base);
    if dealt < m.n("Min damage") * m.n("Recheck factor") {
        stop(game, m, &format!("target damage now {dealt:.1}"));
        return;
    }
    if too_costly(own, own_health(game), m) {
        stop(game, m, &format!("self damage now {own:.1}"));
        return;
    }
    let Some(slot) = hotbar(game, "end_crystal") else {
        missing(game, m, "no end crystals in the hotbar");
        return;
    };
    let Some((face, point)) = util::block_hit(game, look, spot.base, 6.0) else {
        return;
    };
    if !swap(game, m, slot) {
        return;
    }
    let base = spot.base;
    if send_with_look(game, look, move |game: &mut Game<'_>| use_on(game, slot, base, face, point)) {
        st(|s| {
            s.acted = true;
            s.fresh_base = false;
            s.break_clock.reset();
            draw_rolls(s, m, false, false, true);
        });
        set_stage(Stage::Hit);
    }
}
fn hit(game: &mut Game<'_>, m: &Module, look: (f32, f32)) {
    let Some((id, at)) = base_crystal(game, m) else {
        return;
    };
    let (lo, hi) = crystal_box(at);
    let Some(eye) = game.eye_position() else {
        return;
    };
    let reach = m.n("Reach");
    if !look_meets(eye, look, lo, hi, reach) || st(|s| s.last_eye).is_some_and(|l| !look_meets(l, look, lo, hi, reach)) {
        return;
    }
    let Some(base) = st(|s| s.spot.map(|p| p.base)) else {
        return;
    };
    let own = damage_to_self(game, base);
    if too_costly(own, own_health(game), m) {
        return;
    }
    // Swap attack: the slot held before the sequence (your weapon) comes back first, as its own action.
    if m.p("Break mode") == "Swap attack"
        && let Some(home) = st(|s| s.home)
        && game.selected != home
    {
        weapon_hold(game, home);
        mark_acted(game);
        restart_stage();
        return;
    }
    let sent = send_with_look(game, look, move |game: &mut Game<'_>| {
        game.send(S::Attack(play::Attack { id: id as i32 }));
        game.reset_attack_strength();
        game.swing_arm();
        game.send(S::Punch(Empty));
    });
    if sent {
        st(|s| {
            s.acted = true;
            s.hit_id = Some(id);
            s.place_clock.reset();
            s.retried = false;
            draw_rolls(s, m, false, true, false);
        });
        set_stage(Stage::Place);
    }
}

// ---------------------------------------------------------------- drawing

// A box's fade: toward shown or hidden over 180 ms, eased in and out.
#[derive(Clone, Copy)]
struct Fade {
    shown: bool,
    progress: f64,
    at: Option<Instant>,
}
impl Fade {
    const NEW: Fade = Fade { shown: false, progress: 0.0, at: None };
    fn value(&mut self, shown: bool) -> f64 {
        let now = Instant::now();
        let step = self.at.map_or(0.0, |t| now.duration_since(t).as_secs_f64() / 0.18);
        self.at = Some(now);
        self.shown = shown;
        self.progress = if shown { (self.progress + step).min(1.0) } else { (self.progress - step).max(0.0) };
        -((std::f64::consts::PI * self.progress).cos() - 1.0) / 2.0
    }
}
struct View {
    target: Fade,
    target_at: Option<[i32; 3]>,
    placement: Fade,
    placement_at: Option<[i32; 3]>,
    ghosts: Vec<([i32; 3], Fade)>,
}
impl View {
    const NEW: View = View { target: Fade::NEW, target_at: None, placement: Fade::NEW, placement_at: None, ghosts: Vec::new() };
}
static VIEW: Mutex<View> = Mutex::new(View::NEW);
fn view<R>(f: impl FnOnce(&mut View) -> R) -> R {
    f(&mut VIEW.lock().unwrap_or_else(|e| e.into_inner()))
}
// A colour setting: #AARRGGBB, or #RRGGBB (opaque).
fn argb(text: &str) -> u32 {
    let t = text.trim();
    t.strip_prefix('#')
        .or_else(|| t.strip_prefix("0x"))
        .and_then(|s| u32::from_str_radix(s, 16).ok())
        .map(|n| if n <= 0x00ff_ffff { n | 0xff00_0000 } else { n })
        .unwrap_or(0xffff_ffff)
}
fn with_alpha(color: u32, alpha: f64) -> u32 {
    ((alpha.round().clamp(0.0, 255.0) as u32) << 24) | (color & 0x00ff_ffff)
}
// An alpha, faded and pulsing (60 to 100 % with a 0.9 s pulse).
fn pulsed(alpha: f64, fade: f64, pulse: f64) -> f64 {
    (alpha * fade * (0.6 + 0.4 * pulse)).round().clamp(0.0, 255.0)
}
// The target (feet and head), the base and its crystal (filled until the obsidian is there), and while idle the best
// bases the scan found, fading out down the list.
pub(super) fn draw(
    game: &Game<'_>,
    camera: &sunset::Camera,
    modules: &[Module],
    out: &mut impl sunset::Canvas,
    width: f32,
) {
    let Some(m) = modules.iter().find(|m| m.name == NAME && m.on && m.b("Aura") && m.b("Render")) else {
        return;
    };
    if game.player.is_none() {
        return;
    }
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
    let pulse = 0.5 + 0.5 * (millis / 900.0 * std::f64::consts::TAU).sin();
    let (target, base, stage, candidates) = st(|s| {
        let target = s
            .target
            .and_then(|id| game.mobs.iter().find(|mob| mob.id == id))
            .map(|mob| [mob.position.x.floor() as i32, mob.position.y.floor() as i32, mob.position.z.floor() as i32]);
        (target, s.spot.map(|p| p.base), s.stage, s.candidates.iter().map(|c| up(c.base)).collect::<Vec<_>>())
    });
    let lines = super::sunset_visual::Look::LINES;
    let sides = super::sunset_visual::Look::SIDES;
    let outline = |out: &mut _, cell: [i32; 3], color: u32| {
        let (lo, hi) = cell_box(cell, 1.0);
        super::sunset_visual::draw_box(camera, out, &lines, lo, hi, color, width);
    };
    view(|v| {
        let show = m.b("Render target") && target.is_some();
        if show {
            v.target_at = target;
        }
        let f = v.target.value(show);
        if m.b("Render target")
            && f > 0.01
            && let Some(cell) = v.target_at
        {
            let c = argb(m.t("Target color"));
            let a = pulsed(f64::from(c >> 24), f, pulse);
            if a > 0.0 {
                outline(out, cell, with_alpha(c, a));
                outline(out, up(cell), with_alpha(c, a));
            }
        }
        let show = m.b("Render placement") && base.is_some();
        if show {
            v.placement_at = base;
        }
        let f = v.placement.value(show);
        if m.b("Render placement")
            && f > 0.01
            && let Some(cell) = v.placement_at
        {
            let placed = base_block(game, cell);
            for (box_cell, color, filled) in
                [(cell, argb(m.t("Obsidian color")), !placed), (up(cell), argb(m.t("Crystal color")), true)]
            {
                let a = pulsed(f64::from(color >> 24), f, pulse);
                if filled {
                    let grow = 0.01 * pulse;
                    let (lo, hi) = cell_box(box_cell, 1.0);
                    let fill = with_alpha(color, (a / 4.0).max(1.0));
                    super::sunset_visual::draw_box(camera, out, &sides, lo.map(|x| x - grow), hi.map(|x| x + grow), fill, width);
                }
                outline(out, box_cell, with_alpha(color, a));
            }
        }
        let shown = m.n("Candidates shown").max(1.0) as usize;
        let wanted: Vec<[i32; 3]> = if m.b("Render candidates") && stage == Stage::Idle {
            candidates.iter().take(shown).copied().collect()
        } else {
            Vec::new()
        };
        let c = argb(m.t("Candidate color"));
        let alpha = f64::from(c >> 24);
        let n = wanted.len().max(1) as f64;
        for (i, cell) in wanted.iter().enumerate() {
            if !v.ghosts.iter().any(|(g, _)| g == cell) {
                v.ghosts.push((*cell, Fade::NEW));
            }
            let fade = v.ghosts.iter_mut().find(|(g, _)| g == cell).map_or(0.0, |(_, f)| f.value(true));
            let a = pulsed((alpha - alpha * i as f64 / n).max(30.0), fade, pulse);
            if a > 0.0 {
                outline(out, *cell, with_alpha(c, a));
            }
        }
        let ghosts = m.b("Render candidates");
        v.ghosts.retain_mut(|(cell, fade)| {
            if wanted.contains(cell) {
                return true;
            }
            let f = fade.value(false);
            if f < 0.01 {
                return false;
            }
            let a = pulsed(30.0, f, pulse);
            if ghosts && a > 0.0 {
                outline(out, *cell, with_alpha(c, a));
            }
            true
        });
    });
}
