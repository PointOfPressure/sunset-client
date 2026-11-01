#[path="../mixins/sunset/crystal_setup_policy.rs"] mod setup;
use setup::*;
const P:[i32;3]=[4,120,0];
#[test] fn matching_payload_and_world_are_both_required() {
    let mut l=Ledger::new(); assert!(l.sent(P,0,10,2));
    assert!(!l.crystal_allowed(P,12,true)); // client prediction alone
    l.payload([5,120,0],49,true); assert!(!l.crystal_allowed(P,12,true));
    l.payload(P,49,true); assert!(!l.acknowledged(P,false));
    assert!(l.acknowledged(P,true)); assert!(l.crystal_allowed(P,12,true));
}
#[test] fn delayed_setup_cell_does_not_delay_an_existing_base() {
    let mut l=Ledger::new(); l.sent(P,0,10,20); l.payload(P,49,true);
    assert!(l.acknowledged(P,true)); assert!(!l.crystal_allowed(P,29,true));
    assert!(l.crystal_allowed(P,30,true)); assert!(l.crystal_allowed([3,120,0],11,true));
}
#[test] fn three_refusals_stay_bounded_under_repeated_air_payloads() {
    let mut l=Ledger::new();
    for tick in [10,40,70] {
        assert!(l.sent(P,0,tick,2));
        for _ in 0..100 {l.payload(P,0,false);}
        assert!(!l.can_send(P,tick+29));
    }
    assert!(!l.can_send(P,100)); assert!(!l.sent(P,0,10000,2));
    assert_eq!(l.len(),1);
}
#[test] fn distinct_server_state_change_reopens_a_suppressed_cell() {
    let mut l=Ledger::new(); for tick in [0,30,60] {assert!(l.sent(P,0,tick,2));}
    assert!(!l.can_send(P,90)); l.payload(P,7,false); assert!(l.can_send(P,90));
}
#[test] fn late_real_payload_can_confirm_a_timed_out_attempt() {
    let mut l=Ledger::new(); assert!(l.sent(P,0,0,2));
    assert!(l.can_send(P,30)); l.payload(P,49,true);
    assert!(l.acknowledged(P,true)); assert!(l.crystal_allowed(P,31,true));
}
#[test] fn removing_a_confirmed_base_revokes_crystal_authorization() {
    let mut l=Ledger::new(); l.sent(P,0,0,2); l.payload(P,49,true);
    assert!(l.acknowledged(P,true)); l.payload(P,0,false);
    assert!(!l.crystal_allowed(P,40,true)); assert!(l.can_send(P,40));
}
#[test] fn capacity_is_hard_and_expiry_does_not_evict_suppression() {
    let mut l=Ledger::new(); for n in 0..64 {assert!(l.sent([n,120,0],0,0,2));}
    assert_eq!(l.len(),64); assert!(!l.sent([64,120,0],0,100000,2));
    assert!(l.sent([0,120,0],0,100000,2)); assert_eq!(l.len(),64);
}
#[test] fn explicit_context_reset_clears_pending_and_capacity() {
    let mut l=Ledger::new(); l.sent(P,0,10,2); l.clear();
    assert_eq!(l.len(),0); assert!(l.can_send(P,10)); assert!(l.crystal_allowed(P,10,true));
}
#[test] fn rewound_time_never_expires_a_future_pending_send() {
    let mut l=Ledger::new(); l.sent(P,0,100,2); assert!(!l.can_send(P,0));
}
#[test] fn section_payload_preserves_negative_coordinates_and_rejects_overflow() {
    let change=(49i64<<12)|(15<<8)|(2<<4)|7;
    assert_eq!(unpack_section([-1,-4,2],change),Some(([-1,-57,34],49)));
    assert!(unpack_section([i32::MAX,0,0],change).is_none());
    assert!(unpack_section([0,0,0],-1).is_none());
}
