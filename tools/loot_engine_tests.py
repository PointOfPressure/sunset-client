"""Compile production loot modules with the real pinned protocol and adapters.

No native game, connection or fixture starts. The adapter exposes the exact
native identity-field shapes; full game visibility/linking remains an integration
build check. Protocol packets and encoder/decoder are the real compiled crate.
"""
from pathlib import Path
import argparse,re,subprocess,tempfile
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("engine",type=Path)
parser.add_argument("--dependency-dir",type=Path,required=True)
args=parser.parse_args()
ROOT=Path(__file__).resolve().parents[1]/'mixins/sunset'
ENGINE=args.engine.resolve()
DEPS=args.dependency_dir.resolve()
SDK='rustc'
assert (ENGINE/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
remote=(ENGINE/'crates/horizon-client/src/game/remote.rs').read_text(encoding='utf-8')
for shape in ['kind: String,','position: [f64; 3],','entities: HashMap<i32, Tracked>,','level_players: HashMap<i32, u128>,']:
    assert shape in remote,shape
assert 'self.remote = Some(Remote {' in remote and 'session,' in remote
assert 'pub session: Session,' in remote and 'fn handle_packet(&mut self, packet: C)' in remote
net=(ENGINE/'crates/horizon-client/src/net.rs').read_text(encoding='utf-8')
assert 'pub counters: Arc<Mutex<Option<Arc<Counters>>>>' in net
proto=(ENGINE/'crates/horizon-protocol/src/packets/play.rs').read_text(encoding='utf-8')
assert 'EntityEvent { id: i32 => Int, event: i8 => Byte }' in proto
assert 'if kind == "minecraft:player"' in remote and 'r.level_players.insert(p.id, p.uuid);' in remote
def block(text,start):
    opening=text.index('{',start);depth=1;end=opening+1
    while depth:depth+=(text[end]=='{')-(text[end]=='}');end+=1
    return text[start:end]
spawn=block(proto,proto.index('fn spawn() -> SpawnInfo'))
bridge=(ROOT/'loot_remote.rs').read_text(encoding='utf-8')
wrapper=(ROOT/'loot_guard.rs').read_text(encoding='utf-8')
assert 'fn tick' not in wrapper and 'game.mobs' not in bridge
code=r'''
#![allow(dead_code)]
mod game {
 use std::{collections::HashMap,sync::{Arc,Mutex}};
 use horizon_protocol::connection::Counters;
 pub mod sunset {pub const ID:&str="sunset";}
 pub mod sunset_explosive_damage {#[derive(Clone,Copy)]pub struct Blast{pub center:[f64;3],pub power:f32}}
 pub struct Vec3{pub x:f64,pub y:f64,pub z:f64}
 pub struct Player{pub position:Vec3}
 pub struct Session{pub counters:Arc<Mutex<Option<Arc<Counters>>>>}
 pub struct Tracked{pub kind:String,pub position:[f64;3]}
 pub struct Remote{pub session:Session,pub entity_id:i32,pub level_players:HashMap<i32,u128>,pub entities:HashMap<i32,Tracked>}
 pub struct Game<'a>{pub remote:Option<Remote>,pub player:Option<Player>,pub active:bool,pub window:bool,pub life:std::marker::PhantomData<&'a ()>}
 impl Game<'_>{pub fn native_window(&self)->bool{self.window}pub fn native_on(&self,_:&str)->bool{self.active}}
'''
code+='pub mod sunset_loot_policy{'+(ROOT/'loot_policy.rs').read_text(encoding='utf-8')+'}\n'
code+='pub mod remote{pub mod sunset_loot_remote{'+bridge+'}}\n'
code+='pub mod sunset_loot_guard{'+wrapper+'\n'
code+=r'''
#[cfg(test)]mod tests {
 use super::*;use crate::game::*;use std::collections::HashMap;
 use horizon_protocol::{buf::Reader,packets::play::*};
 fn game()->Game<'static>{
  let mut entities=HashMap::new();entities.insert(7,Tracked{kind:"minecraft:player".into(),position:[5.,121.,0.]});
  entities.insert(8,Tracked{kind:"minecraft:snowball".into(),position:[5.,121.,0.]});
  entities.insert(9,Tracked{kind:"minecraft:zombie".into(),position:[5.,121.,0.]});
  Game{remote:Some(Remote{session:crate::game::Session{counters:Arc::new(Mutex::new(None))},entity_id:1,
   level_players:HashMap::from([(7,17),(9,19),(10,20)]),entities}),player:Some(Player{position:Vec3{x:0.,y:121.,z:0.}}),
   active:true,window:true,life:std::marker::PhantomData}
 }
 fn fresh(g:&Game<'_>){scene(g,"minecraft:overworld",true);}
 fn blast(at:[f64;3])->Blast{Blast{center:at,power:6.}}
'''+spawn+r'''
 #[test]fn actual_entity_event_payload_is_int_entity_id_and_one_byte_event(){
  let p=Clientbound::EntityEvent(EntityEvent{id:0x01020304,event:3});let bytes=p.encode();let mut r=Reader::new(&bytes);
  let id=r.varint().unwrap();assert_eq!(r.i32().unwrap(),0x01020304);assert_eq!(r.i8().unwrap(),3);assert_eq!(r.remaining(),0);
  let mut r=Reader::new(&bytes);r.varint().unwrap();assert_eq!(Clientbound::decode(id,r.rest()).unwrap(),p);
 }
 #[test]fn only_current_native_player_identities_can_create_death_zones(){
  let g=game();fresh(&g);
  for (id,event) in [(8,3),(9,3),(7,35),(7,60),(99,3)]{packet(&g,&C::EntityEvent(EntityEvent{id,event}));}
  assert!(allows(&g,blast([5.,121.,0.]),true));
  packet(&g,&C::EntityEvent(EntityEvent{id:7,event:3}));assert!(!allows(&g,blast([5.,121.,0.]),true));
  assert!(allows(&g,blast([5.,121.,0.]),false));assert!(allows(&g,blast([100.,121.,0.]),true));
 }
 #[test]fn missing_known_player_position_is_conservative_without_treating_snowballs_as_deaths(){
  let g=game();fresh(&g);packet(&g,&C::EntityEvent(EntityEvent{id:10,event:3}));
  assert!(!allows(&g,blast([10_000.,121.,0.]),true));assert_eq!(state(|s|s.last),Reason::Overflow);
  clear();assert!(allows(&g,blast([10_000.,121.,0.]),true));
 }
 #[test]fn same_dimension_respawn_keeps_zones_and_changed_dimension_or_login_clears(){
  let g=game();fresh(&g);packet(&g,&C::EntityEvent(EntityEvent{id:7,event:3}));
  let mut p=spawn();p.dimension="minecraft:overworld".into();packet(&g,&C::Respawn(Respawn{spawn:p,keep:0}));
  assert!(!allows(&g,blast([5.,121.,0.]),true));
  let mut p=spawn();p.dimension="minecraft:the_nether".into();packet(&g,&C::Respawn(Respawn{spawn:p,keep:0}));
  assert!(allows(&g,blast([5.,121.,0.]),true));
  packet(&g,&C::EntityEvent(EntityEvent{id:7,event:3}));scene(&g,"minecraft:the_nether",true);
  assert!(allows(&g,blast([5.,121.,0.]),true));
 }
 #[test]fn connection_replacement_cannot_inherit_a_previous_sessions_known_scene(){
  let g=game();fresh(&g);packet(&g,&C::EntityEvent(EntityEvent{id:7,event:3}));let other=game();
  assert!(!allows(&other,blast([1000.,121.,0.]),true));assert_eq!(state(|s|s.last),Reason::UnknownScene);
  scene(&other,"minecraft:overworld",true);assert!(allows(&other,blast([5.,121.,0.]),true));
 }
 #[test]fn local_player_identity_and_master_off_use_no_entity_list_scan(){
  let mut g=game();fresh(&g);g.active=false;packet(&g,&C::EntityEvent(EntityEvent{id:7,event:3}));
  assert!(allows(&g,blast([5.,121.,0.]),true));g.active=true;
  packet(&g,&C::EntityEvent(EntityEvent{id:1,event:3}));assert!(!allows(&g,blast([0.,121.,0.]),true));
 }
}
}
}
'''
with tempfile.TemporaryDirectory(prefix='sunset-loot-native-api-') as temp:
    rust=Path(temp)/'loot_api.rs';binary=Path(temp)/'loot_api.exe';rust.write_text(code,encoding='utf-8')
    protocols=list(DEPS.glob('libhorizon_protocol-*.rlib'));assert len(protocols)==1
    compile_cmd=[str(SDK),'--edition=2024','--test',str(rust),'--extern','horizon_protocol='+str(protocols[0]),'-L','dependency='+str(DEPS),'-o',str(binary)]
    build=subprocess.run(compile_cmd,capture_output=True,text=True,encoding='utf-8')
    if build.returncode:print(build.stdout+build.stderr);raise SystemExit(build.returncode)
    test=subprocess.run([str(binary),'--test-threads=1'],capture_output=True,text=True,encoding='utf-8')
    print(test.stdout+test.stderr);raise SystemExit(test.returncode)
