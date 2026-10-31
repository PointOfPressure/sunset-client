// Standalone: rustc --edition=2024 --test tools/crosshair_tests.rs -o <scratch executable>
#[path = "../mixins/sunset/crosshair_math.rs"]
mod crosshair_math;

use crosshair_math::{Style, geometry};

fn style() -> Style {
    Style { dot_only: false, center_dot: false, length: 6.0, gap: 3.0, thickness: 1.0, dot_size: 2.0, outline: 1.0 }
}

#[test]
fn cross_has_four_equal_arms_and_exact_gap() {
    let shape = geometry([1920, 1080], 3.0, style()).unwrap();
    let r = shape.rects();
    assert_eq!(r.len(), 4);
    assert_eq!(r[0].width, 6.0);
    assert_eq!(r[2].height, 6.0);
    assert_eq!(r[0].height, 1.0);
    assert_eq!(r[2].width, 1.0);
    assert_eq!(r[2].x - (r[0].x + r[0].width), 3.0);
    assert_eq!(r[1].x - (r[2].x + r[2].width), 3.0);
    assert_eq!(r[0].y - (r[2].y + r[2].height), 3.0);
    assert_eq!(r[3].y - (r[0].y + r[0].height), 3.0);
}

#[test]
fn parity_and_size_changes_never_shift_more_than_half_a_pixel() {
    for width in [319, 320, 1920, 1921, 3839, 3840] {
        for height in [239, 240, 1079, 1080, 2159, 2160] {
            for thickness in 1..=8 {
                let shape = geometry([width, height], 1.0, Style { thickness: f64::from(thickness), ..style() }).unwrap();
                let r = shape.rects();
                let center = [r[2].x + r[2].width * 0.5, r[0].y + r[0].height * 0.5];
                for (actual, size) in center.into_iter().zip([width, height]) {
                    assert!((actual - size as f32 * 0.5).abs() <= 0.5);
                }
                assert_eq!(r[0].x + r[0].width + r[1].x, center[0] * 2.0);
                assert_eq!(r[2].y + r[2].height + r[3].y, center[1] * 2.0);
                for rect in r {
                    assert_eq!(rect.x.fract(), 0.0);
                    assert_eq!(rect.y.fract(), 0.0);
                }
            }
        }
    }
}

#[test]
fn physical_geometry_is_independent_of_integer_and_fractional_gui_scale() {
    let reference = geometry([1919, 1079], 1.0, style()).unwrap();
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0] {
        let shape = geometry([1919, 1079], scale, style()).unwrap();
        assert_eq!(shape.rects(), reference.rects());
        for rect in shape.rects() {
            for (gui, physical) in rect.gui(scale).into_iter().zip([rect.x, rect.y, rect.width, rect.height]) {
                assert!((gui * scale - physical).abs() < 0.0002);
            }
        }
    }
}

#[test]
fn center_uses_actual_physical_dimensions_not_rounded_gui_extent() {
    let shape = geometry([1919, 1079], 3.0, style()).unwrap();
    let r = shape.rects();
    assert_eq!(r[2].x + 0.5, 959.5);
    assert_eq!(r[0].y + 0.5, 539.5);
    // ceil(1919 / 3) * 3 would incorrectly use 1920 physical pixels.
    assert_ne!(r[2].x + 0.5, (1919.0_f32 / 3.0).ceil() * 3.0 * 0.5);
}

#[test]
fn dot_mode_always_emits_only_one_centered_dot() {
    for center_dot in [false, true] {
        let shape = geometry([1920, 1080], 1.5, Style { dot_only: true, center_dot, ..style() }).unwrap();
        let r = shape.rects();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].gui(1.0), [959.0, 539.0, 2.0, 2.0]);
    }
}

#[test]
fn optional_dot_adds_one_mark_without_moving_arms() {
    let plain = geometry([1921, 1081], 2.0, style()).unwrap();
    let dotted = geometry([1921, 1081], 2.0, Style { center_dot: true, ..style() }).unwrap();
    assert_eq!(dotted.rects().len(), 5);
    assert_eq!(&dotted.rects()[..4], plain.rects());
}

#[test]
fn outline_expands_every_edge_by_exact_physical_pixels() {
    let shape = geometry([1920, 1080], 2.5, style()).unwrap();
    assert_eq!(shape.outline, 1.0);
    for rect in shape.rects() {
        let outer = rect.expanded(shape.outline);
        assert_eq!(outer.x, rect.x - 1.0);
        assert_eq!(outer.y, rect.y - 1.0);
        assert_eq!(outer.x + outer.width, rect.x + rect.width + 1.0);
        assert_eq!(outer.y + outer.height, rect.y + rect.height + 1.0);
    }
}

#[test]
fn zero_gap_and_outline_are_supported() {
    let shape = geometry([320, 240], 1.0, Style { gap: 0.0, outline: 0.0, ..style() }).unwrap();
    let r = shape.rects();
    assert_eq!(shape.outline, 0.0);
    assert_eq!(r[0].x + r[0].width, r[2].x);
    assert_eq!(r[1].x, r[2].x + r[2].width);
}

#[test]
fn corrupt_dimensions_are_sanitized_and_bounded() {
    let bad = Style { length: f64::INFINITY, gap: f64::NAN, thickness: -20.0, dot_size: 1000.0, outline: 1000.0, center_dot: true, ..style() };
    let shape = geometry([320, 240], 1.0, bad).unwrap();
    assert_eq!(shape.rects()[0].width, 6.0);
    assert_eq!(shape.rects()[0].height, 1.0);
    assert_eq!(shape.rects()[4].width, 8.0);
    assert_eq!(shape.outline, 4.0);
    assert!(shape.rects().iter().all(|r| [r.x, r.y, r.width, r.height].into_iter().all(f32::is_finite)));
}

#[test]
fn invalid_viewports_and_scales_fail_closed() {
    for size in [[0, 1080], [1920, 0], [u32::MAX, 1080]] {
        assert!(geometry(size, 1.0, style()).is_none());
    }
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(geometry([1920, 1080], scale, style()).is_none());
    }
}
