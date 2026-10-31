"""Actual pinned outline traversal + production Trap face/support/order routines.

Uses exact unit-cube outlines for a bounded stone/obsidian fixture and immediate
observed-placement ACKs. Does not start a client, GPU or connection, simulate
native inventory use, or claim live jump/ACK behavior. Elevated eye123.65 is a
geometry seam within an ordinary native jump, not a packet/velocity injection.
"""
from pathlib import Path
import argparse,subprocess,tempfile

def function(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1
    for index in range(opening+1,len(text)):
        depth+=(text[index]=='{')-(text[index]=='}')
        if not depth:return text[start:index+1]
    raise AssertionError(signature)

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('engine',type=Path)
    args=parser.parse_args();engine=args.engine.resolve()
    assert (engine/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    repository=Path(__file__).resolve().parents[1]
    native=(engine/'crates/horizon-client/src/game.rs').read_text(encoding='utf-8')
    city=(repository/'mixins/sunset/city_trap.rs').read_text(encoding='utf-8')
    clip=function(native,'fn clip_ray(&self, origin: [f64; 3], dir: [f64; 3], reach: f64)')
    helpers='\n'.join(function(city,s) for s in ['fn face_aim_with(','fn safe_support(','fn support('])
    policy=(repository/'mixins/sunset/city_policy.rs').as_posix()
    pre=r'''
#![allow(dead_code)]
extern crate self as horizon_client;
pub mod blocks{#[derive(PartialEq)]pub enum RenderShape{Model}}
use std::{collections::HashSet,marker::PhantomData};
#[path="POLICY"]mod policy;
#[derive(PartialEq)]enum Fluid{None}
struct Fact{fluid:Fluid,waterlogged:bool,name:String,render_shape:blocks::RenderShape,
 replaceable:bool,block_entity:bool,fluid_level:u8}
impl Fact{fn has_class(&self,_:&str)->bool{false}}
struct Facts{solid:Fact,air:Fact}
impl Facts{fn get(&self,s:u32)->&Fact{if s==0{&self.air}else{&self.solid}}fn is_air(&self,s:u32)->bool{s==0}}
struct Res{facts:Facts}impl Res{fn facts(&self)->&Facts{&self.facts}}
struct World{blocks:HashSet<[i32;3]>}impl World{
 fn block(&self,x:i32,y:i32,z:i32)->u32{u32::from(self.blocks.contains(&[x,y,z]))}
 fn loaded(&self,_:i32,_:i32)->bool{true}}
struct Game<'a>{world:World,res:Res,eye:[f64;3],marker:PhantomData<&'a ()>}
#[derive(Clone,Copy)]struct Hit{pos:[i32;3],face:[i32;3],point:[f64;3]}
#[derive(Clone,Copy)]struct Face{block:[i32;3],normal:[i32;3],point:[f64;3]}
mod util{
 pub fn dist(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|(a[i]-b[i]).powi(2)).sum::<f64>().sqrt()}
 pub fn facts<'a>(game:&'a super::Game<'_>,p:[i32;3])->&'a super::Fact{game.res.facts().get(game.world.block(p[0],p[1],p[2]))}}
impl Game<'_>{
 fn reach(&self)->f64{4.5}fn eye_position(&self)->Option<[f64;3]>{Some(self.eye)}
 fn outline_boxes(&self,p:[i32;3])->Vec<[f64;6]>{vec![std::array::from_fn(|i|f64::from(p[i%3])+if i<3{0.0}else{1.0})]}
'''
    post=r'''
fn fixture()->Game<'static>{
 let fact=|air|Fact{fluid:Fluid::None,waterlogged:false,name:if air{"minecraft:air"}else{"minecraft:obsidian"}.into(),render_shape:blocks::RenderShape::Model,replaceable:air,block_entity:false,fluid_level:0};
 let mut blocks=HashSet::new();for x in -2..=8{for z in -2..=3{blocks.insert([x,120,z]);}}
 Game{world:World{blocks},res:Res{facts:Facts{solid:fact(false),air:fact(true)}},eye:[0.5,122.62000000476836,0.5],marker:PhantomData}
}
fn simulate(legacy_nearest:bool,allow_jump:bool)->(Vec<[i32;3]>,bool){
 let mut game=fixture();let mut placed=Vec::new();let mut cursor=0;let mut jumped=false;
 for _ in 0..40{
  let full=policy::trap_cells_from([3.2,121.0,0.2],[3.8,122.8,0.8],true,game.eye).unwrap();
  let lower_done=full.iter().filter(|p|p[1]<123).all(|p|game.world.blocks.contains(p));
  let mut remaining:Vec<_>=full.into_iter().filter(|p|!game.world.blocks.contains(p)).collect();
  if remaining.is_empty(){break;}
  if legacy_nearest{remaining.sort_unstable_by(|a,b|a[1].cmp(&b[1]).then_with(||{
   let center=|p:[i32;3]|p.map(|n|f64::from(n)+0.5);
   util::dist(game.eye,center(*a)).total_cmp(&util::dist(game.eye,center(*b)))
  }));}else{policy::order_trap(&mut remaining,game.eye);}
  let mut rays=96;let mut choice=None;let mut evaluated=0;
  let start=if legacy_nearest{cursor%remaining.len()}else{0};
  let count=if legacy_nearest{remaining.len().min(12)}else{remaining.len().min(36)};
  for offset in 0..count{
   let cell=remaining[(start+offset)%remaining.len()];evaluated+=1;
   if support(&game,cell,4.5,&mut rays).is_some(){choice=Some(cell);break;}
   if rays==0{break;}
  }
  if legacy_nearest{cursor=(start+evaluated)%remaining.len();}
  if let Some(cell)=choice{game.world.blocks.insert(cell);placed.push(cell);}
  else if allow_jump&&lower_done&&!jumped{game.eye[1]=123.65;jumped=true;}
  else{break;}
 }
 (placed,jumped)
}
#[test]fn native_rays_reproduce_near_column_occlusion_before_jump_gate(){
 let (placed,jumped)=simulate(true,true);
 assert!(placed.contains(&[2,121,0])&&placed.contains(&[2,122,0]));
 assert!(placed.len()<8,"near geometry unexpectedly completed lower ring: {placed:?}");
 assert!(!jumped);println!("legacy blocked after{} cells: {placed:?}",placed.len());
}
#[test]fn production_far_order_reaches_all_eight_lower_faces_then_real_height_roof(){
 let (lower,jumped)=simulate(false,false);assert!(!jumped);assert_eq!(lower.len(),8,"{lower:?}");
 let (complete,jumped)=simulate(false,true);assert!(jumped);assert_eq!(complete.len(),10,"{complete:?}");
 assert!(complete.contains(&[3,123,0]));assert!(complete[..8].iter().all(|p|p[1]<123));
 assert_eq!(complete[0],[4,121,0]);println!("production ordered cells: {complete:?}");
}
#[test]fn real_native_outline_rays_require_jump_for_head_top_and_reject_hidden_backface(){
 let mut game=fixture();for y in [121,122]{for p in [[2,y,0],[4,y,0],[3,y,-1],[3,y,1]]{game.world.blocks.insert(p);}}
 assert!(face_aim_with(&game,[3,122,-1],4.5,Some([0,1,0]),&mut 48).is_none());
 game.eye[1]=123.65;
 assert!(face_aim_with(&game,[3,122,-1],4.5,Some([0,1,0]),&mut 48).is_some());
 game.world.blocks.insert([3,123,-1]);assert!(face_aim_with(&game,[3,123,-1],4.5,Some([0,0,1]),&mut 48).is_some());
 game.world.blocks.insert([2,123,0]);assert!(face_aim_with(&game,[2,123,0],4.5,Some([1,0,0]),&mut 48).is_none());
}
'''
    with tempfile.TemporaryDirectory(prefix='sunset-trap-visible-') as temp:
        source=Path(temp)/'probe.rs';binary=Path(temp)/'probe.exe'
        source.write_text(pre.replace('POLICY',policy)+clip+'\n}\n'+helpers+post,encoding='utf-8')
        subprocess.run(['rustc','--edition=2024','--test',str(source),'-o',str(binary)],check=True)
        subprocess.run([str(binary),'--nocapture'],check=True)

if __name__=='__main__':main()
