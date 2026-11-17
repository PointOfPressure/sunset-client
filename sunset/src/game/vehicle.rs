// Entity Control (Sunset's, which takes in Boat Fly): the boat you drive goes where you look at the speed you set,
// flies up with Jump and down with Sprint, and holds its height otherwise. Horizon simulates a driven boat on the
// client and sends it once a tick (MoveVehicle), so the boat's own tick is where its motion is replaced. Animals
// are driven by the server from your input on Horizon's server, and need a saddle on any server, so this is boats.

use super::sunset::{Category, Module, with};
use super::Game;
use horizon_core::geometry::Vec3;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use winit::keyboard::KeyCode;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Entity Control", Category::Movement, "Drives and flies boats.")
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

// Ticks until the next anti kick sink.
static SINK_IN: AtomicU32 = AtomicU32::new(0);

// What the driven boat's motion this tick becomes, from the motion its own tick gave it (`velocity`), whether it
// stands on something and is in water; None when Entity Control is off.
pub(super) fn boat(game: &Game<'_>, velocity: Vec3, on_ground: bool, in_water: bool) -> Option<(Vec3, Option<f32>)> {
    with(|s| s.module("Entity Control").filter(|m| m.on).map(|m| drive(game, m, velocity, on_ground, in_water)))
}

fn drive(game: &Game<'_>, m: &Module, velocity: Vec3, on_ground: bool, in_water: bool) -> (Vec3, Option<f32>) {
    let held = |k: KeyCode| game.keys.contains(&k) || game.physical_keys.contains(&k);
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
        // Vanilla kicks a vehicle that floats 80 ticks running; a move down 0.0313 or more starts that count again.
        let left = SINK_IN.load(Relaxed);
        if m.b("Anti fly kick") && !on_ground && !in_water && left == 0 {
            v.y = v.y.min(-0.0313);
            SINK_IN.store(m.n("Delay") as u32, Relaxed);
        } else {
            SINK_IN.store(left.saturating_sub(1), Relaxed);
        }
    }
    (v, m.b("Lock yaw").then_some(game.yaw))
}
