// Where the scoreboard sidebar would draw, and how far to move it: shared between the game crate (which frames it for
// the chat layout editor and sets the offset from where it was dragged) and the render crate's sidebar draw (which
// measures the natural box and applies the offset). Written anew for Sunset.
use horizon_client::hud_layout::{GuiDraw, PoseOp};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering::Relaxed};

// The natural box of the sidebar (GUI pixels), as the last draw measured it.
static NAT_X: AtomicI32 = AtomicI32::new(0);
static NAT_Y: AtomicI32 = AtomicI32::new(0);
static NAT_W: AtomicI32 = AtomicI32::new(0);
static NAT_H: AtomicI32 = AtomicI32::new(0);
// How far the HUD wants it moved from that box.
static OFF_X: AtomicI32 = AtomicI32::new(0);
static OFF_Y: AtomicI32 = AtomicI32::new(0);
static CUSTOM: AtomicBool = AtomicBool::new(false);
// Its size from the layout editor (an f32's bits; 1 the game's own).
static SIZE: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
// It drew this frame (cleared as the HUD reads it, so a sidebar that stops showing stops being framed).
static SHOWN: AtomicBool = AtomicBool::new(false);

// The sidebar just built `draws` at their natural place: record the box they cover, then slide them if a place of its
// own was set, and at a size of its own draw them that much bigger or smaller about the moved box's top left corner (the
// game's own text, stretched: it has no other size to draw). Called from the sidebar's hook.
pub fn apply(draws: &mut Vec<GuiDraw>) {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for d in draws.iter() {
        let (ax, ay, bx, by) = match d {
            GuiDraw::Fill { x0, y0, x1, y1, .. } => (*x0, *y0, *x1, *y1),
            GuiDraw::Text { x, y, .. } => (*x, *y, *x, *y + 9),
            _ => continue,
        };
        x0 = x0.min(ax);
        y0 = y0.min(ay);
        x1 = x1.max(bx);
        y1 = y1.max(by);
    }
    if x0 == i32::MAX {
        return;
    }
    NAT_X.store(x0, Relaxed);
    NAT_Y.store(y0, Relaxed);
    NAT_W.store(x1 - x0, Relaxed);
    NAT_H.store(y1 - y0, Relaxed);
    SHOWN.store(true, Relaxed);
    let (mut dx, mut dy) = (0, 0);
    if CUSTOM.load(Relaxed) {
        (dx, dy) = (OFF_X.load(Relaxed), OFF_Y.load(Relaxed));
        if dx != 0 || dy != 0 {
            for d in draws.iter_mut() {
                shift(d, dx, dy);
            }
        }
    }
    let size = f32::from_bits(SIZE.load(Relaxed));
    if size.is_finite() && size > 0.0 && (size - 1.0).abs() > 0.001 {
        let (ox, oy) = ((x0 + dx) as f32, (y0 + dy) as f32);
        draws.insert(0, GuiDraw::Push(vec![PoseOp::Translate(ox, oy), PoseOp::Scale(size, size), PoseOp::Translate(-ox, -oy)]));
        draws.push(GuiDraw::Pop);
    }
}

// Move one draw by (dx, dy): every coordinate it carries.
fn shift(d: &mut GuiDraw, dx: i32, dy: i32) {
    match d {
        GuiDraw::Fill { x0, y0, x1, y1, .. } => {
            *x0 += dx;
            *x1 += dx;
            *y0 += dy;
            *y1 += dy;
        }
        GuiDraw::Text { x, y, .. }
        | GuiDraw::Sprite { x, y, .. }
        | GuiDraw::AtlasSprite { x, y, .. }
        | GuiDraw::Blit { x, y, .. }
        | GuiDraw::Item { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        GuiDraw::FillGradient { x0, y0, x1, y1, .. } => {
            *x0 += dx;
            *x1 += dx;
            *y0 += dy;
            *y1 += dy;
        }
        GuiDraw::Push(_) | GuiDraw::Pop => {}
    }
}

// The natural box of the sidebar this frame (x, y, w, h in GUI pixels), if it drew; cleared as it is read.
pub fn natural() -> Option<(i32, i32, i32, i32)> {
    SHOWN
        .swap(false, Relaxed)
        .then(|| (NAT_X.load(Relaxed), NAT_Y.load(Relaxed), NAT_W.load(Relaxed), NAT_H.load(Relaxed)))
}

// How far to move the sidebar from its natural place (`moved` false to leave it) and its size (1 the game's own): set
// by the HUD from where it was dragged and the size its frame was given.
pub fn set_offset(moved: bool, dx: i32, dy: i32, size: f32) {
    CUSTOM.store(moved, Relaxed);
    OFF_X.store(dx, Relaxed);
    OFF_Y.store(dy, Relaxed);
    SIZE.store(size.to_bits(), Relaxed);
}
