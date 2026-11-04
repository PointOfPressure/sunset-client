// Debug Report's packet log: the client's own packets as they go on the wire (after Sunset changed, held or added
// them, with the game's ClientTickEnd between ticks), each tagged with the number of client ticks ended before it; the
// last PlayerInput and the last move packet kept apart too, however long ago they went.
// The single player server's threads (horizon-lan-..., horizon-local-...) write here too and are skipped. Only the
// start of a long packet is kept; the report decodes what it can.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const KEEP: usize = 2048;
const BODY: usize = 256;

#[derive(Clone)]
pub struct Sent {
    pub tick: u64,
    pub id: i32,
    // The packet after its id (cut at BODY bytes).
    pub payload: Vec<u8>,
    pub len: usize,
}

static TICKS: AtomicU64 = AtomicU64::new(0);
static LOG: Mutex<VecDeque<Sent>> = Mutex::new(VecDeque::new());
static LAST_INPUT: Mutex<Option<Sent>> = Mutex::new(None);
static LAST_MOVE: Mutex<Option<Sent>> = Mutex::new(None);

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
    if std::thread::current().name().is_some_and(|n| n.starts_with("horizon-lan") || n.starts_with("horizon-local")) {
        return;
    }
    let Some((id, start)) = id(body) else {
        return;
    };
    use crate::ids::play::serverbound as ids;
    let payload = body[start..].get(..BODY).unwrap_or(&body[start..]).to_vec();
    let sent = Sent { tick: TICKS.load(Ordering::Relaxed), id, payload, len: body.len() - start };
    let last = match id {
        ids::PLAYER_INPUT => Some(&LAST_INPUT),
        ids::MOVE_PLAYER_POS | ids::MOVE_PLAYER_POS_ROT | ids::MOVE_PLAYER_ROT | ids::MOVE_PLAYER_STATUS_ONLY => Some(&LAST_MOVE),
        _ => None,
    };
    if let Some(last) = last {
        *last.lock().unwrap_or_else(|e| e.into_inner()) = Some(sent.clone());
    }
    let mut log = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if log.len() == KEEP {
        log.pop_front();
    }
    log.push_back(sent);
    if id == ids::CLIENT_TICK_END {
        TICKS.fetch_add(1, Ordering::Relaxed);
    }
}

// The last PlayerInput and the last move packet sent.
pub fn last_input() -> Option<Sent> {
    LAST_INPUT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn last_move() -> Option<Sent> {
    LAST_MOVE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

// The client ticks ended so far, and the packets kept, oldest first.
pub fn recent() -> (u64, Vec<Sent>) {
    let log = LOG.lock().unwrap_or_else(|e| e.into_inner());
    (TICKS.load(Ordering::Relaxed), log.iter().cloned().collect())
}

// The client ticks ended so far.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}
