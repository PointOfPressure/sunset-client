// Camera and engine-level modules, ported from Meteor's source: Freecam, Free
// Look, Xray, Tunnel ESP, Void ESP, Light Overlay, and the two that sit on
// engine hooks, Timer and No Slow.

use super::perspective::CameraType;
use super::sunset::{Camera, Canvas, Category, Module, accent, line};
use super::sunset_visual::{Look, draw_box, potato};
use super::sunset_util::*;
use super::Game;
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::KeyCode;

const ORES: &str = "coal_ore,deepslate_coal_ore,iron_ore,deepslate_iron_ore,gold_ore,deepslate_gold_ore,lapis_ore,deepslate_lapis_ore,redstone_ore,deepslate_redstone_ore,diamond_ore,deepslate_diamond_ore,emerald_ore,deepslate_emerald_ore,copper_ore,deepslate_copper_ore,nether_gold_ore,nether_quartz_ore,ancient_debris";

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        Module::new("Freecam", Render, "Moves the camera away from your body, which stays where it is: fly with W, A, S, D, Space and Shift, through walls (the wheel changes the speed).")
            .num("Horizontal speed", 1.0, 0.05, 5.0, 0.05)
            .num("Vertical speed", 1.0, 0.05, 5.0, 0.05)
            .bool("Scroll speed", true)
            .pick("Start", &["Inside", "In front", "Above"])
            .bool("Tracer", true)
            .bool("Move player", false)
            .bool("Toggle on damage", false)
            .bool("Toggle on death", false)
            .bool("Toggle on log", true),
        Module::new("Free Look", Render, "Looks around in third person without turning the player, or the other way round.")
            .pick("Mode", &["Player", "Camera"])
            .bool("Toggle perspective", true)
            .num("Camera sensitivity", 8.0, 1.0, 20.0, 1.0)
            .bool("Arrows", true)
            .num("Arrow speed", 4.0, 1.0, 10.0, 1.0),
        Module::new("Xray", Render, "Only draws the listed blocks. Good for mining. Turning it on or off redraws the world. List: Simple ticks ores and valuables, Advanced picks from every block.")
            .pick("List", &["Simple", "Advanced"])
            .text("Blocks", ORES)
            .bool("Show water", false)
            .bool("Show lava", false)
            .bool("Show placed blocks", false)
            .num("Placed blocks kept", 2000.0, 100.0, 10000.0, 100.0),
        hand_view(),
        Module::new("Tunnel ESP", Render, "Highlights one-block tunnels.")
            .pick("Mode", &["Box", "Outline", "Potato"])
            .num("Range", 24.0, 8.0, 48.0, 4.0)
            .num("Height", 0.1, 0.02, 1.0, 0.02)
            .num("Max", 1000.0, 50.0, 4000.0, 50.0),
        Module::new("Void ESP", Render, "Marks holes in the bedrock that lead to the void.")
            .pick("Mode", &["Box", "Outline", "Potato"])
            .bool("Air only", false)
            .num("Horizontal radius", 32.0, 8.0, 64.0, 4.0)
            .num("Hole height", 1.0, 1.0, 5.0, 1.0)
            .bool("Nether roof", true)
            .num("Max", 1000.0, 50.0, 4000.0, 50.0),
        Module::new("Light Overlay", Render, "Shows the blocks where mobs can spawn: red always, yellow in the day. Potato: a dot on each.")
            .pick("Mode", &["Cross", "Potato"])
            .num("Horizontal range", 8.0, 2.0, 16.0, 1.0)
            .num("Vertical range", 4.0, 1.0, 8.0, 1.0)
            .num("Light level", 0.0, 0.0, 15.0, 1.0)
            .num("Max", 1000.0, 50.0, 4000.0, 50.0),
        Module::new("No Render", Render, "Hides fog, darkness, blindness, nausea, screen overlays (fire, water, in a wall, pumpkin, powder snow) and particles (all, or the types in Particles). A switch that is on hides it.")
            .bool("Hide fog", true)
            .bool("Hide water fog", true)
            .bool("Hide lava and powder snow fog", false)
            .bool("Hide blindness", true)
            .bool("Hide darkness", true)
            .bool("Hide nausea", true)
            .bool("Hide fire overlay", true)
            .bool("Hide liquid overlay", true)
            .bool("Hide in-wall overlay", true)
            .bool("Hide pumpkin, spyglass and powder snow overlays", false)
            .bool("Hide all particles", false)
            .text("Particles", ""),
        Module::new("Timer", World, "Changes the speed the game runs at.")
            .num("Multiplier", 2.0, 0.1, 10.0, 0.1),
        Module::new("No Slow", Movement, "Controls item-use movement slowdown.")
            .num("Ground %", 100.0, 20.0, 100.0, 5.0)
            .num("Air %", 100.0, 20.0, 100.0, 5.0),
    ]
}

struct Fc {
    pos: [f64; 3],
    prev: [f64; 3],
    yaw: f32,
    pitch: f32,
    camera: CameraType,
    health: f32,
    // The wheel's change to both speeds.
    speed: f64,
}

struct Fl {
    yaw: f32,
    pitch: f32,
    camera: CameraType,
}

#[derive(Default)]
struct Rs {
    fc: Option<Fc>,
    fl: Option<Fl>,
    xray: Option<(bool, String, bool, bool)>,
    placed: Placed,
    tunnels: Sweep<()>,
    voids: Sweep<()>,
    // Where mobs can spawn, and whether only at night (lit by the sky).
    lights: Sweep<bool>,
    // The Fog option as it was before No Render turned it off.
    fog_before: Option<bool>,
    // The effects No Render hides, to put back.
    hidden_effects: Vec<horizon_client::effects::EffectInstance>,
}

impl Default for Fc {
    fn default() -> Self {
        Self { pos: [0.0; 3], prev: [0.0; 3], yaw: 0.0, pitch: 0.0, camera: CameraType::FirstPerson, health: 20.0, speed: 1.0 }
    }
}

static RS: Mutex<Option<Rs>> = Mutex::new(None);

fn rs<R>(f: impl FnOnce(&mut Rs) -> R) -> R {
    let mut g = RS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Rs::default))
}

fn on(mods: &[Module], name: &str) -> Option<Module> {
    mods.iter().find(|m| m.name == name && m.on).cloned()
}

static IN_CAMERA: AtomicBool = AtomicBool::new(false);

// ---- Zoom (as Wurst's WI Zoom)

// The zoom the key holds (the wheel changes it), the zoom the view is at as it eases there, when it last moved, and
// whether the key was down.
struct Zoom {
    level: f32,
    now: f32,
    at: Option<std::time::Instant>,
    held: bool,
}

static ZOOM: Mutex<Zoom> = Mutex::new(Zoom { level: 0.0, now: 1.0, at: None, held: false });

fn zoom_state<R>(f: impl FnOnce(&mut Zoom) -> R) -> R {
    f(&mut ZOOM.lock().unwrap_or_else(|e| e.into_inner()))
}

// The zoom the view is at this frame, if zoomed: it eases to the held level and back out to 1 in about a tenth of a
// second (at once with Smooth off).
pub(super) fn zoom_now(game: &Game<'_>) -> Option<f32> {
    if !game.native_window() || !game.native_on(super::sunset::ID) || !game.native_playing() {
        return None;
    }
    let m = super::sunset::with(|s| s.module("Zoom").map(|m| (m.on, m.n("Zoom") as f32, m.b("Smooth"), m.b("Remember zoom"))));
    let (on, start, smooth, remember) = m?;
    zoom_state(|z| {
        if on && !z.held && (!remember || z.level < 1.0) {
            z.level = start;
        }
        z.held = on;
        let want = if on { z.level } else { 1.0 };
        let now = std::time::Instant::now();
        let dt = z.at.map_or(0.0, |t| now.duration_since(t).as_secs_f32()).min(0.25);
        z.at = Some(now);
        // Eased in steps of the zoom's logarithm, so 2x to 4x looks like 20x to 40x.
        z.now = if smooth { (z.now.ln() + (want.ln() - z.now.ln()) * (1.0 - (-dt * 25.0).exp())).exp() } else { want };
        if (z.now - want).abs() < 0.001 * want {
            z.now = want;
        }
        (z.now > 1.0).then_some(z.now)
    })
}

// How much slower the mouse turns while zoomed (Scale sensitivity).
pub(super) fn zoom_turn(game: &Game<'_>) -> Option<f64> {
    let scaled = super::sunset::with(|s| s.module("Zoom").is_some_and(|m| m.b("Scale sensitivity")));
    let now = zoom_state(|z| z.now);
    (scaled && game.native_on(super::sunset::ID) && now > 1.0).then(|| 1.0 / f64::from(now))
}

// The wheel while Zoom is held changes the zoom (each notch by a tenth), while Freecam flies its speed.
pub(super) fn camera_wheel(game: &Game<'_>, steps: f32) -> bool {
    if !game.native_window() || !game.native_on(super::sunset::ID) || steps == 0.0 {
        return false;
    }
    let (zoom, scroll, fly) = super::sunset::with(|s| {
        let zoom = s.module("Zoom").filter(|m| m.on);
        (zoom.is_some(), zoom.is_some_and(|m| m.b("Scroll")), s.module("Freecam").filter(|m| m.on && m.b("Scroll speed")).is_some())
    });
    if zoom {
        if scroll {
            zoom_state(|z| z.level = (z.level * 1.1_f32.powf(steps)).clamp(1.0, 50.0));
        }
        return scroll;
    }
    if fly && freecam_flying() {
        rs(|r| {
            if let Some(f) = r.fc.as_mut() {
                f.speed = (f.speed * 1.1_f64.powf(f64::from(steps))).clamp(0.05, 10.0);
            }
        });
        return true;
    }
    false
}

// ---- the camera hooks

// The frame's camera pose when Freecam or Free Look decide it: Some(its answer).
pub(super) fn camera(game: &mut Game<'_>, partial: f32) -> Option<Option<(glam::DVec3, f32, f32)>> {
    if IN_CAMERA.load(Ordering::Relaxed) {
        return None;
    }
    if let Some((pos, prev, yaw, pitch)) = rs(|r| r.fc.as_ref().map(|f| (f.pos, f.prev, f.yaw, f.pitch))) {
        let t = game.clock.camera_fraction();
        let at = glam::DVec3::new(
            prev[0] + (pos[0] - prev[0]) * t,
            prev[1] + (pos[1] - prev[1]) * t,
            prev[2] + (pos[2] - prev[2]) * t,
        );
        return Some(Some((at, yaw, pitch)));
    }
    if let Some((yaw, pitch)) = rs(|r| r.fl.as_ref().map(|f| (f.yaw, f.pitch))) {
        // The game's own third person camera, from the free angles.
        let (sy, sp) = (game.yaw, game.pitch);
        game.yaw = yaw;
        game.pitch = pitch;
        IN_CAMERA.store(true, Ordering::Relaxed);
        let view = game.camera_view(partial);
        IN_CAMERA.store(false, Ordering::Relaxed);
        game.yaw = sy;
        game.pitch = sp;
        return Some(view);
    }
    None
}

fn free_look_player() -> bool {
    super::sunset::with(|s| s.module("Free Look").is_some_and(|m| m.on && m.p("Mode") == "Player"))
}

// Freecam is flying the camera: the view stays third person behind the body (no hand, and the boxes and lines drawn
// over the world project from the camera the frame is drawn from), whatever F5 would make it.
pub(super) fn freecam_flying() -> bool {
    rs(|r| r.fc.is_some())
}

// Whether the mouse turns a free camera (Freecam, or Free Look in Player mode) instead of the player.
pub(super) fn camera_takes_mouse() -> bool {
    let free_look_player = free_look_player();
    rs(|r| r.fc.is_some() || (r.fl.is_some() && free_look_player))
}

// Mouse movement goes to the camera instead of the player: true when it did.
pub(super) fn turn(game: &mut Game<'_>, xo: f64, yo: f64) -> bool {
    let _ = game;
    let free_look_player = free_look_player();
    let sens = super::sunset::with(|s| s.module("Free Look").map_or(8.0, |m| m.n("Camera sensitivity"))) / 8.0;
    rs(|r| {
        if let Some(f) = r.fc.as_mut() {
            f.yaw += (xo * 0.15) as f32;
            f.pitch = (f.pitch + (yo * 0.15) as f32).clamp(-90.0, 90.0);
            true
        } else if let (Some(f), true) = (r.fl.as_mut(), free_look_player) {
            f.yaw += (xo * 0.15 * sens) as f32;
            f.pitch = (f.pitch + (yo * 0.15 * sens) as f32).clamp(-90.0, 90.0);
            true
        } else {
            false
        }
    })
}

// ---- ticks

const MOVE_KEYS: [KeyCode; 7] = [
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::Space,
    KeyCode::ShiftLeft,
    KeyCode::ControlLeft,
];

// Before the game's tick: start and stop the camera modules, and keep the
// player still while Freecam flies.
pub(super) fn pre_tick(game: &mut Game<'_>, mods: &[Module]) {
    let freecam = on(mods, "Freecam");
    let freelook = on(mods, "Free Look");
    let eye = game.eye_position();
    let health = game.player.as_ref().map_or(20.0, |p| p.health.health());
    // Freecam.
    match (freecam.as_ref(), rs(|r| r.fc.is_some())) {
        (Some(m), false) => {
            if let Some(e) = eye {
                let camera = game.camera_type;
                // Where the camera starts: at the eyes, 3 blocks in front of them, or 3 above.
                let yaw = f64::from(game.yaw).to_radians();
                let e = match m.p("Start") {
                    "In front" => [e[0] - yaw.sin() * 3.0, e[1], e[2] + yaw.cos() * 3.0],
                    "Above" => [e[0], e[1] + 3.0, e[2]],
                    _ => e,
                };
                rs(|r| {
                    r.fc = Some(Fc { pos: e, prev: e, yaw: game.yaw, pitch: game.pitch, camera, health, speed: 1.0 });
                });
                game.camera_type = CameraType::ThirdPersonBack;
            }
        }
        (None, true) => {
            if let Some(f) = rs(|r| r.fc.take()) {
                game.camera_type = f.camera;
            }
        }
        _ => {}
    }
    // The keys fly the camera, the body stands still; with Move player they move the body and the camera stays.
    if freecam_flying() {
        if !freecam.as_ref().is_some_and(|m| m.b("Move player")) {
            for k in MOVE_KEYS {
                game.keys.remove(&k);
            }
        }
        game.camera_type = CameraType::ThirdPersonBack;
    }
    // Free Look.
    match (freelook.clone(), rs(|r| r.fl.is_some())) {
        (Some(m), false) => {
            let camera = game.camera_type;
            rs(|r| r.fl = Some(Fl { yaw: game.yaw, pitch: game.pitch, camera }));
            if m.b("Toggle perspective") && camera != CameraType::ThirdPersonBack {
                game.camera_type = CameraType::ThirdPersonBack;
            }
        }
        (None, true) => {
            if let Some(f) = rs(|r| r.fl.take()) {
                if super::sunset::with(|s| s.module("Free Look").is_none_or(|m| m.b("Toggle perspective"))) {
                    game.camera_type = f.camera;
                }
            }
        }
        _ => {}
    }
}

pub(super) fn tick(game: &mut Game<'_>, mods: &[Module], ticks: u64) {
    // Freecam moves a camera each tick; Meteor's speed is blocks a tick, half
    // of it, the whole of it while sprinting.
    if let Some(m) = on(mods, "Freecam") {
        let playing = game.native_playing();
        let keys = game.physical_keys.clone();
        let health = game.player.as_ref().map_or(20.0, |p| p.health.health());
        let gone = game.remote.is_none();
        let mut stop = false;
        rs(|r| {
            let Some(f) = r.fc.as_mut() else {
                return;
            };
            if m.b("Toggle on damage") && health < f.health {
                stop = true;
            }
            if m.b("Toggle on death") && health <= 0.0 || m.b("Toggle on log") && gone {
                stop = true;
            }
            f.health = health;
            f.prev = f.pos;
            if !playing || m.b("Move player") {
                return;
            }
            let down = |k: KeyCode| keys.contains(&k);
            let sprint = if down(KeyCode::ControlLeft) { 1.0 } else { 0.5 } * f.speed;
            let (s, up) = (sprint * m.n("Horizontal speed"), sprint * m.n("Vertical speed"));
            let yaw = f64::from(f.yaw).to_radians();
            let forward = [-yaw.sin(), yaw.cos()];
            let right = [-yaw.cos(), -yaw.sin()];
            let (mut vx, mut vy, mut vz) = (0.0, 0.0, 0.0);
            let (fw, bw) = (down(KeyCode::KeyW), down(KeyCode::KeyS));
            let (rt, lf) = (down(KeyCode::KeyD), down(KeyCode::KeyA));
            if fw {
                vx += forward[0] * s;
                vz += forward[1] * s;
            }
            if bw {
                vx -= forward[0] * s;
                vz -= forward[1] * s;
            }
            if rt {
                vx += right[0] * s;
                vz += right[1] * s;
            }
            if lf {
                vx -= right[0] * s;
                vz -= right[1] * s;
            }
            if (fw || bw) && (rt || lf) {
                vx *= std::f64::consts::FRAC_1_SQRT_2;
                vz *= std::f64::consts::FRAC_1_SQRT_2;
            }
            if down(KeyCode::Space) {
                vy += up;
            }
            if down(KeyCode::ShiftLeft) {
                vy -= up;
            }
            f.pos = [f.pos[0] + vx, f.pos[1] + vy, f.pos[2] + vz];
        });
        if stop {
            toggle_off(game, "Freecam");
        }
    }
    // Free Look's arrow keys.
    if let Some(m) = on(mods, "Free Look") {
        if m.b("Arrows") && game.native_playing() {
            let keys = game.physical_keys.clone();
            let n = m.n("Arrow speed") * 2.0;
            let d = |neg: KeyCode, pos: KeyCode| (f32::from(u8::from(keys.contains(&pos))) - f32::from(u8::from(keys.contains(&neg)))) * 0.5 * n as f32;
            let (dx, dy) = (d(KeyCode::ArrowLeft, KeyCode::ArrowRight), d(KeyCode::ArrowUp, KeyCode::ArrowDown));
            if m.p("Mode") == "Player" {
                rs(|r| {
                    if let Some(f) = r.fl.as_mut() {
                        f.yaw += dx;
                        f.pitch = (f.pitch + dy).clamp(-90.0, 90.0);
                    }
                });
            }
        }
    }
    engine_flags(game, mods);
    let font = mods.iter().find(|m| m.name == "HUD").map_or(1, |m| horizon_client::font::sunset_font::style(m.p("Font")));
    horizon_client::font::sunset_font::STYLE.store(font, Ordering::Relaxed);
    no_render(game, mods);
    xray(game, mods);
    match on(mods, "Tunnel ESP") {
        Some(m) => scan_tunnels(game, &m, ticks),
        None => rs(|r| r.tunnels = Sweep::new()),
    }
    match on(mods, "Void ESP") {
        Some(m) => scan_voids(game, &m, ticks),
        None => rs(|r| r.voids = Sweep::new()),
    }
    match on(mods, "Light Overlay") {
        Some(m) => scan_lights(game, &m, ticks),
        None => rs(|r| r.lights = Sweep::new()),
    }
}

// Timer and No Slow: the flags the engine hooks read. While you move, Speed's own Timer stands in for Timer's
// (Meteor's override).
// Hand View (Meteor's): each arm and what it holds moved (in blocks), turned (degrees) and sized, and the sway the
// hands have when the view turns taken away.
fn hand_view() -> Module {
    let mut m = Module::new("Hand View", Category::Render, "Moves, turns and sizes your hands and what they hold in first person, each hand on its own (small hands: Scale under 1). No sway keeps them still when you turn.")
        .bool("No sway", false);
    for (scale, x, y, z, rx, ry, rz) in [
        ("Right scale", "Right X", "Right Y", "Right Z", "Right turn X", "Right turn Y", "Right turn Z"),
        ("Left scale", "Left X", "Left Y", "Left Z", "Left turn X", "Left turn Y", "Left turn Z"),
    ] {
        m = m.num(scale, 1.0, 0.1, 2.0, 0.05);
        for axis in [x, y, z] {
            m = m.num(axis, 0.0, -1.0, 1.0, 0.02);
        }
        for axis in [rx, ry, rz] {
            m = m.num(axis, 0.0, -180.0, 180.0, 5.0);
        }
    }
    m
}

fn hand_transform(m: &Module, side: &str) -> glam::Mat4 {
    let n = |name: &str| m.n(&format!("{side} {name}")) as f32;
    glam::Mat4::from_translation(glam::Vec3::new(n("X"), n("Y"), n("Z")))
        * glam::Mat4::from_rotation_x(n("turn X").to_radians())
        * glam::Mat4::from_rotation_y(n("turn Y").to_radians())
        * glam::Mat4::from_rotation_z(n("turn Z").to_radians())
        * glam::Mat4::from_scale(glam::Vec3::splat(n("scale")))
}

fn engine_flags(game: &mut Game<'_>, mods: &[Module]) {
    let hands = on(mods, "Hand View");
    let view = hands.as_ref().map(|m| [hand_transform(m, "Right"), hand_transform(m, "Left")]);
    if let Ok(mut v) = horizon_client::first_person::sunset_hand_view::VIEW.write() {
        *v = view;
    }
    horizon_client::first_person::sunset_hand_view::NO_SWAY.store(hands.is_some_and(|m| m.b("No sway")), Ordering::Relaxed);
    let moving = super::sunset::move_input(game) != (0.0, 0.0);
    let speed_timer = if on(mods, "Flight").is_none() {
        on(mods, "Speed").map(|m| m.n("Timer")).filter(|t| (*t - 1.0).abs() > 1e-9 && moving)
    } else { None };
    let mult = speed_timer.or_else(|| on(mods, "Timer").map(|m| m.n("Multiplier"))).unwrap_or(1.0) as f32;
    horizon_client::clock::sunset_timer::MULT.store(mult.to_bits(), Ordering::Relaxed);
    let no_slow = on(mods, "No Slow");
    let factor = no_slow.as_ref().map_or(0.2, |m| {
        let ground = game.player.as_ref().is_some_and(|p| p.ground.on_ground);
        (m.n(if ground { "Ground %" } else { "Air %" }) / 100.0) as f32
    });
    horizon_client::input::sunset_flags::NO_SLOW_FACTOR.store(factor.to_bits(), Ordering::Relaxed);
    horizon_client::input::sunset_flags::NO_SLOW.store(no_slow.is_some(), Ordering::Relaxed);
}

// No Render: fog through Horizon's own Fog option, the effects the player does
// not want to see taken off its list, and the overlay switches.
fn no_render(game: &mut Game<'_>, mods: &[Module]) {
    use horizon_client::{fog::sunset_fog as fog, screen_effects::sunset_overlay as overlay};
    let m = on(mods, "No Render");
    let flag = |name: &str| m.as_ref().is_some_and(|m| m.b(name));
    // Fog: Horizon's Fog option, remembered and put back.
    if flag("Hide fog") {
        if rs(|r| r.fog_before.is_none()) {
            rs(|r| r.fog_before = Some(game.settings.horizon.fog));
        }
        game.settings.horizon.fog = false;
    } else if let Some(before) = rs(|r| r.fog_before.take()) {
        game.settings.horizon.fog = before;
    }
    // Particles: all of them, or the listed types, never made.
    horizon_client::particles::sunset_particles::set(flag("Hide all particles"), m.as_ref().map_or("", |m| m.t("Particles")));
    fog::NO_WATER.store(flag("Hide water fog"), Ordering::Relaxed);
    fog::NO_LAVA.store(flag("Hide lava and powder snow fog"), Ordering::Relaxed);
    overlay::NO_FIRE.store(flag("Hide fire overlay"), Ordering::Relaxed);
    overlay::NO_LIQUID.store(flag("Hide liquid overlay"), Ordering::Relaxed);
    overlay::NO_WALL.store(flag("Hide in-wall overlay"), Ordering::Relaxed);
    overlay::NO_GUI.store(flag("Hide pumpkin, spyglass and powder snow overlays"), Ordering::Relaxed);
    // The dim and blur behind menus: ClickGUI's Menu dim switch (off hides them).
    overlay::NO_MENU.store(!mods.iter().find(|m| m.name == "ClickGUI").is_some_and(|m| m.b("Menu dim")), Ordering::Relaxed);
    // Effects the server put on the player that only change what it sees: kept aside while hidden, counted down
    // as the game would, and put back when No Render (or its switch) goes off.
    let (blind, dark, nausea) = (flag("Hide blindness"), flag("Hide darkness"), flag("Hide nausea"));
    let hide = |id: &str| (blind && id == "blindness") || (dark && id == "darkness") || (nausea && id == "nausea");
    let taken: Vec<_> = game.potions.effects.list.extract_if(.., |e| hide(e.id)).collect();
    let back: Vec<_> = rs(|r| {
        for e in &mut r.hidden_effects {
            if e.duration > 0 {
                e.duration -= 1;
            }
        }
        r.hidden_effects.retain(|e| e.duration != 0);
        for e in taken {
            r.hidden_effects.retain(|h| h.id != e.id);
            r.hidden_effects.push(e);
        }
        r.hidden_effects.extract_if(.., |e| !hide(e.id)).collect()
    });
    for e in back {
        game.potions.effects.add(e);
    }
}

// Xray: the mesher draws only the listed blocks (and water, lava and the blocks you placed, if asked); the world is
// rebuilt when that changes.
fn xray(game: &mut Game<'_>, mods: &[Module]) {
    // Placed blocks are remembered while the switch is on, Xray on or off.
    placed_blocks(game, mods.iter().find(|m| m.name == "Xray"));
    let m = on(mods, "Xray");
    let want = m.as_ref().map(|m| m.t("Blocks").to_owned());
    let (water, lava) = m.as_ref().map_or((false, false), |m| (m.b("Show water"), m.b("Show lava")));
    let key = (want.is_some(), want.clone().unwrap_or_default(), water, lava);
    if rs(|r| r.xray.as_ref() == Some(&key)) {
        return;
    }
    let shown = |name: &str| (water && short(name) == "water") || (lava && short(name) == "lava");
    let ids: HashSet<u32> = match &want {
        Some(list) => game
            .res
            .facts()
            .states
            .iter()
            .enumerate()
            .filter(|(_, s)| listed(list, &s.name) || shown(&s.name))
            .map(|(i, _)| i as u32)
            .collect(),
        None => HashSet::new(),
    };
    if let Ok(mut set) = horizon_client::mesher::sunset_xray::VISIBLE.write() {
        *set = ids;
    }
    horizon_client::mesher::sunset_xray::ON.store(want.is_some(), Ordering::Relaxed);
    game.all_sections_changed();
    rs(|r| r.xray = Some(key));
}

// Show placed blocks: where your own placing put a block, oldest first, for the world or server they are in (kept
// in the mod's data folder), and the places a block was just placed on, with what was there and until which tick
// to look for the new block.
#[derive(Default)]
struct Placed {
    key: String,
    at: Vec<[i32; 3]>,
    pending: Vec<([i32; 3], u32, u64)>,
    changed: bool,
    saved_tick: u64,
}

// The world or server, as the name of its file: the world's folder in single player, the address on a server.
fn place_file(game: &Game<'_>) -> Option<String> {
    let r = game.remote.as_ref()?;
    let address = horizon_client::net::shown_address(&r.session.address);
    let name = if address == "local" { game.world_root.display().to_string() } else { address };
    // FNV-1a: a file name from any text.
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    Some(format!("xray-placed-{hash:016x}"))
}

// A use on a block: the block itself (when it can be replaced) or the one beside the face is where a block may go.
pub(super) fn sent(game: &Game<'_>, packet: &horizon_protocol::packets::play::Serverbound) {
    use horizon_protocol::packets::play::Serverbound as S;
    let S::UseItemOn(p) = packet else {
        return;
    };
    if !rs(|r| !r.placed.key.is_empty()) {
        return;
    }
    let [x, y, z] = p.pos;
    let n = FACES[p.face.clamp(0, 5) as usize];
    let until = game.ticks + 10;
    rs(|r| {
        for at in [[x, y, z], [x + n[0], y + n[1], z + n[2]]] {
            r.placed.pending.push((at, game.world.block(at[0], at[1], at[2]).0, until));
        }
    });
}

// The six faces' directions, in the protocol's order: down, up, north, south, west, east.
const FACES: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

fn placed_blocks(game: &mut Game<'_>, xray: Option<&Module>) {
    let want = xray.filter(|m| m.b("Show placed blocks"));
    let file = want.and_then(|_| place_file(game)).unwrap_or_default();
    if rs(|r| r.placed.key != file) {
        // The last world's changes are kept before the next world's are read.
        if let Some((key, at)) = rs(|r| r.placed.changed.then(|| (r.placed.key.clone(), r.placed.at.clone()))) {
            save_placed(game, &key, &at);
        }
        let at = if file.is_empty() { Vec::new() } else { load_placed(game, &file) };
        mark(game, &at, &[]);
        rs(|r| r.placed = Placed { key: file, at, ..Placed::default() });
        if horizon_client::mesher::sunset_xray::ON.load(Ordering::Relaxed) {
            game.all_sections_changed();
        }
    }
    let Some(m) = want else {
        return;
    };
    let keep = m.n("Placed blocks kept") as usize;
    let tick = game.ticks;
    // A block that turned up where one was placed is remembered; one that is air now (broken) is forgotten.
    let air = |game: &Game<'_>, at: [i32; 3]| game.res.facts().is_air(game.world.block(at[0], at[1], at[2]));
    let pending = rs(|r| std::mem::take(&mut r.placed.pending));
    let (mut new, mut later) = (Vec::new(), Vec::new());
    for (at, before, until) in pending {
        let now = game.world.block(at[0], at[1], at[2]);
        if now.0 != before && !air(game, at) {
            new.push(at);
        } else if until > tick {
            later.push((at, before, until));
        }
    }
    let gone: Vec<[i32; 3]> = if tick % 10 == 0 {
        rs(|r| r.placed.at.clone()).into_iter().filter(|&at| game.world.chunks.contains_key(&(at[0] >> 4, at[2] >> 4)) && air(game, at)).collect()
    } else {
        Vec::new()
    };
    let changed = rs(|r| {
        let p = &mut r.placed;
        p.pending.extend(later);
        p.at.retain(|at| !gone.contains(at));
        for at in &new {
            p.at.retain(|a| a != at);
            p.at.push(*at);
        }
        let over = p.at.len().saturating_sub(keep);
        p.at.drain(..over);
        p.changed |= !new.is_empty() || !gone.is_empty() || over > 0;
        let save = p.changed && tick >= p.saved_tick + 100;
        if save {
            p.changed = false;
            p.saved_tick = tick;
        }
        (!new.is_empty() || !gone.is_empty(), save.then(|| (p.key.clone(), p.at.clone())))
    });
    if changed.0 {
        mark(game, &rs(|r| r.placed.at.clone()), &new);
    }
    if let Some((key, at)) = changed.1 {
        save_placed(game, &key, &at);
    }
}

fn save_placed(game: &Game<'_>, file: &str, at: &[[i32; 3]]) {
    let text: Vec<String> = at.iter().map(|[x, y, z]| format!("{x},{y},{z}")).collect();
    game.native_set_setting(super::sunset::ID, file, &text.join(";"));
}

fn load_placed(game: &Game<'_>, file: &str) -> Vec<[i32; 3]> {
    let text = game.native_setting(super::sunset::ID, file).unwrap_or_default();
    text.split(';')
        .filter_map(|p| {
            let mut n = p.split(',').map(|v| v.trim().parse::<i32>().ok());
            Some([n.next()??, n.next()??, n.next()??])
        })
        .collect()
}

// The places Xray's mesher shows whatever they hold, and the sections of `redraw` drawn again with them.
fn mark(game: &mut Game<'_>, all: &[[i32; 3]], redraw: &[[i32; 3]]) {
    horizon_client::mesher::sunset_xray::set_placed(all.iter().copied().collect());
    if !horizon_client::mesher::sunset_xray::ON.load(Ordering::Relaxed) {
        return;
    }
    for &[x, y, z] in redraw {
        if let Some(sy) = game.world.chunks.get(&(x >> 4, z >> 4)).map(|c| (y - c.min_y()) >> 4)
            && sy >= 0
        {
            game.world.dirty_sections.insert((x >> 4, sy, z >> 4));
        }
    }
}

// ---- Tunnel ESP

// Meteor's TunnelESP: a block you can walk through (air, or no collision and no fluid) or stand on (collision and no
// fluid), each from one lookup.
fn through(game: &Game<'_>, p: [i32; 3]) -> bool {
    let (facts, state) = (game.res.facts(), game.world.block(p[0], p[1], p[2]));
    facts.is_air(state) || (facts.get(state).fluid_level == 0 && !facts.get(state).full_collision)
}

fn walk_on(game: &Game<'_>, p: [i32; 3]) -> bool {
    let (facts, state) = (game.res.facts(), game.world.block(p[0], p[1], p[2]));
    !facts.is_air(state) && facts.get(state).fluid_level == 0 && facts.get(state).full_collision
}

// Room to walk in: a floor, two blocks of space and a roof.
fn walk_in(game: &Game<'_>, [x, y, z]: [i32; 3]) -> bool {
    walk_on(game, [x, y - 1, z]) && through(game, [x, y, z]) && !through(game, [x, y + 2, z]) && through(game, [x, y + 1, z])
}

#[derive(PartialEq, Clone, Copy)]
enum Side {
    Walkable,
    Blocked,
    Partial,
}

fn side(game: &Game<'_>, p: [i32; 3]) -> Side {
    if walk_in(game, p) {
        Side::Walkable
    } else if !through(game, p) && !through(game, [p[0], p[1] + 1, p[2]]) {
        Side::Blocked
    } else {
        Side::Partial
    }
}

// Walkable on two opposite sides and walled on the other two; the cheap tests first, so open ground and solid rock
// are turned away after a lookup or two.
fn is_tunnel(game: &Game<'_>, p: [i32; 3]) -> bool {
    if !walk_in(game, p) {
        return false;
    }
    let mut sides = [Side::Partial; 4];
    for (s, [dx, dz]) in sides.iter_mut().zip([[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        *s = side(game, [p[0] + dx, p[1], p[2] + dz]);
        if *s == Side::Partial {
            return false;
        }
    }
    let [e, w, s, n] = sides;
    (e == Side::Walkable && w == Side::Walkable && s == Side::Blocked && n == Side::Blocked)
        || (e == Side::Blocked && w == Side::Blocked && s == Side::Walkable && n == Side::Walkable)
}

fn scan_tunnels(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(at) = block_pos(game) else {
        return;
    };
    let range = m.n("Range") as i32;
    rs(|r| {
        let ended = r.tunnels.tick(ticks, at, range, range as u64, 40, 8, |c, x, out| {
            for z in c[2] - range..=c[2] + range {
                for y in c[1] - 12..=c[1] + 12 {
                    if is_tunnel(game, [x, y, z]) {
                        out.push(([x, y, z], ()));
                    }
                }
            }
            (2 * range as usize + 1) * 25 * 2
        });
        // Only runs of tunnel, not lone cells.
        if ended {
            let all: HashSet<[i32; 3]> = r.tunnels.found.iter().map(|f| f.0).collect();
            r.tunnels.found.retain(|(p, ())| [[1, 0], [-1, 0], [0, 1], [0, -1]].iter().any(|d| all.contains(&[p[0] + d[0], p[1], p[2] + d[1]])));
        }
    });
}

// ---- Void ESP

fn scan_voids(game: &Game<'_>, m: &Module, ticks: u64) {
    let dim = game.dim.id();
    if dim == "minecraft:the_end" {
        return;
    }
    let Some(at) = block_pos(game) else {
        return;
    };
    let radius = m.n("Horizontal radius") as i32;
    let height = m.n("Hole height") as i32;
    let air_only = m.b("Air only");
    let roof = m.b("Nether roof") && dim == "minecraft:the_nether";
    let wrong = |x: i32, y: i32, z: i32| -> bool {
        if !game.world.loaded(x, z) {
            return true;
        }
        let air = is_air(game, [x, y, z]);
        if air_only { !air } else { !air && facts(game, [x, y, z]).name.ends_with("bedrock") }
    };
    let hole = |x: i32, z: i32, floor: i32, roof: bool| -> bool {
        (0..height).all(|i| !wrong(x, if roof { floor - i } else { floor + i }, z))
    };
    let min_y = if dim == "minecraft:the_nether" { 0 } else { -64 };
    let key = u64::from(air_only) | u64::from(roof) << 1 | (height as u64) << 2 | (radius as u64) << 8;
    rs(|r| {
        r.voids.tick(ticks, at, radius, key, 40, 8, |c, x, out| {
            for z in c[2] - radius..=c[2] + radius {
                if hole(x, z, min_y, false) {
                    out.push(([x, min_y, z], ()));
                }
                if roof && hole(x, z, 127, true) {
                    out.push(([x, 127, z], ()));
                }
            }
            (2 * radius as usize + 1) * height as usize * 2
        })
    });
}

// ---- Light Overlay

fn scan_lights(game: &Game<'_>, m: &Module, ticks: u64) {
    let Some(at) = block_pos(game) else {
        return;
    };
    let (hr, vr) = (m.n("Horizontal range") as i32, m.n("Vertical range") as i32);
    let limit = m.n("Light level") as u8;
    let key = (hr as u64) | (vr as u64) << 8 | u64::from(limit) << 16;
    rs(|r| {
        r.lights.tick(ticks, at, hr, key, 10, 2, |c, x, out| {
            for z in c[2] - hr..=c[2] + hr {
                for y in c[1] - vr..=c[1] + vr {
                    let p = [x, y, z];
                    if !is_air(game, p) {
                        continue;
                    }
                    let below = [x, y - 1, z];
                    let f = facts(game, below);
                    // `isValidSpawnBlock`.
                    if is_air(game, below) || !f.solid_render || f.name.ends_with("bedrock") || f.name.ends_with("barrier") {
                        continue;
                    }
                    let light = game.world.light(x, y, z);
                    let (block, sky) = (light & 0x0F, light >> 4);
                    if block > limit {
                        continue;
                    }
                    out.push((p, sky > limit));
                }
            }
            (2 * hr as usize + 1) * (2 * vr as usize + 1) * 3
        })
    });
}

// ---- drawing

pub(super) fn overlay(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    let me = player_pos(game).unwrap_or([0.0; 3]);
    // Freecam's tracer: a line from the middle of the view to your body.
    if get("Freecam").is_some_and(|m| m.b("Tracer"))
        && freecam_flying()
        && let Some(body) = cam.project([me[0], me[1] + 0.9, me[2]])
    {
        line(out, [cam.w as f32 / 2.0, cam.h as f32 / 2.0], body, 0xFF00_0000 | accent(), lw);
    }
    if let Some(m) = get("Tunnel ESP") {
        rs(|r| flat_boxes(cam, out, m, r.tunnels.found.iter().map(|f| f.0), m.n("Height"), 0xFF00_0000 | accent(), lw));
    }
    if let Some(m) = get("Void ESP") {
        rs(|r| flat_boxes(cam, out, m, r.voids.found.iter().map(|f| f.0).filter(|p| dist(me, center_of(*p)) < 64.0), 1.0, 0xFF80_40FF, lw));
    }
    if let Some(m) = get("Light Overlay") {
        let (dots, max) = (m.p("Mode") == "Potato", m.n("Max") as usize);
        rs(|r| {
            let mut drawn = 0;
            for &(p, potential) in &r.lights.found {
                let (x, y, z) = (f64::from(p[0]), f64::from(p[1]) + 0.0075, f64::from(p[2]));
                let colour = if potential { 0xFFFF_FF55 } else { 0xFFFF_5555 };
                let shown = if dots {
                    potato(cam, out, [x + 0.5, y, z + 0.5], colour, lw)
                } else if cam.sees([x + 0.5, y, z + 0.5], 0.71) {
                    if let (Some(a), Some(b)) = (cam.project([x, y, z]), cam.project([x + 1.0, y, z + 1.0])) {
                        line(out, a, b, colour, lw);
                    }
                    if let (Some(a), Some(b)) = (cam.project([x + 1.0, y, z]), cam.project([x, y, z + 1.0])) {
                        line(out, a, b, colour, lw);
                    }
                    true
                } else {
                    false
                };
                if shown {
                    drawn += 1;
                    if drawn >= max {
                        break;
                    }
                }
            }
        });
    }
}

// Flat boxes on blocks, `h` high, nearest first, up to the module's Max.
fn flat_boxes(cam: &Camera, out: &mut impl Canvas, m: &Module, found: impl Iterator<Item = [i32; 3]>, h: f64, colour: u32, lw: f32) {
    let (look, max) = (Look::of(m), m.n("Max") as usize);
    let mut drawn = 0;
    for p in found {
        let lo = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
        if draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + h, lo[2] + 1.0], colour, lw) {
            drawn += 1;
            if drawn >= max {
                return;
            }
        }
    }
}
