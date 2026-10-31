// Public bridge in horizon_client::hud. The renderer thread publishes one
// small fixed frame at hud_layers HEAD; HUD emission consumes no game locks.

use super::{HudAtlas, HudBatch, Rect, rgba};
use std::cell::Cell;

#[derive(Clone, Copy)]
pub struct Frame {
    rects: [[f32; 4]; 10],
    colors: [u32; 10],
    len: usize,
    crosshair_uv: Option<[f32; 4]>,
}

impl Frame {
    pub const fn empty() -> Self {
        Self { rects: [[0.0; 4]; 10], colors: [0; 10], len: 0, crosshair_uv: None }
    }

    pub fn set_crosshair(&mut self, uv: [f32; 4]) {
        self.crosshair_uv = Some(uv);
    }

    pub fn push(&mut self, rect: [f32; 4], argb: u32) {
        if self.len < self.rects.len() {
            self.rects[self.len] = rect;
            self.colors[self.len] = argb;
            self.len += 1;
        }
    }
}

thread_local! {
    static FRAME: Cell<Frame> = const { Cell::new(Frame::empty()) };
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static MARK: Cell<Option<[f32; 4]>> = const { Cell::new(None) };
    static PENDING: Cell<bool> = const { Cell::new(false) };
}

pub fn set_frame(frame: Frame) {
    FRAME.with(|current| current.set(frame));
    MARK.with(|mark| mark.set(frame.crosshair_uv));
    PENDING.with(|pending| pending.set(false));
    ACTIVE.with(|active| active.set(frame.len != 0 && frame.crosshair_uv.is_some()));
}

// The vanilla mark is the white 15x15 sprite at this integer GUI origin.
// UV comparison uses the actual resource atlas. Suppressing zero vertices
// leaves invert_since(start) empty; attack-indicator ranges stay untouched.
pub fn suppress_mark(batch: &HudBatch, x: f32, y: f32, w: f32, h: f32, rect: Rect, color: u32) -> bool {
    if !ACTIVE.with(Cell::get) || w != 15.0 || h != 15.0 || color != u32::MAX
        || x != ((batch.width as i32 - 15) / 2) as f32
        || y != ((batch.height as i32 - 15) / 2) as f32
        || !batch.scale.is_finite() || batch.scale <= 0.0
    {
        return false;
    }
    if MARK.with(Cell::get) != Some([rect.u0, rect.v0, rect.u1, rect.v1]) {
        return false;
    }
    PENDING.with(|pending| pending.set(true));
    true
}

pub fn draw(batch: &mut HudBatch, atlas: &HudAtlas) {
    if !PENDING.with(|pending| pending.replace(false)) {
        return;
    }
    // Later HUD sprite calls take the inactive fast path. Every frame resets
    // both flags before any eligibility gates, including early screen exits.
    ACTIVE.with(|active| active.set(false));
    let frame = FRAME.with(Cell::get);
    for i in 0..frame.len {
        let [x, y, w, h] = frame.rects[i].map(|v| v / batch.scale);
        let [a, r, g, b] = frame.colors[i].to_be_bytes();
        batch.fill_quad(atlas, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], rgba(r, g, b, a));
    }
}
