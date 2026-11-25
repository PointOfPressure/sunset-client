// Sunset's own UI layer, the part that needs no game: (1) the shapes its GPU pipeline draws (game/ui_renderer.rs: rounded
// rectangles by signed distance, with anti-aliased edges, gradients, borders, shadows and a blur of what is behind), kept
// in layers between parts of the frame's HUD so the game's text still goes over them; (2) a small layout and widget
// layer (rows, columns, padding, scrolling, clipping, hit-testing, hover/press/focus and animation timing), so a
// surface's rectangles are not placed by hand twice, once to draw and once to click.
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub type Rect = [f32; 4]; // x, y, width, height

// ---------------------------------------------------------------- shapes

// 0xAARRGGBB as straight red, green, blue, alpha in 0..1.
pub fn rgba(argb: u32) -> [f32; 4] {
    let [a, r, g, b] = argb.to_be_bytes();
    [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, f32::from(a) / 255.0]
}

// One shape of the pipeline: PRIM_FLOATS floats, in the order the shader's instance attributes take them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prim {
    pub rect: Rect,
    // Corner radii: top left, top right, bottom right, bottom left.
    pub radii: [f32; 4],
    pub fill: [f32; 4],
    // The gradient's far colour (the fill itself without one).
    pub fill2: [f32; 4],
    pub border: [f32; 4],
    // What it is cut to: x0, y0, x1, y1.
    pub clip: [f32; 4],
    // Border width; a shadow's blur (0: a solid shape); how much of the fill's see-through part shows the blurred
    // frame behind (0: none); the corners' power (2 a circle's, 4 a squircle's).
    pub params: [f32; 4],
    // The gradient's direction (x, y; 0, 0: none), opacity, unused.
    pub extra: [f32; 4],
}

pub const PRIM_FLOATS: usize = 32;
const NO_CLIP: [f32; 4] = [-1.0e9, -1.0e9, 1.0e9, 1.0e9];

impl Prim {
    pub fn new(rect: Rect, radius: f32, argb: u32) -> Self {
        let fill = rgba(argb);
        Self { rect, radii: [radius; 4], fill, fill2: fill, border: [0.0; 4], clip: NO_CLIP, params: [0.0, 0.0, 0.0, 2.0], extra: [0.0, 0.0, 1.0, 0.0] }
    }
    // Only some corners round: top left, top right, bottom right, bottom left.
    pub fn corners(mut self, which: [bool; 4]) -> Self {
        for (radius, round) in self.radii.iter_mut().zip(which) {
            if !round {
                *radius = 0.0;
            }
        }
        self
    }
    pub fn squircle(mut self) -> Self {
        self.params[3] = 4.0;
        self
    }
    // A gradient from the fill to argb along angle (degrees; 0 left to right, 90 top to bottom).
    pub fn gradient(mut self, argb: u32, angle: f32) -> Self {
        self.fill2 = rgba(argb);
        let (sin, cos) = angle.to_radians().sin_cos();
        self.extra[0] = cos;
        self.extra[1] = sin;
        self
    }
    pub fn border(mut self, width: f32, argb: u32) -> Self {
        self.params[0] = width;
        self.border = rgba(argb);
        self
    }
    // The blurred frame behind shows through the fill's see-through part (1: all of it).
    pub fn backdrop(mut self, amount: f32) -> Self {
        self.params[2] = amount.clamp(0.0, 1.0);
        self
    }
    // This shape's shadow, to be drawn before it: blur wide, moved by (dx, dy).
    pub fn shadow(&self, blur: f32, [dx, dy]: [f32; 2], argb: u32) -> Self {
        let [x, y, w, h] = self.rect;
        let fill = rgba(argb);
        Self { rect: [x + dx, y + dy, w, h], fill, fill2: fill, border: [0.0; 4], params: [0.0, blur.max(0.01), 0.0, self.params[3]], extra: [0.0, 0.0, 1.0, 0.0], ..*self }
    }
    // The rectangle its pixels can fall in.
    pub fn bounds(&self) -> [f32; 4] {
        let spread = self.params[1] * 2.0 + 1.0;
        let [x, y, w, h] = self.rect;
        [(x - spread).max(self.clip[0]), (y - spread).max(self.clip[1]), (x + w + spread).min(self.clip[2]), (y + h + spread).min(self.clip[3])]
    }
    pub fn floats(&self) -> [f32; PRIM_FLOATS] {
        let mut out = [0.0; PRIM_FLOATS];
        for (i, part) in [self.rect, self.radii, self.fill, self.fill2, self.border, self.clip, self.params, self.extra].iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(part);
        }
        out
    }
}

// A run of shapes drawn once the HUD's first at vertices are: prims[start..end].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cut {
    pub at: u32,
    pub start: u32,
    pub end: u32,
}

fn overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

// A frame's shapes, in layers. A shape joins the open layer unless HUD content drawn since that layer began lies
// over its place: then it would hide that content, so a new layer begins above it. A menu's row fills and its text
// so share a few layers (a few passes), however many rows there are.
#[derive(Default)]
pub struct Layers {
    pub prims: Vec<Prim>,
    pub cuts: Vec<Cut>,
    covered: Vec<[f32; 4]>,
    open: bool,
}

impl Layers {
    pub fn clear(&mut self) {
        self.prims.clear();
        self.cuts.clear();
        self.covered.clear();
        self.open = false;
    }
    // A shape drawn now, the HUD holding vertices so far.
    pub fn push(&mut self, prim: Prim, vertices: u32) {
        let bounds = prim.bounds();
        if !self.open || self.covered.iter().any(|c| overlap(*c, bounds)) {
            self.begin(vertices);
        }
        self.prims.push(prim);
        if let Some(cut) = self.cuts.last_mut() {
            cut.end = self.prims.len() as u32;
        }
    }
    fn begin(&mut self, vertices: u32) {
        let at = self.prims.len() as u32;
        self.covered.clear();
        self.open = true;
        if self.cuts.last().is_none_or(|c| c.at != vertices) {
            self.cuts.push(Cut { at: vertices, start: at, end: at });
        }
    }
    // HUD content was drawn over [x0, y0, x1, y1].
    pub fn cover(&mut self, bounds: [f32; 4]) {
        if self.open && bounds[2] > bounds[0] && bounds[3] > bounds[1] {
            self.covered.push(bounds);
        }
    }
    // A cut with nothing in it: the pipeline is asked whether it can draw on this frame.
    pub fn probe(&mut self, vertices: u32) {
        self.begin(vertices);
        self.open = false;
    }
    // Whoever draws next starts a layer of their own (their coordinates may not be these).
    pub fn seal(&mut self) {
        self.open = false;
        self.covered.clear();
    }
    // The shapes from since on, cut to a rectangle.
    pub fn clip_since(&mut self, since: usize, [x0, y0, x1, y1]: [f32; 4]) {
        for p in self.prims.iter_mut().skip(since) {
            p.clip = [p.clip[0].max(x0), p.clip[1].max(y0), p.clip[2].min(x1), p.clip[3].min(y1)];
        }
    }
    // The shapes from since on moved down by dy, faded to opacity, then scaled about the origin.
    pub fn place_since(&mut self, since: usize, dy: f32, opacity: f32, scale: f32) {
        for p in self.prims.iter_mut().skip(since) {
            p.rect = [p.rect[0] * scale, (p.rect[1] + dy) * scale, p.rect[2] * scale, p.rect[3] * scale];
            p.radii = p.radii.map(|r| r * scale);
            p.clip = [p.clip[0] * scale, (p.clip[1] + dy) * scale, p.clip[2] * scale, (p.clip[3] + dy) * scale];
            p.params[0] *= scale;
            p.params[1] *= scale;
            p.extra[2] *= opacity;
        }
    }
    pub fn cut_ats(&self) -> Vec<u32> {
        self.cuts.iter().map(|c| c.at).collect()
    }
}

// The frame being built, and the one the renderer draws (the same thread builds and draws a frame).
pub static FRAME: Mutex<Layers> = Mutex::new(Layers { prims: Vec::new(), cuts: Vec::new(), covered: Vec::new(), open: false });
// The pipeline drew on the last frame it was asked to: shapes may go to it rather than to the HUD's triangles.
pub static SUPPORTED: AtomicBool = AtomicBool::new(false);
// The frame's texture could be copied: a blur of what is behind is drawn.
pub static BLURS: AtomicBool = AtomicBool::new(false);
// ClickGUI > GPU shapes.
pub static WANTED: AtomicBool = AtomicBool::new(true);

// Counts the frames begun: the renderer uploads a frame's shapes once.
pub static GENERATION: AtomicU64 = AtomicU64::new(0);

// A frame begins with no shapes. True when the last one had cuts (the renderer is told there are none now).
pub fn begin_frame() -> bool {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    frame(|layers| {
        let had = !layers.cuts.is_empty();
        layers.clear();
        had
    })
}

pub fn frame<R>(f: impl FnOnce(&mut Layers) -> R) -> R {
    f(&mut FRAME.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
}

pub fn active() -> bool {
    WANTED.load(Ordering::Relaxed) && SUPPORTED.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------- layout

pub fn inside([x, y, w, h]: Rect, (px, py): (f32, f32)) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

pub fn inset([x, y, w, h]: Rect, [left, top, right, bottom]: [f32; 4]) -> Rect {
    [x + left, y + top, (w - left - right).max(0.0), (h - top - bottom).max(0.0)]
}

// The part of a both b cover (None: nothing).
pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let (x0, y0) = (a[0].max(b[0]), a[1].max(b[1]));
    let (x1, y1) = ((a[0] + a[2]).min(b[0] + b[2]), (a[1] + a[3]).min(b[1] + b[3]));
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

// A column or a row being filled: each take cuts the next piece off the room left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flow {
    room: Rect,
    down: bool,
    gap: f32,
    first: bool,
}

impl Flow {
    // Pieces go down the rectangle, gap apart.
    pub fn column(room: Rect, gap: f32) -> Self {
        Self { room, down: true, gap, first: true }
    }
    // Pieces go along the rectangle, gap apart.
    pub fn row(room: Rect, gap: f32) -> Self {
        Self { room, down: false, gap, first: true }
    }
    pub fn padded(mut self, pad: [f32; 4]) -> Self {
        self.room = inset(self.room, pad);
        self
    }
    // What is left.
    pub fn room(&self) -> Rect {
        self.room
    }
    fn skip_gap(&mut self) {
        if !std::mem::replace(&mut self.first, false) {
            self.skip(self.gap);
        }
    }
    // Empty room.
    pub fn skip(&mut self, size: f32) {
        let [x, y, w, h] = self.room;
        self.room = if self.down { [x, y + size, w, (h - size).max(0.0)] } else { [x + size, y, (w - size).max(0.0), h] };
    }
    // The next piece from the start: size tall in a column, wide in a row (no more than is left).
    pub fn take(&mut self, size: f32) -> Rect {
        self.skip_gap();
        let [x, y, w, h] = self.room;
        let size = size.min(if self.down { h } else { w }).max(0.0);
        self.skip(size);
        if self.down { [x, y, w, size] } else { [x, y, size, h] }
    }
    // A piece from the far end (a footer, a button at the right).
    pub fn take_end(&mut self, size: f32) -> Rect {
        let [x, y, w, h] = self.room;
        let size = size.min(if self.down { h } else { w }).max(0.0);
        let gap = self.gap.min(if self.down { h } else { w } - size).max(0.0);
        if self.down {
            self.room = [x, y, w, (h - size - gap).max(0.0)];
            [x, y + h - size, w, size]
        } else {
            self.room = [x, y, (w - size - gap).max(0.0), h];
            [x + w - size, y, size, h]
        }
    }
    // All that is left.
    pub fn rest(&mut self) -> Rect {
        self.skip_gap();
        let room = self.room;
        self.skip(if self.down { room[3] } else { room[2] });
        room
    }
}

// A list of equal rows in a view that scrolls: which are seen, where, and which one a point is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroll {
    pub view: Rect,
    pub row: f32,
    pub count: usize,
    pub offset: f32,
}

impl Scroll {
    pub fn new(view: Rect, row: f32, count: usize, offset: f32) -> Self {
        let mut s = Self { view, row, count, offset: 0.0 };
        s.offset = offset.clamp(0.0, s.max());
        s
    }
    // How far it can scroll.
    pub fn max(&self) -> f32 {
        (self.count as f32 * self.row - self.view[3]).max(0.0)
    }
    // After steps of the wheel (a step up is positive), rows a step.
    pub fn wheel(&self, steps: f32, rows: f32) -> f32 {
        (self.offset - steps * self.row * rows).clamp(0.0, self.max())
    }
    // The rows with any part in view, top first: (index, rectangle; it may reach past the view, which cuts it).
    pub fn seen(&self) -> impl Iterator<Item = (usize, Rect)> + '_ {
        let first = (self.offset / self.row) as usize;
        (first..self.count)
            .map(|n| (n, [self.view[0], self.view[1] + n as f32 * self.row - self.offset, self.view[2], self.row]))
            .take_while(|(_, r)| r[1] < self.view[1] + self.view[3])
    }
    // Whether all of a row is in view (an icon that can't be cut waits for that).
    pub fn whole(&self, row: Rect) -> bool {
        row[1] >= self.view[1] && row[1] + row[3] <= self.view[1] + self.view[3]
    }
    // The row under a point (None outside the view or past the last row).
    pub fn row_at(&self, at: (f32, f32)) -> Option<usize> {
        if !inside(self.view, at) {
            return None;
        }
        let n = ((at.1 - self.view[1] + self.offset) / self.row) as usize;
        (n < self.count).then_some(n)
    }
    // The view as a clip: x0, y0, x1, y1.
    pub fn clip(&self) -> [f32; 4] {
        [self.view[0], self.view[1], self.view[0] + self.view[2], self.view[1] + self.view[3]]
    }
}

// ---------------------------------------------------------------- widgets

// What names a widget from frame to frame: its surface and its place in it.
pub type Id = (u32, u32);

// The pointer this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pointer {
    pub at: (f32, f32),
    pub down: bool,
}

// What the pointer is doing to a widget.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    pub hovered: bool,
    // Held down, having gone down on it.
    pub pressed: bool,
    // Let go over it after going down on it.
    pub clicked: bool,
    pub focused: bool,
    // 0 to 1: how far its hover has faded in.
    pub glow: f32,
}

// Widgets' state between frames: the one under the pointer, the one held, the one with the keyboard, and every
// running fade.
#[derive(Default)]
pub struct Widgets {
    hot: Option<Id>,
    seen_hot: Option<Id>,
    active: Option<Id>,
    focus: Option<Id>,
    was_down: bool,
    pointer: Pointer,
    now: f64,
    dt: f32,
    fades: HashMap<Id, f32>,
    clips: Vec<Rect>,
}

// Seconds a hover takes to fade in or out.
pub const FADE: f32 = 0.12;

impl Widgets {
    // A frame begins: the pointer, and the time in seconds.
    pub fn begin(&mut self, pointer: Pointer, now: f64) {
        self.dt = if self.now > 0.0 { ((now - self.now) as f32).clamp(0.0, 0.25) } else { 0.0 };
        self.now = now;
        self.hot = self.seen_hot.take();
        self.pointer = pointer;
        self.clips.clear();
        // Fades that ran out are forgotten.
        self.fades.retain(|id, v| *v > 0.001 || Some(*id) == self.hot);
    }
    // A frame ends: a press let go is over.
    pub fn end(&mut self) {
        if !self.pointer.down {
            self.active = None;
        }
        self.was_down = self.pointer.down;
    }
    // Widgets until pop_clip are only hit inside the rectangle (a scrolling list's view).
    pub fn push_clip(&mut self, clip: Rect) {
        let clip = self.clips.last().map_or(Some(clip), |c| intersect(*c, clip)).unwrap_or([0.0; 4]);
        self.clips.push(clip);
    }
    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }
    fn hit(&self, rect: Rect) -> bool {
        inside(rect, self.pointer.at) && self.clips.last().is_none_or(|c| inside(*c, self.pointer.at))
    }
    // A widget at rect this frame. Of widgets one over another the last made wins the pointer, a frame late.
    pub fn interact(&mut self, id: Id, rect: Rect) -> Response {
        let over = self.hit(rect);
        if over {
            self.seen_hot = Some(id);
        }
        let hovered = over && self.hot == Some(id) && self.active.is_none_or(|a| a == id);
        let went_down = self.pointer.down && !self.was_down;
        if hovered && went_down {
            self.active = Some(id);
            self.focus = Some(id);
        }
        let pressed = self.active == Some(id) && self.pointer.down;
        let clicked = self.active == Some(id) && !self.pointer.down && self.was_down && over;
        let glow = self.fade(id, hovered || pressed, FADE);
        Response { hovered, pressed, clicked, focused: self.focus == Some(id), glow }
    }
    // A value that runs to 1 while on and back to 0 while off, over seconds.
    pub fn fade(&mut self, id: Id, on: bool, seconds: f32) -> f32 {
        let step = if seconds > 0.0 { self.dt / seconds } else { 1.0 };
        let v = self.fades.entry(id).or_insert(0.0);
        *v = if on { (*v + step).min(1.0) } else { (*v - step).max(0.0) };
        // Eased: fast at first.
        1.0 - (1.0 - *v) * (1.0 - *v)
    }
    pub fn focus(&self) -> Option<Id> {
        self.focus
    }
    pub fn set_focus(&mut self, id: Option<Id>) {
        self.focus = id;
    }
    // Whether any fade is still running (the surface should be drawn again).
    pub fn animating(&self) -> bool {
        self.fades.values().any(|v| *v > 0.001 && *v < 0.999)
    }
}

// Blends colour a towards b by t (0xAARRGGBB each).
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let (a, b) = (a.to_be_bytes(), b.to_be_bytes());
    u32::from_be_bytes(std::array::from_fn(|i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t.clamp(0.0, 1.0)).round() as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_share_a_layer_until_text_lies_over_them() {
        let mut l = Layers::default();
        // A panel, then two rows' fills and texts: one layer, as no fill lies under an earlier text.
        l.push(Prim::new([0.0, 0.0, 100.0, 100.0], 4.0, 0xFF10_1010), 10);
        l.cover([4.0, 4.0, 60.0, 12.0]);
        l.push(Prim::new([0.0, 20.0, 100.0, 12.0], 2.0, 0x40FF_FFFF), 40);
        l.cover([4.0, 22.0, 60.0, 30.0]);
        assert_eq!(l.cuts, vec![Cut { at: 10, start: 0, end: 2 }]);
        // A window over the first row's text: a new layer, above that text.
        l.push(Prim::new([2.0, 2.0, 50.0, 50.0], 4.0, 0xFF20_2020), 70);
        assert_eq!(l.cuts, vec![Cut { at: 10, start: 0, end: 2 }, Cut { at: 70, start: 2, end: 3 }]);
        // Its own rows join it.
        l.cover([4.0, 4.0, 40.0, 12.0]);
        l.push(Prim::new([2.0, 20.0, 50.0, 12.0], 2.0, 0x40FF_FFFF), 90);
        assert_eq!(l.cuts.len(), 2);
        assert_eq!(l.cuts[1].end, 4);
        assert_eq!(l.cut_ats(), vec![10, 70]);
    }

    #[test]
    fn a_sealed_or_probed_layer_takes_no_more() {
        let mut l = Layers::default();
        l.probe(5);
        assert_eq!(l.cuts, vec![Cut { at: 5, start: 0, end: 0 }]);
        // The same place in the HUD: the probe's cut is used, not a second one.
        l.push(Prim::new([0.0, 0.0, 10.0, 10.0], 0.0, 0xFFFF_FFFF), 5);
        assert_eq!(l.cuts, vec![Cut { at: 5, start: 0, end: 1 }]);
        l.seal();
        l.push(Prim::new([20.0, 0.0, 10.0, 10.0], 0.0, 0xFFFF_FFFF), 9);
        assert_eq!(l.cuts.len(), 2);
        l.clear();
        assert!(l.cuts.is_empty() && l.prims.is_empty());
    }

    #[test]
    fn a_shadow_spreads_its_bounds_and_a_clip_cuts_them() {
        let p = Prim::new([10.0, 10.0, 20.0, 20.0], 3.0, 0xFF00_0000);
        assert_eq!(p.bounds(), [9.0, 9.0, 31.0, 31.0]);
        let s = p.shadow(4.0, [0.0, 2.0], 0x8000_0000);
        assert_eq!(s.bounds(), [1.0, 3.0, 39.0, 41.0]);
        assert_eq!(s.fill[3], 128.0 / 255.0);
        let mut l = Layers::default();
        l.push(s, 0);
        l.clip_since(0, [5.0, 5.0, 25.0, 25.0]);
        assert_eq!(l.prims[0].bounds(), [5.0, 5.0, 25.0, 25.0]);
        l.place_since(0, 1.0, 0.5, 2.0);
        assert_eq!(l.prims[0].rect, [20.0, 26.0, 40.0, 40.0]);
        assert_eq!(l.prims[0].params[1], 8.0);
        assert_eq!(l.prims[0].extra[2], 0.5);
        assert_eq!(l.prims[0].clip, [10.0, 12.0, 50.0, 52.0]);
        assert_eq!(Prim::new([0.0; 4], 5.0, 0).corners([true, false, false, true]).radii, [5.0, 0.0, 0.0, 5.0]);
        assert_eq!(p.floats().len(), PRIM_FLOATS);
        assert_eq!(p.floats()[28..31], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn flows_cut_pieces_with_gaps_and_ends() {
        let mut c = Flow::column([10.0, 10.0, 100.0, 200.0], 4.0).padded([6.0, 0.0, 6.0, 0.0]);
        assert_eq!(c.take(22.0), [16.0, 10.0, 88.0, 22.0]);
        assert_eq!(c.take(16.0), [16.0, 36.0, 88.0, 16.0]);
        let foot = c.take_end(20.0);
        assert_eq!(foot, [16.0, 190.0, 88.0, 20.0]);
        assert_eq!(c.rest(), [16.0, 56.0, 88.0, 130.0]);
        assert_eq!(c.take(5.0)[3], 0.0);
        let mut r = Flow::row(foot, 2.0);
        assert_eq!(r.take_end(30.0), [74.0, 190.0, 30.0, 20.0]);
        assert_eq!(r.take(10.0), [16.0, 190.0, 10.0, 20.0]);
        assert_eq!(r.rest(), [28.0, 190.0, 44.0, 20.0]);
        // No more than there is.
        assert_eq!(Flow::row([0.0, 0.0, 5.0, 5.0], 0.0).take(50.0), [0.0, 0.0, 5.0, 5.0]);
        assert_eq!(intersect([0.0, 0.0, 10.0, 10.0], [5.0, 5.0, 10.0, 10.0]), Some([5.0, 5.0, 5.0, 5.0]));
        assert_eq!(intersect([0.0, 0.0, 10.0, 10.0], [10.0, 0.0, 5.0, 5.0]), None);
    }

    #[test]
    fn a_scrolled_list_shows_and_hits_the_right_rows() {
        let s = Scroll::new([0.0, 100.0, 50.0, 30.0], 12.0, 10, 18.0);
        assert_eq!(s.max(), 90.0);
        let seen: Vec<_> = s.seen().collect();
        assert_eq!(seen.iter().map(|r| r.0).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(seen[0].1, [0.0, 94.0, 50.0, 12.0]);
        assert!(!s.whole(seen[0].1) && s.whole(seen[1].1) && s.whole(seen[2].1));
        assert_eq!(s.row_at((5.0, 101.0)), Some(1));
        assert_eq!(s.row_at((5.0, 129.0)), Some(3));
        assert_eq!(s.row_at((5.0, 131.0)), None);
        assert_eq!(s.row_at((60.0, 110.0)), None);
        // Past the end is clamped; a short list doesn't scroll or hit below its rows.
        assert_eq!(Scroll::new(s.view, 12.0, 10, 500.0).offset, 90.0);
        assert_eq!(s.wheel(1.0, 3.0), 0.0);
        assert_eq!(s.wheel(-1.0, 3.0), 54.0);
        let short = Scroll::new(s.view, 12.0, 1, 40.0);
        assert_eq!((short.offset, short.row_at((5.0, 120.0))), (0.0, None));
        assert_eq!(Scroll::new(s.view, 12.0, 0, 0.0).seen().count(), 0);
    }

    #[test]
    fn widgets_hover_press_click_and_fade() {
        let mut w = Widgets::default();
        let (a, b) = ((1, 0), (1, 1));
        let (ra, rb) = ([0.0, 0.0, 10.0, 10.0], [5.0, 0.0, 10.0, 10.0]);
        let frame = |w: &mut Widgets, at: (f32, f32), down: bool, now: f64| {
            w.begin(Pointer { at, down }, now);
            let out = (w.interact(a, ra), w.interact(b, rb));
            w.end();
            out
        };
        // Where two overlap the later one has the pointer, from the second frame on.
        frame(&mut w, (7.0, 5.0), false, 1.0);
        let (ha, hb) = frame(&mut w, (7.0, 5.0), false, 1.06);
        assert!(!ha.hovered && hb.hovered);
        assert!(hb.glow > 0.0 && hb.glow < 1.0);
        let (_, hb) = frame(&mut w, (7.0, 5.0), false, 1.5);
        assert_eq!(hb.glow, 1.0);
        // Down on b, dragged off: still pressed, a isn't hovered meanwhile, and no click on letting go outside.
        let (_, pb) = frame(&mut w, (7.0, 5.0), true, 1.6);
        assert!(pb.pressed && pb.focused && !pb.clicked);
        let (ha, pb) = frame(&mut w, (2.0, 5.0), true, 1.7);
        assert!(pb.pressed && !ha.hovered);
        let (_, pb) = frame(&mut w, (2.0, 5.0), false, 1.8);
        assert!(!pb.clicked && !pb.pressed);
        // Down and up on a: a click, once.
        frame(&mut w, (2.0, 5.0), false, 1.9);
        frame(&mut w, (2.0, 5.0), true, 2.0);
        let (ca, _) = frame(&mut w, (2.0, 5.0), false, 2.1);
        assert!(ca.clicked && ca.focused);
        let (ca, _) = frame(&mut w, (2.0, 5.0), false, 2.2);
        assert!(!ca.clicked);
        // Held down before coming over it: not a press.
        frame(&mut w, (50.0, 50.0), true, 2.3);
        let (na, _) = frame(&mut w, (2.0, 5.0), true, 2.4);
        assert!(!na.pressed);
        // Outside a clip nothing is hit.
        w.begin(Pointer { at: (2.0, 5.0), down: false }, 3.0);
        w.push_clip([20.0, 20.0, 5.0, 5.0]);
        assert!(!w.interact(a, ra).hovered);
        w.pop_clip();
        w.end();
        w.begin(Pointer { at: (2.0, 5.0), down: false }, 3.1);
        w.push_clip([20.0, 20.0, 5.0, 5.0]);
        assert!(!w.interact(a, ra).hovered);
        w.end();
        assert_eq!(mix(0xFF00_0000, 0xFFFF_FFFF, 0.5), 0xFF80_8080);
        assert_eq!(mix(0x0000_0000, 0x40FF_FFFF, 1.0), 0x40FF_FFFF);
    }
}
