// The probe's packet log: every move packet and every end of a client tick as they go on the wire (after Sunset has
// changed them), so verify.py can count the move packets in a tick. Lines "send <kind>" in the SUNSET_PROBE file.
// Packets are known by their ids, read off packets of each kind made here.

use std::io::Write as _;
use std::sync::{Mutex, OnceLock};

const KINDS: [&str; 5] = ["pos", "posrot", "rot", "status", "tickend"];

static OUT: OnceLock<Option<(Mutex<std::fs::File>, [i32; 5])>> = OnceLock::new();

// The packet id at the start of an encoded packet (a VarInt).
fn id(body: &[u8]) -> Option<i32> {
    let mut value = 0i32;
    for (i, b) in body.iter().take(5).enumerate() {
        value |= i32::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Some(value);
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
        ];
        Some((Mutex::new(file), made.map(|p| id(&p.encode()).unwrap_or(-1))))
    });
    let (Some((file, ids)), Some(got)) = (out, id(body)) else {
        return;
    };
    if let (Some(i), Ok(mut f)) = (ids.iter().position(|x| *x == got), file.lock()) {
        let _ = f.write_all(format!("send {}\n", KINDS[i]).as_bytes());
    }
}
