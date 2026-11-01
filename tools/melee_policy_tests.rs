#[path = "../mixins/sunset/melee_policy.rs"]
mod policy;

#[test]
fn slot_return_requires_ownership_and_the_module_selected_slot() {
    let lease = policy::SlotLease {
        owner: "test",
        previous: 2,
        selected: 4,
        until: 12,
        return_enabled: true,
    };
    assert_eq!(lease.restore(4, 11, true), None);
    assert_eq!(lease.restore(4, 12, true), Some(2));
    assert_eq!(lease.restore(5, 12, true), None);
    assert_eq!(lease.restore(4, 12, false), None);
    assert!(lease.expired(5, 11, true));
    assert!(lease.expired(4, 11, false));
    assert!(!lease.expired(4, 11, true));
    assert_eq!(lease.owner, "test");
}

#[test]
fn shield_direction_uses_target_facing_and_native_angle() {
    assert!(policy::facing_attack([0.0; 3], 0.0, [0.0, 10.0, 2.0], 90.0));
    assert!(!policy::facing_attack(
        [0.0; 3],
        0.0,
        [0.0, 0.0, -2.0],
        90.0
    ));
    assert!(policy::facing_attack(
        [0.0; 3],
        -90.0,
        [2.0, 0.0, 0.0],
        30.0
    ));
    assert!(!policy::facing_attack([0.0; 3], 0.0, [2.0, 0.0, 0.0], 30.0));
    assert!(!policy::facing_attack([f64::NAN; 3], 0.0, [0.0; 3], 90.0));
}

#[test]
fn mace_bonus_tiers_follow_real_fall() {
    assert_eq!(policy::smash_bonus(3.0), Some(12.0));
    assert_eq!(policy::smash_bonus(8.0), Some(22.0));
    assert_eq!(policy::smash_bonus(10.0), Some(24.0));
    assert_eq!(policy::smash_bonus(f64::NAN), None);
}

#[test]
fn reach_measures_the_entity_box_and_rejects_invalid_server_data() {
    assert_eq!(
        policy::box_distance([0.0, 1.0, 0.0], [3.0, 0.0, -1.0], [4.0, 2.0, 1.0]),
        Some(3.0)
    );
    assert_eq!(
        policy::box_distance([3.5, 1.0, 0.0], [3.0, 0.0, -1.0], [4.0, 2.0, 1.0]),
        Some(0.0)
    );
    assert_eq!(policy::box_distance([0.0; 3], [2.0; 3], [1.0; 3]), None);
}

#[test]
fn tap_and_jump_requests_only_follow_actual_sprint_hits_and_knockback() {
    assert!(policy::should_tap(true, true, 1.0, true));
    assert!(!policy::should_tap(true, true, 0.9, true));
    assert!(!policy::should_tap(false, true, 1.0, true));
    assert!(!policy::should_tap(true, true, 1.0, false));
    assert!(policy::jump_knockback([0.2, 0.3, 0.0]));
    assert!(!policy::jump_knockback([0.2, 0.0, 0.0]));
    assert!(!policy::jump_knockback([0.0, 0.3, 0.0]));
    let mut pending = policy::Request::default();
    pending.start(10, 10);
    assert!(pending.active(20));
    assert!(!pending.active(21));
    assert!(!pending.active(10));
    assert!(policy::usable(1, 9, 10));
    assert!(!policy::usable(1, 10, 10));
    assert!(!policy::usable(0, 0, 0));
    assert_eq!(policy::POPULATION_LIMIT, 512);
}





#[test]
fn candidate_attack_speed_uses_its_own_item_and_native_effect_order() {
    let sword = policy::attack_speed(-2.4, []).unwrap();
    let axe = policy::attack_speed(-3.0, []).unwrap();
    let hand = policy::attack_speed(0.0, []).unwrap();
    assert!((sword - 1.6).abs() < 1e-12);
    assert_eq!(axe, 1.0);
    assert_eq!(hand, 4.0);
    assert!(20.0 / axe > 20.0 / hand);
    assert!((policy::attack_speed(-2.4, [0.1, -0.2]).unwrap() - 1.408).abs() < 1e-12);
    assert_eq!(policy::attack_speed(-10.0, []), Some(0.0));
    assert_eq!(policy::attack_speed(2048.0, []), Some(1024.0));
    assert_eq!(policy::attack_speed(f64::NAN, []), None);
    assert_eq!(policy::attack_speed(0.0, [f64::INFINITY]), None);
}
