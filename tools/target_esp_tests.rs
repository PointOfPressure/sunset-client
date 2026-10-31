#[path = "../mixins/sunset/target_esp_math.rs"]
mod target_esp_math;
use target_esp_math::{brackets, clip, eligible, sightline};

#[test]
fn occlusion_ray_is_unit_length_and_bounded_to_the_target_center() {
    let (direction, distance) = sightline([1.0, 2.0, 3.0], [4.0, 6.0, 3.0], 5.0).unwrap();
    assert_eq!(direction, [0.6, 0.8, 0.0]);
    assert_eq!(distance, 5.0);
    assert!(sightline([1.0, 2.0, 3.0], [4.0, 6.0, 3.0], 4.99).is_none());
    assert!(sightline([0.0; 3], [0.0; 3], 32.0).is_none());
    assert!(sightline([f64::NAN, 0.0, 0.0], [1.0; 3], 32.0).is_none());
    assert!(sightline([0.0; 3], [f64::INFINITY, 0.0, 0.0], 32.0).is_none());
}

#[test]
fn lifecycle_eligibility_rejects_self_dead_removed_hidden_and_bad_health() {
    assert!(eligible(2, Some(1), 20.0, false, false, false, false));
    assert!(!eligible(1, Some(1), 20.0, false, false, true, false));
    for health in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(!eligible(2, Some(1), health, false, false, true, false));
    }
    assert!(!eligible(2, Some(1), 20.0, true, false, true, false));
    assert!(!eligible(2, Some(1), 20.0, false, true, true, false));
    assert!(!eligible(2, Some(1), 20.0, false, false, false, true));
    assert!(eligible(2, Some(1), 20.0, false, false, true, true));
}

#[test]
fn brackets_leave_center_open_and_stay_bounded_to_eight_arms() {
    let rects = brackets([100.0, 50.0, 140.0, 130.0], 2.0, 3.0, 9.0).unwrap();
    assert_eq!(rects.len(), 8);
    assert_eq!(rects[0], [97.0, 47.0, 9.0, 2.0]);
    assert_eq!(rects[6], [134.0, 131.0, 9.0, 2.0]);
    for [x, y, w, h] in rects {
        assert!(w > 0.0 && h > 0.0);
        assert!(!(x <= 120.0 && x + w >= 120.0 && y <= 90.0 && y + h >= 90.0));
    }
}

#[test]
fn small_targets_shorten_arms_and_degenerate_projections_disappear() {
    let rects = brackets([20.0, 20.0, 24.0, 28.0], 4.0, 0.0, 20.0).unwrap();
    for rect in rects {
        assert!(rect[2] <= 1.0 && rect[3] <= 1.0);
    }
    assert!(brackets([1.0, 1.0, 2.0, 50.0], 1.0, 0.0, 9.0).is_none());
    assert!(brackets([10.0, 1.0, 2.0, 50.0], 1.0, 0.0, 9.0).is_none());
    assert!(brackets([0.0, f32::NAN, 20.0, 50.0], 1.0, 0.0, 9.0).is_none());
    assert!(brackets([0.0, 0.0, 20.0, 50.0], f32::INFINITY, 0.0, 9.0).is_none());
}

#[test]
fn clipping_keeps_real_corners_without_inventing_screen_edge_markers() {
    let rects = brackets([-100.0, -100.0, 500.0, 500.0], 2.0, 3.0, 9.0).unwrap();
    assert!(
        rects
            .into_iter()
            .all(|r| clip(r, [320.0, 200.0], 1.0).is_none())
    );
    assert_eq!(
        clip([-1.0, 5.0, 5.0, 2.0], [320.0, 200.0], 0.0),
        Some([0.0, 5.0, 4.0, 2.0])
    );
    assert!(clip([400.0, 20.0, 5.0, 2.0], [320.0, 200.0], 1.0).is_none());
    assert!(clip([1.0, 1.0, 5.0, 2.0], [f32::NAN, 200.0], 1.0).is_none());
}

#[test]
fn outline_and_foreground_are_clipped_at_fractional_viewport_edges() {
    for bounds in [[-4.0, 5.0, 20.0, 45.0], [300.0, 180.0, 330.0, 220.0]] {
        let rects = brackets(bounds, 1.5, 3.0, 9.0).unwrap();
        for grow in [0.0, 0.5, 1.25] {
            for rect in rects {
                if let Some([x, y, w, h]) = clip(rect, [319.5, 199.75], grow) {
                    assert!(x >= 0.0 && y >= 0.0 && w > 0.0 && h > 0.0);
                    assert!(x + w <= 319.5 && y + h <= 199.75);
                }
            }
        }
    }
}
