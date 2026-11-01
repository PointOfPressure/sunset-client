"""Compile production manual cursor/range/completion functions in explicit adapters.

Real pinned horizon_core Aabb, production macro ray/sequence, controlled Game
catalog, outline callback and hand effects. No client, packet, world or RPC.
"""
from pathlib import Path
import argparse,hashlib,json,subprocess,tempfile
REPO=Path(__file__).resolve().parents[1]
SOURCE=REPO/'mixins/sunset'
def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('engine',type=Path);parser.add_argument('--dependency-dir',type=Path,required=True);parser.add_argument('--out',type=Path);args=parser.parse_args()
    assert (args.engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    macro=(SOURCE/'combat_macros.rs').read_text(encoding='utf-8');tactics=(SOURCE/'explosive_tactics.rs').read_text(encoding='utf-8')
    cursor='\n'.join(function(macro,s)for s in ['fn direction(','fn camera_click(','enum CursorEntity','fn cursor_entity(','fn crystal_click(','fn crosshair_crystal('])
    ranges='\n'.join(function(tactics,s)for s in ['fn same(','pub(super) fn refresh(','pub(super) fn interaction_ranges('])
    lifecycle='\n'.join(function(macro,s)for s in ['fn same_session(','pub(super) fn reset(','fn complete('])
    code=r'''
#![allow(dead_code)]
use horizon_core::geometry::{Aabb,Vec3};
#[path="POLICY"]mod policy;
mod util{pub fn dist(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|(a[i]-b[i]).powi(2)).sum::<f64>().sqrt()}}
fn bounds(lo:[f64;3],hi:[f64;3])->Aabb{Aabb::new(Vec3::new(lo[0],lo[1],lo[2]),Vec3::new(hi[0],hi[1],hi[2]))}
mod cursor {
 use super::*;use std::{cell::RefCell,marker::PhantomData};
 #[derive(Clone,Copy)]struct Point{x:f64,y:f64,z:f64}
 #[derive(Clone,Copy)]struct Click{pos:[i32;3],face:i32,hit:[f64;3]}
 #[derive(Clone,Copy)]struct Hit{pos:[i32;3],face:[i32;3],point:[f64;3]}
 #[derive(Clone,Copy)]struct Body{id:u32,b:Aabb,removed:bool,living:bool,marker:bool,health:f32}
 impl Body{fn alive(&self)->bool{self.living}fn bounds(&self)->Aabb{self.b}}
 #[derive(Clone,Copy)]struct Flat{bounds:([f64;3],[f64;3]),id:u32}
 impl Flat{fn bounds(&self)->([f64;3],[f64;3]){self.bounds}}
 #[derive(Clone,Copy)]struct Crystal{id:u32,position:Point,invulnerable:bool}
 struct Projectile{b:Aabb,pickable:bool}impl Projectile{fn aabb(&self)->Aabb{self.b}}
 struct Creatures{projectiles:Vec<Projectile>}
 struct Game<'a>{yaw:f32,pitch:f32,eye:Option<[f64;3]>,ranges:Option<(f64,f64)>,wall:Option<f64>,
  mobs:Vec<Body>,crystals:Vec<Crystal>,boats:Vec<Body>,carts:Vec<Body>,armor_stands:Vec<Body>,
  cushions:Vec<Flat>,item_frames:Vec<Flat>,paintings:Vec<Flat>,creatures:Creatures,riding:Option<u32>,
  spectator:Option<u32>,dragon:Option<f64>,line_clear:bool,clips:RefCell<Vec<f64>>,life:PhantomData<&'a ()>}
 impl Game<'_>{fn eye_position(&self)->Option<[f64;3]>{self.eye}
  fn clip_ray(&self,eye:[f64;3],dir:[f64;3],range:f64)->Option<Hit>{self.clips.borrow_mut().push(range);
   self.wall.filter(|d|*d<=range).map(|d|Hit{pos:[0,0,d.floor()as i32],face:[0,0,-1],point:std::array::from_fn(|i|eye[i]+dir[i]*d)})}
  fn remote_dragon_part_hit(&self,_:[f64;3],_:[f64;3],range:f64)->Option<(i32,f64,[f64;3])>{self.dragon.filter(|d|*d<=range).map(|d|(55,d,[0.;3]))}}
 mod sunset_explosive_tactics{pub fn interaction_ranges(g:&super::Game<'_>)->Option<(f64,f64)>{g.ranges}}
 mod remote{pub mod sunset_combat_remote{pub fn player_mode(g:&super::super::Game<'_>,id:u32)->Option<i32>{(g.spectator==Some(id)).then_some(3)}}}
 mod deflect{pub fn redirectable(p:&super::Projectile)->bool{p.pickable}}
 struct Budget;fn clear_line(g:&Game<'_>,_:[f64;3],_:[f64;3],_:Option<[i32;3]>,_:&mut Budget)->bool{g.line_clear}
 mod implementation {use super::*;CURSOR}
 use implementation::*;
 fn fixture()->Game<'static>{Game{yaw:0.,pitch:0.,eye:Some([0.,1.,0.]),ranges:Some((4.5,3.)),wall:None,
  mobs:vec![],crystals:vec![],boats:vec![],carts:vec![],armor_stands:vec![],cushions:vec![],item_frames:vec![],paintings:vec![],
  creatures:Creatures{projectiles:vec![]},riding:None,spectator:None,dragon:None,line_clear:true,clips:RefCell::new(vec![]),life:PhantomData}}
 fn crystal(g:&mut Game<'_>,id:u32,z:f64){g.crystals.push(Crystal{id,position:Point{x:0.,y:0.,z},invulnerable:false});}
 fn body(id:u32,z:f64)->Body{Body{id,b:bounds([-0.3,0.,z-0.3],[0.3,1.8,z+0.3]),removed:false,living:true,marker:false,health:20.}}
 fn flat(z:f64)->Flat{Flat{bounds:([-0.5,0.,z-0.01],[0.5,2.,z+0.01]),id:77}}
 #[test]fn creative_entity_attribute_allows_contact_beyond_old_three(){let mut g=fixture();crystal(&mut g,1,5.2);
  assert!(crosshair_crystal(&g,&mut Budget).is_none());g.ranges=Some((5.,5.));assert_eq!(crosshair_crystal(&g,&mut Budget).unwrap().0,1);}
 #[test]fn custom_survival_entity_range_is_respected(){let mut g=fixture();crystal(&mut g,1,4.8);g.ranges=Some((4.5,4.));assert!(crosshair_crystal(&g,&mut Budget).is_some());g.ranges=Some((4.5,3.7));assert!(crosshair_crystal(&g,&mut Budget).is_none());}
 #[test]fn current_attribute_shrink_rejects_same_old_id(){let mut g=fixture();crystal(&mut g,3,4.8);g.ranges=Some((5.,5.));assert!(crosshair_crystal(&g,&mut Budget).is_some());g.ranges=Some((5.,3.));assert!(crosshair_crystal(&g,&mut Budget).is_none());}
 #[test]fn nearer_living_entity_prevents_crystal_or_block_clickthrough(){let mut g=fixture();crystal(&mut g,1,3.);g.wall=Some(4.);g.mobs.push(body(8,1.));assert!(crosshair_crystal(&g,&mut Budget).is_none());assert!(crystal_click(&g).is_none());}
 #[test]fn farther_living_entity_does_not_hide_nearer_crystal(){let mut g=fixture();crystal(&mut g,1,2.);g.mobs.push(body(8,2.8));assert_eq!(crosshair_crystal(&g,&mut Budget).unwrap().0,1);}
 #[test]fn anchor_block_ray_deliberately_ignores_nearer_entity(){let mut g=fixture();g.wall=Some(4.);g.mobs.push(body(8,1.));assert!(crystal_click(&g).is_none());assert!(camera_click(&g).is_some());}
 #[test]fn blocks_beyond_block_range_still_occlude_extended_entity_range(){let mut g=fixture();g.ranges=Some((2.,5.));g.wall=Some(2.5);crystal(&mut g,1,4.);assert!(crosshair_crystal(&g,&mut Budget).is_none());assert_eq!(g.clips.borrow()[0],5.);}
 #[test]fn block_cursor_uses_raw_attribute_without_autonomous_margin(){let mut g=fixture();g.ranges=Some((6.,3.));g.wall=Some(5.99);assert!(camera_click(&g).is_some());assert_eq!(g.clips.borrow()[0],6.);}
 #[test]fn invalid_context_or_unloaded_line_cannot_break(){let mut g=fixture();crystal(&mut g,1,2.);g.line_clear=false;assert!(crosshair_crystal(&g,&mut Budget).is_none());g.ranges=None;assert!(crystal_click(&g).is_none());}
 #[test]fn spectator_removed_and_own_vehicle_are_not_entity_occluders(){let mut g=fixture();crystal(&mut g,1,3.);g.mobs.push(body(8,1.));g.spectator=Some(8);assert!(crosshair_crystal(&g,&mut Budget).is_some());g.spectator=None;g.mobs[0].removed=true;assert!(crosshair_crystal(&g,&mut Budget).is_some());g.mobs.clear();g.boats.push(body(8,1.));g.riding=Some(8);assert!(crosshair_crystal(&g,&mut Budget).is_some());}
 #[test]fn known_vehicle_frame_and_projectile_entities_occlude(){let mut g=fixture();crystal(&mut g,1,3.);g.boats.push(body(8,1.));assert!(crosshair_crystal(&g,&mut Budget).is_none());g.boats.clear();g.item_frames.push(flat(1.));assert!(crosshair_crystal(&g,&mut Budget).is_none());g.item_frames.clear();g.creatures.projectiles.push(Projectile{b:bounds([0.,0.8,1.5],[0.2,1.1,1.7]),pickable:true});assert!(crosshair_crystal(&g,&mut Budget).is_none());}
 #[test]fn dragon_and_invulnerable_crystal_prevent_break_behind_them(){let mut g=fixture();crystal(&mut g,1,3.);g.dragon=Some(0.5);assert!(crosshair_crystal(&g,&mut Budget).is_none());g.dragon=None;crystal(&mut g,2,1.5);g.crystals[1].invulnerable=true;assert!(crosshair_crystal(&g,&mut Budget).is_none());}
 #[test]fn malformed_observation_and_overfull_scene_refuse_plan(){let mut g=fixture();crystal(&mut g,1,3.);g.mobs.push(body(8,1.));g.mobs[0].b=bounds([f64::NAN,0.,0.],[1.,2.,1.]);assert!(cursor_entity(&g).is_err());g.mobs=vec![body(9,8.);768];assert!(cursor_entity(&g).is_err());}
}
mod range_cache {
 use std::sync::{Arc,Mutex};type Connection=Arc<Mutex<Option<Arc<()>>>>;
 struct Session{counters:Connection}struct Remote{session:Session,entity_id:i32}
 struct Player{block:f64,entity:f64}impl Player{fn attribute(&mut self,key:&str)->Result<f64,()>{Ok(if key=="minecraft:block_interaction_range"{self.block}else{self.entity})}}
 pub(super) struct Game<'a>{remote:Option<Remote>,player:Option<Player>,ticks:u64,dim:u8,life:std::marker::PhantomData<&'a ()>}
 struct Context{session:Connection,dim:u8,id:i32,tick:u64,block:f64,entity:f64,focus:Option<u32>}
 static CONTEXT:Mutex<Option<Context>>=Mutex::new(None);
 RANGES
 fn fixture()->Game<'static>{*CONTEXT.lock().unwrap()=None;Game{remote:Some(Remote{entity_id:7,session:Session{counters:Arc::new(Mutex::new(None))}}),player:Some(Player{block:4.5,entity:3.}),ticks:11,dim:0,life:std::marker::PhantomData}}
 #[test]fn real_attribute_values_survive_without_fixed_reach(){let mut g=fixture();g.player.as_mut().unwrap().block=6.25;g.player.as_mut().unwrap().entity=4.75;assert!(refresh(&mut g));assert_eq!(interaction_ranges(&g),Some((6.25,4.75)));}
 #[test]fn old_tick_session_or_dimension_cannot_reuse_range_cache(){let mut g=fixture();assert!(refresh(&mut g));g.ticks+=1;assert!(interaction_ranges(&g).is_none());assert!(refresh(&mut g));g.dim=2;assert!(interaction_ranges(&g).is_none());assert!(refresh(&mut g));g.remote.as_mut().unwrap().session.counters=Arc::new(Mutex::new(None));assert!(interaction_ranges(&g).is_none());}
 #[test]fn unknown_nonfinite_or_unbounded_attributes_refuse_refresh(){let mut g=fixture();for bad in [f64::NAN,f64::INFINITY,0.,65.]{g.player.as_mut().unwrap().entity=bad;assert!(!refresh(&mut g));}assert!(interaction_ranges(&g).is_none());}
}
mod lifecycle {
 use super::policy::{self,Kind,Step,Sequence};use std::sync::{Arc,Mutex};type Session=Arc<Mutex<Option<Arc<()>>>>;
 struct Remote{entity_id:i32,session:RemoteSession}struct RemoteSession{counters:Session}
 struct Settings{back:bool}impl Settings{fn b(&self,_:&str)->bool{self.back}}
 struct Run{owner:&'static str,dimension:u8,entity:i32,session:Session,previous:usize,owned:usize,settings:Settings,sequence:Sequence}
 struct Game<'a>{dim:u8,remote:Option<Remote>,selected:usize,using:Option<()>,holds:Vec<usize>,life:std::marker::PhantomData<&'a ()>}
 struct State{run:Option<Run>,anchor_latched:bool,anchor_press:Option<u8>,token:u64}
 static STATE:Mutex<State>=Mutex::new(State{run:None,anchor_latched:true,anchor_press:None,token:0});
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.lock().unwrap())}
 const CRYSTAL:&str="Crystal Macro";const ANCHOR:&str="Anchor Macro";
 mod sunset{pub fn cancel_turns(_:&str){}}
 mod sunset_player{pub fn gap_eating()->bool{false}}
 mod sunset_combat_control{pub static OWN:std::sync::Mutex<bool>=std::sync::Mutex::new(false);pub fn owns(_:&str)->bool{*OWN.lock().unwrap()}pub fn release(_:&str){*OWN.lock().unwrap()=false;}}
 fn hold(g:&mut Game<'_>,slot:usize){g.selected=slot;g.holds.push(slot);}
 mod implementation{use super::*;LIFECYCLE}
 use implementation::*;
 fn fixture(kind:Kind,step:Step,back:bool)->Game<'static>{let token=Arc::new(Mutex::new(None));let g=Game{dim:0,remote:Some(Remote{entity_id:7,session:RemoteSession{counters:token.clone()}}),selected:8,using:None,holds:vec![],life:std::marker::PhantomData};
  *sunset_combat_control::OWN.lock().unwrap()=true;state(|s|{s.anchor_latched=true;s.anchor_press=Some(1);s.run=Some(Run{owner:if kind==Kind::Anchor{ANCHOR}else{CRYSTAL},dimension:0,entity:7,session:token,previous:0,owned:8,settings:Settings{back},sequence:Sequence::new(kind,step,false)});});g}
 #[test]fn successful_anchor_releases_latch_after_conditional_switch_back(){let mut g=fixture(Kind::Anchor,Step::Done,true);complete(&mut g);assert_eq!(g.holds,[0]);state(|s|{assert!(!s.anchor_latched);assert!(s.run.is_none());assert!(s.anchor_press.is_none());});}
 #[test]fn successful_without_switch_back_does_not_change_user_hand(){let mut g=fixture(Kind::Anchor,Step::Done,false);complete(&mut g);assert!(g.holds.is_empty());state(|s|assert!(!s.anchor_latched));}
 #[test]fn failed_or_cancelled_sequence_does_not_replay_held_intent(){let mut g=fixture(Kind::Anchor,Step::Charge,true);reset(&mut g);state(|s|assert!(s.anchor_latched));}
 #[test]fn old_scene_done_cannot_rearm_new_scene_held_intent(){let mut g=fixture(Kind::Anchor,Step::Done,true);g.dim=1;complete(&mut g);state(|s|assert!(s.anchor_latched));assert!(g.holds.is_empty());}
 #[test]fn crystal_completion_cannot_unlatch_anchor(){let mut g=fixture(Kind::Crystal,Step::Done,true);complete(&mut g);state(|s|assert!(s.anchor_latched));}
 #[test]fn successful_repeat_requires_observed_sequence_completion(){let mut g=fixture(Kind::Anchor,Step::Detonate,true);state(|s|{let run=s.run.as_mut().unwrap();assert_ne!(run.sequence.step,Step::Done);assert!(run.sequence.acknowledged(Step::Detonate));});complete(&mut g);state(|s|assert!(!s.anchor_latched));}
}
'''.replace('POLICY',(REPO/'mixins/sunset/macro_policy.rs').as_posix()).replace('CURSOR',cursor).replace('RANGES',ranges).replace('LIFECYCLE',lifecycle)
    # Keep privacy identical to production children while allowing this parent
    # harness to exercise them; visibility only does not alter function bodies.
    code=code.replace('fn direction(','pub(super) fn direction(').replace('fn camera_click(','pub(super) fn camera_click(').replace('enum CursorEntity','pub(super) enum CursorEntity').replace('fn cursor_entity(','pub(super) fn cursor_entity(').replace('fn crystal_click(','pub(super) fn crystal_click(').replace('fn crosshair_crystal(','pub(super) fn crosshair_crystal(').replace('fn complete(','pub(super) fn complete(')
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-macro-cursor-') as directory:
        rust=Path(directory)/'cursor.rs';exe=Path(directory)/'cursor.exe';rust.write_text(code,encoding='utf-8')
        core=next(args.dependency_dir.glob('libhorizon_core-*.rlib'))
        for stage,command in [('compile',['rustc','--edition=2024','--test',str(rust),'-L','dependency='+str(args.dependency_dir),'--extern','horizon_core='+str(core),'-o',str(exe)]),('run',[str(exe),'--test-threads=1'])]:
            result=subprocess.run(command,capture_output=True,text=True);records.append({'stage':stage,'command':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr});print(result.stdout+result.stderr)
            if result.returncode:raise SystemExit(result.returncode)
    # Separate source-route contract: these routes are NOT compiled native Game.
    assert '!game.creative' not in function(macro,'fn allowed(')
    assert 'super::sunset_explosive_tactics::refresh(game)' in function(macro,'pub(super) fn intercept_use(')
    assert 'super::sunset_explosive_tactics::refresh(game)' in function(macro,'pub(super) fn tick(')
    assert 'super::sunset_explosive_tactics::refresh(game)' in function(macro,'fn queue(')
    assert 'game.reach()' not in macro and 'in_range(eye, click.hit, 4.5)' not in macro
    assert 'complete(game)' in function(macro,'pub(super) fn tick(')
    assert function(macro,'pub(super) fn intercept_use(').index('Blast::anchor(p)')<function(macro,'pub(super) fn intercept_use(').index('sunset_explosive_tactics::refresh(game)')
    print('source-route contract: 1 passed (current range sampling, success-only completion and protected fallback; not compiled)')
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':1,
            'source_sha256':{p.relative_to(REPO).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()for p in [SOURCE/name for name in ['combat_macros.rs','explosive_tactics.rs','macro_policy.rs']]},
            'scope':'Twenty-two production function adapters with pinned native Aabb and actual macro ray/sequence. Game catalog, outline and hand effects are controlled adapters; no fullGame/client/network or live creative/repeat proof.'},indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
