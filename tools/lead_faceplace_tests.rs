#![allow(dead_code)]
#[path="../mixins/sunset/faceplace_policy.rs"] mod sunset_faceplace_policy;
use sunset_faceplace_policy as faceplace_policy;
#[path="../mixins/sunset/lead_policy.rs"] mod sunset_lead_policy;
use sunset_lead_policy as lead_policy;
#[path="../mixins/sunset/prediction_native.rs"] mod sunset_prediction_native;
use sunset_prediction_native as prediction_native;
#[path="../mixins/sunset/durability_patch.rs"] mod sunset_durability_patch;
use sunset_durability_patch as durability_patch;
use faceplace_policy as face;
use lead_policy::*;
use prediction_native::*;
use horizon_core::{geometry::{Aabb,Vec3,VoxelShape},position::{BlockPos,ChunkPos}};
use horizon_world::{block_collision_scan::{BlockCollisionWorld,CollisionBlock},
    collision_context::CollisionContext,entity_obstruction::EntityIdentity,
    registry::BlockStateId,world_movement::EntityMovementWorld};
use std::io;

fn low_hp(hp:f64)->face::FaceFacts{face::FaceFacts{effective_health:Some(hp),..Default::default()}}
fn prototype()->durability_patch::Prototype{durability_patch::Prototype{
 maximum:Some(100),damage_present:true,unbreakable:false}}
#[test]fn native_wire_patch_unbreakable_and_removal_resolve_damageability(){
 assert!(!durability_patch::resolve(prototype(),99,Some(&[1,0,4,0])).unwrap().low());
 let mut base=prototype();base.unbreakable=true;
 assert!(durability_patch::resolve(base,99,Some(&[0,1,4])).unwrap().low());
}
#[test]fn native_wire_patch_maximum_and_damage_override_are_read_only(){
 // 2 added,0 removed,maxDamage 200(varint c8 01),damage190(be 01).
 let p=[2,0,2,2,200,1,3,2,190,1];
 let result=durability_patch::resolve(prototype(),0,Some(&p)).unwrap();
 assert_eq!((result.maximum,result.lost),(200,190));assert!(result.low());
 assert!(!durability_patch::resolve(prototype(),99,Some(&[0,1,3])).unwrap().low());
 assert!(!durability_patch::resolve(prototype(),99,Some(&[0,1,2])).unwrap().low());
}
#[test]fn native_wire_patch_unknown_non_durability_components_are_skipped_by_length(){
 assert!(durability_patch::resolve(prototype(),95,Some(&[1,0,100,3,9,8,7])).unwrap().low());
}
#[test]fn native_wire_patch_malformed_duplicate_and_oversize_are_unknown(){
 for patch in [&[1,0,4,1,1][..],&[1,0,2,1,128][..],&[0,0,1][..],&[1,1,4,0,4][..],&[65,0][..]]{
 assert!(durability_patch::resolve(prototype(),99,Some(patch)).is_none());}
 assert!(durability_patch::resolve(prototype(),99,Some(&vec![0;8193])).is_none());
}
#[test]fn normal_minimum_and_every_break_factor(){
 for enabled in [false,true]{let facts=low_hp(20.0);
 assert_eq!(face::crystal(4.0,enabled,false,false,&facts).unwrap().minimum,4.0);
 assert_eq!(face::crystal(4.0,enabled,false,true,&facts).unwrap().minimum,2.4);}
}
#[test]fn health_absorption_and_inclusive_boundary(){
 assert_eq!(face::crystal(4.0,true,false,false,&low_hp(10.0)).unwrap().reason,face::Reason::LowHealth);
 assert_eq!(face::crystal(4.0,true,false,false,&low_hp(10.01)).unwrap().reason,face::Reason::Normal);
 // Bridge must supply health+absorption, not health alone.
 assert_eq!(face::crystal(4.0,true,false,true,&low_hp(9.0+4.0)).unwrap().minimum,2.4);
}
#[test]fn forced_works_when_faceplace_disabled_and_never_raises_minimum(){
 let f=low_hp(20.0);
 assert_eq!(face::crystal(6.0,false,true,true,&f).unwrap(),face::Threshold{minimum:1.2,reason:face::Reason::Forced});
 assert_eq!(face::crystal(1.0,false,true,false,&f).unwrap().minimum,1.0);
}
#[test]fn hole_is_exactly_four_horizontal_neighbors(){
 let mut f=low_hp(20.0);f.neighbor_resistance=[Some(600.0);4];
 assert_eq!(face::crystal(4.0,true,false,false,&f).unwrap().reason,face::Reason::Hole);
 f.neighbor_resistance[3]=Some(599.99);assert_eq!(face::crystal(4.0,true,false,false,&f).unwrap().reason,face::Reason::Normal);
 f.neighbor_resistance[3]=None;assert_eq!(face::crystal(4.0,true,false,false,&f).unwrap().reason,face::Reason::Normal);
}
#[test]fn armor_low_inclusive_and_overdamage_saturates(){
 assert!(face::ArmorCondition{damageable:true,maximum:100,lost:90}.low());
 assert!(!face::ArmorCondition{damageable:true,maximum:100,lost:89}.low());
 assert!(face::ArmorCondition{damageable:true,maximum:100,lost:101}.low());
 let mut f=low_hp(20.0);f.armor[2]=Some(face::ArmorCondition{damageable:true,maximum:100,lost:90});
 assert_eq!(face::crystal(4.0,true,false,false,&f).unwrap().reason,face::Reason::LowArmor);
}
#[test]fn missing_unbreakable_and_zero_durability_do_not_trigger(){
 assert!(!face::ArmorCondition{damageable:false,maximum:100,lost:99}.low());
 assert!(!face::ArmorCondition{damageable:true,maximum:0,lost:0}.low());
 assert_eq!(face::crystal(4.0,true,false,false,&Default::default()).unwrap().reason,face::Reason::Normal);
}
#[test]fn lethal_bypass_and_anchor_have_no_faceplace_switch(){
 assert!(face::sufficient(3.0,6.0,Some(3.0)));
 assert!(!face::sufficient(3.0,6.0,Some(3.01)));
 assert!(!face::sufficient(3.0,6.0,None));
 assert_eq!(face::anchor(6.0,Some(3.0)),Some(3.0));assert_eq!(face::anchor(6.0,None),Some(6.0));
}
#[test]fn invalid_numbers_do_not_create_relaxed_or_lethal_plans(){
 assert!(face::crystal(f64::NAN,true,true,true,&low_hp(3.0)).is_none());
 assert!(!face::sufficient(f64::NAN,1.0,Some(2.0)));
 assert_eq!(face::crystal(4.0,true,false,false,&low_hp(f64::NAN)).unwrap().reason,face::Reason::Normal);
}
#[test]fn horizons_are_action_specific_and_saturate(){
 assert_eq!(horizons(Use::CrystalBreak,2),Horizons{others:2,local:0});
 assert_eq!(horizons(Use::CrystalPlace,2),Horizons{others:4,local:2});
 assert_eq!(horizons(Use::Anchor,99),Horizons{others:5,local:0});
 assert_eq!(horizons(Use::CrystalPlace,u32::MAX),Horizons{others:5,local:5});
}
#[test]fn bot_flight_override_has_two_local_tick_identity_lease(){
 let mut lease=FlightLease::default();lease.offer(7,10);
 for now in 10..=12{assert_eq!(lease.offset(7,now,3,|n|Some(Vec3::new(n as f64,0.0,0.0))),Some(Vec3::new(3.0,0.0,0.0)));}
 assert!(lease.offset(7,13,3,|_|panic!("expired path must not run")).is_none());
 assert!(lease.offset(8,11,3,|_|panic!("wrong actor")).is_none());
 assert!(lease.offset(7,11,0,|_|panic!("zero lead")).is_none());
 lease.clear();assert!(lease.offset(7,11,3,|_|panic!("cleared")).is_none());
}
#[test]fn bot_override_shift_is_read_only_and_unknown_path_falls_back(){
 let mut lease=FlightLease::default();lease.offer(1,0);
 assert!(lease.offset(1,0,4,|_|None).is_none());
 assert!(lease.offset(1,0,4,|_|Some(Vec3::new(f64::NAN,0.0,0.0))).is_none());
 let s=snapshot(Vec3::new(3.0,0.0,0.0),false);
 let p=with_offset(s,Vec3::new(2.0,0.5,0.0)).unwrap();
 near(p.feet.x,2.5);near(p.feet.y,1.5);near(s.rendered_feet.x,0.5);
}
#[test]fn rtt_uses_observed_spawn_ema_before_tab_latency(){
 let mut r=Rtt::default();assert_eq!(r.ticks(None),1);assert_eq!(r.ticks(Some(-9)),1);
 assert_eq!(r.ticks(Some(149)),3);r.placement_observed(0);r.placement_observed(21);
 assert_eq!(r.ticks(Some(149)),3);r.placement_observed(4);assert_eq!(r.ticks(Some(900)),4);
 r.placement_observed(8);assert_eq!(r.ticks(None),5);r.reset();assert_eq!(r.ticks(None),1);
 assert_eq!(r.confirm_window(None),4);assert_eq!(r.confirm_window(Some(i32::MAX)),20);
}
#[test]fn tracker_uses_server_updates_not_rendered_or_native_velocity(){
 let mut tracks=Tracks::default();let p=Vec3::ZERO;
 assert_eq!(tracks.observe(1,10,p,2).unwrap().velocity,Vec3::ZERO);
 tracks.observe(1,11,p,2).unwrap();
 let m=tracks.observe(1,12,Vec3::new(0.6,0.0,0.0),2).unwrap();
 assert_eq!(m.velocity,Vec3::new(0.3,0.0,0.0));assert_eq!(m.extra_ticks,0);
 let m=tracks.observe(1,13,Vec3::new(0.6,0.0,0.0),2).unwrap();assert_eq!(m.extra_ticks,1);
 assert_eq!(tracks.observe(1,16,Vec3::new(0.6,0.0,0.0),2).unwrap().velocity,Vec3::ZERO);
}
#[test]fn tracker_gaps_rewind_identity_and_capacity_are_explicit(){
 let mut t=Tracks::default();t.observe(1,1,Vec3::ZERO,2).unwrap();
 let m=t.observe(1,11,Vec3::new(1.0,0.0,0.0),2).unwrap();assert_eq!(m.velocity.x,0.1);
 assert_eq!(t.observe(1,0,Vec3::new(5.0,0.0,0.0),2).unwrap().velocity,Vec3::ZERO);
 t.forget(1);assert_eq!(t.observe(1,2,Vec3::ZERO,2).unwrap().velocity,Vec3::ZERO);
 for id in 2..=32{t.observe(id,2,Vec3::ZERO,2).unwrap();}
 assert!(matches!(t.observe(33,2,Vec3::ZERO,2),Err(Error::Capacity)));
 t.reset();assert!(t.observe(33,2,Vec3::ZERO,2).is_ok());
}
#[derive(Default)]struct Scene{solid:Vec<BlockPos>,missing:bool,border:Option<Aabb>}
impl BlockCollisionWorld for Scene {
 type Error=io::Error;type Chunk=();
 fn collision_chunk(&mut self,_:ChunkPos)->io::Result<Option<()>>{Ok((!self.missing).then_some(()))}
 fn collision_block(&mut self,_:(),p:BlockPos)->io::Result<CollisionBlock>{Ok(CollisionBlock{
 state:BlockStateId(1),large_collision_shape:false,moving_piston:false,empty_shape:!self.solid.contains(&p)})}
 fn is_suffocating(&mut self,_:(),_:CollisionBlock,_:BlockPos)->io::Result<bool>{Ok(true)}
 fn context_shape(&mut self,_:CollisionBlock,_:BlockPos,_:&CollisionContext<'_>)->io::Result<VoxelShape>{
  Ok(VoxelShape::block())}
}
impl EntityMovementWorld for Scene {
 type Error=io::Error;
 fn entity_collisions(&mut self,_:EntityIdentity,_:Aabb)->io::Result<Vec<VoxelShape>>{panic!("forecast must use empty extra entity shapes")}
 fn movement_border(&mut self,_:EntityIdentity,_:Aabb)->io::Result<Option<VoxelShape>>{
  self.border.map(VoxelShape::from_box).transpose().map_err(io::Error::other)}
}
fn snapshot(velocity:Vec3,floating:bool)->Snapshot{Snapshot{rendered_feet:Vec3::new(0.5,1.0,0.5),
 server_feet:Vec3::new(0.5,1.0,0.5),rendered_box:Aabb::new(Vec3::new(0.2,1.0,0.2),Vec3::new(0.8,2.8,0.8)),
 motion:Motion{velocity,extra_ticks:0},floating}}
fn forecast(scene:Scene,s:Snapshot,n:u32,budget:&mut u32,cells:usize)->Result<Projection,Error>{
 let mut w=StrictChunks(scene);let mut k=NativeKernel{world:&mut w,identity:EntityIdentity(7),
 context:CollisionContext::position(1.0),cell_budget_per_step:cells};project(s,n,budget,&mut k)
}
fn near(a:f64,b:f64){assert!((a-b).abs()<1e-9,"{a} != {b}");}
#[test]fn native_free_air_applies_vertical_acceleration_before_clipping(){
 let p=forecast(Scene::default(),snapshot(Vec3::new(0.2,0.0,0.0),false),2,&mut 10,512).unwrap();
 near(p.feet.x,0.9);near(p.feet.y,1.0-0.0784-0.155232);assert_eq!(p.simulated_ticks,2);
}
#[test]fn native_wall_clips_and_stops_that_axis_without_step_up(){
 let scene=Scene{solid:vec![BlockPos::new(1,1,0),BlockPos::new(1,2,0)],..Default::default()};
 let p=forecast(scene,snapshot(Vec3::new(0.4,0.0,0.0),true),3,&mut 10,512).unwrap();
 near(p.feet.x,0.7);near(p.feet.y,1.0);near(p.bounds.max_position().x,1.0);
}
#[test]fn native_floor_prevents_falling_but_horizontal_motion_remains(){
 let scene=Scene{solid:vec![BlockPos::new(0,0,0),BlockPos::new(1,0,0)],..Default::default()};
 let p=forecast(scene,snapshot(Vec3::new(0.2,0.0,0.0),false),3,&mut 10,512).unwrap();
 near(p.feet.y,1.0);near(p.feet.x,1.1);
}
#[test]fn native_corner_clips_each_axis_and_preserves_box_translation(){
 let scene=Scene{solid:vec![BlockPos::new(1,1,0),BlockPos::new(0,1,1),BlockPos::new(1,1,1)],..Default::default()};
 let p=forecast(scene,snapshot(Vec3::new(0.4,0.0,0.4),true),2,&mut 10,512).unwrap();
 near(p.feet.x,0.7);near(p.feet.z,0.7);near(p.bounds.min_position().x,0.4);
}
#[test]fn native_border_shape_is_retained(){
 let scene=Scene{border:Some(Aabb::new(Vec3::new(1.0,-10.0,-10.0),Vec3::new(2.0,10.0,10.0))),..Default::default()};
 let p=forecast(scene,snapshot(Vec3::new(0.4,0.0,0.0),true),2,&mut 10,512).unwrap();near(p.feet.x,0.7);
}
#[test]fn floating_modes_skip_gravity_and_drag(){
 let p=forecast(Scene::default(),snapshot(Vec3::new(0.2,0.1,0.0),true),3,&mut 10,512).unwrap();
 near(p.feet.x,1.1);near(p.feet.y,1.3);
}
#[test]fn server_offset_remains_even_for_zero_lead_or_fast_velocity(){
 for (n,v) in [(0,Vec3::ZERO),(4,Vec3::new(1.01,0.0,0.0))]{
 let mut s=snapshot(v,false);s.server_feet=Vec3::new(1.5,1.0,0.5);
 let p=forecast(Scene{missing:true,..Default::default()},s,n,&mut 0,0).unwrap();
 near(p.offset.x,1.0);assert_eq!(p.simulated_ticks,0);}
}
#[test]fn native_missing_chunks_and_scan_budget_fail_closed(){
 assert!(forecast(Scene{missing:true,..Default::default()},snapshot(Vec3::ZERO,false),1,&mut 10,512).is_err());
 assert!(forecast(Scene::default(),snapshot(Vec3::ZERO,false),1,&mut 10,0).is_err());
}
#[test]fn forecast_global_budget_is_reserved_and_never_silently_truncated(){
 let mut s=snapshot(Vec3::ZERO,true);s.motion.extra_ticks=2;
 assert!(matches!(forecast(Scene::default(),s,5,&mut 6,512),Err(Error::Budget)));
 assert!(matches!(forecast(Scene::default(),s,17,&mut 50,512),Err(Error::Budget)));
 let mut steps=7;let p=forecast(Scene::default(),s,5,&mut steps,512).unwrap();
 assert_eq!(p.simulated_ticks,7);assert_eq!(steps,0);
}
#[test]fn anchor_placement_checks_current_and_predicted_other_player_box(){
 let s=snapshot(Vec3::new(0.4,0.0,0.0),true);
 let p=forecast(Scene::default(),s,3,&mut 10,512).unwrap();
 assert!(occupies(Aabb::block(BlockPos::new(0,1,0)),s.rendered_box,p));
 assert!(occupies(Aabb::block(BlockPos::new(1,1,0)),s.rendered_box,p));
 assert!(!occupies(Aabb::block(BlockPos::new(3,1,0)),s.rendered_box,p));
}
