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

#[test]
fn gui_pause_retains_owned_false_and_detects_later_user_edit() {
    let mut value = true;
    let mut context = Some(Owned::new(false, true));
    assert!(context.as_mut().unwrap().update(&mut value, false));
    assert!(!value);
    assert!(context.is_some());
    for _ in 0..3 {
        assert!(context.as_mut().unwrap().update(&mut value, false));
    }
    // A .set/toggle received while chat is open changes the current value.
    value = true;
    assert!(!context.as_mut().unwrap().update(&mut value, false));
    assert!(!context.take().unwrap().restore(&mut value));
    assert!(value);
    assert!(context.is_none());
}

#[test]
fn paused_field_restore_preserves_unedited_prior_true_and_partial_user_edits() {
    let mut enabled = false;
    let on = Owned::new(true, false);
    assert!(on.restore(&mut enabled));
    assert!(enabled);
    let mut entities = "player";
    let mut owned = Owned::new("@all", "player");
    assert!(owned.update(&mut entities, "player"));
    entities = "husk";
    assert!(!owned.update(&mut entities, "player"));
    assert!(!owned.restore(&mut entities));
    assert_eq!(entities, "husk");
}

#[test]
fn persistence_serializes_baseline_without_consuming_live_context_or_user_edits() {
    let enabled = Owned::new(true, false); // GUI pause owns false, not no ownership.
    let entities = Owned::new("@all".to_owned(), "player".to_owned());
    let auto_switch = Owned::new(false, true);
    let live = (false, "husk".to_owned(), true, 5.0); // The user edited Entities/Range.
    let mut saved = live.clone();
    assert!(enabled.baseline(&mut saved.0));
    assert!(!entities.baseline(&mut saved.1));
    assert!(auto_switch.baseline(&mut saved.2));
    let text = format!(
        "on={}\nEntities={}\nAutoSwitch={}\nRange={}\n",
        saved.0, saved.1, saved.2, saved.3
    );
    assert_eq!(text, "on=true\nEntities=husk\nAutoSwitch=false\nRange=5\n");
    assert_eq!(live, (false, "husk".to_owned(), true, 5.0));
    // Repeated save/export does not consume ownership or adopt the projection.
    let mut next = live.clone();
    enabled.baseline(&mut next.0);
    auto_switch.baseline(&mut next.2);
    assert_eq!(next, saved);
}

#[test]
fn persistence_never_reclaims_a_yielded_field_that_later_matches_applied() {
    let mut lease = Owned::new("@all", "player");
    let mut current = "husk";
    assert!(!lease.update(&mut current, "player"));
    current = "player";
    assert!(!lease.baseline(&mut current));
    assert_eq!(current, "player");
}
