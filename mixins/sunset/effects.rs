// Effects: visuals in the world that only you see. Jump circles (a ring grows where a player jumps), Trails
// (footsteps, rings or sparks where players walk), Hit particles (glowing sparks off what you hit, and off a totem
// pop) and Weather (rain, snow, fireflies or embers round you). Drawn on the HUD as projected shapes; nothing is
// sent. Also the shield breaks ESP colours by.
use super::Game;
use super::sunset::{Camera, Canvas, Category, Module, accent, own_id, with};
use horizon_protocol::packets::play::{Clientbound as C, Serverbound as S};
use std::f32::consts::TAU;
use std::sync::Mutex;
use std::time::Instant;

pub(super) const NAME: &str = "Effects";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Render, "Jump circles, trails, hit particles and weather.")
            .bool("Jump circles", true)
            .bool("Circles self only", false)
            .num("Circle radius", 1.0, 0.2, 5.0, 0.1)
            .num("Circle duration (ms)", 1200.0, 200.0, 4000.0, 100.0)
            .num("Circle thickness", 0.02, 0.005, 0.1, 0.005)
            .num("Circle glow", 0.15, 0.0, 0.5, 0.01)
            .bool("Trails", false)
            .pick("Trail mode", &["Footsteps", "Ring", "Particles"])
            .bool("Trails self only", true)
            .num("Trail duration (ms)", 1500.0, 200.0, 5000.0, 100.0)
            .num("Trail size", 1.0, 0.2, 3.0, 0.1)
            .bool("Hit particles", false)
            .num("Particle amount", 10.0, 1.0, 20.0, 1.0)
            .num("Particle lifespan (s)", 10.0, 1.0, 60.0, 1.0)
            .bool("Particle physics", true)
            .bool("On totem pops", true)
            .bool("Weather", false)
            .pick("Weather mode", &["Rain", "Snow", "Fireflies", "Embers"])
            .num("Density", 200.0, 10.0, 1500.0, 10.0)
            .num("Weather radius", 50.0, 5.0, 100.0, 5.0)
            .num("Weather speed", 1.0, 0.1, 5.0, 0.1)
            .num("Weather size", 3.0, 0.1, 5.0, 0.1)
            .pick("Color", &["Accent", "Custom"])
            .colour("Custom color", "Purple"),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        "Circles self only" | "Circle radius" | "Circle duration (ms)" | "Circle thickness" | "Circle glow" => m.b("Jump circles"),
        "Trail mode" | "Trails self only" | "Trail duration (ms)" | "Trail size" => m.b("Trails"),
        "Particle amount" | "Particle lifespan (s)" | "Particle physics" | "On totem pops" => m.b("Hit particles"),
        "Weather mode" | "Density" | "Weather radius" | "Weather speed" | "Weather size" => m.b("Weather"),
        "Custom color" => m.p("Color") == "Custom",
        _ => true,
    }
}

// ---- shapes

// A filled circle on screen.
pub(super) fn disc(out: &mut impl Canvas, [x, y]: [f32; 2], r: f32, argb: u32) {
    const N: usize = 12;
    if r < 0.3 || argb >> 24 == 0 {
        return;
    }
    let rim = |k: usize| {
        let a = k as f32 * TAU / N as f32;
        [x + r * a.cos(), y + r * a.sin()]
    };
    for k in (0..N).step_by(2) {
        out.quad([[x, y], rim(k), rim(k + 1), rim(k + 2)], argb);
    }
}

// A soft dot: a faint wide halo, a brighter middle, a bright core.
pub(super) fn glow(out: &mut impl Canvas, at: [f32; 2], r: f32, argb: u32) {
    let a = (argb >> 24) as f32;
    let with = |k: f32| ((a * k).round().clamp(0.0, 255.0) as u32) << 24 | (argb & 0x00FF_FFFF);
    disc(out, at, r * 2.2, with(0.12));
    disc(out, at, r * 1.4, with(0.3));
    disc(out, at, r * 0.7, with(0.85));
}

// `rgb` mixed toward white (t > 0) or black (t < 0) by |t|.
pub(super) fn shade(rgb: u32, t: f32) -> u32 {
    let to = if t >= 0.0 { 255.0 } else { 0.0 };
    let k = t.abs().min(1.0);
    let ch = |s: u32| {
        let c = ((rgb >> s) & 0xFF) as f32;
        ((c + (to - c) * k).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

pub(super) fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f32, ((b >> s) & 0xFF) as f32);
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

pub(super) fn alpha(rgb: u32, a: f32) -> u32 {
    ((a.clamp(0.0, 1.0) * 255.0).round() as u32) << 24 | (rgb & 0x00FF_FFFF)
}

// A flat ring round `at` in the ground's plane: its band `thickness` wide at `radius`, with a fainter `glow` band
// either side.
fn ring(cam: &Camera, out: &mut impl Canvas, at: [f64; 3], radius: f32, thickness: f32, glow: f32, rgb: u32, a: f32) {
    const N: usize = 48;
    if !cam.sees(at, f64::from(radius + thickness + glow)) {
        return;
    }
    let point = |r: f32, k: usize| {
        let t = k as f32 * TAU / N as f32;
        cam.project([at[0] + f64::from(r * t.cos()), at[1], at[2] + f64::from(r * t.sin())])
    };
    let (inner, outer) = (radius - thickness / 2.0, radius + thickness / 2.0);
    let bands = [(inner - glow, inner, a * 0.3), (inner, outer, a), (outer, outer + glow, a * 0.3)];
    for (lo, hi, a) in bands.into_iter().filter(|b| b.1 > b.0.max(0.0)) {
        let colour = alpha(rgb, a);
        let lo = lo.max(0.0);
        for k in 0..N {
            if let (Some(p), Some(q), Some(r), Some(s)) = (point(lo, k), point(hi, k), point(hi, k + 1), point(lo, k + 1)) {
                out.quad([p, q, r, s], colour);
            }
        }
    }
}

// A flat oval on the ground at `at`, its long side along `yaw` (half sizes, blocks).
fn oval(cam: &Camera, out: &mut impl Canvas, at: [f64; 3], yaw: f32, half: [f32; 2], argb: u32) {
    const N: usize = 12;
    let (s, c) = yaw.to_radians().sin_cos();
    let (fwd, right) = ([-s, c], [c, s]);
    let rim = |k: usize| {
        let t = k as f32 * TAU / N as f32;
        let (w, l) = (half[0] * t.cos(), half[1] * t.sin());
        cam.project([at[0] + f64::from(right[0] * w + fwd[0] * l), at[1], at[2] + f64::from(right[1] * w + fwd[1] * l)])
    };
    let Some(mid) = cam.project(at) else {
        return;
    };
    for k in (0..N).step_by(2) {
        if let (Some(a), Some(b), Some(c)) = (rim(k), rim(k + 1), rim(k + 2)) {
            out.quad([mid, a, b, c], argb);
        }
    }
}

// ---- state

struct Circle {
    at: [f64; 3],
    born: Instant,
    life: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Trail {
    Footstep,
    Ring,
    Spark,
}

struct Mark {
    at: [f64; 3],
    yaw: f32,
    kind: Trail,
    vel: [f64; 3],
    shade: f32,
    born: Instant,
    life: f32,
}

struct Spark {
    prev: [f64; 3],
    at: [f64; 3],
    vel: [f64; 3],
    pop: bool,
    born: Instant,
}

struct Drop {
    at: [f64; 3],
    vel: [f64; 3],
    age: f32,
    life: f32,
    phase: f32,
    shade: f32,
}

#[derive(Default)]
struct Fx {
    // Players last tick: on the ground, and their y; where the last trail mark went, and which foot.
    last: Vec<(u32, bool, f64)>,
    steps: Vec<(u32, [f64; 3], bool)>,
    circles: Vec<Circle>,
    marks: Vec<Mark>,
    sparks: Vec<Spark>,
    drops: Vec<Drop>,
    weather_from: Option<[f64; 3]>,
    frame: Option<Instant>,
    epoch: Option<Instant>,
    // A player's shield put out by an axe: until when (tick).
    broken: Vec<(u32, u64)>,
    // Each player's or mob's totem pops (event 35) since they appeared: Nametags' Totem pops.
    pops: Vec<(u32, u32)>,
    roll: u64,
}

static FX: Mutex<Option<Fx>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut Fx) -> R) -> R {
    f(FX.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Fx::default))
}

fn random(fx: &mut Fx) -> f64 {
    fx.roll = (fx.roll ^ 0x9E37_79B9_7F4A_7C15).wrapping_mul(0xBF58_476D_1CE4_E5B9) | 1;
    fx.roll ^= fx.roll >> 31;
    (fx.roll >> 11) as f64 / (1u64 << 53) as f64
}

fn module() -> Option<Module> {
    with(|s| s.module(NAME).filter(|m| m.on).cloned())
}

fn colour(m: &Module) -> u32 {
    if m.p("Color") == "Custom" { m.c("Custom color") & 0x00FF_FFFF } else { accent() & 0x00FF_FFFF }
}

// Where your hits and pops start sparks: a mob's middle.
fn sparks_at(fx: &mut Fx, m: &Module, at: [f64; 3], count: usize, pop: bool) {
    let cap = (m.n("Particle lifespan (s)") * 20.0) as usize;
    for _ in 0..count {
        if fx.sparks.len() > cap {
            break;
        }
        let p = [at[0] + (random(fx) - 0.5) * 0.5, at[1] + random(fx) + 0.5, at[2] + (random(fx) - 0.5) * 0.5];
        // Blocks a tick: up to an eighth across, up to an eighth up.
        let vel = [(random(fx) - 0.5) * 0.25, random(fx) * 0.125, (random(fx) - 0.5) * 0.25];
        fx.sparks.push(Spark { prev: p, at: p, vel, pop, born: Instant::now() });
    }
}

fn mob_feet(game: &Game<'_>, id: u32) -> Option<[f64; 3]> {
    game.mobs.iter().find(|mob| mob.id == id).map(|mob| [mob.position.x, mob.position.y, mob.position.z])
}

// Your attack on a mob: Hit particles off it.
pub(super) fn sent(game: &Game<'_>, packet: &S) {
    let S::Attack(a) = packet else {
        return;
    };
    let (Some(m), Ok(id)) = (module().filter(|m| m.b("Hit particles")), u32::try_from(a.id)) else {
        return;
    };
    if let Some(at) = mob_feet(game, id) {
        st(|fx| sparks_at(fx, &m, at, m.n("Particle amount") as usize, false));
    }
}

// A player's totem pop (event 35): twice the sparks; a shield put out (event 30): remembered five seconds for ESP.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    let C::EntityEvent(e) = packet else {
        return;
    };
    let Ok(id) = u32::try_from(e.id) else {
        return;
    };
    let player = game.mobs.iter().any(|mob| mob.id == id && mob.kind.id == "minecraft:player");
    if e.event == 30 && player {
        let until = game.ticks + 100;
        st(|fx| {
            fx.broken.retain(|(i, _)| *i != id);
            fx.broken.push((id, until));
        });
    }
    if e.event == 35 {
        st(|fx| match fx.pops.iter_mut().find(|(i, _)| *i == id) {
            Some((_, n)) => *n += 1,
            None => fx.pops.push((id, 1)),
        });
    }
    if e.event == 35
        && player
        && let Some(m) = module().filter(|m| m.b("Hit particles") && m.b("On totem pops"))
        && let Some(at) = mob_feet(game, id)
    {
        st(|fx| sparks_at(fx, &m, at, m.n("Particle amount") as usize * 2, true));
    }
}

// ESP's shield colours: a player's shield put out by an axe in the last five seconds.
pub(super) fn shield_broken(game: &Game<'_>, id: u32) -> bool {
    st(|fx| fx.broken.iter().any(|&(i, until)| i == id && game.ticks < until))
}

// ---- the tick: jumps, trail marks, spark physics

fn solid(game: &Game<'_>, p: [f64; 3]) -> bool {
    let b = p.map(|v| v.floor() as i32);
    !super::sunset_util::replaceable(game, b)
}

// How many totems a player or mob has popped since they appeared (a respawn or rejoin starts again).
pub(super) fn pops(id: u32) -> u32 {
    st(|fx| fx.pops.iter().find(|(i, _)| *i == id).map_or(0, |(_, n)| *n))
}

pub(super) fn tick(game: &Game<'_>) {
    st(|fx| {
        fx.broken.retain(|&(_, until)| game.ticks < until);
        fx.pops.retain(|(id, _)| game.mobs.iter().any(|mob| mob.id == *id && mob.alive()));
    });
    let Some(m) = module().filter(|_| game.player.is_some()) else {
        st(|fx| {
            let broken = std::mem::take(&mut fx.broken);
            *fx = Fx { broken, roll: fx.roll, ..Fx::default() };
        });
        return;
    };
    let me = own_id(game);
    let mut players: Vec<(u32, [f64; 3], bool, f32)> = Vec::new();
    if let (Some(p), Some(id)) = (game.player.as_ref(), me) {
        players.push((id, [p.position.x, p.position.y, p.position.z], p.ground.on_ground, game.yaw));
    }
    players.extend(
        game.mobs
            .iter()
            .filter(|mob| mob.kind.id == "minecraft:player" && Some(mob.id) != me && mob.alive() && !mob.removed)
            .map(|mob| (mob.id, [mob.position.x, mob.position.y, mob.position.z], mob.on_ground, mob.yaw)),
    );
    let now = Instant::now();
    st(|fx| {
        let mut seen = Vec::with_capacity(players.len());
        for &(id, at, ground, yaw) in &players {
            let own = Some(id) == me;
            // Jump circles: off the ground since last tick and going up.
            let before = fx.last.iter().find(|l| l.0 == id).copied();
            if m.b("Jump circles")
                && (own || !m.b("Circles self only"))
                && let Some((_, was_ground, was_y)) = before
                && was_ground
                && !ground
                && at[1] - was_y > 0.05
            {
                fx.circles.push(Circle { at: [at[0], was_y + 0.01, at[2]], born: now, life: (m.n("Circle duration (ms)") / 1000.0) as f32 });
            }
            seen.push((id, ground, at[1]));
            // Trails: a mark each so far walked on the ground.
            if m.b("Trails") && (own || !m.b("Trails self only")) && ground {
                let kind = match m.p("Trail mode") {
                    "Ring" => Trail::Ring,
                    "Particles" => Trail::Spark,
                    _ => Trail::Footstep,
                };
                let spacing = match kind {
                    Trail::Spark => 0.25,
                    Trail::Ring => 0.65,
                    Trail::Footstep => 0.75,
                };
                let life = (m.n("Trail duration (ms)") / 1000.0) as f32;
                match fx.steps.iter().position(|s| s.0 == id) {
                    None => fx.steps.push((id, at, false)),
                    Some(i) if ((at[0] - fx.steps[i].1[0]).powi(2) + (at[2] - fx.steps[i].1[2]).powi(2)).sqrt() >= spacing => {
                        let left = fx.steps[i].2;
                        match kind {
                            Trail::Footstep => {
                                let (s, c) = (f64::from(yaw.to_radians().sin()), f64::from(yaw.to_radians().cos()));
                                let side = if left { -0.18 } else { 0.18 };
                                let foot = [at[0] + c * side, at[1] + 0.02, at[2] + s * side];
                                fx.marks.push(Mark { at: foot, yaw, kind, vel: [0.0; 3], shade: 0.5, born: now, life });
                            }
                            Trail::Spark => {
                                for _ in 0..4 {
                                    let a = random(fx) * std::f64::consts::TAU;
                                    let speed = 0.025 + random(fx) * 0.05;
                                    let vel = [a.cos() * speed, 0.05 + random(fx) * 0.05, a.sin() * speed];
                                    let shade = random(fx) as f32;
                                    fx.marks.push(Mark { at, yaw, kind, vel, shade, born: now, life });
                                }
                            }
                            Trail::Ring => {
                                let shade = random(fx) as f32;
                                fx.marks.push(Mark { at: [at[0], at[1] + 0.01, at[2]], yaw, kind, vel: [0.0; 3], shade, born: now, life });
                            }
                        }
                        fx.steps[i] = (id, at, !left);
                    }
                    Some(_) => {}
                }
            }
        }
        fx.last = seen;
        fx.steps.retain(|s| players.iter().any(|p| p.0 == s.0));
        fx.circles.retain(|c| c.born.elapsed().as_secs_f32() < c.life);
        fx.marks.retain(|k| k.born.elapsed().as_secs_f32() < k.life);
        // Sparks fall slowly, bounce off blocks (with Particle physics) and go after Particle lifespan or 20 blocks off.
        let life = m.n("Particle lifespan (s)") as f32;
        let physics = m.b("Particle physics");
        let eye = game.eye_position().unwrap_or([0.0; 3]);
        for s in &mut fx.sparks {
            s.prev = s.at;
            if physics {
                if solid(game, [s.at[0], s.at[1], s.at[2] + s.vel[2]]) {
                    s.vel[2] *= -0.8;
                }
                if solid(game, [s.at[0], s.at[1] + s.vel[1], s.at[2]]) {
                    s.vel = [s.vel[0] * 0.999, s.vel[1] * -0.6, s.vel[2] * 0.999];
                }
                if solid(game, [s.at[0] + s.vel[0], s.at[1], s.at[2]]) {
                    s.vel[0] *= -0.8;
                }
            }
            s.at = [s.at[0] + s.vel[0], s.at[1] + s.vel[1], s.at[2] + s.vel[2]];
            s.vel[1] -= if s.pop { 0.01375 } else { 0.00375 };
        }
        fx.sparks.retain(|s| s.born.elapsed().as_secs_f32() < life && super::sunset_util::dist(eye, s.at) <= 20.0);
    });
}

// ---- weather

fn new_drop(game: &Game<'_>, fx: &mut Fx, mode: &str, around: [f64; 3], radius: f64) -> Option<Drop> {
    let a = random(fx) * std::f64::consts::TAU;
    let d = random(fx).sqrt() * radius;
    let (x, z) = (around[0] + a.cos() * d, around[2] + a.sin() * d);
    let r = |fx: &mut Fx| random(fx) - 0.5;
    let (y, vel, life) = match mode {
        "Snow" => (around[1] - 3.0 + random(fx) * 23.0, [r(fx) * 0.02, -0.06, r(fx) * 0.02], 600.0),
        "Fireflies" => (around[1] - 1.0 + random(fx) * 6.0, [r(fx) * 0.015, r(fx) * 0.01, r(fx) * 0.015], 400.0),
        "Embers" => (around[1] - 4.0 + random(fx) * 12.0, [r(fx) * 0.01, 0.05 + random(fx) * 0.02, r(fx) * 0.01], 300.0),
        _ => (around[1] - 4.0 + random(fx) * 28.0, [0.0, -0.45, 0.0], 200.0),
    };
    let at = [x, y, z];
    // Rain and snow only fall under open sky.
    if matches!(mode, "Rain" | "Snow") && (1..=32).any(|k| solid(game, [x, y + f64::from(k), z])) {
        return None;
    }
    if solid(game, at) {
        return None;
    }
    let (phase, shade) = ((random(fx) * std::f64::consts::TAU) as f32, random(fx) as f32);
    Some(Drop { at, vel, age: (random(fx) * life * 0.5) as f32, life: life as f32, phase, shade })
}

fn weather(game: &Game<'_>, fx: &mut Fx, m: &Module, dt: f32) {
    let Some(eye) = game.eye_position() else {
        return;
    };
    let (mode, density, radius) = (m.p("Weather mode"), m.n("Density") as usize, m.n("Weather radius"));
    let refill = fx.drops.is_empty() || fx.weather_from.is_some_and(|w| super::sunset_util::dist(w, eye) > 16.0);
    fx.weather_from = Some(eye);
    let tries = if refill { density * 6 } else { (dt * (density / 20).max(1) as f32).ceil() as usize };
    for _ in 0..tries {
        if fx.drops.len() >= density {
            break;
        }
        if let Some(d) = new_drop(game, fx, mode, eye, radius) {
            fx.drops.push(d);
        }
    }
    let step = f64::from(dt) * m.n("Weather speed");
    for d in &mut fx.drops {
        d.at = [d.at[0] + d.vel[0] * step, d.at[1] + d.vel[1] * step, d.at[2] + d.vel[2] * step];
        d.age += dt * m.n("Weather speed") as f32;
    }
    let far = radius * 1.6;
    fx.drops.retain(|d| d.age <= d.life && super::sunset_util::dist(d.at, eye) <= far && !solid(game, d.at));
}

// ---- the frame

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas) {
    let Some(m) = mods.iter().find(|m| m.name == NAME && m.on) else {
        return;
    };
    let rgb = colour(m);
    let (bright, dark) = (shade(rgb, 0.3), shade(rgb, -0.3));
    let partial = game.clock.camera_fraction();
    st(|fx| {
        let now = Instant::now();
        let dt = fx.frame.map_or(0.0, |f| (now - f).as_secs_f32() * 20.0).min(4.0);
        fx.frame = Some(now);
        if m.b("Weather") {
            weather(game, fx, m, dt);
        } else {
            fx.drops.clear();
        }
        let ease = |f: f32| 1.0 - (1.0 - f) * (1.0 - f);
        // Jump circles: grow to Circle radius, in over the first fifth of their time, out over the rest.
        let (thick, glow_w, radius) = (m.n("Circle thickness") as f32, m.n("Circle glow") as f32, m.n("Circle radius") as f32);
        for c in &fx.circles {
            let f = c.born.elapsed().as_secs_f32() / c.life;
            let a = if f < 0.2 { f / 0.2 } else { 1.0 - (f - 0.2) / 0.8 };
            ring(cam, out, c.at, 0.1 + (radius - 0.1) * ease(f), thick, glow_w, bright, a.clamp(0.0, 1.0));
        }
        let size = m.n("Trail size") as f32;
        for k in &fx.marks {
            let f = k.born.elapsed().as_secs_f32() / k.life;
            let a = 1.0 - f * f;
            match k.kind {
                Trail::Footstep => {
                    oval(cam, out, k.at, k.yaw, [0.25 * size, 0.42 * size], alpha(shade(rgb, -0.4), a * 0.45));
                    oval(cam, out, k.at, k.yaw, [0.16 * size, 0.3 * size], alpha(rgb, a));
                }
                Trail::Ring => ring(cam, out, k.at, (0.05 + 0.55 * ease(f)) * size, 0.018, 0.12, mix(dark, bright, k.shade), a),
                Trail::Spark => {
                    let t = f64::from(k.life * f * 20.0);
                    let at = [k.at[0] + k.vel[0] * t, k.at[1] + k.vel[1] * t - 0.0009 * t * t + 0.05, k.at[2] + k.vel[2] * t];
                    if let (Some(p), Some(px)) = (cam.project(at), cam.pixels_per_block(at)) {
                        glow(out, p, (1.6 - 0.5 * f) * 0.05 * size * px, alpha(mix(dark, bright, k.shade), a));
                    }
                }
            }
        }
        let life = m.n("Particle lifespan (s)") as f32;
        let t = fx.epoch.get_or_insert(now).elapsed().as_secs_f32();
        for (i, s) in fx.sparks.iter().enumerate() {
            let at = [0, 1, 2].map(|k| s.prev[k] + (s.at[k] - s.prev[k]) * partial);
            let (Some(p), Some(px)) = (cam.project(at), cam.pixels_per_block(at)) else {
                continue;
            };
            let f = s.born.elapsed().as_secs_f32() / life;
            let wave = ((t * 4.0 + i as f32 * 0.55).sin() + 1.0) * 0.5;
            glow(out, p, if s.pop { 0.14 } else { 0.09 } * px, alpha(mix(dark, bright, wave), 1.0 - f));
        }
        let mode = m.p("Weather mode");
        let wsize = m.n("Weather size") as f32 * 0.04;
        let reach = m.n("Weather radius") * 1.5;
        for d in &fx.drops {
            if super::sunset_util::dist(d.at, cam.eye()) > reach {
                continue;
            }
            let f = d.age / d.life;
            let mut a = if f < 0.1 { f / 0.1 } else if f > 0.7 { (1.0 - f) / 0.3 } else { 1.0 };
            a *= match mode {
                "Fireflies" => 0.35 + 0.65 * ((d.age * 0.12 + d.phase).sin() * 0.5 + 0.5),
                "Embers" => 0.6 + 0.4 * ((d.age * 0.4 + d.phase).sin() * 0.5 + 0.5),
                _ => 1.0,
            };
            let tint = if m.p("Color") == "Custom" { rgb } else { mix(dark, bright, d.shade) };
            let (Some(p), Some(px)) = (cam.project(d.at), cam.pixels_per_block(d.at)) else {
                continue;
            };
            match mode {
                "Rain" => {
                    let w = (0.5 * wsize * px).max(1.0);
                    let h = 4.5 * wsize * px;
                    out.fill([p[0] - w / 2.0, p[1] - h, w, h], alpha(tint, a * 0.8));
                }
                "Snow" => disc(out, p, 0.8 * wsize * px, alpha(tint, a)),
                "Fireflies" => glow(out, p, 1.25 * wsize * px * 0.5, alpha(tint, a)),
                _ => glow(out, p, 0.7 * wsize * px * 0.5, alpha(tint, a)),
            }
        }
    });
}
