// Where the HUD's watermark and module list go: one of the screen's corners (stacked there with whatever else the
// corner holds, as before), or Custom, a place of their own kept as fractions of the monitor-sized screen (REFERENCE).
// The right-aligned module list keeps its right edge at its fraction (its rows grow to the left as modules turn on),
// everything else its left edge. A smaller window keeps each element as far from its nearest edge or the middle as on
// the full screen, and an element stays whole on the screen however the window is resized. The same places serve
// every Custom HUD element (Compass, Armor HUD, Radar, Scoreboard, the info lines, the layout editor's others). The debug screen (F3) hides an element in the corner it takes,
// a Custom one by the corner it is nearest; F1 hides them all with the rest of the HUD (the game's own extras layer).
//
// The settings, saved in the config and in profiles by name: "<element> position" (the choices in POSITIONS) and
// "<element> x" / "<element> y" (0 to 1, by 0.001; the layout editor with the chat open writes them when one is
// dragged).
use horizon_client::extras::Corner;
use std::sync::atomic::{AtomicU32, Ordering};

// The choices of a "position" setting: the corner first, so a config from before keeps its place.
pub(super) const POSITIONS: &[&str] = &["Top left", "Top right", "Bottom left", "Bottom right", "Custom"];

// The module list's: Top and Bottom of its side (Module list side), or Custom.
pub(super) const LIST_POSITIONS: &[&str] = &["Top", "Bottom", "Custom"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Place {
    Corner(Corner),
    Custom,
}

// A position setting's choice: a corner by its label, Custom, else (an unknown value) `default`.
pub(super) fn parse(choice: &str, default: Corner) -> Place {
    if choice == "Custom" {
        return Place::Custom;
    }
    Place::Corner(Corner::ALL.into_iter().find(|c| c.label() == choice).unwrap_or(default))
}

// The module list's corner: its side's, at the top or the bottom.
pub(super) fn list_place(position: &str, right: bool) -> Place {
    match (position, right) {
        ("Custom", _) => Place::Custom,
        ("Bottom", false) => Place::Corner(Corner::BottomLeft),
        ("Bottom", true) => Place::Corner(Corner::BottomRight),
        (_, false) => Place::Corner(Corner::TopLeft),
        (_, true) => Place::Corner(Corner::TopRight),
    }
}

fn fraction(v: f64) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) as f32 } else { 0.0 }
}

// The screen a Custom place's fractions are of: the monitor's, in the HUD's units (0 until the HUD first draws, or
// with no monitor known). On it a Custom element is exactly where it was dropped. A window smaller (or bigger) than
// that keeps each element as far from the edge or the middle it is nearest as there, rather than at the same
// fraction: elements placed together (a watermark with the FPS under it) stay together instead of running into
// each other or spreading apart, and a window going back to full screen puts everything back where it was.
static REFERENCE: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];

// The monitor the window is on, `monitor` pixels, at `scale` pixels a HUD unit: each frame, before the HUD draws.
pub(super) fn set_reference(monitor: Option<[f32; 2]>, scale: f32) {
    let [w, h] = monitor.filter(|_| scale > 0.0).map_or([0.0, 0.0], |[w, h]| [w / scale, h / scale]);
    REFERENCE[0].store(w.to_bits(), Ordering::Relaxed);
    REFERENCE[1].store(h.to_bits(), Ordering::Relaxed);
}

// The reference screen for `screen`: the monitor's, never smaller than the window (else the window itself).
fn reference(screen: [f32; 2]) -> [f32; 2] {
    let r = [f32::from_bits(REFERENCE[0].load(Ordering::Relaxed)), f32::from_bits(REFERENCE[1].load(Ordering::Relaxed))];
    [0, 1].map(|i| if r[i].is_finite() && r[i] > 0.0 { r[i].max(screen[i]) } else { screen[i] })
}

// Which third of `long` the middle of an element `size` long at `p` is in: 0 the start, 1 the middle, 2 the end.
fn third(p: f32, size: f32, long: f32) -> usize {
    let middle = (p + size / 2.0) / long.max(1.0);
    if middle < 1.0 / 3.0 { 0 } else if middle > 2.0 / 3.0 { 2 } else { 1 }
}

// An element `size` long at `p` on the reference (`from` long), on a screen `to` long: as far from its third's edge
// (or from the middle) as there.
fn anchored(p: f32, size: f32, from: f32, to: f32) -> f32 {
    match third(p, size, from) {
        0 => p,
        2 => to - (from - p),
        _ => p + (to - from) / 2.0,
    }
}

// The inverse of `anchored`: where on the reference an element at `q` on the screen came from. Its own third on the
// screen first; where thirds meet two places can map to `q`, and the one whose third agrees is taken.
fn unanchored(q: f32, size: f32, from: f32, to: f32) -> f32 {
    let came = [q, q - (to - from) / 2.0, q + (from - to)];
    let mine = third(q, size, to);
    [mine, 0, 1, 2].into_iter().map(|t| (t, came[t])).find(|&(t, p)| third(p, size, from) == t).map_or(came[mine], |(_, p)| p)
}

// The top left corner of a Custom element `size` big, at `at` (fractions of the reference screen: its left edge, or
// with `right` its right edge, and its top), on a screen `screen` big: whole on the screen whatever its size.
pub(super) fn custom(at: [f64; 2], size: [f32; 2], screen: [f32; 2], right: bool) -> [f32; 2] {
    custom_on(at, size, screen, right, reference(screen))
}

fn custom_on(at: [f64; 2], size: [f32; 2], screen: [f32; 2], right: bool, from: [f32; 2]) -> [f32; 2] {
    let edge = fraction(at[0]) * from[0];
    let x = anchored(if right { edge - size[0] } else { edge }, size[0], from[0], screen[0]);
    let y = anchored(fraction(at[1]) * from[1], size[1], from[1], screen[1]);
    [x.clamp(0.0, (screen[0] - size[0]).max(0.0)), y.clamp(0.0, (screen[1] - size[1]).max(0.0))]
}

// What `custom` keeps for an element whose top left corner is at `top_left`: the inverse, after keeping it whole
// on the screen.
pub(super) fn stored(top_left: [f32; 2], size: [f32; 2], screen: [f32; 2], right: bool) -> (f64, f64) {
    stored_on(top_left, size, screen, right, reference(screen))
}

fn stored_on(top_left: [f32; 2], size: [f32; 2], screen: [f32; 2], right: bool, from: [f32; 2]) -> (f64, f64) {
    let x = unanchored(top_left[0].clamp(0.0, (screen[0] - size[0]).max(0.0)), size[0], from[0], screen[0]);
    let y = unanchored(top_left[1].clamp(0.0, (screen[1] - size[1]).max(0.0)), size[1], from[1], screen[1]);
    let edge = if right { (x + size[0]).min(from[0]) } else { x };
    (f64::from((edge / from[0].max(1.0)).clamp(0.0, 1.0)), f64::from((y / from[1].max(1.0)).clamp(0.0, 1.0)))
}

// The screen corner nearest the middle of `rect` (x, y, width, height).
pub(super) fn nearest(rect: [f32; 4], screen: [f32; 2]) -> Corner {
    let left = rect[0] + rect[2] / 2.0 < screen[0] / 2.0;
    let top = rect[1] + rect[3] / 2.0 < screen[1] / 2.0;
    match (top, left) {
        (true, true) => Corner::TopLeft,
        (true, false) => Corner::TopRight,
        (false, true) => Corner::BottomLeft,
        (false, false) => Corner::BottomRight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: [f32; 2] = [640.0, 360.0];

    fn close(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3, "{a:?} != {b:?}");
    }

    #[test]
    fn positions_parse_by_label_and_old_values_keep_the_corner() {
        assert_eq!(parse("Top left", Corner::TopLeft), Place::Corner(Corner::TopLeft));
        assert_eq!(parse("Bottom right", Corner::TopLeft), Place::Corner(Corner::BottomRight));
        assert_eq!(parse("Custom", Corner::TopLeft), Place::Custom);
        assert_eq!(parse("nonsense", Corner::TopRight), Place::Corner(Corner::TopRight));
        // Every choice offered is understood.
        assert!(POSITIONS.iter().all(|p| *p == "Custom" || parse(p, Corner::TopLeft) != Place::Custom));
        assert_eq!(list_place("Top", true), Place::Corner(Corner::TopRight));
        assert_eq!(list_place("Bottom", false), Place::Corner(Corner::BottomLeft));
        assert_eq!(list_place("Custom", true), Place::Custom);
    }

    #[test]
    fn a_custom_element_stays_whole_on_any_screen() {
        // In the middle, left-anchored and right-anchored.
        assert_eq!(custom([0.5, 0.5], [100.0, 20.0], SCREEN, false), [320.0, 180.0]);
        assert_eq!(custom([0.5, 0.5], [100.0, 20.0], SCREEN, true), [220.0, 180.0]);
        // Past the edges, or out-of-range values from a hand-edited config: pulled back on.
        assert_eq!(custom([1.0, 1.0], [100.0, 20.0], SCREEN, false), [540.0, 340.0]);
        assert_eq!(custom([0.0, 0.0], [100.0, 20.0], SCREEN, true), [0.0, 0.0]);
        assert_eq!(custom([f64::NAN, 7.0], [100.0, 20.0], SCREEN, false), [0.0, 340.0]);
        // Wider than the screen: at its left edge rather than off it.
        assert_eq!(custom([0.9, 0.0], [800.0, 20.0], SCREEN, true), [0.0, 0.0]);
    }

    #[test]
    fn a_resized_window_keeps_the_anchored_edge_in_proportion() {
        // A right-aligned list with its right edge at the screen's: still at the edge at another size.
        let at = stored([540.0, 10.0], [100.0, 40.0], SCREEN, true);
        assert_eq!(at.0, 1.0);
        close(custom([at.0, at.1], [100.0, 40.0], [1280.0, 720.0], true), [1180.0, 20.0]);
        // And when it gets wider (more modules on), it grows to the left.
        close(custom([at.0, at.1], [160.0, 40.0], SCREEN, true), [480.0, 10.0]);
        // A left-anchored element a third of the way across stays a third of the way across.
        let at = stored([200.0, 120.0], [50.0, 12.0], [600.0, 360.0], false);
        close(custom([at.0, at.1], [50.0, 12.0], [1200.0, 720.0], false), [400.0, 240.0]);
    }

    // F11 out of full screen and back: an element by an edge stays that far from it, two stacked ones stay apart, the
    // middle stays the middle, and full screen again is exactly as before.
    #[test]
    fn a_smaller_window_keeps_elements_by_their_edges() {
        let full = [960.0, 540.0];
        let small = [640.0, 360.0];
        // A watermark (24 high) at the top left and the FPS line under it.
        let mark = custom_on([0.0026, 0.0074], [90.0, 24.0], small, false, full);
        let fps = custom_on([0.0026, 0.0565], [40.0, 12.0], small, false, full);
        assert!(fps[1] >= mark[1] + 24.0, "{mark:?} {fps:?}");
        // The module list on the right edge, a scoreboard 20 from the right, a compass in the middle.
        close(custom_on([1.0, 0.0], [100.0, 80.0], small, true, full), [540.0, 0.0]);
        let board = custom_on([f64::from(840.0f32 / 960.0), 0.5], [100.0, 60.0], small, false, full);
        assert!((small[0] - (board[0] + 100.0) - 20.0).abs() < 1e-3, "{board:?}");
        close(custom_on([f64::from(390.0f32 / 960.0), 0.0], [180.0, 26.0], small, false, full), [230.0, 0.0]);
        // Full screen: the plain fractions, as before.
        close(custom_on([0.25, 0.5], [50.0, 10.0], full, false, full), [240.0, 270.0]);
        // Dropped anywhere on the small window, it comes back there (and on the full screen where it maps).
        for at in [[0.0, 0.0], [150.0, 100.0], [300.0, 170.0], [520.0, 300.0], [590.0, 350.0]] {
            let (fx, fy) = stored_on(at, [50.0, 10.0], small, false, full);
            close(custom_on([fx, fy], [50.0, 10.0], small, false, full), at);
        }
    }

    #[test]
    fn stored_positions_come_back_where_they_were_dropped() {
        for right in [false, true] {
            for at in [[0.0, 0.0], [123.0, 45.0], [500.0, 300.0], [540.0, 340.0]] {
                let (fx, fy) = stored(at, [100.0, 20.0], SCREEN, right);
                let back = custom([fx, fy], [100.0, 20.0], SCREEN, right);
                assert!((back[0] - at[0]).abs() < 1e-3 && (back[1] - at[1]).abs() < 1e-3, "{at:?} {right}: {back:?}");
            }
        }
        // Dropped half off the screen: kept as the place it is drawn at.
        assert_eq!(stored([-50.0, 400.0], [100.0, 20.0], SCREEN, false), (0.0, f64::from(340.0f32 / 360.0)));
    }

    #[test]
    fn f3_hides_a_custom_element_by_its_nearest_corner() {
        assert_eq!(nearest([0.0, 0.0, 50.0, 10.0], SCREEN), Corner::TopLeft);
        assert_eq!(nearest([600.0, 5.0, 40.0, 10.0], SCREEN), Corner::TopRight);
        assert_eq!(nearest([10.0, 300.0, 40.0, 10.0], SCREEN), Corner::BottomLeft);
        assert_eq!(nearest([400.0, 200.0, 40.0, 10.0], SCREEN), Corner::BottomRight);
    }
}
