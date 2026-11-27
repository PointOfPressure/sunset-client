// .highway <width> [height] [length] [rails] [nofloor] | stop: a straight tunnel the way you face (north, east, south
// or west), dug a slice at a time: every block in the slice ahead is broken the way .mine breaks one (the crosshair on
// it, the attack button held, the game's own dig), holes in its floor are filled with whole blocks from the hotbar
// (obsidian first), and you walk on a block on .goto's walker. Lava or water in the way ends it: nothing here seals a
// spring. It rides .goto's task, lease and stops (goto.rs) as .mine does; an open screen only holds it.
// The idea is Meteor's Highway Builder; this is written fresh on Horizon's agent, none of its code.
use super::sunset_goto as walk;
use super::{Break, Game, TaskKind};
use crate::game::sunset_util as util;
use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

// A block still whole after this long is the wrong tool's (or bedrock).
const DIG_SECONDS: f64 = 20.0;
// How long a block put down may take to show, and how often one place is tried.
const PLACE_TICKS: u64 = 20;
const PLACE_TRIES: u8 = 3;
// Walks back onto the lane, or on after a walk that stuck, before the job gives up.
const WALK_TRIES: u8 = 3;
pub(crate) const MOST: u32 = 10_000;

enum Phase {
    // Nothing running: the next block to break or put down, or the step on.
    Seek,
    Dig,
    // on: the walk is the step into the next slice (not one back onto the lane).
    Walk { on: bool },
}

struct Highway {
    // Forward, as (x, z); right of it is (-z, x).
    dir: [i32; 2],
    // The block your feet were in at the start, and the steps walked from it.
    start: [i32; 3],
    step: i32,
    width: i32,
    height: i32,
    length: Option<i32>,
    floor: bool,
    rails: bool,
    dim: &'static str,
    dug: u32,
    placed: u32,
    // Blocks left standing in slices walked past: no line to them from the lane.
    left: u32,
    // Blocks the crosshair could not find from here; looked at again after each block that goes.
    skip: HashSet<[i32; 3]>,
    target: [i32; 3],
    // The block being put down: where, the tick to give it until, the tries so far.
    placing: Option<([i32; 3], u64, u8)>,
    stuck: u8,
    phase: Phase,
    wait_tick: u64,
}
static HIGHWAY: Mutex<Option<Highway>> = Mutex::new(None);

fn job() -> std::sync::MutexGuard<'static, Option<Highway>> {
    HIGHWAY.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn dim() -> Option<&'static str> {
    job().as_ref().map(|m| m.dim)
}

impl Highway {
    // The lanes dug, counted to the right of the one you walk in: 4 wide is -1 to 2.
    fn lanes(&self) -> (i32, i32) {
        (-((self.width - 1) / 2), self.width / 2)
    }

    fn cell(&self, slice: i32, lane: i32, up: i32) -> [i32; 3] {
        let [x, z] = self.dir;
        [self.start[0] + x * slice - z * lane, self.start[1] + up, self.start[2] + z * slice + x * lane]
    }

    // The cells of a slice that are to be open: the tunnel, and over the rails.
    fn open_cells(&self, slice: i32) -> Vec<[i32; 3]> {
        let (lo, hi) = self.lanes();
        let mut cells = Vec::new();
        for lane in lo..=hi {
            cells.extend((0..self.height).map(|up| self.cell(slice, lane, up)));
        }
        if self.rails {
            for lane in [lo - 1, hi + 1] {
                cells.extend((1..self.height).map(|up| self.cell(slice, lane, up)));
            }
        }
        cells
    }

    // The cells of a slice that are to be whole blocks: the floor from your lane outwards (each then has a neighbour
    // to be put against), under the rails too, then the rails.
    fn whole_cells(&self, slice: i32) -> Vec<[i32; 3]> {
        let (lo, hi) = self.lanes();
        let mut cells = Vec::new();
        if self.floor {
            let mut lanes: Vec<i32> = if self.rails { (lo - 1..=hi + 1).collect() } else { (lo..=hi).collect() };
            lanes.sort_by_key(|lane| lane.abs());
            cells.extend(lanes.into_iter().map(|lane| self.cell(slice, lane, -1)));
        }
        if self.rails {
            cells.extend([lo - 1, hi + 1].map(|lane| self.cell(slice, lane, 0)));
        }
        cells
    }

    fn way(&self) -> &'static str {
        match self.dir {
            [0, 1] => "south",
            [-1, 0] => "west",
            [0, -1] => "north",
            _ => "east",
        }
    }

    fn tally(&self) -> String {
        let blocks = if self.step == 1 { "block" } else { "blocks" };
        format!("{} {blocks} {} (dug {}, placed {})", self.step, self.way(), self.dug, self.placed)
    }

    fn stopped(&self, why: &str) -> String {
        let why = why.trim_end_matches('.');
        if why.is_empty() {
            format!("Stopped the highway after {}.", self.tally())
        } else {
            format!("Stopped the highway after {}: {why}.", self.tally())
        }
    }

    fn done(&self) -> String {
        let more = match self.left {
            0 => String::new(),
            1 => "; 1 block in it could not be reached".into(),
            n => format!("; {n} blocks in it could not be reached"),
        };
        format!("Highway done: {}{more}.", self.tally())
    }
}

fn at(pos: [i32; 3]) -> String {
    format!("{} {} {}", pos[0], pos[1], pos[2])
}

// Something to break: a block the crosshair can take. Fire and the like have no outline and are walked through.
fn in_the_way(game: &Game<'_>, pos: [i32; 3]) -> bool {
    !util::is_air(game, pos) && !util::facts(game, pos).outline.is_empty()
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

// The hotbar slot to build with: obsidian if there is any, or the first whole block that stays where it is put
// (no sand or gravel) and is no container.
fn block_slot(game: &Game<'_>) -> Option<usize> {
    let fit = |slot: usize| -> Option<bool> {
        let stack = game.inventory.get(slot)?.as_ref()?;
        let facts = game.res.facts().get(game.res.items().get(&stack.item)?.block?);
        let id = util::short(&stack.item);
        let falls = id.ends_with("sand") || id.ends_with("gravel") || id.ends_with("_powder") || id.contains("anvil");
        (facts.full_collision && !facts.block_entity && !falls).then(|| id == "obsidian")
    };
    (0..9).find(|&slot| fit(slot) == Some(true)).or_else(|| (0..9).find(|&slot| fit(slot).is_some()))
}

// A dig cut short (a stop, a screen) after its block went but before its answer came: the block still counts.
fn settle(game: &Game<'_>, m: &mut Highway) {
    if matches!(m.phase, Phase::Dig) && !in_the_way(game, m.target) {
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
    m.skip.clear();
    true
}

// .highway stop (and .goto stop): ends the job without a word from the task it had going.
pub(super) fn stop(game: &mut Game<'_>) -> Option<String> {
    let mut m = job().take()?;
    settle(game, &mut m);
    walk::drop_walk(game);
    Some(m.stopped(""))
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

// A walk into the middle of a cell of your lane. Ok(false): no way there.
fn walk_to(game: &mut Game<'_>, cell: [i32; 3]) -> Result<bool, String> {
    let middle = [f64::from(cell[0]) + 0.5, f64::from(cell[1]), f64::from(cell[2]) + 0.5];
    let Some(goto) = walk::plan(game, middle, 0.2, false).filter(|g| g.path.complete && g.path.nodes.len() > 1) else {
        return Ok(false);
    };
    walk::run(game, TaskKind::Goto(Box::new(goto)), 10.0)?;
    Ok(true)
}

// What the words after .highway ask for: width, height, length (none: until stopped), rails, floor.
pub(crate) fn parse(args: &[&str]) -> Result<(i32, i32, Option<i32>, bool, bool), String> {
    let usage = "Use .highway <width 1-5> [height 2-5] [length] [rails] [nofloor] or .highway stop.";
    let mut sizes: Vec<i32> = Vec::new();
    let (mut rails, mut floor) = (false, true);
    for word in args {
        match word.to_ascii_lowercase().as_str() {
            "rails" if !sizes.is_empty() => rails = true,
            "nofloor" if !sizes.is_empty() => floor = false,
            number => {
                let n = number.parse::<u32>().map_err(|_| usage.to_string())?;
                let (what, lo, hi) = *[("width", 1, 5), ("height", 2, 5), ("length", 1, MOST)].get(sizes.len()).ok_or(usage)?;
                if !(lo..=hi).contains(&n) {
                    return Err(format!("{n} is not a {what} from {lo} to {hi}."));
                }
                sizes.push(n as i32);
            }
        }
    }
    let width = *sizes.first().ok_or(usage)?;
    Ok((width, sizes.get(1).copied().unwrap_or(3), sizes.get(2).copied(), rails, floor))
}

pub(crate) fn command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    match args {
        [] => {
            return Ok(job().as_ref().map_or_else(
                || "Use .highway <width 1-5> [height 2-5] [length] [rails] [nofloor] or .highway stop.".to_string(),
                |m| format!("Building a highway: {} so far. .highway stop stops.", m.tally()),
            ));
        }
        [word] if word.eq_ignore_ascii_case("stop") => {
            return Ok(stop(game).unwrap_or_else(|| "No highway is being built.".into()));
        }
        _ => {}
    }
    let (width, height, length, rails, floor) = parse(args)?;
    game.require_playing_screen().map_err(|e| e.to_string())?;
    if game.screen_over_playing() || game.sleeping.is_some() {
        return Err("Close the prompt and wake up before walking.".into());
    }
    let p = game.player.as_ref().ok_or("No player.")?;
    if p.flying || !p.ground.on_ground {
        return Err("Stand on the ground first.".into());
    }
    let start = [p.position.x.floor() as i32, p.position.y.round() as i32, p.position.z.floor() as i32];
    crate::game::sunset_movement_owner::can_acquire(game)?;
    // The way you face, to the nearest of the four: yaw 0 is south, 90 west.
    let dir = [[0, 1], [-1, 0], [0, -1], [1, 0]][(game.yaw.rem_euclid(360.0) / 90.0).round() as usize % 4];
    walk::cancel(game, "stopped: a new walking goal started");
    let m = Highway {
        dir,
        start,
        step: 0,
        width,
        height,
        length,
        floor,
        rails,
        dim: game.dim.id(),
        dug: 0,
        placed: 0,
        left: 0,
        skip: HashSet::new(),
        target: [0; 3],
        placing: None,
        stuck: 0,
        phase: Phase::Seek,
        wait_tick: game.ticks,
    };
    let long = length.map_or_else(String::new, |n| format!(", {n} long"));
    let rails = if rails { ", with rails" } else { "" };
    let floor = if floor { "" } else { ", floor left as it is" };
    let text = format!("Building a highway {}: {width} wide, {height} high{long}{rails}{floor}. .highway stop stops.", m.way());
    *job() = Some(m);
    Ok(text)
}

// The job's turn in a frame: with nothing running, the next thing to do.
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
        Ok(false) => walk::report(game, &m.done()),
        Err(e) => walk::report(game, &m.stopped(&e)),
    }
}

// One thing a frame, in this order: back onto the lane, a block to break (nearest first), a block to put down, the
// step on. Ok(false): the length asked for is built.
fn seek(game: &mut Game<'_>, m: &mut Highway) -> Result<bool, String> {
    let p = game.player.as_ref().ok_or("no player")?;
    let feet = [p.position.x.floor() as i32, p.position.y.round() as i32, p.position.z.floor() as i32];
    let built = m.length.is_some_and(|n| m.step >= n);
    if feet == m.cell(m.step + 1, 0, 0) && !built {
        // A step on that a screen cut short after it was all but made. Not past the length asked for: a walk's
        // last step can carry you on into the cell past it (at a low frame rate the keys go late), and that is
        // walked back, not built.
        m.step += 1;
        m.skip.clear();
    } else if feet != m.cell(m.step, 0, 0) {
        if feet[1] != m.start[1] {
            return Err("you are no longer on its level".into());
        }
        m.stuck += 1;
        if m.stuck > WALK_TRIES || !walk_to(game, m.cell(m.step, 0, 0))? {
            return Err("you were moved off it and there is no way back".into());
        }
        m.phase = Phase::Walk { on: false };
        return Ok(true);
    }
    let end = built;
    let slices: Vec<i32> = if end { vec![m.step] } else { vec![m.step, m.step + 1] };
    let open: Vec<[i32; 3]> = slices.iter().flat_map(|&slice| m.open_cells(slice)).collect();
    if let Some(&pos) = open.iter().find(|&&pos| liquid(game, pos)) {
        return Err(format!("{} in the way at {}", util::short(&util::facts(game, pos).name), at(pos)));
    }
    let eye = game.eye_position().ok_or("no player")?;
    let far = |pos: &[i32; 3]| util::dist(eye, util::center_of(*pos));
    let mut todo: Vec<[i32; 3]> = open.into_iter().filter(|pos| in_the_way(game, *pos) && !m.skip.contains(pos)).collect();
    todo.sort_by(|a, b| far(a).total_cmp(&far(b)));
    for pos in todo {
        if dig(game, pos)? {
            m.target = pos;
            m.phase = Phase::Dig;
            return Ok(true);
        }
        m.skip.insert(pos);
    }
    if place(game, m, &slices)? {
        return Ok(true);
    }
    if end {
        m.left += m.open_cells(m.step).into_iter().filter(|&pos| in_the_way(game, pos)).count() as u32;
        return Ok(false);
    }
    // The two cells you walk into have to be open; what else is left standing is counted and left.
    if let Some(pos) = [m.cell(m.step + 1, 0, 0), m.cell(m.step + 1, 0, 1)].into_iter().find(|&pos| in_the_way(game, pos)) {
        return Err(format!("no line to the {} at {} from here", util::short(&util::facts(game, pos).name), at(pos)));
    }
    if !walk_to(game, m.cell(m.step + 1, 0, 0))? {
        return Err(if m.floor { "no way on to the next block" } else { "no way on to the next block (the floor is left as it is)" }.into());
    }
    m.left += m.open_cells(m.step).into_iter().filter(|&pos| in_the_way(game, pos)).count() as u32;
    m.phase = Phase::Walk { on: true };
    Ok(true)
}

// The next cell that is to be a whole block and is not: one place at a time, waited for until the server shows it.
// Ok(true): placing or waiting.
fn place(game: &mut Game<'_>, m: &mut Highway, slices: &[i32]) -> Result<bool, String> {
    let now = game.ticks;
    if let Some((pos, until, tries)) = m.placing {
        if whole(game, pos) {
            m.placed += 1;
            m.placing = None;
        } else if now < until {
            m.wait_tick = now + 1;
            return Ok(true);
        } else if tries >= PLACE_TRIES {
            return Err(format!("no block could be put at {} (something stands there, or the server refused it)", at(pos)));
        }
    }
    let wanted = slices.iter().flat_map(|&slice| m.whole_cells(slice)).find(|&pos| !whole(game, pos) && util::replaceable(game, pos));
    let Some(pos) = wanted else {
        m.placing = None;
        return Ok(false);
    };
    let Some(slot) = block_slot(game) else {
        return Err(format!("no whole block left in the hotbar for {}", at(pos)));
    };
    let tries = m.placing.filter(|(old, ..)| *old == pos).map_or(0, |(.., tries)| tries);
    let sent = util::place_block(game, pos, slot, true, true);
    m.placing = Some((pos, now + if sent { PLACE_TICKS } else { PLACE_TICKS / 2 }, tries + 1));
    m.wait_tick = now + 2;
    Ok(true)
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
        Phase::Seek => Ok(()),
        Phase::Walk { on } => walked(game, &mut m, on, v),
        Phase::Dig => dug(game, &mut m, v),
    };
    match next {
        Ok(()) => *job() = Some(m),
        Err(e) => walk::report(game, &m.stopped(&e)),
    }
    true
}

// A walk has ended: a step on counts once made; one that stuck (gravel fell in, a push) is looked at again.
fn walked(game: &Game<'_>, m: &mut Highway, on: bool, v: &serde_json::Value) -> Result<(), String> {
    if v["arrived"].as_bool() == Some(true) {
        if on {
            m.step += 1;
            m.stuck = 0;
        }
        m.skip.clear();
        return Ok(());
    }
    m.stuck += 1;
    if m.stuck > WALK_TRIES {
        return Err(v["error"].as_str().unwrap_or("the way on is blocked").to_string());
    }
    m.skip.clear();
    m.wait_tick = game.ticks + 10;
    Ok(())
}

fn dug(game: &Game<'_>, m: &mut Highway, v: &serde_json::Value) -> Result<(), String> {
    if v.is_null() {
        m.skip.insert(m.target);
        return Ok(());
    }
    if let Some(error) = v["error"].as_str() {
        if error.starts_with("timed out") {
            let block = v["block"].as_str().unwrap_or("a block").to_string();
            return Err(format!("{block} at {} did not break in {DIG_SECONDS:.0} s with what you hold", at(m.target)));
        }
        // The crosshair lost it (pushed, something fell in the way): not from here.
        m.skip.insert(m.target);
        return Ok(());
    }
    m.dug += 1;
    // What stood behind it can be seen now.
    m.skip.clear();
    m.wait_tick = game.ticks + 1;
    Ok(())
}
