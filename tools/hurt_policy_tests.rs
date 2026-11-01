#[path="../mixins/sunset/hurt_policy.rs"]mod policy;
use policy::*;
fn clock(tick:u64)->Clock {let mut clock=Clock::new();clock.tick(1,tick,|_|true);clock}
fn damage(victim:u32,kind:DamageKind,cause:Option<u32>,direct:Option<u32>)->DamageNotice {
    DamageNotice{victim,enemy_player:true,kind,cause,direct,position:None}
}
#[test]fn cooldown_boundary_uses_raw_delta_before_nonlinear_armor(){
    let mut c=clock(100);c.sent(2,10.0,ShotSource::Crystal(30),1);
    assert_eq!(c.assess(2,8.0,0,1).raw_for_reduction,0.0);
    assert_eq!(c.assess(2,15.0,0,1).raw_for_reduction,5.0);
    c.tick(1,109,|_|true);assert_eq!(c.opens_in(2,1),1);
    c.tick(1,110,|_|true);assert_eq!(c.assess(2,15.0,0,1).raw_for_reduction,15.0);
}
#[test]fn real_damage_event_reconstructs_twenty_tick_cooldown_not_hurt_flash(){
    let mut c=clock(10);c.damaged(1,damage(2,DamageKind::Other,None,None));
    assert_eq!(c.opens_in(2,0),10);assert_eq!(c.opens_in(2,3),7);
    assert_eq!(c.assess(2,30.0,0,3).basis,Basis::UnknownPreviousDamage);
    assert_eq!(c.assess(2,30.0,7,3).basis,Basis::FreshWindow);
}
#[test]fn matching_requires_type_owner_and_direct_crystal_even_for_entity_zero(){
    for notice in [damage(2,DamageKind::Other,Some(0),Some(30)),damage(2,DamageKind::PlayerExplosion,Some(9),Some(30)),damage(2,DamageKind::PlayerExplosion,Some(0),None)] {
        let mut c=clock(10);c.sent(2,10.0,ShotSource::Crystal(30),1);c.damaged(0,notice);assert_eq!(c.counters().confirmed,0);
    }
    let mut c=clock(10);c.sent(2,10.0,ShotSource::Crystal(30),1);c.damaged(0,damage(2,DamageKind::PlayerExplosion,Some(0),Some(30)));
    assert_eq!(c.counters().confirmed,1);
}
#[test]fn anchor_match_is_strict_half_block_radius_and_requires_source_position(){
    let mut c=clock(10);c.sent(2,10.0,ShotSource::Anchor([3.5,121.5,0.5]),1);
    let mut notice=damage(2,DamageKind::AnchorExplosion,None,None);notice.position=Some([4.0,121.5,0.5]);
    c.damaged(1,notice);assert_eq!(c.counters().confirmed,0);
    let mut c=clock(10);c.sent(2,10.0,ShotSource::Anchor([3.5,121.5,0.5]),1);
    notice.position=Some([3.999,121.5,0.5]);c.damaged(1,notice);assert_eq!(c.counters().confirmed,1);
}
#[test]fn removal_is_not_damage_confirmation_and_grace_is_three_full_ticks(){
    let mut c=clock(10);c.sent(2,10.0,ShotSource::Crystal(30),1);c.removed(&[30]);
    c.tick(1,13,|_|true);assert_eq!(c.margin(),0);assert_eq!(c.counters().confirmed,0);
    c.tick(1,14,|_|true);assert_eq!(c.margin(),1);assert_eq!(c.counters().unconfirmed_removals,1);
}
#[test]fn late_confirmation_within_grace_prevents_margin_penalty(){
    let mut c=clock(10);c.sent(2,10.0,ShotSource::Crystal(30),1);c.removed(&[30]);
    c.tick(1,13,|_|true);c.damaged(1,damage(2,DamageKind::PlayerExplosion,Some(1),Some(30)));
    c.tick(1,14,|_|true);assert_eq!(c.margin(),0);
}
#[test]fn adaptive_margin_caps_two_and_eight_crystal_confirmations_relax_one(){
    let mut c=clock(0);
    for i in 0..3 {let tick=i*30;c.tick(1,tick,|_|true);c.sent(2,10.0,ShotSource::Crystal(30),0);c.removed(&[30]);c.tick(1,tick+4,|_|true);}
    assert_eq!(c.margin(),2);
    for i in 0..8 {c.tick(1,100+i*30,|_|true);c.sent(2,10.0,ShotSource::Crystal(30),0);c.damaged(1,damage(2,DamageKind::PlayerExplosion,Some(1),Some(30)));}
    assert_eq!(c.margin(),1);
}
#[test]fn melee_zero_baseline_is_labeled_assumption_not_measured_damage(){
    let mut c=clock(0);c.damaged(1,damage(2,DamageKind::PlayerMelee,Some(9),Some(9)));
    let result=c.assess(2,8.0,0,0);assert_eq!(result.raw_for_reduction,8.0);assert_eq!(result.basis,Basis::MeleeBaselineAssumption);
}
#[test]fn non_enemy_player_cannot_confirm_but_real_cooldown_is_observed(){
    let mut c=clock(0);c.sent(2,8.0,ShotSource::Crystal(30),0);
    let mut event=damage(2,DamageKind::PlayerExplosion,Some(1),Some(30));event.enemy_player=false;c.damaged(1,event);
    assert_eq!(c.counters().confirmed,0);assert_eq!(c.opens_in(2,0),10);
}
#[test]fn sent_closed_window_preserves_first_confirmation_key_and_peak_estimate(){
    let mut c=clock(0);c.sent(2,8.0,ShotSource::Crystal(30),0);c.sent(2,12.0,ShotSource::Crystal(31),0);
    assert_eq!(c.assess(2,14.0,0,0).raw_for_reduction,2.0);
    c.damaged(1,damage(2,DamageKind::PlayerExplosion,Some(1),Some(30)));
    // Observed reference policy confirms the original shot's predicted raw;
    // this does not prove the speculative stronger shot hit the server.
    assert_eq!(c.assess(2,14.0,0,0).raw_for_reduction,6.0);
}
#[test]fn bounded_cap_no_silent_eviction_and_dead_targets_prune(){
    let mut c=clock(0);for id in 0..TARGET_CAP as u32 {assert!(c.sent(id,1.0,ShotSource::Crystal(30),0));}
    assert!(!c.sent(999,1.0,ShotSource::Crystal(30),0));assert_eq!(c.len(),TARGET_CAP);
    assert_eq!(c.assess(999,10.0,0,0).basis,Basis::Capacity);c.tick(1,1,|id|id!=0);assert_eq!(c.len(),TARGET_CAP-1);
}
#[test]fn scene_reset_and_clock_rewind_have_explicit_behavior(){
    let mut c=clock(100);c.sent(2,1.0,ShotSource::Crystal(30),0);c.tick(1,0,|_|true);
    assert_eq!(c.assess(2,10.0,0,0).basis,Basis::ClockRewind);c.tick(2,0,|_|true);assert_eq!(c.assess(2,10.0,0,0).basis,Basis::FreshWindow);
}
#[test]fn invalid_raw_and_anchor_positions_never_become_target_credit(){
    let mut c=clock(0);for raw in [f32::NAN,f32::INFINITY,-1.0] {assert!(!c.sent(2,raw,ShotSource::Crystal(30),0));assert_eq!(c.assess(2,raw,0,0).basis,Basis::Invalid);}
    assert!(!c.sent(2,1.0,ShotSource::Anchor([f64::NAN,0.0,0.0]),0));
    let mut uninitialized=Clock::new();assert_eq!(uninitialized.assess(2,10.0,0,0).basis,Basis::Invalid);
    assert!(!uninitialized.sent(2,1.0,ShotSource::Crystal(30),0));
}
#[test]fn receipt_rtt_uses_only_real_matched_spawn_and_confirmation_window_bounds(){
    let mut timing=ReceiptTiming::new();assert_eq!(timing.rtt_ticks(0),1);assert_eq!(timing.confirm_window(0),4);
    assert_eq!(timing.rtt_ticks(99),2);assert_eq!(timing.confirm_window(i32::MAX),20);
    timing.placed([1,2,3],10);assert!(!timing.spawned([9,2,3],12));assert!(timing.spawned([1,2,3],14));assert_eq!(timing.rtt_ticks(999),4);
    timing.placed([1,2,3],20);timing.spawned([1,2,3],28);assert_eq!(timing.rtt_ticks(0),5);
}
#[test]fn receipt_bound_stale_zero_age_and_reset_are_not_fake_rtt_samples(){
    let mut timing=ReceiptTiming::new();for x in 0..64 {assert!(timing.placed([x,0,0],0));}assert!(!timing.placed([64,0,0],0));
    assert!(!timing.spawned([0,0,0],0));assert!(!timing.spawned([1,0,0],21));assert_eq!(timing.rtt_ticks(0),1);
    timing.reset();assert!(timing.placed([64,0,0],0));
}
