"""Compile the actual Crystal setup loop and state with narrow world seams.

Checks candidate continuity and bounded work. Geometry/material/damage adapters
are explicit; this does not run a native client or reproduce world exposure.
"""
from pathlib import Path
import subprocess,tempfile,json,argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out',type=Path)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[1]
new=(repo/'mixins/sunset/crystal_aura.rs').read_text(encoding='utf-8')
def fn(src,sig):
 a=src.index(sig);b=src.index('{',a);d=1;c=b+1
 while d:d+=(src[c]=='{')-(src[c]=='}');c+=1
 return src[a:c]
pure=new.split('// BEGIN PURE CRYSTAL STATE\n')[1].split('// END PURE CRYSTAL STATE')[0]
support=r'''
use std::sync::Mutex;
use state::{Action,Clock};
#[derive(Clone,Copy,Debug)]struct Rating{full_damage:f64}
#[derive(Debug)]struct Plan{action:Action,target:u32,aim:[f64;3],rating:Rating,click:Option<BaseClick>}
#[derive(Clone,Copy,Debug)]struct BaseClick{point:[f64;3]}
struct Module{auto:bool}impl Module{fn b(&self,_:&str)->bool{self.auto}}
struct Game<'a>{base:[i32;3],place:bool,replaceable:bool,ack:bool,tool:bool,crystal:bool,geometry:bool,damage:f64,evaluations:usize,floor:Option<i32>,phantom:std::marker::PhantomData<&'a ()>}
impl Default for Game<'_>{fn default()->Self{Self{base:[4,0,0],place:true,replaceable:false,ack:true,tool:true,crystal:true,geometry:true,damage:8.0,evaluations:0,floor:None,phantom:std::marker::PhantomData}}}
struct Ledger{ready:bool}impl Ledger{fn can_send(&self,_:[i32;3],_:u64)->bool{self.ready}}
struct State{clock:Clock,retained:Option<Action>,setup:Ledger}
static STATE:Mutex<Option<State>>=Mutex::new(None);
fn st<R>(f:impl FnOnce(&mut State)->R)->R{f(STATE.lock().unwrap().as_mut().unwrap())}
fn reset(){*STATE.lock().unwrap()=Some(State{clock:Clock::default(),retained:None,setup:Ledger{ready:true}});}
fn place_valid(g:&Game<'_>,p:[i32;3])->bool{g.place&&g.ack&&p==g.base}
fn setup_valid(g:&Game<'_>,p:[i32;3])->bool{g.replaceable&&p==g.base}
fn center(p:[i32;3])->[f64;3]{p.map(f64::from)}
fn optimistic(_:[f64;3],_:&[u32])->f64{8.0}
fn crystal_slot(g:&Game<'_>,_:&Module)->Option<usize>{g.crystal.then_some(1)}
fn obsidian_slot(g:&Game<'_>,_:&Module)->Option<usize>{g.tool.then_some(2)}
fn reachable(g:&Game<'_>,_:&Module,a:Action,_:&mut Budget)->Option<[f64;3]>{if !g.geometry{return None;}match a{Action::Place(p)=>Some(center(p)),_=>None}}
fn view_allowed(_: &Game<'_>,_:&Module,_:Action,_:[f64;3])->bool{true}
fn valid_damage(g:&mut Game<'_>,_:&Module,_:[f64;3],_:&[u32],_:&[u32],_:Action,_:&mut Budget)->Option<(Rating,u32)>{g.evaluations+=1;(g.damage>0.0).then_some((Rating{full_damage:g.damage},1))}
fn credit(_: &Game<'_>,_:&Module,_:Action,_:[f64;3])->f64{0.0}
fn keep(best:&mut Option<Plan>,p:Plan){if best.as_ref().is_none_or(|b|p.rating.full_damage>b.rating.full_damage){*best=Some(p);}}
struct Budget;
struct Stage(bool);impl Stage{fn new_crystal(&self)->bool{self.0}}
mod util{use super::*;pub fn block_pos(_: &Game<'_>)->Option<[i32;3]>{Some([0;3])}}
mod tactics{use super::*;pub fn block_range(_: &Game<'_>)->f64{4.5}}
mod sunset_crystal_bot{use super::*;pub struct Steer{pub floor:i32,pub eager:bool}pub fn steer(g:&Game<'_>)->Option<Steer>{g.floor.map(|floor|Steer{floor,eager:false})}}
mod setup{pub fn upgrade(c:f64,b:f64,_:bool)->bool{b>c+1.0}}
mod base_native{use super::*;pub fn plan(g:&Game<'_>,p:[i32;3],_:usize,_:f64,r:&mut usize)->Option<BaseClick>{if *r==0||!g.geometry{return None;}*r-=1;Some(BaseClick{point:center(p)})}}
const SCAN_CELLS:usize=96;const MAX_CANDIDATES:usize=4;
'''.replace('struct Rating{full_damage:f64}','struct Rating{full_damage:f64,look_credit:f64}').replace('Rating{full_damage:g.damage}','Rating{full_damage:g.damage,look_credit:0.0}')
def section(src):
 a=src.index('    if best.is_none()&&stage.new_crystal()');b=src.index('    if let Some(plan)=best {',a)
 return src[a:b]
def module(name,src):
 helpers=fn(new,'fn setup_action(')+'\n'+fn(new,'fn setup_cell(')
 return f'mod {name}{{use super::*;\n'+helpers+'''pub(super) fn run(game:&mut Game<'_>,staged:bool,used:usize)->Option<Plan>{
 let m=&Module{auto:true};let ticks=20;let targets=vec![1];let friends=vec![];let mut budget=Budget;
 let mut best=None;let stage=Stage(!staged);let mut evaluated=used;
 '''+section(src)+'\nbest\n}}\n'
tests=r'''
fn discover(g:&mut Game<'_>)->Plan{for _ in 0..14{if let Some(p)=after::run(g,false,0){return p;}}panic!("real96-cell scan did not discover fixture");}
#[test]fn retained_place_survives_a_complete_real_scan_cycle(){reset();let mut g=Game::default();let p=discover(&mut g);assert_eq!(p.action,Action::Place(g.base));for _ in 0..14{g.evaluations=0;let p=after::run(&mut g,false,3).unwrap();assert_eq!(p.action,Action::Place(g.base));assert_eq!(g.evaluations,1);}}
#[test]fn current_damage_and_geometry_can_invalidate_retained_plan(){reset();let mut g=Game::default();discover(&mut g);g.damage=0.0;assert!(after::run(&mut g,false,0).is_none());assert!(st(|s|s.retained).is_none());g.damage=8.0;discover(&mut g);g.geometry=false;assert!(after::run(&mut g,false,0).is_none());assert!(st(|s|s.retained).is_none());}
#[test]fn pending_base_only_becomes_place_after_valid_observed_world(){reset();let mut g=Game{place:false,replaceable:true,..Game::default()};assert_eq!(discover(&mut g).action,Action::Base(g.base));g.place=true;g.replaceable=false;g.ack=false;assert!(after::run(&mut g,false,0).is_none());st(|s|s.retained=Some(Action::Base(g.base)));g.ack=true;assert_eq!(after::run(&mut g,false,0).unwrap().action,Action::Place(g.base));}
#[test]fn staged_receipt_or_missing_material_suppresses_and_clears_setup(){reset();let mut g=Game::default();discover(&mut g);assert!(after::run(&mut g,true,0).is_none());assert!(st(|s|s.retained).is_none());discover(&mut g);g.crystal=false;assert!(after::run(&mut g,false,0).is_none());assert!(st(|s|s.retained).is_none());}
#[test]fn bot_floor_and_ledger_are_refreshed_instead_of_cached(){reset();let mut g=Game::default();discover(&mut g);g.floor=Some(2);assert!(after::run(&mut g,false,0).is_none());g.floor=None;g.place=false;g.replaceable=true;st(|s|s.retained=Some(Action::Base(g.base)));st(|s|s.setup.ready=false);assert!(after::run(&mut g,false,0).is_none());}
#[test]fn the_fourth_evaluation_is_a_hard_shared_limit(){reset();let mut g=Game::default();discover(&mut g);g.evaluations=0;assert!(after::run(&mut g,false,4).is_none());assert_eq!(g.evaluations,0);}
#[test]fn cached_base_still_requires_a_current_obsidian_stack(){reset();let mut g=Game{place:false,replaceable:true,tool:false,..Game::default()};st(|s|s.retained=Some(Action::Base(g.base)));assert!(after::run(&mut g,false,0).is_none());assert_eq!(g.evaluations,0);assert!(st(|s|s.retained).is_none());}
#[test]fn fresh_rating_and_target_are_recomputed_instead_of_stored_in_retention(){reset();let mut g=Game::default();let first=discover(&mut g);assert_eq!(first.rating.full_damage,8.0);g.damage=3.0;let next=after::run(&mut g,false,0).unwrap();assert_eq!(next.rating.full_damage,3.0);assert_eq!(next.action,first.action);}
'''
crate='#![allow(dead_code)]\n'+pure+support+module('after',new)+tests
with tempfile.TemporaryDirectory() as d:
 p=Path(d);(p/'test.rs').write_text(crate)
 build=subprocess.run(['rustc','--edition=2021','--test',str(p/'test.rs'),'-o',str(p/'test.exe')],text=True,capture_output=True)
 if build.returncode:raise RuntimeError(build.stderr)
 run=subprocess.run([str(p/'test.exe'),'--test-threads=1'],text=True,capture_output=True)
 print(run.stdout);assert run.returncode==0,run.stderr
 assert 's.retained=None' in fn(new,'pub(super) fn suspend(')
 assert 'suspend();' in fn(new,'pub(super) fn reset(')
 assert 'st(|s|s.retained=None);restore_weapon' in new
 assert '.take(MAX_CANDIDATES-1)' in new and 'if evaluated>=MAX_CANDIDATES{break;}' in section(new)
 result={'compiled_tests':8,'source_route_assertions':4,'rustc_warnings':build.stderr,'result':'pass','scope':'actual before/after scan loop and actual setup_action; narrow world/material/damage adapters, no native World/exposure/rotation claim'}
 if args.out:args.out.write_text(json.dumps(result,indent=2)+'\n')
 print('Retained setup: 8 compiled production-loop tests; 4 source-route assertions: PASS')
