// Original low-cost marker for one current combat/crosshair target. Behavioral
// reference research and scope differences are recorded in the source docs.

use super::Game;
use super::remote_chunks::ChunkFormat;
use super::sunset::{Camera, Canvas, Category, Module, accent, gui_open, own_id};
use super::sunset_target_esp_math::{brackets, clip, eligible, sightline};
use super::sunset_util::dist;
use std::sync::{Arc, Mutex};

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(
            "Target ESP",
            Category::Render,
            "Marks your current combat target or the living entity under your crosshair.",
        )
        .pick("Target", &["Combat or crosshair", "Combat", "Crosshair"])
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
            .and_then(|(i, _)| allowed(i))
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
