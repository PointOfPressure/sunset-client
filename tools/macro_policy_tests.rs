#[path = "../mixins/sunset/macro_policy.rs"]
mod policy;
use policy::{Kind, Sequence, Step, Wait};

#[test]
fn ghost_crystal_stops_after_three_sent_attacks_and_rests_for_one_hundred_ticks() {
    let mut r = policy::Retries::new();
    assert!(!r.allows(0, 0));
    for tick in [0, 30, 60] {
        assert!(r.allows(5, tick));
        r.sent(5, tick);
    }
    assert!(!r.allows(5, 61));
    assert!(!r.allows(5, 159));
    assert!(r.allows(5, 160));
    r.retain(|_| false);
    assert!(r.allows(5, 1));
}
#[test]
fn attack_bookkeeping_has_a_fixed_population_limit_and_session_reset() {
    let mut r = policy::Retries::new();
    for id in 1..=256 {
        r.sent(id, 0);
    }
    assert!(!r.allows(257, 0));
    r.clear();
    assert!(r.allows(257, 0));
}

#[test]
fn single_anchor_requires_observed_transactions_in_order() {
    let mut s = Sequence::new(Kind::Anchor, Step::AnchorPlace, false);
    assert!(!s.acknowledged(Step::Detonate));
    assert_eq!(s.step, Step::AnchorPlace);
    for step in [Step::AnchorPlace, Step::Guard, Step::Charge, Step::Detonate] {
        assert_eq!(s.step, step);
        assert!(s.acknowledged(step));
    }
    assert_eq!(s.step, Step::Done);
    assert!(!s.acknowledged(Step::Done));
}
#[test]
fn double_anchor_is_two_detonations_with_a_new_observed_anchor_between() {
    let mut s = Sequence::new(Kind::Anchor, Step::Guard, true);
    for step in [
        Step::Guard,
        Step::Charge,
        Step::Detonate,
        Step::AnchorPlace,
        Step::Guard,
        Step::Charge,
        Step::Detonate,
    ] {
        assert_eq!(s.step, step);
        assert!(s.acknowledged(step));
    }
    assert_eq!(s.remaining, 1);
    assert_eq!(s.step, Step::Done);
}
#[test]
fn crystal_cycle_never_inherits_double_anchor_semantics() {
    let mut s = Sequence::new(Kind::Crystal, Step::Base, true);
    assert_eq!(s.remaining, 1);
    for step in [Step::Base, Step::CrystalPlace, Step::CrystalBreak] {
        assert!(s.acknowledged(step));
    }
    assert_eq!(s.step, Step::Done);
}
#[test]
fn no_ack_means_no_progress_and_timeout_is_bounded() {
    let mut s = Sequence::new(Kind::Anchor, Step::Charge, false);
    for now in 10..40 {
        assert_eq!(policy::wait(10, now, false), Wait::Pending);
        assert_eq!(s.step, Step::Charge);
    }
    assert_eq!(policy::wait(10, 40, false), Wait::Expired);
    assert_eq!(policy::wait(10, 11, true), Wait::Acknowledged);
    assert!(s.acknowledged(Step::Charge));
    assert_eq!(s.step, Step::Detonate);
    assert_eq!(policy::wait(10, 9, true), Wait::Expired);
}
#[test]
fn restoration_never_overwrites_the_users_new_slot() {
    assert_eq!(policy::restore(1, 3, 3, true), Some(1));
    assert_eq!(policy::restore(1, 3, 4, true), None);
    assert_eq!(policy::restore(1, 3, 3, false), None);
    assert_eq!(policy::restore(1, 1, 1, true), None);
    assert_eq!(policy::restore(40, 3, 3, true), None);
}
#[test]
fn all_supported_click_faces_stop_the_visibility_ray_outside_the_support() {
    for (d, face, hit) in [
        ([0, -1, 0], 0, [0.5, 0., 0.5]),
        ([0, 1, 0], 1, [0.5, 1., 0.5]),
        ([0, 0, -1], 2, [0.5, 0.5, 0.]),
        ([0, 0, 1], 3, [0.5, 0.5, 1.]),
        ([-1, 0, 0], 4, [0., 0.5, 0.5]),
        ([1, 0, 0], 5, [1., 0.5, 0.5]),
    ] {
        assert_eq!(policy::face(d), Some(face));
        let p = policy::outside(hit, face).unwrap();
        assert!(p.iter().any(|n| *n < 0. || *n > 1.));
    }
    assert!(policy::face([1, 1, 0]).is_none());
    assert!(policy::outside([f64::NAN; 3], 1).is_none());
}
#[test]
fn manual_ray_respects_first_hitbox_contact_and_rejects_objects_behind_camera() {
    let lo = [3.5, 121., -0.5];
    let hi = [5.5, 123., 1.5];
    let eye = [0.6, 122.62, 0.5];
    assert_eq!(
        policy::ray(eye, [1., 0., 0.], lo, hi, 3.),
        Some([3.5, 122.62, 0.5])
    );
    assert!(policy::ray(eye, [-1., 0., 0.], lo, hi, 3.).is_none());
    assert!(policy::ray(eye, [0., 0., 0.], lo, hi, 3.).is_none());
    assert!(policy::ray([0., 0., 0.], [f64::INFINITY, 0., 0.], lo, hi, 3.).is_none());
}
