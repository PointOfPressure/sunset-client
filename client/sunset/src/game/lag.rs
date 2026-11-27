// Fake Lag (from Rust's PR #16, rebuilt): what you send waits on a line (sunset_lag_line) and goes in the order it was
// sent, whole client ticks at a time, so the server sees a slower connection rather than a broken client: no move is
// merged, dropped or moved past its ClientTickEnd, and the Pongs answering the server's pings stay where the client
// wrote them. The modes only decide when the line holds and for how long:
// - Delay: always, each packet Delay (ms) late.
// - Pulse: chokes of Shortest to Longest choke (ms), each let go at once at its end, with Pulse gap (ms) (a tick at
//   least) between them.
// - Dynamic: Delay's line while an enemy is near (Trigger: one you face and walk towards, or the nearest closing in),
//   kept a few ticks after it stops, so the edge of the range does not flip it every tick.
// - Knockback: one choke after each push the game takes (after Velocity's Knockback delay, if that holds it).
// - Repel (after vape-rs's Fake Lag, MIT): a hit on a target still red from the last one (its hurt time no longer than
//   Delay) waits, with everything behind it, Delay and up to 100 ms more, or until the target's hurt time has all but
//   run out: the hit lands as the target can be hurt again. A second hit sends it all.
// - Hold (what Blink was): the game's moves are not sent at all while it is on (runtime_packets.rs BLINK), and one
//   move carries you to where you are when it goes off or after Max ticks. This line holds nothing then. With
//   Direction Bi-directional what the server sends waits too, but for its pings (sunset_packets::stall, after
//   vape-rs's Blink);
//   Breadcrumbs draws where you went since it began.
// An action (a hit, a use, a click, chat), with Release on action, sends what is held first and goes at once; so does
// an accepted teleport. A teleport, a respawn or a new dimension lets everything held go before the game answers it,
// and the answering tick goes undelayed. A disconnect, a transfer or a proxy's reconfiguration throws what is held
// away: it was written for a connection that is gone. Turning the module off, or a mode whose condition ends, lets the
// line drain on its schedule; Blink, a combat macro or GUI Move taking the stream empties it at once.
// Backtrack and Knockback Delay hold what comes in: sunset_backtrack and sunset_knockback_delay.
use super::sunset::{self, Camera, Canvas, Category, Module};
use super::sunset_lag_line::{self as line, Decision, Discard, Flush, Hysteresis, Kind, Line, Phase, Plan, Pulse};
use super::{Game, Screen};
use crate::game::sunset_packets::{Link, Linked};
use horizon_protocol::packets::play::Serverbound as S;
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

pub(super) const LAG: &str = "Fake Lag";
// Dynamic lets go after its condition failed this many ticks running.
const EXIT_TICKS: u32 = 3;
// Sunset's tick tells the line how to hold; one that stops coming (Sunset off) holds nothing after this.
const TOLD: Duration = Duration::from_millis(250);

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(LAG, Category::Combat, "Delays what you send, in order. Hold keeps your moves back until you turn it off.")
            .pick("Mode", &["Delay", "Pulse", "Dynamic", "Knockback", "Repel", "Hold"])
            .num("Delay (ms)", 100.0, 10.0, 1000.0, 10.0)
            // Hold (Blink): 0 holds until you turn it off.
            .num("Max ticks", 0.0, 0.0, 200.0, 10.0)
            .pick("Direction", &["Outgoing only", "Bi-directional"])
            .bool("Breadcrumbs", false)
            .pick("Trigger", &["Facing", "Closing"])
            .num("Range", 6.0, 3.0, 12.0, 0.5)
            .num("Shortest choke (ms)", 100.0, 0.0, 1000.0, 10.0)
            .num("Longest choke (ms)", 150.0, 0.0, 1000.0, 10.0)
            .num("Pulse gap (ms)", 0.0, 0.0, 1000.0, 10.0)
            .bool("Release on action", true)
            .bool("Show server position", true),
    ]
}

// Which settings show for a mode (the ClickGUI).
pub(super) fn setting_visible(m: &Module, name: &str) -> bool {
    let mode = m.p("Mode");
    match name {
        "Delay (ms)" => matches!(mode, "Delay" | "Dynamic" | "Repel"),
        "Max ticks" | "Direction" | "Breadcrumbs" => mode == "Hold",
        "Release on action" => mode != "Hold",
        "Trigger" | "Range" => mode == "Dynamic",
        "Shortest choke (ms)" | "Longest choke (ms)" => matches!(mode, "Pulse" | "Knockback"),
        "Pulse gap (ms)" => matches!(mode, "Pulse" | "Knockback"),
        _ => true,
    }
}

// ---------------------------------------------------------------- state

#[derive(Default)]
struct Dynamic {
    gate: Hysteresis,
    // Closing: how far the nearest enemy was last tick.
    enemy_was: Option<f64>,
}

struct Note {
    at: SystemTime,
    tick: u64,
    what: Cow<'static, str>,
    packets: usize,
}

#[derive(Default)]
struct Lag {
    line: Line<S>,
    // The connection what is held was written for.
    link: Option<Link>,
    // The life what is held belongs to: entity id and dimension.
    life: Option<(i32, super::Dim)>,
    // As Sunset's tick last said: actions release, and when it said so.
    actions: bool,
    told: Option<Instant>,
    mode: &'static str,
    pulse: Pulse,
    dynamic: Dynamic,
    // The game took a push on you since the last tick.
    knocked: bool,
    // Where the server has you: the last position that went on the wire.
    sent_at: Option<[f64; 3]>,
    roll: u64,
    notes: VecDeque<Note>,
    // Repel: Delay as Sunset's tick last said, and the hold under way: until when, and whose hit began it.
    delay: Duration,
    repel: Option<(Instant, i32)>,
    // Hold's Breadcrumbs: where you were each tick since it began to hold.
    crumbs: Vec<[f64; 3]>,
}

static LAG_STATE: Mutex<Option<Lag>> = Mutex::new(None);

thread_local! {
    // Inside Fake Lag's own delivery: a packet a module sends from there goes behind what is still to go.
    static DELIVERING: Cell<u32> = const { Cell::new(0) };
}

fn lag<R>(f: impl FnOnce(&mut Lag) -> R) -> R {
    f(LAG_STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Lag::default))
}

fn note(l: &mut Lag, what: impl Into<Cow<'static, str>>, packets: usize) {
    if l.notes.len() == 16 {
        l.notes.pop_front();
    }
    l.notes.push_back(Note { at: SystemTime::now(), tick: ::sunset_core::wire::ticks(), what: what.into(), packets });
}

// ---------------------------------------------------------------- packets

// Packets that do something: with Release on action, what is held goes before them, and they go at once.
fn action(packet: &S) -> bool {
    matches!(
        packet,
        S::Attack(_) | S::Interact(_) | S::UseItemOn(_) | S::UseItem(_) | S::PlayerAction(_) | S::Punch(_)
            | S::ContainerClick(_) | S::ContainerButtonClick(_) | S::ContainerClose(_) | S::Chat(_)
            | S::ChatCommand(_) | S::ChatCommandSigned(_) | S::SignUpdate(_) | S::EditBook(_) | S::PlaceRecipe(_)
            | S::SelectTrade(_) | S::SetCreativeModeSlot(_) | S::PickItemFromBlock(_) | S::PickItemFromEntity(_)
            | S::Unknown(..)
    )
}

fn kind(packet: &S, actions: bool, repel: bool) -> Kind {
    match packet {
        S::ClientTickEnd(_) => Kind::TickEnd,
        // Repel holds a hit: the swing that follows it is no reason to let it go.
        S::Punch(_) if repel => Kind::Other,
        // Answers to the server's re-anchoring: never behind what was written before it.
        S::AcceptTeleportation(_) | S::ConfigurationAcknowledged(_) => Kind::Trigger,
        _ if actions && action(packet) => Kind::Trigger,
        _ => Kind::Other,
    }
}

fn position(packet: &S) -> Option<[f64; 3]> {
    match packet {
        S::MovePlayerPos(p) => Some([p.x, p.y, p.z]),
        S::MovePlayerPosRot(p) => Some([p.x, p.y, p.z]),
        _ => None,
    }
}

// What is held is the connection's it was written for; another connection's (or a reconfigured one's) is thrown away.
fn bind(l: &mut Lag, session: Option<&horizon_client::net::Session>) {
    let linked = l.link.as_ref().map_or(Linked::Other, |old| old.to(session));
    if linked == Linked::Same {
        return;
    }
    if !l.line.is_empty() {
        let why = if linked == Linked::Reconfigured { Discard::Reconfigured } else { Discard::Session };
        let n = l.line.discard(why);
        l.pulse.reset();
        l.dynamic = Dynamic::default();
        note(l, format!("discarded: {}", why.name()), n);
    }
    l.link = session.map(Link::of);
}

// A packet going out now, after the send hook or from the line: Sunset hears of it, then the wire takes it.
fn deliver(game: &Game<'_>, packet: S) {
    DELIVERING.with(|d| d.set(d.get() + 1));
    super::sunset_combat_wire::delivered(game, &packet);
    if let Some(r) = &game.remote {
        r.session.send(packet);
    }
    DELIVERING.with(|d| d.set(d.get() - 1));
}

// What may go now goes, in order; a packet a module sends meanwhile is queued behind and may go in the same pass.
fn release(game: &Game<'_>, now: Instant) {
    for _ in 0..2 * line::MOST {
        let n = lag(|l| l.line.releasable(now));
        if n == 0 {
            return;
        }
        for _ in 0..n {
            let Some(p) = lag(|l| l.line.pop(now)) else { break };
            deliver(game, p);
        }
    }
}

// Panic, while its cleanup waits for a plugin callback: a packet the game sends goes behind what was held, never before
// it (the send hook calls this before letting it through).
pub(super) fn drain(game: &Game<'_>) {
    if DELIVERING.with(Cell::get) == 0 && lag(|l| l.line.len()) > 0 {
        flush(game, Flush::Stopped);
    }
}

// Everything held goes now, in order.
fn flush(game: &Game<'_>, why: Flush) -> usize {
    let now = Instant::now();
    let n = lag(|l| {
        let n = l.line.len();
        if n > 0 {
            l.line.note_flush(why);
        }
        n
    });
    for _ in 0..n + line::MOST {
        let Some(p) = lag(|l| l.line.pop(now)) else { break };
        deliver(game, p);
    }
    n
}

// The send hook, on the packet that would go out (after Sunset's other changes to it): true if Fake Lag took it (held
// it, or sent it itself behind what it held).
pub(super) fn held(game: &Game<'_>, packet: &S) -> bool {
    let now = Instant::now();
    if DELIVERING.with(Cell::get) > 0 {
        lag(|l| {
            let k = kind(packet, l.actions, l.mode == "Repel");
            l.line.push(packet.clone(), k, now, now);
        });
        return true;
    }
    let session = game.remote.as_ref().map(|r| &r.session);
    // Repel: a hit on a target whose hurt time is no longer than Delay starts a hold, and waits in it itself.
    let began = match packet {
        S::Attack(a) if lag(|l| l.mode == "Repel" && l.repel.is_none() && l.told.is_some_and(|t| now - t < TOLD)) => {
            let hurt = game.mobs.iter().find(|m| m.id as i32 == a.id).map_or(0, |m| m.hurt_time);
            lag(|l| {
                let ticks = l.delay.as_millis().div_ceil(50) as i32;
                let begins = hurt > 0 && hurt as i32 <= ticks;
                if begins {
                    l.roll ^= l.roll << 13;
                    l.roll ^= l.roll >> 7;
                    l.roll ^= l.roll << 17;
                    let until = now + l.delay + Duration::from_millis(l.roll % 100);
                    l.repel = Some((until, a.id));
                    l.line.hold(Plan::Until(until));
                    note(l, "repel: holding a hit", 0);
                }
                begins
            })
        }
        _ => false,
    };
    // The ordinary path: one lock, nothing held, the packet goes.
    let idle = lag(|l| {
        bind(l, session);
        // Sunset's tick stopped coming: hold nothing more, let what is held drain.
        if l.line.phase() == Phase::Holding && l.told.is_none_or(|t| now - t >= TOLD) {
            l.line.stop();
        }
        l.line.phase() == Phase::Idle
    });
    if idle {
        return false;
    }
    release(game, now);
    let decision = lag(|l| {
        let k = if began { Kind::Other } else { kind(packet, l.actions, l.mode == "Repel") };
        let d = l.line.decide(k, now);
        match d {
            Decision::Pass => l.line.passed(k),
            Decision::Hold(due) => l.line.push(packet.clone(), k, now, due),
            Decision::Flush(_) => l.line.push(packet.clone(), k, now, now),
        }
        d
    });
    match decision {
        Decision::Pass => false,
        Decision::Hold(_) => {
            release(game, now);
            true
        }
        Decision::Flush(why) => {
            if why == Flush::Overflow {
                lag(|l| {
                    let n = l.line.len();
                    note(l, "overflow: sent everything held", n);
                });
            }
            flush(game, why);
            true
        }
    }
}

// Every packet that went on the wire (sunset_combat_wire::delivered): where the server now has you.
pub(super) fn sent(_game: &Game<'_>, packet: &S) {
    if let Some(at) = position(packet) {
        lag(|l| l.sent_at = Some(at));
    }
}

pub(super) fn pending() -> bool {
    lag(|l| !l.line.is_empty())
}

// The server pushed you (a velocity packet the game took): Knockback's choke starts.
pub(super) fn knocked() {
    lag(|l| l.knocked = true);
}

// A position packet from the server (remote_teleport's head, before the game answers it): what is held goes first
// (the server ignores moves until the client accepts), and the answer goes undelayed.
pub(super) fn correction(game: &Game<'_>) {
    let session = game.remote.as_ref().map(|r| &r.session);
    let busy = lag(|l| {
        bind(l, session);
        l.line.phase() != Phase::Idle
    });
    if !busy {
        return;
    }
    let n = flush(game, Flush::Reanchor);
    lag(|l| {
        l.line.reset();
        l.pulse.reset();
        note(l, "teleport: sent everything held", n);
    });
}

// Panic: everything held goes now, in order, and nothing more is held. (Thrown away, the Pongs among it would leave
// the server waiting on answers it never gets, and the ticks it already counted on would be missing.)
pub(super) fn shutdown(game: &mut Game<'_>) {
    let session = game.remote.as_ref().map(|r| &r.session);
    lag(|l| bind(l, session));
    let n = flush(game, Flush::Stopped);
    lag(|l| {
        if n > 0 {
            note(l, "Sunset stopped: sent everything held", n);
        }
        l.line.clear_keeping_stats();
        l.told = None;
        l.pulse.reset();
        l.dynamic = Dynamic::default();
        l.knocked = false;
    });
    super::remote::sunset_gui_move_remote::retry_idle(game);
}

fn rolled(lo: f64, hi: f64) -> Duration {
    let r = lag(|l| {
        l.roll = (l.roll ^ 0x9E37_79B9_7F4A_7C15) | 1;
        l.roll ^= l.roll << 13;
        l.roll ^= l.roll >> 7;
        l.roll ^= l.roll << 17;
        (l.roll >> 11) as f64 / (1u64 << 53) as f64
    });
    let (lo, hi) = (lo.min(hi).max(0.0), lo.max(hi).max(0.0));
    Duration::from_millis((lo + r * (hi - lo)) as u64)
}

fn live(game: &Game<'_>) -> bool {
    game.remote.is_some()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
}

// Another module owns the outgoing stream this tick: Fake Lag hands it over empty.
fn yielding(mods: &[Module]) -> Option<&'static str> {
    if hold_of(mods).is_some() {
        return Some("Hold");
    }
    if let Some(owner) = super::sunset_combat_control::owner() {
        return Some(owner);
    }
    super::sunset_gui_move::stopping().then_some("GUI Move")
}

// A player who is not a friend within range; with `facing`, only one you look at (30 degrees) and walk towards.
pub(super) fn enemy(game: &Game<'_>, range: f64, facing: bool) -> bool {
    let (Some(p), Some(eye), Some(me)) = (game.player.as_ref(), game.eye_position(), game.remote.as_ref().map(|r| r.entity_id as u32)) else {
        return false;
    };
    let friends = sunset::with(|s| s.friends.clone());
    let (yaw, pitch) = (f64::from(game.yaw).to_radians(), f64::from(game.pitch).to_radians());
    let look = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    game.mobs.iter().any(|m| {
        if m.removed || !m.alive() || m.kind.id != "minecraft:player" || m.id == me {
            return false;
        }
        if game.remote_player_name(m.id).is_none_or(|n| friends.iter().any(|f| f.eq_ignore_ascii_case(n))) {
            return false;
        }
        let d = [m.position.x - p.position.x, m.position.y - p.position.y, m.position.z - p.position.z];
        if (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() > range {
            return false;
        }
        if !facing {
            return true;
        }
        let b = m.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let to = [(lo.x + hi.x) / 2.0 - eye[0], (lo.y + hi.y) / 2.0 - eye[1], (lo.z + hi.z) / 2.0 - eye[2]];
        let len = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt().max(1e-9);
        let dot = (0..3).map(|i| look[i] * to[i] / len).sum::<f64>();
        dot >= 30f64.to_radians().cos() && (p.velocity.x * to[0] + p.velocity.z * to[2]) / len > 0.02
    })
}

fn nearest_enemy(game: &Game<'_>) -> Option<f64> {
    let me = super::sunset_util::player_pos(game)?;
    game.mobs
        .iter()
        .filter(|m| super::sunset_util::player_enemy(game, m))
        .map(|m| super::sunset_util::dist(me, [m.position.x, m.position.y, m.position.z]))
        .min_by(f64::total_cmp)
}

// When this tick's mode wants the line to hold, and how.
// Fake Lag on in its Hold mode: what Blink was. The game's moves are held elsewhere (runtime_packets.rs), so the
// modules that asked whether Blink is on ask this (State::is_on answers "Blink" with it).
pub(super) fn hold_of(mods: &[Module]) -> Option<&Module> {
    mods.iter().find(|m| m.name == LAG && m.on && m.p("Mode") == "Hold")
}

fn plan(game: &Game<'_>, m: &Module, now: Instant) -> Option<Plan> {
    let delay = Plan::Latency(Duration::from_millis(m.n("Delay (ms)").clamp(10.0, 1000.0) as u64));
    let gap = Duration::from_millis(m.n("Pulse gap (ms)").max(0.0) as u64);
    let choke = || rolled(m.n("Shortest choke (ms)"), m.n("Longest choke (ms)"));
    match m.p("Mode") {
        "Pulse" => {
            let mut pulse = lag(|l| l.pulse);
            let plan = pulse.step(now, true, choke, gap);
            lag(|l| l.pulse = pulse);
            plan
        }
        "Knockback" => {
            let (mut pulse, knocked) = lag(|l| (l.pulse, std::mem::take(&mut l.knocked)));
            let plan = pulse.step(now, knocked, choke, gap);
            lag(|l| l.pulse = pulse);
            plan
        }
        "Dynamic" => {
            let range = m.n("Range");
            let near = if m.p("Trigger") == "Closing" {
                let d = nearest_enemy(game).filter(|d| *d <= range);
                let was = lag(|l| std::mem::replace(&mut l.dynamic.enemy_was, d));
                d.is_some_and(|d| was.is_none_or(|w| d <= w + 0.01))
            } else {
                enemy(game, range, true)
            };
            lag(|l| l.dynamic.gate.step(near, EXIT_TICKS)).then_some(delay)
        }
        // Repel: nothing waits until a hit begins a hold (held), which lasts until its time, or until the target's
        // hurt time has all but run out (the hit then lands as it can be hurt again).
        "Repel" => {
            let Some((until, id)) = lag(|l| l.repel) else {
                return None;
            };
            let hurt = game.mobs.iter().find(|x| x.id as i32 == id).map(|x| x.hurt_time as i32);
            if now >= until || hurt.is_none_or(|h| h <= 1) {
                lag(|l| l.repel = None);
                return None;
            }
            Some(Plan::Until(until))
        }
        _ => Some(delay),
    }
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let now = Instant::now();
    let session = game.remote.as_ref().map(|r| &r.session);
    lag(|l| bind(l, session));
    // A new life or world in the same session: what is held goes first (the server takes no move until the client has
    // accepted where it now is), and the new life starts with nothing held.
    let life = game.remote.as_ref().map(|r| (r.entity_id, game.dim));
    let reanchor = lag(|l| {
        let changed = l.life.is_some() && life.is_some() && l.life != life;
        l.life = life;
        changed && l.line.phase() != Phase::Idle
    });
    if reanchor {
        let n = flush(game, Flush::Reanchor);
        lag(|l| {
            l.line.reset();
            l.pulse.reset();
            note(l, "new life or dimension: sent everything held", n);
        });
    }
    let m = mods.iter().find(|m| m.name == LAG && m.on);
    // Hold: its breadcrumbs while moves are held, and what the server sends held too with Bi-directional.
    let holding = hold_of(mods).filter(|_| sunset::BLINK_HELD.load(std::sync::atomic::Ordering::Relaxed));
    match (holding, game.player.as_ref()) {
        (Some(hold), Some(p)) => {
            let at = [p.position.x, p.position.y, p.position.z];
            lag(|l| {
                if hold.b("Breadcrumbs") && l.crumbs.len() < 2000 && l.crumbs.last() != Some(&at) {
                    l.crumbs.push(at);
                }
            });
            if hold.p("Direction") == "Bi-directional" && live(game) {
                crate::game::sunset_packets::stall();
            } else {
                crate::game::sunset_packets::stall_off();
            }
        }
        _ => {
            lag(|l| l.crumbs.clear());
            crate::game::sunset_packets::stall_off();
        }
    }
    if m.is_none_or(|m| m.p("Mode") != "Repel") {
        lag(|l| l.repel = None);
    }
    if let Some(who) = yielding(mods) {
        if pending() {
            let n = flush(game, Flush::Yield);
            lag(|l| note(l, format!("{who} took the outgoing stream: sent everything held"), n));
        }
        lag(|l| {
            l.line.stop();
            l.pulse.reset();
            l.dynamic = Dynamic::default();
            l.knocked = false;
        });
    } else if let Some(m) = m.filter(|_| live(game)) {
        let plan = plan(game, m, now);
        lag(|l| match plan {
            Some(plan) => l.line.hold(plan),
            None => l.line.stop(),
        });
    } else {
        lag(|l| {
            l.line.stop();
            l.pulse.reset();
            l.dynamic = Dynamic::default();
            l.knocked = false;
        });
    }
    lag(|l| {
        l.told = Some(now);
        l.actions = m.is_none_or(|m| m.b("Release on action"));
        l.mode = m.map_or("", |m| m.p("Mode"));
        l.delay = Duration::from_millis(m.map_or(100.0, |m| m.n("Delay (ms)")).clamp(10.0, 1000.0) as u64);
    });
    release(game, now);
}

// Show server position: where the server has you while Fake Lag holds what you send.
pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    // Hold's Breadcrumbs: the way you went since it began to hold, the first stretch yellow and the rest red.
    if hold_of(mods).is_some_and(|m| m.b("Breadcrumbs")) {
        let crumbs = lag(|l| l.crumbs.clone());
        for (i, pair) in crumbs.windows(2).enumerate() {
            if let (Some(a), Some(b)) = (cam.project(pair[0]), cam.project(pair[1])) {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.01 && len < 4000.0 {
                    let (nx, ny) = (-dy / len * lw, dx / len * lw);
                    let colour = if i == 0 { 0xFFFF_E050 } else { 0xFFFF_5555 };
                    out.quad([[a[0] + nx, a[1] + ny], [b[0] + nx, b[1] + ny], [b[0] - nx, b[1] - ny], [a[0] - nx, a[1] - ny]], colour);
                }
            }
        }
    }
    if !mods.iter().any(|m| m.name == LAG && m.on && m.b("Show server position")) {
        return;
    }
    let (Some(at), Some(p)) = (lag(|l| l.sent_at.filter(|_| !l.line.is_empty())), game.player.as_ref()) else {
        return;
    };
    let half = f64::from(p.dimensions.width) / 2.0;
    let lo = [at[0] - half, at[1], at[2] - half];
    let hi = [at[0] + half, at[1] + f64::from(p.dimensions.height), at[2] + half];
    super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED, lo, hi, 0xFFFF_5555, lw);
}

// ---------------------------------------------------------------- Debug Report

pub(super) fn report(out: &mut String) {
    let now = Instant::now();
    lag(|l| {
        let s = &l.line.stats;
        let _ = writeln!(out, "\n## Fake Lag (outgoing)");
        let plan = match l.line.plan() {
            Some(Plan::Latency(d)) => format!("each packet {} ms late", d.as_millis()),
            Some(Plan::Until(at)) => format!("choke ends in {} ms", at.saturating_duration_since(now).as_millis()),
            None => "not holding".into(),
        };
        let mode = if l.mode.is_empty() { "module off" } else { l.mode };
        let pulse = if l.pulse.choking() { "; a choke running" } else { "" };
        let _ = writeln!(out, "- State: {} ({mode}; {plan}{pulse}); release on action {}", l.line.phase().name(), if l.actions { "on" } else { "off" });
        let link = l.link.as_ref().map_or("none".to_owned(), |k| format!("session #{} epoch {}", k.serial, k.epoch));
        let _ = writeln!(out, "- Held now: {} packets, oldest {} ms; bound to {link}", l.line.len(), l.line.oldest(now).as_millis());
        let _ = writeln!(out, "- Since start: {} held, {} released; deepest {} packets, longest wait {} ms", s.captured, s.released, s.max_depth, s.max_wait_ms);
        let flushes: Vec<String> =
            line::FLUSHES.iter().zip(s.flushes).filter(|(_, n)| *n > 0).map(|(f, n)| format!("{} {n}", f.name())).collect();
        let _ = writeln!(out, "- Sent at once (times, by reason): {}", if flushes.is_empty() { "none".into() } else { flushes.join(", ") });
        let discards: Vec<String> =
            line::DISCARDS.iter().zip(s.discarded).filter(|(_, n)| *n > 0).map(|(d, n)| format!("{} {n}", d.name())).collect();
        let _ = writeln!(out, "- Thrown away (packets, by reason): {}", if discards.is_empty() { "none".into() } else { discards.join(", ") });
        if s.reordered > 0 {
            let _ = writeln!(out, "- **Reordered by Fake Lag: {} (a bug: it must keep the order)**", s.reordered);
        }
        if !l.notes.is_empty() {
            let _ = writeln!(out, "\n| Time | Tick | Event | Packets |\n|---|---|---|---|");
            for n in l.notes.iter().rev() {
                let at = chrono::DateTime::<chrono::Local>::from(n.at).format("%H:%M:%S%.3f");
                let _ = writeln!(out, "| {at} | {} | {} | {} |", n.tick, n.what, n.packets);
            }
        }
    });
}

// ---------------------------------------------------------------- old settings

// Fake Lag before its line (and Backtrack inside it): Always is Delay, Timed is Pulse, Ranged is Dynamic closing in
// (with its Range choke as the delay), Off was Backtrack alone (Fake Lag off, its key Backtrack's). Backtrack's
// settings are the Backtrack module's, on if Fake Lag was with it, and on the same key.
pub(super) fn migrated(text: Cow<'_, str>) -> Cow<'_, str> {
    let line_of = |l: &str| l.split_once('=').and_then(|(k, v)| k.strip_prefix("fakelag.").map(|f| (f.to_owned(), v.trim().to_owned())));
    let old_mode = |v: &str| matches!(v, "Always" | "Timed" | "Ranged" | "Off");
    let backtrack_field = |f: &str| f.starts_with("backtrack") || matches!(f, "shortestlatencyms" | "longestlatencyms");
    // Range choke was saved as `rangechoke` and, in builds between, `rangechokems`.
    let choke_field = |f: &str| matches!(f, "rangechoke" | "rangechokems");
    let old = text.lines().filter_map(line_of).any(|(f, v)| (f == "mode" && old_mode(&v)) || choke_field(&f) || backtrack_field(&f));
    if !old {
        return text;
    }
    let fields: Vec<(String, String)> = text.lines().filter_map(line_of).collect();
    let get = |f: &str| fields.iter().rev().find(|(k, _)| k == f).map(|(_, v)| v.as_str());
    let mode = get("mode").unwrap_or("Dynamic");
    let backtrack = get("backtrack") == Some("true");
    let mut out: Vec<String> = text
        .lines()
        .filter(|l| line_of(*l).is_none_or(|(f, _)| f != "mode" && !choke_field(&f) && !backtrack_field(&f)))
        .map(str::to_owned)
        .collect();
    let (new_mode, off) = match mode {
        "Always" => ("Delay", false),
        "Timed" => ("Pulse", false),
        "Ranged" => ("Dynamic", false),
        "Off" => ("Delay", true),
        other => (other, false),
    };
    out.push(format!("fakelag.mode={new_mode}"));
    if mode == "Ranged" {
        out.push("fakelag.trigger=Closing".into());
        if let Some(choke) = get("rangechoke").or_else(|| get("rangechokems")) {
            out.push(format!("fakelag.delayms={choke}"));
        }
    }
    if off {
        out.push("fakelag.on=false".into());
    }
    if backtrack && !text.lines().any(|l| l.starts_with("backtrack.")) {
        // A profile that leaves Fake Lag's switch alone leaves Backtrack's too.
        if let Some(on) = get("on") {
            out.push(format!("backtrack.on={on}"));
        }
        if let Some(key) = get("key").filter(|k| *k != "none") {
            out.push(format!("backtrack.key={key}"));
            if let Some(bind) = get("bindmode") {
                out.push(format!("backtrack.bindmode={bind}"));
            }
            if off {
                out.push("fakelag.key=none".into());
            }
        }
        for (old, new) in [
            ("backtrackminrange", "minrange"),
            ("backtrackmaxrange", "maxrange"),
            ("shortestlatencyms", "shortestlatencyms"),
            ("longestlatencyms", "longestlatencyms"),
            ("backtrackweapononly", "weapononly"),
            ("showserverposition", "showserverposition"),
        ] {
            if let Some(v) = get(old) {
                out.push(format!("backtrack.{new}={v}"));
            }
        }
    }
    Cow::Owned(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::migrated;
    use std::borrow::Cow;
    use std::collections::HashMap;

    fn fields(text: &str) -> HashMap<String, String> {
        migrated(Cow::Borrowed(text)).lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.into(), v.into())).collect()
    }

    #[test]
    fn current_settings_are_left_alone() {
        let text = "fakelag.on=true\nfakelag.mode=Pulse\nfakelag.delayms=200\n";
        assert!(matches!(migrated(Cow::Borrowed(text)), Cow::Borrowed(_)));
    }

    #[test]
    fn old_modes_become_the_new_ones() {
        for (old, new) in [("Always", "Delay"), ("Timed", "Pulse"), ("Dynamic", "Dynamic"), ("Knockback", "Knockback")] {
            let out = fields(&format!("fakelag.on=true\nfakelag.mode={old}\nfakelag.rangechoke=600\n"));
            assert_eq!(out["fakelag.mode"], new, "{old}");
            assert_eq!(out["fakelag.on"], "true");
            assert!(!out.contains_key("fakelag.rangechoke"));
        }
        let out = fields("fakelag.mode=Dynamic\nfakelag.rangechokems=600\nfakelag.delayms=100\n");
        assert!(!out.contains_key("fakelag.rangechokems") && out["fakelag.delayms"] == "100", "{out:?}");
        let out = fields("fakelag.mode=Ranged\nfakelag.rangechoke=450\nfakelag.delayms=100\n");
        assert_eq!((out["fakelag.mode"].as_str(), out["fakelag.trigger"].as_str(), out["fakelag.delayms"].as_str()), ("Dynamic", "Closing", "450"));
    }

    #[test]
    fn backtrack_moves_to_its_own_module() {
        let out = fields(
            "fakelag.on=true\nfakelag.key=key.keyboard.g\nfakelag.mode=Off\nfakelag.backtrack=true\nfakelag.backtrackmaxrange=7\n\
             fakelag.shortestlatencyms=200\nfakelag.backtrackweapononly=true\nfakelag.showserverposition=false\n",
        );
        assert_eq!(out["fakelag.on"], "false");
        assert_eq!(out["fakelag.key"], "none");
        assert_eq!(out["backtrack.on"], "true");
        assert_eq!(out["backtrack.key"], "key.keyboard.g");
        assert_eq!(out["backtrack.maxrange"], "7");
        assert_eq!(out["backtrack.shortestlatencyms"], "200");
        assert_eq!(out["backtrack.weapononly"], "true");
        assert_eq!(out["backtrack.showserverposition"], "false");
        assert!(!out.keys().any(|k| k.starts_with("fakelag.backtrack")));
        // With a mode as well, both stay on one key.
        let out = fields("fakelag.on=true\nfakelag.key=key.keyboard.g\nfakelag.mode=Always\nfakelag.backtrack=true\n");
        assert_eq!((out["fakelag.mode"].as_str(), out["fakelag.key"].as_str(), out["backtrack.key"].as_str()), ("Delay", "key.keyboard.g", "key.keyboard.g"));
        // Backtrack off: nothing of it is carried over.
        let out = fields("fakelag.on=true\nfakelag.mode=Dynamic\nfakelag.backtrack=false\nfakelag.backtrackminrange=2\n");
        assert!(!out.keys().any(|k| k.starts_with("backtrack.")));
    }
}
