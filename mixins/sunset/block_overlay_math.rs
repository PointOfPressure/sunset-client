// Block Overlay's rules and shapes: when it shows, valid settings, the outline's boxes moved to the block.

/// Unlike the host selection stroke, this optional HUD overlay hides in menus.
pub(super) fn visible(playing: bool, hide_gui: bool, menu_open: bool, may_build: bool, has_player: bool) -> bool {
    playing && !hide_gui && !menu_open && may_build && has_player
}

/// Reject malformed config values rather than passing NaNs into HUD vertices.
pub(super) fn bounded(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value.is_finite() { value.clamp(min, max) } else { fallback }
}

pub(super) fn colour(rgb: u32, opacity: f64) -> u32 {
    let alpha = (bounded(opacity, 0.0, 1.0, 0.0) * 255.0).round() as u32;
    (alpha << 24) | (rgb & 0x00FF_FFFF)
}

/// Outline coordinates are local to the block; keep f64 until projection.
pub(super) fn translated(shape: [f64; 6], base: [f64; 3]) -> [f64; 6] {
    std::array::from_fn(|i| base[i % 3] + shape[i])
}

pub(super) fn valid_box(b: [f64; 6]) -> bool {
    b.iter().all(|v| v.is_finite()) && (0..3).all(|i| b[i] < b[i + 3])
}

pub(super) const EDGES: [(usize, usize); 12] = [
    (0, 1), (2, 3), (4, 5), (6, 7),
    (0, 2), (1, 3), (4, 6), (5, 7),
    (0, 4), (1, 5), (2, 6), (3, 7),
];

pub(super) fn box_edge(b: [f64; 6], (start, end): (usize, usize)) -> [f64; 6] {
    std::array::from_fn(|i| {
        let axis = i % 3;
        let corner = if i < 3 { start } else { end };
        b[axis + if corner & (1 << axis) == 0 { 0 } else { 3 }]
    })
}

// Index bit 0 selects high x, bit 1 high y, bit 2 high z.
pub(super) const FACES: [([usize; 4], usize, bool); 6] = [
    ([0, 2, 6, 4], 0, false),
    ([1, 3, 7, 5], 0, true),
    ([0, 1, 5, 4], 1, false),
    ([2, 3, 7, 6], 1, true),
    ([0, 1, 3, 2], 2, false),
    ([4, 5, 7, 6], 2, true),
];
