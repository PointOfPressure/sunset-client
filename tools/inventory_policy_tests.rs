#[path="../mixins/sunset/inventory_policy.rs"] mod policy;
use policy::*;
#[derive(Clone,Debug,PartialEq)] struct Item { id:&'static str, count:u32, components:u64 }
fn totem()->Option<Item>{Some(Item{id:"totem",count:1,components:17})}
fn sword()->Option<Item>{Some(Item{id:"sword",count:1,components:91})}
fn swap()->Swap<Item>{Swap::new([17,OFFHAND],[totem(),sword()],100).unwrap()}

#[test] fn native_player_menu_and_inventory_packets_use_different_offhand_ids(){
    for i in 0..9 {assert_eq!(player_wire(i),Some(i+36));}
    for i in 9..36 {assert_eq!(player_wire(i),Some(i));}
    assert_eq!(player_wire(OFFHAND),Some(45));
    assert_eq!(player_internal(40),Some(OFFHAND));
    assert_eq!(player_internal(35),Some(35));
    for bad in [-1,36,39,41,504] {assert!(player_internal(bad).is_none());}
    for bad in [36,40,45,300,500] {assert!(player_wire(bad).is_none());}
}
#[test] fn prediction_is_not_confirmation_and_both_server_slots_are_required(){
    let mut s=swap();
    let client_prediction=s.expected.clone();
    assert_eq!(client_prediction,[sword(),totem()]);
    assert_eq!(s.status(101,30),Status::Waiting);
    s.observe(OFFHAND,&totem());
    s.observe(OFFHAND,&totem());
    s.observe(12,&None);
    assert_eq!(s.status(102,30),Status::Waiting);
    s.observe(17,&sword());
    assert_eq!(s.status(103,30),Status::Confirmed);
}
#[test] fn server_correction_or_component_difference_never_confirms_a_swap(){
    let mut s=swap(); s.observe(17,&totem());
    assert_eq!(s.status(101,30),Status::Rejected);
    let mut s=swap();
    let mut changed=totem().unwrap(); changed.components+=1;
    s.observe(OFFHAND,&Some(changed)); s.observe(17,&sword());
    assert_eq!(s.status(101,30),Status::Rejected);
}
#[test] fn cursor_or_menu_cancellation_overrides_completed_slot_acks(){
    let mut s=swap();s.observe(17,&sword());s.observe(OFFHAND,&totem());
    assert_eq!(s.confirmations(),3);s.reject();
    assert_eq!(s.status(101,30),Status::Rejected);
}
#[test] fn timeout_and_rewound_ticks_stop_retries_even_after_late_acks(){
    let mut s=swap(); assert_eq!(s.status(99,30),Status::Expired);
    assert_eq!(s.status(129,30),Status::Waiting);
    s.observe(17,&sword());s.observe(OFFHAND,&totem());
    assert_eq!(s.status(130,30),Status::Expired);
    assert!(Swap::new([1,1],[totem(),None],0).is_none());
    assert!(Swap::new([1,2],[None::<Item>,None],0).is_none());
    assert!(Swap::new([1,2],[totem(),totem()],0).is_none());
}
#[test] fn rejected_source_is_blocked_until_its_real_inventory_state_changes(){
    let f=Failure {slots:[17,OFFHAND],before:[totem(),sword()]};
    assert!(f.blocks([17,OFFHAND],&[totem(),sword()]));
    assert!(!f.blocks([17,OFFHAND],&[totem(),None]));
    let mut changed=sword().unwrap();changed.components=5;
    assert!(!f.blocks([17,OFFHAND],&[totem(),Some(changed)]));
}
#[test] fn key_throw_is_an_edge_and_cannot_repeat_a_held_binding(){
    let mut edge=Edge::default();assert!(edge.update(true));
    for _ in 0..100 {assert!(!edge.update(true));}
    assert!(!edge.update(false));assert!(edge.update(true));
}
#[test] fn double_hand_returns_after_ten_safe_ticks_and_preserves_the_original_slot(){
    let mut h=Hand::default();assert_eq!(h.tick(true,2,false,Some(8),true),Some(8));
    assert_eq!(h.held,Some((2,8)));
    for _ in 0..9 {assert_eq!(h.tick(false,8,true,Some(8),true),None);}
    assert_eq!(h.tick(false,8,true,Some(8),true),Some(2));
    assert_eq!(h.held,None);
}
#[test] fn double_hand_respects_manual_slot_changes_and_rearms_only_after_safety(){
    let mut h=Hand::default();assert_eq!(h.tick(true,1,false,Some(7),true),Some(7));
    assert_eq!(h.tick(true,3,false,Some(7),true),None);assert_eq!(h.held,None);
    assert_eq!(h.tick(true,3,false,Some(7),true),None);
    for _ in 0..10 {h.tick(false,3,false,Some(7),true);}
    assert_eq!(h.tick(true,3,false,Some(7),true),Some(7));
    assert_eq!(h.restore(4),None,"manual selection must never be overwritten");
}
#[test] fn popped_mainhand_can_take_one_next_hotbar_totem_without_losing_return_slot(){
    let mut h=Hand::default();assert_eq!(h.tick(true,2,false,Some(8),true),Some(8));
    assert_eq!(h.tick(true,8,false,Some(4),true),Some(4));
    assert_eq!(h.held,Some((2,4)));assert_eq!(h.restore(4),Some(2));
    assert_eq!(Hand::default().tick(true,2,false,Some(99),true),None);
    assert_eq!(Hand::default().tick(true,2,false,Some(8),false),None);
}
#[test] fn malformed_delay_inputs_never_shorten_the_safety_wait(){
    for bad in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,-1.0] {assert_eq!(delay(bad,20),20);}
    assert_eq!(delay(2.9,20),2);assert_eq!(delay(200.0,20),20);
}
