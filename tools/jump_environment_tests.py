"""Compile production local motion with the pinned native climbing registry.

Five environmental bridge checks; no Game/client/network. The proof explicitly
records whether JumpReset currently calls this bridge, without turning these
kernel checks into a claim of actual jump behavior.
"""
from pathlib import Path
import argparse,hashlib,json,subprocess,tempfile


def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('engine',type=Path)
    parser.add_argument('--dependency-dir',required=True,type=Path)
    parser.add_argument('--out',type=Path)
    args=parser.parse_args()
    revision='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    assert (args.engine/'.horizon-revision').read_text().strip()==revision
    repo=Path(__file__).resolve().parents[1]
    production=(repo/'mixins/sunset/prediction_remote.rs').read_text(encoding='utf-8')
    helper=function(production,'pub(crate) fn local_motion(')
    record=function(production,'pub(crate) struct LocalMotion')
    code=r'''
#![allow(dead_code)]
use horizon_core::{geometry::{Vec3,Aabb},position::BlockPos};
use horizon_world::{registry::{BlockRegistry,BlockStateId},climbing::ClimbingRegistry};
use std::collections::BTreeSet;
struct Fluids{was_touching_water:bool}
struct Player{position:Vec3,old_position:Vec3,flying:bool,fall_flying:bool,spectator:bool,
 fluids:Fluids,lava:bool,last_climbable:Option<BlockPos>}
impl Player{fn bounds(&self)->Aabb{Aabb::block(BlockPos::containing(self.position))}fn is_in_lava(&self)->bool{self.lava}}
struct World{at:BlockStateId,below:BlockStateId,missing:bool}
impl World{fn loaded(&self,_:i32,_:i32)->bool{!self.missing}
 fn block(&self,_:i32,y:i32,_:i32)->BlockStateId{if y==121{self.at}else{self.below}}}
struct Motion<'a>{climbing:ClimbingRegistry<'a>}
struct Game<'a>{player:Option<Player>,world:World,motion:Motion<'a>,riding:Option<u32>}
LOCAL
HELPER
fn registry()->BlockRegistry {
 let facts=br#"{"states":[
 ["air",true,false,"","minecraft:air"],
 ["ladder",false,false,"",{"id":"minecraft:ladder","properties":{"facing":"north"}}],
 ["trapdoor",false,false,"",{"id":"minecraft:oak_trapdoor","properties":{"facing":"north","open":"true"}}],
 ["ladder",false,false,"",{"id":"minecraft:ladder","properties":{"facing":"south"}}]
 ],"defaults":{"minecraft:air":0,"minecraft:ladder":1,"minecraft:oak_trapdoor":2}}"#;
 BlockRegistry::from_facts_json(facts.as_slice()).unwrap()
}
fn game(registry:&BlockRegistry,at:u32,below:u32)->Game<'_>{
 let trapdoors=BTreeSet::from(["minecraft:oak_trapdoor".into()]);
 let climbable=BTreeSet::from(["minecraft:ladder".into()]);
 Game{player:Some(Player{position:Vec3::new(0.5,121.0,0.5),old_position:Vec3::new(0.5,121.0,0.5),
  flying:false,fall_flying:false,spectator:false,fluids:Fluids{was_touching_water:false},lava:false,last_climbable:None}),
  world:World{at:BlockStateId(at),below:BlockStateId(below),missing:false},
  motion:Motion{climbing:ClimbingRegistry::new(registry,&trapdoors,&climbable,&BTreeSet::new()).unwrap()},riding:None}
}
#[test]fn current_ladder_with_empty_landing_history_is_still_climbing(){
 let registry=registry();let game=game(&registry,1,0);
 assert!(game.player.as_ref().unwrap().last_climbable.is_none());
 assert!(local_motion(&game).unwrap().floating);
}
#[test]fn stale_history_on_air_is_not_current_climbing(){
 let registry=registry();let mut game=game(&registry,0,0);
 game.player.as_mut().unwrap().last_climbable=Some(BlockPos::new(0,119,0));
 assert!(!local_motion(&game).unwrap().floating);
}
#[test]fn native_open_trapdoor_requires_the_aligned_ladder_below(){
 let registry=registry();let aligned=game(&registry,2,1);let crossed=game(&registry,2,3);
 assert!(local_motion(&aligned).unwrap().floating);assert!(!local_motion(&crossed).unwrap().floating);
}
#[test]fn water_lava_gliding_and_riding_remain_blocked(){
 let registry=registry();
 for scenario in 0..4{let mut game=game(&registry,0,0);
  match scenario{0=>game.player.as_mut().unwrap().fluids.was_touching_water=true,
   1=>game.player.as_mut().unwrap().lava=true,2=>game.player.as_mut().unwrap().fall_flying=true,_=>game.riding=Some(7)};
  assert!(local_motion(&game).unwrap().floating);
 }
}
#[test]fn missing_current_chunk_cannot_establish_jump_eligibility(){
 let registry=registry();let mut game=game(&registry,0,0);game.world.missing=true;
 assert!(local_motion(&game).is_none());
}
'''.replace('LOCAL',record).replace('HELPER',helper)
    deps=args.dependency_dir.resolve();core,=deps.glob('libhorizon_core-*.rlib')
    worlds=sorted(deps.glob('libhorizon_world-*.rlib'));assert worlds
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-jump-environment-') as directory:
        source=Path(directory)/'environment.rs';exe=Path(directory)/'environment.exe'
        source.write_text(code,encoding='utf-8')
        for world in worlds:
            command=['rustc','--edition=2024','--test',str(source),'-L',f'dependency={deps}',
                     '--extern',f'horizon_core={core}','--extern',f'horizon_world={world}','-o',str(exe)]
            result=subprocess.run(command,capture_output=True,text=True)
            records.append({'command':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
            if result.returncode==0:break
        if result.returncode==0:
            result=subprocess.run([str(exe)],capture_output=True,text=True)
            records.append({'command':[str(exe)],'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
        print(result.stdout+result.stderr)
    melee=repo/'mixins/sunset/melee_assist.rs'
    pre_input=function(melee.read_text(encoding='utf-8'),'pub(super) fn pre_input(')
    wired='sunset_prediction_remote::local_motion(game)' in pre_input and 'p.last_climbable' not in pre_input
    if not wired:print('Integration note: JumpReset still uses historical climbing state; these five tests verify the native bridge only.')
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'native_host_revision':revision,
            'jump_reset_uses_current_climbing_bridge':wired,
            'source_sha256':{p.relative_to(repo).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()
                             for p in [repo/'mixins/sunset/prediction_remote.rs',melee]},
            'scope':'Five compiled production local_motion adapter checks with actual native registry/climbing kernel. No actual jump/client/network.'},indent=2)+'\n',encoding='utf-8')
    raise SystemExit(result.returncode)


if __name__=='__main__':main()
