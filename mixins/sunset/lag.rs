// Fake Lag and Knockback Delay, from Rust's PR #16. Fake Lag holds what you send (moves, the end of each client
// tick, sprint, swings...) and lets it go in order after the delay (Dynamic, Always), or all of it at the end of a
// choke (Timed, Ranged, Knockback); an action (hit, use, click, chat) sends what is held first and goes straight out.
// What goes out at once never carries two moves in a tick: the held moves become one, to where the last one had you,
// and it waits for the next tick if the tick sent one already. Its Backtrack holds what the server sends while the
// player you hit moves away from you (crate::net::sunset_packets). Knockback Delay holds the server's pushes to you
// (your velocity, explosions) in the network module and lets the game take them after the delay.
use super::sunset::{self, Camera, Canvas, Category, Module};
use super::{Game, Screen};
use horizon_client::net::sunset_packets as net;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LAG: &str = "Fake Lag";
// Held for a second at most, 128 packets at most: past either, all of it goes.
const MOST: usize = 128;
const OLDEST: Duration = Duration::from_secs(1);

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(LAG, Category::Combat, "Delays what you send.")
            .pick("Mode", &["Dynamic", "Always", "Timed", "Ranged", "Knockback", "Off"])
            .num("Delay (ms)", 100.0, 10.0, 1000.0, 10.0)
            .num("Range", 6.0, 3.0, 12.0, 0.5)
            .num("Shortest choke (ms)", 100.0, 0.0, 1000.0, 10.0)
            .num("Longest choke (ms)", 150.0, 0.0, 1000.0, 10.0)
            .num("Range choke (ms)", 600.0, 50.0, 1000.0, 10.0)
            .bool("Show server position", true)
            .bool("Backtrack", false)
            .num("Backtrack min range", 1.0, 0.0, 10.0, 0.5)
            .num("Backtrack max range", 5.0, 0.0, 10.0, 0.5)
            .num("Shortest latency (ms)", 100.0, 0.0, 1500.0, 10.0)
            .num("Longest latency (ms)", 150.0, 0.0, 1500.0, 10.0)
            .bool("Backtrack weapon only", false),
    ]
}

struct Held {
    packet: S,
    born: Instant,
    due: Instant,
}

#[derive(Default)]
struct Lag {
    held: VecDeque<Held>,
    // Holding now for how long, as of the last tick (a tick that stops coming, Sunset off, holds nothing).
    on: Option<(Duration, Instant)>,
    // Where the server has you: the last position that went out.
    sent_at: Option<[f64; 3]>,
    // The life the held moves belong to: entity id and dimension.
    life: Option<(i32, super::Dim)>,
    // The choke modes: everything waits until then, and goes together.
    choke_until: Option<Instant>,
    // Ranged: how far the nearest enemy was last tick.
    enemy_was: Option<f64>,
    // Knockback: the server pushed you since the last tick.
    knocked: bool,
    // The tick a move went on the wire.
    moved_tick: Option<u64>,
    // Backtrack: the player you last hit.
    last_hit: Option<u32>,
    roll: u64,
}

static LAG_STATE: Mutex<Option<Lag>> = Mutex::new(None);

fn lag<R>(f: impl FnOnce(&mut Lag) -> R) -> R {
    f(LAG_STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Lag::default))
}

// Packets that do something: they never wait, and what was held goes before them.
fn action(packet: &S) -> bool {
    matches!(
        packet,
        S::Attack(_) | S::Interact(_) | S::UseItemOn(_) | S::UseItem(_) | S::PlayerAction(_) | S::Punch(_)
            | S::ContainerClick(_) | S::ContainerButtonClick(_) | S::ContainerClose(_) | S::Chat(_)
            | S::ChatCommand(_) | S::ChatCommandSigned(_) | S::SignUpdate(_) | S::EditBook(_) | S::PlaceRecipe(_)
            | S::SelectTrade(_) | S::SetCreativeModeSlot(_) | S::PickItemFromBlock(_) | S::PickItemFromEntity(_)
            | S::AcceptTeleportation(_) | S::ConfigurationAcknowledged(_) | S::Unknown(..)
    )
}

// A position the server has to take as given: no good after a teleport, a respawn or a new world.
fn motion(packet: &S) -> bool {
    matches!(packet, S::MovePlayerPos(_) | S::MovePlayerPosRot(_) | S::MovePlayerRot(_) | S::MovePlayerStatus(_) | S::MoveVehicle(_))
}

fn position(packet: &S) -> Option<[f64; 3]> {
    match packet {
        S::MovePlayerPos(p) => Some([p.x, p.y, p.z]),
        S::MovePlayerPosRot(p) => Some([p.x, p.y, p.z]),
        _ => None,
    }
}

fn look(packet: &S) -> Option<(f32, f32)> {
    match packet {
        S::MovePlayerRot(p) => Some((p.yaw, p.pitch)),
        S::MovePlayerPosRot(p) => Some((p.yaw, p.pitch)),
        _ => None,
    }
}

fn flags(packet: &S) -> u8 {
    match packet {
        S::MovePlayerPos(p) => p.flags,
        S::MovePlayerPosRot(p) => p.flags,
        S::MovePlayerRot(p) => p.flags,
        S::MovePlayerStatus(p) => p.flags,
        _ => 0,
    }
}

fn player_move(packet: &S) -> bool {
    motion(packet) && !matches!(packet, S::MoveVehicle(_))
}

// The player's moves among `packets` as one, where the last of them was, with the last look among them and the last
// one's flags; the vehicle's moves as their last. Each goes in the place of the last of its kind.
fn merged(packets: Vec<S>) -> Vec<S> {
    let last_move = packets.iter().rposition(player_move);
    let last_vehicle = packets.iter().rposition(|p| matches!(p, S::MoveVehicle(_)));
    let at = packets.iter().rev().find_map(position);
    let turn = packets.iter().rev().find_map(look);
    let mut out = Vec::with_capacity(packets.len());
    for (i, packet) in packets.into_iter().enumerate() {
        if player_move(&packet) {
            if Some(i) == last_move {
                let flags = flags(&packet);
                out.push(match (at, turn) {
                    (Some([x, y, z]), Some((yaw, pitch))) => S::MovePlayerPosRot(play::MovePlayerPosRot { x, y, z, yaw, pitch, flags }),
                    (Some([x, y, z]), None) => S::MovePlayerPos(play::MovePlayerPos { x, y, z, flags }),
                    (None, Some((yaw, pitch))) => S::MovePlayerRot(play::MovePlayerRot { yaw, pitch, flags }),
                    (None, None) => S::MovePlayerStatus(play::MovePlayerStatus { flags }),
                });
            }
        } else if !matches!(packet, S::MoveVehicle(_)) || Some(i) == last_vehicle {
            out.push(packet);
        }
    }
    out
}

fn send(game: &Game<'_>, packets: Vec<S>) {
    let Some(r) = &game.remote else {
        return;
    };
    if let Some(at) = packets.iter().rev().find_map(position) {
        lag(|l| l.sent_at = Some(at));
    }
    for packet in packets {
        if motion(&packet) {
            lag(|l| l.moved_tick = Some(game.ticks));
        }
        super::sunset_combat_wire::forwarded(game, &packet);
        r.session.send(packet);
    }
}

// Lets go of what is due (or all of it), its moves as one (none with `superseded`: a move goes right after). A tick
// that sent a move already keeps that one back for the next.
fn release(game: &Game<'_>, all: bool, superseded: bool) {
    let now = Instant::now();
    let due: Vec<S> = lag(|l| {
        let n = l.held.iter().take_while(|h| all || h.due <= now || now - h.born >= OLDEST).count();
        l.held.drain(..n).map(|h| h.packet).collect()
    });
    let due = if superseded { due.into_iter().filter(|p| !motion(p)).collect() } else { merged(due) };
    let moved = lag(|l| l.moved_tick == Some(game.ticks));
    let (moves, rest): (Vec<S>, Vec<S>) = due.into_iter().partition(|p| moved && motion(p));
    lag(|l| {
        for packet in moves.into_iter().rev() {
            l.held.push_front(Held { packet, born: now, due: now });
        }
    });
    send(game, rest);
}

pub(super) fn pending() -> bool {
    lag(|l| !l.held.is_empty())
}

// The send hook, on the packet that would go out (after Sunset's other changes to it): true if Fake Lag holds it.
pub(super) fn held(game: &Game<'_>, packet: &S) -> bool {
    if let S::Attack(p) = packet
        && p.id >= 0
    {
        lag(|l| l.last_hit = Some(p.id as u32));
    }
    let on = lag(|l| l.on.filter(|(_, at)| at.elapsed() < Duration::from_millis(250)).map(|(d, _)| d));
    let choke = lag(|l| l.choke_until.filter(|_| on.is_some()));
    let full = lag(|l| l.held.len() >= MOST);
    let now = Instant::now();
    // A move after one went this tick (what was held, let go by an action): it waits for the next tick.
    if motion(packet) && lag(|l| l.moved_tick == Some(game.ticks)) {
        lag(|l| l.held.push_back(Held { packet: packet.clone(), born: now, due: now }));
        return true;
    }
    let Some(delay) = on.filter(|_| !action(packet) && !full) else {
        release(game, true, motion(packet));
        if let Some(at) = position(packet) {
            lag(|l| l.sent_at = Some(at));
        }
        if motion(packet) {
            lag(|l| l.moved_tick = Some(game.ticks));
        }
        return false;
    };
    release(game, false, false);
    lag(|l| {
        // Each packet's turn comes after the one before it; in a choke, all at its end.
        let due = match choke {
            Some(end) => end.max(now),
            None => l.held.back().map_or(now + delay, |b| b.due.max(now + delay)),
        };
        l.held.push_back(Held { packet: packet.clone(), born: now, due });
    });
    true
}

// The server pushed you (a velocity packet): Knockback's choke starts.
pub(super) fn knocked() {
    lag(|l| l.knocked = true);
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

// A teleport from the server: held positions are stale; the rest (each tick's end) still goes, before the answer.
pub(super) fn correction(game: &Game<'_>) {
    let kept = lag(|l| {
        l.held.drain(..).map(|h| h.packet).filter(|p| !motion(p)).collect()
    });
    send(game, kept);
    lag(|l| l.choke_until = None);
}

fn live(game: &Game<'_>) -> bool {
    game.remote.is_some()
        && game.screen == Screen::Playing
        && !sunset::gui_open()
        && !game.spectator()
        && game.player.as_ref().is_some_and(|p| p.health.health() > 0.0)
}

// A player who is not a friend within range; with `facing`, only one you look at (30 degrees) and walk towards.
fn enemy(game: &Game<'_>, range: f64, facing: bool) -> bool {
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

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module]) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let live = live(game);
    // A new life or world: the held moves are of the old one.
    let life = game.remote.as_ref().map(|r| (r.entity_id, game.dim));
    let stale = lag(|l| {
        let stale = l.life != life;
        l.life = life;
        if stale {
            l.held.drain(..).map(|h| h.packet).filter(|p| !motion(p)).collect()
        } else {
            Vec::new()
        }
    });
    send(game, stale);
    // Not while Blink holds the moves itself, nor while a combat module is in the middle of something.
    let paused = get("Blink").is_some() || super::sunset_combat_control::owner().is_some();
    let m = get(LAG).filter(|_| live && !paused);
    let on = m.filter(|m| match m.p("Mode") {
        "Always" => true,
        "Dynamic" => enemy(game, m.n("Range"), true),
        "Off" => false,
        _ => choking(game, m),
    });
    if on.is_none() {
        lag(|l| l.choke_until = None);
    }
    lag(|l| l.on = on.map(|m| (Duration::from_millis(m.n("Delay (ms)").clamp(10.0, 1000.0) as u64), Instant::now())));
    release(game, on.is_none(), false);
    backtrack(game, get(LAG).filter(|m| live && m.b("Backtrack")));
    // Knockback Delay: what the network module holds by for the next pushes.
    let kb = get("Velocity").filter(|m| {
        m.b("Knockback delay") && live && (!m.b("Delay only near enemies") || enemy(game, 6.0, false))
            && !(m.b("Delay not in water") && game.player.as_ref().is_some_and(|p| p.fluids.was_touching_water))
    });
    match (kb, game.remote.as_ref()) {
        (Some(m), Some(r)) => {
            let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
            let (a, b) = if ground {
                (m.n("Ground delay min (ms)"), m.n("Ground delay max (ms)"))
            } else {
                (m.n("Air delay min (ms)"), m.n("Air delay max (ms)"))
            };
            net::delay(r.entity_id, m.n("Delay chance (%)"), [a.min(b) as u64, a.max(b) as u64]);
        }
        _ => net::delay_off(),
    }
}

// The choke modes, each tick: whether to hold now. A choke that ran out lets everything go and the next one starts
// (Timed), while an enemy within Range comes closer for up to Range choke (Ranged), or once after each push on you
// (Knockback).
fn choking(game: &Game<'_>, m: &Module) -> bool {
    let now = Instant::now();
    let ended = lag(|l| l.choke_until.is_some_and(|end| now >= end));
    let knocked = lag(|l| std::mem::take(&mut l.knocked));
    let fresh = |d: Duration| lag(|l| l.choke_until = Some(now + d));
    let choke = || rolled(m.n("Shortest choke (ms)"), m.n("Longest choke (ms)"));
    if ended {
        release(game, true, false);
        lag(|l| l.choke_until = None);
    }
    match m.p("Mode") {
        "Timed" => {
            if lag(|l| l.choke_until.is_none()) {
                fresh(choke());
            }
            true
        }
        "Ranged" => {
            let near = nearest_enemy(game).filter(|d| *d <= m.n("Range"));
            let was = lag(|l| std::mem::replace(&mut l.enemy_was, near));
            let closing = near.is_some_and(|d| was.is_none_or(|w| d <= w + 0.01));
            if !closing {
                lag(|l| l.choke_until = None);
                return false;
            }
            if lag(|l| l.choke_until.is_none()) {
                fresh(Duration::from_millis(m.n("Range choke (ms)") as u64));
            }
            true
        }
        _ => {
            if knocked {
                fresh(choke());
            }
            lag(|l| l.choke_until.is_some())
        }
    }
}

fn nearest_enemy(game: &Game<'_>) -> Option<f64> {
    let me = super::sunset_util::player_pos(game)?;
    game.mobs
        .iter()
        .filter(|m| super::sunset_util::player_enemy(game, m))
        .map(|m| super::sunset_util::dist(me, [m.position.x, m.position.y, m.position.z]))
        .min_by(f64::total_cmp)
}

// Backtrack: the player you last hit, while within its range band (and with a weapon in hand, with Weapon only), is
// told to the network module each tick; it holds what comes in while they move away.
fn backtrack(game: &Game<'_>, m: Option<&Module>) {
    let target = m.and_then(|m| {
        if m.b("Backtrack weapon only") && !holds_weapon(game) {
            return None;
        }
        let id = lag(|l| l.last_hit)?;
        let mob = game.mobs.iter().find(|t| t.id == id && super::sunset_util::player_enemy(game, t))?;
        let at = [mob.position.x, mob.position.y, mob.position.z];
        let d = super::sunset_util::dist(super::sunset_util::player_pos(game)?, at);
        (d >= m.n("Backtrack min range") && d <= m.n("Backtrack max range")).then_some((m, mob.id, at))
    });
    match (target, game.remote.as_ref(), game.eye_position()) {
        (Some((m, id, at)), Some(r), Some(eye)) => {
            let (a, b) = (m.n("Shortest latency (ms)"), m.n("Longest latency (ms)"));
            net::backtrack(r.entity_id, id as i32, at, eye, [a.min(b) as u64, a.max(b) as u64]);
        }
        _ => net::backtrack_off(),
    }
}

fn holds_weapon(game: &Game<'_>) -> bool {
    game.held().is_some_and(|s| {
        &*s.item == "minecraft:mace"
            || ["minecraft:swords", "minecraft:axes"].iter().any(|t| game.res.items().item_tags.get(*t).is_some_and(|t| t.contains(&*s.item)))
    })
}

// Show server position: where the server has you while Fake Lag holds your moves, and where Backtrack's player really
// is while it holds what comes in.
pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    if !mods.iter().any(|m| m.name == LAG && m.on && m.b("Show server position")) {
        return;
    }
    if let Some(at) = net::backtrack_shown() {
        let lo = [at[0] - 0.3, at[1], at[2] - 0.3];
        let hi = [at[0] + 0.3, at[1] + 1.8, at[2] + 0.3];
        super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED, lo, hi, 0xFF55_AAFF, lw);
    }
    let (Some(at), Some(p)) = (lag(|l| l.sent_at.filter(|_| !l.held.is_empty())), game.player.as_ref()) else {
        return;
    };
    let half = f64::from(p.dimensions.width) / 2.0;
    let lo = [at[0] - half, at[1], at[2] - half];
    let hi = [at[0] + half, at[1] + f64::from(p.dimensions.height), at[2] + half];
    super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED, lo, hi, 0xFFFF_5555, lw);
}
