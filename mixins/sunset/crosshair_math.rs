// Original, allocation-free crosshair geometry. Dimensions and rectangle edges
// are physical pixels; only the last conversion uses the HUD's GUI scale.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl Rect {
    pub(super) fn expanded(self, pixels: f32) -> Self {
        Self {
            x: self.x - pixels,
            y: self.y - pixels,
            width: self.width + 2.0 * pixels,
            height: self.height + 2.0 * pixels,
        }
    }

    pub(super) fn gui(self, scale: f32) -> [f32; 4] {
        [self.x / scale, self.y / scale, self.width / scale, self.height / scale]
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Style {
    pub(super) dot_only: bool,
    pub(super) center_dot: bool,
    pub(super) length: f64,
    pub(super) gap: f64,
    pub(super) thickness: f64,
    pub(super) dot_size: f64,
    pub(super) outline: f64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Geometry {
    rects: [Rect; 5],
    len: usize,
    pub(super) outline: f32,
}

impl Geometry {
    pub(super) fn rects(&self) -> &[Rect] {
        &self.rects[..self.len]
    }
}

fn pixels(value: f64, default: f64, min: f64, max: f64) -> f32 {
    let value = if value.is_finite() { value } else { default };
    value.round().clamp(min, max) as f32
}

// An odd-width mark cannot be centered exactly on an even-width viewport and
// still have integer edges. Choose the nearest crisp center, at most half a
// physical pixel towards the upper left. All arms share that center.
fn centered(size: u32, width: f32) -> f32 {
    ((size as f32 - width) * 0.5).floor()
}

pub(super) fn geometry(size: [u32; 2], scale: f32, style: Style) -> Option<Geometry> {
    // At larger sizes f32 cannot represent every physical-pixel edge. Real
    // render targets are well below this; fail closed instead of drawing NaNs.
    if size.into_iter().any(|s| s == 0 || s > (1 << 23)) || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let length = pixels(style.length, 6.0, 1.0, 32.0);
    let gap = pixels(style.gap, 3.0, 0.0, 32.0);
    let thickness = pixels(style.thickness, 1.0, 1.0, 8.0);
    let dot_size = pixels(style.dot_size, 2.0, 1.0, 8.0);
    let mut out = Geometry {
        rects: [Rect::default(); 5],
        len: 0,
        outline: pixels(style.outline, 1.0, 0.0, 4.0),
    };
    if !style.dot_only {
        let x = centered(size[0], thickness);
        let y = centered(size[1], thickness);
        out.rects[..4].copy_from_slice(&[
            Rect { x: x - gap - length, y, width: length, height: thickness },
            Rect { x: x + thickness + gap, y, width: length, height: thickness },
            Rect { x, y: y - gap - length, width: thickness, height: length },
            Rect { x, y: y + thickness + gap, width: thickness, height: length },
        ]);
        out.len = 4;
    }
    if style.dot_only || style.center_dot {
        out.rects[out.len] = Rect {
            x: centered(size[0], dot_size),
            y: centered(size[1], dot_size),
            width: dot_size,
            height: dot_size,
        };
        out.len += 1;
    }
    Some(out)
}
