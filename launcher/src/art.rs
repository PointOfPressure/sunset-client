//! The launcher's two painted pictures, rasterised on the CPU at the window's device scale and cached as images: the
//! Overview banner's setting sun (its smooth glow, gradients and reflection would band or step as GPUI's flat paths
//! and two-stop gradients) and the Sunset logo. Both are drawn analytically, edge coverage from exact distances, so
//! every edge is anti-aliased and every horizontal edge sits on a pixel row. A picture is redrawn only when its pixel
//! size or the scale changes; every other frame repaints the cached sprite.

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

/// A capsule's signed distance: the segment a..b widened by `r`.
fn capsule(x: f32, y: f32, a: (f32, f32), b: (f32, f32), r: f32) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let f = clamp01(((x - a.0) * vx + (y - a.1) * vy) / (vx * vx + vy * vy).max(1e-6));
    (x - a.0 - f * vx).hypot(y - a.1 - f * vy) - r
}

// ---- The logo -------------------------------------------------------------------------------------------------------

/// One detail level of the logo, on its 200 x 100 unit box (y down, the horizon bar along the bottom): the bar's
/// thickness and inset from the box's sides, the gap between the sun's base and the bar, the sun's radius, the bands
/// cut through it (heights above its base, bottom up), the rays' angles (degrees; 270 is straight up), inner and outer
/// radius and width. The same table is assets/make-icons.py's DETAILS, which draws the SVG and the icons.
struct Detail {
    bar: f32,
    inset: f32,
    lift: f32,
    r: f32,
    gaps: &'static [(f32, f32)],
    rays: &'static [f32],
    ray: (f32, f32),
    ray_w: f32,
}

const LARGE: Detail = Detail { bar: 8., inset: 0., lift: 6., r: 54., gaps: &[(6., 12.5), (20., 25.5), (34., 38.)], rays: &[200., 235., 270., 305., 340.], ray: (66., 80.), ray_w: 11. };
const MEDIUM: Detail = Detail { bar: 11., inset: 0., lift: 7., r: 50., gaps: &[(7., 16.), (26., 32.)], rays: &[200., 235., 270., 305., 340.], ray: (62., 74.), ray_w: 14. };
const COMPACT: Detail = Detail { bar: 11., inset: 16., lift: 8., r: 50., gaps: &[(8., 17.), (27., 34.)], rays: &[220., 270., 320.], ray: (63., 76.), ray_w: 15. };
const SMALL: Detail = Detail { bar: 13., inset: 22., lift: 11., r: 50., gaps: &[(11., 24.)], rays: &[225., 270., 315.], ray: (64., 76.), ray_w: 16. };

/// The level for a logo `w` device pixels wide: fewer, bolder bands and rays as it shrinks.
fn detail(w: f32) -> &'static Detail {
    if w >= 64. {
        &LARGE
    } else if w >= 40. {
        &MEDIUM
    } else if w >= 26. {
        &COMPACT
    } else {
        &SMALL
    }
}

/// The logo's coverage at pixel centre (x, y), drawn `k` device pixels per unit with its box's top left at (ox, oy).
struct Logo {
    d: &'static Detail,
    k: f32,
    ox: f32,
    cx: f32,
    cy: f32,
    bar: (f32, f32),
    // Rows of the sun's slices, top and bottom, top first; the cap's top is open.
    slices: Vec<(f32, f32)>,
    rays: Vec<((f32, f32), (f32, f32))>,
}

impl Logo {
    fn new(k: f32, ox: f32, oy: f32, d: &'static Detail) -> Self {
        let snap = |v: f32| (oy + k * v).round();
        let bar_b = snap(100.);
        let bar_t = snap(100. - d.bar).min(bar_b - 1.);
        let base = snap(100. - d.bar - d.lift).min(bar_t - 1.);
        // Slice edges from the base up, each at least a row above the last.
        let mut edges = vec![base];
        for &(lo, hi) in d.gaps {
            let e = snap(100. - d.bar - d.lift - lo).min(edges[edges.len() - 1] - 1.);
            edges.push(e);
            edges.push(snap(100. - d.bar - d.lift - hi).min(e - 1.));
        }
        edges.push(f32::NEG_INFINITY);
        let slices = edges.chunks(2).map(|p| (p[1], p[0])).collect();
        let (cx, cy) = (ox + 100. * k, base);
        let rays = d
            .rays
            .iter()
            .map(|deg| {
                let (s, c) = deg.to_radians().sin_cos();
                ((cx + d.ray.0 * k * c, cy + d.ray.0 * k * s), (cx + d.ray.1 * k * c, cy + d.ray.1 * k * s))
            })
            .collect();
        Logo { d, k, ox, cx, cy, bar: (bar_t, bar_b), slices, rays }
    }

    fn cover(&self, x: f32, y: f32) -> f32 {
        let disc = (x - self.cx).hypot(y - self.cy) - self.d.r * self.k;
        let mut sdf = f32::INFINITY;
        for &(top, bot) in &self.slices {
            sdf = sdf.min(disc.max(top - y).max(y - bot));
        }
        for &(a, b) in &self.rays {
            sdf = sdf.min(capsule(x, y, a, b, self.d.ray_w * self.k / 2.));
        }
        let t = (self.bar.1 - self.bar.0) / 2.;
        let yc = (self.bar.0 + self.bar.1) / 2.;
        let (l, r) = (self.ox + self.d.inset * self.k + t, self.ox + (200. - self.d.inset) * self.k - t);
        sdf = sdf.min(capsule(x, y, (l, yc), (r, yc), t));
        clamp01(0.5 - sdf)
    }
}

/// The logo in `color` on a transparent `w` x `h` picture (twice as wide as tall), its bar along the bottom.
fn logo_pixels(w: u32, h: u32, color: u32) -> Vec<u8> {
    let k = w as f32 / 200.;
    let logo = Logo::new(k, 0., h as f32 - 100. * k, detail(w as f32));
    let (r, g, b) = ((color >> 16) as u8, (color >> 8) as u8, color as u8);
    rows(w, h, |y, line| {
        for (x, px) in line.chunks_exact_mut(4).enumerate() {
            let a = logo.cover(x as f32 + 0.5, y as f32 + 0.5);
            px.copy_from_slice(&[b, g, r, (a * 255.).round() as u8]);
        }
    })
}

/// Paints the Sunset logo over `b` (best twice as wide as tall), in `color`.
pub fn paint_logo(window: &mut Window, b: Bounds<Pixels>, color: u32) {
    paint(window, "logo", b, 0., u64::from(color), move |w, h, _| logo_pixels(w, h, color));
}

// ---- The banner's sun -----------------------------------------------------------------------------------------------

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

/// The banner's colours (sRGB) and the sun's band rhythm.
pub struct Palette {
    pub card: u32,
    pub ember: u32,
    pub strip: u32,
}

const SUN_TOP: u32 = 0xfa8a2e; // the sun's hot top
const SUN_MID: u32 = 0xf26a12; // Sunset orange's hover shade, at 60 % of its height
const SUN_FOOT: u32 = 0xcf4800; // deep orange at the horizon
const GLOW: u32 = 0xdb5000;
/// The bands cut across the lower sun (bottom and top edge, in sun radii above the horizon): a steady 0.13 pitch,
/// the gaps widening and the slices of sun between them thinning towards the horizon.
const BANDS: [(f32, f32); 4] = [(0.052, 0.130), (0.198, 0.260), (0.344, 0.390), (0.490, 0.520)];
/// The reflection's vertical squash, and how many sun radii its brightness takes to fall by e.
const MIRROR: f32 = 0.42;

/// The sun's geometry in a `w` x `h` device-pixel banner whose strip under the horizon is `strip` pixels tall.
struct Sun {
    horizon: f32,
    r: f32,
    cx: f32,
    /// How far (in radii outside the disc) the glow reaches before it has faded to nothing.
    reach: f32,
    bands: Vec<(f32, f32)>,
}

impl Sun {
    fn new(w: f32, h: f32, strip: f32, s: f32) -> Self {
        // The horizon runs through the gap between the banner's text and its button row, at any height.
        let horizon = (h - strip).round().max(1.);
        let r = (horizon * 0.58).min(w * 0.17).min(200. * s).max(1.);
        // Right of centre, with the glow clear of the card's right edge.
        let cx = (w * 0.76).min(w - r * 1.6 - 24. * s);
        // The glow fades out before the card's top and right edges.
        let reach = ((horizon - 2. * s) / r - 1.).min((w - 2. * s - cx) / r - 1.).clamp(0.2, 1.6);
        let mut bands = Vec::new();
        let mut floor = horizon;
        for (lo, hi) in BANDS {
            let bot = (horizon - lo * r).round().min(floor - 1.);
            let top = (horizon - hi * r).round().min(bot - 1.);
            bands.push((top, bot));
            floor = top;
        }
        Sun { horizon, r, cx, reach, bands }
    }

    /// The glow's strength (linear) `t` radii outside the disc: three soft falloffs, the whole faded smoothly to
    /// nothing at `reach`, so there is no ring or edge anywhere in it.
    fn glow(&self, t: f32) -> f32 {
        if t >= self.reach {
            return 0.;
        }
        let fade = 1. - (t / self.reach).powi(2);
        (0.11 * (-t / 0.10).exp() + 0.06 * (-t / 0.32).exp() + 0.025 * (-t / 0.9).exp()) * fade * fade
    }

    /// The disc's colour (linear) `v` radii above the horizon: hot at the top, deep at the foot.
    fn disc(v: f32, top: [f32; 3], mid: [f32; 3], foot: [f32; 3]) -> [f32; 3] {
        if v > 0.6 { mix3(mid, top, smoothstep(0.6, 1., v)) } else { mix3(foot, mid, smoothstep(0., 0.6, v)) }
    }

    /// How much of the pixel row `y..y + 1` lies in a band (sky), 0..1, for rows above the horizon.
    fn in_band(&self, y: f32) -> f32 {
        self.bands.iter().map(|&(top, bot)| ((y + 1.).min(bot) - y.max(top)).max(0.)).sum::<f32>().min(1.)
    }
}

/// The banner's picture: sky, the striped sun with its glow, the horizon line, and the strip under it with the sun's
/// reflection. Opaque; the caller rounds its corners and draws the card's border over it.
fn hero_pixels(w: u32, h: u32, s: f32, strip: f32, pal: &Palette) -> Vec<u8> {
    let s = s.max(0.5);
    let sun = Sun::new(w as f32, h as f32, strip * s, s);
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
                    let d = dx.hypot(hz - yc);
                    // The glow, everywhere above the horizon (in the bands too: sky lit by the sun behind it).
                    let g = sun.glow(((d - r) / r).max(0.)) * if d < r { 0.75 } else { 1. };
                    c = [c[0] + glow[0] * g, c[1] + glow[1] * g, c[2] + glow[2] * g];
                    let cov = clamp01(r - d + 0.5) * (1. - sun.in_band(y));
                    if cov > 0. {
                        c = mix3(c, Sun::disc((hz - yc) / r, top, mid, foot), cov);
                    }
                } else {
                    let dy = yc - hz;
                    // Under the horizon the glow is a low haze hugging the horizon under the sun: brightest at the line,
                    // falling off two and a half times faster downwards than sideways.
                    let g = sun.glow((dx.abs() - r).max(0.).hypot(dy * 2.5) / r) * 0.5;
                    c = [c[0] + glow[0] * g, c[1] + glow[1] * g, c[2] + glow[2] * g];
                    // The reflection: the sun mirrored and squashed, its edges softening and its light fading with depth.
                    let below = dy - line_h;
                    let v = below / MIRROR;
                    if below > 0. && v < r {
                        let half = (r * r - v * v).sqrt();
                        let soft = 1. + below * 0.12;
                        let mut cov = clamp01((half - dx.abs()) / soft + 0.5);
                        // The bands, mirrored: rows of the reflection whose source lies in a band are dark.
                        let band = sun.bands.iter().map(|&(t, b)| {
                            let (t, b) = (hz + line_h + (hz - b) * MIRROR, hz + line_h + (hz - t) * MIRROR);
                            ((y + 1.).min(b) - y.max(t)).max(0.)
                        });
                        cov *= 1. - band.sum::<f32>().min(1.);
                        let fade = 0.12 * (-below / (0.08 * r)).exp() * (1. - smoothstep(0.5, 1., v / r));
                        // Broken into ripples, longer apart the nearer they come (in device pixels, so 2x matches 1x).
                        let ripple = (below / s).sqrt() * 2.2;
                        cov *= smoothstep(0.15, 0.55, (ripple * std::f32::consts::TAU).sin() * 0.5 + 0.5);
                        c = mix3(c, Sun::disc(v / r, top, mid, foot), cov * fade);
                    }
                }
            }
            // The horizon line: a faint rule across the card, bright and hot under the sun.
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
    fn logo_levels_fit_their_box() {
        // Every level's upright ray stays inside the 200 x 100 box at the sizes that use it (the tile icons centre
        // the small ones on their own), and the sun clears its rays.
        for d in [&LARGE, &MEDIUM] {
            assert!(100. - d.bar - d.lift - d.ray.1 - d.ray_w / 2. >= 0.);
        }
        for d in [&LARGE, &MEDIUM, &COMPACT, &SMALL] {
            assert!(d.ray.0 - d.ray_w / 2. - d.r >= 4.);
        }
    }

    #[test]
    fn logo_is_crisp_and_banded() {
        // At 64 px: a solid sun crown, sky in the first band, full rows in the bar, nothing above the box's top row.
        let (w, h) = (64, 32);
        let px = logo_pixels(w, h, 0xdb5000);
        let a = |x: u32, y: u32| px[((y * w + x) * 4 + 3) as usize];
        assert_eq!(a(32, h - 1), 255);
        assert!((0..w).all(|x| a(x, 0) == 0 || x.abs_diff(32) <= 2));
        let logo = Logo::new(w as f32 / 200., 0., 0., &LARGE);
        let (top, bot) = logo.slices[1];
        assert_eq!(a(32, bot as u32), 0, "the lowest band shows through at row {bot}");
        assert_eq!(a(32, top as u32), 255);
        assert!(px.chunks(4).all(|p| p[0] == 0x00 && p[1] == 0x50 && p[2] == 0xdb));
    }

    #[test]
    fn hero_sun_is_smooth_and_contained() {
        let pal = Palette { card: 0x0d0d0f, ember: 0x221003, strip: 0x080706 };
        for (w, h, s) in [(1100u32, 420u32, 1f32), (2560, 1000, 2.), (700, 250, 1.), (1500, 600, 1.5)] {
            let px = hero_pixels(w, h, s, 88., &pal);
            let at = |x: u32, y: u32| {
                let i = ((y * w + x) * 4) as usize;
                [px[i + 2] as i32, px[i + 1] as i32, px[i] as i32]
            };
            let sun = Sun::new(w as f32, h as f32, 88. * s, s);
            // The glow has faded to the sky's own colour at the card's top and right edges.
            let sky = at(4, 0);
            for x in (0..w).step_by(7) {
                let p = at(x, 0);
                assert!((0..3).all(|i| (p[i] - sky[i]).abs() <= 1), "{w}x{h}: glow at the top edge, x {x}: {p:?} vs {sky:?}");
            }
            let edge = at(w - 1, (sun.horizon - 2.) as u32);
            let far = at(2, (sun.horizon - 2.) as u32);
            assert!((0..3).all(|i| (edge[i] - far[i]).abs() <= 2), "{w}x{h}: glow at the right edge {edge:?} vs {far:?}");
            // Straight up out of the sun the glow only ever dims (give or take the sub-step nudge), with no jump.
            let (cx, hz) = (sun.cx as u32, sun.horizon as u32);
            let mut prev = at(cx, hz - (sun.r as u32) - 2);
            for y in (1..hz - (sun.r as u32) - 2).rev() {
                let p = at(cx, y);
                assert!((0..3).all(|i| p[i] <= prev[i] + 1 && prev[i] - p[i] <= 6), "{w}x{h}: step at y {y}: {p:?} after {prev:?}");
                prev = p;
            }
            // The sun's top is hotter than its foot, and the bands are sky.
            let (tp, ft) = (at(cx, hz - sun.r as u32 + 3), at(cx, hz - 2));
            assert!(tp[1] > ft[1] + 30, "{tp:?} {ft:?}");
            let (bt, bb) = sun.bands[0];
            let gap = at(cx, ((bt + bb) / 2.) as u32);
            assert!(gap[0] < ft[0] - 60, "band {gap:?} vs sun {ft:?}");
            // The strip at its bottom row is its own colour: the reflection has faded out.
            let bottom = at(cx, h - 1);
            assert!((0..3).all(|i| (bottom[i] - [8, 7, 6][i]).abs() <= 2), "{bottom:?}");
        }
    }
}
