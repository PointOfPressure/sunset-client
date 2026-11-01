"""Compile actual pinned remote mining methods + draft helper in a narrow mock.

The break state machine and ScopedHit production source are not reimplemented.
World, item lookup, rendering and transport are deterministic local test seams.
No native client, network connection, GPU, or server is started.
"""
from pathlib import Path
import os
import subprocess
import tempfile

import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('engine',type=Path)
parser.add_argument('--range-only',action='store_true',help='Run only the three native reach regressions; default keeps the original five tests.')
args=parser.parse_args()
DRAFT=Path(__file__).resolve().parents[1]/'mixins/sunset'
ENGINE=args.engine.resolve()/'crates/horizon-client/src'
assert (args.engine.resolve()/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'

def function(text, signature):
    start=text.index(signature)
    opening=text.index("{",start)
    depth=1
    for i in range(opening+1,len(text)):
        depth+=(text[i]=="{")-(text[i]=="}")
        if not depth:
            return text[start:i+1]
    raise AssertionError(signature)

remote=(ENGINE/"game/remote.rs").read_text(encoding="utf-8")
mining=function(remote,"fn remote_mining(&mut self, start: bool)")
stop=function(remote,"pub(super) fn stop_remote_mining(&mut self)")
face=function(remote,"fn face_direction(face: [i32; 3])")
helper=(DRAFT/"city_mining.rs").as_posix()

PRELUDE=r'''
#![allow(dead_code,private_interfaces)]
extern crate self as horizon_client;
pub mod first_person{pub enum Hand{Main}}
mod game {
    use std::{collections::BTreeMap,marker::PhantomData};
    #[derive(Clone,Copy,Debug,PartialEq)] pub struct BlockStateId(pub u32);
    #[derive(Clone,Copy,Debug,PartialEq)] pub struct Hit{pub pos:[i32;3],pub face:[i32;3],pub point:[f64;3]}
    #[derive(Clone,Debug)] pub struct Stack{pub item:String,pub count:u32}
    pub struct Mining{pub pos:[i32;3],pub progress:f32,pub ticks:u32}
    pub struct Remote{pub destroying:Option<([i32;3],u8,f32)>,pub destroying_item:Option<String>,pub destroy_delay:u32,
        pub predictions:BTreeMap<[i32;3],(i32,i32)>}
    #[derive(Clone)] pub struct Fact{pub hardness:f32}
    pub struct Facts;
    impl Facts{fn get(&self,_:BlockStateId)->Fact{Fact{hardness:1.0}}fn is_air(&self,s:BlockStateId)->bool{s.0==0}}
    pub struct Items;
    impl Items{fn can_destroy_block(&self,s:Option<&Stack>,_:bool)->bool{s.is_some_and(|s|s.item!="blocked")}}
    pub struct Res;
    impl Res{fn facts(&self)->Facts{Facts}fn items(&self)->Items{Items}}
    pub struct World{pub blocks:BTreeMap<[i32;3],BlockStateId>}
    impl World{fn block(&self,x:i32,y:i32,z:i32)->BlockStateId{self.blocks.get(&[x,y,z]).copied().unwrap_or(BlockStateId(1))}}
    pub mod play{#[derive(Debug)]pub struct PlayerAction{pub action:i32,pub pos:[i32;3],pub direction:u8,pub sequence:i32}}
    pub enum S{PlayerAction(play::PlayerAction)}
    pub struct Pos{pub x:i32,pub y:i32,pub z:i32}
    pub struct Game<'r>{pub forced_hit:Option<Hit>,pub mining:Option<Mining>,pub remote:Option<Remote>,pub res:Res,
        pub world:World,pub creative:bool,pub inventory:Vec<Option<Stack>>,pub selected:usize,pub hit:Option<Hit>,
        pub progress:f32,pub restricted:bool,pub actions:Vec<play::PlayerAction>,pub sequence:i32,pub marker:PhantomData<&'r ()>}
    pub fn view_vector(_:f32,_:f32)->[f64;3]{[0.0,0.0,1.0]}
    impl Game<'_>{
        fn held(&self)->Option<&Stack>{self.inventory[self.selected].as_ref()}
        fn pick(&self)->Option<Hit>{self.forced_hit.or(self.hit)}
        fn eye_position(&self)->Option<[f64;3]>{Some([0.5,65.62,0.5])}
        fn clip_ray(&self,eye:[f64;3],_:[f64;3],range:f64)->Option<Hit>{
            self.hit.filter(|hit|(0..3).map(|i|(hit.point[i]-eye[i]).powi(2)).sum::<f64>()<=range*range)
        }
        fn reach(&self)->f64{4.5}
        fn remote_break_restricted(&self)->bool{self.restricted}
        fn destroy_progress(&self,_:[i32;3])->f32{self.progress}
        fn main_hand_key(&self)->Option<String>{self.held().map(|s|s.item.clone())}
        fn next_sequence(&mut self)->i32{self.sequence+=1;self.sequence}
        fn predict_break(&mut self,p:[i32;3],s:BlockStateId,sequence:i32){
            self.remote.as_mut().unwrap().predictions.insert(p,(sequence,s.0 as i32));self.world.blocks.insert(p,BlockStateId(0));}
        fn send(&mut self,s:S){let S::PlayerAction(p)=s;self.actions.push(p);}
        fn tutorial_destroy(&mut self,_:BlockStateId,_:f32){}
        fn reset_attack_strength(&mut self){}
        // This mining-only harness includes the whole scoped bridge file for
        // type checking. Trap's native-use path has its own production adapter.
        fn remote_use_block(&mut self,_:crate::first_person::Hand)->bool{
            panic!("native block use is outside this mining-only harness")
        }
        fn breaking_block_effects(&mut self,_:[i32;3],_:[i32;3],_:bool){}
        fn remote_can_place(&mut self,h:&Hit,_:&Stack)->Option<(Pos,())>{
            Some((Pos{x:h.pos[0]+h.face[0],y:h.pos[1]+h.face[1],z:h.pos[2]+h.face[2]},()))}
'''
TESTS=r'''
    }
    pub mod remote {
        use super::*;
        FACE
        #[path="HELPER"] pub mod helper;
    }
    use remote::face_direction;
    fn fixture()->Game<'static>{Game{forced_hit:None,mining:None,remote:Some(Remote{destroying:None,destroying_item:None,
        destroy_delay:0,predictions:BTreeMap::new()}),res:Res,world:World{blocks:BTreeMap::new()},creative:false,
        inventory:vec![Some(Stack{item:"pickaxe".into(),count:1});9],selected:0,
        hit:Some(Hit{pos:[2,64,0],face:[-1,0,0],point:[2.0,64.5,0.5]}),progress:0.25,restricted:false,
        actions:vec![],sequence:0,marker:PhantomData}}
    #[test]fn native_reach_can_exceed_old_fixed_survival_limit(){
        let mut g=fixture();let p=[5,65,0];
        g.hit=Some(Hit{pos:p,face:[-1,0,0],point:[5.6,65.5,0.5]});
        assert!(!remote::helper::step(&mut g,p,[-1,0,0],0.0,0.0));
        assert!(g.actions.is_empty());
        assert!(remote::helper::step_with_range(&mut g,p,[-1,0,0],0.0,0.0,6.0));
        assert_eq!(g.actions.len(),1);assert_eq!(g.actions[0].action,0);
    }
    #[test]fn native_reach_reduction_cannot_be_bypassed_by_fixed_default(){
        let mut g=fixture();let p=[2,64,0];
        assert!(!remote::helper::step_with_range(&mut g,p,[-1,0,0],0.0,0.0,1.0));
        assert!(g.actions.is_empty());
        assert!(remote::helper::step_with_range(&mut g,p,[-1,0,0],0.0,0.0,3.0));
    }
    #[test]fn native_reach_invalid_values_preserve_packets_and_forced_hit(){
        let mut g=fixture();let old=Hit{pos:[7,64,0],face:[1,0,0],point:[7.5,64.5,0.5]};g.forced_hit=Some(old);
        for reach in [0.0,-1.0,65.0,f64::NAN,f64::INFINITY]{
            assert!(!remote::helper::step_with_range(&mut g,[2,64,0],[-1,0,0],0.0,0.0,reach));
            assert_eq!(g.forced_hit,Some(old));assert!(g.actions.is_empty());
        }
    }
    #[test]fn native_mining_starts_once_accumulates_and_stops_at_threshold(){
        let mut g=fixture();let p=[2,64,0];
        for _ in 0..4{assert!(remote::helper::step(&mut g,p,[-1,0,0],0.0,0.0));}
        assert_eq!(g.actions.iter().map(|a|a.action).collect::<Vec<_>>(),[0]);
        assert_eq!(remote::helper::progress(&g,p),Some(0.75));
        assert!(remote::helper::step(&mut g,p,[-1,0,0],0.0,0.0));
        assert_eq!(g.actions.iter().map(|a|a.action).collect::<Vec<_>>(),[0,3]);
        assert_eq!(g.actions[1].direction,4);assert!(remote::helper::predicted(&g,p));
        assert_eq!(g.world.block(2,64,0),BlockStateId(0));assert!(g.mining.is_none());
        assert!(!remote::helper::busy(&g));
    }
    #[test]fn scoped_hit_restores_previous_override_after_native_early_exit(){
        let mut g=fixture();let previous=Hit{pos:[8,64,8],face:[0,1,0],point:[8.5,65.0,8.5]};
        g.forced_hit=Some(previous);g.restricted=true;
        assert!(remote::helper::step(&mut g,[2,64,0],[-1,0,0],0.0,0.0));
        assert_eq!(g.forced_hit,Some(previous));assert!(g.actions.is_empty());
        assert!(!remote::helper::step(&mut g,[2,64,0],[1,0,0],0.0,0.0));
        assert_eq!(g.forced_hit,Some(previous));
    }
    #[test]fn native_item_change_aborts_old_progress_and_restarts(){
        let mut g=fixture();let p=[2,64,0];
        remote::helper::step(&mut g,p,[-1,0,0],0.0,0.0);
        g.inventory[0]=Some(Stack{item:"other_pickaxe".into(),count:1});
        remote::helper::step(&mut g,p,[-1,0,0],0.0,0.0);
        assert_eq!(g.actions.iter().map(|a|a.action).collect::<Vec<_>>(),[0,2,0]);
        assert_eq!(remote::helper::progress(&g,p),Some(0.0));
    }
    #[test]fn cancellation_cannot_abort_another_mining_position(){
        let mut g=fixture();let p=[2,64,0];remote::helper::step(&mut g,p,[-1,0,0],0.0,0.0);
        remote::helper::stop(&mut g,[3,64,0]);assert!(remote::helper::busy(&g));assert_eq!(g.actions.len(),1);
        remote::helper::stop(&mut g,p);assert!(!remote::helper::busy(&g));assert!(g.mining.is_none());
        assert_eq!(g.actions.iter().map(|a|a.action).collect::<Vec<_>>(),[0,2]);
    }
    #[test]fn native_restrictions_and_preview_destination_are_preserved(){
        let mut g=fixture();g.inventory[0].as_mut().unwrap().item="blocked".into();
        remote::helper::step(&mut g,[2,64,0],[-1,0,0],0.0,0.0);assert!(g.actions.is_empty());
        assert!(remote::helper::preview(&mut g,[2,65,0],[2,64,0],[0,1,0],[2.5,65.0,0.5],0));
        assert!(!remote::helper::preview(&mut g,[2,66,0],[2,64,0],[0,1,0],[2.5,65.0,0.5],0));
    }
}
'''

with tempfile.TemporaryDirectory(prefix="sunset-city-native-") as tmp:
    source=Path(tmp)/"native.rs"
    # Preserve the native lexical remote module, including private face helper.
    native="}\npub mod remote { use super::*;\n"+face+"\nimpl Game<'_>{\n"+mining+"\n"+stop+"}\n"
    native+='#[path="'+helper+'"] pub mod helper;\n}\n'
    source.write_text(PRELUDE+native+TESTS[TESTS.index("    fn fixture"):])
    binary=Path(tmp)/("native.exe" if os.name=="nt" else "native")
    subprocess.run(["rustc","--edition=2024","--test",str(source),"-o",str(binary)],check=True)
    subprocess.run([str(binary)]+(['native_reach_'] if args.range_only else ['--skip','native_reach_']),check=True)

assert "self.stop_remote_mining();" in function(remote,"fn remote_continue_attack(&mut self, down: bool)")
city=(DRAFT/"city_trap.rs").read_text(encoding="utf-8")
compact=lambda text:"".join(text.split())
idle=compact(function(city,"pub(super) fn suppress_idle_abort"))
assert "city_current()" in idle and "!city_clicks(game)" in idle
observed=compact(function(city,"fn observed"))
assert "mine.server_air=true" in observed and "game.res.facts().is_air(state_id)" in observed
assert "BlockChangedAck" not in function(city,"pub(super) fn packet")
assert "Blast::explosion" in city and ".ignoring(block)" in city
print(f'Native mining body + production scoped helper: {3 if args.range_only else 5} tests; source contracts: PASS')
