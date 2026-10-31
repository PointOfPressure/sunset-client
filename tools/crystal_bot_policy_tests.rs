#[path = "../mixins/sunset/crystal_bot_policy.rs"]
mod policy;
use policy::*;
fn view() -> View {
    View {
        tick: 100,
        health: 20.0,
        absorption: 0.0,
        distance: Some(5.0),
        enemies_near: 1,
        totems: 1,
        empty_offhand: false,
        refill_possible: false,
        supplied: true,
        food: true,
        using_food: false,
        eat_below: 14.0,
        threat: Some(0.0),
        threat_complete: true,
        strafe: true,
        side: 1,
    }
}
#[test]
fn healthy_press_closes_but_preserves_a_real_standoff() {
    let mut b = Brain::default();
    let d = b.decide(view()).unwrap();
    assert_eq!(d.phase, Phase::Press);
    assert_eq!(d.forward, 1);
    assert!(d.auras);
    assert_eq!(
        b.decide(View {
            distance: Some(2.0),
            ..view()
        })
        .unwrap()
        .forward,
        -1
    );
}
#[test]
fn food_retreat_is_latched_until_observed_recovery() {
    let mut b = Brain::default();
    let d = b
        .decide(View {
            health: 10.0,
            ..view()
        })
        .unwrap();
    assert_eq!(d.phase, Phase::Retreat);
    assert!(!d.auras);
    assert!(!d.eat);
    let d = b
        .decide(View {
            tick: 101,
            health: 15.0,
            distance: Some(7.0),
            ..view()
        })
        .unwrap();
    assert_eq!(d.phase, Phase::Eat);
    assert!(d.eat);
    let d = b
        .decide(View {
            tick: 150,
            health: 15.0,
            absorption: 4.0,
            ..view()
        })
        .unwrap();
    assert_eq!(d.phase, Phase::Press);
}
#[test]
fn actual_refill_priority_yields_food_and_auras() {
    let mut b = Brain::default();
    let d = b
        .decide(View {
            health: 8.0,
            distance: Some(9.0),
            empty_offhand: true,
            refill_possible: true,
            ..view()
        })
        .unwrap();
    assert_eq!(d.phase, Phase::Supply);
    assert!(!d.eat);
    assert!(!d.auras);
}
#[test]
fn no_items_does_not_chase_or_create_combat() {
    let mut b = Brain::default();
    let d = b
        .decide(View {
            supplied: false,
            distance: Some(9.0),
            ..view()
        })
        .unwrap();
    assert_eq!(d.phase, Phase::Supply);
    assert_eq!(d.forward, 0);
    assert!(!d.auras);
}
#[test]
fn unknown_threat_and_multiple_enemies_trigger_bounded_retreat() {
    let mut b = Brain::default();
    let d = b
        .decide(View {
            threat: None,
            ..view()
        })
        .unwrap();
    assert_eq!(d.phase, Phase::Retreat);
    assert!(b.retreat_until >= 130);
    let mut b = Brain::default();
    assert_eq!(
        b.decide(View {
            totems: 0,
            enemies_near: 2,
            ..view()
        })
        .unwrap()
        .phase,
        Phase::Retreat
    );
}
#[test]
fn partial_threat_samples_guard_without_claiming_exhaustive_safety() {
    let d = Brain::default()
        .decide(View {
            threat_complete: false,
            ..view()
        })
        .unwrap();
    assert_eq!(d.phase, Phase::Guard);
    assert!(d.auras);
    assert_eq!(d.ideal, 4.3);
}
#[test]
fn only_three_actual_pop_events_create_long_retreat() {
    let mut b = Brain::default();
    b.pop(10);
    b.pop(20);
    assert_eq!(b.retreat_until, 0);
    b.pop(30);
    assert_eq!(b.retreat_until, 230);
    let mut b = Brain::default();
    b.pop(0);
    b.pop(601);
    b.pop(602);
    assert_eq!(b.retreat_until, 0);
}
#[test]
fn module_ownership_never_restores_over_user_changes() {
    let mut current = true;
    let mut lease = Owned::new(false, true);
    assert!(lease.update(&mut current, false));
    assert!(lease.restore(&mut current));
    assert!(!current);
    let mut current = false;
    let mut lease = Owned::new(false, true);
    assert!(!lease.update(&mut current, true));
    assert!(!lease.restore(&mut current));
    assert!(!current);
}
#[test]
fn movement_rejects_unknown_drops_and_caps_turns() {
    assert!(support_safe(true, Some(0.5), false));
    assert!(!support_safe(true, None, false));
    assert!(!support_safe(true, Some(2.0), false));
    assert!(!support_safe(true, Some(0.5), true));
    let d = camera_step([179.0, 0.0], [-179.0, 0.0], 1.0).unwrap();
    assert_eq!(d, [2.0, 0.0]);
    let d = camera_step([0.0, 0.0], [90.0, 40.0], 1.0).unwrap();
    assert!((d[0].hypot(d[1]) - 18.0).abs() < 1e-9);
}
#[test]
fn malformed_observations_fail_before_actions() {
    let mut b = Brain::default();
    assert!(
        b.decide(View {
            health: f64::NAN,
            ..view()
        })
        .is_none()
    );
    assert!(
        b.decide(View {
            distance: Some(f64::NAN),
            ..view()
        })
        .is_none()
    );
    assert!(
        b.decide(View {
            threat: Some(-1.0),
            ..view()
        })
        .is_none()
    );
    assert_eq!(
        (
            POPULATION_LIMIT,
            CRYSTAL_LIMIT,
            ANCHOR_CELLS,
            THREAT_PREDICTIONS
        ),
        (512, 128, 64, 4)
    );
    assert!(camera_step([f32::NAN, 0.0], [0.0; 2], 1.0).is_none());
}

#[test]
fn repeated_gui_pause_keeps_false_owned_until_a_real_user_override() {
    let mut value = true;
    let mut context = Some(Owned::new(false, true));
    for _ in 0..4 {
        assert!(context.as_mut().unwrap().update(&mut value, false));
    }
    assert!(!value);
    assert!(context.is_some());
    value = true;
    assert!(!context.as_mut().unwrap().update(&mut value, false));
    assert!(!context.take().unwrap().restore(&mut value));
    assert!(value);
}

#[test]
fn partial_restore_does_not_erase_entities_set_during_chat() {
    let mut enabled = false;
    let on = Owned::new(true, false);
    let mut entities = "player";
    let mut owned = Owned::new("@all", "player");
    assert!(owned.update(&mut entities, "player"));
    entities = "husk";
    assert!(!owned.update(&mut entities, "player"));
    assert!(on.restore(&mut enabled));
    assert!(enabled);
    assert!(!owned.restore(&mut entities));
    assert_eq!(entities, "husk");
}

#[test]
fn serialized_config_projects_only_owned_fields_and_keeps_user_edits() {
    let crystal = Owned::new(false, true);
    let anchor = Owned::new(true, false);
    let hotbar = Owned::new(false, true);
    let entities = Owned::new("@all", "player");
    let live = (true, false, true, "husk", 16.0);
    let mut saved = live;
    crystal.baseline(&mut saved.0);
    anchor.baseline(&mut saved.1);
    hotbar.baseline(&mut saved.2);
    assert!(!entities.baseline(&mut saved.3));
    let text = format!(
        "crystal={}\nanchor={}\nhotbar={}\nEntities={}\nMaxSelf={}\n",
        saved.0, saved.1, saved.2, saved.3, saved.4
    );
    assert_eq!(
        text,
        "crystal=false\nanchor=true\nhotbar=false\nEntities=husk\nMaxSelf=16\n"
    );
    assert_eq!(live, (true, false, true, "husk", 16.0));
    let mut again = live;
    crystal.baseline(&mut again.0);
    anchor.baseline(&mut again.1);
    hotbar.baseline(&mut again.2);
    assert_eq!(again, saved);
}

#[test]
fn yielded_persistence_keeps_later_explicit_user_value() {
    let mut lease = Owned::new(false, true);
    let mut value = false;
    assert!(!lease.update(&mut value, true));
    value = true;
    assert!(!lease.baseline(&mut value));
    assert!(value);
}
