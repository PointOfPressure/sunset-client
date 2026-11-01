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
        Step::Charge,
        Step::Detonate,
    ] {
        assert_eq!(s.step, step);
        assert!(s.acknowledged(step));
    }
    assert_eq!(s.remaining, 1);
    assert_eq!(s.step, Step::Done);
}
#[test]fn double_refill_requires_top_face_and_feet_height_not_eye_height(){
    assert!(policy::double_eligible(true,1,121,121,true,true));
    for face in [0,2,3,4,5]{assert!(!policy::double_eligible(true,face,121,121,true,true));}
    assert!(!policy::double_eligible(true,1,120,121,true,true));
    assert!(!policy::double_eligible(true,1,121,121,false,true));
    assert!(!policy::double_eligible(true,1,121,121,true,false));
}
#[test]fn human_pace_has_the_observed_distribution_and_never_changes_configured_delay(){
    let mut histogram=[0;3];
    for sample in 0..100{histogram[(policy::human_gap(sample)-2) as usize]+=1;}
    assert_eq!(histogram,[70,25,5]);
    assert!(!policy::action_due(Some(10),13,4.,2));assert!(policy::action_due(Some(10),15,4.,2));
}
#[test]fn crystal_next_action_uses_its_own_delay_and_retains_age_across_replans(){
    assert!(!policy::action_due(Some(10),10,0.,0));assert!(policy::action_due(Some(10),11,0.,0));
    assert!(!policy::action_due(Some(10),11,0.,3));assert!(policy::action_due(Some(10),13,0.,3));
    assert!(!policy::action_due(Some(10),9,0.,0));assert!(!policy::action_due(None,10,f64::NAN,0));
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
        assert_eq!(policy::wait(10, now, false,30), Wait::Pending);
        assert_eq!(s.step, Step::Charge);
    }
    assert_eq!(policy::wait(10, 40, false,30), Wait::Expired);
    assert_eq!(policy::wait(10, 11, true,30), Wait::Acknowledged);
    assert!(s.acknowledged(Step::Charge));
    assert_eq!(s.step, Step::Detonate);
    assert_eq!(policy::wait(10, 9, true,30), Wait::Expired);
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

// Production routing and open-ray guard subset contracts.
use policy::{ClickFacts,UseRoute};
fn facts()->ClickFacts {ClickFacts{known:true,..Default::default()}}
fn flags(slots:&[usize])->[bool;9]{std::array::from_fn(|i|slots.contains(&i))}
#[test]fn already_main_item_wins_over_every_offhand_route(){
    assert_eq!(policy::placement_route(3,1,flags(&[3,0]),flags(&[1]),true),Some(UseRoute{select:None,offhand:false}));
}
#[test]fn permitted_current_main_keeps_offhand_and_does_not_reselect_matching_hotbar(){
    assert_eq!(policy::placement_route(3,1,flags(&[0]),flags(&[3]),true),Some(UseRoute{select:None,offhand:true}));
}
#[test]fn blocked_main_prefers_first_actual_hotbar_supply_to_a_pass_through_slot(){
    assert_eq!(policy::placement_route(7,1,flags(&[6,2]),flags(&[1]),true),Some(UseRoute{select:Some(2),offhand:false}));
}
#[test]fn offhand_only_supply_reselects_saved_original_before_other_empty_slots(){
    assert_eq!(policy::placement_route(7,6,flags(&[]),flags(&[0,6]),true),Some(UseRoute{select:Some(6),offhand:true}));
}
#[test]fn unusable_saved_original_falls_back_to_first_permitted_main(){
    assert_eq!(policy::placement_route(7,6,flags(&[]),flags(&[4,2]),true),Some(UseRoute{select:Some(2),offhand:true}));
    assert_eq!(policy::placement_route(7,99,flags(&[]),flags(&[4]),true),Some(UseRoute{select:Some(4),offhand:true}));
}
#[test]fn absent_supply_or_permitted_main_does_not_create_route(){
    assert!(policy::placement_route(0,0,flags(&[]),flags(&[1]),false).is_none());
    assert!(policy::placement_route(0,0,flags(&[]),flags(&[]),true).is_none());
    assert!(policy::placement_route(99,0,flags(&[0]),flags(&[0]),true).is_none());
}
#[test]fn axes_only_pass_on_obsidian_or_bedrock(){
    let f=ClickFacts{axe:true,..facts()};
    assert!(policy::click_through(f,true));assert!(!policy::click_through(f,false));
}
#[test]fn ordinary_sword_mace_totem_pass_on_both_support_types(){
    for f in [ClickFacts{sword:true,..facts()},ClickFacts{mace:true,..facts()},ClickFacts{totem:true,..facts()}]{
        assert!(policy::click_through(f,false));assert!(policy::click_through(f,true));
    }
}
#[test]fn consumption_shield_lunge_or_swappable_equipment_overrides_combat_identity(){
    let f=ClickFacts{sword:true,..facts()};
    for bad in [ClickFacts{consuming:true,..f},ClickFacts{blocking:true,..f},ClickFacts{kinetic:true,..f},ClickFacts{equip_swap:true,..f}]{
        assert!(!policy::click_through(bad,true));
    }
}
#[test]fn empty_passes_unknown_custom_stack_and_unrelated_item_do_not(){
    assert!(policy::click_through(ClickFacts{empty:true,..Default::default()},false));
    assert!(!policy::click_through(ClickFacts{sword:true,..Default::default()},true));
    assert!(!policy::click_through(facts(),true));
}
#[test]fn charged_anchor_still_requires_a_detonator_without_requiring_more_glowstone(){
    assert!(policy::anchor_supplied(true,false,true));
    assert!(!policy::anchor_supplied(true,true,false));
    assert!(!policy::anchor_supplied(true,false,false));
}
#[test]fn new_and_uncharged_anchors_require_both_supplies_before_start(){
    assert!(policy::anchor_supplied(false,true,true));
    assert!(!policy::anchor_supplied(false,false,true));
    assert!(!policy::anchor_supplied(false,true,false));
}

#[test]fn already_covered_cutoff_uses_strict_fifteen_percent(){
    assert!(policy::guard_already_covered(72,10));assert!(!policy::guard_already_covered(72,11));
    assert!(policy::guard_already_covered(20,2));assert!(!policy::guard_already_covered(20,3));
}
#[test]fn candidate_shade_counts_only_rays_currently_open(){
    assert_eq!(policy::guard_shade_needed(11),Some(2));
    assert_eq!(policy::guard_shade_needed(72),Some(11));
    // A cube closing two of eleven open rays is useful despite the larger
    // complete body sample count. This case rejected by the previous subset.
    assert!(2>=policy::guard_shade_needed(11).unwrap());
    assert!(2<policy::guard_shade_needed(72).unwrap());
}
#[test]fn empty_or_unbounded_sample_sets_cannot_establish_cover(){
    assert!(!policy::guard_already_covered(0,0));assert!(!policy::guard_already_covered(257,0));
    assert!(!policy::guard_already_covered(10,11));
    assert!(policy::guard_shade_needed(0).is_none());assert!(policy::guard_shade_needed(257).is_none());
}
#[test]fn no_open_rays_means_existing_cover_without_any_candidate(){
    assert!(policy::guard_already_covered(72,0));assert!(policy::guard_shade_needed(0).is_none());
}
#[test]fn sparse_boundary_still_requires_one_positive_intersection(){
    assert_eq!(policy::guard_shade_needed(1),Some(1));assert_eq!(policy::guard_shade_needed(6),Some(1));
    assert_eq!(policy::guard_shade_needed(7),Some(2));
}
#[test]fn maximum_native_grid_needs_thirty_nine_open_ray_intersections(){
    assert_eq!(policy::guard_shade_needed(256),Some(39));
    assert!(policy::guard_already_covered(256,38));assert!(!policy::guard_already_covered(256,39));
}
