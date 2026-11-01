"""Compile the actual HEAD wrapper with an instrumented native body and real projectile."""
from pathlib import Path
import argparse,subprocess,tempfile,json

def main():
 p=argparse.ArgumentParser();p.add_argument('--dependency-dir',type=Path,required=True);a=p.parse_args();root=Path(__file__).resolve().parent;production=root.parent/'mixins/sunset'
 code=r"""
#![allow(dead_code)]
use horizon_client::{projectiles::{Projectile,ProjectileKind},mobs::Who};use horizon_core::geometry::Vec3;
mod game{use super::*;pub mod sunset{pub const ID:&str="sunset";}pub struct Session{pub counters:std::sync::Arc<std::sync::Mutex<Option<u8>>>}pub struct Remote{pub entity_id:i32,pub session:Session}#[derive(Clone,Copy,PartialEq)]pub enum Dim{Overworld,Nether}pub struct Mob{pub id:u32,pub removed:bool}pub struct Creatures{pub projectiles:Vec<Projectile>}
pub struct Game<'a>{pub dim:Dim,pub ticks:u64,pub window:bool,pub master:bool,pub remote:Option<Remote>,pub player:Option<()>,pub mobs:Vec<Mob>,pub creatures:Creatures,pub calls:u32,pub args:Option<(i32,String,[f64;3],(f32,f32),[f64;3],i32)>,pub reject:bool,pub panic_once:bool,pub life:std::marker::PhantomData<&'a()>}
#[path="OWNER"]pub mod sunset_pearl_owner;
HEAD
impl Game<'_>{pub fn native_window(&self)->bool{self.window}pub fn native_on(&self,_:&str)->bool{self.master}
pub fn spawn_remote_projectile(&mut self,id:i32,kind:&str,position:[f64;3],rotation:(f32,f32),velocity:[f64;3],data:i32)->bool{
 if let std::ops::ControlFlow::Break(value)=sunset_restore_pearl_owner(self,id,kind,position,rotation,velocity,data){return value;}
 // Instrumented original body; production Projectile construction remains real.
 self.calls+=1;self.args=Some((id,kind.into(),position,rotation,velocity,data));if self.panic_once{self.panic_once=false;panic!("injected original unwind");}if self.reject{return false;}
 let k=match kind{"minecraft:ender_pearl"=>ProjectileKind::EnderPearl,"minecraft:egg"=>ProjectileKind::Egg,_=>return false};
 self.creatures.projectiles.retain(|p|p.id!=id as u32);let mut pearl=Projectile::new(k,Vec3::new(position[0],position[1],position[2]),None);pearl.id=id as u32;pearl.velocity=Vec3::new(velocity[0],velocity[1],velocity[2]);pearl.yaw=rotation.0;pearl.pitch=rotation.1;self.creatures.projectiles.push(pearl);true
}}}
fn fixture()->game::Game<'static>{game::Game{dim:game::Dim::Overworld,ticks:100,window:true,master:true,remote:Some(game::Remote{entity_id:0,session:game::Session{counters:std::sync::Arc::new(std::sync::Mutex::new(Some(0)))}}),player:Some(()),mobs:vec![game::Mob{id:42,removed:false}],creatures:game::Creatures{projectiles:vec![]},calls:0,args:None,reject:false,panic_once:false,life:std::marker::PhantomData}}
fn spawn(g:&mut game::Game)->bool{g.spawn_remote_projectile(111,"minecraft:ender_pearl",[1.,122.,3.],(-90.,-10.),[1.5,0.2,0.],42)}
#[test]fn original_runs_once_and_exact_original_arguments_are_preserved(){let mut g=fixture();assert!(spawn(&mut g));assert_eq!(g.calls,1);assert_eq!(g.args,Some((111,"minecraft:ender_pearl".into(),[1.,122.,3.],(-90.,-10.),[1.5,0.2,0.],42)));assert_eq!(g.creatures.projectiles.len(),1);assert_eq!(g.creatures.projectiles[0].owner,Some(Who::Mob(42)));}
#[test]fn unrelated_kind_master_off_and_nonwindow_bypass_original_once(){for mode in 0..3{let mut g=fixture();let kind=if mode==0{"minecraft:egg"}else{"minecraft:ender_pearl"};if mode==1{g.master=false;}if mode==2{g.window=false;}assert!(g.spawn_remote_projectile(111,kind,[0.;3],(0.,0.),[0.;3],42));assert_eq!(g.calls,1);assert_eq!(g.creatures.projectiles[0].owner,None);}}
#[test]fn original_rejection_is_propagated_once_without_metadata(){let mut g=fixture();g.reject=true;assert!(!spawn(&mut g));assert_eq!(g.calls,1);assert!(g.creatures.projectiles.is_empty());}
#[test]fn unwind_drop_guard_restores_future_invocation_metadata(){let mut g=fixture();g.panic_once=true;assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||spawn(&mut g))).is_err());assert!(spawn(&mut g));assert_eq!(g.calls,2);assert_eq!(g.creatures.projectiles[0].owner,Some(Who::Mob(42)));}
#[test]fn original_negative_id_is_not_shadow_reinterpreted_by_wrapper(){let mut g=fixture();assert!(g.spawn_remote_projectile(-7,"minecraft:ender_pearl",[0.;3],(0.,0.),[0.;3],42));assert_eq!(g.calls,1);assert_eq!(g.args.as_ref().unwrap().0,-7);assert_eq!(g.creatures.projectiles[0].owner,None);}
#[test]fn reentry_state_is_per_thread_and_never_suppresses_another_owner(){let threads=(0..2).map(|_|std::thread::spawn(||{let mut g=fixture();assert!(spawn(&mut g));assert_eq!(g.calls,1);assert_eq!(g.creatures.projectiles[0].owner,Some(Who::Mob(42)));})).collect::<Vec<_>>();for thread in threads{thread.join().unwrap();}}
"""
 code=code.replace('OWNER',(production/'pearl_owner.rs').as_posix()).replace('HEAD',(production/'pearl_owner_hook.rs').read_text(encoding='utf-8'))
 deps={name:sorted(a.dependency_dir.glob('lib'+name+'-*.rlib'),key=lambda f:f.stat().st_mtime,reverse=True)[0]for name in['horizon_client','horizon_core']};command=['rustc','--edition=2024','--test','SOURCE','-L','dependency='+str(a.dependency_dir)]
 for name,path in deps.items():command+=['--extern',name+'='+str(path)]
 for library in(Path.home()/'.cargo/registry/src').glob('*/windows_x86_64_msvc-*/lib/windows.0.*.lib'):command+=['-L','native='+str(library.parent)]
 with tempfile.TemporaryDirectory(prefix='owner-head-invoke-')as tmp:
  f=Path(tmp)/'test.rs';binary=Path(tmp)/'test.exe';f.write_text(code,encoding='utf-8');r=subprocess.run([str(f)if x=='SOURCE'else x for x in command]+['-o',str(binary)],capture_output=True,text=True,encoding='utf-8');assert r.returncode==0,r.stderr
  r=subprocess.run([str(binary)],capture_output=True,text=True,encoding='utf-8');print(r.stdout);assert r.returncode==0,r.stdout+r.stderr
 whole=(production/'horizon-mixin.toml').read_text(encoding='utf-8');start=whole.index('id = "pearl-owner-spawn"');end=whole.find('[[injection]]',start+1);manifest=whole[start:end if end>=0 else len(whole)];assert 'kind = "HEAD"'in manifest and 'cancellable = true'in manifest and '$value'not in manifest and ' as 'not in manifest
 hook=(production/'pearl_owner_hook.rs').read_text(encoding='utf-8');assert hook.count('game.spawn_remote_projectile(')==1 and 'impl Drop for Reset' in hook and hook.index('kind != "minecraft:ender_pearl"')<hook.index('SUNSET_PEARL_SPAWN_PASS.with')
 print('source-route contract: 2 passed')

if __name__=='__main__':main()
