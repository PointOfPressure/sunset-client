// SPDX-License-Identifier: MIT
// Radar geometry for Horizon. AI-assisted contribution.
// Behavior-research credit and provenance: docs/radar-source-notes.md.

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
}
