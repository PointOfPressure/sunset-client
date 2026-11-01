#[path="../mixins/sunset/loot_policy.rs"]mod policy;
use policy::*;
#[test]fn recent_death_uses_full_blast_radius_and_disclosed_drop_margin(){
 let mut g=Guard::new();g.record(0,12,[0.,121.,0.]);
 assert_eq!(g.check(1,[14.,121.,0.],6.),Reason::NearDeath);
 assert_eq!(g.check(2,[14.001,121.,0.],6.),Reason::Clear);
 assert_eq!(g.check(3,[12.,121.,0.],5.),Reason::NearDeath);
 assert_eq!(g.check(4,[12.001,121.,0.],5.),Reason::Clear);
}
#[test]fn expiry_and_explicit_clear_do_not_need_a_tick_hook(){
 let mut g=Guard::new();g.record(100,1,[1.,2.,3.]);
 assert_eq!(g.check(30_099,[1.,2.,3.],6.),Reason::NearDeath);
 assert_eq!(g.check(30_100,[1.,2.,3.],6.),Reason::Clear);
 assert_eq!(g.active(),0);g.record(30_101,1,[1.,2.,3.]);g.clear();
 assert_eq!(g.check(30_102,[1.,2.,3.],6.),Reason::Clear);
}
#[test]fn cap_overflow_never_silently_evicts_an_unexpired_death(){
 let mut g=Guard::new();for i in 0..CAPACITY{g.record(0,i as i32,[i as f64*100.,0.,0.]);}
 assert_eq!(g.active(),16);g.record(29_000,99,[50_000.,0.,0.]);
 assert_eq!(g.check(31_000,[29_000.,0.,0.],6.),Reason::Overflow);
 assert_eq!(g.check(58_999,[29_000.,0.,0.],6.),Reason::Overflow);
 assert_eq!(g.check(59_000,[29_000.,0.,0.],6.),Reason::Clear);
}
#[test]fn repeated_notification_updates_one_cell_without_losing_other_places(){
 let mut g=Guard::new();g.record(0,3,[0.;3]);g.record(100,3,[0.;3]);assert_eq!(g.active(),1);
 g.record(101,3,[100.,0.,0.]);assert_eq!(g.active(),2);
 assert_eq!(g.check(30_099,[0.;3],6.),Reason::NearDeath);
 assert_eq!(g.check(30_100,[0.;3],6.),Reason::Clear);
}
#[test]fn missing_or_invalid_known_player_position_temporarily_refuses_all_blasts(){
 let mut g=Guard::new();g.record(0,1,[f64::NAN,0.,0.]);
 assert_eq!(g.check(1,[999_999.,0.,0.],6.),Reason::Overflow);
 g.clear();g.unknown_death(10);assert_eq!(g.check(11,[0.;3],5.),Reason::Overflow);
}
#[test]fn invalid_blasts_clock_rewind_and_saturation_fail_closed(){
 let mut g=Guard::new();assert_eq!(g.check(1,[f64::INFINITY,0.,0.],6.),Reason::InvalidBlast);
 for power in [0.,-1.,f32::NAN,17.]{assert_eq!(g.check(2,[0.;3],power),Reason::InvalidBlast);}
 g.record(100,1,[0.;3]);assert_eq!(g.check(99,[100.;3],6.),Reason::ClockRewind);
 g.record(99,2,[100.;3]);assert_eq!(g.check(101,[1_000.;3],6.),Reason::Overflow);
 g.clear();g.record(u64::MAX-1,1,[0.;3]);assert_eq!(g.check(u64::MAX,[0.;3],6.),Reason::NearDeath);
}
#[test]fn full_dimension_keys_are_exact_bounded_and_do_not_alias_custom_dimensions(){
 assert!(Dimension::of("").is_none());assert!(Dimension::of(&"x".repeat(257)).is_none());
 assert!(Dimension::of("example:a")!=Dimension::of("example:b"));
 assert!(Dimension::of("minecraft:overworld")==Dimension::of("minecraft:overworld"));
 assert!(std::mem::size_of::<Guard>()<=1024);
}
