// Fake Lag and Knockback Delay, from Rust's PR #16. Fake Lag holds what you send (moves, the end of each client
// tick, sprint, swings...) and lets it go in order after the delay; an action (hit, use, click, chat) sends what is
// held first and goes straight out. Knockback Delay holds the server's pushes to you (your velocity, explosions) in
// the network module (crate::net::sunset_packets) and lets the game take them after the delay.
use super::sunset::{self, Camera, Canvas, Category, Module};
use super::{Game, Screen};
use horizon_client::net::sunset_packets as net;
use horizon_protocol::packets::play::Serverbound as S;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LAG: &str = "Fake Lag";
const KB: &str = "Knockback Delay";
// Held for a second at most, 128 packets at most: past either, all of it goes.
const MOST: usize = 128;
const OLDEST: Duration = Duration::from_secs(1);

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(LAG, Category::Combat, "Holds back what you send for a while, then sends it in order: the server sees you late. Hits, using items and chat go at once (with what was held before them). Dynamic: only while you look at and walk towards a player in Range.")
            .pick("Mode", &["Dynamic", "Always"])
            .num("Delay (ms)", 100.0, 10.0, 1000.0, 10.0)
            .num("Range", 6.0, 3.0, 12.0, 0.5)
            .bool("Show server position", true),
        Module::new(KB, Category::Combat, "Takes the server's pushes on you (hits, explosions) a little late, by chance: in the air after Air delay, on the ground after Ground delay. Other players' movement comes in on time.")
            .num("Chance (%)", 40.0, 0.0, 100.0, 5.0)
            .num("Air delay min (ms)", 50.0, 0.0, 500.0, 10.0)
            .num("Air delay max (ms)", 100.0, 0.0, 500.0, 10.0)
            .num("Ground delay min (ms)", 200.0, 0.0, 1000.0, 10.0)
            .num("Ground delay max (ms)", 250.0, 0.0, 1000.0, 10.0)
            .bool("Not in water", false)
            .bool("Only near enemies", true),
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

fn send(game: &Game<'_>, packets: Vec<S>) {
    let Some(r) = &game.remote else {
        return;
    };
    if let Some(at) = packets.iter().rev().find_map(position) {
        lag(|l| l.sent_at = Some(at));
    }
    for packet in packets {
        super::sunset_combat_wire::forwarded(game, &packet);
        r.session.send(packet);
    }
}

// Lets go of what is due (or all of it).
fn release(game: &Game<'_>, all: bool) {
    let now = Instant::now();
    let due = lag(|l| {
        let n = l.held.iter().take_while(|h| all || h.due <= now || now - h.born >= OLDEST).count();
        l.held.drain(..n).map(|h| h.packet).collect()
    });
    send(game, due);
}

// The send hook, on the packet that would go out (after Sunset's other changes to it): true if Fake Lag holds it.
pub(super) fn held(game: &Game<'_>, packet: &S) -> bool {
    let on = lag(|l| l.on.filter(|(_, at)| at.elapsed() < Duration::from_millis(250)).map(|(d, _)| d));
    let full = lag(|l| l.held.len() >= MOST);
    let Some(delay) = on.filter(|_| !action(packet) && !full) else {
        release(game, true);
        if let Some(at) = position(packet) {
            lag(|l| l.sent_at = Some(at));
        }
        return false;
    };
    release(game, false);
    lag(|l| {
        let now = Instant::now();
        // Each packet's turn comes after the one before it.
        let due = l.held.back().map_or(now + delay, |b| b.due.max(now + delay));
        l.held.push_back(Held { packet: packet.clone(), born: now, due });
    });
    true
}

// A teleport from the server: held positions are stale; the rest (each tick's end) still goes, before the answer.
pub(super) fn correction(game: &Game<'_>) {
    let kept = lag(|l| {
        l.held.drain(..).map(|h| h.packet).filter(|p| !motion(p)).collect()
    });
    send(game, kept);
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
    let on = get(LAG).filter(|m| live && !paused && (m.p("Mode") == "Always" || enemy(game, m.n("Range"), true)));
    lag(|l| l.on = on.map(|m| (Duration::from_millis(m.n("Delay (ms)").clamp(10.0, 1000.0) as u64), Instant::now())));
    release(game, on.is_none());
    // Knockback Delay: what the network module holds by for the next pushes.
    let kb = get(KB).filter(|m| {
        live && (!m.b("Only near enemies") || enemy(game, 6.0, false))
            && !(m.b("Not in water") && game.player.as_ref().is_some_and(|p| p.fluids.was_touching_water))
    });
    match (kb, game.remote.as_ref()) {
        (Some(m), Some(r)) => {
            let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
            let (a, b) = if ground {
                (m.n("Ground delay min (ms)"), m.n("Ground delay max (ms)"))
            } else {
                (m.n("Air delay min (ms)"), m.n("Air delay max (ms)"))
            };
            net::delay(r.entity_id, m.n("Chance (%)"), [a.min(b) as u64, a.max(b) as u64]);
        }
        _ => net::delay_off(),
    }
}

// Show server position: where the server has you while Fake Lag holds your moves.
pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    if !mods.iter().any(|m| m.name == LAG && m.on && m.b("Show server position")) {
        return;
    }
    let (Some(at), Some(p)) = (lag(|l| l.sent_at.filter(|_| !l.held.is_empty())), game.player.as_ref()) else {
        return;
    };
    let half = f64::from(p.dimensions.width) / 2.0;
    let lo = [at[0] - half, at[1], at[2] - half];
    let hi = [at[0] + half, at[1] + f64::from(p.dimensions.height), at[2] + half];
    super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED, lo, hi, 0xFFFF_5555, lw);
}
