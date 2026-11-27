// .goto x y z (from Rust's PR #24): walks there on Horizon's own A* path and walking task, the one horizon-ctl's goto
// runs, so it lives next to them in agent.rs. Movement keys, a screen, the menu, Panic or turning Sunset off stop it.
// .follow <player> [blocks] aims the same walk at a player, and again each time they move on (Follow, below).
// .mine <block> [count] (mine.rs, beside this file in agent.rs) walks to blocks and breaks them on the same lease.
use crate::game::sunset_compat::SunsetGame as _;
use super::{Game, Goto, MOVE_KEYS, Progress, TaskKind};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

pub(super) const TASK: &str = "sunset.goto";
const RANGE: f64 = 128.0;

// The running walk's answer channel and the dimension it started in.
static WALK: Mutex<Option<(Receiver<String>, &'static str)>> = Mutex::new(None);

// .follow: who, how near to stay, and how the walks toward them are going.
struct Follow {
    name: String,
    keep: f64,
    dim: &'static str,
    // Where the walk now running is headed: the player's place when it was planned.
    goal: [f64; 3],
    // No new path before this tick: half a second between plans, longer after each plan that found no way.
    plan_tick: u64,
    misses: u32,
    // The last tick the player was in sight, and whether their going out of sight has been said.
    seen_tick: u64,
    lost: bool,
}
const KEEP: f64 = 3.0;
// Half a minute out of sight ends the follow.
const LOST_TICKS: u64 = 600;
static FOLLOW: Mutex<Option<Follow>> = Mutex::new(None);

fn follow() -> std::sync::MutexGuard<'static, Option<Follow>> {
    FOLLOW.lock().unwrap_or_else(|e| e.into_inner())
}

// What a stopped follow says: the walk's own reason ("stopped: you moved") with the player's name in front.
fn unfollowed(name: &str, why: &str) -> String {
    let why = why.strip_prefix("stopped: ").or_else(|| why.strip_prefix("stopped ")).unwrap_or(why);
    format!("Stopped following {name}: {why}.")
}

pub(crate) fn owns(game: &Game<'_>) -> bool {
    game.agent.task.as_ref().is_some_and(|t| t.name == TASK)
}

pub(crate) fn foreign_busy(game: &Game<'_>) -> bool {
    game.agent.glide.is_some() || game.agent.task.as_ref().is_some_and(|t| t.name != TASK)
}

// A foreign task may have pressed its mapped input before the next frame can retire Baritone's lease.
pub(crate) fn foreign_input(game: &Game<'_>) -> (Vec<winit::keyboard::KeyCode>, [bool; 2]) {
    let Some(task) = game.agent.task.as_ref().filter(|t| t.name != TASK) else { return (Vec::new(), [false; 2]); };
    match &task.kind {
        TaskKind::Move { keys, .. } => (keys.clone(), [false; 2]),
        TaskKind::Jump { .. } => (vec![winit::keyboard::KeyCode::Space], [false; 2]),
        TaskKind::Goto(goto) => (if goto.sprint { vec![winit::keyboard::KeyCode::ControlLeft] } else { Vec::new() }, [false; 2]),
        TaskKind::Break(_) => (Vec::new(), [game.left, false]),
        TaskKind::Click { sneak, .. } => (sneak.as_ref().map_or_else(Vec::new, |_| vec![winit::keyboard::KeyCode::ShiftLeft]), [false, game.right]),
        _ => (Vec::new(), [false; 2]),
    }
}

// What a walk, a follow or a .mine has to say once its command has long been answered: a line in the chat under it,
// as the command's own reply was. Not a toast: Notifications is off unless you turn it on, and "Arrived." or why a
// job stopped is not news to do without.
pub(super) fn report(game: &mut Game<'_>, text: &str) {
    crate::game::sunset::chat_line(game, text);
}

// Whatever stops the walk ends a follow or a .mine too, walking at that moment or not.
pub(crate) fn cancel(game: &mut Game<'_>, why: &str) -> bool {
    let followed = follow().take();
    let Some(why) = followed.map(|f| unfollowed(&f.name, why)).or_else(|| super::sunset_mine::stopped(game, why)) else {
        return stop_walk(game, why);
    };
    if !stop_walk(game, &why) {
        report(game, &why);
    }
    true
}

// Ends the task of ours now running (a walk, or .mine's dig): its answer carries `why` (frame says it), its keys,
// its held attack button and the lease are given back.
fn stop_walk(game: &mut Game<'_>, why: &str) -> bool {
    if !owns(game) {
        return false;
    }
    if let Some(task) = game.agent.task.take() {
        if matches!(task.kind, TaskKind::Break(_)) {
            let _ = game.agent_mouse(winit::event::MouseButton::Left, false);
        }
        let _ = task.reply.send(serde_json::json!({ "error": why }).to_string());
    }
    // The walk holds movement keys only: the mouse buttons stay as you hold them.
    for key in MOVE_KEYS {
        game.keys.release(&key, horizon_client::held::Source::Agent);
    }
    release_lease(game);
    true
}

fn release_lease(game: &mut Game<'_>) {
    if let Some(lease) = crate::game::sunset_movement_owner::current()
        .filter(|l| l.owner == crate::game::sunset_movement_owner::Owner::Goto)
    { crate::game::sunset_movement_owner::release(game, lease); }
}

// Drops the walk without a word: it is ended if still going, and an answer it already gave is not read.
pub(super) fn drop_walk(game: &mut Game<'_>) {
    stop_walk(game, "navigation stopped");
    *WALK.lock().unwrap_or_else(|e| e.into_inner()) = None;
    release_lease(game);
}

fn parse(args: &[&str]) -> Result<Option<[i32; 3]>, &'static str> {
    if let [word] = args
        && word.eq_ignore_ascii_case("stop")
    {
        return Ok(None);
    }
    let usage = "Use .goto <x> <y> <z> (whole block coordinates) or .goto stop.";
    let [x, y, z] = args else {
        return Err(usage);
    };
    let at = [x, y, z].map(|w| w.parse::<i32>());
    let [Ok(x), Ok(y), Ok(z)] = at else {
        return Err(usage);
    };
    if x.unsigned_abs() > 29_999_983 || z.unsigned_abs() > 29_999_983 || !(-64..=319).contains(&y) {
        return Err("That is outside the world.");
    }
    Ok(Some([x, y, z]))
}

pub(crate) fn command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    let Some(at) = parse(args)? else {
        if let Some(name) = unfollow(game) {
            return Ok(format!("Stopped following {name}."));
        }
        if let Some(text) = super::sunset_mine::stop(game) {
            return Ok(text);
        }
        let stopped = cancel(game, "navigation stopped");
        return Ok(if stopped { "Stopped walking." } else { "Not walking anywhere." }.into());
    };
    game.require_playing_screen().map_err(|e| e.to_string())?;
    if game.screen_over_playing() || game.sleeping.is_some() { return Err("Close the prompt and wake up before walking.".into()); }
    crate::game::sunset_movement_owner::can_acquire(game)?;
    let p = game.player.as_ref().ok_or("No player.")?;
    if p.flying || !p.ground.on_ground {
        return Err("Stand on the ground first.".into());
    }
    let target = [f64::from(at[0]) + 0.5, f64::from(at[1]), f64::from(at[2]) + 0.5];
    let far = (target[0] - p.position.x).hypot(target[2] - p.position.z).hypot(target[1] - p.position.y);
    if far > RANGE {
        return Err(format!("That is {far:.0} blocks away; .goto walks up to {RANGE:.0}."));
    }
    let goto = plan(game, target, 0.6, false).ok_or("No walking path there through loaded terrain.")?;
    let nodes = goto.path.nodes.len();
    let (tx, rx) = channel();
    cancel(game, "stopped: a new walking goal started");
    let lease = crate::game::sunset_movement_owner::acquire(game, crate::game::sunset_movement_owner::Owner::Goto)?;
    if let Err(e) = game.start_task(TASK, TaskKind::Goto(Box::new(goto)), &tx, 60.0) {
        crate::game::sunset_movement_owner::release(game, lease);
        return Err(e.to_string());
    }
    *WALK.lock().unwrap_or_else(|e| e.into_inner()) = Some((rx, game.dim.id()));
    Ok(format!("Walking to {} {} {} ({nodes} steps). .goto stop stops.", at[0], at[1], at[2]))
}

// Starts a task of Sunset's own on the movement lease (taken first unless a walk of ours holds it); its answer
// comes back through frame.
pub(super) fn run(game: &mut Game<'_>, kind: TaskKind, timeout: f64) -> Result<(), String> {
    use crate::game::sunset_movement_owner as owner;
    let lease = match owner::current().filter(|l| l.owner == owner::Owner::Goto) {
        Some(lease) => lease,
        None => owner::acquire(game, owner::Owner::Goto)?,
    };
    let (tx, rx) = channel();
    if let Err(e) = game.start_task(TASK, kind, &tx, timeout) {
        owner::release(game, lease);
        return Err(e.to_string());
    }
    *WALK.lock().unwrap_or_else(|e| e.into_inner()) = Some((rx, game.dim.id()));
    Ok(())
}

// A walk from where you stand to within `near` of `target`, if loaded terrain has a way.
pub(super) fn plan(game: &Game<'_>, target: [f64; 3], near: f64, sprint: bool) -> Option<Goto> {
    let p = game.player.as_ref()?;
    let from = [p.position.x.floor() as i32, p.position.y.round() as i32, p.position.z.floor() as i32];
    let path = game.find_path(from, target, near, &[])?;
    let position = [p.position.x, p.position.y, p.position.z];
    let mut next = 1;
    let aim = path.aim(position, &mut next, target).unwrap_or(target);
    Some(Goto {
        target,
        near,
        sprint,
        path,
        blocked: Vec::new(),
        progress: Progress::new(position, aim),
        next,
        progress_tick: game.ticks,
        replans: 0,
        jump_until: 0,
        warning: None,
    })
}

// Where a player in sight stands, by the name the server lists them under (any case), and that name.
fn player_at(game: &Game<'_>, name: &str) -> Option<(String, [f64; 3])> {
    let (id, listed) = game.remote_player_names().into_iter().find(|(_, n)| n.eq_ignore_ascii_case(name))?;
    let m = game.mobs.iter().find(|m| m.id == id && m.alive() && !m.removed)?;
    Some((listed, [m.position.x, m.position.y, m.position.z]))
}

fn apart(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2]).hypot(a[1] - b[1])
}

// Near enough to stand: what the walk itself counts as arrived (navigation::arrived), with a little slack so a
// player who shifts on the spot does not start a walk.
fn beside(me: [f64; 3], at: [f64; 3], keep: f64) -> bool {
    (me[0] - at[0]).hypot(me[2] - at[2]) <= keep + 0.75 && (me[1] - at[1]).abs() <= keep.max(1.0) + 1.0
}

// Ends a follow without a word from the walk it had going (the command answers for it).
fn unfollow(game: &mut Game<'_>) -> Option<String> {
    let f = follow().take()?;
    drop_walk(game);
    Some(f.name)
}

// .follow <player> [blocks] | stop: walk after a player in sight and stand `blocks` from them (3 unless said),
// walking on each time they move away. Stopped by what stops .goto; a screen only holds it until it shuts.
pub(crate) fn follow_command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    let usage = "Use .follow <player> [blocks to stay away, 1 to 16] or .follow stop.";
    let (name, keep) = match args {
        [] => {
            return Ok(follow().as_ref().map_or_else(|| usage.to_string(), |f| {
                format!("Following {}, staying {} blocks away. .follow stop stops.", f.name, f.keep)
            }));
        }
        [word] if word.eq_ignore_ascii_case("stop") => {
            return Ok(unfollow(game).map_or_else(|| "Not following anyone.".into(), |n| format!("Stopped following {n}.")));
        }
        [name] => (*name, KEEP),
        [name, blocks] => {
            let keep = blocks.parse::<f64>().ok().filter(|k| (1.0..=16.0).contains(k))
                .ok_or_else(|| format!("{blocks} is not a distance from 1 to 16 blocks."))?;
            (*name, keep)
        }
        _ => return Err(usage.into()),
    };
    game.require_playing_screen().map_err(|e| e.to_string())?;
    if game.screen_over_playing() || game.sleeping.is_some() { return Err("Close the prompt and wake up before walking.".into()); }
    let Some((listed, at)) = player_at(game, name) else {
        let known = crate::game::remote::sunset_remote::tab_names(game).iter().any(|n| n.eq_ignore_ascii_case(name));
        return Err(if known { format!("{name} is not in sight.") } else { format!("No player called {name} here.") });
    };
    crate::game::sunset_movement_owner::can_acquire(game)?;
    // Starting over: a walk or a follow already going ends first, with its own line.
    cancel(game, "stopped: a new walking goal started");
    *follow() = Some(Follow {
        name: listed.clone(),
        keep,
        dim: game.dim.id(),
        goal: at,
        plan_tick: game.ticks,
        misses: 0,
        seen_tick: game.ticks,
        lost: false,
    });
    Ok(format!("Following {listed}, staying {keep} blocks away. .follow stop stops."))
}

// A frame of a follow: plan a walk toward the player when they have moved on, wait while they are near.
fn follow_frame(game: &mut Game<'_>, paused: bool) {
    let Some(mut f) = follow().take() else { return };
    let now = game.ticks;
    let me = game.player.as_ref().map(|p| ([p.position.x, p.position.y, p.position.z], !p.flying && p.ground.on_ground));
    let seen = player_at(game, &f.name).map(|(_, at)| at).zip(me).filter(|(at, (me, _))| apart(*at, *me) <= RANGE);
    let Some((at, (me, grounded))) = seen else {
        // Out of sight: the walk going ends where they were last seen. Half a minute of it ends the follow.
        if now.saturating_sub(f.seen_tick) > LOST_TICKS {
            let why = format!("Stopped following {}: out of sight for half a minute.", f.name);
            if !stop_walk(game, &why) {
                report(game, &why);
            }
            return;
        }
        if !f.lost && now.saturating_sub(f.seen_tick) > 40 {
            f.lost = true;
            report(game, &format!("{} is out of sight; waiting half a minute for them.", f.name));
        }
        *follow() = Some(f);
        return;
    };
    f.seen_tick = now;
    f.lost = false;
    let walking = owns(game);
    // Standing near them, or walking to where they still are: nothing to plan. Nor in the air, nor behind a screen.
    let settled = if walking { apart(f.goal, at) < 1.5 } else { beside(me, at, f.keep) };
    if paused || settled || now < f.plan_tick || !grounded {
        *follow() = Some(f);
        return;
    }
    // Half a second between paths, more for a long way (a path across a hundred blocks is no small search).
    let far = apart(me, at);
    f.plan_tick = now + 10 + (far / 4.0) as u64;
    // Run after someone well ahead, walk beside someone near.
    let sprint = far > f.keep + 4.0;
    let Some(goto) = plan(game, at, f.keep, sprint) else {
        // No way to them (in the air, across a gap): try again, less and less often, up to every 10 s.
        f.misses = (f.misses + 1).min(5);
        f.plan_tick = now + 20 * (1u64 << (f.misses - 1)).min(10);
        if f.misses == 3 {
            report(game, &format!("No walking path to {} right now; still trying.", f.name));
        }
        *follow() = Some(f);
        return;
    };
    f.misses = 0;
    let (tx, rx) = channel();
    if walking {
        // The walk going gives way to the new one; its keys and lease carry over, and its answer is not news.
        if let Some(task) = game.agent.task.take() {
            let _ = task.reply.send(serde_json::json!({ "error": "replanned" }).to_string());
        }
        if !sprint {
            game.keys.release(&winit::keyboard::KeyCode::ControlLeft, horizon_client::held::Source::Agent);
        }
    } else if crate::game::sunset_movement_owner::acquire(game, crate::game::sunset_movement_owner::Owner::Goto).is_err() {
        // Something else has the movement keys for now (a held button, a script): look again in a second.
        f.plan_tick = now + 20;
        *follow() = Some(f);
        return;
    }
    if game.start_task(TASK, TaskKind::Goto(Box::new(goto)), &tx, 60.0).is_err() {
        drop_walk(game);
        f.plan_tick = now + 20;
        *follow() = Some(f);
        return;
    }
    *WALK.lock().unwrap_or_else(|e| e.into_inner()) = Some((rx, f.dim));
    f.goal = at;
    *follow() = Some(f);
}

// Each frame, before Horizon steps the task: stop it when it should, and say how it ended.
pub(crate) fn frame(game: &mut Game<'_>) {
    if !game.sunset_window() { return; }
    let walk_dim = WALK.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(_, dim)| *dim);
    let follow_dim = follow().as_ref().map(|f| f.dim);
    let Some(dim) = walk_dim.or(follow_dim).or_else(super::sunset_mine::dim) else {
        return;
    };
    let screen = crate::game::sunset::gui_open() || game.require_playing_screen().is_err()
        || game.screen_over_playing() || game.sleeping.is_some();
    if dim != game.dim.id() || !game.native_on("sunset") {
        cancel(game, "stopped: you changed dimension or Sunset was turned off");
    } else if screen && (follow_dim.is_some() || super::sunset_mine::hold(game)) {
        // A follow or a .mine outlasts a screen (the chat, the inventory, the menu): it stands while one is up.
        drop_walk(game);
    } else if screen {
        cancel(game, "stopped: a screen opened or Sunset was turned off");
    }
    walk_answer(game);
    follow_frame(game, screen);
    super::sunset_mine::frame(game, screen);
}

// The walk's answer, once it has one: the lease goes back, and a walk of .goto's says how it ended.
fn walk_answer(game: &mut Game<'_>) {
    let mut walk = WALK.lock().unwrap_or_else(|e| e.into_inner());
    let Some((rx, _)) = walk.as_ref() else {
        return;
    };
    let reply = match rx.try_recv() {
        Ok(reply) => Some(reply),
        Err(TryRecvError::Disconnected) => None,
        Err(TryRecvError::Empty) => return,
    };
    *walk = None;
    drop(walk);
    release_lease(game);
    // No answer (the task was dropped unanswered): Null, which a follow or a .mine reads as a step that failed.
    let v = reply.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).unwrap_or(serde_json::Value::Null);
    let arrived = v["arrived"].as_bool() == Some(true);
    if let Some(f) = follow().as_mut() {
        // A follow's walks end without a word: it walks on when the player does. One that got stuck waits a second.
        if !arrived {
            f.plan_tick = game.ticks + 20;
        }
        return;
    }
    if super::sunset_mine::answer(game, &v) || v.is_null() {
        return;
    }
    let text = if arrived { "Arrived." } else { v["error"].as_str().unwrap_or("Stopped walking.") };
    report(game, text);
}

// A movement key you press stops the walk; the game then takes the key as usual, so your movement wins at once.
pub(crate) fn key(game: &mut Game<'_>, key: &str) {
    // Keys typed into a screen (the chat) are not movement: a follow or a .mine waits behind the screen.
    if (follow().is_some() || super::sunset_mine::dim().is_some()) && (game.require_playing_screen().is_err() || game.screen_over_playing()) {
        return;
    }
    let moves = ["key.forward", "key.back", "key.left", "key.right", "key.jump", "key.sneak", "key.sprint"];
    if moves.iter().any(|m| game.settings.keys.get(m) == key) {
        cancel(game, "stopped: you moved");
    }
}
