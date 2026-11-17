// baritone.utils.PathRenderer, SelectionRenderer and IRenderer, and ElytraBehavior.onRenderPass, written from the Java: the
// goal, the blocks the path breaks, places and walks into, the current and next path, the search in progress, the selections
// and the elytra's view of its flight, all as lines. A line is drawn at 40 percent opacity unless a setting says otherwise,
// with the width of its setting, over everything when its `...IgnoreDepth` setting is on and depth-tested when not.
// The beacon beam of a GoalXZ is the game's own beam geometry (`goal_beams`, called from the beacon hook), as the Java
// asks the BeaconRenderer for it.
use super::baritone::Baritone;
use super::baritone_goals::{GoalRef, GoalRender};
use super::baritone_gworld::dimension_extent_of;
use super::baritone_settings::Settings;
use super::baritone_utils::Pos;
use super::Game;
use horizon_client::gizmos::Line;
use horizon_core::geometry::Axis;

/// The lines of a frame: the depth-tested ones and the ones over everything.
#[derive(Default)]
pub struct Canvas {
    pub depth: Vec<Line>,
    pub top: Vec<Line>,
}

/// `IRenderer.glColor(color, alpha)`: the color of a setting at an opacity.
fn with_alpha(color: u32, alpha: f32) -> u32 {
    (color & 0x00FF_FFFF) | (((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24)
}

/// The alpha of `IRenderer.startLines(color, width, ignoreDepth)`.
const LINE_ALPHA: f32 = 0.4;

impl Canvas {
    fn line(&mut self, ignore_depth: bool, a: [f64; 3], b: [f64; 3], color: u32, width: f32) {
        let line = Line { start: a, end: b, color, width };
        if ignore_depth { self.top.push(line) } else { self.depth.push(line) }
    }

    /// `IRenderer.emitAABB`: the bottom square, the top square and the four edges between them.
    fn aabb(&mut self, ignore_depth: bool, min: [f64; 3], max: [f64; 3], color: u32, width: f32) {
        let (x0, y0, z0, x1, y1, z1) = (min[0], min[1], min[2], max[0], max[1], max[2]);
        for y in [y0, y1] {
            self.line(ignore_depth, [x0, y, z0], [x1, y, z0], color, width);
            self.line(ignore_depth, [x1, y, z0], [x1, y, z1], color, width);
            self.line(ignore_depth, [x1, y, z1], [x0, y, z1], color, width);
            self.line(ignore_depth, [x0, y, z1], [x0, y, z0], color, width);
        }
        for (x, z) in [(x0, z0), (x1, z0), (x1, z1), (x0, z1)] {
            self.line(ignore_depth, [x, y0, z], [x, y1, z], color, width);
        }
    }
}

/// What the renderer reads of the game and the settings.
struct Scene<'a> {
    s: &'a Settings,
    game: &'a Game<'a>,
    player: Option<[f64; 3]>,
    min_y: i32,
    max_y: i32,
}

impl Scene<'_> {
    fn path_width(&self) -> f32 {
        self.s.float("pathRenderLineWidthPixels") as f32
    }

    fn goal_width(&self) -> f32 {
        self.s.float("goalRenderLineWidthPixels") as f32
    }
}

/// `VoxelShape.bounds()` of the block at `pos`, or a whole block when its shape is empty or depends on its neighbours.
fn block_bounds(game: &Game<'_>, pos: Pos) -> ([f64; 3], [f64; 3]) {
    let id = game.world.block(pos[0], pos[1], pos[2]);
    let (lo, hi) = match game.res.collision().cached_shapes().cached_shape(id) {
        Ok(Some(shape)) if !shape.is_empty() => ([shape.min(Axis::X), shape.min(Axis::Y), shape.min(Axis::Z)], [shape.max(Axis::X), shape.max(Axis::Y), shape.max(Axis::Z)]),
        _ => ([0.0; 3], [1.0; 3]),
    };
    ([f64::from(pos[0]) + lo[0], f64::from(pos[1]) + lo[1], f64::from(pos[2]) + lo[2]], [f64::from(pos[0]) + hi[0], f64::from(pos[1]) + hi[1], f64::from(pos[2]) + hi[2]])
}

/// `drawManySelectionBoxes`.
fn draw_many_selection_boxes<'a>(c: &mut Canvas, sc: &Scene<'_>, positions: impl Iterator<Item = &'a Pos>, color: u32) {
    let ignore = sc.s.flag("renderSelectionBoxesIgnoreDepth");
    let color = with_alpha(color, LINE_ALPHA);
    for pos in positions {
        let (lo, hi) = block_bounds(sc.game, *pos);
        // emitAABB(.., .002): the box is grown a little so it does not z-fight with the block.
        c.aabb(ignore, [lo[0] - 0.002, lo[1] - 0.002, lo[2] - 0.002], [hi[0] + 0.002, hi[1] + 0.002, hi[2] + 0.002], color, sc.path_width());
    }
}

/// `drawPath`: lines through the positions, runs in one direction joined into one line, fading out when asked.
#[allow(clippy::too_many_arguments)]
pub fn draw_path(c: &mut Canvas, s: &Settings, positions: &[Pos], start_index: usize, color: u32, fade_out: bool, fade_start0: i32, fade_end0: i32, offset: f64) {
    let (width, ignore) = (s.float("pathRenderLineWidthPixels") as f32, s.flag("renderPathIgnoreDepth"));
    let mut line_color = with_alpha(color, LINE_ALPHA);
    let fade_start = fade_start0 + start_index as i32;
    let fade_end = fade_end0 + start_index as i32;
    let extra_offset = offset + 0.03;
    let as_line = s.flag("renderPathAsLine");
    let mut i = start_index;
    while i + 1 < positions.len() {
        let start = positions[i];
        let mut next = i + 1;
        let mut end = positions[next];
        let dir = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        while next + 1 < positions.len()
            && (!fade_out || (next as i32 + 1) < fade_start)
            && dir == [positions[next + 1][0] - end[0], positions[next + 1][1] - end[1], positions[next + 1][2] - end[2]]
        {
            next += 1;
            end = positions[next];
        }
        if fade_out {
            let alpha = if (i as i32) <= fade_start {
                0.4
            } else {
                if (i as i32) > fade_end {
                    break;
                }
                0.4 * (1.0 - (i as i32 - fade_start) as f32 / (fade_end - fade_start) as f32)
            };
            line_color = with_alpha(color, alpha);
        }
        let p = |n: Pos, extra: f64| [f64::from(n[0]) + offset, f64::from(n[1]) + extra, f64::from(n[2]) + offset];
        c.line(ignore, p(start, offset), p(end, offset), line_color, width);
        if !as_line {
            c.line(ignore, p(end, offset), p(end, extra_offset), line_color, width);
            c.line(ignore, p(end, extra_offset), p(start, extra_offset), line_color, width);
            c.line(ignore, p(start, extra_offset), p(start, offset), line_color, width);
        }
        i = next;
    }
}

/// `drawDankLitGoalBox`: two squares at `y1` and `y2` (none where it is None), and four columns up the corners.
#[allow(clippy::too_many_arguments)]
fn dank_lit_goal_box(c: &mut Canvas, sc: &Scene<'_>, color: u32, min_x: f64, max_x: f64, min_z: f64, max_z: f64, min_y: f64, max_y: f64, y1: Option<f64>, y2: Option<f64>) {
    let (width, ignore) = (sc.goal_width(), sc.s.flag("renderGoalIgnoreDepth"));
    let color = with_alpha(color, LINE_ALPHA);
    for y in [y1, y2].into_iter().flatten() {
        c.line(ignore, [min_x, y, min_z], [max_x, y, min_z], color, width);
        c.line(ignore, [max_x, y, min_z], [max_x, y, max_z], color, width);
        c.line(ignore, [max_x, y, max_z], [min_x, y, max_z], color, width);
        c.line(ignore, [min_x, y, max_z], [min_x, y, min_z], color, width);
    }
    let mut y = min_y;
    while y < max_y {
        let top = max_y.min(y + 16.0);
        for (x, z) in [(min_x, min_z), (max_x, min_z), (max_x, max_z), (min_x, max_z)] {
            c.line(ignore, [x, y, z], [x, top, z], color, width);
        }
        y += 16.0;
    }
}

/// `drawGoal`.
fn draw_goal(c: &mut Canvas, sc: &Scene<'_>, goal: &GoalRef, color: u32) {
    // y = 1 causes rendering issues when the player is at the same y as the top of a block for some reason.
    let animated = sc.s.flag("renderGoalAnimated");
    let y = if animated {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        ((nanos / 100_000) % 20_000) as f64 / 20_000.0 * std::f64::consts::PI * 2.0
    } else {
        0.999
    };
    let y = if animated { y.cos() } else { y };
    match goal.render_kind() {
        GoalRender::Pos { pos, halved } => {
            let (min_x, max_x) = (f64::from(pos[0]) + 0.002, f64::from(pos[0]) + 1.0 - 0.002);
            let (min_z, max_z) = (f64::from(pos[2]) + 0.002, f64::from(pos[2]) + 1.0 - 0.002);
            let y = if halved { y / 2.0 } else { y };
            let py = f64::from(pos[1]);
            let (mut y1, mut y2) = (1.0 + y + py, 1.0 - y + py);
            let min_y = py;
            let mut max_y = min_y + 2.0;
            if halved {
                y1 -= 0.5;
                y2 -= 0.5;
                max_y -= 1.0;
            }
            dank_lit_goal_box(c, sc, color, min_x, max_x, min_z, max_z, min_y, max_y, Some(y1), Some(y2));
        }
        GoalRender::Xz(x, z) => {
            if sc.s.flag("renderGoalXZBeacon") {
                return; // the beam is drawn with the beacons
            }
            let (min_x, max_x) = (f64::from(x) + 0.002, f64::from(x) + 1.0 - 0.002);
            let (min_z, max_z) = (f64::from(z) + 0.002, f64::from(z) + 1.0 - 0.002);
            dank_lit_goal_box(c, sc, color, min_x, max_x, min_z, max_z, f64::from(sc.min_y), f64::from(sc.max_y), None, None);
        }
        GoalRender::Composite(goals) => {
            for g in &goals {
                draw_goal(c, sc, g, color);
            }
        }
        GoalRender::Inverted(origin) => draw_goal(c, sc, &origin, sc.s.color("colorInvertedGoalBox")),
        GoalRender::YLevel(level) => {
            let Some(p) = sc.player else { return };
            let size = sc.s.float("yLevelBoxSize");
            let min_y = f64::from(level);
            let (y1, y2) = (1.0 + y + min_y, 1.0 - y + min_y);
            dank_lit_goal_box(c, sc, color, p[0] - size, p[0] + size, p[2] - size, p[2] + size, min_y, min_y + 2.0, Some(y1), Some(y2));
        }
        GoalRender::Nothing => {}
    }
}

/// How far from the camera a beam drawn over everything is put, in blocks.
const NEAR_SIDE: f64 = 1.5;

/// Each GoalXZ a goal holds, with the color it is drawn in.
fn xz_goals(goal: &GoalRef, color: u32, s: &Settings, out: &mut Vec<(i32, i32, u32)>) {
    match goal.render_kind() {
        GoalRender::Xz(x, z) => out.push((x, z, color)),
        GoalRender::Composite(goals) => goals.iter().for_each(|g| xz_goals(g, color, s, out)),
        GoalRender::Inverted(origin) => xz_goals(&origin, s.color("colorInvertedGoalBox"), s, out),
        _ => {}
    }
}

/// `PathRenderer.drawGoal`'s GoalXZ branch with `renderGoalXZBeacon`: `BeaconRenderer.renderBeaconBeam` from the world's
/// lowest block, with the beacon's texture, for each GoalXZ of the goal. `camera` is where the frame is drawn from.
pub fn goal_beams(b: &Baritone, game: &Game<'_>, camera: glam::DVec3, alpha: f64, out: &mut Vec<horizon_client::mesher::Vertex>) {
    let s = &b.core.settings;
    if !s.flag("renderGoal") || !s.flag("renderGoalXZBeacon") {
        return;
    }
    let Some(goal) = b.core.pathing.goal() else { return };
    let Some(sprite) = game.gui.atlas.sprites.get("entity/beacon/beacon_beam").copied() else { return };
    let mut beams = Vec::new();
    xz_goals(goal, s.color("colorGoalBox"), s, &mut beams);
    // The Java gives the beam `getMaxY()` as its height, from `getMinY()`.
    let (min_y, max_y) = dimension_extent_of(game);
    let animated = s.flag("renderGoalAnimated");
    let time = if animated { game.world.game_time.rem_euclid(40) as f32 + alpha as f32 } else { 0.0 };
    for (x, z, color) in beams {
        let origin = glam::Vec3::new((f64::from(x) - camera.x) as f32, (-camera.y) as f32, (f64::from(z) - camera.z) as f32);
        let shape = horizon_client::block_entity_render::BeamShape { scale: 1.0, time, start: min_y, height: max_y, color: color & 0x00FF_FFFF, solid_radius: 0.2, glow_radius: 0.25 };
        let from = out.len();
        horizon_client::block_entity_render::beam(glam::Mat4::from_translation(origin), shape, sprite, out);
        if s.flag("renderGoalIgnoreDepth") {
            // `RenderSystem.disableDepthTest()`: the beacon pass is depth tested, so the beam is drawn as if seen from a block
            // and a half away, along the same rays: it looks the same, and is nearer than any terrain behind it.
            let distance = f64::from(origin.x).hypot(f64::from(origin.z));
            let scale = (NEAR_SIDE / distance.max(NEAR_SIDE)) as f32;
            for vertex in &mut out[from..] {
                vertex.pos = vertex.pos.map(|c| c * scale);
            }
        }
    }
}

/// `PathRenderer.render`, `SelectionRenderer.renderSelections` and the elytra's lines.
pub fn lines(b: &Baritone, game: &Game<'_>, out: &mut Canvas) {
    let s = &b.core.settings;
    let player = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]);
    let (min_y, max_y) = dimension_extent_of(game);
    let sc = Scene { s, game, player, min_y, max_y };
    let pathing = &b.core.pathing;

    if let Some(goal) = pathing.goal() {
        if s.flag("renderGoal") {
            draw_goal(out, &sc, goal, s.color("colorGoalBox"));
        }
    }

    // GuiClick.onRender: the block under the mouse, and the box a drag would select.
    if let Some(over) = b.core.click.over {
        draw_many_selection_boxes(out, &sc, [over].iter(), 0xFF00_FFFF);
        if let Some(start) = b.core.click.click_start.filter(|start| *start != over) {
            let lo = [0, 1, 2].map(|i| f64::from(start[i].min(over[i])));
            let hi = [0, 1, 2].map(|i| f64::from(start[i].max(over[i])) + 1.0);
            out.aabb(true, lo, hi, with_alpha(0xFFFF_0000, LINE_ALPHA), sc.path_width());
        }
    }

    if s.flag("renderPath") {
        let current = pathing.current.as_ref();
        if let Some(ex) = current {
            if s.flag("renderSelectionBoxes") {
                draw_many_selection_boxes(out, &sc, ex.to_break().iter(), s.color("colorBlocksToBreak"));
                draw_many_selection_boxes(out, &sc, ex.to_place().iter(), s.color("colorBlocksToPlace"));
                draw_many_selection_boxes(out, &sc, ex.to_walk_into().iter(), s.color("colorBlocksToWalkInto"));
            }
            // Render the current path, if there is one.
            let begin = ex.position().saturating_sub(3);
            draw_path(out, s, &ex.path.positions, begin, s.color("colorCurrentPath"), s.flag("fadePath"), 10, 20, 0.5);
        }
        if let Some(next) = pathing.next.as_ref() {
            draw_path(out, s, &next.path.positions, 0, s.color("colorNextPath"), s.flag("fadePath"), 10, 20, 0.5);
        }
        // If there is a path calculation currently running, render the path calculation process.
        if let Some((best, recent)) = pathing.in_progress_paths() {
            if let Some(p) = best {
                draw_path(out, s, &p.positions, 0, s.color("colorBestPathSoFar"), s.flag("fadePath"), 10, 20, 0.5);
            }
            if let Some(p) = recent {
                draw_path(out, s, &p.positions, 0, s.color("colorMostRecentConsidered"), s.flag("fadePath"), 10, 20, 0.5);
                draw_many_selection_boxes(out, &sc, std::iter::once(&p.dest()), s.color("colorMostRecentConsidered"));
            }
        }
    }

    draw_selections(out, s, b);
    draw_elytra(out, &sc, b);
}

/// `SelectionRenderer.renderSelections`.
fn draw_selections(c: &mut Canvas, s: &Settings, b: &Baritone) {
    let selections = &b.core.selections.selections;
    if !s.flag("renderSelection") || selections.is_empty() {
        return;
    }
    let opacity = s.float("selectionOpacity") as f32;
    let (ignore, width) = (s.flag("renderSelectionIgnoreDepth"), s.float("selectionLineWidth") as f32);
    let color = with_alpha(s.color("colorSelection"), opacity);
    let block = |p: Pos, grow: f64| ([f64::from(p[0]) - grow, f64::from(p[1]) - grow, f64::from(p[2]) - grow], [f64::from(p[0]) + 1.0 + grow, f64::from(p[1]) + 1.0 + grow, f64::from(p[2]) + 1.0 + grow]);
    for sel in selections {
        // SELECTION_BOX_EXPANSION.
        let (lo, hi) = (sel.min(), sel.max());
        c.aabb(ignore, [f64::from(lo[0]) - 0.005, f64::from(lo[1]) - 0.005, f64::from(lo[2]) - 0.005], [f64::from(hi[0]) + 1.005, f64::from(hi[1]) + 1.005, f64::from(hi[2]) + 1.005], color, width);
    }
    if s.flag("renderSelectionCorners") {
        let pos1 = with_alpha(s.color("colorSelectionPos1"), opacity);
        for sel in selections {
            let (lo, hi) = block(sel.pos1, 0.0);
            c.aabb(ignore, lo, hi, pos1, width);
        }
        let pos2 = with_alpha(s.color("colorSelectionPos2"), opacity);
        for sel in selections {
            let (lo, hi) = block(sel.pos2, 0.0);
            c.aabb(ignore, lo, hi, pos2, width);
        }
    }
}

/// `ElytraBehavior.onRenderPass`: the path ahead in red, where it aims in green, the rays it cast and the flight it simulated.
fn draw_elytra(c: &mut Canvas, sc: &Scene<'_>, b: &Baritone) {
    use super::baritone_goals::{rc, GoalBlock};
    let Some(e) = b.procs.elytra.behavior() else { return };
    let s = sc.s;
    if !e.visible_path.is_empty() {
        draw_path(c, s, &e.visible_path, 0, 0xFFFF_0000, false, 0, 0, 0.0);
    }
    if let Some(aim) = e.aim_pos {
        draw_goal(c, sc, &rc(GoalBlock(aim)), 0xFF00_FF00);
    }
    let (width, ignore) = (sc.path_width(), s.flag("renderPathIgnoreDepth"));
    if s.flag("elytraRenderRaytraces") {
        for (lines, color) in [(&e.clear_lines, 0xFF00_FF00u32), (&e.blocked_lines, 0xFF00_00FF)] {
            let color = with_alpha(color, LINE_ALPHA);
            for (a, z) in lines {
                c.line(ignore, a.to_array(), z.to_array(), color, width);
            }
        }
    }
    if let (Some(line), true, Some(p)) = (&e.simulation_line, s.flag("elytraRenderSimulation"), sc.player) {
        let color = with_alpha(0xFF36_CCDC, LINE_ALPHA);
        for w in line.windows(2) {
            c.line(ignore, [w[0].x + p[0], w[0].y + p[1], w[0].z + p[2]], [w[1].x + p[0], w[1].y + p[1], w[1].z + p[2]], color, width);
        }
    }
}
