// Target ESP: a light marker on the one target you are fighting (Kill Aura's, or what the crosshair is on): corner
// brackets round it, or (Style) Ghost, glowing trails circling it, or Square, a turning square on it; those two fade
// in and out and follow the target smoothly.

use super::Game;
use super::remote_chunks::ChunkFormat;
use super::sunset::{Camera, Canvas, Category, Module, accent, gui_open, own_id};
use super::sunset_target_esp_math::{brackets, clip, eligible, sightline};
use super::sunset_effects::{alpha, glow, mix, shade};
use super::sunset_util::dist;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(
            "Target ESP",
            Category::Render,
            "Marks your target.",
        )
        .pick("Target", &["Combat or crosshair", "Combat", "Crosshair"])
        .pick("Style", &["Brackets", "Ghost", "Square"])
        .bool("Players only", false)
        .bool("Through walls", false)
        .num("Range", 32.0, 4.0, 64.0, 1.0)
        .num("Corner length", 9.0, 3.0, 20.0, 1.0)
        .num("Padding", 3.0, 0.0, 10.0, 1.0)
        .num("Thickness", 1.5, 0.5, 4.0, 0.5)
        .bool("Outline", true)
        .pick("Color", &["Accent", "Custom"])
        .colour("Custom color", "White"),
    ]
}

#[derive(Clone)]
struct Selected {
    index: usize,
    id: u32,
    // The engine replaces this Arc on every join/respawn/dimension change.
    // Retaining the old Arc prevents address reuse from accepting a new level.
    level: Arc<ChunkFormat>,
}

static SELECTED: Mutex<Option<Selected>> = Mutex::new(None);

// Ghost and Square: how much they show (0 to 1, in over 850 ms and out over 1100 ms), from what when it last
// changed, and where they draw (eased toward the target's middle).
struct Shown {
    level: f32,
    from: f32,
    since: Instant,
    had: bool,
    at: Option<[f64; 3]>,
    size: [f64; 2],
    frame: Instant,
    epoch: Instant,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn smoother(f: f32) -> f32 {
    let f = f.clamp(0.0, 1.0);
    f * f * f * (f * (f * 6.0 - 15.0) + 10.0)
}

fn visible(game: &Game<'_>) -> bool {
    game.native_playing()
        && !game.ui.hide_gui
        && !gui_open()
        && !game.spectator()
        && game
            .player
            .as_ref()
            .is_some_and(|p| p.health.health() > 0.0)
}

fn accepts(game: &Game<'_>, m: &Module, mob: &horizon_client::mobs::Mob) -> bool {
    mob.kind.id != "minecraft:armor_stand"
        && eligible(
            mob.id,
            own_id(game),
            mob.health,
            mob.removed,
            mob.is_invisible(),
            mob.kind.id == "minecraft:player",
            m.b("Players only"),
        )
}

// Call after Kill Aura's tick; pass None when Kill Aura is disabled. Selection
// work (including the native crosshair ray test) happens once a tick, not once
// a rendered frame. There is no last-hit memory or search for a nearby target.
pub(super) fn tick(game: &Game<'_>, modules: &[Module], combat_id: Option<u32>) {
    let mut selected = SELECTED.lock().unwrap_or_else(|e| e.into_inner());
    *selected = None;
    let Some(m) = modules.iter().find(|m| m.name == "Target ESP" && m.on) else {
        return;
    };
    if !visible(game) {
        return;
    }
    let Some(remote) = game.remote.as_ref() else {
        return;
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let allowed = |i: usize| {
        let mob = game.mobs.get(i)?;
        if !accepts(game, m, mob) {
            return None;
        }
        let b = mob.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let mid = [
            (lo.x + hi.x) / 2.0,
            (lo.y + hi.y) / 2.0,
            (lo.z + hi.z) / 2.0,
        ];
        let (direction, distance) = sightline(eye, mid, m.n("Range"))?;
        // Use the host's voxel traversal and real contextual OUTLINE shapes,
        // including partial slabs/doors/fences. No full-cube approximation.
        // This one center ray runs on the tick, never for every rendered frame.
        if !m.b("Through walls") && game.clip_ray(eye, direction, distance - 0.001).is_some() {
            return None;
        }
        Some(i)
    };
    let combat = if m.p("Target") != "Crosshair" {
        combat_id
            .and_then(|id| game.mobs.iter().position(|mob| mob.id == id))
            .and_then(allowed)
    } else {
        None
    };
    let picked = combat.or_else(|| {
        (m.p("Target") != "Combat")
            .then(|| game.mob_hit())
            .flatten()
            .and_then(|(i, ..)| allowed(i))
    });
    if let Some(index) = picked {
        *selected = Some(Selected {
            index,
            id: game.mobs[index].id,
            level: Arc::clone(&remote.chunk_format),
        });
    }
}

// Constant work: one indexed entity lookup, eight projected points, at most
// sixteen clipped rectangles in the existing HUD batch, and no allocations.
pub(super) fn draw(
    game: &Game<'_>,
    cam: &Camera,
    m: &Module,
    viewport: [f32; 2],
    lw: f32,
    out: &mut impl Canvas,
) {
    if !m.on || !visible(game) {
        return;
    }
    if m.p("Style") != "Brackets" {
        return styled(game, cam, m, out);
    }
    let selected = SELECTED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (Some(selected), Some(remote)) = (selected, game.remote.as_ref()) else {
        return;
    };
    if !Arc::ptr_eq(&selected.level, &remote.chunk_format) {
        return;
    }
    let Some(mob) = game
        .mobs
        .get(selected.index)
        .filter(|mob| mob.id == selected.id && accepts(game, m, mob))
    else {
        return;
    };
    let alpha = game.clock.camera_fraction();
    let off = [
        (mob.previous.x - mob.position.x) * (1.0 - alpha),
        (mob.previous.y - mob.position.y) * (1.0 - alpha),
        (mob.previous.z - mob.position.z) * (1.0 - alpha),
    ];
    let b = mob.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let lo = [lo.x + off[0], lo.y + off[1], lo.z + off[2]];
    let hi = [hi.x + off[0], hi.y + off[1], hi.z + off[2]];
    let mid = [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ];
    if dist(cam.eye(), mid) > m.n("Range") {
        return;
    }
    // Skip boxes crossing the camera's near plane rather than emitting huge
    // screen-spanning geometry. The marker returns once all corners project.
    let Some(bounds) = cam.corners(lo, hi).and_then(|points| cam.bounds(&points)) else {
        return;
    };
    let Some(rects) = brackets(
        bounds,
        m.n("Thickness") as f32,
        m.n("Padding") as f32,
        m.n("Corner length") as f32,
    ) else {
        return;
    };
    if m.b("Outline") {
        for rect in rects {
            if let Some(rect) = clip(rect, viewport, lw.max(0.5)) {
                out.fill(rect, 0xE010_1010);
            }
        }
    }
    let color = if m.p("Color") == "Custom" {
        m.c("Custom color")
    } else {
        0xFF00_0000 | accent()
    };
    for rect in rects {
        if let Some(rect) = clip(rect, viewport, 0.0) {
            out.fill(rect, color);
        }
    }
}

// The target's box this frame (between ticks), if one is selected and still there.
fn target_box(game: &Game<'_>, m: &Module) -> Option<([f64; 3], [f64; 3])> {
    let selected = SELECTED.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
    let remote = game.remote.as_ref()?;
    if !Arc::ptr_eq(&selected.level, &remote.chunk_format) {
        return None;
    }
    let mob = game.mobs.get(selected.index).filter(|mob| mob.id == selected.id && accepts(game, m, mob))?;
    let alpha = game.clock.camera_fraction();
    let off = [
        (mob.previous.x - mob.position.x) * (1.0 - alpha),
        (mob.previous.y - mob.position.y) * (1.0 - alpha),
        (mob.previous.z - mob.position.z) * (1.0 - alpha),
    ];
    let b = mob.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    Some(([lo.x + off[0], lo.y + off[1], lo.z + off[2]], [hi.x + off[0], hi.y + off[1], hi.z + off[2]]))
}

// Ghost and Square.
fn styled(game: &Game<'_>, cam: &Camera, m: &Module, out: &mut impl Canvas) {
    let now = Instant::now();
    let found = target_box(game, m).filter(|(lo, hi)| dist(cam.eye(), [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0]) <= m.n("Range"));
    let mut guard = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    let s = guard.get_or_insert_with(|| Shown { level: 0.0, from: 0.0, since: now, had: false, at: None, size: [0.6, 1.8], frame: now, epoch: now });
    let dt = (now - s.frame).as_secs_f32().min(0.1);
    s.frame = now;
    if found.is_some() != s.had {
        (s.since, s.from, s.had) = (now, s.level, found.is_some());
    }
    let gone = (now - s.since).as_millis() as f32;
    s.level = if s.had { s.from + (1.0 - s.from) * smoother(gone / 850.0) } else { s.from * (1.0 - smoother(gone / 1100.0)) }.clamp(0.0, 1.0);
    if let Some((lo, hi)) = found {
        s.size = [hi[0] - lo[0], hi[1] - lo[1]];
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let k = f64::from(1.0 - (-dt * 14.0).exp());
        s.at = Some(match s.at.filter(|a| dist(*a, mid) <= 50.0) {
            Some(a) => [0, 1, 2].map(|i| a[i] + (mid[i] - a[i]) * k),
            None => mid,
        });
    }
    let (Some(at), level) = (s.at, s.level) else {
        return;
    };
    if level < 0.01 {
        return;
    }
    let rgb = if m.p("Color") == "Custom" { m.c("Custom color") & 0x00FF_FFFF } else { accent() };
    let t = (now - s.epoch).as_secs_f32();
    let [width, height] = s.size;
    if m.p("Style") == "Ghost" {
        ghost(cam, out, at, width, height, level, rgb, t);
    } else {
        square(cam, out, at, height, level, rgb, t);
    }
}

// Ghost: four trails of glowing dots circling the target, each a hundred degrees long and a quarter of its height
// above the last, alternately one way and the other, faster and slower, bobbing; brightest (toward white) at their
// heads.
fn ghost(cam: &Camera, out: &mut impl Canvas, at: [f64; 3], width: f64, height: f64, level: f32, rgb: u32, t: f32) {
    let mut turn = t / 3.0 + (t / 2.5).sin() / 10.0;
    let mut rise = height as f32 / 4.0;
    let mut head_first = true;
    for _ in 0..4 {
        let start = turn * 360.0;
        for step in 0..25 {
            let deg = start + step as f32 * 4.0;
            let mut along = (deg - (start - 90.0)) / 240.0;
            let size = 0.15 * ((if head_first { along } else { (1.0 - along) * 1.5 }).max(0.15) + 0.45) * 2.8;
            if head_first {
                along = 1.0 - along;
            }
            let dot = size * (1.0 - along.powi(5)) * level;
            let a = size * (1.0 - along.sqrt()) * level;
            let r = deg.to_radians();
            let y = (f64::from(rise) + f64::from((r * 1.5).sin()) * 0.25 - 0.9) * f64::from(level);
            let p = [at[0] + f64::from(r.cos()) * width * f64::from(level), at[1] + y, at[2] + f64::from(r.sin()) * width * f64::from(level)];
            // Each dot is a soft sprite `dot` across whose bright part is about a fifth of it.
            if let (Some(q), Some(px)) = (cam.project(p), cam.pixels_per_block(p)) {
                glow(out, q, dot * 0.2 * px, alpha(mix(rgb, 0xFF_FFFF, (a * 2.5).min(1.0)), a * 1.5));
            }
        }
        turn *= -1.35;
        head_first = !head_first;
        rise += height as f32 / 4.0;
    }
}

// Square: a square on the target's middle, turning, its sides shading between the accent and a lighter one.
fn square(cam: &Camera, out: &mut impl Canvas, at: [f64; 3], height: f64, level: f32, rgb: u32, t: f32) {
    let (Some(c), Some(px)) = (cam.project(at), cam.pixels_per_block(at)) else {
        return;
    };
    let half = ((height / 2.0 + 0.5) * 0.8 * 0.85) as f32 * level * px / 2.0;
    let spin = (t * 36.0).to_radians();
    let corner = |k: usize| {
        let a = spin + k as f32 * std::f32::consts::FRAC_PI_2;
        [c[0] + half * a.cos() * std::f32::consts::SQRT_2, c[1] + half * a.sin() * std::f32::consts::SQRT_2]
    };
    let w = (half * 0.12).max(1.5);
    for k in 0..4 {
        let (p, q) = (corner(k), corner(k + 1));
        let tone = ((t + k as f32).sin() * 0.5 + 0.5) * 0.5;
        let colour = alpha(mix(rgb, shade(rgb, 0.4), tone), level);
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
        out.quad([[p[0] + nx, p[1] + ny], [q[0] + nx, q[1] + ny], [q[0] - nx, q[1] - ny], [p[0] - nx, p[1] - ny]], colour);
    }
    glow(out, c, w, alpha(shade(rgb, 0.4), level));
}
