// The probe's packet log: every move packet, every end of a client tick and every accepted teleport (a server's
// correction, or a /tp) as they go on the wire (after Sunset has changed them), so verify.py can count the move
// packets in a tick and the times a server put the player back. Lines "send <kind>" in the SUNSET_PROBE file; a move
// packet's line ends with the height it says (or "-") and its on-ground flag (0 or 1), so the falls the server counts
// can be added up. Packets are known by their ids, read off packets of each kind made here. And "order <kind>" lines
// for the client's own hits (attack, a spear's stab), move packets (move) and tick ends, in the order they go, for the
// check that a hit goes before its tick's move packet as vanilla sends it: those skip the single player server's
// packets (its threads are named horizon-lan-... and horizon-local-...), so they hold in single player too. And a
// "pkt <id>" line for every packet the client sends (the same threads skipped), to count all of them in a tick. And an
// "input <flags>" line for each PlayerInput (the move keys the server predicts the move from; the same threads skipped),
// and a "look <yaw> <pitch>" line after each move packet that carries a look (the same threads skipped), and a
// "tickat <unix seconds>" line after each "order tickend" (when that client tick went).

use std::io::Write as _;
use std::sync::{Mutex, OnceLock};

const KINDS: [&str; 6] = ["pos", "posrot", "rot", "status", "tickend", "teleport"];

static OUT: OnceLock<Option<(Mutex<std::fs::File>, [i32; 13])>> = OnceLock::new();

// The packet id at the start of an encoded packet (a VarInt), and its length.
fn id(body: &[u8]) -> Option<(i32, usize)> {
    let mut value = 0i32;
    for (i, b) in body.iter().take(5).enumerate() {
        value |= i32::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

pub fn sent(body: &[u8]) {
    let out = OUT.get_or_init(|| {
        use crate::packets::play::{self, Serverbound as S};
        let path = std::env::var_os("SUNSET_PROBE")?;
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
        let made = [
            S::MovePlayerPos(play::MovePlayerPos { x: 0.0, y: 0.0, z: 0.0, flags: 0 }),
            S::MovePlayerPosRot(play::MovePlayerPosRot { x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0, flags: 0 }),
            S::MovePlayerRot(play::MovePlayerRot { yaw: 0.0, pitch: 0.0, flags: 0 }),
            S::MovePlayerStatus(play::MovePlayerStatus { flags: 0 }),
            S::ClientTickEnd(crate::packets::Empty),
            S::AcceptTeleportation(play::AcceptTeleportation { id: 0, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0 }),
            S::Attack(play::Attack { id: 0 }),
            S::PlayerAction(play::PlayerAction { action: 8, pos: [0; 3], direction: 0, sequence: 0 }),
            S::PlayerInput(play::PlayerInput { flags: 0 }),
            S::ContainerClick(play::ContainerClick { container: 0, state: 0, slot: 0, button: 0, input: 0, changed: Vec::new(), carried: None }),
            S::ContainerClose(play::ContainerClose { container: 0 }),
            S::PlayerCommand(play::PlayerCommand { id: 0, action: 2, data: 0 }),
            S::UseItemOn(play::UseItemOn { hand: 0, pos: [0; 3], face: 1, cursor: [0.5; 3], inside: false, world_border: false, sequence: 0 }),
        ];
        Some((Mutex::new(file), made.map(|p| id(&p.encode()).map_or(-1, |(id, _)| id))))
    });
    let (Some((file, ids)), Some((got, start))) = (out, id(body)) else {
        return;
    };
    let Ok(mut f) = file.lock() else {
        return;
    };
    let own = !std::thread::current().name().is_some_and(|n| n.starts_with("horizon-lan") || n.starts_with("horizon-local"));
    if own {
        let _ = f.write_all(format!("pkt {got}\n").as_bytes());
    }
    let Some(i) = ids.iter().position(|x| *x == got) else {
        return;
    };
    if own && i >= 9 {
        let data = &body[start..];
        let mut at = 0;
        let mut number = || { let (value, len) = id(data.get(at..)?)?; at += len; Some(value) };
        match i {
            9 => {
                if let (Some(container), Some(state)) = (number(), number()) {
                    drop(number);
                    if let Some(slot) = data.get(at..at + 2).and_then(|b| b.try_into().ok()).map(i16::from_be_bytes)
                        && let Some(button) = data.get(at + 2)
                        && let Some((input, _)) = id(&data[at + 3..])
                    {
                        let _ = writeln!(f, "inventory click {container} {state} {slot} {button} {input}");
                    }
                }
            }
            10 => { if let Some(container) = number() { let _ = writeln!(f, "inventory close {container}"); } }
            11 => {
                if let (Some(_), Some(action)) = (number(), number()) && matches!(action, 1 | 2) {
                    let _ = writeln!(f, "sprint {}", u8::from(action == 1));
                }
            }
            _ => { let _ = writeln!(f, "order place"); }
        }
        return;
    }
    if own && i == 6 && let Some((target, _)) = id(&body[start..]) {
        let _ = f.write_all(format!("attack {target}\n").as_bytes());
    }
    if i < KINDS.len() {
        let _ = f.write_all(format!("send {}{}\n", KINDS[i], moved(i, &body[start..])).as_bytes());
    }
    if own && let Some((yaw, pitch)) = looked(i, &body[start..]) {
        let _ = f.write_all(format!("look {yaw:.2} {pitch:.2}\n").as_bytes());
    }
    if i == 8 {
        if own && let Some(flags) = body.get(start) {
            let _ = f.write_all(format!("input {flags}\n").as_bytes());
        }
        return;
    }
    let order = match i {
        0..=3 => "move",
        4 => "tickend",
        6 => "attack",
        // PlayerAction's action comes first: 8 is a spear's stab.
        7 if body.get(start) == Some(&8) => "stab",
        _ => return,
    };
    if own {
        let _ = f.write_all(format!("order {order}\n").as_bytes());
        // When each client tick left (unix seconds), to tell a steady stream from bursts.
        if i == 4 {
            let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
            let _ = writeln!(f, "tickat {at:.4}");
        }
    }
}

// " <y> <ground>" for a move packet (its fields after the id: x y z as doubles, then the look as two floats, then the
// flags byte), else nothing.
fn moved(kind: usize, body: &[u8]) -> String {
    let (y, flags) = match kind {
        0 => (Some(8), 24),
        1 => (Some(8), 32),
        2 => (None, 8),
        3 => (None, 0),
        _ => return String::new(),
    };
    let Some(&flags) = body.get(flags) else {
        return String::new();
    };
    let y = y.and_then(|at| body.get(at..at + 8)).and_then(|b| b.try_into().ok()).map(f64::from_be_bytes);
    format!(" {} {}", y.map_or("-".to_string(), |y| format!("{y:.5}")), flags & 1)
}

// The look a move packet carries (posrot: after x y z; rot: first), as two floats.
fn looked(kind: usize, body: &[u8]) -> Option<(f32, f32)> {
    let at = match kind {
        1 => 24,
        2 => 0,
        _ => return None,
    };
    let float = |from: usize| body.get(from..from + 4).and_then(|b| b.try_into().ok()).map(f32::from_be_bytes);
    Some((float(at)?, float(at + 4)?))
}
