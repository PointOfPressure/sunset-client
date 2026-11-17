// The engine supplies positions only. Sunset owns projection, palette, shapes and frame/DPI stroke width.

use super::{Dim, Game};
use super::sunset::{Camera, Canvas, Module, gui_open, line};
use super::sunset_block_overlay_math::{bounded, translated, valid_box};
use super::sunset_visual::{Look, draw_box};
use std::collections::HashSet;

pub(super) fn draw(game: &Game<'_>, camera: &Camera, modules: &[Module], out: &mut impl Canvas, line_width: f32) {
    let Some(module) = modules.iter().find(|m| m.name == "Baritone" && m.on) else { return };
    if !game.native_window() || !game.native_playing() || game.ui.hide_gui || gui_open() || game.player.is_none() { return; }
    let Some(snapshot) = super::sunset_baritone::draw_snapshot() else { return };
    let dimension = match game.dim {
        Dim::Overworld => "minecraft:overworld", Dim::Nether => "minecraft:the_nether", Dim::End => "minecraft:the_end",
    };
    if snapshot.job == 0 || snapshot.world != super::sunset_seedcracker::world_id(game) || snapshot.dimension != dimension { return; }
    let look = Look::of(module);
    let width = line_width * bounded(module.n("Line width"), 0.5, 4.0, 1.0) as f32;
    let mut left = bounded(module.n("Render cap"), 0.0, 2048.0, 256.0) as usize;
    if module.b("Targets") {
        let mut shown = HashSet::new();
        for pos in snapshot.target.into_iter().chain(snapshot.breaking.iter().copied()) {
            if left == 0 { break; }
            if !shown.insert(pos) { continue; }
            if block(game, camera, out, &look, pos, module.c("Target color"), width) { left -= 1; }
        }
        for &pos in snapshot.placing.iter().take(left) {
            if !shown.insert(pos) { continue; }
            let lo = pos.map(f64::from);
            if draw_box(camera, out, &look, lo, lo.map(|v| v + 1.0), module.c("Goal color"), width) { left -= 1; }
        }
    }
    if module.b("Goal") && left > 0 {
        if let Some(pos) = snapshot.goal.filter(|p| game.world.chunks.contains_key(&(p[0] >> 4, p[2] >> 4))) {
            let lo = pos.map(f64::from);
            if draw_box(camera, out, &look, lo, lo.map(|v| v + 1.0), module.c("Goal color"), width) { left -= 1; }
        }
    }
    if !module.b("Path") { return; }
    for (path, colour) in [(&snapshot.path, module.c("Path color")), (&snapshot.next_path, module.c("Goal color"))] {
        for pair in path.windows(2).take(2048) {
            if left == 0 { return; }
            let [a, b] = [pair[0].map(|v| f64::from(v) + 0.5), pair[1].map(|v| f64::from(v) + 0.5)];
            if let (Some(a), Some(b)) = (camera.project(a), camera.project(b)) {
                line(out, a, b, colour, width);
                left -= 1;
            }
        }
    }
}

fn block(
    game: &Game<'_>, camera: &Camera, out: &mut impl Canvas, look: &Look,
    pos: [i32; 3], colour: u32, width: f32,
) -> bool {
    if !game.world.chunks.contains_key(&(pos[0] >> 4, pos[2] >> 4)) { return false; }
    let state = game.world.block(pos[0], pos[1], pos[2]);
    if game.world.is_air(state) { return false; }
    let base = pos.map(f64::from);
    let draw = |out: &mut _, shape| {
        let b = translated(shape, base);
        valid_box(b) && draw_box(camera, out, look, [b[0], b[1], b[2]], [b[3], b[4], b[5]], colour, width)
    };
    if let Some(shape) = game.context_outline(pos, state) { return draw(out, shape); }
    let fact = game.res.facts().get(state);
    if fact.outline.is_empty() { return draw(out, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]); }
    let offset = if fact.outline_offset {
        game.pack.context.tables.offset(state, pos[0], pos[2]).unwrap_or([0.0; 3])
    } else { [0.0; 3] };
    let mut drawn = false;
    for &shape in fact.outline.iter().take(32) {
        drawn |= draw(out, translated(shape, offset));
    }
    drawn
}
