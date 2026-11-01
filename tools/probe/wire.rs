// The probe's packet log: every move packet, every end of a client tick and every accepted teleport (a server's
// correction, or a /tp) as they go on the wire (after Sunset has changed them), so verify.py can count the move
// packets in a tick and the times a server put the player back. Lines "send <kind>" in the SUNSET_PROBE file; a move
// packet's line ends with the height it says (or "-") and its on-ground flag (0 or 1), so the falls the server counts
// can be added up. Packets are known by their ids, read off packets of each kind made here. And "order <kind>" lines
// for the client's own hits (attack, a spear's stab), move packets (move) and tick ends, in the order they go, for the
// check that a hit goes before its tick's move packet as vanilla sends it: those skip the single player server's
// packets (its threads are named horizon-lan-... and horizon-local-...), so they hold in single player too.

use std::io::Write as _;
use std::sync::{Mutex, OnceLock};

const KINDS: [&str; 6] = ["pos", "posrot", "rot", "status", "tickend", "teleport"];

static OUT: OnceLock<Option<(Mutex<std::fs::File>, [i32; 8])>> = OnceLock::new();

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
        ];
        Some((Mutex::new(file), made.map(|p| id(&p.encode()).map_or(-1, |(id, _)| id))))
    });
    let (Some((file, ids)), Some((got, start))) = (out, id(body)) else {
        return;
    };
    let Some(i) = ids.iter().position(|x| *x == got) else {
        return;
    };
    let Ok(mut f) = file.lock() else {
        return;
    };
    if i < KINDS.len() {
        let _ = f.write_all(format!("send {}{}\n", KINDS[i], moved(i, &body[start..])).as_bytes());
    }
    let order = match i {
        0..=3 => "move",
        4 => "tickend",
        6 => "attack",
        // PlayerAction's action comes first: 8 is a spear's stab.
        7 if body.get(start) == Some(&8) => "stab",
        _ => return,
    };
    if !std::thread::current().name().is_some_and(|n| n.starts_with("horizon-lan") || n.starts_with("horizon-local")) {
        let _ = f.write_all(format!("order {order}\n").as_bytes());
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
