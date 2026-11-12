// The packets Crystal Aura and Anchor Aura learn from (blocks placed, players seen, hurt, equipped, gone; a new world
// or life), Debug Report's server clock and disconnects, Jump Reset's knockback and the inventory modules' slot updates, copied as they arrive. The game handles them in the same frame; Sunset reads the copies at its next tick
// (crate::game::sunset::combat_packets). Kept to the last 4096, should no tick come to read them.

use horizon_protocol::packets::play::Clientbound as C;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

// ---------------------------------------------------------------- which connection a held packet belongs to

// Horizon's Session has no serial number, so a held packet is bound to its Session by the Session's own `counters`
// cell, held weakly: the allocation cannot be reused for another Session while a Link to it exists, so comparing
// addresses is exact. The epoch counts the connection's boundaries as the game takes them in (a join, a proxy's
// reconfiguration start and end, a disconnect): the network thread acknowledges a reconfiguration by itself, so a play
// packet written before one can never reach the same server, and after it would reach the next backend.
static EPOCH: AtomicU64 = AtomicU64::new(0);
static SERIAL: AtomicU64 = AtomicU64::new(0);
static LAST_SESSION: Mutex<Option<Weak<CountersCell>>> = Mutex::new(None);

type CountersCell = Mutex<Option<Arc<horizon_protocol::connection::Counters>>>;

#[derive(Clone, Debug)]
pub struct Link {
    cell: Weak<CountersCell>,
    // For reports: the n-th session this game polled, and its epoch.
    pub serial: u64,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linked {
    Same,
    // The same Session, past a reconfiguration or a disconnect.
    Reconfigured,
    // Another Session, or none.
    Other,
}

impl Link {
    pub fn of(session: &super::Session) -> Self {
        Self { cell: Arc::downgrade(&session.counters), serial: SERIAL.load(Ordering::Acquire), epoch: EPOCH.load(Ordering::Acquire) }
    }

    pub fn to(&self, session: Option<&super::Session>) -> Linked {
        match session {
            Some(s) if std::ptr::eq(self.cell.as_ptr(), Arc::as_ptr(&s.counters)) => {
                if self.epoch == EPOCH.load(Ordering::Acquire) { Linked::Same } else { Linked::Reconfigured }
            }
            _ => Linked::Other,
        }
    }
}

fn boundary(event: &super::Event) -> bool {
    matches!(
        event,
        super::Event::Joined { .. } | super::Event::Reconfiguring | super::Event::Reconfigured(_) | super::Event::Disconnected { .. }
    )
}

// Every poll, before anything reads its events (also while Sunset is stopped).
pub fn note(session: &super::Session, events: &[super::Event]) {
    {
        let mut last = LAST_SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_ref().is_none_or(|w| !std::ptr::eq(w.as_ptr(), Arc::as_ptr(&session.counters))) {
            *last = Some(Arc::downgrade(&session.counters));
            SERIAL.fetch_add(1, Ordering::AcqRel);
        }
    }
    let n = events.iter().filter(|e| boundary(e)).count() as u64;
    if n > 0 {
        EPOCH.fetch_add(n, Ordering::AcqRel);
    }
}

// What an incoming queue did, for Debug Report.
#[derive(Clone, Copy, Debug, Default)]
pub struct QueueStats {
    pub held: u64,
    pub released: u64,
    // By reason: see FLUSH_REASONS.
    pub flushed: [u64; 7],
    // Thrown away: held for another session.
    pub discarded_session: u64,
    // Thrown away: pushes of a life that ended (Knockback Delay).
    pub discarded_life: u64,
    pub max_depth: usize,
    pub depth: usize,
    pub target_switches: u64,
}

pub const FLUSH_REASONS: [&str; 7] =
    ["connection boundary", "teleport", "own velocity", "target removed", "target switch", "not moving away", "off"];

fn flushed<T>(held: &mut VecDeque<(Instant, T)>, stats: &mut QueueStats, reason: usize, out: &mut Vec<T>) {
    if held.is_empty() {
        return;
    }
    stats.flushed[reason] += held.len() as u64;
    stats.released += held.len() as u64;
    out.extend(held.drain(..).map(|(_, e)| e));
}

// GUI Move owns the client prediction until its held clicks have reached the server. Keep incoming authority
// separate meanwhile; the game rebases the same semantic clicks before another mouse action can use it.
static GUI_MOVE_ACTIVE: AtomicBool = AtomicBool::new(false);
static GUI_MOVE_UPDATES: Mutex<Vec<C>> = Mutex::new(Vec::new());

pub fn gui_move_active(active: bool) {
    GUI_MOVE_ACTIVE.store(active, Ordering::Release);
}

pub fn gui_move_take() -> Vec<C> {
    std::mem::take(&mut *GUI_MOVE_UPDATES.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn gui_move_pending() -> bool {
    !GUI_MOVE_UPDATES.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
}

fn gui_move_key(packet: &C) -> Option<(u8, i32)> {
    match packet {
        C::ContainerSetContent(p) if p.container == 0 => Some((0, 0)),
        C::ContainerSetSlot(p) if p.container == 0 && (0..46).contains(&p.slot) => Some((1, i32::from(p.slot))),
        C::SetPlayerInventory(p) if (0..41).contains(&p.slot) => Some((2, p.slot)),
        C::SetCursorItem(_) => Some((3, 0)),
        _ => None,
    }
}

fn gui_move_defer(holding: &mut bool, boundary: bool, inventory: bool) -> bool {
    if boundary { *holding = false; }
    *holding && inventory
}

#[cfg(test)]
mod gui_move_boundary_tests {
    use super::gui_move_defer;

    #[test]
    fn later_inventory_authority_keeps_its_order_after_a_menu_boundary() {
        let mut holding = true;
        assert!(gui_move_defer(&mut holding, false, true));
        assert!(!gui_move_defer(&mut holding, true, false));
        assert!(!gui_move_defer(&mut holding, false, false));
        assert!(!gui_move_defer(&mut holding, false, true));
    }
}

pub fn gui_move_hold(events: Vec<super::Event>) -> Vec<super::Event> {
    if !GUI_MOVE_ACTIVE.load(Ordering::Acquire) { return events; }
    let mut holding = true;
    events.into_iter().filter(|event| {
        let super::Event::Packet(packet) = event else { return true; };
        // A boundary is applied by native_client_packet's HEAD hook before its native handler. Only updates
        // preceding it belong to the old shadow. Everything after it retains the network's original order.
        let boundary = matches!(&**packet, C::OpenScreen(_) | C::MountScreenOpen(_) | C::ContainerClose(_)
            | C::Respawn(_) | C::Login(_) | C::Disconnect(_));
        if boundary {
            GUI_MOVE_ACTIVE.store(false, Ordering::Release);
        }
        let key = gui_move_key(packet);
        if !gui_move_defer(&mut holding, boundary, key.is_some()) { return true; }
        let Some(key) = key else { return true; };
        let mut updates = GUI_MOVE_UPDATES.lock().unwrap_or_else(|e| e.into_inner());
        if key == (0, 0) {
            // A full own-menu snapshot supersedes every older own-menu slot/cursor snapshot.
            updates.clear();
        } else {
            updates.retain(|old| gui_move_key(old) != Some(key));
        }
        updates.push((**packet).clone());
        false
    }).collect()
}

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

// The plugin API's: every packet (once a plugin listens for them, the last 4096 kept) and the connection's joins and
// ends, read by the game at its next tick and frame.
pub static PLUGIN_PACKETS: AtomicBool = AtomicBool::new(false);
static PLUGIN_IN: Mutex<VecDeque<C>> = Mutex::new(VecDeque::new());
pub enum Connection {
    Joined,
    // The reason, its translation key when it had one, and whether the server gave it.
    Left(String, Option<String>, bool),
}
static PLUGIN_CONNECTION: Mutex<Vec<Connection>> = Mutex::new(Vec::new());

pub fn plugin_packets() -> Vec<C> {
    PLUGIN_IN.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
}

pub fn plugin_connection() -> Vec<Connection> {
    std::mem::take(&mut *PLUGIN_CONNECTION.lock().unwrap_or_else(|e| e.into_inner()))
}

fn plugin_seen(event: &super::Event) {
    match event {
        super::Event::Joined { .. } => PLUGIN_CONNECTION.lock().unwrap_or_else(|e| e.into_inner()).push(Connection::Joined),
        super::Event::Disconnected { reason, key, from_server } => PLUGIN_CONNECTION
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Connection::Left(reason.chars().take(512).collect(), key.clone(), *from_server)),
        super::Event::Packet(p) if PLUGIN_PACKETS.load(Ordering::Relaxed) => {
            let mut queue = PLUGIN_IN.lock().unwrap_or_else(|e| e.into_inner());
            if queue.len() == 4096 {
                queue.pop_front();
            }
            queue.push_back((**p).clone());
        }
        _ => {}
    }
}

// Debug Report: the server's clock as it came (when, and its game time), for a TPS estimate, and the connections'
// ends with their reasons.
static CLOCK: Mutex<VecDeque<(Instant, i64)>> = Mutex::new(VecDeque::new());
static ENDS: Mutex<VecDeque<(std::time::SystemTime, String, bool)>> = Mutex::new(VecDeque::new());

fn debug_seen(event: &super::Event) {
    match event {
        super::Event::Joined { .. } => CLOCK.lock().unwrap_or_else(|e| e.into_inner()).clear(),
        super::Event::Disconnected { reason, from_server, .. } => {
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
        plugin_seen(event);
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
                    | C::PlayerInfoUpdate(_)
                    | C::PlayerInfoRemove(_)
                    | C::SetEquipment(_)
                    | C::DamageEvent(_)
                    | C::EntityEvent(_)
                    | C::SetEntityMotion(_)
                    | C::ContainerSetSlot(_)
                    | C::ContainerSetContent(_)
                    | C::SetPlayerInventory(_)
                    | C::SetCursorItem(_)
                    | C::ContainerClose(_)
                    | C::OpenScreen(_)
            )
            && seen.len() < 4096
        {
            seen.push((**p).clone());
        }
        if let super::Event::Packet(p) = event
            && SOUNDS_ON.load(Ordering::Relaxed)
            && matches!(
                **p,
                C::Sound(_)
                    | C::SoundEntity(_)
                    | C::BlockEvent(_)
                    | C::LevelEvent(_)
                    | C::BlockDestruction(_)
                    | C::TakeItemEntity(_)
                    | C::Explode(_)
            )
        {
            let mut sounds = SOUNDS.lock().unwrap_or_else(|e| e.into_inner());
            if sounds.len() < 512 {
                sounds.push((**p).clone());
            }
        }
    }
}

// Sound Detector's packets (sounds, block and level events, mining, pickups, explosions), kept apart from the rest so a
// noisy place can't push those out, and only while it is on: the last 512 till its next tick.
static SOUNDS_ON: AtomicBool = AtomicBool::new(false);
static SOUNDS: Mutex<Vec<C>> = Mutex::new(Vec::new());

pub fn sounds_on(on: bool) {
    if SOUNDS_ON.swap(on, Ordering::Relaxed) && !on {
        SOUNDS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

pub fn take_sounds() -> Vec<C> {
    std::mem::take(&mut *SOUNDS.lock().unwrap_or_else(|e| e.into_inner()))
}

// No Interact's Anti trap: what it drops as it arrives, as Sunset's tick last told it (kept while told within 250
// ms): the spawns of these entity types within `range` of `at`, and the sign editor.
struct Trap {
    kinds: Vec<i32>,
    at: [f64; 3],
    range: f64,
    sign: bool,
    told: Instant,
}

static TRAP: Mutex<Option<Trap>> = Mutex::new(None);

pub fn trap(kinds: Vec<i32>, at: [f64; 3], range: f64, sign: bool) {
    *TRAP.lock().unwrap_or_else(|e| e.into_inner()) = Some(Trap { kinds, at, range, sign, told: Instant::now() });
}

pub fn trap_off() {
    *TRAP.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn trap_filter(mut events: Vec<super::Event>) -> Vec<super::Event> {
    let guard = TRAP.lock().unwrap_or_else(|e| e.into_inner());
    let Some(trap) = guard.as_ref().filter(|t| t.told.elapsed() < Duration::from_millis(250)) else {
        return events;
    };
    events.retain(|event| {
        let super::Event::Packet(p) = event else {
            return true;
        };
        match &**p {
            C::OpenSignEditor(_) => !trap.sign,
            C::AddEntity(a) if trap.kinds.contains(&a.kind) => {
                let d2: f64 = (0..3).map(|k| (a.position[k] - trap.at[k]).powi(2)).sum();
                d2 > trap.range * trap.range
            }
            _ => true,
        }
    });
    events
}

pub fn take() -> Vec<C> {
    std::mem::take(&mut *SEEN.lock().unwrap_or_else(|e| e.into_inner()))
}

// Knockback Delay (Velocity's): the pushes on you (your velocity, an explosion's knockback) wait here, by chance, before
// the game takes them; the rest comes in on time. Sunset's tick says each tick how: whose pushes, the chance and the
// wait (ms). Its state is its own: Fake Lag's Knockback mode only sees a push once the game has taken it.
struct Delay {
    me: i32,
    chance: f64,
    wait: [u64; 2],
    told: Instant,
    link: Option<Link>,
    held: VecDeque<(Instant, super::Event)>,
    roll: u64,
    stats: QueueStats,
}

static DELAY: Mutex<Option<Delay>> = Mutex::new(None);

pub fn delay(me: i32, chance: f64, wait: [u64; 2]) {
    let mut d = DELAY.lock().unwrap_or_else(|e| e.into_inner());
    let d = d.get_or_insert_with(|| Delay {
        me,
        chance,
        wait,
        told: Instant::now(),
        link: None,
        held: VecDeque::new(),
        roll: 0x9e37_79b9_7f4a_7c15,
        stats: QueueStats::default(),
    });
    (d.me, d.chance, d.wait, d.told) = (me, chance, wait, Instant::now());
}

pub fn delay_off() {
    if let Some(d) = DELAY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        d.told -= Duration::from_secs(1);
    }
}

pub fn delay_stats() -> Option<QueueStats> {
    DELAY.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|d| QueueStats { depth: d.held.len(), ..d.stats })
}

// Held packets of another Session never reach this one; true when they were thrown away.
fn rebind(link: &mut Option<Link>, session: &super::Session, held: usize, stats: &mut QueueStats) -> bool {
    let stale = link.as_ref().is_some_and(|l| l.to(Some(session)) == Linked::Other);
    if stale {
        stats.discarded_session += held as u64;
    }
    if link.is_none() || stale {
        *link = Some(Link::of(session));
    }
    stale
}

pub fn hold(session: &super::Session, events: Vec<super::Event>) -> Vec<super::Event> {
    let mut guard = DELAY.lock().unwrap_or_else(|e| e.into_inner());
    let Some(d) = guard.as_mut() else {
        return events;
    };
    if rebind(&mut d.link, session, d.held.len(), &mut d.stats) {
        d.held.clear();
    }
    // Sunset's tick stopped coming (off, a menu, no world): nothing waits.
    let now = Instant::now();
    let on = horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() && now - d.told < Duration::from_millis(250);
    if d.held.is_empty() && !on {
        return events;
    }
    let mut out = Vec::with_capacity(events.len() + d.held.len());
    let due = d.held.iter().take_while(|(at, _)| !on || *at <= now).count();
    d.stats.released += due as u64;
    out.extend(d.held.drain(..due).map(|(_, e)| e));
    for event in events {
        let super::Event::Packet(p) = &event else {
            // The connection's boundary: what was held goes first, in order.
            if boundary(&event) {
                flushed(&mut d.held, &mut d.stats, 0, &mut out);
            }
            out.push(event);
            continue;
        };
        match &**p {
            // A new world or life: the old one's pushes are no good.
            C::Login(_) | C::Respawn(_) => {
                d.stats.discarded_life += d.held.len() as u64;
                d.held.clear();
            }
            // A teleport: what came before it first.
            C::PlayerPosition(_) => flushed(&mut d.held, &mut d.stats, 1, &mut out),
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
                d.stats.held += 1;
                d.stats.max_depth = d.stats.max_depth.max(d.held.len());
                continue;
            }
        }
        flushed(&mut d.held, &mut d.stats, 6, &mut out);
        out.push(event);
    }
    out
}

// Backtrack: while the player you last hit moves away from you, everything the server sends waits a little (each
// packet Shortest to Longest latency, in order), so the world, them included, reaches you that much later and stays in
// reach a moment longer; when they stop moving away, all of it comes at once. A teleport, your own velocity, a new
// world, the connection's boundaries, their removal or a new target lets it all through first. Sunset's tick says each
// tick whom, from where (your eyes) and how long; a tick that stops coming lets go of it all. Its state is its own: Fake
// Lag (outgoing) shares nothing with it.
struct Backtrack {
    me: i32,
    target: i32,
    eye: [f64; 3],
    latency: [u64; 2],
    told: Instant,
    link: Option<Link>,
    // Where the server has them, from what it sent, starting from where the game had them with nothing held.
    server: Option<[f64; 3]>,
    // A new target: what is held (the old one's world) goes before anything is held for the new one.
    switched: bool,
    away: bool,
    last_due: Option<Instant>,
    held: VecDeque<(Instant, super::Event)>,
    roll: u64,
    stats: QueueStats,
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
        link: None,
        server: None,
        switched: false,
        away: false,
        last_due: None,
        held: VecDeque::new(),
        roll: 0x2545_f491_4f6c_dd1d,
        stats: QueueStats::default(),
    });
    if b.target != target {
        // The game's position of the new target is behind what is held: track it only once that has gone.
        b.switched = !b.held.is_empty();
        b.server = None;
        b.away = false;
        b.stats.target_switches += 1;
    } else if b.server.is_none() && b.held.is_empty() && !b.switched {
        b.server = Some(at);
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

pub fn backtrack_stats() -> Option<QueueStats> {
    BACKTRACK.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|b| QueueStats { depth: b.held.len(), ..b.stats })
}

fn far(eye: [f64; 3], at: [f64; 3]) -> f64 {
    let centre = [at[0], at[1] + 0.9, at[2]];
    (0..3).map(|i| (centre[i] - eye[i]).powi(2)).sum::<f64>().sqrt()
}

pub fn backtrack_hold(session: &super::Session, events: Vec<super::Event>) -> Vec<super::Event> {
    let mut guard = BACKTRACK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(b) = guard.as_mut() else {
        return events;
    };
    if rebind(&mut b.link, session, b.held.len(), &mut b.stats) {
        b.held.clear();
        b.server = None;
        b.away = false;
        b.switched = false;
    }
    let now = Instant::now();
    let on = horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() && now - b.told < Duration::from_millis(250);
    if b.held.is_empty() && !on {
        b.server = None;
        b.switched = false;
        return events;
    }
    let mut out = Vec::with_capacity(events.len() + b.held.len());
    if b.switched {
        flushed(&mut b.held, &mut b.stats, 4, &mut out);
        b.switched = false;
    }
    let due = b.held.iter().take_while(|(at, _)| on && *at <= now).count();
    if on && b.held.len() < 4096 {
        b.stats.released += due as u64;
        out.extend(b.held.drain(..due).map(|(_, e)| e));
    } else {
        flushed(&mut b.held, &mut b.stats, 6, &mut out);
    }
    for event in events {
        let super::Event::Packet(p) = &event else {
            flushed(&mut b.held, &mut b.stats, 0, &mut out);
            b.away = false;
            if boundary(&event) {
                b.server = None;
            }
            out.push(event);
            continue;
        };
        let flush = match &**p {
            C::Login(_) | C::Respawn(_) | C::Disconnect(_) => Some(0),
            C::PlayerPosition(_) => Some(1),
            C::SetEntityMotion(m) if m.id == b.me => Some(2),
            C::RemoveEntities(r) if r.ids.contains(&b.target) => Some(3),
            _ => None,
        };
        if flush.is_some() || !on {
            flushed(&mut b.held, &mut b.stats, flush.unwrap_or(6), &mut out);
            b.away = false;
            if flush.is_some_and(|f| f != 2) {
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
            flushed(&mut b.held, &mut b.stats, 5, &mut out);
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
        b.stats.held += 1;
        b.stats.max_depth = b.stats.max_depth.max(b.held.len());
    }
    out
}
