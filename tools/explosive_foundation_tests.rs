#[path = "../mixins/sunset/explosive_policy.rs"]
mod policy;
use policy::*;

fn limits() -> Limits {
    Limits { min_target: 4.0, max_self: 6.0, max_friend: 2.0, min_remaining: 2.0, anti_suicide: true }
}

#[test]
fn thresholds_include_caps_but_never_allow_a_lethal_hit() {
    assert!(safe(limits(), 4.0, 6.0, 8.0, 2.0));
    assert!(!safe(limits(), 3.99, 6.0, 8.0, 2.0));
    assert!(!safe(limits(), 4.0, 6.01, 8.0, 2.0));
    assert!(!safe(limits(), 4.0, 6.0, 7.99, 2.0));
    assert!(!safe(limits(), 4.0, 6.0, 8.0, 2.01));
    let no_reserve = Limits { min_remaining: 0.0, ..limits() };
    assert!(!safe(no_reserve, 4.0, 6.0, 6.0, 0.0));
    assert!(safe(Limits { anti_suicide: false, ..limits() }, 4.0, 6.0, 1.0, 0.0));
}

#[test]
fn nonfinite_or_negative_inputs_fail_closed_in_every_policy_position() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01] {
        for position in 0..8 {
            let mut values = [4.0, 6.0, 2.0, 2.0, 4.0, 6.0, 8.0, 2.0];
            values[position] = bad;
            let l = Limits { min_target:values[0], max_self:values[1], max_friend:values[2],
                min_remaining:values[3], anti_suicide:true };
            assert!(!safe(l, values[4], values[5], values[6], values[7]), "position={position} bad={bad}");
        }
    }
}

#[test]
fn range_has_an_inclusive_boundary_and_rejects_bad_positions() {
    assert!(in_range([0.0;3], [3.0,4.0,0.0], 5.0));
    assert!(!in_range([0.0;3], [3.0,4.0,0.01], 5.0));
    assert!(!in_range([f64::NAN,0.0,0.0], [0.0;3], 5.0));
    assert!(!in_range([0.0;3], [0.0;3], -1.0));
    assert!(!in_range([0.0;3], [0.0;3], f64::INFINITY));
}

#[test]
fn explosion_formula_matches_pinned_engine_f32_conversion_without_integer_rounding() {
    assert_eq!(raw_damage(0.0, 6.0, 1.0), Some(85.0));
    assert_eq!(raw_damage(0.0, 5.0, 1.0), Some(71.0));
    assert_eq!(raw_damage(6.0, 6.0, 1.0), Some(32.5));
    assert_eq!(raw_damage(12.0, 6.0, 1.0), Some(1.0));
    assert_eq!(raw_damage(12.00001, 6.0, 1.0), Some(0.0));
    // Engine's +1 survives completely obstructed exposure within radius.
    assert_eq!(raw_damage(2.0, 5.0, 0.0), Some(1.0));
    for (distance,power,exposure) in [(f64::NAN,6.0,1.0),(-1.0,6.0,1.0),
        (1.0,f32::INFINITY,1.0),(1.0,0.0,1.0),(1.0,6.0,f32::NAN),(1.0,6.0,1.01)] {
        assert!(raw_damage(distance,power,exposure).is_none());
    }
}

#[test]
fn player_grid_is_45_engine_samples_with_xz_offsets_only() {
    let grid = samples([-0.3,0.0,-0.3],[0.3,1.8,0.3]).unwrap();
    assert_eq!(grid.len(),45);
    let offset=(1.0-(2.2f64).floor()/2.2)/2.0;
    assert_eq!(grid[0],[-0.3+offset,0.0,-0.3+offset]);
    assert!(grid.iter().all(|p| p[1]>=0.0 && p[1]<=1.8));
    assert_eq!(samples([0.0;3],[0.0;3]).unwrap().len(),8);
}

#[test]
fn malformed_or_giant_bounds_never_generate_a_partial_grid() {
    for (lo,hi) in [([f64::NAN,0.0,0.0],[1.0;3]),([1.0;3],[0.0;3]),
        ([0.0;3],[1000.0;3]),([0.0;3],[16.0;3]),([30_000_001.0;3],[30_000_002.0;3])] {
        assert!(samples(lo,hi).is_none());
    }
}

#[test]
fn budget_reserves_whole_predictions_and_has_absolute_ray_and_prediction_caps() {
    let mut b=Budget::new(2);
    assert!(b.prediction(45)); assert!(b.prediction(45)); assert!(!b.prediction(45));
    assert_eq!((b.predictions,b.rays,b.rejected),(2,90,1));
    let mut b=Budget::new(usize::MAX);
    for _ in 0..16 { assert!(b.prediction(256)); }
    assert!(!b.prediction(1)); assert!(!b.line());
    assert_eq!((b.predictions,b.rays),(16,4096));
    let mut b=Budget::new(usize::MAX);
    for _ in 0..64 { assert!(b.prediction(0)); }
    assert!(!b.prediction(0));
    let mut b=Budget::new(3);
    assert!(!b.prediction(257));
    assert_eq!((b.predictions,b.rays),(0,0));
    assert!(!Budget::new(0).prediction(0));
}

#[test]
fn raw_damage_is_monotone_with_exposure_and_distance_for_both_explosives() {
    for power in [5.0,6.0] {
        for distance in (0..100).map(|n|f64::from(n)*0.1) {
            let values: Vec<_>=(0..=100).map(|n|raw_damage(distance,power,n as f32/100.0).unwrap()).collect();
            assert!(values.windows(2).all(|v|v[0]<=v[1]));
        }
        for exposure in [0.0,0.2,0.5,1.0] {
            let values: Vec<_>=(0..=121).map(|n|raw_damage(f64::from(n)*0.1,power,exposure).unwrap()).collect();
            assert!(values.windows(2).all(|v|v[0]>=v[1]));
        }
    }
}

#[test]
fn armor_break_guard_uses_the_full_pre_mitigation_hit_and_exact_break_boundary() {
    assert!(armor_survives(60.0, 84, 100, true, true, true));
    assert!(!armor_survives(60.0, 85, 100, true, true, true));
    assert!(armor_survives(3.9, 98, 100, true, true, true));
    assert!(!armor_survives(3.9, 99, 100, true, true, true));
    assert!(!armor_survives(40.0, u32::MAX, u32::MAX, true, true, true));
    assert!(armor_survives(40.0, 0, 0, true, true, true));
    assert!(armor_survives(40.0, 99, 100, false, true, true));
}

#[test]
fn unknown_components_or_durability_effects_never_receive_equipment_credit() {
    assert!(!armor_survives(60.0, 0, 500, true, false, true));
    assert!(!armor_survives(60.0, 0, 500, true, true, false));
    assert!(!armor_survives(f32::NAN, 0, 500, true, true, true));
    assert!(!armor_survives(f32::INFINITY, 0, 500, true, true, true));
}
