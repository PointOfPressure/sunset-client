// Reuse Horizon's own bounded A* and walking task, without a control socket.
use super::{Game, Goto, MOVE_KEYS, TaskKind};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError, channel},
};

const TASK: &str = "sunset.goto";
struct Pending {
    rx: Receiver<String>,
    dimension: &'static str,
}
static REPLY: Mutex<Option<Pending>> = Mutex::new(None);
static PENDING: AtomicBool = AtomicBool::new(false);

fn owns(game: &Game<'_>) -> bool {
    game.agent.task.as_ref().is_some_and(|t| t.name == TASK)
}

pub(in crate::game) fn cancel(game: &mut Game<'_>, why: &str) -> bool {
    if !owns(game) {
        return false;
    }
    if let Some(task) = game.agent.task.take() {
        let _ = task.reply.send(serde_json::json!({"error": why}).to_string());
    }
    // The task only owns movement. Leave the player's mouse buttons alone.
    for key in MOVE_KEYS {
        game.keys.remove(&key);
    }
    true
}

pub(in crate::game) fn command(game: &mut Game<'_>, args: &[&str]) -> Result<String, String> {
    let Some(at) = super::sunset_goto_args::parse(args)? else {
        return Ok(if cancel(game, "navigation stopped") {
            "Navigation stopped."
        } else {
            "No Sunset navigation is running."
        }
        .into());
    };
    game.playing().map_err(|e| e.to_string())?;
    if game.agent.task.is_some() || game.agent.glide.is_some() {
        return Err("Another action is running; stop it before starting .goto.".into());
    }
    if game.keys.iter().any(|k| MOVE_KEYS.contains(k)) {
        return Err("Release movement keys before starting .goto.".into());
    }
    let p = game.player.as_ref().ok_or("No player.")?;
    if p.flying || !p.ground.on_ground {
        return Err("Start .goto while standing on the ground.".into());
    }
    let from = [
        p.position.x.floor() as i32,
        p.position.y.round() as i32,
        p.position.z.floor() as i32,
    ];
    let target = [f64::from(at[0]) + 0.5, f64::from(at[1]), f64::from(at[2]) + 0.5];
    let distance = (target[0] - p.position.x).powi(2) + (target[1] - p.position.y).powi(2) + (target[2] - p.position.z).powi(2);
    if !distance.is_finite() || distance > 128.0_f64.powi(2) {
        return Err("Choose a destination within 128 blocks in loaded terrain.".into());
    }
    let path = game
        .find_path(from, target, 0.6)
        .ok_or("No walking path through loaded terrain. Choose a reachable destination.")?;
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
    game.start_task(TASK, TaskKind::Goto(Box::new(goto)), &tx, 60.0)
        .map_err(|e| e.to_string())?;
    *REPLY.lock().unwrap_or_else(|e| e.into_inner()) = Some(Pending {
        rx,
        dimension: game.dim.id(),
    });
    PENDING.store(true, Ordering::Relaxed);
    Ok(format!(
        "Walking to {} {} {} ({nodes} path nodes). .goto stop cancels.",
        at[0], at[1], at[2]
    ))
}

// Called before Horizon advances its task, even when simulation is paused.
pub(in crate::game) fn frame(game: &mut Game<'_>) {
    if !game.native_window() || !PENDING.load(Ordering::Relaxed) {
        return;
    }
    let changed_dimension = REPLY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|p| p.dimension != game.dim.id());
    if changed_dimension
        || !game.native_on("sunset")
        || crate::game::sunset::gui_open()
        || game.ui.chat_screen.is_some()
        || game.playing().is_err()
    {
        cancel(game, "navigation stopped when a screen opened or Sunset was disabled");
    }
    let reply = {
        let mut pending = REPLY.lock().unwrap_or_else(|e| e.into_inner());
        match pending.as_ref().map(|p| p.rx.try_recv()) {
            Some(Ok(reply)) => {
                *pending = None;
                PENDING.store(false, Ordering::Relaxed);
                Some(reply)
            }
            None | Some(Err(TryRecvError::Disconnected)) => {
                *pending = None;
                PENDING.store(false, Ordering::Relaxed);
                None
            }
            Some(Err(TryRecvError::Empty)) => None,
        }
    };
    if let Some(reply) = reply.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) {
        let text = if reply["arrived"].as_bool() == Some(true) {
            "Arrived."
        } else {
            reply["error"].as_str().unwrap_or("Navigation stopped.")
        };
        crate::game::sunset::say(game, text);
    }
}

// Keyboard, controller and control-socket presses meet at key_pressed, after
// mapping and toggle handling. Preserve the pressed mapping's current state
// when releasing the route's movement, so manual movement wins immediately.
pub(in crate::game) fn key(game: &mut Game<'_>, key: winit::keyboard::KeyCode) {
    if game.native_window() && MOVE_KEYS.contains(&key) {
        let held = game.keys.contains(&key);
        if cancel(game, "navigation stopped by movement input") && held {
            game.keys.insert(key);
        }
    }
}
