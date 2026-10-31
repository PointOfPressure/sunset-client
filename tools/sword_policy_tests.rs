#[path = "../mixins/sunset/sword_policy.rs"]
mod policy;
use policy::*;
fn view() -> View {
    View {
        distance: 3.5,
        enemy_distance: 3.5,
        closing: 0.0,
        enemy_facing: 0.0,
        charge: 0.5,
        health: 20.0,
        eating: false,
        hurt: false,
        immune: false,
        shielded: false,
        grounded: true,
        can_jump: true,
        aggressive: false,
        strafe: true,
        jump_crits: true,
        jump_reset: true,
        eat_below: 10.0,
        side: 1,
    }
}
#[test]
fn patient_recharges_outside_reach_and_aggressive_closes() {
    let p = decide(view()).unwrap();
    let a = decide(View {
        aggressive: true,
        ..view()
    })
    .unwrap();
    assert_eq!(p.forward, -1);
    assert_eq!(a.forward, 1);
    assert!(p.desired_gap > a.desired_gap);
    assert_eq!(p.side, 1);
}
#[test]
fn ready_closes_but_invulnerability_and_food_pause_attacks() {
    assert!(
        decide(View {
            distance: 2.8,
            charge: 1.0,
            ..view()
        })
        .unwrap()
        .attack
    );
    assert!(
        !decide(View {
            distance: 2.8,
            charge: 1.0,
            immune: true,
            ..view()
        })
        .unwrap()
        .attack
    );
    let food = decide(View {
        health: 8.0,
        charge: 1.0,
        distance: 2.8,
        ..view()
    })
    .unwrap();
    assert!(food.eat);
    assert_eq!(food.forward, -1);
    assert!(!food.attack);
    assert!(!food.jump);
}
#[test]
fn jumps_require_real_ground_and_useful_window() {
    assert!(!decide(view()).unwrap().jump);
    assert!(
        decide(View {
            enemy_facing: 150.0,
            ..view()
        })
        .unwrap()
        .jump
    );
    assert!(
        !decide(View {
            enemy_facing: 150.0,
            grounded: false,
            ..view()
        })
        .unwrap()
        .jump
    );
    assert!(
        decide(View {
            hurt: true,
            ..view()
        })
        .unwrap()
        .jump
    );
}
#[test]
fn leases_restore_only_unchanged_owned_fields() {
    let mut v = 2;
    let mut lease = Owned::new(1, 2);
    assert!(lease.update(&mut v, 3));
    assert_eq!(v, 3);
    assert!(lease.restore(&mut v));
    assert_eq!(v, 1);
    let mut v = 9;
    let mut lease = Owned::new(1, 2);
    assert!(!lease.update(&mut v, 3));
    assert!(!lease.restore(&mut v));
    assert_eq!(v, 9);
}
#[test]
fn safe_step_rejects_unknown_support_and_large_drops() {
    assert!(support_safe(true, Some(0.5), false));
    assert!(!support_safe(true, None, false));
    assert!(!support_safe(true, Some(2.0), false));
    assert!(!support_safe(true, Some(0.5), true));
    assert!(!support_safe(false, Some(0.5), false));
    assert!(!support_safe(true, Some(f64::NAN), false));
}
#[test]
fn camera_wraps_short_way_and_caps_combined_turn() {
    let d = camera_step([179.0, 0.0], [-179.0, 0.0], 1.0).unwrap();
    assert_eq!(d, [2.0, 0.0]);
    let d = camera_step([0.0, 0.0], [120.0, 50.0], 1.0).unwrap();
    assert!((d[0].hypot(d[1]) - 18.0).abs() < 1e-8);
    assert!(camera_step([f32::NAN, 0.0], [0.0; 2], 1.0).is_none());
}
#[test]
fn malformed_state_yields_before_moving() {
    assert!(
        decide(View {
            distance: f64::NAN,
            ..view()
        })
        .is_none()
    );
    assert!(
        decide(View {
            health: 0.0,
            ..view()
        })
        .is_none()
    );
    assert_eq!(POPULATION_LIMIT, 512);
}
