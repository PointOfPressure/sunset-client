"""Actual Optimizer tick/sent/scene/render-mask functions and native level value.
Game, transport, item registry and base attribute lookup are narrow adapters.
No world, client, render device or RPC is started.
"""
from pathlib import Path
import subprocess,tempfile,json,argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('engine',type=Path)
parser.add_argument('--out',type=Path)
parser.add_argument('--dependency-dir',type=Path,required=True)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[1]
engine=args.engine.resolve()
assert (engine/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
deps=args.dependency_dir.resolve()
def body(s,sig):
 a=s.index(sig);b=s.index('{',a);d=1;c=b+1
 while d:d+=(s[c]=='{')-(s[c]=='}');c+=1
 return s[a:c]
s=(repo/'mixins/sunset/crystal_optimizer.rs').read_text();macro=(repo/'mixins/sunset/combat_macros.rs').read_text()
level=body((engine/'crates/horizon-items/src/enchant.rs').read_text(),'pub fn level_value(')
helpers='\n'.join(body(s,sig) for sig in ['fn state<R>','fn module(','fn same(','fn session(','fn window(','fn active(','pub(super) fn reset(','fn swinging_hand(','pub(super) fn tick(','pub(super) fn sent(','pub(super) fn packet(','pub(super) fn hidden_id(','pub(super) fn hidden_position(','pub(super) fn has_predictions(','pub(super) fn duplicate('])
state=body(s,'struct State {')
pre=r'''
#![allow(dead_code)]
extern crate self as horizon_client;
use std::{any::Any,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},collections::BTreeMap};
#[path="POLICY"]mod policy;
type Dim=u8;
#[derive(Clone)]struct Module{on:bool}
mod sunset{use super::*;pub const ID:&str="sunset";pub static ON:AtomicBool=AtomicBool::new(true);pub struct State{module:Module}impl State{pub fn module(&self,_:&str)->Option<&Module>{Some(&self.module)}}pub fn with<R>(f:impl FnOnce(&State)->R)->R{f(&State{module:Module{on:ON.load(Ordering::SeqCst)}})}}
pub mod enchant{use serde_json::Value;
LEVEL
}
pub mod items{pub type ItemStack=crate::Stack;}
pub mod spear{pub struct Kinetic;impl Kinetic{pub fn of(s:&str)->Option<Self>{s.ends_with("_spear").then_some(Self)}}}
pub mod cooldowns{use super::*;pub fn group(s:&Stack,_:Option<&()>)->bool{s.cooldown}}
struct Cooldowns;impl Cooldowns{fn is_on_cooldown(&self,group:&bool)->bool{*group}}
#[derive(Clone)]pub struct Stack{item:String,count:u32,base:Option<f32>,extra:Option<Extra>,cooldown:bool,enchants:Vec<(String,u32)>}
impl Stack{fn enchantments(&self)->&[(String,u32)]{&self.enchants}}
#[derive(Clone)]struct Extra{other:Vec<u8>}
struct Info{use_cooldown:Option<()>}
struct Definition{effects:serde_json::Value}
struct Items{info:Info,enchantments:BTreeMap<String,Definition>}impl Items{fn get(&self,s:&str)->Option<&Info>{(s!="unknown").then_some(&self.info)}}
struct Health(f32);impl Health{fn health(&self)->f32{self.0}}
struct Player{health:Health,auto_spin_attack_ticks:u32,range:Option<f64>}
impl Player{fn attribute(&mut self,name:&str)->Result<f64,()>{assert_eq!(name,"minecraft:entity_interaction_range");self.range.ok_or(())}}
struct Net{counters:Arc<u32>}struct Remote{entity_id:i32,session:Net}
#[derive(Clone)]struct V3{x:f64,y:f64,z:f64}
#[derive(Clone)]struct Crystal{id:u32,position:V3,invulnerable:bool}
struct Border(bool);impl Border{fn contains(&self,_:f64,_:f64)->bool{self.0}}
struct Game<'a>{remote:Option<Remote>,dim:Dim,player:Option<Player>,crystals:Vec<Crystal>,selected:usize,inventory:Vec<Option<Stack>>,ticks:u64,creative:bool,
  native:bool,master:bool,spec:bool,eye:[f64;3],effect:f32,rtt:u32,items:Items,border:Border,cooldowns:Cooldowns,marker:std::marker::PhantomData<&'a ()>}
impl Game<'_>{fn native_window(&self)->bool{self.native}fn native_on(&self,_:&str)->bool{self.master}fn spectator(&self)->bool{self.spec}fn eye_position(&self)->Option<[f64;3]>{Some(self.eye)}fn items(&self)->&Items{&self.items}fn held(&self)->Option<&Stack>{self.inventory.get(self.selected)?.as_ref()}fn effect_attack_bonus(&self)->f32{self.effect}}
mod sunset_prediction{pub fn rtt(g:&crate::Game<'_>)->u32{g.rtt}}
mod sunset_combat_macros{use super::*;
ENCHANT_HELPER
}
struct Attack{id:i32}struct Carried{slot:i32}enum S{Attack(Attack),SetCarriedItem(Carried),Other}
struct Remove{ids:Vec<i32>}enum C{RemoveEntities(Remove),Login(()),Respawn(()),Disconnect(()),Other}
mod production{use super::*;use policy::{History,Masks};
 const NAME:&str="Crystal Optimizer";type Session=(i32,Dim,Arc<dyn Any+Send+Sync>);
 #[derive(Default)]STATE_TYPE
 static STATE:Mutex<Option<State>>=Mutex::new(None);static HAS_MASKS:AtomicBool=AtomicBool::new(false);
 // Explicit item/base attribute seam; the actual forwarded eligibility,
 // history/enchant decision, mask timing and scene functions are below.
 fn hand_base(g:&Game<'_>,slot:usize)->Option<f32>{g.inventory.get(slot)?.as_ref().map_or(Some(1.0),|s|s.base)}
 ACTUAL_HELPERS
 pub(super) fn count()->usize{state(|s|s.masks.count())}
 pub(super) fn stubborn()->usize{state(|s|s.masks.stubborn())}
}
fn fixture()->Game<'static>{sunset::ON.store(true,Ordering::SeqCst);production::reset();Game{remote:Some(Remote{entity_id:1,session:Net{counters:Arc::new(1)}}),dim:0,player:Some(Player{health:Health(20.0),auto_spin_attack_ticks:0,range:Some(6.0)}),
 crystals:vec![Crystal{id:7,position:V3{x:5.5,y:121.0,z:0.5},invulnerable:true}],selected:0,inventory:vec![None;9],ticks:100,creative:false,native:true,master:true,spec:false,eye:[0.5,122.62,0.5],effect:0.0,rtt:2,
 items:Items{info:Info{use_cooldown:None},enchantments:BTreeMap::new()},border:Border(true),cooldowns:Cooldowns,marker:std::marker::PhantomData}}
fn next(g:&mut Game<'_>){g.ticks+=1;let tick=g.ticks;production::tick(g,tick);}
fn hit(g:&mut Game<'_>){let tick=g.ticks;production::tick(g,tick);production::sent(g,&S::Attack(Attack{id:7}));}
fn stack(base:f32)->Stack{Stack{item:"minecraft:diamond_sword".into(),count:1,base:Some(base),extra:None,cooldown:false,enchants:vec![]}}

mod game {
 pub(super) type Game<'a>=crate::Game<'a>;
 pub(super) mod sunset {pub(crate) use crate::sunset::{ID,with};}
 pub(super) mod sunset_crystal_optimizer {pub(crate) fn tick(g:&mut super::Game<'_>,tick:u64){crate::production::tick(g,tick);}}
}
include!("HOOK_ACTUAL");
#[test]fn native_increment_attack_post_phase_refreshes_without_allowing_previous_tick_cache(){
 let mut g=fixture();let old=g.ticks;production::tick(&mut g,old);
 g.ticks+=1;production::sent(&g,&S::Attack(Attack{id:7}));assert_eq!(production::count(),0,"previous post-tick cache is denied");
 sunset_optimizer_before_attack(&mut g);production::sent(&g,&S::Attack(Attack{id:7}));assert_eq!(production::count(),1);
 assert!(!production::hidden_id(&g,7,[5.5,121.,0.5]));let now=g.ticks;production::tick(&mut g,now);
 g.ticks+=1;let next=g.ticks;production::tick(&mut g,next);assert!(production::hidden_id(&g,7,[5.5,121.,0.5]));
 assert_eq!(g.crystals.len(),1);
}
#[test]fn actual_attribute_change_and_unknown_at_attack_replace_same_tick_positive_cache(){
 for range in [Some(1.),None,Some(f64::NAN)] {
  let mut g=fixture();let t=g.ticks;production::tick(&mut g,t);
  g.player.as_mut().unwrap().range=range;sunset_optimizer_before_attack(&mut g);
  production::sent(&g,&S::Attack(Attack{id:7}));assert_eq!(production::count(),0);
 }
 let mut g=fixture();let t=g.ticks;production::tick(&mut g,t);
 g.remote.as_mut().unwrap().session.counters=Arc::new(1);
 assert!(!production::has_predictions(&g));g.player.as_mut().unwrap().range=Some(1.);
 sunset_optimizer_before_attack(&mut g);production::sent(&g,&S::Attack(Attack{id:7}));assert_eq!(production::count(),0);
 sunset::ON.store(false,Ordering::SeqCst);g.player.as_mut().unwrap().range=Some(6.);
 sunset_optimizer_before_attack(&mut g);production::sent(&g,&S::Attack(Attack{id:7}));assert_eq!(production::count(),0);
}
'''
crate=pre.replace('HOOK_ACTUAL',(repo/'mixins/sunset/optimizer_attack.rs').as_posix()).replace('POLICY',(repo/'mixins/sunset/city_policy.rs').as_posix()).replace('LEVEL',level).replace('ENCHANT_HELPER',body(macro,'pub(super) fn enchant_damage(')).replace('STATE_TYPE',state).replace('ACTUAL_HELPERS',helpers)
with tempfile.TemporaryDirectory() as tmp:
 p=Path(tmp);(p/'test.rs').write_text(crate)
 candidates=sorted(deps.glob('libserde_json-*.rlib'));assert candidates
 build=subprocess.run(['rustc','--edition=2024','--test',str(p/'test.rs'),'--extern','serde_json='+str(candidates[0]),'-L','dependency='+str(deps),'-o',str(p/'test.exe')],text=True,capture_output=True)
 assert build.returncode==0,build.stderr
 run=subprocess.run([str(p/'test.exe'),'--test-threads=1'],text=True,capture_output=True);print(run.stdout);assert run.returncode==0,run.stderr
 remote=(engine/'crates/horizon-client/src/game/remote.rs').read_text()
 level_body=body(remote,'fn tick_remote_level(')
 assert level_body.index('self.ticks += 1')<level_body.index('self.remote_handle_input()')
 remote_tick=body(remote,'pub(super) fn tick_remote(')
 assert remote_tick.index('self.tick_remote_level()?')<remote_tick.index('self.native_tick()')
 print('source-route contract: 2 passed (native increment/input/post ordering; not compiled)')
 result={'compiled_tests':2,'source_contracts':2,'warnings':build.stderr,'scope':'Actual integrated native-attack HEAD helper and Optimizer cache/sent/tick kernels; native phase ordering pinned by two source contracts. Game/transport/attribute backend are explicit adapters; no game or GPU.'}
 if args.out:args.out.write_text(json.dumps(result,indent=2)+'\n')
