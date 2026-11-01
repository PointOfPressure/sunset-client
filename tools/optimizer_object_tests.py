"""Actual pinned remote_object_hit + ray_box and actual HEAD wrapper.
Noncrystal/dragon hit producers are explicit tuple seams; no Game starts.
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
native=body((engine/'crates/horizon-client/src/game/remote_vehicles.rs').read_text(),'pub(super) fn remote_object_hit(')
native=native.replace('pub(super) fn remote_object_hit(', 'fn original_object_hit(',1).replace('        let (eye, dir) = self.aim()?;', '        self.calls.borrow_mut().push(attacking);\n        let (eye, dir) = self.aim()?;',1)
ray=body((engine/'crates/horizon-client/src/game.rs').read_text(),'fn ray_box(')
dragon=body((engine/'crates/horizon-client/src/game/dragon.rs').read_text(),'pub(super) fn remote_dragon_part_hit(')
dragon_signature=dragon[:dragon.index('{')].replace('pub(super) ','',1)
code=r'''
#![allow(dead_code)]
#[derive(Clone,Copy,Debug)]struct Vec3{x:f64,y:f64,z:f64}impl Vec3{fn new(x:f64,y:f64,z:f64)->Self{Self{x,y,z}}}
#[derive(Clone,Copy)]struct Aabb{lo:Vec3,hi:Vec3}impl Aabb{fn new(lo:Vec3,hi:Vec3)->Self{Self{lo,hi}}fn min_position(&self)->Vec3{self.lo}fn max_position(&self)->Vec3{self.hi}}
RAY
mod game {
 use super::*;
 #[derive(Clone)]struct BlockObject{id:u32,pos:[i32;3]}
 #[derive(Clone)]struct MovingObject{id:u32,position:Vec3}
 #[derive(Clone)]pub struct Crystal{pub id:u32,pub position:Vec3}
 #[derive(Clone)]pub struct Game<'a>{pub crystals:Vec<Crystal>,pub creative:bool,pub predicted:bool,pub hidden:Vec<u32>,
   paintings:Vec<BlockObject>,item_frames:Vec<BlockObject>,armor_stands:Vec<MovingObject>,cushions:Vec<MovingObject>,boats:Vec<MovingObject>,carts:Vec<MovingObject>,
   other:[Option<f64>;6],parts:Vec<(i32,f64,[f64;3])>,pub calls:std::cell::RefCell<Vec<bool>>,aim_ok:bool,marker:std::marker::PhantomData<&'a ()>}
 pub mod sunset_crystal_optimizer{pub fn has_predictions(g:&super::Game<'_>)->bool{g.predicted}pub fn hidden_id(g:&super::Game<'_>,id:u32,_:[f64;3])->bool{g.hidden.contains(&id)}}
 impl Game<'_>{fn aim(&self)->Option<([f64;3],[f64;3])>{self.aim_ok.then_some(([0.5,1.,0.5],[1.,0.,0.]))}
  fn painting_hit(&self)->Option<(usize,f64)>{self.other[0].map(|v|(0,v))}fn frame_hit(&self)->Option<(usize,f64)>{self.other[1].map(|v|(0,v))}
  fn armor_stand_hit(&self)->Option<(usize,f64,())>{self.other[2].map(|v|(0,v,()))}fn cushion_hit(&self)->Option<(usize,f64,())>{self.other[3].map(|v|(0,v,()))}
  fn boat_hit(&self)->Option<(usize,f64)>{self.other[4].map(|v|(0,v))}fn cart_hit(&self)->Option<(usize,f64)>{self.other[5].map(|v|(0,v))}
  DRAGON_SIGNATURE {let _=(eye,dir);self.parts.iter().filter(|(_,d,_)|*d<=reach).copied().min_by(|a,b|a.1.total_cmp(&b.1))}
 }
 pub mod remote_vehicles {
  use super::*;
  #[derive(Debug,PartialEq)]pub struct ObjectHit{pub id:i32,pub distance:f64,pub location:[f64;3]}
  include!("HOOK");
  impl Game<'_>{
   ORIGINAL
   pub fn remote_object_hit(&self,attacking:bool)->Option<ObjectHit>{match sunset_optimizer_objects(self,attacking){std::ops::ControlFlow::Continue(())=>self.original_object_hit(attacking),std::ops::ControlFlow::Break(hit)=>hit}}
   pub fn baseline(&self,attacking:bool)->Option<ObjectHit>{self.original_object_hit(attacking)}
  }
 }
 pub fn fixture()->Game<'static>{let mov=|id|MovingObject{id,position:Vec3::new(id as f64,0.,0.)};Game{crystals:vec![Crystal{id:7,position:Vec3::new(2.5,0.,0.5)},Crystal{id:8,position:Vec3::new(3.5,0.,0.5)}],creative:false,predicted:true,hidden:vec![7],
  paintings:vec![BlockObject{id:80,pos:[3,0,0]}],item_frames:vec![BlockObject{id:81,pos:[3,0,0]}],armor_stands:vec![mov(82)],cushions:vec![mov(83)],boats:vec![mov(84)],carts:vec![mov(85)],other:[None;6],parts:vec![],calls:std::cell::RefCell::new(vec![]),aim_ok:true,marker:std::marker::PhantomData}}
 pub fn other(g:&mut Game<'_>,index:usize,d:f64){g.other[index]=Some(d);}
 pub fn parts(g:&mut Game<'_>,parts:Vec<(i32,f64,[f64;3])>){g.parts=parts;}
 pub fn aim(g:&mut Game<'_>,ok:bool){g.aim_ok=ok;}
}
fn expected(g:&game::Game<'_>)->Option<game::remote_vehicles::ObjectHit>{let mut copy=g.clone();copy.predicted=false;copy.crystals.retain(|c|!copy.hidden.contains(&c.id));copy.baseline(true)}
#[test]fn unmasked_fast_path_returns_the_exact_original_tuple_once(){let mut g=game::fixture();game::other(&mut g,0,0.25);assert_eq!(g.remote_object_hit(true),g.baseline(true));assert_eq!(*g.calls.borrow(),[true,true]);}
#[test]fn every_native_noncrystal_kind_behind_ghost_uses_original_geometry_and_location(){for kind in 0..6{let mut g=game::fixture();game::other(&mut g,kind,1.5);assert_eq!(g.remote_object_hit(true),expected(&g));assert_eq!(g.remote_object_hit(true).unwrap().id,80+kind as i32);assert_eq!(g.crystals.len(),2);}}
#[test]fn unmasked_colocated_crystal_survives_identity_filter(){let mut g=game::fixture();g.crystals[1].position=g.crystals[0].position;let got=g.remote_object_hit(true).unwrap();assert_eq!(got.id,8);assert_eq!(Some(got),expected(&g));}
#[test]fn native_dragon_part_backfill_and_equal_distance_order_match_original(){let mut g=game::fixture();game::parts(&mut g,vec![(91,2.8,[8.,9.,10.]),(90,1.5,[1.,2.,3.]),(92,9.,[11.,12.,13.])]);assert_eq!(g.remote_object_hit(true),expected(&g));assert_eq!(g.remote_object_hit(true).unwrap().id,90);game::other(&mut g,4,2.);game::parts(&mut g,vec![(90,2.,[1.,2.,3.])]);assert_eq!(g.remote_object_hit(true),expected(&g));assert_eq!(g.remote_object_hit(true).unwrap().id,84);}
#[test]fn creative_and_survival_backfill_keep_the_pinned_native_reach(){let mut g=game::fixture();g.crystals[1].position=Vec3::new(5.5,0.,0.5);assert!(g.remote_object_hit(true).is_none());g.creative=true;assert_eq!(g.remote_object_hit(true),expected(&g));assert_eq!(g.remote_object_hit(true).unwrap().id,8);}
#[test]fn nonattack_and_nohit_paths_release_reentry_guard_and_preserve_native_behavior(){let mut g=game::fixture();assert_eq!(g.remote_object_hit(false),g.baseline(false));game::aim(&mut g,false);assert!(g.remote_object_hit(true).is_none());game::aim(&mut g,true);assert_eq!(g.remote_object_hit(true),expected(&g));g.predicted=false;assert_eq!(g.remote_object_hit(true),g.baseline(true));}
'''.replace('RAY',ray).replace('DRAGON_SIGNATURE',dragon_signature).replace('ORIGINAL',native).replace('HOOK',(repo/'mixins/sunset/optimizer_objects.rs').as_posix())
with tempfile.TemporaryDirectory() as tmp:
 p=Path(tmp);(p/'test.rs').write_text(code)
 build=subprocess.run(['rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test.exe')],text=True,capture_output=True);assert build.returncode==0,build.stderr
 run=subprocess.run([str(p/'test.exe')],text=True,capture_output=True);print(run.stdout);assert run.returncode==0,run.stderr
 result={'compiled_tests':6,'source_contracts':0,'warnings':build.stderr,'scope':'Actual pinned original remote_object_hit and ray_box; actual integrated HEAD helper. Other-object and dragon-part ray producers are explicit tuple seams; no client or GPU.'}
 if args.out:args.out.write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result))
