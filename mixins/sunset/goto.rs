// .goto x y z (from Rust's PR #24): walks there on Horizon's own A* path and walking task, the one horizon-ctl's goto
// runs, so it lives next to them in agent.rs. Movement keys, a screen, the menu, Panic or turning Sunset off stop it.
use super::{Game, Goto, MOVE_KEYS, TaskKind};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

const TASK: &str = "sunset.goto";
const RANGE: f64 = 128.0;

// The running walk's answer channel and the dimension it started in.
static WALK: Mutex<Option<(Receiver<String>, &'static str)>> = Mutex::new(None);

fn owns(game: &Game<'_>) -> bool {
    game.agent.task.as_ref().is_some_and(|t| t.name == TASK)
}

pub(crate) fn cancel(game: &mut Game<'_>, why: &str) -> bool {
    if !owns(game) {
        return false;
    }
    if let Some(task) = game.agent.task.take() {
        let _ = task.reply.send(serde_json::json!({ "error": why }).to_string());
    }
    // The walk holds movement keys only: the mouse buttons stay as you hold them.
    for key in MOVE_KEYS {
        game.keys.remove(&key);
    }
    true
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
    if x.abs() > 29_999_983 || z.abs() > 29_999_983 || !(-64..=319).contains(&y) {
        return Err("That is outside the world.");
    }
    Ok(Some([x, y, z]))
}

pub(crate) fn command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    let Some(at) = parse(args)? else {
        let stopped = cancel(game, "navigation stopped");
        return Ok(if stopped { "Stopped walking." } else { "Not walking anywhere." }.into());
    };
    game.playing().map_err(|e| e.to_string())?;
    if game.agent.task.is_some() || game.agent.glide.is_some() {
        return Err("Something else is moving you; stop it first.".into());
    }
    if game.keys.iter().any(|k| MOVE_KEYS.contains(k)) {
        return Err("Let go of the movement keys first.".into());
    }
    let p = game.player.as_ref().ok_or("No player.")?;
    if p.flying || !p.ground.on_ground {
        return Err("Stand on the ground first.".into());
    }
    let from = [p.position.x.floor() as i32, p.position.y.round() as i32, p.position.z.floor() as i32];
    let target = [f64::from(at[0]) + 0.5, f64::from(at[1]), f64::from(at[2]) + 0.5];
    let far = (target[0] - p.position.x).hypot(target[2] - p.position.z).hypot(target[1] - p.position.y);
    if far > RANGE {
        return Err(format!("That is {far:.0} blocks away; .goto walks up to {RANGE:.0}."));
    }
    let path = game.find_path(from, target, 0.6).ok_or("No walking path there through loaded terrain.")?;
    let nodes = path.len();
    let goto = Goto {
        target,
        near: 0.6,
        sprint: false,
        path,
        next: 1,
        progress_tick: game.ticks,
        replans: 0,
        jump_until: 0,
        warning: None,
    };
    let (tx, rx) = channel();
    game.start_task(TASK, TaskKind::Goto(Box::new(goto)), &tx, 60.0).map_err(|e| e.to_string())?;
    *WALK.lock().unwrap_or_else(|e| e.into_inner()) = Some((rx, game.dim.id()));
    Ok(format!("Walking to {} {} {} ({nodes} steps). .goto stop stops.", at[0], at[1], at[2]))
}

// Each frame, before Horizon steps the task: stop it when it should, and say how it ended.
pub(crate) fn frame(game: &mut Game<'_>) {
    let mut walk = WALK.lock().unwrap_or_else(|e| e.into_inner());
    let Some((rx, dim)) = walk.as_ref() else {
        return;
    };
    if *dim != game.dim.id() || !game.native_on("sunset") || crate::game::sunset::gui_open() || game.playing().is_err() {
        cancel(game, "stopped: a screen opened or Sunset was turned off");
    }
    let reply = match rx.try_recv() {
        Ok(reply) => Some(reply),
        Err(TryRecvError::Disconnected) => None,
        Err(TryRecvError::Empty) => return,
    };
    *walk = None;
    drop(walk);
    let Some(v) = reply.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) else {
        return;
    };
    let text = if v["arrived"].as_bool() == Some(true) {
        "Arrived."
    } else {
        v["error"].as_str().unwrap_or("Stopped walking.")
    };
    crate::game::sunset::say(game, text);
}

// A movement key you press stops the walk; the game then takes the key as usual, so your movement wins at once.
pub(crate) fn key(game: &mut Game<'_>, key: &str) {
    let moves = ["key.forward", "key.back", "key.left", "key.right", "key.jump", "key.sneak", "key.sprint"];
    if moves.iter().any(|m| game.settings.keys.get(m) == key) {
        cancel(game, "stopped: you moved");
    }
}
