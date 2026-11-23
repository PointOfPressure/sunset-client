// Runtime part (see runtime.rs): the camera, shapes and painting them.

use super::Game;
use horizon_client::hud::HudBatch;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

// ---------------------------------------------------------------- HUD

pub(super) struct Camera {
    pub(super) pos: [f64; 3],
    fwd: [f64; 3],
    pub(super) up: [f64; 3],
    pub(super) right: [f64; 3],
    focal: f64,
    aspect: f64,
    pub(super) w: f64,
    pub(super) h: f64,
    // The sine and cosine of half the view's width and of half its height: the sides of what it sees.
    cone: [f64; 4],
}

impl Camera {
    // Where the view is from.
    pub(super) fn pos(&self) -> [f64; 3] {
        self.pos
    }

    pub(super) fn of(game: &Game<'_>, w: f32, h: f32) -> Option<Self> {
        let (last_pos, last_yaw, last_pitch, fov) = game.locator_camera?;
        let p = game.player.as_ref()?;
        let alpha = game.clock.camera_fraction();
        let (pos, yaw, pitch) = if game.camera_type.is_first_person() {
            let lerp = |a: f64, b: f64| a + alpha * (b - a);
            let eye = f64::from(game.previous_eye) + alpha * f64::from(game.eye - game.previous_eye);
            (
                [
                    lerp(game.previous_position.x, p.position.x),
                    lerp(game.previous_position.y, p.position.y) + eye,
                    lerp(game.previous_position.z, p.position.z),
                ],
                game.yaw,
                game.pitch,
            )
        } else {
            (last_pos, last_yaw, last_pitch)
        };
        Some(Self::aimed(pos, yaw, pitch, fov, w, h))
    }

    // A view from `eye` toward `target`, `fov` degrees tall, `w` by `h` pixels: the Sidebar menu's ESP preview's.
    pub(super) fn looking_at(eye: [f64; 3], target: [f64; 3], fov: f32, w: f32, h: f32) -> Self {
        let d = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
        let d = [d[0] / len, d[1] / len, d[2] / len];
        // The inverse of `aimed`'s forward: [-sin yaw cos pitch, -sin pitch, cos yaw cos pitch].
        let pitch = (-d[1]).clamp(-1.0, 1.0).asin().to_degrees() as f32;
        let yaw = (-d[0]).atan2(d[2]).to_degrees() as f32;
        Self::aimed(eye, yaw, pitch, fov, w, h)
    }

    // A view from `pos` turned `yaw` and `pitch` degrees, `fov` degrees tall, `w` by `h` pixels.
    fn aimed(pos: [f64; 3], yaw: f32, pitch: f32, fov: f32, w: f32, h: f32) -> Self {
        let (sy, cy) = f64::from(yaw.to_radians()).sin_cos();
        let (sp, cp) = f64::from(pitch.to_radians()).sin_cos();
        let focal = 1.0 / f64::from((fov / 2.0).to_radians()).tan();
        let aspect = f64::from(w) / f64::from(h);
        let (sh, ch) = (aspect / focal).atan().sin_cos();
        let (sv, cv) = (1.0 / focal).atan().sin_cos();
        Self {
            pos,
            fwd: [-sy * cp, -sp, cy * cp],
            up: [-sy * sp, cp, cy * sp],
            right: [-cy, 0.0, -sy],
            focal,
            aspect,
            w: f64::from(w),
            h: f64::from(h),
            cone: [sh, ch, sv, cv],
        }
    }
    // Whether anything of a ball round `at` can be on screen: the test before any other work on a thing.
    pub(super) fn sees(&self, at: [f64; 3], radius: f64) -> bool {
        let o = [at[0] - self.pos[0], at[1] - self.pos[1], at[2] - self.pos[2]];
        let dot = |a: [f64; 3]| a[0] * o[0] + a[1] * o[1] + a[2] * o[2];
        let depth = dot(self.fwd);
        let [sh, ch, sv, cv] = self.cone;
        depth > -radius && depth * sh - dot(self.right).abs() * ch > -radius && depth * sv - dot(self.up).abs() * cv > -radius
    }
    // Whether a point on screen is within `margin` of it.
    pub(super) fn on_screen(&self, p: [f32; 2], margin: f32) -> bool {
        p[0] > -margin && p[1] > -margin && p[0] < self.w as f32 + margin && p[1] < self.h as f32 + margin
    }
    pub(super) fn project(&self, at: [f64; 3]) -> Option<[f32; 2]> {
        let o = [at[0] - self.pos[0], at[1] - self.pos[1], at[2] - self.pos[2]];
        let dot = |a: [f64; 3]| a[0] * o[0] + a[1] * o[1] + a[2] * o[2];
        let depth = dot(self.fwd);
        if depth < 0.05 {
            return None;
        }
        let x = dot(self.right) * self.focal / (depth * self.aspect);
        let y = dot(self.up) * self.focal / depth;
        Some([((1.0 + x) / 2.0 * self.w) as f32, ((1.0 - y) / 2.0 * self.h) as f32])
    }
    // Screen pixels a block across makes at `at` (None behind the camera): how big to draw a thing of a set size.
    pub(super) fn pixels_per_block(&self, at: [f64; 3]) -> Option<f32> {
        let o = [at[0] - self.pos[0], at[1] - self.pos[1], at[2] - self.pos[2]];
        let depth = o[0] * self.fwd[0] + o[1] * self.fwd[1] + o[2] * self.fwd[2];
        (depth >= 0.05).then(|| (self.focal / depth * self.h / 2.0) as f32)
    }
    // Where the view is from.
    pub(super) fn eye(&self) -> [f64; 3] {
        self.pos
    }
    // A box's eight corners on screen (None behind the camera): corner i is at hi on x, y and z where bits 1, 2 and
    // 4 of i are set.
    pub(super) fn points(&self, lo: [f64; 3], hi: [f64; 3]) -> [Option<[f32; 2]>; 8] {
        std::array::from_fn(|i| {
            self.project([
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ])
        })
    }
    // The eight, when all of the box is in front.
    pub(super) fn corners(&self, lo: [f64; 3], hi: [f64; 3]) -> Option<[[f32; 2]; 8]> {
        let pts = self.points(lo, hi);
        pts.iter().all(Option::is_some).then(|| pts.map(Option::unwrap_or_default))
    }
    // The screen rectangle round those corners, unless it is far off screen.
    pub(super) fn bounds(&self, pts: &[[f32; 2]; 8]) -> Option<[f32; 4]> {
        let r = pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, &[x, y]| [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)]);
        let margin = 4.0 * self.w as f32;
        (r[2] > -margin && r[0] < margin && r[3] > -margin && r[1] < margin).then_some(r)
    }
}

pub(super) enum Shape {
    Fill([f32; 4], u32),
    // A rectangle with rounded corners, and their radius.
    Round([f32; 4], f32, u32),
    // A rectangle with only some corners rounded: top left, top right, bottom right, bottom left.
    Corners([f32; 4], f32, [bool; 4], u32),
    // A slanted shape (a line): its four corners in order round it.
    Quad([[f32; 2]; 4], u32),
    Text(String, [f32; 2], u32),
    // A line of text in the font's bolder weight: a panel's header.
    Head(String, [f32; 2], u32),
    // An item's icon as the game draws it in its own menus (by id), and its size.
    Item(std::sync::Arc<str>, [f32; 2], f32),
    // A GUI sprite (by its atlas name) stretched over a rectangle, from the part of it given as fractions u0, v0, u1, v1.
    Sprite(&'static str, [f32; 4], [f32; 4]),
    // A line of text at a size (1: the usual).
    Scaled(String, [f32; 2], u32, f32),
    // A player's face from a skin texture (and its hat layer), and its size.
    Face(String, [f32; 2], f32),
    // A panel with squircle corners, and their radius.
    Squircle([f32; 4], f32, u32),
    // One of the menu's icons (a Lucide character) at a size (1: the text's).
    Icon(&'static str, [f32; 2], u32, f32),
    // What is drawn from ClipStart on is cut to the rectangle ClipEnd gives: x0, y0, x1, y1.
    ClipStart,
    ClipEnd([f32; 4]),
}

// Where shapes go: a list drawn after the frame's other work (the menu, plugins), or straight into the frame's
// batch (what the world overlays draw, so a frame of them makes no list and no copies of their text).
pub(super) trait Canvas {
    fn fill(&mut self, r: [f32; 4], argb: u32);
    fn quad(&mut self, corners: [[f32; 2]; 4], argb: u32);
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32);
    // An item's icon as the game's menus draw it (enchantment glint included), `size` pixels square.
    fn item(&mut self, _stack: &horizon_client::items::ItemStack, _at: [f32; 2], _size: f32) {}
    // A line of text at a size (1: the usual).
    fn scaled(&mut self, text: &str, at: [f32; 2], rgb: u32, _size: f32) {
        self.text(text, at, rgb);
    }
}

impl Canvas for Vec<Shape> {
    fn fill(&mut self, r: [f32; 4], argb: u32) {
        self.push(Shape::Fill(r, argb));
    }
    fn quad(&mut self, corners: [[f32; 2]; 4], argb: u32) {
        self.push(Shape::Quad(corners, argb));
    }
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.push(Shape::Text(text.to_owned(), at, rgb));
    }
    fn item(&mut self, stack: &horizon_client::items::ItemStack, at: [f32; 2], size: f32) {
        self.push(Shape::Item(stack.item.clone(), at, size));
    }
    fn scaled(&mut self, text: &str, at: [f32; 2], rgb: u32, size: f32) {
        self.push(Shape::Scaled(text.to_owned(), at, rgb, size));
    }
}

pub(super) struct Painter<'a, 'g> {
    pub(super) game: &'a Game<'g>,
    pub(super) batch: &'a mut HudBatch,
}

impl Canvas for Painter<'_, '_> {
    fn fill(&mut self, r: [f32; 4], argb: u32) {
        self.game.native_fill(self.batch, r, argb);
    }
    fn quad(&mut self, corners: [[f32; 2]; 4], argb: u32) {
        let [a, r, g, b] = argb.to_be_bytes();
        self.batch.fill_quad(&self.game.gui.atlas, corners, horizon_client::hud::rgba(r, g, b, a));
    }
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.game.native_text(self.batch, text, at, rgb, true);
    }
    fn item(&mut self, stack: &horizon_client::items::ItemStack, [x, y]: [f32; 2], size: f32) {
        let start = self.batch.vertices.len();
        super::sunset_hud_bars::plain(|| self.game.draw_item_model(self.batch, stack, x, y));
        self.batch.scale_since(start, x, y, size / 16.0, size / 16.0);
    }
    fn scaled(&mut self, text: &str, [x, y]: [f32; 2], rgb: u32, size: f32) {
        let start = self.batch.vertices.len();
        self.game.native_text(self.batch, text, [x, y], rgb, true);
        self.batch.scale_since(start, x, y, size, size);
    }
}

pub(super) fn outline(out: &mut impl Canvas, r: [f32; 4], argb: u32, lw: f32, fill: bool) {
    let [x0, y0, x1, y1] = r;
    if fill {
        out.fill([x0, y0, x1 - x0, y1 - y0], (argb & 0x00FF_FFFF) | 0x2800_0000);
    }
    out.fill([x0, y0, x1 - x0, lw], argb);
    out.fill([x0, y1 - lw, x1 - x0, lw], argb);
    out.fill([x0, y0, lw, y1 - y0], argb);
    out.fill([x1 - lw, y0, lw, y1 - y0], argb);
}

pub(super) fn health_colour(fraction: f32) -> u32 {
    if fraction > 0.66 {
        0x55FF55
    } else if fraction > 0.33 {
        0xFFFF55
    } else {
        0xFF5555
    }
}

// The frame's list of shapes, kept from frame to frame so it is not grown again each time.
pub(super) static SHAPES: Mutex<Vec<Shape>> = Mutex::new(Vec::new());

// A rounded rectangle: straight sides as plain fills, each corner a fan with one physical pixel of edge fade (`px`).
pub(super) fn round_fill(game: &Game<'_>, batch: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, which: [bool; 4], argb: u32, px: f32) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    if r < px {
        game.native_fill(batch, [x, y, w, h], argb);
        return;
    }
    game.native_fill(batch, [x + r, y, w - 2.0 * r, h], argb);
    if h > 2.0 * r {
        game.native_fill(batch, [x, y + r, r, h - 2.0 * r], argb);
        game.native_fill(batch, [x + w - r, y + r, r, h - 2.0 * r], argb);
    }
    let [a, red, g, b] = argb.to_be_bytes();
    let colour = horizon_client::hud::rgba(red, g, b, a);
    let (inner, outer) = ((r - px * 0.5).max(0.0), r + px * 0.5);
    let segments = (((r / px).sqrt() * 2.0).ceil() as usize).clamp(3, 16);
    let at = |c: [f32; 2], radius: f32, angle: f32| [c[0] + radius * angle.cos(), c[1] + radius * angle.sin()];
    let quarter = std::f32::consts::FRAC_PI_2;
    let corners = [([x + r, y + r], 2.0), ([x + w - r, y + r], 3.0), ([x + w - r, y + h - r], 0.0), ([x + r, y + h - r], 1.0)];
    for ((c, turn), round) in corners.into_iter().zip(which) {
        if !round {
            // A square corner: the square the arc would have cut.
            game.native_fill(batch, [c[0] - r * f32::from(u8::from(c[0] < x + w / 2.0)), c[1] - r * f32::from(u8::from(c[1] < y + h / 2.0)), r, r], argb);
            continue;
        }
        for i in 0..segments {
            let a0 = (turn + i as f32 / segments as f32) * quarter;
            let a1 = (turn + (i + 1) as f32 / segments as f32) * quarter;
            let (p0, p1) = (at(c, inner, a0), at(c, inner, a1));
            batch.fill_quad(&game.gui.atlas, [c, p0, p1, p1], colour);
            let start = batch.vertices.len();
            batch.fill_quad(&game.gui.atlas, [p0, at(c, outer, a0), at(c, outer, a1), p1], colour);
            // fill_quad's two triangles use corners 0,1,2,0,2,3.
            for (vertex, corner) in batch.vertices[start..].iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                if corner == 1 || corner == 2 {
                    vertex.color &= 0x00FF_FFFF;
                }
            }
        }
    }
}

// Draws a list of shapes (and empties it, keeping its room).
// Text whose colour has an alpha byte (other than 0 and FF: opaque) is drawn that see-through: the module list's
// rows fading in and out. `shadow`: text with its drop shadow (the HUD's Text shadow; the menu always has it).
pub(super) fn paint(game: &Game<'_>, batch: &mut HudBatch, shapes: &mut Vec<Shape>, px: f32, shadow: bool) {
    let mut clip = 0;
    for s in shapes.drain(..) {
        match s {
            Shape::Squircle(r, radius, argb) => super::sunset_dashboard::squircle(game, batch, r, radius, argb, px),
            Shape::Icon(glyph, at, argb, size) => crate::game::sunset_font::icon(game, batch, glyph, at, argb, size),
            Shape::ClipStart => clip = batch.vertices.len(),
            Shape::ClipEnd([x0, y0, x1, y1]) => batch.clip_since(clip, x0, y0, x1, y1),
            Shape::Fill(r, argb) => game.native_fill(batch, r, argb),
            Shape::Round(r, radius, argb) => round_fill(game, batch, r, radius, [true; 4], argb, px),
            Shape::Corners(r, radius, which, argb) => round_fill(game, batch, r, radius, which, argb, px),
            Shape::Quad(corners, argb) => {
                let [a, r, g, b] = argb.to_be_bytes();
                batch.fill_quad(&game.gui.atlas, corners, horizon_client::hud::rgba(r, g, b, a));
            }
            // A fading row keeps its alpha; drawn in Sunset's font like the rest.
            Shape::Text(t, at, argb) => crate::game::sunset_font::draw(game, batch, &t, at, argb, shadow),
            Shape::Head(t, at, rgb) => {
                crate::game::sunset_font::BOLD.store(true, Ordering::Relaxed);
                game.native_text(batch, &t, at, rgb, shadow);
                crate::game::sunset_font::BOLD.store(false, Ordering::Relaxed);
            }
            Shape::Item(item, [x, y], size) => {
                let start = batch.vertices.len();
                let stack = horizon_client::items::ItemStack { item, count: 1, damage: 0, extra: None };
                super::sunset_hud_bars::plain(|| game.draw_item_model(batch, &stack, x, y));
                batch.scale_since(start, x, y, size / 16.0, size / 16.0);
            }
            Shape::Sprite(name, [x, y, w, h], [a, b, c, d]) => {
                if let Some(r) = game.gui.atlas.sprites.get(name).copied() {
                    let (du, dv) = (r.u1 - r.u0, r.v1 - r.v0);
                    let part = horizon_client::hud::Rect { u0: r.u0 + du * a, v0: r.v0 + dv * b, u1: r.u0 + du * c, v1: r.v0 + dv * d, ..r };
                    batch.rect_uv(x, y, w, h, part, u32::MAX);
                }
            }
            Shape::Scaled(t, [x, y], rgb, size) => {
                let start = batch.vertices.len();
                game.native_text(batch, &t, [x, y], rgb, shadow);
                batch.scale_since(start, x, y, size, size);
            }
            Shape::Face(skin, [x, y], size) => {
                // The face is 8 by 8 at (8, 8) of the 64 by 64 skin, its hat beside it at (40, 8).
                let Some(r) = game.gui.atlas.sprites.get(skin.as_str()).copied() else {
                    continue;
                };
                let (du, dv) = (r.u1 - r.u0, r.v1 - r.v0);
                for u in [8.0, 40.0] {
                    let part = horizon_client::hud::Rect {
                        u0: r.u0 + du * u / 64.0,
                        v0: r.v0 + dv * 8.0 / 64.0,
                        u1: r.u0 + du * (u + 8.0) / 64.0,
                        v1: r.v0 + dv * 16.0 / 64.0,
                        ..r
                    };
                    batch.rect_uv(x, y, size, size, part, horizon_client::hud::rgba(255, 255, 255, 255));
                }
            }
        }
    }
}
