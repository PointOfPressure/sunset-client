"""Actual production score/tool/pin functions + pinned native raw tool lookup.
Registry, local/target damage and Game are narrow adapters, not world tests.
"""
from pathlib import Path
import subprocess,tempfile,json,argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('engine',type=Path)
parser.add_argument('--out',type=Path)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[1]
engine=args.engine.resolve()
assert (engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
def block(text,signature):
 a=text.index(signature);b=text.index('{',a);depth=1;c=b+1
 while depth:depth+=(text[c]=='{')-(text[c]=='}');c+=1
 return text[a:c]
city=(repo/'mixins/sunset/city_trap.rs').read_text(encoding='utf-8');items=(engine/'crates/horizon-items/src/items.rs').read_text()
endangered=block((repo/'mixins/sunset/explosive_tactics.rs').read_text(),'pub(super) fn endangered(')
pre=r'''
#![allow(dead_code)]
use std::collections::BTreeMap;
type ItemStack=Stack;
#[derive(Clone)]struct Stack{item:String,count:u32,damage:u32,extra:Option<Extra>}
#[derive(Clone)]struct Extra{other:Vec<u8>}
struct Rule{speed:Option<f32>,correct:Option<bool>,blocks:Vec<String>}
struct Info{max_damage:u32,tool_damage_per_block:Option<u32>,tool:Vec<Rule>,tool_default_speed:f32}
struct Items{entries:BTreeMap<String,Info>}
impl Items{fn get(&self,item:&str)->Option<&Info>{self.entries.get(item)}fn block_matches(&self,a:&str,b:&str)->bool{a==b}
MINING
CORRECT
}
struct Facts{name:String,hardness:f32}
struct Health{value:f32,abs:f32}impl Health{fn health(&self)->f32{self.value}fn absorption(&self)->f32{self.abs}}
struct Player{health:Health}
struct Game<'a>{inventory:Vec<Option<Stack>>,items:Items,player:Option<Player>,creative:bool,own:f64,target:f64,marker:std::marker::PhantomData<&'a ()>}
impl Game<'_>{fn items(&self)->&Items{&self.items}}
struct Module;
struct Target{pos:[f64;3]}
mod util{use super::*;pub fn facts(_: &Game<'_>,_:[i32;3])->Facts{Facts{name:"minecraft:obsidian".into(),hardness:50.0}}}
mod sunset_explosive_policy{pub struct Budget;}
mod sunset_explosive_damage{use super::*;#[derive(Clone,Copy)]pub struct Blast{pub center:[f64;3],pub ignored:Option<[i32;3]>}impl Blast{pub fn explosion(center:[f64;3],_:f32)->Self{Self{center,ignored:None}}pub fn ignoring(mut self,p:[i32;3])->Self{self.ignored=Some(p);self}}
pub struct Prediction{pub estimate:f64}pub fn local_damage(g:&mut Game<'_>,b:Blast,_:&mut sunset_explosive_policy::Budget)->Option<Prediction>{assert_eq!(b.center,[3.5,121.0,0.5]);assert_eq!(b.ignored,Some([3,121,0]));Some(Prediction{estimate:g.own})}}
mod sunset_explosive_tactics{use super::*;
ENDANGERED
pub fn friend_damage(g:&Game<'_>,_:&Target,_:sunset_explosive_damage::Blast,_:&mut sunset_explosive_policy::Budget)->Option<f64>{Some(g.target)}}
mod production{use super::*;
TOOL
SCORE
PIN
pub(super) fn pick(g:&Game<'_>)->Option<usize>{tool(g,[3,121,0])}
pub(super) fn score(g:&mut Game<'_>)->Option<f64>{city_score(g,&Module,&Target{pos:[4.5,121.0,0.5]},[3,121,0],&mut sunset_explosive_policy::Budget)}
pub(super) fn pin(p:[f64;3])->bool{same_hole(&Target{pos:p},[4,121,0])}}
fn fixture()->Game<'static>{let mut entries=BTreeMap::new();let mut inventory=vec![None;9];for (slot,name,speed) in [(0,"diamond_pickaxe",8.0),(1,"netherite_pickaxe",9.0),(2,"wooden_pickaxe",2.0)]{let id=format!("minecraft:{name}");entries.insert(id.clone(),Info{max_damage:100,tool_damage_per_block:Some(1),tool:vec![Rule{speed:Some(speed),correct:Some(slot<2),blocks:vec!["minecraft:obsidian".into()]}],tool_default_speed:1.0});inventory[slot]=Some(Stack{item:id,count:1,damage:0,extra:None});}Game{inventory,items:Items{entries},player:Some(Player{health:Health{value:20.0,abs:0.0}}),creative:false,own:4.0,target:12.0,marker:std::marker::PhantomData}}
#[test]fn defended_target_minus_half_observed_self_drives_rank(){let mut g=fixture();assert_eq!(production::score(&mut g),Some(10.0));g.target=6.0;assert_eq!(production::score(&mut g),Some(4.0));}
#[test]fn exact_reference_endangerment_boundary_and_absorption(){let mut g=fixture();g.own=10.0;g.player.as_mut().unwrap().health.value=13.5;assert!(production::score(&mut g).is_none());g.player.as_mut().unwrap().health.abs=0.5;assert_eq!(production::score(&mut g),Some(7.0));}
#[test]fn creative_and_nonfinite_values_have_explicit_guard_behavior(){let mut g=fixture();g.own=17.0;assert!(production::score(&mut g).is_none());g.creative=true;assert_eq!(production::score(&mut g),Some(3.5));g.own=f64::NAN;assert!(production::score(&mut g).is_none());g.own=4.0;g.target=f64::INFINITY;assert!(production::score(&mut g).is_none());}
#[test]fn no_arbitrary_old_self_cap_or_hidden_minimum_remains(){let mut g=fixture();g.own=13.0;g.target=8.0;assert_eq!(production::score(&mut g),Some(1.5));g.target=6.5;assert!(production::score(&mut g).is_none());}
#[test]fn actual_native_raw_tool_speed_selects_fastest_suitable_stack(){let g=fixture();assert_eq!(production::pick(&g),Some(1));assert_eq!(g.items.mining_speed(g.inventory[0].as_ref(),"minecraft:obsidian"),8.0);assert!(!g.items.correct_for_drops(g.inventory[2].as_ref(),"minecraft:obsidian"));}
#[test]fn last_durability_and_unknown_components_remain_resource_safeguards(){let mut g=fixture();g.inventory[1].as_mut().unwrap().damage=99;assert_eq!(production::pick(&g),Some(0));g.inventory[0].as_mut().unwrap().extra=Some(Extra{other:vec![1]});assert!(production::pick(&g).is_none());}
#[test]fn empty_stack_and_unsuitable_tools_cannot_start_native_mining(){let mut g=fixture();g.inventory[1].as_mut().unwrap().count=0;g.inventory[0]=None;assert!(production::pick(&g).is_none());}
#[test]fn target_pin_uses_same_column_and_one_block_vertical_tolerance(){assert!(production::pin([4.1,120.0,0.1]));assert!(production::pin([4.9,122.9,0.9]));assert!(!production::pin([5.0,121.0,0.5]));assert!(!production::pin([4.5,123.0,0.5]));}
'''
crate=pre.replace('MINING',block(items,'pub fn mining_speed(')).replace('CORRECT',block(items,'pub fn correct_for_drops(')).replace('ENDANGERED',endangered).replace('TOOL',block(city,'fn tool(')).replace('SCORE',block(city,'fn city_score(')).replace('PIN',block(city,'fn same_hole('))
with tempfile.TemporaryDirectory() as tmp:
 p=Path(tmp);(p/'test.rs').write_text(crate)
 build=subprocess.run(['rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test.exe')],text=True,capture_output=True)
 assert build.returncode==0,build.stderr
 run=subprocess.run([str(p/'test.exe')],text=True,capture_output=True);print(run.stdout);assert run.returncode==0,run.stderr
 tick=block(city,'fn city_tick(')
 assert tick.index('tool(game, mine.block)')<tick.index('if !city_basic(game)')<tick.index('city_face(game,mine.block,range)')
 assert tick.index('if !held {sunset::turn')<tick.index('let continuing = city_current()')
 assert 'tick.saturating_sub(mine.started) >= 200' in tick
 assert 'game.native_playing()' not in block(city,'fn city_live(')
 assert 'C::Login(_)|C::Respawn(_)|C::Disconnect(_)' in block(city,'pub(super) fn packet(')
 assert '"Notifications",true' in block(city,'pub(super) fn defaults(').split('Module::new(TRAP')[0]
 for file in ['city_trap.rs','city_mining.rs']:
  parse=subprocess.run(['rustfmt','--edition','2024','--emit','stdout',str(repo/'mixins/sunset'/file)],text=True,capture_output=True);assert parse.returncode==0,parse.stderr
 result=json.dumps({'compiled_tests':8,'source_route_assertions':6,'warnings':build.stderr,'rustfmt_parse_files':2,'scope':'actual production score/tool/pin + pinned raw item speed/correctness; narrow registry/Game/damage adapters; no world or nativeclient claim'},indent=2)+'\n'
 if args.out:args.out.write_text(result)
 print('City parity: 8 compiled actual-production tests; 6 source-route assertions: PASS')
