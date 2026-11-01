#[path = "../mixins/sunset/anchor_state.rs"]
mod state;
use state::{Step, Wait};







#[test]
fn automated_supports_cannot_open_an_interactive_screen() {
    for name in [
        "obsidian",
        "minecraft:stone",
        "cobbled_deepslate",
        "netherite_block",
    ] {
        assert!(state::inert_support(name));
    }
    for name in [
        "crafting_table",
        "minecraft:smithing_table",
        "anvil",
        "loom",
        "chest",
        "respawn_anchor",
        "unknown:block",
        "",
    ] {
        assert!(!state::inert_support(name));
    }
}

#[test]
fn each_clicked_face_ray_stops_outside_support() {
    for (face, hit, axis, sign) in [
        (0, [0.5, 0.0, 0.5], 1, -1.0),
        (1, [0.5, 1.0, 0.5], 1, 1.0),
        (2, [0.5, 0.5, 0.0], 2, -1.0),
        (3, [0.5, 0.5, 1.0], 2, 1.0),
        (4, [0.0, 0.5, 0.5], 0, -1.0),
        (5, [1.0, 0.5, 0.5], 0, 1.0),
    ] {
        let end = state::face_ray_end(hit, face).unwrap();
        assert!((end[axis] - hit[axis]) * sign > 0.0);
        for i in 0..3 {
            if i != axis {
                assert_eq!(end[i], hit[i]);
            }
        }
        assert!(end.iter().any(|n| *n < 0.0 || *n > 1.0));
    }
    assert!(state::face_ray_end([0.0; 3], 6).is_none());
    assert!(state::face_ray_end([f64::NAN; 3], 1).is_none());
}

#[test]
fn native_ordering_rule_requires_full_charge_with_offhand_fuel() {
    assert_eq!(state::next(None, true), Step::Place);
    for charge in 0..4 {
        assert_eq!(state::next(Some(charge), true), Step::Charge);
        assert!(!state::can_detonate(charge, false, true));
    }
    assert_eq!(state::next(Some(4), true), Step::Detonate);
    assert!(state::can_detonate(4, false, true));
    assert!(!state::can_detonate(4, true, true));
}

#[test]
fn one_charge_is_sufficient_without_offhand_fuel() {
    assert_eq!(state::next(Some(0), false), Step::Charge);
    for charge in 1..=4 {
        assert_eq!(state::next(Some(charge), false), Step::Detonate);
        assert!(state::can_detonate(charge, false, false));
        assert!(!state::can_detonate(charge, true, false));
    }
    assert!(!state::can_detonate(0, false, false));
    assert!(!state::can_detonate(5, false, false));
}

#[test]
fn unchanged_world_does_not_progress_or_repeat() {
    for tick in 100..130 {
        assert_eq!(state::observed(77, 77, 100, tick, 30), Wait::Pending);
    }
    assert_eq!(state::observed(77, 77, 100, 130, 30), Wait::Expired);
    assert_eq!(state::observed(77, 78, 100, 101, 30), Wait::Changed);
    assert_eq!(state::observed(77, 0, 100, 130, 30), Wait::Changed);
    assert_eq!(state::observed(77, 77, 100, 99, 30), Wait::Expired);
}
