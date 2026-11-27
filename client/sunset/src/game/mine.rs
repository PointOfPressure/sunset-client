// .mine <block> [count] | stop: walks to the nearest block of a kind that open space touches, breaks it the way
// horizon-ctl's break does (the crosshair on it, the attack button held, the game's own dig), walks over what fell
// and goes on to the next. It only walks where .goto walks and never tunnels: a block it cannot walk up to or put
// the crosshair on is left, and counted at the end. It rides .goto's task, lease and stops (goto.rs); an open screen
// only holds it, as it does a follow.
use crate::game::sunset_compat::SunsetGame as _;
use super::sunset_goto as walk;
use super::{Break, Game, TaskKind};
use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

// How far it looks: a square of 32 blocks round you, 12 up and down.
const RADIUS: i32 = 32;
const HEIGHT: i32 = 12;
const MOST: u32 = 1024;
// A block still whole after this long is the wrong tool's.
const DIG_SECONDS: f64 = 20.0;

enum Phase {
    // Nothing running: pick the next block.
    Seek,
    // Walking up to the block; `closer`: a walk right up to it is still to try if the crosshair can't find it.
    Walk { closer: bool },
    Dig,
    // Walking over to what fell (gather).
    Collect,
}

struct Mine {
    word: String,
    left: u32,
    mined: u32,
    dim: &'static str,
    // Blocks given up on: no way to them, or no line to them from where the way ends.
    skip: HashSet<[i32; 3]>,
    // What the last look found, farthest first; looked for again after each block mined (a vein opens up).
    found: Option<Vec<[i32; 3]>>,
    target: [i32; 3],
    // The block just mined, and the walks made so far to what fell from it.
    fell: Option<([i32; 3], u8)>,
    phase: Phase,
    wait_tick: u64,
}
static MINE: Mutex<Option<Mine>> = Mutex::new(None);

fn mine() -> std::sync::MutexGuard<'static, Option<Mine>> {
    MINE.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn dim() -> Option<&'static str> {
    mine().as_ref().map(|m| m.dim)
}

// A dig cut short (a stop, a screen) after its block went but before its answer came: the block still counts.
fn settle(game: &Game<'_>, m: &mut Mine) {
    if matches!(m.phase, Phase::Dig) && !is_wanted(game, &m.word, m.target) {
        m.mined += 1;
        m.left = m.left.saturating_sub(1);
        m.fell = Some((m.target, 0));
    }
}

// The job ended by whatever stops a walk: what to say for it.
pub(super) fn stopped(game: &Game<'_>, why: &str) -> Option<String> {
    let mut m = mine().take()?;
    settle(game, &mut m);
    let why = why.strip_prefix("stopped: ").or_else(|| why.strip_prefix("stopped ")).unwrap_or(why);
    Some(format!("Stopped mining {} after {}: {why}.", m.word, m.mined))
}

// A screen is up: the job stands (goto.rs drops what it had going) and looks again once the screen has shut.
pub(super) fn hold(game: &Game<'_>) -> bool {
    let mut job = mine();
    let Some(m) = job.as_mut() else { return false };
    settle(game, m);
    m.phase = Phase::Seek;
    m.found = None;
    true
}

// .mine stop (and .goto stop): ends the job without a word from the task it had going.
pub(super) fn stop(game: &mut Game<'_>) -> Option<String> {
    let mut m = mine().take()?;
    settle(game, &mut m);
    walk::drop_walk(game);
    Some(format!("Stopped mining {} after {}.", m.word, m.mined))
}

// Whether a block's id is what `word` asks for: the id itself, a kind of ore (diamond: diamond_ore and
// deepslate_diamond_ore; quartz and gold: the nether's too; debris: ancient_debris), or any log for "log".
fn wanted(word: &str, name: &str) -> bool {
    let id = name.strip_prefix("minecraft:").unwrap_or(name);
    if id == word {
        return true;
    }
    match word {
        "log" | "logs" => id.ends_with("_log") || id.ends_with("_stem"),
        "debris" => id == "ancient_debris",
        _ => id.strip_suffix("_ore").is_some_and(|ore| {
            ore == word || ore.strip_prefix("deepslate_") == Some(word) || ore.strip_prefix("nether_") == Some(word)
        }),
    }
}

fn centre(pos: [i32; 3]) -> [f64; 3] {
    [f64::from(pos[0]) + 0.5, f64::from(pos[1]), f64::from(pos[2]) + 0.5]
}

fn is_wanted(game: &Game<'_>, word: &str, pos: [i32; 3]) -> bool {
    wanted(word, &game.res.facts().get(game.world.block(pos[0], pos[1], pos[2])).name)
}

// Every block of the kind round you that open space touches (a side with no full block against it), farthest first.
fn look(game: &Game<'_>, word: &str, skip: &HashSet<[i32; 3]>) -> Vec<[i32; 3]> {
    let Some(p) = game.player.as_ref() else { return Vec::new() };
    let me = [p.position.x, p.position.y, p.position.z];
    let at = [me[0].floor() as i32, me[1].floor() as i32, me[2].floor() as i32];
    let facts = game.res.facts();
    let open = |x: i32, y: i32, z: i32| {
        let f = facts.get(game.world.block(x, y, z));
        !(f.full_collision || f.legacy_solid)
    };
    // Most of what a look reads is a handful of states (stone, air): the last answer is kept.
    let mut last = None;
    let mut found = Vec::new();
    for y in (at[1] - HEIGHT).max(-64)..=(at[1] + HEIGHT).min(319) {
        for x in at[0] - RADIUS..=at[0] + RADIUS {
            for z in at[2] - RADIUS..=at[2] + RADIUS {
                let state = game.world.block(x, y, z);
                let hit = match last {
                    Some((s, hit)) if s == state => hit,
                    _ => {
                        let hit = wanted(word, &facts.get(state).name);
                        last = Some((state, hit));
                        hit
                    }
                };
                if hit && !skip.contains(&[x, y, z])
                    && (open(x + 1, y, z) || open(x - 1, y, z) || open(x, y + 1, z) || open(x, y - 1, z)
                        || open(x, y, z + 1) || open(x, y, z - 1))
                {
                    found.push([x, y, z]);
                }
            }
        }
    }
    let far = |b: &[i32; 3]| {
        let c = centre(*b);
        (c[0] - me[0]).powi(2) + (c[1] - me[1]).powi(2) + (c[2] - me[2]).powi(2)
    };
    found.sort_by(|a, b| far(b).total_cmp(&far(a)));
    found
}

// Puts the crosshair on the block and holds the attack button on it. Ok(false): no line to it from here.
fn dig(game: &mut Game<'_>, pos: [i32; 3]) -> Result<bool, String> {
    let before = game.world.block(pos[0], pos[1], pos[2]);
    if game.aim_block(pos, None).is_err() {
        return Ok(false);
    }
    let task = Break {
        pos,
        before,
        items_before: game.dropped.iter().map(|i| i.id).collect(),
        inventory: game.inventory_counts(),
        started_tick: game.ticks,
        collect_until: None,
        broke_tick: 0,
        drops: BTreeMap::new(),
    };
    walk::run(game, TaskKind::Break(Box::new(task)), DIG_SECONDS)?;
    if let Err(e) = game.agent_mouse(winit::event::MouseButton::Left, true) {
        walk::drop_walk(game);
        return Err(e.to_string());
    }
    Ok(true)
}

// A walk to within `near` of the block, all the way (a path that only gets nearer is no use to a crosshair).
fn approach(game: &mut Game<'_>, pos: [i32; 3], near: f64) -> Result<bool, String> {
    let Some(goto) = walk::plan(game, centre(pos), near, false).filter(|g| g.path.complete) else {
        return Ok(false);
    };
    walk::run(game, TaskKind::Goto(Box::new(goto)), 40.0)?;
    Ok(true)
}

fn done(m: &Mine) -> String {
    let left = m.skip.len();
    let more = match left {
        0 => String::new(),
        1 => "; 1 more in sight could not be reached".into(),
        n => format!("; {n} more in sight could not be reached"),
    };
    format!("Mined {} {}{more}.", m.mined, m.word)
}

pub(crate) fn command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    let usage = "Use .mine <block> [how many] (diamond, iron, log, or a block's id) or .mine stop.";
    let (word, left) = match args {
        [] => {
            return Ok(mine().as_ref().map_or_else(|| usage.to_string(), |m| {
                format!("Mining {}: {} so far. .mine stop stops.", m.word, m.mined)
            }));
        }
        [word] if word.eq_ignore_ascii_case("stop") => {
            return Ok(stop(game).unwrap_or_else(|| "Not mining anything.".into()));
        }
        [word] => (*word, MOST),
        [word, count] => {
            let count = count.parse::<u32>().ok().filter(|n| (1..=MOST).contains(n))
                .ok_or_else(|| format!("{count} is not a number of blocks from 1 to {MOST}."))?;
            (*word, count)
        }
        _ => return Err(usage.into()),
    };
    let word = word.to_ascii_lowercase();
    let word = word.strip_prefix("minecraft:").unwrap_or(&word).to_string();
    game.require_playing_screen().map_err(|e| e.to_string())?;
    if game.screen_over_playing() || game.sleeping.is_some() { return Err("Close the prompt and wake up before walking.".into()); }
    let p = game.player.as_ref().ok_or("No player.")?;
    if p.flying || !p.ground.on_ground {
        return Err("Stand on the ground first.".into());
    }
    crate::game::sunset_movement_owner::can_acquire(game)?;
    let found = look(game, &word, &HashSet::new());
    if found.is_empty() {
        return Err(format!("No {word} in sight within {RADIUS} blocks (covered blocks don't count)."));
    }
    walk::cancel(game, "stopped: a new walking goal started");
    let seen = found.len();
    *mine() = Some(Mine {
        word: word.clone(),
        left,
        mined: 0,
        dim: game.dim.id(),
        skip: HashSet::new(),
        found: Some(found),
        target: [0; 3],
        fell: None,
        phase: Phase::Seek,
        wait_tick: game.ticks,
    });
    let upto = if left < MOST { format!(" up to {left}") } else { String::new() };
    Ok(format!("Mining{upto} {word}: {seen} in sight within {RADIUS} blocks. .mine stop stops."))
}

// The job's turn in a frame: with nothing running, the next block.
pub(super) fn frame(game: &mut Game<'_>, paused: bool) {
    let Some(mut m) = mine().take() else { return };
    if paused || !matches!(m.phase, Phase::Seek) || walk::owns(game) || game.ticks < m.wait_tick
        || !game.player.as_ref().is_some_and(|p| !p.flying && p.ground.on_ground)
    {
        *mine() = Some(m);
        return;
    }
    match seek(game, &mut m) {
        Ok(true) => *mine() = Some(m),
        Ok(false) => crate::game::sunset::say(game, &done(&m)),
        Err(e) => crate::game::sunset::say(game, &format!("Stopped mining {} after {}: {e}", m.word, m.mined)),
    }
}

// One block a frame: the nearest found, dug from here if the crosshair reaches it, walked up to if not. Ok(false):
// nothing is left to try.
fn seek(game: &mut Game<'_>, m: &mut Mine) -> Result<bool, String> {
    if gather(game, m)? {
        return Ok(true);
    }
    if m.left == 0 {
        return Ok(false);
    }
    if m.found.is_none() {
        m.found = Some(look(game, &m.word, &m.skip));
    }
    let Some(pos) = m.found.as_mut().and_then(Vec::pop) else {
        return Ok(false);
    };
    if !is_wanted(game, &m.word, pos) {
        return Ok(true);
    }
    m.target = pos;
    if dig(game, pos)? {
        m.phase = Phase::Dig;
    } else if approach(game, pos, 2.5)? {
        m.phase = Phase::Walk { closer: true };
    } else {
        m.skip.insert(pos);
    }
    Ok(true)
}

// What fell from the block just mined and still lies there (a server's drop shows up a few ticks after the block
// goes, so this looks at the items themselves a moment later, not at the dig's own answer): a walk over to it, twice
// at most. Ok(true): walking.
fn gather(game: &mut Game<'_>, m: &mut Mine) -> Result<bool, String> {
    let Some((block, walks)) = m.fell else { return Ok(false) };
    let Some(p) = game.player.as_ref() else { return Ok(false) };
    let me = [p.position.x, p.position.y, p.position.z];
    let from = [f64::from(block[0]) + 0.5, f64::from(block[1]) + 0.5, f64::from(block[2]) + 0.5];
    let far = |a: [f64; 3], b: [f64; 3]| (a[0] - b[0]).hypot(a[2] - b[2]).hypot(a[1] - b[1]);
    let item = game.dropped.iter()
        .map(|i| [i.position.x, i.position.y, i.position.z])
        .filter(|at| far(*at, from) < 3.0)
        .min_by(|a, b| far(*a, me).total_cmp(&far(*b, me)));
    if let Some(at) = item
        && walks < 2
        && let Some(goto) = walk::plan(game, at, 0.6, false)
        && goto.path.nodes.len() > 1
    {
        walk::run(game, TaskKind::Goto(Box::new(goto)), 6.0)?;
        m.fell = Some((block, walks + 1));
        m.phase = Phase::Collect;
        return Ok(true);
    }
    m.fell = None;
    Ok(false)
}

// The answer of a task the job started. False: no job, the answer is .goto's own.
pub(super) fn answer(game: &mut Game<'_>, v: &serde_json::Value) -> bool {
    let Some(mut m) = mine().take() else { return false };
    if v["cancelled"].as_bool() == Some(true) {
        // Horizon ended the task itself (its agent's stop, the window closing): the job goes with it.
        crate::game::sunset::say(game, &format!("Stopped mining {} after {}.", m.word, m.mined));
        return true;
    }
    let next = match m.phase {
        Phase::Seek => Ok(true),
        Phase::Walk { closer } => arrived(game, &mut m, closer),
        Phase::Dig => dug(game, &mut m, v),
        Phase::Collect => {
            m.phase = Phase::Seek;
            // A moment on the spot: what fell is picked up as you stand over it.
            m.wait_tick = game.ticks + 6;
            Ok(true)
        }
    };
    match next {
        Ok(true) => *mine() = Some(m),
        Ok(false) => crate::game::sunset::say(game, &done(&m)),
        Err(e) => crate::game::sunset::say(game, &format!("Stopped mining {} after {}: {e}", m.word, m.mined)),
    }
    true
}

// The walk up to the block has ended (there, or stuck on the way): dig it, walk right up to it, or give it up.
fn arrived(game: &mut Game<'_>, m: &mut Mine, closer: bool) -> Result<bool, String> {
    m.phase = Phase::Seek;
    if !is_wanted(game, &m.word, m.target) {
        // Someone else had it.
        m.found = None;
    } else if dig(game, m.target)? {
        m.phase = Phase::Dig;
    } else if closer && game.player.as_ref().is_some_and(|p| !p.flying && p.ground.on_ground) && approach(game, m.target, 1.2)? {
        m.phase = Phase::Walk { closer: false };
    } else {
        m.skip.insert(m.target);
    }
    Ok(true)
}

fn dug(game: &mut Game<'_>, m: &mut Mine, v: &serde_json::Value) -> Result<bool, String> {
    m.phase = Phase::Seek;
    if v.is_null() {
        m.skip.insert(m.target);
        return Ok(true);
    }
    if let Some(error) = v["error"].as_str() {
        if error.starts_with("timed out") {
            let block = v["block"].as_str().unwrap_or("the block").to_string();
            return Err(format!("{block} did not break in {DIG_SECONDS:.0} s with what you hold."));
        }
        // The crosshair lost it (pushed, something in the way): not this one.
        m.skip.insert(m.target);
        return Ok(true);
    }
    m.mined += 1;
    m.left = m.left.saturating_sub(1);
    m.found = None;
    // What fell is looked for in a moment (gather), once a server has had time to show it.
    m.fell = Some((m.target, 0));
    m.wait_tick = game.ticks + 8;
    Ok(true)
}
