// Radar geometry for Horizon.

use std::sync::OnceLock;

const CIRCLE_POINTS: usize = 256;
static CIRCLE: OnceLock<[[f32; 2]; CIRCLE_POINTS]> = OnceLock::new();

// Physical pixels, not GUI pixels: edges stay one device pixel wide even
// with a fractional desktop scale or a large Minecraft GUI scale.
pub(super) fn pixel_width(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale.recip()
    } else {
        1.0
    }
}

pub(super) fn circle_segments(radius: f32, scale: f32) -> usize {
    let physical_radius = f64::from(radius.max(0.0) / pixel_width(scale));
    // Bound the maximum chord error to a quarter of a device pixel for
    // supported window sizes, while keeping small markers inexpensive.
    [16, 32, 64, 128, 256]
        .into_iter()
        .find(|&segments| {
            physical_radius * (1.0 - (std::f64::consts::PI / segments as f64).cos()) <= 0.25
        })
        .unwrap_or(CIRCLE_POINTS)
}

pub(super) fn circle_point(center: [f32; 2], radius: f32, i: usize, segments: usize) -> [f32; 2] {
    let points = CIRCLE.get_or_init(|| {
        std::array::from_fn(|j| {
            let angle = std::f64::consts::TAU * j as f64 / CIRCLE_POINTS as f64;
            [angle.cos() as f32, angle.sin() as f32]
        })
    });
    let [x, y] = points[(i % segments) * (CIRCLE_POINTS / segments)];
    [center[0] + radius * x, center[1] + radius * y]
}

// North-up uses +X right, +Z down. Rotating puts the player's forward
// direction up (Minecraft yaw zero faces +Z).
pub(super) fn offset(dx: f64, dz: f64, yaw: f64, rotate: bool) -> [f64; 2] {
    if !rotate {
        return [dx, dz];
    }
    let (sin, cos) = yaw.to_radians().sin_cos();
    [-dx * cos - dz * sin, dx * sin - dz * cos]
}

pub(super) fn project(
    dx: f64,
    dz: f64,
    yaw: f64,
    rotate: bool,
    range: f64,
    radius: f32,
) -> Option<[f32; 2]> {
    if ![dx, dz, yaw, range, f64::from(radius)]
        .iter()
        .all(|n| n.is_finite())
        || range <= 0.0
        || radius <= 0.0
        || dx.hypot(dz) > range
    {
        return None;
    }
    let [x, y] = offset(dx, dz, yaw, rotate);
    Some([(x / range) as f32 * radius, (y / range) as f32 * radius])
}

pub(super) fn fitted_size(requested: f32, width: f32, height: f32) -> Option<f32> {
    // Four pixels of screen margin, twelve for the range label. Do not
    // rewrite the saved setting when temporarily fitting a small viewport.
    let size = requested.min(width - 4.0).min(height - 16.0).floor();
    (size.is_finite() && size >= 64.0).then_some(size)
}

// ---- Locator radar (behaviour after Combatant's Visual Range, written fresh): where the server's locator bar puts a
// player. Horizon and vanilla send every player's waypoint in the dimension: the exact block while their chunk is in
// your view, the chunk while it is not, a bearing only past 332 blocks; a carved pumpkin or a mob head hides them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Fix {
    Block([i32; 3]),
    Chunk([i32; 2]),
    // The yaw that faces them, in radians (yaw 0 looks to +Z, 90 degrees to -X).
    Bearing(f32),
}

impl Fix {
    // How close the server says they are: 1 a bearing only, 2 within 332 blocks, 3 in your view range.
    pub(super) fn rank(self) -> u8 {
        match self {
            Fix::Bearing(_) => 1,
            Fix::Chunk(_) => 2,
            Fix::Block(_) => 3,
        }
    }

    // From (x, z) to them; None for a bearing only.
    pub(super) fn offset_from(self, x: f64, z: f64) -> Option<[f64; 2]> {
        match self {
            Fix::Block([bx, _, bz]) => Some([f64::from(bx) + 0.5 - x, f64::from(bz) + 0.5 - z]),
            Fix::Chunk([cx, cz]) => Some([f64::from(cx) * 16.0 + 8.0 - x, f64::from(cz) * 16.0 + 8.0 - z]),
            Fix::Bearing(_) => None,
        }
    }

    // The unit direction to them from (x, z).
    pub(super) fn direction_from(self, x: f64, z: f64) -> Option<[f64; 2]> {
        let d = match self {
            Fix::Bearing(a) => {
                let a = f64::from(a);
                [-a.sin(), a.cos()]
            }
            _ => {
                let [dx, dz] = self.offset_from(x, z)?;
                let length = dx.hypot(dz);
                if length < 1e-6 {
                    return None;
                }
                [dx / length, dz / length]
            }
        };
        d.iter().all(|v| v.is_finite()).then_some(d)
    }
}

// A compass point for a direction (x east, z south).
pub(super) fn compass([dx, dz]: [f64; 2]) -> &'static str {
    const POINTS: [&str; 8] = ["S", "SW", "W", "NW", "N", "NE", "E", "SE"];
    let yaw = (-dx).atan2(dz).to_degrees().rem_euclid(360.0);
    POINTS[(((yaw + 22.5) / 45.0) as usize) % 8]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Op {
    Track,
    Untrack,
    Update,
}

// The waypoints as the server keeps them for you, and who it has just brought closer. A waypoint that moves more than
// a block in a tick (or changes kind) is untracked and tracked again, so the kind it had just before (within
// REMEMBER ms) still counts as where it was.
#[derive(Default)]
pub(super) struct Book {
    pub(super) players: Vec<(u128, Fix)>,
    gone: Vec<(u128, u8, u64)>,
    // Brought closer: who, the rank before (0 none), the fix now.
    pub(super) closer: Vec<(u128, u8, Fix)>,
}

const REMEMBER: u64 = 2000;

impl Book {
    pub(super) fn clear(&mut self) {
        self.players.clear();
        self.gone.clear();
        self.closer.clear();
    }

    pub(super) fn apply(&mut self, op: Op, uuid: u128, fix: Option<Fix>, now: u64, alerts: bool) {
        let at = self.players.iter().position(|(u, _)| *u == uuid);
        let before = at.map(|i| self.players[i].1);
        self.gone.retain(|(_, _, when)| now.saturating_sub(*when) <= REMEMBER);
        let after = match (op, fix) {
            (Op::Untrack, _) => {
                if let Some(i) = at {
                    self.players.swap_remove(i);
                    self.gone.retain(|(u, ..)| *u != uuid);
                    if self.gone.len() < 256 {
                        self.gone.push((uuid, before.map_or(0, Fix::rank), now));
                    }
                }
                return;
            }
            (Op::Track, Some(f)) => f,
            // Vanilla's update moves a waypoint of the same kind only.
            (Op::Update, Some(f)) if before.is_some_and(|b| b.rank() == f.rank()) => f,
            _ => return,
        };
        let was = before.map(Fix::rank).or_else(|| self.gone.iter().find(|(u, ..)| *u == uuid).map(|g| g.1)).unwrap_or(0);
        self.gone.retain(|(u, ..)| *u != uuid);
        match at {
            Some(i) => self.players[i].1 = after,
            None if self.players.len() < 1024 => self.players.push((uuid, after)),
            None => return,
        }
        if alerts && after.rank() >= 2 && after.rank() > was && self.closer.len() < 64 {
            self.closer.push((uuid, was, after));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: [f32; 2], expected: [f32; 2]) {
        assert!(
            (actual[0] - expected[0]).abs() < 0.001,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual[1] - expected[1]).abs() < 0.001,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn north_up_and_heading_up_match_minecraft_cardinal_yaws() {
        near(project(0., -24., 0., false, 48., 60.).unwrap(), [0., -30.]);
        near(project(24., 0., 90., false, 48., 60.).unwrap(), [30., 0.]);
        // Forward at yaw 0, 90, 180 and -90 must always appear above center.
        for (dx, dz, yaw) in [
            (0., 24., 0.),
            (-24., 0., 90.),
            (0., -24., 180.),
            (24., 0., -90.),
        ] {
            near(project(dx, dz, yaw, true, 48., 60.).unwrap(), [0., -30.]);
        }
        near(project(24., 0., 0., true, 48., 60.).unwrap(), [-30., 0.]);
    }

    #[test]
    fn range_is_horizontal_inclusive_and_rejects_invalid_coordinates() {
        near(project(48., 0., 0., false, 48., 60.).unwrap(), [60., 0.]);
        assert!(project(48.001, 0., 0., false, 48., 60.).is_none());
        assert!(project(40., 40., 0., true, 48., 60.).is_none());
        assert!(project(f64::NAN, 0., 0., false, 48., 60.).is_none());
        assert!(project(0., 0., f64::INFINITY, true, 48., 60.).is_none());
        assert!(project(0., 0., 0., false, 0., 60.).is_none());
    }

    #[test]
    fn requested_size_fits_small_viewports_without_mutating_settings() {
        assert_eq!(fitted_size(150., 427., 240.), Some(150.));
        assert_eq!(fitted_size(320., 320., 180.), Some(164.));
        assert_eq!(fitted_size(150., 70., 100.), Some(66.));
        assert_eq!(fitted_size(150., 60., 100.), None);
    }

    #[test]
    fn edge_width_is_one_physical_pixel_at_fractional_and_integer_gui_scales() {
        for scale in [0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 8.0] {
            assert!((pixel_width(scale) * scale - 1.0).abs() < 0.00001);
        }
        assert_eq!(pixel_width(f32::NAN), 1.0);
        assert_eq!(pixel_width(0.0), 1.0);
    }

    #[test]
    fn circle_tessellation_tracks_device_resolution_with_bounded_chord_error() {
        for radius in [2.3, 3.0, 49.0, 74.0, 159.0] {
            for scale in [1.0, 1.25, 2.0, 3.0, 4.0, 8.0] {
                let segments = circle_segments(radius, scale);
                let error = f64::from(radius * scale)
                    * (1.0 - (std::f64::consts::PI / segments as f64).cos());
                assert!(error <= 0.25001, "{radius} at scale {scale}: {error}");
                assert!(segments <= 256 && segments >= 16 && segments.is_power_of_two());
            }
        }
        assert!(circle_segments(74.0, 3.0) > circle_segments(74.0, 1.0));
    }

    #[test]
    fn cached_circle_vertices_are_closed_and_not_quantized_to_gui_scanlines() {
        near(circle_point([0., 0.], 10., 0, 64), [10., 0.]);
        near(circle_point([0., 0.], 10., 16, 64), [0., 10.]);
        near(circle_point([0., 0.], 10., 64, 64), [10., 0.]);
        let [x, y] = circle_point([0., 0.], 10., 1, 64);
        assert!((x.hypot(y) - 10.0).abs() < 0.00001);
        assert!((y - y.round()).abs() > 0.01);
    }

    #[test]
    fn locator_bearing_points_where_the_server_measured() {
        // Horizon's azimuth: atan2(receiver.x - source.x, source.z - receiver.z); a source due east (+X) of you.
        let a = (0.0f64 - 100.0).atan2(0.0) as f32;
        let d = Fix::Bearing(a).direction_from(0.0, 0.0).unwrap();
        assert!((d[0] - 1.0).abs() < 1e-6 && d[1].abs() < 1e-6, "{d:?}");
        assert_eq!(compass(d), "E");
        assert_eq!(compass([0.0, -1.0]), "N");
        assert_eq!(compass([-1.0, 1.0]), "SW");
        let c = Fix::Chunk([2, -1]).offset_from(0.0, 0.0).unwrap();
        assert_eq!(c, [40.0, -8.0]);
        assert!(Fix::Block([0, 64, 0]).direction_from(0.5, 0.5).is_none());
    }

    #[test]
    fn locator_book_reports_only_coming_closer() {
        let mut b = Book::default();
        b.apply(Op::Track, 1, Some(Fix::Bearing(0.5)), 0, true);
        assert!(b.closer.is_empty(), "a bearing only is not close");
        // Kind changes come as untrack + track.
        b.apply(Op::Untrack, 1, None, 100, true);
        b.apply(Op::Track, 1, Some(Fix::Chunk([30, 0])), 100, true);
        assert_eq!(b.closer, vec![(1, 1, Fix::Chunk([30, 0]))]);
        b.closer.clear();
        b.apply(Op::Update, 1, Some(Fix::Chunk([29, 0])), 150, true);
        b.apply(Op::Update, 1, Some(Fix::Block([5, 64, 5])), 160, true);
        assert_eq!(b.players, vec![(1, Fix::Chunk([29, 0]))], "an update of another kind is ignored");
        b.apply(Op::Untrack, 1, None, 200, true);
        b.apply(Op::Track, 1, Some(Fix::Block([5, 64, 5])), 200, true);
        assert_eq!(b.closer, vec![(1, 2, Fix::Block([5, 64, 5]))]);
        b.closer.clear();
        // A fast mover re-tracked at the same kind, and one moving away: nothing.
        b.apply(Op::Untrack, 1, None, 300, true);
        b.apply(Op::Track, 1, Some(Fix::Block([9, 64, 5])), 300, true);
        b.apply(Op::Untrack, 1, None, 400, true);
        b.apply(Op::Track, 1, Some(Fix::Chunk([3, 3])), 400, true);
        assert!(b.closer.is_empty(), "{:?}", b.closer);
        // Gone long enough (out of the dimension, a pumpkin) and back close: counts again; quiet (just joined): not.
        b.apply(Op::Untrack, 1, None, 500, true);
        b.apply(Op::Track, 1, Some(Fix::Block([1, 64, 1])), 5000, true);
        b.apply(Op::Track, 2, Some(Fix::Block([1, 64, 1])), 5000, false);
        assert_eq!(b.closer, vec![(1, 0, Fix::Block([1, 64, 1]))]);
        assert_eq!(b.players.len(), 2);
    }
}
