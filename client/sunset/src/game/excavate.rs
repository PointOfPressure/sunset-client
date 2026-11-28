// .excavate <x1> <y1> <z1> <x2> <y2> <z2> | stop: empties a box of its blocks, each broken the way .mine breaks one
// (the crosshair on it, the attack button held, the game's own dig). Of the blocks the crosshair reaches from where
// you stand the highest goes first, so a box is taken from the top and you come down into the pit as it deepens;
// when none is in reach you walk up to the nearest one open space touches on .goto's walker. A block under your feet
// with a drop of more than three blocks below it, one that touches lava or water (nothing here seals a spring),
// bedrock, and one with no way or line to it are left and counted. It rides .goto's task, lease and stops (goto.rs)
// as .mine does; an open screen only holds it. The idea is Meteor's Excavator, which drives Baritone; this is
// written fresh on Horizon's agent, none of its code.
use super::sunset_goto as walk;
use super::{Game, TaskKind};
use crate::game::sunset_util as util;
use std::collections::HashSet;
use std::sync::Mutex;

// A block still whole after this long is the wrong tool's.
const DIG_SECONDS: f64 = 20.0;
// The longest side, the most blocks in a box, and how far from you it may start.
const SIDE: i32 = 64;
const MOST: i64 = 32_768;
const FAR: f64 = 64.0;

enum Phase {
    // Nothing running: the next block, or a walk up to one.
    Seek,
    Dig,
    Walk,
}

struct Excavation {
    lo: [i32; 3],
    hi: [i32; 3],
    dim: &'static str,
    dug: u32,
    // Since the last block dug (it can open a line or a way): blocks walked up to or found with no way to them, one
    // walk each, and blocks the crosshair lost on the way down.
    walked: HashSet<[i32; 3]>,
    lost: HashSet<[i32; 3]>,
    target: [i32; 3],
    phase: Phase,
    wait_tick: u64,
}
static EXCAVATION: Mutex<Option<Excavation>> = Mutex::new(None);

fn job() -> std::sync::MutexGuard<'static, Option<Excavation>> {
    EXCAVATION.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn dim() -> Option<&'static str> {
    job().as_ref().map(|m| m.dim)
}

fn at(pos: [i32; 3]) -> String {
    format!("{} {} {}", pos[0], pos[1], pos[2])
}

// Something to break: a block with an outline the crosshair can take, bedrock and the like aside.
fn to_dig(game: &Game<'_>, pos: [i32; 3]) -> bool {
    let f = util::facts(game, pos);
    !util::is_air(game, pos) && !f.outline.is_empty() && f.hardness >= 0.0
}

// Lava or water, standing or flowing, with no block of its own round it.
fn liquid(game: &Game<'_>, pos: [i32; 3]) -> bool {
    let f = util::facts(game, pos);
    f.fluid != horizon_client::blocks::Fluid::None && f.outline.is_empty()
}

fn whole(game: &Game<'_>, pos: [i32; 3]) -> bool {
    let f = util::facts(game, pos);
    f.full_collision || f.legacy_solid
}

const SIDES: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

fn next_to(pos: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]]
}

impl Excavation {
    fn cells(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        let (lo, hi) = (self.lo, self.hi);
        (lo[1]..=hi[1]).rev().flat_map(move |y| (lo[0]..=hi[0]).flat_map(move |x| (lo[2]..=hi[2]).map(move |z| [x, y, z])))
    }

    fn size(&self) -> String {
        let [a, b, c] = std::array::from_fn(|i| self.hi[i] - self.lo[i] + 1);
        format!("{a}x{b}x{c} from {} to {}", at(self.lo), at(self.hi))
    }

    fn tally(&self) -> String {
        if self.dug == 1 { "1 block".into() } else { format!("{} blocks", self.dug) }
    }

    fn stopped(&self, why: &str) -> String {
        let why = why.trim_end_matches('.');
        if why.is_empty() {
            format!("Stopped excavating after {}.", self.tally())
        } else {
            format!("Stopped excavating after {}: {why}.", self.tally())
        }
    }

    fn done(&self, game: &Game<'_>) -> String {
        let left = self.cells().filter(|&pos| to_dig(game, pos)).count();
        let more = match left {
            0 => String::new(),
            1 => "; 1 block is left: no way or line to it, or not safe to dig (a drop under, sand or gravel over, lava or water beside)".into(),
            n => format!("; {n} blocks are left: no way or line to them, or not safe to dig (a drop under, sand or gravel over, lava or water beside)"),
        };
        format!("Excavated {}{more}.", self.tally())
    }
}

// A dig cut short (a stop, a screen) after its block went but before its answer came: the block still counts.
fn settle(game: &Game<'_>, m: &mut Excavation) {
    if matches!(m.phase, Phase::Dig) && !to_dig(game, m.target) {
        m.dug += 1;
    }
}

// The job ended by whatever stops a walk: what to say for it.
pub(super) fn stopped(game: &Game<'_>, why: &str) -> Option<String> {
    let mut m = job().take()?;
    settle(game, &mut m);
    let why = why.strip_prefix("stopped: ").or_else(|| why.strip_prefix("stopped ")).unwrap_or(why);
    Some(m.stopped(why))
}

// A screen is up: the job stands (goto.rs drops what it had going) and looks again once the screen has shut.
pub(super) fn hold(game: &Game<'_>) -> bool {
    let mut held = job();
    let Some(m) = held.as_mut() else { return false };
    settle(game, m);
    m.phase = Phase::Seek;
    m.walked.clear();
    m.lost.clear();
    true
}

// .excavate stop (and .goto stop): ends the job without a word from the task it had going.
pub(super) fn stop(game: &mut Game<'_>) -> Option<String> {
    let mut m = job().take()?;
    settle(game, &mut m);
    walk::drop_walk(game);
    Some(m.stopped(""))
}

// A walk to within `near` of the block, all the way (a path that only gets nearer is no use to a crosshair).
fn approach(game: &mut Game<'_>, pos: [i32; 3], near: f64) -> Result<bool, String> {
    let middle = [f64::from(pos[0]) + 0.5, f64::from(pos[1]), f64::from(pos[2]) + 0.5];
    let Some(goto) = walk::plan(game, middle, near, false).filter(|g| g.path.complete && g.path.nodes.len() > 1) else {
        return Ok(false);
    };
    walk::run(game, TaskKind::Goto(Box::new(goto)), 40.0)?;
    Ok(true)
}

pub(crate) fn parse(args: &[&str]) -> Result<([i32; 3], [i32; 3]), String> {
    let usage = "Use .excavate <x1> <y1> <z1> <x2> <y2> <z2> (two corners, whole block coordinates) or .excavate stop.";
    let n: Vec<i32> = args.iter().map(|w| w.parse::<i32>()).collect::<Result<_, _>>().map_err(|_| usage.to_string())?;
    let [x1, y1, z1, x2, y2, z2] = n[..] else { return Err(usage.into()) };
    let (lo, hi) = ([x1.min(x2), y1.min(y2), z1.min(z2)], [x1.max(x2), y1.max(y2), z1.max(z2)]);
    if lo[0].unsigned_abs() > 29_999_983 || hi[0].unsigned_abs() > 29_999_983 || lo[2].unsigned_abs() > 29_999_983
        || hi[2].unsigned_abs() > 29_999_983 || lo[1] < -64 || hi[1] > 319
    {
        return Err("That is outside the world.".into());
    }
    let sides: [i32; 3] = std::array::from_fn(|i| hi[i] - lo[i] + 1);
    if sides.iter().any(|&s| s > SIDE) {
        return Err(format!("A box is at most {SIDE} blocks on a side."));
    }
    let volume: i64 = sides.iter().map(|&s| i64::from(s)).product();
    if volume > MOST {
        return Err(format!("That box holds {volume} blocks; .excavate takes up to {MOST}."));
    }
    Ok((lo, hi))
}

pub(crate) fn command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    match args {
        [] => {
            return Ok(job().as_ref().map_or_else(
                || "Use .excavate <x1> <y1> <z1> <x2> <y2> <z2> or .excavate stop.".to_string(),
                |m| format!("Excavating {}: {} so far. .excavate stop stops.", m.size(), m.tally()),
            ));
        }
        [word] if word.eq_ignore_ascii_case("stop") => {
            return Ok(stop(game).unwrap_or_else(|| "Not excavating anything.".into()));
        }
        _ => {}
    }
    let (lo, hi) = parse(args)?;
    game.require_playing_screen().map_err(|e| e.to_string())?;
    if game.screen_over_playing() || game.sleeping.is_some() {
        return Err("Close the prompt and wake up before walking.".into());
    }
    let p = game.player.as_ref().ok_or("No player.")?;
    if p.flying || !p.ground.on_ground {
        return Err("Stand on the ground first.".into());
    }
    let me = [p.position.x, p.position.y, p.position.z];
    let nearest: [f64; 3] = std::array::from_fn(|i| me[i].clamp(f64::from(lo[i]), f64::from(hi[i]) + 1.0));
    let far = util::dist(me, nearest);
    if far > FAR {
        return Err(format!("That box is {far:.0} blocks away; .excavate starts within {FAR:.0}."));
    }
    let loaded = (lo[0] >> 4..=hi[0] >> 4).all(|cx| (lo[2] >> 4..=hi[2] >> 4).all(|cz| game.world.chunks.contains_key(&(cx, cz))));
    if !loaded {
        return Err("Part of that box is not loaded.".into());
    }
    crate::game::sunset_movement_owner::can_acquire(game)?;
    let m = Excavation {
        lo,
        hi,
        dim: game.dim.id(),
        dug: 0,
        walked: HashSet::new(),
        lost: HashSet::new(),
        target: [0; 3],
        phase: Phase::Seek,
        wait_tick: game.ticks,
    };
    let count = m.cells().filter(|&pos| to_dig(game, pos)).count();
    if count == 0 {
        return Err("Nothing to dig in that box.".into());
    }
    walk::cancel(game, "stopped: a new walking goal started");
    let blocks = if count == 1 { "1 block".to_string() } else { format!("{count} blocks") };
    let text = format!("Excavating {}: {blocks} to dig. .excavate stop stops.", m.size());
    *job() = Some(m);
    Ok(text)
}

// The job's turn in a frame: with nothing running, the next block.
pub(super) fn frame(game: &mut Game<'_>, paused: bool) {
    let Some(mut m) = job().take() else { return };
    if paused || !matches!(m.phase, Phase::Seek) || walk::owns(game) || game.ticks < m.wait_tick
        || !game.player.as_ref().is_some_and(|p| !p.flying && p.ground.on_ground)
    {
        *job() = Some(m);
        return;
    }
    match seek(game, &mut m) {
        Ok(true) => *job() = Some(m),
        Ok(false) => walk::report(game, &m.done(game)),
        Err(e) => walk::report(game, &m.stopped(&e)),
    }
}

// Sand, gravel, concrete powder, an anvil: what falls when the block under it goes.
fn falls(game: &Game<'_>, pos: [i32; 3]) -> bool {
    let id = util::short(&util::facts(game, pos).name);
    id.ends_with("sand") || id.ends_with("gravel") || id.ends_with("_powder") || id.contains("anvil")
}

// Whether a block may go now: not one beside lava or water, not one under sand or gravel outside the box (inside,
// that goes first: it is higher), and not the one you stand on over a drop.
fn may_dig(game: &Game<'_>, m: &Excavation, pos: [i32; 3]) -> bool {
    if SIDES.iter().any(|&d| liquid(game, next_to(pos, d))) {
        return false;
    }
    let above = next_to(pos, [0, 1, 0]);
    if above[1] > m.hi[1] && falls(game, above) {
        return false;
    }
    let Some(p) = game.player.as_ref() else { return false };
    let (lo, hi) = { let b = p.bounds(); (b.min_position(), b.max_position()) };
    let under = pos[1] == p.position.y.round() as i32 - 1
        && lo.x < f64::from(pos[0] + 1) && hi.x > f64::from(pos[0])
        && lo.z < f64::from(pos[2] + 1) && hi.z > f64::from(pos[2]);
    !under || (1..=3).any(|d| whole(game, [pos[0], pos[1] - d, pos[2]]))
}

// One thing a frame: the highest block the crosshair reaches from here, else a walk up to the nearest block open
// space touches. Ok(false): nothing is left to try.
fn seek(game: &mut Game<'_>, m: &mut Excavation) -> Result<bool, String> {
    let eye = game.eye_position().ok_or("no player")?;
    let far = |pos: &[i32; 3]| util::dist(eye, util::center_of(*pos));
    let left: Vec<[i32; 3]> = m.cells().filter(|&pos| to_dig(game, pos) && may_dig(game, m, pos)).collect();
    // In reach, the highest first and of those the nearest: the crosshair tries each until one takes.
    let reach = game.reach() + 0.5;
    let mut near: Vec<[i32; 3]> = left.iter().copied().filter(|pos| far(pos) <= reach && !m.lost.contains(pos)).collect();
    near.sort_by(|a, b| b[1].cmp(&a[1]).then(far(a).total_cmp(&far(b))));
    for pos in near {
        if walk::dig(game, pos, DIG_SECONDS)? {
            m.target = pos;
            m.phase = Phase::Dig;
            return Ok(true);
        }
    }
    // None from here: a walk up to the nearest that open space touches and that has not been walked to since the
    // last block went.
    let open = |pos: [i32; 3]| SIDES.iter().any(|&d| !whole(game, next_to(pos, d)));
    let mut ways: Vec<[i32; 3]> = left.into_iter().filter(|&pos| open(pos) && !m.walked.contains(&pos)).collect();
    ways.sort_by(|a, b| far(a).total_cmp(&far(b)));
    for pos in ways {
        m.walked.insert(pos);
        if approach(game, pos, 2.5)? {
            m.target = pos;
            m.phase = Phase::Walk;
            return Ok(true);
        }
    }
    Ok(false)
}

// The answer of a task the job started. False: no job, the answer is .goto's own.
pub(super) fn answer(game: &mut Game<'_>, v: &serde_json::Value) -> bool {
    let Some(mut m) = job().take() else { return false };
    if v["cancelled"].as_bool() == Some(true) {
        // Horizon ended the task itself (its agent's stop, the window closing): the job goes with it.
        walk::report(game, &m.stopped(""));
        return true;
    }
    let next = match std::mem::replace(&mut m.phase, Phase::Seek) {
        Phase::Seek | Phase::Walk => Ok(()),
        Phase::Dig => dug(game, &mut m, v),
    };
    match next {
        Ok(()) => *job() = Some(m),
        Err(e) => walk::report(game, &m.stopped(&e)),
    }
    true
}

fn dug(game: &Game<'_>, m: &mut Excavation, v: &serde_json::Value) -> Result<(), String> {
    if v.is_null() {
        m.lost.insert(m.target);
        return Ok(());
    }
    if let Some(error) = v["error"].as_str() {
        if error.starts_with("timed out") {
            let block = v["block"].as_str().map_or("a block", util::short).to_string();
            return Err(format!("{block} at {} did not break in {DIG_SECONDS:.0} s with what you hold", at(m.target)));
        }
        // The crosshair lost it (pushed, something fell in the way): it waits for the next block to go.
        m.lost.insert(m.target);
        return Ok(());
    }
    m.dug += 1;
    // What stood behind it can be seen now, and walked to again.
    m.walked.clear();
    m.lost.clear();
    m.wait_tick = game.ticks + 1;
    Ok(())
}
