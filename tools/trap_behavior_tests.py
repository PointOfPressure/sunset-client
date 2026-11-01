"""Actual Trap fresh placement loop/native delegation and target selectors.
Native use/prediction and Game are explicit seams; real clip is tested separately.
"""
from pathlib import Path
import subprocess,tempfile,json,argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('engine',type=Path)
parser.add_argument('--out',type=Path)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[1]
engine=args.engine.resolve()
assert (engine/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
def body(s,sig):
 a=s.index(sig);b=s.index('{',a);d=1;c=b+1
 while d:d+=(s[c]=='{')-(s[c]=='}');c+=1
 return s[a:c]
city=(repo/'mixins/sunset/city_trap.rs').read_text();bridge=(repo/'mixins/sunset/city_mining.rs').read_text()
tick=body(city,'fn trap_tick(');a=tick.index('        let mut budget=96;');b=tick.index('        if game.selected == slot {',a);loop=tick[a:b]
pre=r'''
#![allow(dead_code)]
extern crate self as horizon_client;
use std::{collections::{BTreeMap,HashSet},marker::PhantomData,sync::{Mutex,atomic::{AtomicBool,Ordering}}};
#[path="POLICY"]mod policy;
pub mod first_person{pub enum Hand{Main}}
mod winit{pub mod keyboard{#[derive(Hash,PartialEq,Eq)]pub enum KeyCode{KeyQ,KeyF}}}
#[derive(Clone,Copy,Debug,PartialEq)]struct Hit{pos:[i32;3],face:[i32;3],point:[f64;3]}
#[derive(Clone,Copy)]struct Face{block:[i32;3],normal:[i32;3],point:[f64;3]}
#[derive(Clone)]struct Stack{count:u32}
struct Pos{x:i32,y:i32,z:i32}
#[derive(Clone,Copy,PartialEq)]struct BlockStateId(u32);
struct World{blocks:BTreeMap<[i32;3],BlockStateId>}impl World{fn block(&self,x:i32,y:i32,z:i32)->BlockStateId{self.blocks.get(&[x,y,z]).copied().unwrap_or(BlockStateId(0))}}
struct Remote{predictions:BTreeMap<[i32;3],()>}
struct Game<'a>{world:World,inventory:Vec<Option<Stack>>,selected:usize,creative:bool,remote:Option<Remote>,forced_hit:Option<Hit>,ticks:u64,
  acted:bool,native_reject:bool,native_calls:usize,punches:usize,contact:bool,using:Option<()>,carried:Option<()>,riding:Option<u32>,mining:Option<()>,keys:HashSet<winit::keyboard::KeyCode>,left:bool,right:bool,marker:PhantomData<&'a ()>}
impl Game<'_>{fn eye_position(&self)->Option<[f64;3]>{Some([0.5,2.62,0.5])}
 fn remote_can_place(&mut self,h:&Hit,s:&Stack)->Option<(Pos,())>{let pos=std::array::from_fn::<_,3,_>(|i|h.pos[i]+h.face[i]);(s.count>0&&self.world.block(h.pos[0],h.pos[1],h.pos[2]).0!=0&&self.world.block(pos[0],pos[1],pos[2]).0==0).then_some((Pos{x:pos[0],y:pos[1],z:pos[2]},()))}
 // This is a narrow native-use seam. Production bridge calls the real
 // pinned remote_use_block, including its validated native prediction.
 fn remote_use_block(&mut self,_:first_person::Hand)->bool{self.native_calls+=1;if self.native_reject{return true;}let hit=self.forced_hit.unwrap();let stack=self.inventory[self.selected].clone().unwrap();let Some((p,_))=self.remote_can_place(&hit,&stack) else{return true;};let pos=[p.x,p.y,p.z];self.world.blocks.insert(pos,BlockStateId(3));self.remote.as_mut().unwrap().predictions.insert(pos,());self.acted=true;if !self.creative{let stack=self.inventory[self.selected].as_mut().unwrap();stack.count-=1;if stack.count==0{self.inventory[self.selected]=None;}}true}
 fn send(&mut self,_:S){self.punches+=1;}}
struct Empty;enum S{Punch(Empty)}
struct Module{rotate:bool}impl Module{fn b(&self,_:&str)->bool{self.rotate}}
mod sunset{pub struct State;impl State{pub fn is_on(&self,_:&str)->bool{false}}pub fn with<R>(f:impl FnOnce(&State)->R)->R{f(&State)}}
mod sunset_player{pub fn gap_eating()->bool{false}}mod sunset_render{pub fn camera_takes_mouse()->bool{false}}
mod sunset_melee_assist{pub fn acted(g:&crate::Game<'_>)->bool{g.acted}}
mod util{pub fn inventory_idle(_: &crate::Game<'_>)->bool{true}pub fn center_of(p:[i32;3])->[f64;3]{p.map(|v|f64::from(v)+0.5)}}
mod remote{pub mod sunset_city_mining{use crate::*;
SCOPED
DROP
PREVIEW
PLACE
pub fn busy(_: &Game<'_>)->bool{false}
}}
#[derive(Default)]struct State{ledger:policy::Ledger,placements:u64}
static STATE:Mutex<Option<State>>=Mutex::new(None);
fn state<R>(f:impl FnOnce(&mut State)->R)->R{f(STATE.lock().unwrap().get_or_insert_with(State::default))}
fn can_fill(g:&Game<'_>,p:[i32;3])->bool{g.world.block(p[0],p[1],p[2]).0==0}
// Geometry is independently covered by five pinned native-clip tests. This
// seam exposes support only once the native-use backend has predicted it.
fn support(g:&Game<'_>,cell:[i32;3],_:f64,budget:&mut usize)->Option<Face>{if *budget==0{return None;}*budget-=1;let below=[cell[0],cell[1]-1,cell[2]];(g.world.block(below[0],below[1],below[2]).0!=0).then_some(Face{block:below,normal:[0,1,0],point:[f64::from(cell[0])+0.5,f64::from(cell[1]),f64::from(cell[2])+0.5]})}
fn trap_contact(g:&Game<'_>,f:Face,_:f64)->Option<[f64;3]>{g.contact.then_some(f.point)}
mod production{use super::*;
HANDS_FREE
TRAP_HANDS
pub(super) fn run(game:&mut Game<'_>,plan:Vec<[i32;3]>,limit:usize,rotate:bool){let range=64.0;let slot=0;let snapshot=Module{rotate};
LOOP
}
pub(super) fn free(g:&Game<'_>)->bool{trap_hands_free(g)}}
fn fixture(count:u32)->Game<'static>{*STATE.lock().unwrap()=None;let mut blocks=BTreeMap::new();for x in -2..=40{for z in -2..=3{blocks.insert([x,0,z],BlockStateId(1));}}Game{world:World{blocks},inventory:vec![Some(Stack{count});9],selected:0,creative:false,remote:Some(Remote{predictions:BTreeMap::new()}),forced_hit:None,ticks:100,acted:false,native_reject:false,native_calls:0,punches:0,contact:true,using:None,carried:None,riding:None,mining:None,keys:HashSet::new(),left:false,right:false,marker:PhantomData}}
fn ring()->Vec<[i32;3]>{policy::trap_cells([3.2,1.0,0.2],[3.8,2.8,0.8],false).unwrap()}
#[test]fn eight_budget_uses_native_prediction_to_build_four_new_head_supports_same_tick(){let mut g=fixture(64);production::run(&mut g,ring(),8,false);assert_eq!(g.native_calls,8);assert_eq!(g.punches,8);assert_eq!(g.inventory[0].as_ref().unwrap().count,56);assert!(g.acted);for cell in ring(){assert_eq!(g.world.block(cell[0],cell[1],cell[2]).0,3);}}
#[test]fn accepted_other_action_blocks_the_first_use_without_packets_or_world_changes(){let mut g=fixture(64);g.acted=true;production::run(&mut g,ring(),8,false);assert_eq!(g.native_calls,0);assert_eq!(g.punches,0);assert_eq!(g.inventory[0].as_ref().unwrap().count,64);}
#[test]fn rotated_use_requires_actual_sent_contact_and_respects_one_budget(){let mut g=fixture(64);g.contact=false;production::run(&mut g,ring(),1,true);assert_eq!(g.native_calls,0);g.contact=true;production::run(&mut g,ring(),1,true);assert_eq!(g.native_calls,1);}
#[test]fn creative_count_one_is_reusable_but_survival_never_spends_past_stack(){let mut g=fixture(1);g.creative=true;production::run(&mut g,ring(),8,false);assert_eq!(g.native_calls,8);assert_eq!(g.inventory[0].as_ref().unwrap().count,1);let mut g=fixture(1);production::run(&mut g,ring(),8,false);assert_eq!(g.native_calls,1);assert!(g.inventory[0].is_none());}
#[test]fn at_most_thirty_six_cells_are_visited_even_if_later_cell_is_usable(){let mut g=fixture(64);let mut plan=vec![[1000,1,1000];36];plan.push([1,1,0]);production::run(&mut g,plan,8,false);assert_eq!(g.native_calls,0);}
#[test]fn rejected_native_use_is_not_counted_and_scoped_crosshair_is_restored(){let mut g=fixture(64);let old=Hit{pos:[9,9,9],face:[1,0,0],point:[9.0;3]};g.forced_hit=Some(old);g.native_reject=true;production::run(&mut g,ring(),1,false);assert_eq!(g.forced_hit,Some(old));assert_eq!(g.punches,0);assert_eq!(state(|s|s.placements),0);assert_eq!(g.inventory[0].as_ref().unwrap().count,64);}
#[test]fn held_buttons_alone_do_not_block_but_real_use_mining_and_swap_input_do(){let mut g=fixture(64);g.left=true;g.right=true;assert!(production::free(&g));g.mining=Some(());assert!(!production::free(&g));g.mining=None;g.using=Some(());assert!(!production::free(&g));g.using=None;g.keys.insert(winit::keyboard::KeyCode::KeyF);assert!(!production::free(&g));}
'''
crate=pre.replace('POLICY',(repo/'mixins/sunset/city_policy.rs').as_posix()).replace('SCOPED',body(bridge,"struct ScopedHit<'a, 'r>" )).replace('DROP',body(bridge,"impl Drop for ScopedHit<'_, '_>" )).replace('PREVIEW',body(bridge,'pub(crate) fn preview(')).replace('PLACE',body(bridge,'pub(crate) fn place(')).replace('HANDS_FREE',body(city,'fn hands_free(')).replace('TRAP_HANDS',body(city,'fn trap_hands_free(')).replace('LOOP',loop)
# Reuse the exact current target selector body with only registry/player seams.
selector=r'''
#![allow(dead_code)]
use std::collections::BTreeMap;
#[path="POLICY"]mod policy;
#[derive(Clone)]struct Target{id:u32,pos:[f64;3],lo:[f64;3],hi:[f64;3]}
struct Kind{id:&'static str}struct Mob{id:u32,removed:bool,live:bool,enemy:bool,kind:Kind,pos:[f64;3],name:String}impl Mob{fn alive(&self)->bool{self.live}}
struct Remote{entity_id:i32}struct Module;
struct Game<'a>{mobs:Vec<Mob>,remote:Option<Remote>,feet:[f64;3],marker:std::marker::PhantomData<&'a ()>}
impl Game<'_>{fn remote_player_name(&self,id:u32)->Option<&str>{self.mobs.iter().find(|m|m.id==id).map(|m|m.name.as_str())}}
mod sunset{pub struct State{pub friends:Vec<String>}pub fn with<R>(f:impl FnOnce(&State)->R)->R{f(&State{friends:vec![]})}}
mod util{use super::*;pub fn player_pos(g:&Game<'_>)->Option<[f64;3]>{Some(g.feet)}pub fn player_enemy(_: &Game<'_>,m:&Mob)->bool{m.enemy}pub fn mob_target(m:&Mob)->Target{Target{id:m.id,pos:m.pos,lo:m.pos.map(|v|v-0.3),hi:m.pos.map(|v|v+0.3)}}pub fn dist(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|(a[i]-b[i]).powi(2)).sum::<f64>().sqrt()}}
SELECTOR
fn fixture()->Game<'static>{Game{mobs:vec![],remote:Some(Remote{entity_id:1}),feet:[0.0,121.0,0.0],marker:std::marker::PhantomData}}
fn mob(id:u32,pos:[f64;3])->Mob{Mob{id,removed:false,live:true,enemy:true,kind:Kind{id:"minecraft:player"},pos,name:format!("enemy{id}")}}
#[test]fn target_range_is_inclusive_feet_distance_not_eye_to_feet(){let mut g=fixture();g.mobs.push(mob(2,[2.5,121.0,0.0]));assert_eq!(opponents_with(&g,&Module,1.0+1.5,None).len(),1);assert!(opponents_with(&g,&Module,2.49,None).is_empty());}
#[test]fn nearest_player_uses_native_enemy_filter_before_ranking(){let mut g=fixture();let mut friend=mob(2,[0.5,121.0,0.0]);friend.enemy=false;g.mobs.push(friend);g.mobs.push(mob(3,[1.5,121.0,0.0]));g.mobs.push(mob(4,[1.0,121.0,0.0]));let result=opponents_with(&g,&Module,5.5,None);assert_eq!(result.iter().map(|t|t.id).collect::<Vec<_>>(),[4,3]);}
#[test]fn deferred_only_target_cannot_fall_back_to_a_different_or_malformed_player(){let mut g=fixture();g.mobs.push(mob(2,[1.0,121.0,0.0]));g.mobs.push(mob(3,[f64::NAN,121.0,0.0]));assert!(opponents_with(&g,&Module,5.5,Some(3)).is_empty());assert!(opponents_with(&g,&Module,5.5,Some(4)).is_empty());}
'''.replace('POLICY',(repo/'mixins/sunset/city_policy.rs').as_posix()).replace('SELECTOR',body(city,'fn opponents_with('))
results=[]
for name,code in [('placement',crate),('target',selector)]:
 with tempfile.TemporaryDirectory() as tmp:
  p=Path(tmp);(p/'test.rs').write_text(code)
  compile=subprocess.run(['rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test.exe')],text=True,capture_output=True)
  assert compile.returncode==0,compile.stderr
  run=subprocess.run([str(p/'test.exe'),'--test-threads=1'],text=True,capture_output=True);print(run.stdout);assert run.returncode==0,run.stderr
  results.append({'kind':name,'warnings':compile.stderr})
assert tick.count('m.n("Range")')==1 and tick.count('snapshot.n("Range")')==1
assert 'next_look(game,desired,0.0)' in tick and 'trap_contact(game,selected[0].2,range)' in tick
wire=(repo/'mixins/sunset/combat_wire.rs').read_text();assert 'S::UseItem(_)|S::UseItemOn(_)' in wire and 'note_action(game)' in wire
native=(engine/'crates/horizon-client/src/game/remote.rs').read_text()
assert 'remote_use_item_on(hand)' in body(native,'fn remote_use_block(')
result=json.dumps({'new_compiled_tests':10,'source_contracts':4,'runs':results,'scope':'Actual fresh placement loop + ScopedHit/native-use delegation bridge + current target/hands-free selector; native use/world/registry seams explicit. Geometry verified independently against actual pinned clip.'},indent=2)+'\n'
if args.out:args.out.write_text(result)
print('source-route contract: 4 passed (target-only Range, bounded actual sent turn, accepted-action observer, native use delegation; not compiled)')
