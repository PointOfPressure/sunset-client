// Effects: visuals in the world that only you see. Jump circles (a ring grows where a player jumps), Trails
// (footsteps, rings or sparks where players walk), Hit particles (glowing sparks off what you hit, and off a totem
// pop), Landing ripples (a ring spreads where a player lands from a fall), Explosion shockwaves (a ring spreads from
// an explosion's centre) and Weather (rain, snow, fireflies or embers round you); Kill effect (a burst or a pillar of
// light where what you hit dies), Crit sparks (stars off your critical hits), Break bursts (chips off a block as it
// breaks) and Target aura (a ring or two lights round what you last hit). Drawn on the HUD as projected shapes;
// nothing is sent. With Hide behind blocks an effect a block stands in front of is not drawn, and everything fades
// with distance. Trail marks and weather are too many to test against the blocks every frame: each keeps its last
// answer and a share of the ones on screen is asked again each frame (see Turns). A dot too small to show its
// halo is one quad. Also the shield breaks and totem pops ESP and Nametags colour by.
use super::Game;
use super::sunset::{Camera, Canvas, Category, Module, accent, own_id, with};
use horizon_protocol::packets::play::{Clientbound as C, Serverbound as S};
use std::f32::consts::TAU;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

pub(super) const NAME: &str = "Effects";

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Render, "Jump circles, trails, hit particles, shockwaves and weather.")
            .group("Jump circles")
            .bool("Jump circles", true)
            .bool("Circles self only", false)
            .pick("Circle style", &["Single", "Double"])
            .num("Circle radius", 1.0, 0.2, 5.0, 0.1)
            .num("Circle duration (ms)", 1200.0, 200.0, 4000.0, 100.0)
            .num("Circle thickness", 0.02, 0.005, 0.1, 0.005)
            .num("Circle glow", 0.15, 0.0, 0.5, 0.01)
            .group("Trails")
            .bool("Trails", false)
            .pick("Trail mode", &["Footsteps", "Ring", "Particles"])
            .bool("Trails self only", true)
            .num("Trail duration (ms)", 1500.0, 200.0, 5000.0, 100.0)
            .num("Trail size", 1.0, 0.2, 3.0, 0.1)
            .group("Hit particles")
            .bool("Hit particles", false)
            .pick("Particle shape", &["Glow", "Diamond"])
            .num("Particle size", 1.0, 0.25, 2.0, 0.05)
            .num("Particle amount", 10.0, 1.0, 20.0, 1.0)
            .num("Particle lifespan (s)", 10.0, 1.0, 60.0, 1.0)
            .bool("Particle physics", true)
            .bool("On totem pops", true)
            .group("Landing ripples")
            .bool("Landing ripples", false)
            .bool("Landings self only", true)
            .num("Minimum fall", 0.6, 0.4, 4.0, 0.1)
            .num("Landing radius", 1.5, 0.4, 4.0, 0.1)
            .num("Landing duration (ms)", 650.0, 200.0, 2000.0, 50.0)
            .group("Explosion shockwaves")
            .bool("Explosion shockwaves", false)
            .num("Shockwave scale", 1.0, 0.5, 2.0, 0.1)
            .num("Shockwave duration (ms)", 850.0, 250.0, 2000.0, 50.0)
            .group("Weather")
            .bool("Weather", false)
            .pick("Weather mode", &["Rain", "Snow", "Fireflies", "Embers"])
            .num("Density", 200.0, 10.0, 1500.0, 10.0)
            .num("Weather radius", 50.0, 5.0, 100.0, 5.0)
            .num("Weather speed", 1.0, 0.1, 5.0, 0.1)
            .num("Weather size", 3.0, 0.1, 5.0, 0.1)
            .group("Kill effect")
            .bool("Kill effect", false)
            .pick("Kill style", &["Burst", "Pillar"])
            .group("Crit sparks")
            .bool("Crit sparks", false)
            .group("Break bursts")
            .bool("Break bursts", false)
            .group("Target aura")
            .bool("Target aura", false)
            .pick("Aura style", &["Ring", "Orbit"])
            .num("Aura time (s)", 3.0, 1.0, 10.0, 0.5)
            .group("Color")
            .pick("Color", &["Accent", "Custom"])
            .colour("Custom color", "Purple")
            .bool("Hide behind blocks", true),
    ]
}

pub(super) fn visible(m: &Module, name: &str) -> bool {
    match name {
        "Circles self only" | "Circle style" | "Circle radius" | "Circle duration (ms)" | "Circle thickness" | "Circle glow" => m.b("Jump circles"),
        "Trail mode" | "Trails self only" | "Trail duration (ms)" | "Trail size" => m.b("Trails"),
        "Landings self only" | "Minimum fall" | "Landing radius" | "Landing duration (ms)" => m.b("Landing ripples"),
        "Shockwave scale" | "Shockwave duration (ms)" => m.b("Explosion shockwaves"),
        "Particle shape" | "Particle size" | "Particle amount" | "Particle lifespan (s)" | "Particle physics" | "On totem pops" => m.b("Hit particles"),
        "Weather mode" | "Density" | "Weather radius" | "Weather speed" | "Weather size" => m.b("Weather"),
        "Kill style" => m.b("Kill effect"),
        "Aura style" | "Aura time (s)" => m.b("Target aura"),
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
    // Under a pixel and a half across no corner shows: one quad instead of six.
    if r < 1.5 {
        out.fill([x - r, y - r, 2.0 * r, 2.0 * r], argb);
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
    // Too small for a halo to show (far weather, far sparks): the core alone, one quad, not eighteen.
    if r < 1.2 {
        disc(out, at, (r * 1.1).max(0.6), with(0.9));
        return;
    }
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
    // Project each rim point once, and share the edges between the core and glow bands.
    let edges = |r: f32| -> [Option<[f32; 2]>; N] {
        std::array::from_fn(|k| {
            let (sin, cos) = (k as f64 * std::f64::consts::TAU / N as f64).sin_cos();
            cam.project([at[0] + f64::from(r) * cos, at[1], at[2] + f64::from(r) * sin])
        })
    };
    let inner = (radius - thickness / 2.0).max(0.0);
    let outer = radius + thickness / 2.0;
    let inner_edge = edges(inner);
    let outer_edge = edges(outer);
    let mut band = |lo: &[_; N], hi: &[_; N], opacity| {
        let color = alpha(rgb, opacity);
        for k in 0..N {
            let n = (k + 1) % N;
            if let (Some(p), Some(q), Some(r), Some(s)) = (lo[k], hi[k], hi[n], lo[n]) {
                out.quad([p, q, r, s], color);
            }
        }
    };
    band(&inner_edge, &outer_edge, a);
    if glow > 0.0 {
        band(&edges((inner - glow).max(0.0)), &inner_edge, a * 0.3);
        band(&outer_edge, &edges(outer + glow), a * 0.3);
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

struct PlayerTrack {
    id: u32,
    at: [f64; 3],
    ground: bool,
    peak: f64,
}

struct Pulse {
    at: [f64; 3],
    born: Instant,
    radius: f32,
    life: f32,
}

fn keep_recent<T>(queue: &mut VecDeque<T>, item: T, limit: usize) {
    if queue.len() >= limit {
        queue.pop_front();
    }
    queue.push_back(item);
}

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
    // No block in front of it when it was last tested (false until the first test).
    seen: bool,
}

struct Spark {
    prev: [f64; 3],
    at: [f64; 3],
    vel: [f64; 3],
    pop: bool,
    born: Instant,
}

// A piece of a burst (Kill effect, Crit sparks, Break bursts): it flies, falls and fades.
struct Bit {
    prev: [f64; 3],
    at: [f64; 3],
    vel: [f64; 3],
    born: Instant,
    life: f32,
    // Blocks across.
    size: f32,
    shape: Shape,
    shade: f32,
    gravity: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Glow,
    Star,
    Chip,
}

struct Kill {
    at: [f64; 3],
    born: Instant,
    pillar: bool,
}

const KILL_LIFE: f32 = 1.1;
const BITS: usize = 320;

struct Drop {
    at: [f64; 3],
    vel: [f64; 3],
    age: f32,
    life: f32,
    phase: f32,
    shade: f32,
    // As Mark's.
    seen: bool,
}

// Whose turn it is to be tested against the blocks this frame: `left` of them, on from where the last frame
// stopped, so every trail mark and weather drop on screen is asked again within a few frames whatever their number.
struct Turns {
    from: usize,
    left: usize,
    next: usize,
}

impl Turns {
    // `dt` in ticks since the last frame: `rate` tests a tick, never under 16 or over `most` a frame.
    fn new(from: usize, dt: f32, rate: f32, most: usize) -> Self {
        Self { from, left: ((dt * rate) as usize).clamp(16, most), next: 0 }
    }

    fn take(&mut self, i: usize) -> bool {
        let mine = i >= self.from && self.left > 0;
        if mine {
            self.left -= 1;
            self.next = i + 1;
        }
        mine
    }

    // Where the next frame goes on: from the start again once this one reached the end.
    fn end(self) -> usize {
        if self.left > 0 { 0 } else { self.next }
    }
}

#[derive(Default)]
struct Fx {
    // Players last tick: on the ground, and their y; where the last trail mark went, and which foot.
    last: Vec<PlayerTrack>,
    steps: Vec<(u32, [f64; 3], bool)>,
    circles: VecDeque<Circle>,
    marks: VecDeque<Mark>,
    landings: VecDeque<Pulse>,
    explosions: VecDeque<Pulse>,
    sparks: Vec<Spark>,
    drops: Vec<Drop>,
    // Where the block tests of the trail marks and of the weather go on next frame.
    mark_turn: usize,
    drop_turn: usize,
    weather_from: Option<[f64; 3]>,
    weather_mode: Option<&'static str>,
    frame: Option<Instant>,
    epoch: Option<Instant>,
    // A player's shield put out by an axe: until when (tick).
    broken: Vec<(u32, u64)>,
    // Each player's or mob's totem pops (event 35) since they appeared: Nametags' Totem pops.
    pops: Vec<(u32, u32)>,
    roll: u64,
    // What you hit, newest last: Target aura's target, and whose death is your kill or crit.
    hits: VecDeque<(u32, Instant)>,
    bits: Vec<Bit>,
    kills: VecDeque<Kill>,
    // A block you began to break, and when (tick): gone within two ticks, it broke at once.
    digging: Option<([f64; 3], u64)>,
    // The last Crit sparks: on whom and when (a server's crit packet after your own hit is the same crit).
    crit: Option<(u32, Instant)>,
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
        if fx.sparks.len() >= cap {
            break;
        }
        let p = [at[0] + (random(fx) - 0.5) * 0.5, at[1] + random(fx) + 0.5, at[2] + (random(fx) - 0.5) * 0.5];
        // Blocks a tick: up to an eighth across, up to an eighth up.
        let vel = [(random(fx) - 0.5) * 0.25, random(fx) * 0.125, (random(fx) - 0.5) * 0.25];
        fx.sparks.push(Spark { prev: p, at: p, vel, pop, born: Instant::now() });
    }
}

// A burst of count bits from at, flying out at up to speed blocks a tick.
fn burst(fx: &mut Fx, at: [f64; 3], count: usize, speed: f64, up: f64, size: f32, life: f32, shape: Shape, gravity: f64) {
    for _ in 0..count {
        if fx.bits.len() >= BITS {
            break;
        }
        let a = random(fx) * std::f64::consts::TAU;
        let v = speed * (0.35 + 0.65 * random(fx));
        let vel = [a.cos() * v, up * (0.4 + 0.6 * random(fx)), a.sin() * v];
        let shade = random(fx) as f32;
        let life = life * (0.6 + 0.4 * random(fx) as f32);
        fx.bits.push(Bit { prev: at, at, vel, born: Instant::now(), life, size, shape, shade, gravity });
    }
}

// You hit it in the last secs seconds.
fn hit_lately(fx: &Fx, id: u32, secs: f32) -> bool {
    fx.hits.iter().any(|(i, at)| *i == id && at.elapsed().as_secs_f32() <= secs)
}

// A block stands between the eye and a place: each block the line passes through is looked at once, up to 48
// blocks. Not the eye's own block, and not the last two thirds of a block: an effect on the ground or on a block's
// face is not behind that block.
fn hidden(game: &Game<'_>, eye: [f64; 3], at: [f64; 3]) -> bool {
    let d = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len.is_nan() || len > 48.0 {
        return true;
    }
    // Along the line, 0 at the eye and 1 at the place: where it stops, how far one block is on each axis, and
    // where it next crosses into another block on each.
    let end = 1.0 - 0.66 / len;
    if end <= 0.0 {
        return false;
    }
    let mut cell = eye.map(|v| v.floor() as i32);
    let per = d.map(|v| if v == 0.0 { f64::INFINITY } else { (1.0 / v).abs() });
    let mut next = [0, 1, 2].map(|k| {
        let edge = if d[k] > 0.0 { f64::from(cell[k]) + 1.0 - eye[k] } else { eye[k] - f64::from(cell[k]) };
        if d[k] == 0.0 { f64::INFINITY } else { edge * per[k] }
    });
    for _ in 0..160 {
        let k = if next[0] <= next[1] && next[0] <= next[2] { 0 } else if next[1] <= next[2] { 1 } else { 2 };
        if next[k] >= end {
            return false;
        }
        cell[k] += if d[k] > 0.0 { 1 } else { -1 };
        next[k] += per[k];
        if !super::sunset_util::replaceable(game, cell) {
            return true;
        }
    }
    false
}

// Light falls off with distance: full to 20 blocks, gone at 44.
fn falloff(eye: [f64; 3], at: [f64; 3]) -> f32 {
    (1.0 - (super::sunset_util::dist(eye, at) as f32 - 20.0) / 24.0).clamp(0.0, 1.0)
}

fn mob_feet(game: &Game<'_>, id: u32) -> Option<[f64; 3]> {
    game.mobs.iter().find(|mob| mob.id == id).map(|mob| [mob.position.x, mob.position.y, mob.position.z])
}

// Your attack on a mob: Hit particles off it.
pub(super) fn sent(game: &Game<'_>, packet: &S) {
    // A block you finished breaking (PlayerAction 3): Break bursts' chips off it. One that breaks at once only ever
    // sends its start (0): tick sees whether the block went.
    if let S::PlayerAction(p) = packet {
        if matches!(p.action, 0 | 3) && module().is_some_and(|m| m.b("Break bursts")) {
            let at = [f64::from(p.pos[0]) + 0.5, f64::from(p.pos[1]) + 0.5, f64::from(p.pos[2]) + 0.5];
            st(|fx| {
                if p.action == 3 {
                    fx.digging = None;
                    burst(fx, at, 14, 0.09, 0.14, 0.11, 1.6, Shape::Chip, 0.022);
                } else {
                    fx.digging = Some((at, game.ticks));
                }
            });
        }
        return;
    }
    let S::Attack(a) = packet else {
        return;
    };
    let (Some(m), Ok(id)) = (module(), u32::try_from(a.id)) else {
        return;
    };
    st(|fx| {
        fx.hits.retain(|(i, _)| *i != id);
        keep_recent(&mut fx.hits, (id, Instant::now()), 8);
    });
    if m.b("Hit particles")
        && let Some(at) = mob_feet(game, id)
    {
        st(|fx| sparks_at(fx, &m, at, m.n("Particle amount") as usize, false));
    }
    // Crit sparks, as you see the hit: falling, off the ground, not gliding, swimming or climbing (the game's own
    // rule for a critical hit, less the charge, which the hit has already spent). Horizon's servers send no crit
    // animation; a vanilla or Paper server's (packet) is not counted twice.
    if m.b("Crit sparks")
        && game.player.as_ref().is_some_and(|p| {
            !p.ground.on_ground && p.velocity.y < 0.0 && !p.fall_flying && !p.fluids.was_touching_water && p.last_climbable.is_none()
        })
        && game.riding.is_none()
    {
        crit_stars(game, id);
    }
}

fn crit_stars(game: &Game<'_>, id: u32) {
    let Some(mob) = game.mobs.iter().find(|mob| mob.id == id) else {
        return;
    };
    let b = mob.bounds();
    let at = [mob.position.x, mob.position.y + (b.max_position().y - b.min_position().y) * 0.6, mob.position.z];
    st(|fx| {
        if fx.crit.is_some_and(|(i, since)| i == id && since.elapsed().as_secs_f32() < 0.5) {
            return;
        }
        fx.crit = Some((id, Instant::now()));
        burst(fx, at, 12, 0.11, 0.1, 0.13, 1.4, Shape::Star, 0.012);
    });
}

// A player's totem pop (event 35): twice the sparks; a shield put out (event 30): remembered five seconds for ESP.
pub(super) fn packet(game: &Game<'_>, packet: &C) {
    if matches!(packet, C::Login(_) | C::Respawn(_) | C::Disconnect(_)) {
        st(|fx| *fx = Fx::default());
        return;
    }
    if let C::Explode(blast) = packet {
        if let Some(m) = module().filter(|m| m.b("Explosion shockwaves"))
            && let Some(eye) = game.eye_position()
            && super::sunset_util::dist(eye, blast.center) <= 48.0
            && blast.radius.is_finite() && blast.radius > 0.0
        {
            let radius = (blast.radius.clamp(0.7, 6.0) * m.n("Shockwave scale") as f32).min(10.0);
            let life = (m.n("Shockwave duration (ms)") / 1000.0) as f32;
            st(|fx| keep_recent(&mut fx.explosions, Pulse { at: blast.center, born: Instant::now(), radius, life }, 24));
        }
        return;
    }
    // A critical hit's stars (Animate 4, and 5 for an enchanted weapon's) on what you just hit: Crit sparks.
    if let C::Animate(a) = packet {
        if matches!(a.action, 4 | 5)
            && let Ok(id) = u32::try_from(a.id)
            && module().is_some_and(|m| m.b("Crit sparks"))
            && st(|fx| hit_lately(fx, id, 1.5))
        {
            crit_stars(game, id);
        }
        return;
    }
    // A block broken near you by someone else (level event 2001): Break bursts.
    if let C::LevelEvent(e) = packet {
        if e.kind == 2001
            && module().is_some_and(|m| m.b("Break bursts"))
            && let Some(eye) = game.eye_position()
        {
            let at = [f64::from(e.pos[0]) + 0.5, f64::from(e.pos[1]) + 0.5, f64::from(e.pos[2]) + 0.5];
            if super::sunset_util::dist(eye, at) <= 24.0 {
                st(|fx| burst(fx, at, 10, 0.08, 0.13, 0.1, 1.4, Shape::Chip, 0.022));
            }
        }
        return;
    }
    let C::EntityEvent(e) = packet else {
        return;
    };
    let Ok(id) = u32::try_from(e.id) else {
        return;
    };
    // What you hit in the last five seconds died (event 3): Kill effect.
    if e.event == 3
        && let Some(m) = module().filter(|m| m.b("Kill effect"))
        && let Some(at) = mob_feet(game, id)
    {
        let pillar = m.p("Kill style") == "Pillar";
        st(|fx| {
            if hit_lately(fx, id, 5.0) {
                keep_recent(&mut fx.kills, Kill { at, born: Instant::now(), pillar }, 12);
                let mid = [at[0], at[1] + 0.9, at[2]];
                if pillar {
                    burst(fx, mid, 10, 0.03, 0.22, 0.1, 1.1, Shape::Glow, -0.002);
                } else {
                    burst(fx, mid, 26, 0.16, 0.2, 0.12, 1.0, Shape::Glow, 0.014);
                }
            }
        });
    }
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
            let pops = std::mem::take(&mut fx.pops);
            *fx = Fx { broken, pops, roll: fx.roll, ..Fx::default() };
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
            let before = fx.last.iter().find(|last| last.id == id)
                .filter(|last| super::sunset_util::dist(last.at, at) <= 4.0);
            let peak = before.map_or(at[1], |last| if last.ground { at[1] } else { last.peak.max(at[1]) });
            if let Some(last) = before {
                if m.b("Jump circles") && (own || !m.b("Circles self only"))
                    && last.ground && !ground && at[1] - last.at[1] > 0.05
                {
                    let at = [at[0], last.at[1] + 0.01, at[2]];
                    let life = (m.n("Circle duration (ms)") / 1000.0) as f32;
                    keep_recent(&mut fx.circles, Circle { at, born: now, life }, 48);
                }
                if m.b("Landing ripples") && (own || !m.b("Landings self only"))
                    && !last.ground && ground && peak - at[1] >= m.n("Minimum fall")
                {
                    let at = [at[0], at[1] + 0.02, at[2]];
                    let radius = m.n("Landing radius") as f32;
                    let life = (m.n("Landing duration (ms)") / 1000.0) as f32;
                    keep_recent(&mut fx.landings, Pulse { at, radius, life, born: now }, 48);
                }
            }
            seen.push(PlayerTrack { id, at, ground, peak: if ground { at[1] } else { peak } });
            // A teleport, airborne step or disabled trail ends the current set of footprints.
            if before.is_none() || !ground || !m.b("Trails") || (!own && m.b("Trails self only")) {
                fx.steps.retain(|s| s.0 != id);
                continue;
            }
            {
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
                                keep_recent(&mut fx.marks, Mark { at: foot, yaw, kind, vel: [0.0; 3], shade: 0.5, born: now, life, seen: false }, 512);
                            }
                            Trail::Spark => {
                                for _ in 0..4 {
                                    let a = random(fx) * std::f64::consts::TAU;
                                    let speed = 0.025 + random(fx) * 0.05;
                                    let vel = [a.cos() * speed, 0.05 + random(fx) * 0.05, a.sin() * speed];
                                    let shade = random(fx) as f32;
                                    keep_recent(&mut fx.marks, Mark { at, yaw, kind, vel, shade, born: now, life, seen: false }, 512);
                                }
                            }
                            Trail::Ring => {
                                let shade = random(fx) as f32;
                                keep_recent(&mut fx.marks, Mark { at: [at[0], at[1] + 0.01, at[2]], yaw, kind, vel: [0.0; 3], shade, born: now, life, seen: false }, 512);
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
        fx.circles.retain(|c| m.b("Jump circles") && c.born.elapsed().as_secs_f32() < c.life);
        fx.marks.retain(|k| m.b("Trails") && k.born.elapsed().as_secs_f32() < k.life);
        fx.landings.retain(|p| m.b("Landing ripples") && p.born.elapsed().as_secs_f32() < p.life);
        fx.explosions.retain(|p| m.b("Explosion shockwaves") && p.born.elapsed().as_secs_f32() < p.life);
        if !m.b("Hit particles") {
            fx.sparks.clear();
        }
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
        // Bursts' bits fly, fall (or rise) and stop on blocks.
        for b in &mut fx.bits {
            b.prev = b.at;
            let next = [b.at[0] + b.vel[0], b.at[1] + b.vel[1], b.at[2] + b.vel[2]];
            if physics && solid(game, next) {
                b.vel = [b.vel[0] * 0.3, b.vel[1] * -0.35, b.vel[2] * 0.3];
            } else {
                b.at = next;
            }
            b.vel = [b.vel[0] * 0.96, b.vel[1] - b.gravity, b.vel[2] * 0.96];
        }
        fx.bits.retain(|b| b.born.elapsed().as_secs_f32() < b.life && super::sunset_util::dist(eye, b.at) <= 48.0);
        if let Some((at, since)) = fx.digging {
            if !solid(game, at) {
                fx.digging = None;
                if m.b("Break bursts") {
                    burst(fx, at, 14, 0.09, 0.14, 0.11, 1.6, Shape::Chip, 0.022);
                }
            } else if game.ticks.saturating_sub(since) > 2 {
                fx.digging = None;
            }
        }
        fx.kills.retain(|k| k.born.elapsed().as_secs_f32() < KILL_LIFE);
        fx.hits.retain(|(_, at)| at.elapsed().as_secs_f32() < 12.0);
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
    Some(Drop { at, vel, age: (random(fx) * life * 0.5) as f32, life: life as f32, phase, shade, seen: false })
}

fn weather(game: &Game<'_>, fx: &mut Fx, m: &Module, dt: f32) {
    let Some(eye) = game.eye_position() else {
        return;
    };
    let (mode, density, radius) = (m.p("Weather mode"), m.n("Density") as usize, m.n("Weather radius"));
    let refill = fx.weather_mode != Some(mode)
        || fx.weather_from.is_none_or(|w| super::sunset_util::dist(w, eye) > 16.0);
    if refill {
        fx.weather_mode = Some(mode);
        fx.drops.clear();
    }
    fx.drops.truncate(density);
    fx.weather_from = Some(eye);
    let tries = if refill { (density * 2).min(360) } else { (dt * (density / 20).max(1) as f32).ceil() as usize };
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
            fx.weather_mode = None;
        }
        let ease = |f: f32| 1.0 - (1.0 - f) * (1.0 - f);
        let eye = cam.eye();
        let occlude = m.b("Hide behind blocks");
        // How much of an effect at a place shows: nothing behind a block, less far off.
        let shown = |at: [f64; 3]| if occlude && hidden(game, eye, at) { 0.0 } else { falloff(eye, at) };
        // Jump circles: grow to Circle radius, in over the first fifth of their time, out over the rest.
        let (thick, glow_w, radius) = (m.n("Circle thickness") as f32, m.n("Circle glow") as f32, m.n("Circle radius") as f32);
        for c in &fx.circles {
            let f = c.born.elapsed().as_secs_f32() / c.life;
            let a = if f < 0.2 { f / 0.2 } else { 1.0 - (f - 0.2) / 0.8 };
            let a = a * shown([c.at[0], c.at[1] + 0.2, c.at[2]]);
            if a <= 0.0 { continue; }
            let r = 0.1 + (radius - 0.1) * ease(f.clamp(0.0, 1.0));
            ring(cam, out, c.at, r, thick, glow_w, bright, a.clamp(0.0, 1.0));
            if m.p("Circle style") == "Double" {
                ring(cam, out, c.at, r * 0.66, thick * 0.7, glow_w * 0.6, rgb, a * 0.65);
            }
        }
        for (waves, setting, thickness) in [
            (&fx.landings, "Landing ripples", 0.035),
            (&fx.explosions, "Explosion shockwaves", 0.07),
        ] {
            if !m.b(setting) { continue; }
            for wave in waves {
                let f = (wave.born.elapsed().as_secs_f32() / wave.life).clamp(0.0, 1.0);
                let a = (1.0 - f).powi(2) * shown([wave.at[0], wave.at[1] + 0.2, wave.at[2]]);
                if a <= 0.0 { continue; }
                let r = wave.radius * (0.16 + 0.84 * ease(f));
                ring(cam, out, wave.at, r, thickness, thickness * 3.5, bright, a);
                ring(cam, out, wave.at, r * 0.7, thickness * 0.55, thickness, rgb, a * 0.7);
            }
        }
        let size = m.n("Trail size") as f32;
        let mut turns = Turns::new(fx.mark_turn, dt, 240.0, 256);
        for (i, k) in fx.marks.iter_mut().enumerate() {
            let f = k.born.elapsed().as_secs_f32() / k.life;
            // Where it is tested: a little over the ground it lies on (a spark, where it has flown to).
            let t = if k.kind == Trail::Spark { f64::from(k.life * f * 20.0) } else { 0.0 };
            let over = [k.at[0] + k.vel[0] * t, k.at[1] + k.vel[1] * t - 0.0009 * t * t + 0.1, k.at[2] + k.vel[2] * t];
            let far = falloff(eye, over);
            // One within reach of your feet is in view; the others are tested in turn while they are on screen.
            if occlude && super::sunset_util::dist(eye, over) > 2.5 {
                let on = far > 0.0
                    && cam.project(over).zip(cam.pixels_per_block(over)).is_some_and(|(p, px)| cam.on_screen(p, px * size));
                if !on {
                    k.seen = false;
                    continue;
                }
                if turns.take(i) {
                    k.seen = !hidden(game, eye, over);
                }
                if !k.seen {
                    continue;
                }
            }
            let a = (1.0 - f * f) * far;
            if a <= 0.0 { continue; }
            match k.kind {
                Trail::Footstep => {
                    oval(cam, out, k.at, k.yaw, [0.25 * size, 0.42 * size], alpha(shade(rgb, -0.4), a * 0.45));
                    oval(cam, out, k.at, k.yaw, [0.16 * size, 0.3 * size], alpha(rgb, a));
                }
                Trail::Ring => ring(cam, out, k.at, (0.05 + 0.55 * ease(f)) * size, 0.018, 0.12, mix(dark, bright, k.shade), a),
                Trail::Spark => {
                    let at = [k.at[0] + k.vel[0] * t, k.at[1] + k.vel[1] * t - 0.0009 * t * t + 0.05, k.at[2] + k.vel[2] * t];
                    if let (Some(p), Some(px)) = (cam.project(at), cam.pixels_per_block(at)) {
                        glow(out, p, (1.6 - 0.5 * f) * 0.05 * size * px, alpha(mix(dark, bright, k.shade), a));
                    }
                }
            }
        }
        fx.mark_turn = turns.end();
        let life = m.n("Particle lifespan (s)") as f32;
        let t = fx.epoch.get_or_insert(now).elapsed().as_secs_f32();
        for (i, s) in fx.sparks.iter().enumerate() {
            let at = [0, 1, 2].map(|k| s.prev[k] + (s.at[k] - s.prev[k]) * partial);
            let (Some(p), Some(px)) = (cam.project(at), cam.pixels_per_block(at)) else {
                continue;
            };
            let f = s.born.elapsed().as_secs_f32() / life;
            let wave = ((t * 4.0 + i as f32 * 0.55).sin() + 1.0) * 0.5;
            let r = ((if s.pop { 0.14 } else { 0.09 }) * px * m.n("Particle size") as f32).min(24.0);
            if !cam.on_screen(p, r * 3.0) { continue; }
            let see = shown(at);
            if see <= 0.0 { continue; }
            let color = alpha(mix(dark, bright, wave), (1.0 - f) * see);
            if m.p("Particle shape") == "Diamond" {
                let [x, y] = p;
                out.quad([[x, y - r], [x + r * 0.7, y], [x, y + r], [x - r * 0.7, y]], color);
            } else {
                glow(out, p, r, color);
            }
        }
        // Bursts' bits: a glow, a four-pointed star or a square chip.
        for b in &fx.bits {
            let at = [0, 1, 2].map(|k| b.prev[k] + (b.at[k] - b.prev[k]) * partial);
            let (Some(p), Some(px)) = (cam.project(at), cam.pixels_per_block(at)) else {
                continue;
            };
            let f = (b.born.elapsed().as_secs_f32() / b.life).clamp(0.0, 1.0);
            let r = (b.size * px * (1.0 - 0.5 * f)).min(28.0);
            if !cam.on_screen(p, r * 3.0) { continue; }
            let see = shown(at);
            if see <= 0.0 { continue; }
            let color = alpha(mix(rgb, bright, b.shade), (1.0 - f * f) * see);
            let [x, y] = p;
            match b.shape {
                Shape::Glow => glow(out, p, r * 0.6, color),
                Shape::Star => {
                    out.quad([[x, y - r], [x + r * 0.22, y], [x, y + r], [x - r * 0.22, y]], color);
                    out.quad([[x - r, y], [x, y - r * 0.22], [x + r, y], [x, y + r * 0.22]], color);
                }
                Shape::Chip => out.fill([x - r * 0.5, y - r * 0.5, r, r], color),
            }
        }
        // Kill effect: a ring spreads from the feet; Pillar adds a column of light that thins as it fades.
        for k in &fx.kills {
            let f = (k.born.elapsed().as_secs_f32() / KILL_LIFE).clamp(0.0, 1.0);
            let see = shown([k.at[0], k.at[1] + 0.9, k.at[2]]);
            if see <= 0.0 { continue; }
            let a = (1.0 - f).powi(2) * see;
            ring(cam, out, [k.at[0], k.at[1] + 0.05, k.at[2]], 0.3 + 1.5 * ease(f), 0.05, 0.16, bright, a);
            if k.pillar {
                let top = [k.at[0], k.at[1] + 3.2, k.at[2]];
                if let (Some(lo), Some(hi), Some(px)) = (cam.project(k.at), cam.project(top), cam.pixels_per_block(k.at)) {
                    for (width, strength) in [(0.34, 0.25), (0.16, 0.6), (0.06, 0.95)] {
                        let w = width * px * (1.0 - 0.7 * f);
                        out.quad([[lo[0] - w, lo[1]], [lo[0] + w, lo[1]], [hi[0] + w, hi[1]], [hi[0] - w, hi[1]]], alpha(bright, a * strength));
                    }
                }
            }
        }
        // Target aura: round what you last hit, for Aura time after the hit.
        if m.b("Target aura")
            && let Some((id, since)) = fx.hits.back().copied()
            && since.elapsed().as_secs_f32() < m.n("Aura time (s)") as f32
            && let Some(mob) = game.mobs.iter().find(|mob| mob.id == id && mob.alive() && !mob.removed)
        {
            let feet = [mob.position.x, mob.position.y, mob.position.z];
            let b = mob.bounds();
            let (wide, tall) = (((b.max_position().x - b.min_position().x) as f32).max(0.6), (b.max_position().y - b.min_position().y) as f32);
            let left = 1.0 - since.elapsed().as_secs_f32() / m.n("Aura time (s)") as f32;
            let see = shown([feet[0], feet[1] + f64::from(tall) * 0.5, feet[2]]) * left.min(0.25) * 4.0;
            if see > 0.0 {
                let t = fx.epoch.get_or_insert(now).elapsed().as_secs_f32();
                if m.p("Aura style") == "Orbit" {
                    for k in 0..2 {
                        let a = f64::from(t * 3.2 + k as f32 * std::f32::consts::PI);
                        let r = f64::from(wide) * 0.85;
                        let at = [feet[0] + a.cos() * r, feet[1] + f64::from(tall * (0.5 + 0.35 * (t * 1.7 + k as f32).sin())), feet[2] + a.sin() * r];
                        if let (Some(p), Some(px)) = (cam.project(at), cam.pixels_per_block(at)) {
                            glow(out, p, 0.09 * px, alpha(bright, see));
                        }
                    }
                } else {
                    let pulse = 0.5 + 0.5 * (t * 5.0).sin();
                    ring(cam, out, [feet[0], feet[1] + 0.06, feet[2]], wide * (0.85 + 0.06 * pulse), 0.035, 0.12, bright, see);
                    ring(cam, out, [feet[0], feet[1] + f64::from(tall) * 0.5, feet[2]], wide * 0.75, 0.02, 0.07, rgb, see * 0.6);
                }
            }
        }
        let mode = m.p("Weather mode");
        let wsize = m.n("Weather size") as f32 * 0.04;
        let reach = m.n("Weather radius") * 1.5;
        let mut turns = Turns::new(fx.drop_turn, dt, 300.0, 600);
        for (i, d) in fx.drops.iter_mut().enumerate() {
            if super::sunset_util::dist(d.at, eye) > reach {
                d.seen = false;
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
            // Off screen: nothing to draw (most of the drops behind and beside you). Far ones are fainter.
            a *= falloff(eye, d.at);
            let on = if a > 0.0 { cam.project(d.at).zip(cam.pixels_per_block(d.at)) } else { None };
            let Some((p, px)) = on.filter(|&(p, _)| cam.on_screen(p, 8.0)) else {
                d.seen = false;
                continue;
            };
            // Behind a block: the answer of its last turn (a drop new on the screen waits for its first).
            if occlude {
                if turns.take(i) {
                    d.seen = !hidden(game, eye, d.at);
                }
                if !d.seen {
                    continue;
                }
            }
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
        fx.drop_turn = turns.end();
    });
}
