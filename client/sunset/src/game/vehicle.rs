// Entity Control (Sunset's, which takes in Boat Fly): the boat or the mount you drive goes where you look at the
// speed you set, flies up with Jump and down with Sprint, and holds its height otherwise. Horizon simulates a
// driven boat on the client and sends it once a tick (MoveVehicle), so the boat's own tick is where its motion is
// replaced. A mount is the same when your client is the one that moves it: a saddled horse, donkey, mule or camel
// you sit first on, a pig or strider you steer with its rod, a harnessed happy ghast. Any other animal is moved by
// the server and is left alone, and a mount that comes down from high up takes the fall the server counts.

use super::sunset::{Category, Module, with};
use super::Game;
use horizon_core::geometry::Vec3;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use winit::keyboard::KeyCode;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Entity Control", Category::Movement, "Drives and flies boats and saddled mounts.")
            .bool("Boats", true)
            .bool("Mounts", true)
            .bool("Lock yaw", true)
            .bool("Speed", true)
            .num("Horizontal speed", 10.0, 1.0, 50.0, 0.5)
            .bool("Only on ground", false)
            .bool("In water", true)
            .bool("Fly", false)
            .num("Vertical speed", 6.0, 0.5, 20.0, 0.5)
            .num("Fall speed", 0.0, 0.0, 10.0, 0.1)
            .bool("Anti fly kick", true)
            .num("Delay", 40.0, 1.0, 79.0, 1.0),
    ]
}

// Anti fly kick. Vanilla kicks a vehicle that floats 80 ticks running (a boat 160); a move down 0.0313 or more
// starts that count again. A server takes "floating" from the last move it handled before its own tick, and a slow
// frame or the network hands it several of a client's ticks at once, so one sinking tick between two hovering ones
// can go unseen every time (Horizon's own server kicked a hovering horse that way). So the sink lasts SINK_TICKS
// ticks running, and the tick after gives the height back: the vehicle holds its height over a cycle.
const SINK_TICKS: u32 = 3;
const SINK: f64 = 0.0313;
// Where in its cycle of Delay ticks the vehicle is (the sink is the cycle's first ticks), and the height the sink
// has taken so far.
static CYCLE: AtomicU32 = AtomicU32::new(SINK_TICKS + 1);
static LOST: AtomicU64 = AtomicU64::new(0);

// What the driven boat's motion this tick becomes, from the motion its own tick gave it (`velocity`), whether it
// stands on something and is in water; None when Entity Control is off.
pub(super) fn boat(game: &Game<'_>, velocity: Vec3, on_ground: bool, in_water: bool) -> Option<(Vec3, Option<f32>)> {
    with(|s| s.module("Entity Control").filter(|m| m.on && m.b("Boats")).map(|m| drive(game, m, velocity, on_ground, in_water)))
}

// The same for the mount your client moves (its motion after its own tick); None when Entity Control or Mounts is off.
pub(super) fn mount(game: &Game<'_>, velocity: Vec3, on_ground: bool, in_water: bool) -> Option<(Vec3, Option<f32>)> {
    with(|s| s.module("Entity Control").filter(|m| m.on && m.b("Mounts")).map(|m| drive(game, m, velocity, on_ground, in_water)))
}

fn drive(game: &Game<'_>, m: &Module, velocity: Vec3, on_ground: bool, in_water: bool) -> (Vec3, Option<f32>) {
    let held = |k: KeyCode| game.keys.raw(&k) || game.physical_keys.contains(&k);
    let mut v = velocity;
    if m.b("Speed") && (!m.b("Only on ground") || on_ground || m.b("Fly")) && (m.b("In water") || !in_water) {
        // Turn the keys' direction by the view.
        let (f, s) = (f64::from(held(KeyCode::KeyW)) - f64::from(held(KeyCode::KeyS)), f64::from(held(KeyCode::KeyA)) - f64::from(held(KeyCode::KeyD)));
        let len = f.hypot(s);
        let (x, z) = if len > 0.0 {
            let r = f64::from(game.yaw).to_radians();
            let (f, s) = (f / len, s / len);
            (s * r.cos() - f * r.sin(), f * r.cos() + s * r.sin())
        } else {
            (0.0, 0.0)
        };
        let speed = m.n("Horizontal speed") / 20.0;
        v = Vec3::new(x * speed, v.y, z * speed);
    }
    if m.b("Fly") {
        let up = m.n("Vertical speed") / 20.0;
        v.y = if held(KeyCode::Space) { up } else { 0.0 } - if held(KeyCode::ControlLeft) { up } else { m.n("Fall speed") / 20.0 };
        if !m.b("Anti fly kick") || on_ground || in_water {
            // The first sink comes a Delay after it leaves the ground or the water.
            CYCLE.store(SINK_TICKS + 1, Relaxed);
            LOST.store(0f64.to_bits(), Relaxed);
        } else {
            let (t, lost) = (CYCLE.load(Relaxed), f64::from_bits(LOST.load(Relaxed)));
            if t < SINK_TICKS {
                let sunk = v.y.min(-SINK);
                LOST.store((lost + v.y - sunk).to_bits(), Relaxed);
                v.y = sunk;
            } else if t == SINK_TICKS {
                v.y += lost;
                LOST.store(0f64.to_bits(), Relaxed);
            }
            CYCLE.store((t + 1) % (m.n("Delay") as u32).max(SINK_TICKS + 2), Relaxed);
        }
    }
    (v, m.b("Lock yaw").then_some(game.yaw))
}
