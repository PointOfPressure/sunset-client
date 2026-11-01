"""Real native air motion, mouse rotation and voxel-ray replay of the pearl/stand conflict."""
from pathlib import Path
import argparse,subprocess,tempfile,json

def function(text,start):
 a=text.index(start);b=text.index('{',a);depth=1
 for i in range(b+1,len(text)):
  depth+=(text[i]=='{')-(text[i]=='}')
  if not depth:return text[a:i+1]
 raise AssertionError(start)
def movement(source,name):
 a=source.index('    if occupied && !eating && !critical_retreat {');b=source.index('    if !coordinate(',a)
 return 'fn '+name+'(game:&mut Game,target:util::Target,occupied:bool)->policy::Decision{let own=util::player_pos(game).unwrap();let eating=false;let critical_retreat=false;let stance=tactics::Stance::Press;let mut decision=bot_decision(policy::Phase::Press,1,0,true,true,false);'+source[a:b]+'decision}'
def main():
 p=argparse.ArgumentParser();p.add_argument('--dependency-dir',type=Path,required=True);a=p.parse_args();root=Path(__file__).resolve().parent;production=root.parent/'mixins/sunset';s=(production/'crystal_bot.rs').read_text(encoding='utf-8')
 # Remove only the new hold-priority branch as a negative control, preserving
 # the original pending-hold then stand-override behavior in current code.
 start=s.index('    if occupied && !eating && !critical_retreat {');lo=s.index('    if let Some(plan) = state(|s| s.tactics.pearl) {',start);tail='    } else if let Some(spot) = state(|s| s.tactics.stand) {';hi=s.index(tail,lo)
 before=s[:lo]+'    if let Some(spot) = state(|s| s.tactics.stand) {'+s[hi+len(tail):]
 methods='\n'.join(function(s,sig)for sig in['fn face_spot(','fn bot_decision(','fn pearl_land(','fn pearl_approved(','fn continue_pearl('])
 structs='\n'.join('#[derive(Clone,Copy)]\n'+function(s,sig)for sig in['struct PearlPlan {','struct ObservedPearlFlight {','struct PearlSearch {'])
 plan=function(s,'fn plan_pearl(');lo=plan.index('    let eye =');hi=plan.index('    if search.cursor >= max ||',lo)
 choose='fn choose(game:&Game,spot:[f64;3])->Option<PearlPlan>{let own=util::player_pos(game).unwrap();let escape=false;let mut search=PearlSearch{escape,observed_flight:None,spot,began:game.ticks,cursor:0,best:None};'+plan[lo:hi]+'search.best.map(|(_,plan)|plan)}'
 accept=function(s,'BotAction::Pearl(plan) => {');accept='fn accept(game:&mut Game,plan:PearlPlan){let look=plan.look;'+accept[accept.index('{')+1:-1]+'}'
 look=function((production/'runtime.rs').read_text(encoding='utf-8'),'pub(super) fn look_at(')
 code=r"""
#![allow(dead_code)]
use horizon_core::{geometry::{Vec3,VoxelShape,traverse_blocks},position::BlockPos};
use horizon_client::mouse::Rotation;
use horizon_world::living_air::{self,LivingAirWorld};
mod game{use super::*;
 pub struct Ground{pub on_ground:bool}pub struct Player{pub ground:Ground,pub position:Vec3,pub velocity:Vec3}
 pub struct Mob{pub id:u32,pub position:Vec3}
 pub struct Game<'a>{pub ticks:u64,pub yaw:f32,pub pitch:f32,pub player:Option<Player>,pub mobs:Vec<Mob>,pub queued:Vec<PearlPlan>,pub uses:u32,pub destination_safe:bool,pub ready:bool,pub life:std::marker::PhantomData<&'a()>}
 impl Game<'_>{pub fn eye_position(&self)->Option<[f64;3]>{let p=self.player.as_ref()?.position;Some([p.x,p.y+1.62000000476836,p.z])}pub fn turn_player(&mut self,xo:f64,yo:f64){let mut r=Rotation{x_rot:self.pitch,y_rot:self.yaw,x_rot_o:self.pitch,y_rot_o:self.yaw};r.turn(xo,yo);self.pitch=r.x_rot;self.yaw=r.y_rot;}}
 pub mod remote{pub mod sunset_combat_remote{pub fn use_main_at(g:&mut super::super::Game,_:f32,_:f32)->bool{g.uses+=1;true}}}
 STRUCTS
 pub mod bot{use super::*;use std::cell::RefCell;
 #[path="POLICY"]mod policy;#[path="TACTICS"]mod tactics;
 struct Module;
 struct Fight{next_pearl:u64}struct Tactics{pearl:Option<PearlPlan>,stand:Option<[f64;3]>,drop_y:Option<f64>,spot_after:u64,pearl_at:Option<u64>,fight:Fight}
 struct State{tactics:Tactics}thread_local!{static STATE:RefCell<State>=RefCell::new(fresh());}fn fresh()->State{State{tactics:Tactics{pearl:None,stand:Some([4.5,121.,.5]),drop_y:None,spot_after:1000,pearl_at:None,fight:Fight{next_pearl:0}}}}
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{STATE.with(|s|f(&mut s.borrow_mut()))}fn pearl_note(_:usize,_:u32){}
 mod sunset{LOOK}
 mod util{#[derive(Clone,Copy)]pub struct Target{pub id:u32,pub pos:[f64;3],pub lo:[f64;3],pub hi:[f64;3]}pub fn player_pos(g:&super::Game)->Option<[f64;3]>{let p=g.player.as_ref()?.position;Some([p.x,p.y,p.z])}pub fn find_hotbar(_: &super::Game,_:impl FnMut(&str)->bool)->Option<usize>{Some(0)}pub fn dist(a:[f64;3],b:[f64;3])->f64{a.iter().zip(b).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()}}
 fn enemy(_: &Game,m:&Mob)->bool{m.id==7}fn pick_stand(_: &mut Game,_:&util::Target,_:tactics::Stance){}
 fn pearl_ready(g:&Game)->bool{g.ready}
 fn safe_pearl_landing(g:&Game,land:tactics::PearlLanding)->bool{g.destination_safe&&land.at[0]>=-24.&&land.at[0]<=64.&&land.at[2]>=-20.&&land.at[2]<=20.}
 fn pearl_from(g:&Game,from:[f64;3],velocity:[f64;3])->Option<tactics::PearlLanding>{tactics::pearl_path(from,velocity,|a,b|{if a==b{return Some((None,false));}let from=Vec3::new(a[0],a[1],a[2]);let to=Vec3::new(b[0],b[1],b[2]);let hit=traverse_blocks(from,to,512,|p:BlockPos|{if (118..=120).contains(&p.y)&&(-24..=64).contains(&p.x)&&(-20..=20).contains(&p.z)&&(g.destination_safe||p.x<8){VoxelShape::block().clip(from,to,p).map(|hit|hit.location)}else{None}}).ok()?;Some((hit.map(|p|[p.x,p.y,p.z]),false))})}
 enum BotAction{Pearl(PearlPlan)}fn queue_action(g:&mut Game,_:&Module,_:usize,_:(f32,f32),action:BotAction)->bool{let BotAction::Pearl(p)=action;g.queued.push(p);true}
 METHODS
 CHOOSE
 ACCEPT
 OLD_MOVE
 NEW_MOVE
 struct Air<'a>{player:&'a mut Player,yaw:f32}
 impl LivingAirWorld for Air<'_>{type Error=();fn movement_support_position(&mut self)->Result<BlockPos,()>{Ok(BlockPos::new(self.player.position.x.floor()as i32,120,self.player.position.z.floor()as i32))}fn on_ground(&mut self)->Result<bool,()>{Ok(true)}fn block_friction(&mut self,_:BlockPos)->Result<f32,()>{Ok(0.6)}fn friction_modifier(&mut self)->Result<f64,()>{Ok(1.)}fn speed(&mut self)->Result<f32,()>{Ok(0.13)}fn flying_speed(&mut self)->Result<f32,()>{Ok(0.026)}fn yaw(&mut self)->Result<f32,()>{Ok(self.yaw)}fn velocity(&self)->Vec3{self.player.velocity}fn set_velocity(&mut self,v:Vec3)->Result<(),()>{self.player.velocity=v;Ok(())}fn on_climbable(&mut self)->Result<bool,()>{Ok(false)}fn reset_fall_distance(&mut self)->Result<(),()>{Ok(())}fn in_scaffolding(&mut self)->Result<bool,()>{Ok(false)}fn suppressing_ladder_slide(&mut self)->Result<bool,()>{Ok(false)}fn is_player_class(&self)->bool{true}fn move_self(&mut self,m:Vec3)->Result<(),()>{let p=self.player.position;self.player.position=Vec3::new(p.x+m.x,(p.y+m.y).max(121.),p.z+m.z);if p.y+m.y<=121.{self.player.velocity.y=0.;}Ok(())}fn horizontal_collision(&self)->bool{false}fn jumping(&self)->bool{false}fn was_in_powder_snow(&self)->bool{false}fn can_walk_powder_snow(&mut self)->Result<bool,()>{Ok(false)}fn levitation_amplifier(&mut self)->Result<Option<i32>,()>{Ok(None)}fn is_client_side(&mut self)->Result<bool,()>{Ok(true)}fn has_chunk_at(&mut self,_:BlockPos)->Result<bool,()>{Ok(true)}fn effective_gravity(&mut self)->Result<f64,()>{Ok(0.08)}fn y(&self)->f64{self.player.position.y}fn minimum_y(&mut self)->Result<i32,()>{Ok(-64)}fn discard_friction(&mut self)->Result<bool,()>{Ok(false)}fn air_drag_modifier(&mut self)->Result<f64,()>{Ok(1.)}fn omnidirectional_air_mover(&mut self)->Result<bool,()>{Ok(false)}}
 fn physics(g:&mut Game,d:policy::Decision){let input=Vec3::new(f64::from(d.side as f32*0.98),0.,f64::from(d.forward as f32*0.98));let mut air=Air{player:g.player.as_mut().unwrap(),yaw:g.yaw};living_air::travel_in_air(&mut air,input).unwrap();}
 fn fixture()->(Game<'static>,util::Target){state(|s|*s=fresh());(Game{ticks:100,yaw:-90.,pitch:25.212677,player:Some(Player{ground:Ground{on_ground:true},position:Vec3::new(1.315,121.,.5),velocity:Vec3::new(.1396,-.0784,0.)}),mobs:vec![Mob{id:7,position:Vec3::new(10.5,121.,.5)}],queued:vec![],uses:0,destination_safe:true,ready:true,life:std::marker::PhantomData},util::Target{id:7,pos:[10.5,121.,.5],lo:[10.2,121.,.2],hi:[10.8,122.8,.8]})}
 fn actual_plan(g:&Game)->(PearlPlan,[f64;3]){let peer=pearl_from(g,[11.971071232375024,122.7558228013006,.5053177073796009],[1.471071232375023,.2358227980223401,.0053177073796009154]).unwrap();let plan=choose(g,peer.at).expect("recorded first8 must contain valid native forecast");assert!(pearl_approved(g,plan));(plan,peer.at)}
 fn run(old:bool,unsafe_destination:bool)->(Game<'static>,Option<u64>,Vec<(u64,f64,f64,i8)>){let(mut g,t)=fixture();let(plan,_)=actual_plan(&g);state(|s|s.tactics.pearl=Some(plan));g.destination_safe=!unsafe_destination;let mut sent=None;let mut trace=vec![];for step in 0..=15{g.ticks=100+step;let occupied=continue_pearl(&mut g,&Module,&t);let d=if old{old_movement(&mut g,t,occupied)}else{new_movement(&mut g,t,occupied)};physics(&mut g,d);while let Some(p)=g.queued.pop(){accept(&mut g,p);}trace.push((g.ticks,g.player.as_ref().unwrap().position.x,g.player.as_ref().unwrap().velocity.x,d.forward));if g.uses>0{sent=Some(step);break;}}(g,sent,trace)}
 #[test]fn actual_original_stand_override_exhausts_14tick_native_still_deadline(){let(g,sent,trace)=run(true,false);assert_eq!(sent,None);assert_eq!(g.uses,0);assert!(trace.iter().filter(|(t,_,_,_)|*t<=114).all(|(_,_,vx,_)|vx.abs()>.01));assert!(trace.iter().any(|(_,x,_,_)|*x>4.));}
 #[test]fn fixed_production_hold_sends_only_after_native_friction_and_fresh_voxel_approval(){let(g,sent,trace)=run(false,false);assert!(sent.is_some_and(|tick|tick<=7));assert_eq!(g.uses,1);assert!(trace.iter().all(|(_,_,_,forward)|*forward==0));assert!(g.player.as_ref().unwrap().velocity.x.abs()<=.01);assert!((g.pitch+17.).abs()<.01);}
 #[test]fn hold_keeps_prior_stand_intent_and_normal_movement_resumes_without_pearl(){let(mut g,t)=fixture();let(plan,_)=actual_plan(&g);state(|s|s.tactics.pearl=Some(plan));let d=new_movement(&mut g,t,true);assert_eq!((d.forward,d.side,d.sprint,d.auras),(0,0,false,false));assert_eq!(state(|s|s.tactics.stand),Some([4.5,121.,.5]));state(|s|s.tactics.pearl=None);let d=new_movement(&mut g,t,false);assert_eq!(d.forward,1);}
 #[test]fn unsafe_destination_still_denies_native_use_and_hold_expires_instead_of_sticking(){let(g,sent,trace)=run(false,true);assert_eq!(sent,None);assert_eq!(g.uses,0);assert_eq!(trace.last().unwrap().3,1);assert!(state(|s|s.tactics.pearl.is_none()));}
 #[test]fn pending_hold_prioritizes_throw_camera_over_feet_stand_native_rotation(){let(mut g,t)=fixture();let(plan,_)=actual_plan(&g);state(|s|s.tactics.pearl=Some(plan));new_movement(&mut g,t,true);assert!(g.pitch<0.);let(mut old,t)=fixture();state(|s|s.tactics.pearl=Some(plan));old_movement(&mut old,t,true);assert!(old.pitch>25.);}
 }
}
"""
 code=code.replace('STRUCTS',structs).replace('POLICY',(production/'crystal_bot_policy.rs').resolve().as_posix()).replace('TACTICS',(production/'bot_tactics_policy.rs').resolve().as_posix()).replace('LOOK',look).replace('METHODS',methods).replace('CHOOSE',choose).replace('ACCEPT',accept).replace('OLD_MOVE',movement(before,'old_movement')).replace('NEW_MOVE',movement(s,'new_movement'))
 import re
 code=re.sub(r'(?<![\w.)\]])\.(\d)',r'0.\1',code)
 deps={name:sorted(a.dependency_dir.glob('lib'+name+'-*.rlib'),key=lambda f:f.stat().st_mtime,reverse=True)[0]for name in['horizon_client','horizon_core','horizon_world']};command=['rustc','--edition=2024','--test','SOURCE','-L','dependency='+str(a.dependency_dir)]
 for name,path in deps.items():command+=['--extern',name+'='+str(path)]
 for library in(Path.home()/'.cargo/registry/src').glob('*/windows_x86_64_msvc-*/lib/windows.0.*.lib'):command+=['-L','native='+str(library.parent)]
 with tempfile.TemporaryDirectory(prefix='native-pearl-hold-')as tmp:
  f=Path(tmp)/'test.rs';binary=Path(tmp)/'test.exe';f.write_text(code,encoding='utf-8');r=subprocess.run([str(f)if x=='SOURCE'else x for x in command]+['-o',str(binary)],capture_output=True,text=True,encoding='utf-8');assert r.returncode==0,r.stderr
  r=subprocess.run([str(binary)],capture_output=True,text=True,encoding='utf-8');print(r.stdout);assert r.returncode==0,r.stdout+r.stderr
 body=function(s,'BotAction::Pearl(plan) => {');assert '&& pearl_ready(game)'in body and '&& game.ticks <= plan.until'in body and '&& pearl_approved(game, plan)'in body and 'use_main_at(game,look.0,look.1)'in body
 assert 'game.ticks > plan.until'in function(s,'fn continue_pearl(')
 print('source-route contract: 2 passed')

if __name__=='__main__':main()
