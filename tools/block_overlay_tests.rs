// Host-free checks: rustc --edition=2024 --test tools/block_overlay_tests.rs
#[path = "../mixins/sunset/block_overlay_math.rs"]
mod math;
#[path = "../mixins/sunset/block_overlay_flags.rs"]
mod flags;

#[test]
fn hidden_hud_and_each_menu_or_missing_world_condition_hide_overlay() {
    assert!(math::visible(true, false, false, true, true));
    assert!(!math::visible(false, false, false, true, true));
    assert!(!math::visible(true, true, false, true, true));
    assert!(!math::visible(true, false, true, true, true));
    assert!(!math::visible(true, false, false, false, true));
    assert!(!math::visible(true, false, false, true, false));
}

#[test]
fn opacity_replaces_palette_alpha_and_preserves_rgb() {
    assert_eq!(math::colour(0xAA12_3456, 0.0), 0x0012_3456);
    assert_eq!(math::colour(0x0012_3456, 1.0), 0xFF12_3456);
    assert_eq!(math::colour(0xFF12_3456, 0.5), 0x8012_3456);
    assert_eq!(math::colour(0xFF12_3456, 0.15), 0x2612_3456);
}

#[test]
fn untrusted_opacity_is_bounded_and_nonfinite_is_invisible() {
    assert_eq!(math::colour(0x123456, -1.0), 0x0012_3456);
    assert_eq!(math::colour(0x123456, 2.0), 0xFF12_3456);
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(math::colour(0x123456, v), 0x0012_3456);
    }
}

#[test]
fn corrupted_width_falls_back_instead_of_emitting_nan_vertices() {
    assert_eq!(math::bounded(f64::NAN, 0.5, 4.0, 1.0), 1.0);
    assert_eq!(math::bounded(f64::INFINITY, 0.5, 4.0, 1.0), 1.0);
    assert_eq!(math::bounded(-2.0, 0.5, 4.0, 1.0), 0.5);
    assert_eq!(math::bounded(12.0, 0.5, 4.0, 1.0), 4.0);
}

#[test]
fn partial_shapes_keep_dimensions_offset_and_negative_world_position() {
    let slab = [0.0, 0.0, 0.0, 1.0, 0.5, 1.0];
    let b = math::translated(slab, [-12.25, 64.0, 3.125]);
    assert_eq!(b, [-12.25, 64.0, 3.125, -11.25, 64.5, 4.125]);
    assert!(math::valid_box(b));
}

#[test]
fn dynamic_context_box_can_extend_outside_unit_cube() {
    let open_shulker = [0.0, -0.5, 0.0, 1.0, 1.0, 1.0];
    assert_eq!(math::translated(open_shulker, [0.0, 1.0, 0.0]), [0.0, 0.5, 0.0, 1.0, 2.0, 1.0]);
    assert!(math::valid_box(open_shulker));
}

#[test]
fn malformed_and_zero_volume_boxes_do_not_fill() {
    assert!(!math::valid_box([0.0; 6]));
    assert!(!math::valid_box([1.0, 0.0, 0.0, 0.0, 1.0, 1.0]));
    assert!(!math::valid_box([0.0, 0.0, 0.0, f64::NAN, 1.0, 1.0]));
    assert!(!math::valid_box([0.0, 0.0, 0.0, 1.0, f64::INFINITY, 1.0]));
}

#[test]
fn translated_union_edges_preserve_shape_topology_at_far_coordinates() {
    let edge = [0.125, 0.5, 0.75, 0.875, 0.5, 0.75];
    let e = math::translated(edge, [29_999_999.0, -64.0, -29_999_999.0]);
    assert_eq!(e[3] - e[0], 0.75);
    assert_eq!(e[4], e[1]);
    assert_eq!(e[5], e[2]);
}

#[test]
fn fill_faces_cover_each_axis_side_without_crossing_quad_corners() {
    let mut seen = [[false; 2]; 3];
    for (corners, axis, high) in math::FACES {
        assert!(!seen[axis][usize::from(high)]);
        seen[axis][usize::from(high)] = true;
        for i in 0..4 {
            assert_eq!((corners[i] >> axis) & 1, usize::from(high));
            assert_eq!((corners[i] ^ corners[(i + 1) % 4]).count_ones(), 1);
        }
    }
    assert!(seen.into_iter().flatten().all(|v| v));
}

#[test]
fn contextual_box_has_twelve_distinct_axis_aligned_edges_without_heap_storage() {
    let b = [-2.0, 3.0, -4.0, -1.0, 4.5, -3.0];
    let mut seen = [[false; 8]; 8];
    for (a, z) in math::EDGES {
        assert_eq!((a ^ z).count_ones(), 1);
        assert!(!seen[a][z]);
        seen[a][z] = true;
        seen[z][a] = true;
        let e = math::box_edge(b, (a, z));
        assert_eq!((0..3).filter(|&axis| e[axis] != e[axis + 3]).count(), 1);
        for (i, v) in e.into_iter().enumerate() {
            assert!(v == b[i % 3] || v == b[i % 3 + 3]);
        }
    }
    assert_eq!(math::box_edge(b, (0, 2)), [-2.0, 3.0, -4.0, -2.0, 4.5, -4.0]);
}

#[test]
fn each_frame_publication_replaces_the_previous_suppression_value() {
    flags::publish(false);
    assert!(!flags::active());
    flags::publish(true);
    assert!(flags::active());
    flags::publish(true);
    assert!(flags::active());
    flags::publish(false);
    assert!(!flags::active());
}

#[test]
fn frame_suppression_never_hides_another_threads_outline() {
    flags::publish(true);
    assert!(flags::active());
    assert!(!std::thread::spawn(flags::active).join().unwrap());
    flags::publish(false);
}
