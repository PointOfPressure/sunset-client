// Compile the actual bridge against a recording HUD; no game or GPU needed.
#[path = "../mixins/sunset/crosshair_flags.rs"]
mod crosshair_flags;

#[derive(Clone, Copy, PartialEq)]
struct Rect { u0: f32, v0: f32, u1: f32, v1: f32 }
const MARK: Rect = Rect { u0: 0.1, v0: 0.2, u1: 0.3, v1: 0.4 };
const INDICATOR: Rect = Rect { u0: 0.5, v0: 0.6, u1: 0.7, v1: 0.8 };
struct HudAtlas;
struct HudBatch {
    scale: f32,
    width: f32,
    height: f32,
    fills: Vec<([[f32; 2]; 4], u32)>,
    vertices: usize,
    marks: usize,
    indicators: usize,
    inverts: Vec<std::ops::Range<usize>>,
}
impl HudBatch {
    fn rect_uv(&mut self, x: f32, y: f32, w: f32, h: f32, rect: Rect, color: u32) {
        if crosshair_flags::suppress_mark(self, x, y, w, h, rect, color) { return; }
        if rect == MARK { self.marks += 1; } else { self.indicators += 1; }
        self.vertices += 6;
    }
    fn invert_since(&mut self, start: usize) {
        if start != self.vertices { self.inverts.push(start..self.vertices); }
    }
    fn fill_quad(&mut self, _: &HudAtlas, corners: [[f32; 2]; 4], color: u32) {
        self.fills.push((corners, color));
        self.vertices += 6;
    }
}
fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 { u32::from_le_bytes([r, g, b, a]) }
fn batch(scale: f32) -> HudBatch {
    HudBatch { scale, width: 1280.0, height: 720.0, fills: Vec::new(), vertices: 0, marks: 0, indicators: 0, inverts: Vec::new() }
}
fn frame() -> crosshair_flags::Frame {
    let mut frame = crosshair_flags::Frame::empty();
    frame.set_crosshair([MARK.u0, MARK.v0, MARK.u1, MARK.v1]);
    frame.push([960.0, 540.0, 6.0, 3.0], 0xFF12_3456);
    frame
}
fn vanilla_crosshair(batch: &mut HudBatch) {
    let start = batch.vertices;
    batch.rect_uv(632.0, 352.0, 15.0, 15.0, MARK, u32::MAX);
    batch.invert_since(start);
    // Attack indicator UVs and geometry must continue through the host.
    for _ in 0..2 {
        let start = batch.vertices;
        batch.rect_uv(632.0, 368.0, 16.0, 4.0, INDICATOR, u32::MAX);
        batch.invert_since(start);
    }
}

#[test]
fn replacement_keeps_indicator_inversion_and_never_inverts_custom_color() {
    crosshair_flags::set_frame(frame());
    let mut custom = batch(1.5);
    vanilla_crosshair(&mut custom);
    assert_eq!((custom.marks, custom.indicators), (0, 2));
    assert_eq!(custom.inverts, [0..6, 6..12]);
    crosshair_flags::draw(&mut custom, &HudAtlas);
    assert_eq!(custom.fills[0], ([[640.0, 360.0], [644.0, 360.0], [644.0, 362.0], [640.0, 362.0]], rgba(0x12, 0x34, 0x56, 0xFF)));
    assert_eq!(custom.inverts, [0..6, 6..12]);
    crosshair_flags::draw(&mut custom, &HudAtlas);
    assert_eq!(custom.fills.len(), 1);
    crosshair_flags::set_frame(crosshair_flags::Frame::empty());
    let mut restored = batch(1.5);
    vanilla_crosshair(&mut restored);
    crosshair_flags::draw(&mut restored, &HudAtlas);
    assert_eq!((restored.marks, restored.indicators, restored.fills.len()), (1, 2, 0));
    assert_eq!(restored.inverts, [0..6, 6..12, 12..18]);
}

#[test]
fn only_exact_mark_uv_position_dimensions_and_color_are_suppressed() {
    for (x, y, w, h, rect, color) in [
        (631.0, 352.0, 15.0, 15.0, MARK, u32::MAX),
        (632.0, 351.0, 15.0, 15.0, MARK, u32::MAX),
        (632.0, 352.0, 14.0, 15.0, MARK, u32::MAX),
        (632.0, 352.0, 15.0, 14.0, MARK, u32::MAX),
        (632.0, 352.0, 15.0, 15.0, INDICATOR, u32::MAX),
        (632.0, 352.0, 15.0, 15.0, MARK, 0xFFFF_FFFE),
    ] {
        crosshair_flags::set_frame(frame());
        let mut hud = batch(1.5);
        hud.rect_uv(x, y, w, h, rect, color);
        crosshair_flags::draw(&mut hud, &HudAtlas);
        assert_eq!(hud.vertices, 6);
        assert!(hud.fills.is_empty());
    }
}

#[test]
fn new_frame_reset_clears_pending_draw_and_invalid_scale_falls_back() {
    crosshair_flags::set_frame(frame());
    let mut hud = batch(1.5);
    vanilla_crosshair(&mut hud);
    crosshair_flags::set_frame(crosshair_flags::Frame::empty());
    crosshair_flags::draw(&mut hud, &HudAtlas);
    assert!(hud.fills.is_empty());
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        crosshair_flags::set_frame(frame());
        let mut invalid = batch(scale);
        vanilla_crosshair(&mut invalid);
        crosshair_flags::draw(&mut invalid, &HudAtlas);
        assert_eq!(invalid.marks, 1);
        assert!(invalid.fills.is_empty());
    }
}

#[test]
fn renderer_threads_start_inactive_and_cannot_reset_each_others_frame() {
    crosshair_flags::set_frame(frame());
    std::thread::spawn(|| {
        let mut initially_inactive = batch(1.5);
        vanilla_crosshair(&mut initially_inactive);
        assert_eq!(initially_inactive.marks, 1);
        crosshair_flags::set_frame(frame());
        let mut active = batch(1.5);
        vanilla_crosshair(&mut active);
        crosshair_flags::draw(&mut active, &HudAtlas);
        assert_eq!((active.marks, active.fills.len()), (0, 1));
        crosshair_flags::set_frame(crosshair_flags::Frame::empty());
    }).join().unwrap();
    let mut parent = batch(1.5);
    vanilla_crosshair(&mut parent);
    crosshair_flags::draw(&mut parent, &HudAtlas);
    assert_eq!((parent.marks, parent.fills.len()), (0, 1));
    crosshair_flags::set_frame(crosshair_flags::Frame::empty());
}
