"""Compile actual explosive player selection with narrow entity adapters.

Tests cover safety-list retention, invalid data and the entity cap. No native
client, connection, world ray or damage outcome is exercised.
"""
from pathlib import Path
import subprocess,tempfile


def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)


def main():
    repo=Path(__file__).resolve().parents[1]
    text=(repo/'mixins/sunset/explosive_tactics.rs').read_text(encoding='utf-8')
    functions='\n'.join(function(text,sig) for sig in ['pub(super) fn target_score(','pub(super) fn players('])
    code=r'''
#![allow(dead_code)]
use std::sync::Mutex;
#[derive(Clone,Debug)]struct Target{id:u32,pos:[f64;3],lo:[f64;3],hi:[f64;3],health:f32}
struct Position{x:f64,y:f64,z:f64}
struct Player{position:Position}
struct Remote{entity_id:i32}
struct Kind{id:&'static str}
struct Mob{kind:Kind,id:u32,removed:bool,target:Target,name:Option<String>}
impl Mob{fn alive(&self)->bool{self.target.health>0.0}}
struct Game<'a>{player:Option<Player>,remote:Option<Remote>,mobs:Vec<Mob>,life:std::marker::PhantomData<&'a ()>}
impl Game<'_>{fn remote_player_name(&self,id:u32)->Option<&str>{self.mobs.iter().find(|m|m.id==id)?.name.as_deref()}}
mod sunset {
 pub struct State{pub friends:Vec<String>}
 pub fn with<R>(f:impl FnOnce(&State)->R)->R{f(&State{friends:vec!["friend".into()]})}
}
mod sunset_crystal_bot {pub struct Steer{pub target:u32}pub fn steer(_: &super::Game<'_>)->Option<Steer>{None}}
mod util {
 pub fn dist(a:[f64;3],b:[f64;3])->f64{a.iter().zip(b).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()}
 pub fn mob_target(m:&super::Mob)->super::Target{m.target.clone()}
 pub fn player_enemy(_: &super::Game<'_>,_:&super::Mob)->bool{true}
}
mod prediction {pub fn effective_health(_: &super::Game<'_>,target:&super::Target)->Option<f64>{Some(f64::from(target.health))}}
mod tactics {
 use super::*;
 struct Context{focus:Option<u32>}
 static CONTEXT:Mutex<Option<Context>>=Mutex::new(None);
 fn same(_: &Game<'_>,_:&Context)->bool{true}
 FUNCTIONS
 pub fn fixture()->Game<'static>{
  *CONTEXT.lock().unwrap()=None;
  let mob=|id:u32,pos:[f64;3],name:&str|Mob{kind:Kind{id:"minecraft:player"},id,removed:false,
   target:Target{id,pos,lo:pos.map(|p|p-0.3),hi:pos.map(|p|p+0.3),health:20.0},name:Some(name.into())};
  Game{player:Some(Player{position:Position{x:0.0,y:121.0,z:0.0}}),remote:Some(Remote{entity_id:1}),
   mobs:vec![mob(2,[2.0,121.0,0.0],"enemy"),mob(3,[11.0,121.0,0.0],"friend"),mob(4,[10.0,121.0,0.0],"distant enemy")],life:std::marker::PhantomData}
 }
}
#[test]fn friend_beyond_target_search_is_retained_while_distant_enemy_is_not(){
 let game=tactics::fixture();let (targets,friends)=tactics::players(&game).unwrap();
 assert_eq!(targets.iter().map(|t|t.id).collect::<Vec<_>>(),[2]);
 assert_eq!(friends.iter().map(|t|t.id).collect::<Vec<_>>(),[3]);
 // A reachable crystal center4 places this friend only7 away, inside power6
 // blast reach12. Enemy search distance cannot be the safety cutoff.
 assert!(util::dist([4.0,121.0,0.0],friends[0].pos)<12.0);
}
#[test]fn malformed_friend_still_fails_closed_before_any_range_shortcut(){
 let mut game=tactics::fixture();game.mobs[1].target.pos[0]=f64::NAN;
 assert!(tactics::players(&game).is_none());
}
#[test]fn entity_population_bound_still_refuses_extra_scanning(){
 let mut game=tactics::fixture();
 while game.mobs.len()<=512{let mut t=game.mobs[0].target.clone();t.id=game.mobs.len() as u32+10;
  game.mobs.push(Mob{kind:Kind{id:"minecraft:player"},id:t.id,removed:false,target:t,name:Some("other".into())});}
 assert!(tactics::players(&game).is_none());
}
'''.replace('FUNCTIONS',functions)
    with tempfile.TemporaryDirectory(prefix='sunset-explosive-players-') as directory:
        source=Path(directory)/'players.rs';exe=Path(directory)/'players.exe'
        source.write_text(code,encoding='utf-8')
        subprocess.run(['rustc','--edition=2024','--test',str(source),'-o',str(exe)],check=True)
        subprocess.run([str(exe),'--test-threads=1'],check=True)


if __name__=='__main__':main()
