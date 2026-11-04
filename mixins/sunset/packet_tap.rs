// The packets Crystal Aura and Anchor Aura learn from (blocks placed, players seen, hurt, equipped, gone; a new world
// or life), Debug Report's server clock and disconnects, Jump Reset's knockback and the inventory modules' slot updates, copied as they arrive. The game handles them in the same frame; Sunset reads the copies at its next tick
// (crate::game::sunset::combat_packets). Kept to the last 4096, should no tick come to read them.

use horizon_protocol::packets::play::Clientbound as C;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// Server Scan's evidence, each piece tagged with the configuration it belongs to: 1 for the first, one more for
// each proxy backend switch. This tap counts configurations as the game takes them in (Joined, then each
// Reconfigured), so play packets still queued from the old backend keep its number. A configuration payload
// arrives before its Reconfigured, so the network thread tags it with the number after the game's.
#[derive(Debug)]
pub enum ScanEvent {
    ConfigChannel(u32, String),
    Brand(u32, String),
    Channel(u32, String),
    Commands(u32),
    Generation(u32),
    Overflow,
}
static SCAN_ACTIVE: AtomicBool = AtomicBool::new(false);
static SCAN: Mutex<VecDeque<ScanEvent>> = Mutex::new(VecDeque::new());
static PLAY_GEN: AtomicU32 = AtomicU32::new(0);
// A brand event waits for the next event to tell its configuration: a switch means the new backend sent it
// while configuring, a play packet means the current backend sent it in play.
static BRAND_WAITING: AtomicBool = AtomicBool::new(false);
static BRAND: Mutex<Option<String>> = Mutex::new(None);

pub fn scan_enable(on: bool) {
    SCAN_ACTIVE.store(on, Ordering::Relaxed);
    if !on { SCAN.lock().unwrap_or_else(|e| e.into_inner()).clear(); }
}

// A new connection: its first configuration is number 1.
pub fn scan_clear() {
    PLAY_GEN.store(0, Ordering::Relaxed);
    BRAND_WAITING.store(false, Ordering::Relaxed);
    SCAN.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

pub fn scan_generation() -> u32 { PLAY_GEN.load(Ordering::Relaxed) }

// The connection is in a configuration after the first (a proxy switching backends) right now.
pub fn scan_configuring(session: &super::Session) -> bool { session.configuring.load(Ordering::Relaxed) }

fn scan_push(event: ScanEvent) {
    let mut queue = SCAN.lock().unwrap_or_else(|e| e.into_inner());
    scan_push_to(&mut queue, event);
}

fn scan_push_to(queue: &mut VecDeque<ScanEvent>, event: ScanEvent) {
    if queue.len() == 256 {
        queue.clear();
        queue.push_back(ScanEvent::Overflow);
    }
    queue.push_back(event);
}

fn channel_id(id: &str) -> bool { !id.is_empty() && id.len() <= 128 && id.is_ascii() }

// minecraft:register lists the channels the server listens on, NUL separated.
fn registered(data: &[u8], mut push: impl FnMut(String)) {
    for id in data.split(|&b| b == 0).take(64) {
        if let Ok(id) = std::str::from_utf8(id) && channel_id(id) {
            push(id.to_owned());
        }
    }
}

// Configuration payloads (network thread). Play's minecraft:brand comes here too; the brand is read from its
// game event instead. A game a whole configuration behind would tag these one too low.
pub fn scan_config(payload: &horizon_protocol::packets::configuration::CustomPayload) {
    if !SCAN_ACTIVE.load(Ordering::Relaxed) { return; }
    let generation = PLAY_GEN.load(Ordering::Relaxed) + 1;
    match payload.channel.as_str() {
        "minecraft:brand" | "minecraft:unregister" => {}
        "minecraft:register" => registered(&payload.data, |id| scan_push(ScanEvent::ConfigChannel(generation, id))),
        id if channel_id(id) => scan_push(ScanEvent::ConfigChannel(generation, id.to_owned())),
        _ => {}
    }
}

pub fn scan_take() -> VecDeque<ScanEvent> {
    std::mem::take(&mut *SCAN.lock().unwrap_or_else(|e| e.into_inner()))
}

fn brand_to(generation: u32) {
    if BRAND_WAITING.swap(false, Ordering::Relaxed)
        && let Some(brand) = BRAND.lock().unwrap_or_else(|e| e.into_inner()).take()
    {
        scan_push(ScanEvent::Brand(generation, brand));
    }
}

fn scan_seen(event: &super::Event, active: bool) {
    let generation = match event {
        super::Event::Joined { .. } | super::Event::Reconfigured(_) => PLAY_GEN.fetch_add(1, Ordering::Relaxed) + 1,
        _ => PLAY_GEN.load(Ordering::Relaxed),
    };
    if !active { return; }
    match event {
        super::Event::Brand(brand) => {
            *BRAND.lock().unwrap_or_else(|e| e.into_inner()) = Some(brand.chars().take(128).collect());
            BRAND_WAITING.store(true, Ordering::Relaxed);
        }
        super::Event::Joined { .. } | super::Event::Reconfigured(_) => {
            brand_to(generation);
            scan_push(ScanEvent::Generation(generation));
        }
        super::Event::Packet(packet) => match &**packet {
            _ if BRAND_WAITING.load(Ordering::Relaxed) => {
                brand_to(generation);
                scan_seen(event, active);
            }
            C::Commands(_) => scan_push(ScanEvent::Commands(generation)),
            C::CustomPayload(payload) if payload.channel == "minecraft:register" =>
                registered(&payload.data, |id| scan_push(ScanEvent::Channel(generation, id))),
            C::CustomPayload(payload) if payload.channel != "minecraft:unregister" && channel_id(&payload.channel) =>
                scan_push(ScanEvent::Channel(generation, payload.channel.clone())),
            _ => {}
        },
        _ => {}
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;

    #[test]
    fn scan_queue_is_bounded_and_marks_lost_evidence() {
        let mut queue = VecDeque::new();
        for _ in 0..1024 { scan_push_to(&mut queue, ScanEvent::Commands(1)); }
        assert!(queue.len() <= 256);
        assert!(queue.iter().any(|event| matches!(event, ScanEvent::Overflow)));
    }

    #[test]
    fn register_payload_is_split_and_bounded() {
        let mut out = Vec::new();
        registered(b"worldedit:cui\0bungeecord:main\0\0\xff\xfe", |id| out.push(id));
        assert_eq!(out, ["worldedit:cui", "bungeecord:main"]);
        let mut many = 0;
        registered(&b"a:b\0".repeat(500), |_| many += 1);
        assert_eq!(many, 64);
    }
}

static SEEN: Mutex<Vec<C>> = Mutex::new(Vec::new());

// Debug Report: the server's clock as it came (when, and its game time), for a TPS estimate, and the connections'
// ends with their reasons.
static CLOCK: Mutex<VecDeque<(Instant, i64)>> = Mutex::new(VecDeque::new());
static ENDS: Mutex<VecDeque<(std::time::SystemTime, String, bool)>> = Mutex::new(VecDeque::new());

fn debug_seen(event: &super::Event) {
    match event {
        super::Event::Joined { .. } => CLOCK.lock().unwrap_or_else(|e| e.into_inner()).clear(),
        super::Event::Disconnected { reason, from_server } => {
            let mut ends = ENDS.lock().unwrap_or_else(|e| e.into_inner());
            if ends.len() == 20 {
                ends.pop_front();
            }
            ends.push_back((std::time::SystemTime::now(), reason.chars().take(512).collect(), *from_server));
        }
        super::Event::Packet(p) => {
            if let C::SetTime(t) = &**p {
                let mut clock = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
                if clock.len() == 30 {
                    clock.pop_front();
                }
                clock.push_back((Instant::now(), t.game_time));
            }
        }
        _ => {}
    }
}

// The server's ticks a second over the clock updates kept (about the last 30 seconds), once they span 3 seconds, and
// the seconds they span.
pub fn tps() -> Option<(f64, f64)> {
    let clock = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (first, last) = (clock.front()?, clock.back()?);
    let seconds = last.0.duration_since(first.0).as_secs_f64();
    (seconds >= 3.0).then(|| ((last.1 - first.1) as f64 / seconds, seconds))
}

// The connections' ends kept (when, the reason, whether the server gave it), oldest first.
pub fn ends() -> Vec<(std::time::SystemTime, String, bool)> {
    ENDS.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}

pub fn seen(events: &[super::Event]) {
    let scan_active = SCAN_ACTIVE.load(Ordering::Relaxed);
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    for event in events {
        scan_seen(event, scan_active);
        debug_seen(event);
        if let super::Event::Packet(p) = event
            && matches!(
                **p,
                C::Login(_)
                    | C::Respawn(_)
                    | C::Disconnect(_)
                    | C::PlayerPosition(_)
                    | C::BlockUpdate(_)
                    | C::SectionBlocksUpdate(_)
                    | C::AddEntity(_)
                    | C::RemoveEntities(_)
                    | C::SetEntityData(_)
                    | C::SetEquipment(_)
                    | C::DamageEvent(_)
                    | C::EntityEvent(_)
                    | C::SetEntityMotion(_)
                    | C::ContainerSetSlot(_)
                    | C::ContainerSetContent(_)
                    | C::SetPlayerInventory(_)
                    | C::SetCursorItem(_)
                    | C::OpenScreen(_)
            )
            && seen.len() < 4096
        {
            seen.push((**p).clone());
        }
    }
}

pub fn take() -> Vec<C> {
    std::mem::take(&mut *SEEN.lock().unwrap_or_else(|e| e.into_inner()))
}

// Knockback Delay: the pushes on you (your velocity, an explosion's knockback) wait here, by chance, before the game
// takes them; the rest comes in on time. Sunset's tick says each tick how: whose pushes, the chance and the wait (ms).
struct Delay {
    me: i32,
    chance: f64,
    wait: [u64; 2],
    told: Instant,
    held: VecDeque<(Instant, super::Event)>,
    roll: u64,
}

static DELAY: Mutex<Option<Delay>> = Mutex::new(None);

pub fn delay(me: i32, chance: f64, wait: [u64; 2]) {
    let mut d = DELAY.lock().unwrap_or_else(|e| e.into_inner());
    let d = d.get_or_insert_with(|| Delay { me, chance, wait, told: Instant::now(), held: VecDeque::new(), roll: 0x9e37_79b9_7f4a_7c15 });
    (d.me, d.chance, d.wait, d.told) = (me, chance, wait, Instant::now());
}

pub fn delay_off() {
    if let Some(d) = DELAY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        d.told -= Duration::from_secs(1);
    }
}

pub fn hold(events: Vec<super::Event>) -> Vec<super::Event> {
    let mut guard = DELAY.lock().unwrap_or_else(|e| e.into_inner());
    let Some(d) = guard.as_mut() else {
        return events;
    };
    // Sunset's tick stopped coming (off, a menu, no world): nothing waits.
    let now = Instant::now();
    let on = now - d.told < Duration::from_millis(250);
    if d.held.is_empty() && !on {
        return events;
    }
    let mut out = Vec::with_capacity(events.len() + d.held.len());
    let due = d.held.iter().take_while(|(at, _)| !on || *at <= now).count();
    out.extend(d.held.drain(..due).map(|(_, e)| e));
    for event in events {
        if let super::Event::Packet(p) = &event {
            match &**p {
                // A new world or life: the old one's pushes are no good.
                C::Login(_) | C::Respawn(_) => d.held.clear(),
                // A teleport: what came before it first.
                C::PlayerPosition(_) => out.extend(d.held.drain(..).map(|(_, e)| e)),
                C::SetEntityMotion(m) if m.id == d.me => {}
                C::Explode(x) if x.knockback.is_some() => {}
                _ => {
                    out.push(event);
                    continue;
                }
            }
            if on && matches!(&**p, C::SetEntityMotion(_) | C::Explode(_)) {
                // One waiting already: this one goes after it; else the chance decides.
                let at = if let Some((last, _)) = d.held.back() {
                    Some(*last)
                } else {
                    d.roll ^= d.roll << 13;
                    d.roll ^= d.roll >> 7;
                    d.roll ^= d.roll << 17;
                    let [lo, hi] = d.wait;
                    let lucky = ((d.roll % 10_000) as f64) < d.chance * 100.0;
                    (lucky && hi > 0).then(|| now + Duration::from_millis(lo + (d.roll >> 16) % (hi - lo + 1)))
                };
                if let Some(at) = at {
                    d.held.push_back((at, event));
                    continue;
                }
            }
            out.extend(d.held.drain(..).map(|(_, e)| e));
        }
        out.push(event);
    }
    out
}

// Backtrack (Fake Lag's): while the player you last hit moves away from you, everything the server sends waits a
// little (each packet Shortest to Longest latency, in order), so the world, them included, reaches you that much later
// and stays in reach a moment longer; when they stop moving away, all of it comes at once. A teleport, your own
// velocity, a new world or their removal lets it all through first. Sunset's tick says each tick whom, from where
// (your eyes) and how long; a tick that stops coming lets go of it all.
struct Backtrack {
    me: i32,
    target: i32,
    eye: [f64; 3],
    latency: [u64; 2],
    told: Instant,
    // Where the server has them, from what it sent (their position as the game had it when they became the target).
    server: Option<[f64; 3]>,
    away: bool,
    last_due: Option<Instant>,
    held: VecDeque<(Instant, super::Event)>,
    roll: u64,
}

static BACKTRACK: Mutex<Option<Backtrack>> = Mutex::new(None);

pub fn backtrack(me: i32, target: i32, at: [f64; 3], eye: [f64; 3], latency: [u64; 2]) {
    let mut b = BACKTRACK.lock().unwrap_or_else(|e| e.into_inner());
    let b = b.get_or_insert_with(|| Backtrack {
        me,
        target,
        eye,
        latency,
        told: Instant::now(),
        server: None,
        away: false,
        last_due: None,
        held: VecDeque::new(),
        roll: 0x2545_f491_4f6c_dd1d,
    });
    if b.target != target || b.server.is_none() {
        b.server = Some(at);
        b.away = false;
    }
    (b.me, b.target, b.eye, b.latency, b.told) = (me, target, eye, latency, Instant::now());
}

pub fn backtrack_off() {
    if let Some(b) = BACKTRACK.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        b.told -= Duration::from_secs(1);
    }
}

// Where Backtrack's player really is, while it holds what came in.
pub fn backtrack_shown() -> Option<[f64; 3]> {
    BACKTRACK.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|b| !b.held.is_empty()).and_then(|b| b.server)
}

fn far(eye: [f64; 3], at: [f64; 3]) -> f64 {
    let centre = [at[0], at[1] + 0.9, at[2]];
    (0..3).map(|i| (centre[i] - eye[i]).powi(2)).sum::<f64>().sqrt()
}

pub fn backtrack_hold(events: Vec<super::Event>) -> Vec<super::Event> {
    let mut guard = BACKTRACK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(b) = guard.as_mut() else {
        return events;
    };
    let now = Instant::now();
    let on = now - b.told < Duration::from_millis(250);
    if b.held.is_empty() && !on {
        b.server = None;
        return events;
    }
    let mut out = Vec::with_capacity(events.len() + b.held.len());
    let due = b.held.iter().take_while(|(at, _)| on && *at <= now).count();
    let due = if on && b.held.len() < 4096 { due } else { b.held.len() };
    out.extend(b.held.drain(..due).map(|(_, e)| e));
    for event in events {
        let super::Event::Packet(p) = &event else {
            out.extend(b.held.drain(..).map(|(_, e)| e));
            out.push(event);
            continue;
        };
        let flush = match &**p {
            C::Login(_) | C::Respawn(_) | C::Disconnect(_) | C::PlayerPosition(_) => true,
            C::SetEntityMotion(m) => m.id == b.me,
            C::RemoveEntities(r) => r.ids.contains(&b.target),
            _ => false,
        };
        if flush || !on {
            out.extend(b.held.drain(..).map(|(_, e)| e));
            b.away = false;
            if flush {
                b.server = None;
            }
            out.push(event);
            continue;
        }
        let moved = match &**p {
            C::MoveEntityPos(m) if m.0.id == b.target => b.server.zip(m.0.delta.as_ref()).map(|(at, d)| d.end(at)),
            C::MoveEntityPosRot(m) if m.0.id == b.target => b.server.zip(m.0.delta.as_ref()).map(|(at, d)| d.end(at)),
            C::TeleportEntity(t) if t.id == b.target && t.relatives == 0 => Some(t.position),
            C::EntityPositionSync(s) if s.id == b.target => Some(s.path.end()),
            _ => None,
        };
        if let Some(at) = moved {
            b.away = b.server.is_some_and(|was| far(b.eye, at) > far(b.eye, was) + 0.001);
            b.server = Some(at);
        }
        if !b.away {
            out.extend(b.held.drain(..).map(|(_, e)| e));
            out.push(event);
            continue;
        }
        b.roll ^= b.roll << 13;
        b.roll ^= b.roll >> 7;
        b.roll ^= b.roll << 17;
        let [lo, hi] = b.latency;
        let mut at = now + Duration::from_millis(lo + b.roll % (hi - lo + 1));
        if let Some(last) = b.last_due.filter(|last| at <= *last) {
            at = last + Duration::from_micros(1);
        }
        b.last_due = Some(at);
        b.held.push_back((at, event));
    }
    out
}
