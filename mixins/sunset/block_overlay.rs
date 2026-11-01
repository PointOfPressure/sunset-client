// Block Overlay: the block you look at, its own selection shape outlined and filled in a colour. Drawn with the rest
// of Sunset's overlays over the frame (so not hidden by terrain); the game's own outline is left out while it shows.

use super::Game;
use super::sunset::{Camera, Canvas, Category, Module, gui_open, line};
use super::sunset_block_overlay_math::{EDGES, FACES, bounded, box_edge, colour, translated, valid_box, visible};

pub(super) fn defaults() -> Module {
    Module::new("Block Overlay", Category::Render, "Colors the targeted block's actual selection shape. Hides with F1 and in menus. Projected overlay without terrain depth testing.")
        .colour("Color", "Gold")
        .bool("Outline", true)
        .num("Outline opacity", 0.9, 0.0, 1.0, 0.05)
        .num("Line width", 1.0, 0.5, 4.0, 0.25)
        .bool("Fill", true)
        .num("Fill opacity", 0.15, 0.0, 1.0, 0.05)
}

fn active(game: &Game<'_>, m: &Module) -> bool {
    m.on && visible(game.native_playing(), game.ui.hide_gui, gui_open(), game.may_build(), game.player.is_some())
}

/// For a hook at Game::render's native outline extraction. Do not apply this to
/// Game::pick itself: gameplay and our own current-target read must stay intact.
/// No state is changed, so disabling immediately restores the native stroke.
pub(super) fn suppress_native_outline(game: &Game<'_>) -> bool {
    if !game.native_window() || !game.native_on("sunset") {
        return false;
    }
    // gui_open() takes the same lock as with(), so evaluate visibility first.
    if !visible(game.native_playing(), game.ui.hide_gui, gui_open(), game.may_build(), game.player.is_some()) {
        return false;
    }
    super::sunset::with(|s| s.module("Block Overlay").is_some_and(|m| {
        m.on && ((m.b("Outline") && colour(0, m.n("Outline opacity")) >> 24 != 0)
            || (m.b("Fill") && colour(0, m.n("Fill opacity")) >> 24 != 0))
    }))
}

pub(super) fn overlay(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let Some(m) = mods.iter().find(|m| m.name == "Block Overlay" && m.on) else {
        return;
    };
    if !active(game, m) {
        return;
    }
    let rgb = m.c("Color");
    let outline = if m.b("Outline") { colour(rgb, m.n("Outline opacity")) } else { 0 };
    let fill = if m.b("Fill") { colour(rgb, m.n("Fill opacity")) } else { 0 };
    if (outline | fill) >> 24 == 0 {
        return;
    }
    // Use the engine's pick (and its cached tick hit when present), including
    // reach and contextual shapes. Never retain it across frames or world loads.
    let Some(hit) = game.pick() else {
        return;
    };
    let state = game.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
    let facts = game.res.facts();
    if facts.is_air(state) {
        return; // A previously picked block can have been removed this tick.
    }
    let fact = facts.get(state);
    let position = hit.pos.map(f64::from);
    let width = lw * bounded(m.n("Line width"), 0.5, 4.0, 1.0) as f32;

    // Held light/scaffolding and animated shulker shapes use the same context
    // bounds as the host. Draw these directly, without outline_boxes()'s Vec.
    if let Some(shape) = game.context_outline(hit.pos, state) {
        let b = translated(shape, position);
        if valid_box(b) {
            if fill >> 24 != 0 {
                fill_box(cam, out, b, fill);
            }
            if outline >> 24 != 0 {
                for corners in EDGES {
                    edge(cam, out, box_edge(b, corners), outline, width);
                }
            }
        }
        return;
    }

    let offset = if fact.outline_offset {
        game.pack.context.tables.offset(state, hit.pos[0], hit.pos[2]).unwrap_or([0.0; 3])
    } else {
        [0.0; 3]
    };
    let base = std::array::from_fn(|i| position[i] + offset[i]);
    if fill >> 24 != 0 {
        for &shape in fact.outline.iter() {
            let b = translated(shape, base);
            if valid_box(b) {
                fill_box(cam, out, b, fill);
            }
        }
    }
    if outline >> 24 != 0 {
        // These are the host's shape-union edges, not twelve edges per box.
        // Stairs, fences and panes retain their actual selection geometry.
        for &e in fact.outline_edges.iter() {
            edge(cam, out, translated(e, base), outline, width);
        }
    }
}

fn edge(cam: &Camera, out: &mut impl Canvas, e: [f64; 6], colour: u32, width: f32) {
    if !e.iter().all(|v| v.is_finite()) {
        return;
    }
    if let (Some(a), Some(b)) = (cam.project([e[0], e[1], e[2]]), cam.project([e[3], e[4], e[5]])) {
        line(out, a, b, colour, width);
    }
}

fn fill_box(cam: &Camera, out: &mut impl Canvas, b: [f64; 6], colour: u32) {
    let lo = [b[0], b[1], b[2]];
    let hi = [b[3], b[4], b[5]];
    let points = cam.points(lo, hi);
    let eye = cam.eye();
    for (face, axis, high) in FACES {
        let facing = if high { eye[axis] > hi[axis] } else { eye[axis] < lo[axis] };
        if let (true, [Some(a), Some(b), Some(c), Some(d)]) = (facing, face.map(|i| points[i])) {
            out.quad([a, b, c, d], colour);
        }
    }
}
