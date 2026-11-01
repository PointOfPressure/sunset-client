"""Compile the whole production bridge with real pinned Game::clip_ray/face_direction.

Only unit-cube outlines and a recording dry-preview adapter are supplied. This
checks real ray traversal, hand selection and the preview gate, not full native
BlockItem.place execution, server ACKs or a renderer/client build.
"""
from pathlib import Path
import argparse, subprocess, tempfile

def function(text, signature):
    start=text.index(signature); opening=text.index('{',start); depth=1
    for index in range(opening+1,len(text)):
        depth+=(text[index]=='{')-(text[index]=='}')
        if not depth:return text[start:index+1]
    raise AssertionError(signature)

def main():
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument('engine',type=Path)
    engine=parser.parse_args().engine.resolve(); root=Path(__file__).resolve().parents[1]/'mixins/sunset'
    assert (engine/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    native=(engine/'crates/horizon-client/src/game.rs').read_text(encoding='utf-8')
    remote=(engine/'crates/horizon-client/src/game/remote.rs').read_text(encoding='utf-8')
    # Pin the actual bridge ABI: mutable Game, borrowed ItemStack, native Hit,
    # dry desired position and BlockStateId changes; the adapter below records
    # those arguments rather than silently accepting any call.
    preview=function(remote,'fn remote_can_place(')
    assert 'hit: &super::Hit, stack: &ItemStack' in preview and 'Option<DryPlace>' in preview
    assert 'DryLevel::new(level)' in preview and 'b.place_block(' in preview
    assert 'type DryChanges = Vec<(horizon_core::position::BlockPos, BlockStateId)>;' in remote
    clip=function(native,'fn clip_ray(&self, origin: [f64; 3], dir: [f64; 3], reach: f64)')
    face=function(remote,'fn face_direction(')
    pre=r'''
#![allow(dead_code)]
extern crate self as horizon_client;
pub mod blocks { #[derive(PartialEq)] pub enum RenderShape{Model} }
mod game {
use std::{collections::HashSet,marker::PhantomData};
#[derive(Clone,Copy,PartialEq)]struct BlockStateId(u32);
#[derive(Clone,Copy)]struct BlockPos{x:i32,y:i32,z:i32}
#[derive(Clone,Copy,PartialEq)]enum Fluid{None,Water}
struct Fact{fluid:Fluid,waterlogged:bool,name:String,render_shape:crate::blocks::RenderShape,
 full_collision:bool,replaceable:bool,block_entity:bool,fluid_level:u8}
struct Facts{solid:Fact,air:Fact} impl Facts{
 fn get(&self,id:BlockStateId)->&Fact{if id.0==0{&self.air}else{&self.solid}}
 fn is_air(&self,id:BlockStateId)->bool{id.0==0}}
struct Res{facts:Facts}impl Res{fn facts(&self)->&Facts{&self.facts}}
struct World{blocks:HashSet<[i32;3]>}impl World{
 fn block(&self,x:i32,y:i32,z:i32)->BlockStateId{BlockStateId(u32::from(self.blocks.contains(&[x,y,z])))}
 fn loaded(&self,_:i32,_:i32)->bool{true}}
#[derive(Clone)]struct ItemStack{item:String,count:u32,tag:u64}
#[derive(Clone,Copy)]struct Hit{pos:[i32;3],face:[i32;3],point:[f64;3]}
struct Game<'a>{world:World,res:Res,eye:[f64;3],inventory:Vec<Option<ItemStack>>,offhand:Option<ItemStack>,
 previews:Vec<u64>,accept:bool,wrong_destination:bool,wrong_state:bool,marker:PhantomData<&'a ()>}
mod sunset_util{
 pub const OFFHAND:usize=40;
 pub fn facts<'a>(game:&'a super::Game<'_>,p:[i32;3])->&'a super::Fact{game.res.facts().get(game.world.block(p[0],p[1],p[2]))}}
mod remote {
 FACE
 #[path="BRIDGE"]pub(crate) mod sunset_aura_base;
}
impl Game<'_>{
 fn reach(&self)->f64{4.5}fn eye_position(&self)->Option<[f64;3]>{Some(self.eye)}
 fn outline_boxes(&self,p:[i32;3])->Vec<[f64;6]>{vec![std::array::from_fn(|i|f64::from(p[i%3])+if i<3{0.0}else{1.0})]}
 fn remote_can_place(&mut self,hit:&Hit,stack:&ItemStack)->Option<(BlockPos,Vec<(BlockPos,BlockStateId)>)>{
  self.previews.push(stack.tag);if !self.accept{return None;}
  let cell: [i32;3]=std::array::from_fn(|i|hit.pos[i]+hit.face[i]);
  let p=BlockPos{x:cell[0]+i32::from(self.wrong_destination),y:cell[1],z:cell[2]};
  Some((p,vec![(p,BlockStateId(if self.wrong_state{0}else{1}))]))
 }
 CLIP
}
fn fixture()->Game<'static>{
 let fact=|air|Fact{fluid:Fluid::None,waterlogged:false,name:if air{"minecraft:air"}else{"minecraft:obsidian"}.into(),
 render_shape:crate::blocks::RenderShape::Model,full_collision:!air,replaceable:air,block_entity:false,fluid_level:0};
 let mut blocks=HashSet::new();blocks.insert([3,120,0]);
 let obsi=|tag|Some(ItemStack{item:"minecraft:obsidian".into(),count:1,tag});
 let mut inventory=vec![None;36];inventory[2]=obsi(22);
 Game{world:World{blocks},res:Res{facts:Facts{solid:fact(false),air:fact(true)}},eye:[0.5,122.62,0.5],
 inventory,offhand:obsi(4040),previews:vec![],accept:true,wrong_destination:false,wrong_state:false,marker:PhantomData}
}
#[test]fn real_rays_and_native_face_mapping_prove_visible_support(){
 let mut g=fixture();let click=remote::sunset_aura_base::plan(&mut g,[3,121,0],2,4.5,&mut 48).unwrap();
 assert_eq!(click.clicked,[3,120,0]);assert_eq!(click.normal,[0,1,0]);assert_eq!(remote::sunset_aura_base::face(click),1);
 assert_eq!(g.previews,vec![22]);
}
#[test]fn real_offhand_stack_reaches_preview_without_a_hotbar_guess(){
 let mut g=fixture();assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],40,4.5,&mut 48).is_some());
 assert_eq!(g.previews,vec![4040]);
 assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],17,4.5,&mut 48).is_none());
 assert_eq!(g.previews.len(),1);
}
#[test]fn denied_wrong_destination_or_wrong_block_preview_refuses_setup(){
 for mode in 0..3{let mut g=fixture();g.accept=mode!=0;g.wrong_destination=mode==1;g.wrong_state=mode==2;
 assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],2,4.5,&mut 48).is_none());assert_eq!(g.previews,vec![22]);}
}
#[test]fn obstruction_budget_and_range_refuse_before_native_preview(){
 let mut g=fixture();assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],2,4.5,&mut 0).is_none());
 assert!(g.previews.is_empty());assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],2,1.0,&mut 48).is_none());
 g.world.blocks.insert([1,121,0]);g.world.blocks.insert([1,122,0]);
 assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],2,4.5,&mut 48).is_none());assert!(g.previews.is_empty());
}
#[test]fn setup_never_clicks_interactive_or_empty_wrong_hand_support(){
 let mut g=fixture();g.res.facts.solid.name="minecraft:respawn_anchor".into();
 assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],40,4.5,&mut 48).is_none());
 assert!(g.previews.is_empty());let mut g=fixture();g.offhand.as_mut().unwrap().count=0;
 assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],40,4.5,&mut 48).is_none());
 assert!(g.previews.is_empty());
}
#[test]fn level_zero_water_and_malformed_face_never_reach_preview(){
 let mut g=fixture();g.res.facts.solid.fluid=Fluid::Water;g.res.facts.solid.fluid_level=0;
 assert!(remote::sunset_aura_base::plan(&mut g,[3,121,0],2,4.5,&mut 48).is_none());
 assert!(g.previews.is_empty());let mut g=fixture();
 let click=remote::sunset_aura_base::BaseClick{clicked:[3,120,0],normal:[i32::MIN,0,0],point:[3.5,121.0,0.5]};
 assert!(!remote::sunset_aura_base::valid(&mut g,[3,121,0],2,click,4.5,&mut 48));
 assert!(g.previews.is_empty());
}
}
'''
    code=pre.replace('FACE',face).replace('BRIDGE',(root/'aura_base.rs').as_posix()).replace('CLIP',clip)
    with tempfile.TemporaryDirectory(prefix='sunset-base-api-') as temp:
        src=Path(temp)/'probe.rs'; exe=Path(temp)/'probe.exe'; src.write_text(code,encoding='utf-8')
        subprocess.run(['rustc','--edition=2024','--test',str(src),'-o',str(exe)],check=True)
        subprocess.run([str(exe)],check=True)

if __name__=='__main__':main()
