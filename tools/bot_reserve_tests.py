"""Compile actual original Bot search and deferred acceptance against explicit effects."""
from pathlib import Path
import argparse,json,subprocess,tempfile,re

def function(text,start):
 a=text.index(start);b=text.index('{',a);depth=1
 for i in range(b+1,len(text)):
  depth+=(text[i]=='{')-(text[i]=='}')
  if not depth:return text[a:i+1]
 raise AssertionError(start)
def main():
 root=Path(__file__).resolve().parent;production=root.parent/'mixins/sunset';s=(production/'crystal_bot.rs').read_text(encoding='utf-8')
 structs='\n'.join('#[derive(Clone,Copy,Debug,PartialEq)]\n'+function(s,marker)for marker in['struct PearlPlan {','struct ObservedPearlFlight {','struct PearlSearch {'])
 # Counterfactual removes only the current reserve-one comparison and
 # reinstates the previous generic blanket denial; motion eligibility is real.
 baseline=function(s,'fn plan_pearl(').replace('fn plan_pearl(','fn before_plan_pearl(',1)
 start=baseline.index('        let following =');end=baseline.index('        if let Some((id, landing)) = actual',start)
 baseline=baseline[:start]+baseline[end:]
 old_veto='            if count(game, "totem_of_undying") == 0 && game.player.as_ref().map_or(0.,|p|p.health.health()+p.health.absorption()) <= 12. || r.pearl_low() && target.health <= 8. { return false; }\n'
 at=baseline.index('            let motion =');baseline=baseline[:at]+old_veto+baseline[at:]
 methods='\n'.join(function(s,marker)for marker in['fn pearl_note(','fn plan_pearl(','fn continue_pearl(','fn pearl_approved('])
 accepted=function(s,'BotAction::Pearl(plan) => {');body=accepted[accepted.index('{')+1:-1]
 accept='fn accept_plan(game:&mut Game,plan:PearlPlan){let look=plan.look;'+body+'}'
 code=r"""
#![allow(dead_code)]
use std::cell::RefCell;
#[derive(Clone,Copy)]struct V{x:f64,y:f64,z:f64}impl From<[f64;3]>for V{fn from(p:[f64;3])->Self{Self{x:p[0],y:p[1],z:p[2]}}}
mod horizon_client{pub mod projectiles{#[derive(Clone,Copy)]pub enum ProjectileKind{EnderPearl,Other}}pub mod mobs{#[derive(Clone,Copy,PartialEq)]pub enum Who{Mob(u32)}}}
struct Projectile{id:u32,kind:horizon_client::projectiles::ProjectileKind,owner:Option<horizon_client::mobs::Who>,age:u64,position:V,velocity:V}
struct Creatures{projectiles:Vec<Projectile>}struct Health{raw:f32,abs:f32}impl Health{fn health(&self)->f32{self.raw}fn absorption(&self)->f32{self.abs}}
struct Ground{on_ground:bool}struct Player{health:Health,ground:Ground,velocity:V}struct Mob{id:u32,position:V}
struct Game<'a>{pearls:u32,totems:u32,offhand:bool,target_abs:Option<f64>,ticks:u64,scene:u64,ready:bool,safe_land:bool,player:Option<Player>,creatures:Creatures,mobs:Vec<Mob>,calls:RefCell<Vec<(u64,f32,f32)>>,queued:Vec<PearlPlan>,sends:u32,landing:[f64;3],_life:std::marker::PhantomData<&'a()>}
impl Game<'_>{fn eye_position(&self)->Option<[f64;3]>{Some([0.,122.62,0.])}}
struct Module;struct Rival(bool);impl Rival{fn pearl_low(&self)->bool{self.0}}
mod sunset_prediction{pub fn effective_health(g:&super::Game,t:&super::util::Target)->Option<f64>{let hp=f64::from(t.health)+g.target_abs?;(hp.is_finite()&&hp>0.).then_some(hp)}}
mod tactics{READY_POLICY #[derive(PartialEq)]pub enum Pause{Go,Hold}#[derive(Clone,Copy)]pub struct PearlLanding{pub at:[f64;3],pub ticks:u32}pub fn sub(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|i|a[i]-b[i])}pub fn add(a:[f64;3],b:[f64;3])->[f64;3]{std::array::from_fn(|i|a[i]+b[i])}pub fn mul(a:[f64;3],b:f64)->[f64;3]{a.map(|x|x*b)}pub fn horizontal(a:[f64;3])->f64{a[0].hypot(a[2])}}
struct PauseClock(bool);impl PauseClock{fn state(&self,_:u64)->tactics::Pause{if self.0{tactics::Pause::Go}else{tactics::Pause::Hold}}}
struct Fight{closing_at:u64,next_pearl:u64,low_since:Option<(u64,bool)>}
struct Tactics{pause:PauseClock,pearl_search:Option<PearlSearch>,last_pearl:Option<u32>,fight:Fight,pearl:Option<PearlPlan>,pearl_at:Option<u64>}
struct State{pearl_counts:[u32;9],rival_low:bool,session:Option<u64>,target:Option<u32>,tactics:Tactics}
thread_local!{static STATE:RefCell<State>=RefCell::new(fresh());}
fn state<R>(f:impl FnOnce(&mut State)->R)->R{STATE.with(|s|f(&mut s.borrow_mut()))}
fn fresh()->State{State{pearl_counts:[0;9],rival_low:false,session:Some(9),target:Some(7),tactics:Tactics{pause:PauseClock(true),pearl_search:None,last_pearl:None,fight:Fight{closing_at:100,next_pearl:0,low_since:None},pearl:None,pearl_at:None}}}
fn same(g:&Game,scene:&u64)->bool{g.scene==*scene}fn pearl_ready(g:&Game)->bool{g.ready&&g.pearls>0&&g.player.as_ref().is_some_and(|p|tactics::pearl_health_ready(f64::from(p.health.raw+p.health.abs),g.offhand))}fn rival()->Rival{Rival(state(|s|s.rival_low))}fn count(g:&Game,item:&str)->u32{match item{"ender_pearl"=>g.pearls,"totem_of_undying"=>g.totems,_=>0}}fn enemy(_: &Game,m:&Mob)->bool{m.id==7}
mod util{#[derive(Clone,Copy)]pub struct Target{pub id:u32,pub pos:[f64;3],pub prev:[f64;3],pub health:f32}pub fn dist(a:[f64;3],b:[f64;3])->f64{a.iter().zip(b).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()}pub fn player_pos(_: &super::Game)->Option<[f64;3]>{Some([0.,121.,0.])}pub fn find_hotbar(_: &super::Game,_:impl FnMut(&str)->bool)->Option<usize>{Some(0)}}
fn pearl_from(g:&Game,_:[f64;3],_:[f64;3])->Option<tactics::PearlLanding>{Some(tactics::PearlLanding{at:g.landing,ticks:10})}
fn pearl_land(g:&Game,look:(f32,f32))->Option<tactics::PearlLanding>{g.calls.borrow_mut().push((g.ticks,look.0,look.1));Some(tactics::PearlLanding{at:g.landing,ticks:10})}
fn safe_pearl_landing(g:&Game,_:tactics::PearlLanding)->bool{g.safe_land}
enum BotAction{Pearl(PearlPlan)}fn queue_action(g:&mut Game,_:&Module,_:usize,_:(f32,f32),action:BotAction)->bool{let BotAction::Pearl(p)=action;g.queued.push(p);true}
mod remote{pub mod sunset_combat_remote{pub fn use_main_at(g:&mut super::super::Game,_:f32,_:f32)->bool{g.sends+=1;true}}}
STRUCTURES
mod bot{use super::*;METHODS BASELINE ACCEPT
fn fixture()->(Game<'static>,util::Target){state(|s|*s=fresh());(Game{pearls:8,totems:2,offhand:false,target_abs:Some(0.),ticks:100,scene:9,ready:true,safe_land:true,player:Some(Player{health:Health{raw:20.,abs:0.},ground:Ground{on_ground:true},velocity:[0.;3].into()}),creatures:Creatures{projectiles:vec![Projectile{id:42,kind:horizon_client::projectiles::ProjectileKind::EnderPearl,owner:Some(horizon_client::mobs::Who::Mob(7)),age:0,position:[10.,122.,0.].into(),velocity:[1.5,0.,0.].into()}]},mobs:vec![Mob{id:7,position:[10.,121.,0.].into()}],calls:RefCell::new(vec![]),queued:vec![],sends:0,landing:[20.,121.,0.],_life:std::marker::PhantomData},util::Target{id:7,pos:[10.,121.,0.],prev:[10.,121.,0.],health:20.})}
fn start(g:&mut Game,t:&util::Target){assert!(!plan_pearl(g,&Module,t,false));assert_eq!(g.calls.borrow().len(),8);assert_eq!(state(|s|s.tactics.pearl_search.unwrap().cursor),8);}
#[test]fn stationary_observed_peer_flight_finishes_exact_eight_candidate_slices(){let(mut g,t)=fixture();for step in 0..8{g.ticks=100+step;g.creatures.projectiles[0].age=step;let queued=plan_pearl(&mut g,&Module,&t,false);assert_eq!(queued,step==7);let calls=g.calls.borrow().iter().filter(|(tick,_,_)|*tick==g.ticks).count();assert_eq!(calls,if step==7{4}else{8});}assert_eq!(g.calls.borrow().len(),60);assert_eq!(g.queued.len(),1);assert!(state(|s|s.tactics.pearl_search.is_none()));}
#[test]fn pre_fix_actual_invocation_loses_same_stationary_peer_after_first_slice(){let(mut g,t)=fixture();assert!(!before_plan_pearl(&mut g,&Module,&t,false));for step in 1..8{g.ticks=100+step;g.creatures.projectiles[0].age=step;assert!(!before_plan_pearl(&mut g,&Module,&t,false));}assert_eq!(g.calls.borrow().len(),8);assert!(g.queued.is_empty());}
#[test]fn consumed_peer_is_retained_without_renewing_its_ten_tick_start(){let(mut g,t)=fixture();start(&mut g,&t);g.ticks=110;g.creatures.projectiles[0].age=99;assert!(!plan_pearl(&mut g,&Module,&t,false));let search=state(|s|s.tactics.pearl_search.unwrap());assert_eq!(search.cursor,16);assert_eq!(search.observed_flight.unwrap(),ObservedPearlFlight{target:7,peer:42,began:100});}
#[test]fn expired_or_rewound_observed_intent_is_cleared_before_search(){for tick in[99,111]{let(mut g,t)=fixture();start(&mut g,&t);g.ticks=tick;g.creatures.projectiles[0].age=99;assert!(!plan_pearl(&mut g,&Module,&t,false));assert_eq!(g.calls.borrow().len(),8);assert!(state(|s|s.tactics.pearl_search.is_none()));}}
#[test]fn exact_scene_target_and_pause_changes_clear_without_more_candidates(){for mode in 0..3{let(mut g,t)=fixture();start(&mut g,&t);g.ticks=101;match mode{0=>g.scene=10,1=>state(|s|s.target=Some(8)),_=>state(|s|s.tactics.pause.0=false)}assert!(!plan_pearl(&mut g,&Module,&t,false));assert_eq!(g.calls.borrow().len(),8);assert!(state(|s|s.tactics.pearl_search.is_none()));}}
#[test]fn new_peer_id_replaces_ticket_and_restarts_at_eight_not_sixteen(){let(mut g,t)=fixture();start(&mut g,&t);g.ticks=101;g.creatures.projectiles[0].id=43;g.landing=[22.,121.,0.];assert!(!plan_pearl(&mut g,&Module,&t,false));let search=state(|s|s.tactics.pearl_search.unwrap());assert_eq!(search.cursor,8);assert_eq!(search.observed_flight.unwrap().peer,43);assert_eq!(search.observed_flight.unwrap().began,101);}
#[test]fn only_target_owned_young_observed_projectile_can_start_retention(){for mode in 0..2{let(mut g,t)=fixture();if mode==0{g.creatures.projectiles[0].owner=Some(horizon_client::mobs::Who::Mob(8));}else{g.creatures.projectiles[0].age=3;}assert!(!plan_pearl(&mut g,&Module,&t,false));assert!(g.calls.borrow().is_empty());assert!(state(|s|s.tactics.pearl_search.is_none()));}}
#[test]fn existing_generic_runaway_search_keeps_no_observed_flight_ticket(){let(mut g,mut t)=fixture();g.creatures.projectiles.clear();t.pos=[20.,121.,0.];t.prev=[19.,121.,0.];g.landing=[32.,121.,0.];for tick in 100..102{g.ticks=tick;assert!(!plan_pearl(&mut g,&Module,&t,false));assert!(state(|s|s.tactics.pearl_search.unwrap().observed_flight.is_none()));}assert_eq!(g.calls.borrow().len(),16);}
#[test]fn actual_deferred_acceptance_rechecks_readiness_clock_ground_and_landing(){for mode in 0..5{let(mut g,t)=fixture();for tick in 100..108{g.ticks=tick;plan_pearl(&mut g,&Module,&t,false);}let plan=g.queued.pop().unwrap();match mode{0=>g.ready=false,1=>g.safe_land=false,2=>g.player.as_mut().unwrap().ground.on_ground=false,3=>g.ticks=plan.until+1,_=>{}}accept_plan(&mut g,plan);assert_eq!(g.sends,if mode==4{1}else{0});}}
fn generic(g:&mut Game,t:&mut util::Target){g.creatures.projectiles.clear();t.pos=[20.,121.,0.];t.prev=[19.,121.,0.];g.landing=[32.,121.,0.];}
fn searched(g:&Game)->bool{g.calls.borrow().len()==8}
#[test]fn reserve_low_no_totem_zero_one_two_generic_chase(){for pearls in 0..=2{let(mut g,mut t)=fixture();generic(&mut g,&mut t);g.totems=0;g.pearls=pearls;g.player.as_mut().unwrap().health.raw=12.;plan_pearl(&mut g,&Module,&t,false);assert_eq!(searched(&g),pearls==2);}}
#[test]fn reserve_low_no_totem_zero_one_two_observed_and_retained_chase(){for pearls in 0..=2{let(mut g,t)=fixture();g.totems=0;g.pearls=pearls;g.player.as_mut().unwrap().health.raw=12.;plan_pearl(&mut g,&Module,&t,false);assert_eq!(searched(&g),pearls==2);if pearls==2{g.pearls=1;g.ticks+=1;g.creatures.projectiles[0].age=3;plan_pearl(&mut g,&Module,&t,false);assert_eq!(g.calls.borrow().len(),8);}}}
#[test]fn reserve_total_totem_count_does_not_override_shared_offhand_health_safety(){let(mut g,mut t)=fixture();generic(&mut g,&mut t);g.pearls=1;g.totems=1;g.player.as_mut().unwrap().health.raw=12.;plan_pearl(&mut g,&Module,&t,false);assert!(searched(&g));let(mut g,mut t)=fixture();generic(&mut g,&mut t);g.pearls=1;g.totems=1;g.player.as_mut().unwrap().health.raw=4.;plan_pearl(&mut g,&Module,&t,false);assert!(!searched(&g));g.offhand=true;plan_pearl(&mut g,&Module,&t,false);assert!(searched(&g));}
#[test]fn reserve_rival_low_uses_effective_target_health_and_only_generic_intent(){for(target_hp,abs,pearls,expected)in[(8.,Some(0.),1,false),(8.,Some(0.),2,true),(8.01,Some(0.),1,true),(6.,Some(4.),1,true),(8.,None,1,false),(8.,None,2,true)]{let(mut g,mut t)=fixture();generic(&mut g,&mut t);state(|s|s.rival_low=true);g.totems=0;g.pearls=pearls;g.target_abs=abs;t.health=target_hp;plan_pearl(&mut g,&Module,&t,false);assert_eq!(searched(&g),expected);}let(mut g,mut t)=fixture();state(|s|s.rival_low=true);g.totems=0;g.pearls=1;t.health=4.;plan_pearl(&mut g,&Module,&t,false);assert!(searched(&g));g.ticks+=1;g.creatures.projectiles[0].age=3;plan_pearl(&mut g,&Module,&t,false);assert_eq!(g.calls.borrow().len(),16);}
#[test]fn reserve_raw_own_health_boundary_is_not_hidden_by_absorption(){for(raw,abs,expected)in[(12.,8.,false),(12.01,0.,true),(4.,10.,false)]{let(mut g,mut t)=fixture();generic(&mut g,&mut t);g.pearls=1;g.totems=0;g.player.as_mut().unwrap().health=Health{raw,abs};plan_pearl(&mut g,&Module,&t,false);assert_eq!(searched(&g),expected);}}
#[test]fn reserve_does_not_apply_to_escape_or_bypass_finite_shared_readiness(){let(mut g,t)=fixture();g.totems=0;g.pearls=1;g.player.as_mut().unwrap().health.raw=10.;plan_pearl(&mut g,&Module,&t,true);assert!(searched(&g));for raw in[f32::NAN,f32::INFINITY,6.]{let(mut g,t)=fixture();g.pearls=2;g.totems=0;g.player.as_mut().unwrap().health.raw=raw;plan_pearl(&mut g,&Module,&t,false);assert!(!searched(&g));}}
#[test]fn reserve_allowance_does_not_invent_generic_chase_trigger(){let(mut g,t)=fixture();g.creatures.projectiles.clear();g.pearls=2;g.totems=0;g.player.as_mut().unwrap().health.raw=12.;plan_pearl(&mut g,&Module,&t,false);assert!(g.calls.borrow().is_empty());}
#[test]fn reserve_before_actual_invocation_blanket_denies_multiple_pearls(){let(mut old,mut t)=fixture();generic(&mut old,&mut t);old.pearls=2;old.totems=0;old.player.as_mut().unwrap().health.raw=12.;before_plan_pearl(&mut old,&Module,&t,false);assert!(!searched(&old));let(mut new,mut t)=fixture();generic(&mut new,&mut t);new.pearls=2;new.totems=0;new.player.as_mut().unwrap().health.raw=12.;plan_pearl(&mut new,&Module,&t,false);assert!(searched(&new));}

}
"""
 code=code.replace('STRUCTURES',structs).replace('METHODS',methods).replace('BASELINE',baseline).replace('ACCEPT',accept)
 ready_policy=function((production/'bot_tactics_policy.rs').read_text(encoding='utf-8'),'pub fn pearl_health_ready(')
 code=code.replace('READY_POLICY',ready_policy)
 for match in reversed(list(re.finditer(r'#\[test\]fn (?!reserve_)\w+\(',code))):
  name=re.search(r'fn (\w+)\(',match.group()).group(1);fragment=function(code[match.start():],'fn '+name+'(');end=code.index(fragment,match.start())+len(fragment);code=code[:match.start()]+code[end:]
 with tempfile.TemporaryDirectory(prefix='bot-reserve-one-')as tmp:
  f=Path(tmp)/'test.rs';binary=Path(tmp)/'test.exe';f.write_text(code,encoding='utf-8');r=subprocess.run(['rustc','--edition=2024','--test',str(f),'-o',str(binary)],capture_output=True,text=True,encoding='utf-8');assert r.returncode==0,r.stderr
  r=subprocess.run([str(binary)],capture_output=True,text=True,encoding='utf-8');print(r.stdout);assert r.returncode==0,r.stdout+r.stderr
 old_plan=baseline;new_plan=function(s,'fn plan_pearl(')
 assert old_plan[old_plan.index('            let motion ='):]==new_plan[new_plan.index('            let motion ='):]
 assert 'tactics::pearl_health_ready(life, totem)'in function(s,'fn pearl_ready(') and new_plan.index('if !escape {')<new_plan.index('let keep_one')<new_plan.index('if let Some((id, landing)) = actual')
 print('source-route contract: 2 passed')

if __name__=='__main__':main()
