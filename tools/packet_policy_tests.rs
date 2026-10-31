#[path="../mixins/sunset/packet_policy.rs"]mod policy;
use policy::*;

#[test]fn due_times_never_reorder_newer_impulses(){
    let mut q=Queue::new(16,1000,1000);
    q.push("first",100,250,10).unwrap();q.push("second",150,0,10).unwrap();
    assert!(q.drain(349,false).is_empty());assert_eq!(q.drain(350,false),["first","second"]);
    assert_eq!((q.accepted,q.released,q.bytes()),(2,2,0));
}
#[test]fn count_and_byte_caps_are_absolute_and_fail_without_stealing_packets(){
    let mut q=Queue::new(2,20,1000);
    q.push(1,0,100,10).unwrap();q.push(2,0,100,10).unwrap();
    assert_eq!(q.push(3,0,100,1),Err(3));assert_eq!(q.len(),2);
    assert_eq!(q.drain(0,true),[1,2]);assert_eq!(q.bytes(),0);
    assert_eq!(q.push(4,0,0,21),Err(4));assert!(q.is_empty());
    assert!(Queue::new(0,20,1000).push(1,0,0,1).is_err());
}
#[test]fn age_ceiling_flushes_a_backlog_even_if_later_packets_are_not_due(){
    let mut q=Queue::new(64,4096,1000);
    q.push(1,0,1000,1).unwrap();q.push(2,999,1000,1).unwrap();
    assert_eq!(q.oldest_age(999),999);assert_eq!(q.drain(1000,false),[1,2]);
    assert_eq!(q.oldest_age(1000),0);
}
#[test]fn clock_rewind_and_disconnect_clear_do_not_replay_into_a_new_epoch(){
    let mut q=Queue::new(16,1000,1000);q.push(1,500,100,1).unwrap();
    assert_eq!(q.drain(499,false),[1]);
    q.push(2,600,100,1).unwrap();q.clear();
    assert!(q.drain(u64::MAX,true).is_empty());assert_eq!(q.dropped,1);
}
#[test]fn correction_discards_queued_coordinates_without_forwarding_them(){
    let mut q=Queue::new(64,4096,1000);
    q.push([5.0,70.0,2.0],0,500,40).unwrap();
    q.push([6.0,70.0,2.0],50,500,40).unwrap();
    q.clear();
    assert!(q.drain(5000,true).is_empty());
    assert_eq!((q.released,q.dropped),(0,2));
}
#[test]fn delay_and_queue_limits_saturate_at_explicit_hard_ceilings(){
    let mut q=Queue::new(usize::MAX,usize::MAX,u64::MAX);
    for i in 0..64 {q.push(i,10,u64::MAX,1).unwrap();}
    assert!(q.push(99,10,1,1).is_err());
    assert!(q.drain(1009,false).is_empty());assert_eq!(q.drain(1010,false).len(),64);
    let mut q=Queue::new(1,10,1000);q.push(7,u64::MAX,u64::MAX,1).unwrap();
    assert_eq!(q.drain(u64::MAX,false),[7]);
}
#[test]fn chance_setting_has_exact_boundaries_and_no_nonfinite_enablement(){
    for invalid in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,-1.0,0.0] {
        assert!(!(0..10_000).any(|n|probability(invalid,n)));
    }
    assert_eq!((0..10_000).filter(|n|probability(40.0,*n)).count(),4000);
    assert_eq!((0..10_000).filter(|n|probability(100.0,*n)).count(),10_000);
}
#[test]fn reversed_delay_settings_are_inclusive_finite_and_bounded(){
    for n in 0..1000 {
        assert_eq!(uniform_between(100.0,50.0,n),uniform_between(50.0,100.0,n));
        assert!((50..=100).contains(&uniform_between(100.0,50.0,n)));
    }
    assert_eq!(uniform_between(10.0,10.0,1000),10);
    assert_eq!(uniform_between(f64::NAN,f64::INFINITY,12),0);
    assert_eq!(bounded(99999.0,10,1000),1000);
}
#[test]fn dynamic_lag_requires_approach_aim_range_and_finite_vectors(){
    let eye=[0.0;3];let look=[0.0,0.0,1.0];let towards=[0.0,0.0,0.1];
    assert!(approaching(eye,look,towards,[0.0,0.0,5.0],5.0,6.0));
    assert!(!approaching(eye,look,[0.0,0.0,-0.1],[0.0,0.0,5.0],5.0,6.0));
    assert!(!approaching(eye,look,[0.0,0.0,0.02],[0.0,0.0,5.0],5.0,6.0));
    assert!(!approaching(eye,look,towards,[0.0,0.0,7.0],7.0,6.0));
    assert!(!approaching(eye,look,towards,[5.0,0.0,0.0],5.0,6.0));
    assert!(!approaching(eye,look,towards,[f64::NAN,0.0,0.0],5.0,6.0));
    assert!(!approaching(eye,look,towards,eye,0.0,6.0));
    for angle in [29.9f64,30.1] {
        let a=angle.to_radians();
        assert_eq!(approaching(eye,look,[0.0,0.0,0.1],[a.sin()*5.0,0.0,a.cos()*5.0],5.0,6.0),angle<30.0);
    }
}
