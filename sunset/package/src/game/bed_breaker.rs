// Bed Breaker (BedWars; asked by vs.): the nearest bed in Range that is not yours is broken, and with Cover on Break
// what stands between you and it first (the block the line from your eye to the bed meets), one block at a time. Each
// dig is the game's own, as Vein Miner's: START, a swing every tick, STOP once the block's progress is full, never two
// digs in a tick, the look turned to the block first with Rotate. Cover on Ignore digs the bed itself through what
// covers it (a server that does not check the line takes that; one that does refuses it). Spare own bed leaves the
// bed nearest you when the module came on (within 16 blocks: your base's) alone. It waits while you mine, use an
// item or have a menu open, and a dig the server is still counting is dropped when it stops (PlayerAction 0 start, 2
// abort, 3 stop). No movement packets are sent. Behaviour after other clients' bed breakers (LiquidBounce's Fucker
// among them); written for Sunset.
use crate::game::sunset_compat::SunsetGame as _;
use super::sunset::{Camera, Canvas, Category, Module, cancel_turns, gui_open};
use super::sunset_util::{center_of, dist, sequence, turned_to};
use super::{Dim, Game, Screen};
use crate::game::sunset_packets::{Link, Linked};
use horizon_core::geometry::Direction;
use horizon_protocol::packets::Empty;
use horizon_protocol::packets::play::{PlayerAction, Serverbound};
use std::sync::Mutex;

pub(super) const NAME: &str = "Bed Breaker";
// Ticks to wait for the server to clear a block, or for a turn to go out.
const WAIT: u64 = 20;
// A bed within this of you as the module comes on is yours.
const OWN: f64 = 16.0;
type Pos = [i32; 3];

pub(super) fn defaults() -> Vec<Module> {
    vec![Module::new(NAME, Category::World, "Breaks beds near you, and what covers them (BedWars).")
        .num("Range", 4.5, 2.0, 6.0, 0.5)
        .pick("Cover", &["Break", "Ignore"])
        .bool("Spare own bed", true)
        .bool("Rotate", true)
        .bool("Render", true)]
}

enum Phase {
    Idle,
    // START asked for; waits for it to go out.
    Start(Pos, u64),
    Dig(Pos, f32),
    // STOP asked for; waits for the block to go.
    Stop(Pos, u64),
}

struct Breaker {
    link: Link,
    life: (i32, Dim),
    // Your own bed's blocks, found as the module came on (None: none near, or not spared).
    own: Option<Pos>,
    phase: Phase,
    // The bed being gone for (drawn), and the block being dug for it (the bed or its cover).
    bed: Option<Pos>,
}

static STATE: Mutex<Option<Breaker>> = Mutex::new(None);
// The last dig action that went out: block, face, action.
static SENT: Mutex<Option<(Pos, u8, i32)>> = Mutex::new(None);

fn sent() -> std::sync::MutexGuard<'static, Option<(Pos, u8, i32)>> {
    SENT.lock().unwrap_or_else(|e| e.into_inner())
}

fn state() -> std::sync::MutexGuard<'static, Option<Breaker>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn loaded(game: &Game<'_>, [x, _, z]: Pos) -> bool {
    game.world.chunks.contains_key(&(x >> 4, z >> 4))
}

fn name_at<'a>(game: &'a Game<'_>, pos: Pos) -> &'a str {
    game.res.facts().get(game.world.block(pos[0], pos[1], pos[2])).name.as_ref()
}

fn bed_at(game: &Game<'_>, pos: Pos) -> bool {
    loaded(game, pos) && name_at(game, pos).ends_with("_bed")
}

fn gone(game: &Game<'_>, pos: Pos) -> bool {
    loaded(game, pos) && game.res.facts().is_air(game.world.block(pos[0], pos[1], pos[2]))
}

// The beds' blocks within range of the eye, nearest first.
fn beds(game: &Game<'_>, eye: [f64; 3], range: f64) -> Vec<Pos> {
    let r = range.ceil() as i32 + 1;
    let at = eye.map(|v| v.floor() as i32);
    let mut found: Vec<(f64, Pos)> = Vec::new();
    for x in at[0] - r..=at[0] + r {
        for y in at[1] - r..=at[1] + r {
            for z in at[2] - r..=at[2] + r {
                let pos = [x, y, z];
                let far = dist(eye, center_of(pos));
                if far <= range && bed_at(game, pos) {
                    found.push((far, pos));
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|f| f.1).collect()
}

// A bed's block is your own bed's: the block found as the module came on, or its other half beside it.
fn own(own: Option<Pos>, pos: Pos) -> bool {
    own.is_some_and(|o| (0..3).map(|i| (o[i] - pos[i]).abs()).sum::<i32>() <= 1)
}

// What to dig to get at a bed, and by which face: the bed itself where the line from the eye meets it first (or with
// Cover on Ignore), else the block in the way, if it can be broken.
fn target(game: &Game<'_>, eye: [f64; 3], bed: Pos, ignore_cover: bool) -> Option<(Pos, u8)> {
    let center = center_of(bed);
    let delta: [f64; 3] = std::array::from_fn(|i| center[i] - eye[i]);
    let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    if length == 0.0 {
        return None;
    }
    let face_of = |normal: [i32; 3]| Direction::ALL.iter().position(|d| d.normal() == normal).map(|i| i as u8);
    let hit = game.clip_ray(eye, delta.map(|d| d / length), length + 0.01);
    match hit {
        Some(hit) if hit.pos == bed => face_of(hit.face).map(|face| (bed, face)),
        Some(hit) if !ignore_cover => {
            let facts = game.res.facts().get(game.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
            // Bedrock and the like can't be dug: nothing to do from here.
            (facts.hardness >= 0.0 && !game.block_action_restricted(hit.pos)).then(|| face_of(hit.face).map(|face| (hit.pos, face))).flatten()
        }
        // Through the cover, or nothing met at all (the bed's low box under the line): its top.
        _ => Some((bed, 1)),
    }
}

// The arm swings, and the server hears it (Punch), as each tick of the game's own mining does.
fn swing(game: &mut Game<'_>) {
    game.swing_arm();
    game.send(Serverbound::Punch(Empty));
}

// Sends a dig action with a swing, after the server's look has turned to the block when Rotate says so.
fn dig(game: &mut Game<'_>, rotate: bool, action: i32, pos: Pos, face: u8) {
    turned_to(game, NAME, rotate, center_of(pos), move |game| {
        let sequence = sequence(game);
        game.send(Serverbound::PlayerAction(PlayerAction { action, pos, direction: face, sequence }));
        swing(game);
        *sent() = Some((pos, face, action));
    });
}

// Off, or the world changed: what is asked for is dropped, and a dig the server is counting aborted.
pub(super) fn stop(game: &mut Game<'_>) {
    let Some(b) = state().take() else {
        return;
    };
    cancel_turns(NAME);
    let same = game.remote.as_ref().is_some_and(|r| b.link.to(Some(&r.session)) == Linked::Same && b.life == (r.entity_id, game.dim));
    if let Some((pos, face, 0)) = sent().take()
        && same
    {
        let sequence = sequence(game);
        game.send(Serverbound::PlayerAction(PlayerAction { action: 2, pos, direction: face, sequence }));
    }
}

pub(super) fn tick(game: &mut Game<'_>, m: Option<&Module>) {
    let Some(m) = m else {
        stop(game);
        return;
    };
    let live = game.native_playing()
        && game.screen == Screen::Playing
        && !gui_open()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0);
    let (Some(remote), Some(eye)) = (game.remote.as_ref().filter(|_| live), game.eye_position()) else {
        stop(game);
        return;
    };
    let (link, life) = (Link::of(&remote.session), (remote.entity_id, game.dim));
    // Another world or life: start afresh there (its own bed is another).
    if state().as_ref().is_some_and(|b| b.link.to(Some(&remote.session)) != Linked::Same || b.life != life) {
        stop(game);
    }
    if state().is_none() {
        let own = m.b("Spare own bed").then(|| beds(game, eye, OWN).into_iter().next()).flatten();
        *sent() = None;
        *state() = Some(Breaker { link, life, own, phase: Phase::Idle, bed: None });
    }
    // Your own mining and item use come first: a dig under way is dropped for them.
    let busy = game.mining.is_some() || game.left || game.using.is_some();
    let (range, rotate, ignore_cover) = (m.n("Range"), m.b("Rotate"), m.p("Cover") == "Ignore");
    let mut guard = state();
    let Some(b) = guard.as_mut() else {
        return;
    };
    if busy {
        if !matches!(b.phase, Phase::Idle) {
            b.phase = Phase::Idle;
            b.bed = None;
            drop(guard);
            cancel_turns(NAME);
            if let Some((pos, face, 0)) = sent().take() {
                let sequence = sequence(game);
                game.send(Serverbound::PlayerAction(PlayerAction { action: 2, pos, direction: face, sequence }));
            }
        }
        return;
    }
    match b.phase {
        Phase::Start(pos, until) => {
            if sent().is_some_and(|(at, _, action)| at == pos && action == 0) {
                b.phase = Phase::Dig(pos, 0.0);
            } else if game.ticks >= until {
                b.phase = Phase::Idle;
            }
            return;
        }
        Phase::Dig(pos, progress) => {
            if gone(game, pos) {
                *sent() = None;
                b.phase = Phase::Idle;
                return;
            }
            // Out of reach meanwhile, or the bed it was for is gone: the dig is dropped.
            let lost = dist(eye, center_of(pos)) > range + 1.0 || b.bed.is_none_or(|bed| !bed_at(game, bed));
            if lost {
                b.phase = Phase::Idle;
                b.bed = None;
                drop(guard);
                if let Some((pos, face, 0)) = sent().take() {
                    let sequence = sequence(game);
                    game.send(Serverbound::PlayerAction(PlayerAction { action: 2, pos, direction: face, sequence }));
                }
                return;
            }
            let progress = progress + game.destroy_progress(pos);
            if progress >= 1.0 {
                let face = sent().map_or(1, |(_, face, _)| face);
                b.phase = Phase::Stop(pos, game.ticks + WAIT);
                drop(guard);
                dig(game, rotate, 3, pos, face);
            } else {
                b.phase = Phase::Dig(pos, progress);
                drop(guard);
                swing(game);
            }
            return;
        }
        Phase::Stop(pos, until) => {
            if gone(game, pos) || game.ticks >= until {
                *sent() = None;
                b.phase = Phase::Idle;
            }
            // The next START waits a tick: never two digs in one tick.
            return;
        }
        Phase::Idle => {}
    }
    let spared = b.own;
    let found = beds(game, eye, range).into_iter().filter(|bed| !own(spared, *bed)).find_map(|bed| target(game, eye, bed, ignore_cover).map(|t| (bed, t)));
    let Some((bed, (pos, face))) = found else {
        b.bed = None;
        return;
    };
    b.bed = Some(bed);
    b.phase = Phase::Start(pos, game.ticks + WAIT);
    drop(guard);
    dig(game, rotate, 0, pos, face);
}

// Render: a box on the bed it is going for.
pub(super) fn draw(cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    if !mods.iter().any(|m| m.name == NAME && m.on && m.b("Render")) {
        return;
    }
    let Some(bed) = state().as_ref().and_then(|b| b.bed) else {
        return;
    };
    let lo = bed.map(f64::from);
    let hi = [lo[0] + 1.0, lo[1] + 0.5625, lo[2] + 1.0];
    super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED, lo, hi, 0xFFFF_5555, lw);
}
