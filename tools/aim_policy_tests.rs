#[path = "../mixins/sunset/aim_policy.rs"] mod policy;
use policy::*;

fn candidate(id:u32,distance:f64,angle:f64)->Candidate { Candidate{id,distance,angle} }
#[test]fn recent_hit_precedes_a_smaller_crosshair_angle(){
    assert_eq!(choose(&[candidate(1,3.0,25.0),candidate(2,2.0,1.0)],4.0,60.0,Some(1),None),Some(1));
}
#[test]fn stickiness_expands_only_the_kept_target(){
    let c=[candidate(1,4.5,39.0),candidate(2,3.0,3.0)];
    assert_eq!(choose(&c,4.0,60.0,None,Some(1)),Some(1));
    assert_eq!(choose(&c,4.0,60.0,Some(1),None),Some(2));
    assert_eq!(choose(&[candidate(1,4.50001,39.0)],4.0,60.0,None,Some(1)),None);
}
#[test]fn nearest_angle_not_nearest_distance_and_last_tie_wins(){
    assert_eq!(choose(&[candidate(1,1.0,10.0),candidate(2,4.0,2.0),candidate(3,2.0,2.0)],4.0,60.0,None,None),Some(3));
}
#[test]fn invalid_candidates_never_win(){
    assert_eq!(choose(&[candidate(1,f64::NAN,0.0),candidate(2,1.0,f64::INFINITY)],4.0,60.0,None,None),None);
}
fn controller()->Easing { let mut e=Easing::default();e.follow(Some(1),73);e }
#[test]fn no_recent_manual_motion_means_no_assistance(){
    let mut e=controller();assert_eq!(e.nudge(1000,[0.0;2],Some([40.0;2]),0.05,60.0,true,0.15),[0.0;2]);
}
#[test]fn input_expires_after_150_ms_even_when_a_target_moves(){
    let mut e=controller();assert!(e.nudge(1000,[1.0;2],Some([40.0;2]),0.05,60.0,true,0.15)[0]>0.0);
    assert_eq!(e.nudge(1151,[0.0;2],Some([40.0;2]),0.05,60.0,true,0.15),[0.0;2]);
}
#[test]fn it_never_pulls_against_recent_input(){
    let mut e=controller();assert_eq!(e.nudge(1000,[-1.0,-1.0],Some([40.0;2]),0.05,60.0,true,0.15),[0.0;2]);
}
#[test]fn vertical_switch_disables_pitch_but_preserves_yaw(){
    let out=controller().nudge(1000,[1.0;2],Some([40.0;2]),0.05,60.0,false,0.15);
    assert!(out[0]>0.0);assert_eq!(out[1],0.0);
}
#[test]fn whole_mouse_counts_accumulate_subcount_motion(){
    let mut e=controller();let mut moved=0.0;
    for t in 0..100 { let out=e.nudge(t,[0.01;2],Some([0.2,0.2]),0.01,60.0,true,0.15);assert!((out[0]/0.15).fract().abs()<1e-9);moved+=out[0]; }
    assert!(moved>0.0);
}
#[test]fn frame_time_is_capped_before_pace_and_speed_limit(){
    let a=controller().nudge(1000,[1.0;2],Some([100.0;2]),0.05,60.0,true,0.15);
    let b=controller().nudge(1000,[1.0;2],Some([100.0;2]),10.0,60.0,true,0.15);
    assert_eq!(a,b);assert!(a[0]<=60.0*1.15*0.05);
}
#[test]fn a_crosshair_already_in_the_core_clears_residual_motion(){
    let mut e=controller();e.nudge(1000,[1.0;2],Some([0.1;2]),0.01,60.0,true,0.15);
    assert_eq!(e.nudge(1001,[0.0;2],None,0.01,60.0,true,0.15),[0.0;2]);
}
#[test]fn target_and_clock_reset_remove_old_motion(){
    let mut e=controller();e.nudge(1000,[1.0;2],Some([40.0;2]),0.01,60.0,true,0.15);e.reset();e.follow(Some(2),99);
    assert_eq!(e.nudge(1100,[0.0;2],Some([40.0;2]),0.01,60.0,true,0.15),[0.0;2]);
}
