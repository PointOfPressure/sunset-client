// Camera and engine-level modules, ported from Meteor's source: Freecam, Free
// Look, Xray, Tunnel ESP, Void ESP, Light Overlay, and the two that sit on
// engine hooks, Timer and No Slow.

use super::perspective::CameraType;
use super::sunset::{Camera, Category, Module, Shape, accent, line, outline};
use super::sunset_util::*;
use super::{Game, Screen};
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::keyboard::KeyCode;

const ORES: &str = "coal_ore,deepslate_coal_ore,iron_ore,deepslate_iron_ore,gold_ore,deepslate_gold_ore,lapis_ore,deepslate_lapis_ore,redstone_ore,deepslate_redstone_ore,diamond_ore,deepslate_diamond_ore,emerald_ore,deepslate_emerald_ore,copper_ore,deepslate_copper_ore,nether_gold_ore,nether_quartz_ore,ancient_debris";

pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    vec![
        Module::new("Freecam", Render, "Moves the camera away from the player: fly around while you stand still.")
            .num("Speed", 1.0, 0.1, 5.0, 0.1)
            .bool("Toggle on damage", false)
            .bool("Toggle on death", false)
            .bool("Toggle on log", true),
        Module::new("Free Look", Render, "Looks around in third person without turning the player, or the other way round.")
            .pick("Mode", &["Player", "Camera"])
            .bool("Toggle perspective", true)
            .num("Camera sensitivity", 8.0, 1.0, 20.0, 1.0)
            .bool("Arrows", true)
            .num("Arrow speed", 4.0, 1.0, 10.0, 1.0),
        Module::new("Xray", Render, "Only draws the listed blocks. Good for mining. Turning it on or off redraws the world.")
            .text("Blocks", ORES),
        Module::new("Tunnel ESP", Render, "Highlights one-block tunnels.")
            .num("Range", 24.0, 8.0, 48.0, 4.0)
            .num("Height", 0.1, 0.02, 1.0, 0.02),
        Module::new("Void ESP", Render, "Marks holes in the bedrock that lead to the void.")
            .bool("Air only", false)
            .num("Horizontal radius", 32.0, 8.0, 64.0, 4.0)
            .num("Hole height", 1.0, 1.0, 5.0, 1.0)
            .bool("Nether roof", true),
        Module::new("Light Overlay", Render, "Shows the blocks where mobs can spawn: red always, yellow in the day.")
            .num("Horizontal range", 8.0, 2.0, 16.0, 1.0)
            .num("Vertical range", 4.0, 1.0, 8.0, 1.0)
            .num("Light level", 0.0, 0.0, 15.0, 1.0),
        Module::new("No Render", Render, "Hides fog, darkness, blindness, nausea, screen overlays (fire, water, in a wall, pumpkin, powder snow) and the dark sheet and blur behind menus.")
            .bool("Fog", true)
            .bool("Water fog", true)
            .bool("Lava and powder snow fog", false)
            .bool("Blindness", true)
            .bool("Darkness", true)
            .bool("Nausea", true)
            .bool("Fire overlay", true)
            .bool("Liquid overlay", true)
            .bool("In wall overlay", true)
            .bool("Pumpkin, spyglass and powder snow overlays", false)
            .bool("Menu background", true),
        Module::new("Timer", World, "Changes the speed the game runs at.")
            .num("Multiplier", 2.0, 0.1, 10.0, 0.1),
        Module::new("No Slow", Movement, "Stops using an item (eating, drawing a bow, blocking) from slowing you down."),
    ]
}

struct Fc {
    pos: [f64; 3],
    prev: [f64; 3],
    yaw: f32,
    pitch: f32,
    camera: CameraType,
    health: f32,
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
    xray: Option<(bool, String)>,
    tunnels: Vec<[i32; 3]>,
    tunnels_at: u64,
    voids: Vec<([i32; 3], u32)>,
    voids_at: u64,
    lights: Vec<([i32; 3], bool)>,
    lights_at: u64,
    // The Fog option as it was before No Render turned it off.
    fog_before: Option<bool>,
}

impl Default for Fc {
    fn default() -> Self {
        Self { pos: [0.0; 3], prev: [0.0; 3], yaw: 0.0, pitch: 0.0, camera: CameraType::FirstPerson, health: 20.0 }
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

// Mouse movement goes to the camera instead of the player: true when it did.
pub(super) fn turn(game: &mut Game<'_>, xo: f64, yo: f64) -> bool {
    let _ = game;
    super::sunset_combat::aim_note_user(xo, yo);
    let free_look_player = super::sunset::with(|s| s.module("Free Look").is_some_and(|m| m.on && m.p("Mode") == "Player"));
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
    match (freecam.is_some(), rs(|r| r.fc.is_some())) {
        (true, false) => {
            if let Some(e) = eye {
                let camera = game.camera_type;
                rs(|r| {
                    r.fc = Some(Fc { pos: e, prev: e, yaw: game.yaw, pitch: game.pitch, camera, health });
                });
                game.camera_type = CameraType::ThirdPersonBack;
            }
        }
        (false, true) => {
            if let Some(f) = rs(|r| r.fc.take()) {
                game.camera_type = f.camera;
            }
        }
        _ => {}
    }
    if rs(|r| r.fc.is_some()) {
        for k in MOVE_KEYS {
            game.keys.remove(&k);
        }
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
            if !playing {
                return;
            }
            let down = |k: KeyCode| keys.contains(&k);
            let s = if down(KeyCode::ControlLeft) { 1.0 } else { 0.5 } * m.n("Speed");
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
                vy += s;
            }
            if down(KeyCode::ShiftLeft) {
                vy -= s;
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
    let clean = mods.iter().find(|m| m.name == "HUD").is_none_or(|m| m.p("Font") != "Minecraft");
    horizon_client::font::sunset_font::ON.store(clean, Ordering::Relaxed);
    no_render(game, mods);
    xray(game, mods);
    if let Some(m) = on(mods, "Tunnel ESP") {
        scan_tunnels(game, &m, ticks);
    }
    if let Some(m) = on(mods, "Void ESP") {
        scan_voids(game, &m, ticks);
    }
    if let Some(m) = on(mods, "Light Overlay") {
        scan_lights(game, &m, ticks);
    }
}

// Timer and No Slow: the flags the engine hooks read.
fn engine_flags(_game: &mut Game<'_>, mods: &[Module]) {
    let mult = on(mods, "Timer").map_or(1.0, |m| m.n("Multiplier") as f32);
    horizon_client::clock::sunset_timer::MULT.store(mult.to_bits(), Ordering::Relaxed);
    horizon_client::input::sunset_flags::NO_SLOW.store(on(mods, "No Slow").is_some(), Ordering::Relaxed);
}

// No Render: fog through Horizon's own Fog option, the effects the player does
// not want to see taken off its list, and the overlay switches.
fn no_render(game: &mut Game<'_>, mods: &[Module]) {
    use horizon_client::{fog::sunset_fog as fog, screen_effects::sunset_overlay as overlay};
    let m = on(mods, "No Render");
    let flag = |name: &str| m.as_ref().is_some_and(|m| m.b(name));
    // Fog: Horizon's Fog option, remembered and put back.
    if flag("Fog") {
        if rs(|r| r.fog_before.is_none()) {
            rs(|r| r.fog_before = Some(game.settings.horizon.fog));
        }
        game.settings.horizon.fog = false;
    } else if let Some(before) = rs(|r| r.fog_before.take()) {
        game.settings.horizon.fog = before;
    }
    fog::NO_WATER.store(flag("Water fog"), Ordering::Relaxed);
    fog::NO_LAVA.store(flag("Lava and powder snow fog"), Ordering::Relaxed);
    overlay::NO_FIRE.store(flag("Fire overlay"), Ordering::Relaxed);
    overlay::NO_LIQUID.store(flag("Liquid overlay"), Ordering::Relaxed);
    overlay::NO_WALL.store(flag("In wall overlay"), Ordering::Relaxed);
    overlay::NO_GUI.store(flag("Pumpkin, spyglass and powder snow overlays"), Ordering::Relaxed);
    overlay::NO_MENU.store(flag("Menu background"), Ordering::Relaxed);
    // Effects the server put on the player that only change what it sees.
    let (blind, dark, nausea) = (flag("Blindness"), flag("Darkness"), flag("Nausea"));
    if blind || dark || nausea {
        game.potions.effects.list.retain(|e| {
            !((blind && e.id == "blindness") || (dark && e.id == "darkness") || (nausea && e.id == "nausea"))
        });
    }
}

// Xray: the mesher draws only the listed blocks; the world is rebuilt when
// that changes.
fn xray(game: &mut Game<'_>, mods: &[Module]) {
    let m = on(mods, "Xray");
    let want = m.as_ref().map(|m| m.t("Blocks").to_owned());
    let key = (want.is_some(), want.clone().unwrap_or_default());
    if rs(|r| r.xray.as_ref() == Some(&key)) {
        return;
    }
    let ids: HashSet<u32> = match &want {
        Some(list) => game
            .res
            .facts()
            .states
            .iter()
            .enumerate()
            .filter(|(_, s)| listed(list, &s.name))
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

// ---- Tunnel ESP

fn solid(game: &Game<'_>, p: [i32; 3]) -> bool {
    !is_air(game, p) && facts(game, p).full_collision
}

fn can_walk(game: &Game<'_>, p: [i32; 3]) -> bool {
    !solid(game, p) && !solid(game, [p[0], p[1] + 1, p[2]]) && !is_fluid(game, p)
}

fn is_fluid(game: &Game<'_>, p: [i32; 3]) -> bool {
    facts(game, p).fluid_level > 0
}

#[derive(PartialEq, Clone, Copy)]
enum Side {
    Walkable,
    Blocked,
    Partial,
}

fn side(game: &Game<'_>, p: [i32; 3]) -> Side {
    if can_walk(game, p) {
        Side::Walkable
    } else if solid(game, p) && solid(game, [p[0], p[1] + 1, p[2]]) {
        Side::Blocked
    } else {
        Side::Partial
    }
}

fn is_tunnel(game: &Game<'_>, p: [i32; 3]) -> bool {
    if !can_walk(game, p) {
        return false;
    }
    let s = |dx: i32, dz: i32| side(game, [p[0] + dx, p[1], p[2] + dz]);
    let (e, w, n, so) = (s(1, 0), s(-1, 0), s(0, -1), s(0, 1));
    if [e, w, n, so].contains(&Side::Partial) {
        return false;
    }
    (e == Side::Walkable && w == Side::Walkable && n == Side::Blocked && so == Side::Blocked)
        || (e == Side::Blocked && w == Side::Blocked && n == Side::Walkable && so == Side::Walkable)
}

fn scan_tunnels(game: &Game<'_>, m: &Module, ticks: u64) {
    if rs(|r| r.tunnels_at != 0 && ticks.saturating_sub(r.tunnels_at) < 40) {
        return;
    }
    let Some(at) = block_pos(game) else {
        return;
    };
    let range = m.n("Range") as i32;
    let mut found: HashSet<[i32; 3]> = HashSet::new();
    for x in at[0] - range..=at[0] + range {
        for z in at[2] - range..=at[2] + range {
            for y in at[1] - 12..=at[1] + 12 {
                if is_tunnel(game, [x, y, z]) {
                    found.insert([x, y, z]);
                }
            }
        }
    }
    // Only runs of tunnel, not lone cells.
    let kept: Vec<[i32; 3]> = found
        .iter()
        .copied()
        .filter(|p| [[1, 0], [-1, 0], [0, 1], [0, -1]].iter().any(|d| found.contains(&[p[0] + d[0], p[1], p[2] + d[1]])))
        .take(1500)
        .collect();
    rs(|r| {
        r.tunnels = kept;
        r.tunnels_at = ticks.max(1);
    });
}

// ---- Void ESP

fn scan_voids(game: &Game<'_>, m: &Module, ticks: u64) {
    if rs(|r| r.voids_at != 0 && ticks.saturating_sub(r.voids_at) < 40) {
        return;
    }
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
    let mut found = Vec::new();
    for x in at[0] - radius..=at[0] + radius {
        for z in at[2] - radius..=at[2] + radius {
            if hole(x, z, min_y, false) {
                found.push(([x, min_y, z], 0xFF80_40FF));
            }
            if m.b("Nether roof") && dim == "minecraft:the_nether" && hole(x, z, 127, true) {
                found.push(([x, 127, z], 0xFF80_40FF));
            }
        }
    }
    found.truncate(2500);
    rs(|r| {
        r.voids = found;
        r.voids_at = ticks.max(1);
    });
}

// ---- Light Overlay

fn scan_lights(game: &Game<'_>, m: &Module, ticks: u64) {
    if rs(|r| r.lights_at != 0 && ticks.saturating_sub(r.lights_at) < 10) {
        return;
    }
    let Some(at) = block_pos(game) else {
        return;
    };
    let (hr, vr) = (m.n("Horizontal range") as i32, m.n("Vertical range") as i32);
    let limit = m.n("Light level") as u8;
    let mut found = Vec::new();
    for x in at[0] - hr..=at[0] + hr {
        for z in at[2] - hr..=at[2] + hr {
            for y in at[1] - vr..=at[1] + vr {
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
                found.push((p, sky > limit));
            }
        }
    }
    found.truncate(2500);
    rs(|r| {
        r.lights = found;
        r.lights_at = ticks.max(1);
    });
}

// ---- drawing

pub(super) fn overlay(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut Vec<Shape>, lw: f32) {
    let flat = |p: [i32; 3], h: f64| {
        let lo = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
        cam.rect(lo, [lo[0] + 1.0, lo[1] + h, lo[2] + 1.0])
    };
    let me = player_pos(game).unwrap_or([0.0; 3]);
    if let Some(m) = on(mods, "Tunnel ESP") {
        let h = m.n("Height");
        for p in rs(|r| r.tunnels.clone()) {
            if let Some(r) = flat(p, h) {
                outline(out, r, 0xFF00_0000 | accent(), lw, true);
            }
        }
    }
    if on(mods, "Void ESP").is_some() {
        for (p, c) in rs(|r| r.voids.clone()) {
            if dist(me, center_of(p)) < 64.0 {
                if let Some(r) = flat(p, 1.0) {
                    outline(out, r, c, lw, true);
                }
            }
        }
    }
    if on(mods, "Light Overlay").is_some() {
        for (p, potential) in rs(|r| r.lights.clone()) {
            let (x, y, z) = (f64::from(p[0]), f64::from(p[1]) + 0.0075, f64::from(p[2]));
            let colour = if potential { 0xFFFF_FF55 } else { 0xFFFF_5555 };
            if let (Some(a), Some(b)) = (cam.project([x, y, z]), cam.project([x + 1.0, y, z + 1.0])) {
                line(out, a, b, colour, lw);
            }
            if let (Some(a), Some(b)) = (cam.project([x + 1.0, y, z]), cam.project([x, y, z + 1.0])) {
                line(out, a, b, colour, lw);
            }
        }
    }
    let _ = Screen::Playing;
}

// Once a frame.
pub(super) fn frame(game: &mut Game<'_>) {
    let mods = super::sunset::snapshot();
    super::sunset_combat::aim_frame(game, &mods);
}
