"""Actual native projectile + AddEntity codec tests of the original owner bridge."""
from pathlib import Path
import argparse,json,subprocess,tempfile

def function(text,start):
 a=text.index(start);b=text.index('{',a);depth=1
 for i in range(b+1,len(text)):
  depth+=(text[i]=='{')-(text[i]=='}')
  if not depth:return text[a:i+1]
 raise AssertionError(start)
def main():
 p=argparse.ArgumentParser();p.add_argument('engine',type=Path);p.add_argument('--dependency-dir',type=Path,required=True);a=p.parse_args();root=Path(__file__).resolve().parent;production=root.parent/'mixins/sunset'
 code=r"""
#![allow(dead_code)]
use horizon_client::{projectiles::{Projectile,ProjectileKind},mobs::Who};
use horizon_core::geometry::{Vec3,Aabb};
use horizon_client::projectiles::{ProjectileWorld,Hit};
mod physics{use super::*;REMOTE_TICK}
struct Empty;impl ProjectileWorld for Empty{fn clip(&self,_:Vec3,_:Vec3)->Option<(Vec3,[i32;3],[i32;3])>{None}fn in_water(&self,_:Vec3)->bool{false}fn targets(&self)->&[(Who,Aabb)]{&[]}}
use horizon_protocol::{buf::Reader,packets::play::{self,Clientbound as C}};
mod game{
 use super::*;
 pub struct Session{pub counters:std::sync::Arc<std::sync::Mutex<Option<u8>>>}pub struct Remote{pub entity_id:i32,pub session:Session}#[derive(Clone,Copy,PartialEq)]pub enum Dim{Overworld,Nether}pub struct Mob{pub id:u32,pub removed:bool}pub struct Creatures{pub projectiles:Vec<Projectile>}
 pub mod sunset{pub const ID:&str="sunset";}
 pub struct Game<'a>{pub dim:Dim,pub ticks:u64,pub window:bool,pub master:bool,pub remote:Option<Remote>,pub player:Option<()>,pub mobs:Vec<Mob>,pub creatures:Creatures,pub life:std::marker::PhantomData<&'a()>}
 #[path="OWNER"]pub mod sunset_pearl_owner;
 HOOK
 impl Game<'_>{pub fn trace(&self)->Option<(u64,i32,i32,i32,Option<Who>,String)>{sunset_pearl_owner::snapshot(self).map(|s|(s.tick,s.own_id,s.spawn_id,s.raw_data,s.resolved,format!("{:?}",s.reason)))}pub fn native_window(&self)->bool{self.window}pub fn native_on(&self,_:&str)->bool{self.master}pub fn hook(&mut self,value:bool,id:i32,kind:&str,data:i32)->bool{sunset_owner_metadata(value,self,id,kind,data)}}
}
fn native_pearl()->Projectile{let mut p=Projectile::new(ProjectileKind::EnderPearl,Vec3::new(1.,122.,3.),None);p.id=111;p.velocity=Vec3::new(1.5,-0.1,0.);p.age=1;p}
fn fixture()->game::Game<'static>{game::Game{dim:game::Dim::Overworld,ticks:100,window:true,master:true,remote:Some(game::Remote{entity_id:0,session:game::Session{counters:std::sync::Arc::new(std::sync::Mutex::new(Some(0)))}}),player:Some(()),mobs:vec![game::Mob{id:42,removed:false}],creatures:game::Creatures{projectiles:vec![native_pearl()]},life:std::marker::PhantomData}}
fn packet(data:i32)->play::AddEntity{let packet=C::AddEntity(play::AddEntity{id:111,uuid:9,kind:88,position:[1.,122.,3.],velocity:[1.5,-0.1,0.],pitch:1,yaw:2,head_yaw:2,data});let raw=packet.encode();let mut r=Reader::new(&raw);let id=r.varint().unwrap();let C::AddEntity(p)=C::decode(id,r.rest()).unwrap()else{panic!()};p}
#[test]fn real_add_entity_codec_raw_owner_resolves_exact_native_mob(){let p=packet(42);let mut g=fixture();assert!(g.hook(true,p.id,"minecraft:ender_pearl",p.data));assert_eq!(g.creatures.projectiles[0].owner,Some(Who::Mob(42)));}
#[test]fn raw_zero_local_entity_is_not_misread_as_plus_one_or_no_owner(){let p=packet(0);let mut g=fixture();g.hook(true,p.id,"minecraft:ender_pearl",p.data);assert_eq!(g.creatures.projectiles[0].owner,Some(Who::Player));let mut g=fixture();let p=packet(1);g.hook(true,p.id,"minecraft:ender_pearl",p.data);assert_eq!(g.creatures.projectiles[0].owner,None);}
#[test]fn rejected_unknown_negative_or_wrong_native_id_cannot_assign_owner(){for(data,id,accepted)in[(999,111,true),(-1,111,true),(42,-1,true),(42,112,true),(42,111,false)]{let mut g=fixture();assert_eq!(g.hook(accepted,id,"minecraft:ender_pearl",data),accepted);assert_eq!(g.creatures.projectiles[0].owner,None);}}
#[test]fn bridge_requires_both_packet_kind_and_actual_created_native_pearl(){let mut g=fixture();g.hook(true,111,"minecraft:wind_charge",42);assert_eq!(g.creatures.projectiles[0].owner,None);g.creatures.projectiles[0].kind=ProjectileKind::Egg;g.hook(true,111,"minecraft:ender_pearl",42);assert_eq!(g.creatures.projectiles[0].owner,None);}
#[test]fn only_present_current_session_entities_can_resolve_and_no_owner_cache_leaks(){let mut g=fixture();g.hook(true,111,"minecraft:ender_pearl",42);assert_eq!(g.creatures.projectiles[0].owner,Some(Who::Mob(42)));g.creatures.projectiles=vec![native_pearl()];g.mobs.clear();g.remote.as_mut().unwrap().entity_id=99;g.hook(true,111,"minecraft:ender_pearl",42);assert_eq!(g.creatures.projectiles[0].owner,None);g.mobs.push(game::Mob{id:42,removed:true});g.hook(true,111,"minecraft:ender_pearl",42);assert_eq!(g.creatures.projectiles[0].owner,None);g.remote=None;g.mobs[0].removed=false;g.hook(true,111,"minecraft:ender_pearl",42);assert_eq!(g.creatures.projectiles[0].owner,None);}
#[test]fn metadata_bridge_leaves_native_identity_geometry_velocity_age_unchanged(){let mut g=fixture();let p=&g.creatures.projectiles[0];let before=(p.id,p.position,p.velocity,p.age,p.kind.clone());g.hook(true,111,"minecraft:ender_pearl",42);let p=&g.creatures.projectiles[0];assert_eq!((p.id,p.position,p.velocity,p.age,p.kind.clone()),before);assert_eq!(p.owner,Some(Who::Mob(42)));}
#[test]fn master_off_and_nonwindow_game_leave_existing_native_metadata_alone(){for mode in 0..2{let mut g=fixture();if mode==0{g.master=false;}else{g.window=false;}assert!(g.hook(true,111,"minecraft:ender_pearl",42));assert_eq!(g.creatures.projectiles[0].owner,None);}}
#[test]fn actual_remote_tick_owner_metadata_cannot_invoke_local_teleport_or_damage(){let mut outcomes=Vec::new();for owner in[None,Some(Who::Player),Some(Who::Mob(42))]{let mut pearl=native_pearl();pearl.owner=owner;for _ in 0..20{physics::remote_projectile_tick(&mut pearl,&Empty);}outcomes.push((pearl.position,pearl.velocity,pearl.age));}assert_eq!(outcomes[0],outcomes[1]);assert_eq!(outcomes[0],outcomes[2]);}

#[test]fn diagnostic_last_record_survives_expiry_but_never_crosses_scene(){let mut g=fixture();g.hook(true,111,"minecraft:ender_pearl",42);g.creatures.projectiles.clear();g.ticks=999;assert_eq!(g.trace(),Some((100,0,111,42,Some(Who::Mob(42)),"BoundMob".into())));let counters=g.remote.as_ref().unwrap().session.counters.clone();g.remote.as_mut().unwrap().session.counters=std::sync::Arc::new(std::sync::Mutex::new(Some(0)));assert!(g.trace().is_none());g.remote.as_mut().unwrap().session.counters=counters;g.dim=game::Dim::Nether;assert!(g.trace().is_none());g.dim=game::Dim::Overworld;g.remote.as_mut().unwrap().entity_id=1;assert!(g.trace().is_none());g.remote=None;assert!(g.trace().is_none());}
#[test]fn diagnostic_records_actual_mapping_raw_id_failure_reason_and_unchanged_gate(){for(accepted,id,data,reason,owner)in[(true,111,42,"BoundMob",Some(Who::Mob(42))),(true,111,0,"BoundLocal",Some(Who::Player)),(true,111,999,"UnknownOwner",None),(true,112,42,"MissingNative",Some(Who::Mob(42))),(false,111,42,"Rejected",None),(true,-1,42,"InvalidSpawn",None),(true,111,-1,"InvalidOwner",None)]{let mut g=fixture();assert_eq!(g.hook(accepted,id,"minecraft:ender_pearl",data),accepted);let trace=g.trace().unwrap();assert_eq!((trace.1,trace.2,trace.3,trace.4,trace.5),(0,id,data,owner,reason.into()));g.master=false;g.ticks=200;g.hook(true,111,"minecraft:ender_pearl",0);assert_eq!(g.trace().unwrap().0,100);}}

"""
 remote=(a.engine/'crates/horizon-client/src/game/remote_projectiles.rs').read_text(encoding='utf-8');remote_tick=function(remote,'pub(super) fn remote_projectile_tick(')
 code=code.replace('REMOTE_TICK',remote_tick).replace('OWNER',(production/'pearl_owner.rs').as_posix()).replace('HOOK',"fn sunset_owner_metadata(value:bool,game:&mut crate::game::Game<'_>,id:i32,kind:&str,data:i32)->bool{crate::game::sunset_pearl_owner::accepted_spawn(game,value,id,kind,data)}")
 deps={name:sorted(a.dependency_dir.glob('lib'+name+'-*.rlib'),key=lambda f:f.stat().st_mtime,reverse=True)[0]for name in['horizon_client','horizon_core','horizon_protocol']}
 command=['rustc','--edition=2024','--test','SOURCE','-L','dependency='+str(a.dependency_dir)]
 for name,path in deps.items():command+=['--extern',name+'='+str(path)]
 for library in(Path.home()/'.cargo/registry/src').glob('*/windows_x86_64_msvc-*/lib/windows.0.*.lib'):command+=['-L','native='+str(library.parent)]
 with tempfile.TemporaryDirectory(prefix='native-pearl-owner-')as tmp:
  f=Path(tmp)/'test.rs';binary=Path(tmp)/'test.exe';f.write_text(code,encoding='utf-8');r=subprocess.run([str(f)if x=='SOURCE'else x for x in command]+['-o',str(binary)],capture_output=True,text=True,encoding='utf-8');assert r.returncode==0,r.stderr
  r=subprocess.run([str(binary),'--test-threads=1'],capture_output=True,text=True,encoding='utf-8');print(r.stdout);assert r.returncode==0,r.stdout+r.stderr
 native=(a.engine/'crates/horizon-client/src/game/remote_projectiles.rs').read_text(encoding='utf-8');assert 'pub(super) fn spawn_remote_projectile('in native and 'data: i32' in native and 'let mut p = Projectile::new(k, at, None);'in native
 manifest=(production/'horizon-mixin.toml').read_text(encoding='utf-8');start=manifest.index('id = "pearl-owner-spawn"');end=manifest.find('[[injection]]',start+1);entry=manifest[start:end if end>=0 else len(manifest)]
 assert 'kind = "HEAD"' in entry and 'cancellable = true' in entry and "horizon_client::game::remote_projectiles::Game<'_>::spawn_remote_projectile" in entry and 'value = "id"' in entry
 assert 'name = "sunset_pearl_owner"' in manifest and 'source = "pearl_owner.rs"' in manifest and 'sunset_pearl_owner::accepted_spawn(' in (production/'pearl_owner_hook.rs').read_text(encoding='utf-8')
 assert 'targets: Vec::new()' in function(native,'pub(super) fn tick_remote_projectiles(') and 'p.velocity = Vec3::ZERO' in remote_tick and 'teleport' not in remote_tick and 'hurt' not in remote_tick
 print('source-route contract: 4 passed')

if __name__=='__main__':main()
