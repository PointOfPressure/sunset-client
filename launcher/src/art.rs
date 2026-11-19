//! The Overview banner's picture, rasterised on the CPU at the window's device scale and cached as an image: the
//! Sunset mark setting on the horizon (its smooth glow, gradients and reflection would band or step as GPUI's flat
//! paths and two-stop gradients). It is drawn analytically, edge coverage from exact distances, so every edge is
//! anti-aliased and the horizon sits on a pixel row. The picture is redrawn only when its pixel size or the scale
//! changes; every other frame repaints the cached sprite.

use gpui::{Bounds, Corners, Pixels, RenderImage, Window, point, px, size};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    // The last picture drawn per slot, with the key it was drawn for.
    static CACHE: RefCell<Vec<(&'static str, u64, Arc<RenderImage>)>> = const { RefCell::new(Vec::new()) };
}

/// Paints the picture `draw` makes (BGRA, the given device-pixel width and height, then the scale) over `b` snapped
/// to whole device pixels, with `radius` rounded corners. `key` names everything else the picture depends on.
fn paint(window: &mut Window, slot: &'static str, b: Bounds<Pixels>, radius: f32, key: u64, draw: impl FnOnce(u32, u32, f32) -> Vec<u8>) {
    let s = window.scale_factor();
    let (x0, y0) = ((f32::from(b.origin.x) * s).round(), (f32::from(b.origin.y) * s).round());
    let (x1, y1) = ((f32::from(b.origin.x + b.size.width) * s).round(), (f32::from(b.origin.y + b.size.height) * s).round());
    let (w, h) = ((x1 - x0).max(0.) as u32, (y1 - y0).max(0.) as u32);
    if w == 0 || h == 0 {
        return;
    }
    let key = key ^ (u64::from(w) << 40) ^ (u64::from(h) << 20) ^ u64::from(s.to_bits()).rotate_left(7);
    let image = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(e) = c.iter().find(|e| e.0 == slot && e.1 == key) {
            return Some(e.2.clone());
        }
        let buf = image::ImageBuffer::from_raw(w, h, draw(w, h, s))?;
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(buf)]));
        if let Some(i) = c.iter().position(|e| e.0 == slot) {
            let (_, _, old) = c.swap_remove(i);
            // Frees its atlas space; a failure only keeps the old sprite around.
            window.drop_image(old).ok();
        }
        c.push((slot, key, image.clone()));
        Some(image)
    });
    if let Some(image) = image {
        let at = Bounds::new(point(px(x0 / s), px(y0 / s)), size(px(w as f32 / s), px(h as f32 / s)));
        window.paint_image(at, at, Corners::all(px(radius)), image, 0, false).ok();
    }
}

/// Runs `row(y, pixels)` over every row of a `w` x `h` BGRA buffer, the rows shared out over the CPU's threads.
fn rows(w: u32, h: u32, row: impl Fn(u32, &mut [u8]) + Sync) -> Vec<u8> {
    let stride = w as usize * 4;
    let mut buf = vec![0u8; stride * h as usize];
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 16);
    let per = (h as usize).div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (i, chunk) in buf.chunks_mut(stride * per).enumerate() {
            let row = &row;
            scope.spawn(move || {
                for (j, line) in chunk.chunks_mut(stride).enumerate() {
                    row((i * per + j) as u32, line);
                }
            });
        }
    });
    buf
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0., 1.)
}

fn smoothstep(e0: f32, e1: f32, v: f32) -> f32 {
    let t = clamp01((v - e0) / (e1 - e0));
    t * t * (3. - 2. * t)
}

// ---- The banner's mark ----------------------------------------------------------------------------------------------

/// sRGB (0..1) to linear light.
fn lin(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn lin3(c: u32) -> [f32; 3] {
    [lin((c >> 16 & 0xff) as f32 / 255.), lin((c >> 8 & 0xff) as f32 / 255.), lin((c & 0xff) as f32 / 255.)]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Linear light back to sRGB through a table, so a picture costs no powf per pixel.
struct Encode(Vec<f32>);

impl Encode {
    const N: usize = 4096;

    fn new() -> Self {
        Encode(
            (0..=Self::N)
                .map(|i| {
                    let v = i as f32 / Self::N as f32;
                    255. * if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1. / 2.4) - 0.055 }
                })
                .collect(),
        )
    }

    /// The 0..255 sRGB value of linear `v`, before rounding.
    fn get(&self, v: f32) -> f32 {
        let f = clamp01(v) * Self::N as f32;
        let i = (f as usize).min(Self::N - 1);
        self.0[i] + (self.0[i + 1] - self.0[i]) * (f - i as f32)
    }
}

/// Interleaved gradient noise in -0.5..0.5: a sub-step nudge before rounding, so the long, shallow glow and sky
/// gradients come out smooth instead of in visible 8-bit steps. It is under one step: no grain can be seen.
fn nudge(x: u32, y: u32) -> f32 {
    let v = 52.982_918 * (0.067_110_56 * x as f32 + 0.005_837_15 * y as f32).fract();
    v.fract() - 0.5
}

/// The banner's colours (sRGB).
pub struct Palette {
    pub card: u32,
    pub ember: u32,
    pub strip: u32,
}

const SUN_TOP: u32 = 0xfa8a2e; // the arcs' hot top
const SUN_MID: u32 = 0xf26a12; // Sunset orange's hover shade, at 60 % of their height
const SUN_FOOT: u32 = 0xcf4800; // deep orange at the horizon
const GLOW: u32 = 0xdb5000;
/// The mark's rings on the SVG's grid (launcher/assets/sunset-launcher.svg), in outer radii: the outer ring's inner
/// edge, the inner ring's outer and inner edges, and how far the inner ring's centre sits above the horizon (its ends
/// drop straight down to it).
const OUTER_IN: f32 = 73.233 / 94.72;
const INNER: (f32, f32) = (53.061 / 94.72, 31.573 / 94.72);
const INNER_LIFT: f32 = 2.193 / 94.72;
/// The reflection's vertical squash, and how many outer radii its brightness takes to fall by e.
const MIRROR: f32 = 0.42;

/// The mark's geometry in a `w` x `h` device-pixel banner whose strip under the horizon is `strip` pixels tall.
struct Mark {
    horizon: f32,
    /// The outer ring's outer radius.
    r: f32,
    cx: f32,
    /// How far (in radii outside the mark) the glow reaches before it has faded to nothing.
    reach: f32,
}

impl Mark {
    fn new(w: f32, h: f32, strip: f32, s: f32) -> Self {
        // The horizon runs through the gap between the banner's text and its button row, at any height.
        let horizon = (h - strip).round().max(1.);
        let r = (horizon * 0.58).min(w * 0.17).min(200. * s).max(1.);
        // Right of centre, with the glow clear of the card's right edge.
        let cx = (w * 0.76).min(w - r * 1.6 - 24. * s);
        // The glow fades out before the card's top and right edges.
        let reach = ((horizon - 2. * s) / r - 1.).min((w - 2. * s - cx) / r - 1.).clamp(0.2, 1.6);
        Mark { horizon, r, cx, reach }
    }

    /// The mark's signed distance (device pixels, negative inside) at `dx` from its centre and `up` above the
    /// horizon (`up` >= 0): the outer half ring on the horizon, and the inner one standing on short straight legs.
    fn sdf(&self, dx: f32, up: f32) -> f32 {
        let r = self.r;
        let ring = |d: f32, o: f32, i: f32| (d - (o + i) * r / 2.).abs() - (o - i) * r / 2.;
        let outer = ring(dx.hypot(up), 1., OUTER_IN);
        let lift = INNER_LIFT * r;
        let inner = ring(if up > lift { dx.hypot(up - lift) } else { dx.abs() }, INNER.0, INNER.1);
        outer.min(inner)
    }

    /// The glow's strength (linear) `t` radii outside the mark: three soft falloffs, the whole faded smoothly to
    /// nothing at `reach`, so there is no ring or edge anywhere in it.
    fn glow(&self, t: f32) -> f32 {
        if t >= self.reach {
            return 0.;
        }
        let fade = 1. - (t / self.reach).powi(2);
        (0.11 * (-t / 0.10).exp() + 0.06 * (-t / 0.32).exp() + 0.025 * (-t / 0.9).exp()) * fade * fade
    }

    /// The arcs' colour (linear) `v` radii above the horizon: hot at the top, deep at the foot.
    fn shade(v: f32, top: [f32; 3], mid: [f32; 3], foot: [f32; 3]) -> [f32; 3] {
        if v > 0.6 { mix3(mid, top, smoothstep(0.6, 1., v)) } else { mix3(foot, mid, smoothstep(0., 0.6, v)) }
    }
}

/// The banner's picture: sky, the Sunset mark with its glow, the horizon line, and the strip under it with the mark's
/// reflection. Opaque; the caller rounds its corners and draws the card's border over it.
fn hero_pixels(w: u32, h: u32, s: f32, strip: f32, pal: &Palette) -> Vec<u8> {
    let s = s.max(0.5);
    let sun = Mark::new(w as f32, h as f32, strip * s, s);
    let enc = Encode::new();
    let strip_c = lin3(pal.strip);
    let (top, mid, foot, glow) = (lin3(SUN_TOP), lin3(SUN_MID), lin3(SUN_FOOT), lin3(GLOW));
    let line_h = s.round().max(1.);
    let (hz, r, cx) = (sun.horizon, sun.r, sun.cx);
    let reach_px = (1. + sun.reach) * r;
    let sky_rgb = |y: f32| {
        // The card's sky, in sRGB steps like the gradient it replaces: card at the top to ember at the horizon.
        let t = clamp01(y / hz);
        let c = |sh: u32| ((pal.card >> sh & 0xff) as f32 + (((pal.ember >> sh) & 0xff) as f32 - (pal.card >> sh & 0xff) as f32) * t) / 255.;
        [lin(c(16)), lin(c(8)), lin(c(0))]
    };
    rows(w, h, |row, line| {
        let y = row as f32;
        let yc = y + 0.5;
        let above = yc < hz;
        let base = if above { sky_rgb(yc) } else { strip_c };
        for (col, out) in line.chunks_exact_mut(4).enumerate() {
            let x = col as f32 + 0.5;
            let dx = x - cx;
            let mut c = base;
            if dx.abs() < reach_px + r {
                if above {
                    let d = sun.sdf(dx, hz - yc);
                    // The glow radiates from the arcs, in the gaps between them too (softer inside the outer ring).
                    let g = sun.glow((d / r).max(0.)) * if dx.hypot(hz - yc) < r { 0.75 } else { 1. };
                    c = [c[0] + glow[0] * g, c[1] + glow[1] * g, c[2] + glow[2] * g];
                    let cov = clamp01(0.5 - d);
                    if cov > 0. {
                        c = mix3(c, Mark::shade((hz - yc) / r, top, mid, foot), cov);
                    }
                } else {
                    let dy = yc - hz;
                    // Under the horizon the glow is a low haze hugging the horizon under the mark: brightest at the line,
                    // falling off two and a half times faster downwards than sideways.
                    let g = sun.glow((dx.abs() - r).max(0.).hypot(dy * 2.5) / r) * 0.5;
                    c = [c[0] + glow[0] * g, c[1] + glow[1] * g, c[2] + glow[2] * g];
                    // The reflection: the mark mirrored and squashed, its edges softening and its light fading with depth.
                    let below = dy - line_h;
                    let v = below / MIRROR;
                    if below > 0. && v < r {
                        let soft = 1. + below * 0.12;
                        let mut cov = clamp01(0.5 - sun.sdf(dx, v) / soft);
                        let fade = 0.12 * (-below / (0.08 * r)).exp() * (1. - smoothstep(0.5, 1., v / r));
                        // Broken into ripples, longer apart the nearer they come (in device pixels, so 2x matches 1x).
                        let ripple = (below / s).sqrt() * 2.2;
                        cov *= smoothstep(0.15, 0.55, (ripple * std::f32::consts::TAU).sin() * 0.5 + 0.5);
                        c = mix3(c, Mark::shade(v / r, top, mid, foot), cov * fade);
                    }
                }
            }
            // The horizon line: a faint rule across the card, bright and hot under the mark.
            if y >= hz && y < hz + line_h {
                let peak = (-(dx / (1.1 * r)).powi(2)).exp();
                c = mix3(c, mix3(glow, top, peak * 0.5), 0.42 + 0.5 * peak);
            }
            let n = nudge(col as u32, row);
            let q = |v: f32| (enc.get(v) + n).round().clamp(0., 255.) as u8;
            out.copy_from_slice(&[q(c[2]), q(c[1]), q(c[0]), 255]);
        }
    })
}

/// Paints the Overview banner's picture over `b` with `radius` corners; `strip` is the height (logical pixels)
/// of the strip under its horizon.
pub fn paint_hero(window: &mut Window, b: Bounds<Pixels>, radius: f32, strip: f32, pal: Palette) {
    let key = u64::from(strip.to_bits()) ^ (u64::from(pal.card) << 8) ^ (u64::from(pal.ember) << 32) ^ u64::from(pal.strip).rotate_left(52);
    paint(window, "hero", b, radius, key, move |w, h, s| hero_pixels(w, h, s, strip, &pal));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_matches_the_svg() {
        // On the SVG's 256 grid scaled so the outer radius is 94.72: inside each ring and leg, outside in the gaps,
        // the hole and above the top.
        let m = Mark { horizon: 0., r: 94.72, cx: 0., reach: 1. };
        for (dx, up) in [(0., 84.), (-84., 1.), (59.4, 59.4), (0., 44.5), (-42.3, 1.), (42.3, 2.)] {
            assert!(m.sdf(dx, up) < -5., "({dx}, {up}) should be inside: {}", m.sdf(dx, up));
        }
        for (dx, up) in [(0., 64.), (-63., 1.), (0., 15.), (20., 1.), (0., 100.), (100., 1.)] {
            assert!(m.sdf(dx, up) > 5., "({dx}, {up}) should be outside: {}", m.sdf(dx, up));
        }
        // The edges are where the SVG puts them.
        for (dx, up) in [(0., 94.72), (0., 73.233), (-53.061, 1.), (0., 31.573 + 2.193), (31.573, 0.5)] {
            assert!(m.sdf(dx, up).abs() < 0.01, "edge ({dx}, {up}): {}", m.sdf(dx, up));
        }
    }

    #[test]
    fn hero_mark_is_smooth_and_contained() {
        let pal = Palette { card: 0x0d0d0f, ember: 0x221003, strip: 0x080706 };
        for (w, h, s) in [(1100u32, 420u32, 1f32), (2560, 1000, 2.), (700, 250, 1.), (1500, 600, 1.5)] {
            let px = hero_pixels(w, h, s, 88., &pal);
            let at = |x: u32, y: u32| {
                let i = ((y * w + x) * 4) as usize;
                [px[i + 2] as i32, px[i + 1] as i32, px[i] as i32]
            };
            let sun = Mark::new(w as f32, h as f32, 88. * s, s);
            // The glow has faded to the sky's own colour at the card's top and right edges.
            let sky = at(4, 0);
            for x in (0..w).step_by(7) {
                let p = at(x, 0);
                assert!((0..3).all(|i| (p[i] - sky[i]).abs() <= 1), "{w}x{h}: glow at the top edge, x {x}: {p:?} vs {sky:?}");
            }
            let edge = at(w - 1, (sun.horizon - 2.) as u32);
            let far = at(2, (sun.horizon - 2.) as u32);
            assert!((0..3).all(|i| (edge[i] - far[i]).abs() <= 2), "{w}x{h}: glow at the right edge {edge:?} vs {far:?}");
            // Straight up out of the mark the glow only ever dims (give or take the sub-step nudge), with no jump.
            let (cx, hz) = (sun.cx as u32, sun.horizon as u32);
            let mut prev = at(cx, hz - (sun.r as u32) - 2);
            for y in (1..hz - (sun.r as u32) - 2).rev() {
                let p = at(cx, y);
                assert!((0..3).all(|i| p[i] <= prev[i] + 1 && prev[i] - p[i] <= 6), "{w}x{h}: step at y {y}: {p:?} after {prev:?}");
                prev = p;
            }
            // The arcs' top is hotter than their foot; the gap between the rings and the hole inside are sky, the gap
            // lit a little by the glow.
            let u = sun.r / 94.72;
            let (tp, ft) = (at(cx, hz - sun.r as u32 + 3), at(cx + (84. * u) as u32, hz - 2));
            assert!(tp[1] > ft[1] + 30, "{tp:?} {ft:?}");
            let (gap, hole) = (at(cx, hz - (64. * u) as u32), at(cx, hz - (12. * u) as u32));
            for p in [gap, hole] {
                assert!(p[0] < ft[0] - 60, "sky {p:?} vs arc {ft:?}");
            }
            assert!(gap[0] > sky[0] + 10, "the gap has no glow: {gap:?} vs {sky:?}");
            // The strip at its bottom row is its own colour: the reflection has faded out.
            let bottom = at(cx, h - 1);
            assert!((0..3).all(|i| (bottom[i] - [8, 7, 6][i]).abs() <= 2), "{bottom:?}");
        }
    }
}
