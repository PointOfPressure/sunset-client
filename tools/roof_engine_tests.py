"""Compile pinned Game::clip_ray and draft real-face helper against obsidian boxes."""
from pathlib import Path
import subprocess,tempfile

import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('engine',type=Path)
args=parser.parse_args()
assert (args.engine/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
DRAFT=Path(__file__).resolve().parents[1]/'mixins/sunset'
engine=(args.engine/'crates/horizon-client/src/game.rs').read_text(encoding='utf-8')

def body(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1
    for i in range(opening+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[start:i+1]
    raise AssertionError(signature)
clip=body(engine,'fn clip_ray(&self, origin: [f64; 3], dir: [f64; 3], reach: f64)')
face=body((DRAFT/'city_trap.rs').read_text(encoding="utf-8"),'fn face_aim_with(')
pre=r'''
#![allow(dead_code)]
extern crate self as horizon_client;
pub mod blocks{#[derive(PartialEq)]pub enum RenderShape{Model}}
use std::{collections::HashSet,marker::PhantomData};
#[derive(PartialEq)]enum Fluid{None}
struct Fact{fluid:Fluid,waterlogged:bool,name:String,render_shape:blocks::RenderShape}
struct Facts{solid:Fact}
impl Facts{fn get(&self,_:u32)->&Fact{&self.solid}fn is_air(&self,state:u32)->bool{state==0}}
struct Res{facts:Facts}impl Res{fn facts(&self)->&Facts{&self.facts}}
struct World{blocks:HashSet<[i32;3]>}impl World{fn block(&self,x:i32,y:i32,z:i32)->u32{u32::from(self.blocks.contains(&[x,y,z]))}fn loaded(&self,_:i32,_:i32)->bool{true}}
struct Game<'a>{world:World,res:Res,eye:[f64;3],marker:PhantomData<&'a ()>}
#[derive(Clone,Copy)]struct Hit{pos:[i32;3],face:[i32;3],point:[f64;3]}
#[derive(Clone,Copy)]struct Face{block:[i32;3],normal:[i32;3],point:[f64;3]}
mod policy{pub fn near(a:[f64;3],b:[f64;3],r:f64)->bool{(0..3).map(|i|(a[i]-b[i]).powi(2)).sum::<f64>()<=r*r}}
mod util{pub fn dist(a:[f64;3],b:[f64;3])->f64{(0..3).map(|i|(a[i]-b[i]).powi(2)).sum::<f64>().sqrt()}}
impl Game<'_>{
    fn reach(&self)->f64{4.5}fn eye_position(&self)->Option<[f64;3]>{Some(self.eye)}
    fn outline_boxes(&self,p:[i32;3])->Vec<[f64;6]>{vec![std::array::from_fn(|i|f64::from(p[i%3])+if i<3{0.0}else{1.0})]}
'''
post=r'''
fn fixture(eye:[f64;3])->Game<'static>{
    let mut blocks=HashSet::new();
    for y in [121,122]{for p in [[2,y,0],[4,y,0],[3,y,-1],[3,y,1]]{blocks.insert(p);}}
    Game{world:World{blocks},res:Res{facts:Facts{solid:Fact{fluid:Fluid::None,waterlogged:false,name:"minecraft:obsidian".into(),render_shape:blocks::RenderShape::Model}}},eye,marker:PhantomData}
}
#[test]fn real_outline_rays_reject_grounded_top_and_accept_jumped_top_and_inward_side(){
    let mut g=fixture([0.5,122.62000000476836,0.5]);
    assert!(face_aim_with(&g,[3,122,-1],4.5,Some([0,1,0]),&mut 48).is_none());
    g.eye[1]=123.65;
    assert!(face_aim_with(&g,[3,122,-1],4.5,Some([0,1,0]),&mut 48).is_some());
    g.world.blocks.insert([3,123,-1]);
    assert!(face_aim_with(&g,[3,123,-1],4.5,Some([0,0,1]),&mut 48).is_some());
}
#[test]fn nearest_scaffold_hidden_back_face_stays_impossible_during_native_height_jump(){
    let mut g=fixture([0.5,123.65,0.5]);g.world.blocks.insert([2,123,0]);
    assert!(face_aim_with(&g,[2,123,0],4.5,Some([1,0,0]),&mut 48).is_none());
}
'''
with tempfile.TemporaryDirectory(prefix='sunset-roof-rays-') as tmp:
    source=Path(tmp)/'rays.rs';binary=Path(tmp)/'rays.exe'
    source.write_text(pre+clip+'\n}\n'+face+post,encoding='utf-8')
    subprocess.run(['rustc','--edition=2024','--test',str(source),'-o',str(binary)],check=True)
    subprocess.run([str(binary)],check=True)
print('Actual native outline traversal + production face selector: PASS')
