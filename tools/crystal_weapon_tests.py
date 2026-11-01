"""Compile actual production shared Aura hand ownership in narrow adapters.

No native game starts. The extracted scene/restore/take/idle functions exercise
cross-Aura ownership transfer, user edits, emergency deferral and cleanup.
"""
from pathlib import Path
import subprocess,tempfile

def function(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1;cursor=opening+1
    while depth:depth+=(text[cursor]=='{')-(text[cursor]=='}');cursor+=1
    return text[start:cursor]

def main():
    source=Path(__file__).resolve().parents[1]/'mixins/sunset'
    text=(source/'crystal_aura.rs').read_text()
    helpers='\n'.join(function(text,s) for s in ['fn same_setup_scene(','fn restore_weapon(','pub(super) fn restore_shared(','pub(super) fn take_shared(','pub(super) fn idle_shared(','fn weapon_hold('])
    harness = r'''
#![allow(dead_code)]
use std::{any::Any,sync::{Arc,Mutex}};
#[path="POLICY"]mod sunset_macro_policy;
struct Health(f32);impl Health{fn health(&self)->f32{self.0}}
struct Player{health:Health}
struct Session{counters:Arc<Mutex<Option<Arc<()>>>>}
struct Remote{entity_id:i32,session:Session}
struct Game<'a>{selected:usize,dim:u8,ticks:u64,remote:Option<Remote>,player:Option<Player>,using:Option<()>,holds:Vec<usize>,marker:std::marker::PhantomData<&'a ()>}
mod util{pub const OFFHAND:usize=40;}
mod sunset_combat_control{pub static OWNER:std::sync::Mutex<Option<&'static str>>=std::sync::Mutex::new(None);pub fn owner()->Option<&'static str>{*OWNER.lock().unwrap()}}
mod sunset_player{pub static GAP:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);pub fn gap_eating()->bool{GAP.load(std::sync::atomic::Ordering::Relaxed)}}
mod sunset{pub fn hold(game:&mut super::Game<'_>,slot:usize){game.selected=slot;game.holds.push(slot);}}
mod sunset_combat_macros{pub fn note_main(_: &super::Game<'_>){}}
mod aura{
use super::*;
const NAME:&str="Crystal Aura";
type SetupScene=(i32,u8,Arc<dyn Any+Send+Sync>);
struct WeaponLease{scene:SetupScene,original:usize,owned:usize,owner:&'static str,until:u64}
struct State{weapon:Option<WeaponLease>}
static STATE:Mutex<State>=Mutex::new(State{weapon:None});
fn st<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.lock().unwrap())}
HELPERS
#[cfg(test)]mod tests{
use super::*;
fn fixture()->Game<'static>{sunset_player::GAP.store(false,std::sync::atomic::Ordering::Relaxed);st(|s|s.weapon=None);*sunset_combat_control::OWNER.lock().unwrap()=None;
 Game{selected:0,dim:1,ticks:0,remote:Some(Remote{entity_id:7,session:Session{counters:Arc::new(Mutex::new(None))}}),
 player:Some(Player{health:Health(20.)}),using:None,holds:Vec::new(),marker:std::marker::PhantomData}}
#[test]fn alternating_explosives_preserve_the_first_user_slot(){let mut g=fixture();take_shared(&mut g,5,NAME);take_shared(&mut g,3,"Anchor Aura");take_shared(&mut g,8,NAME);restore_weapon(&mut g,true);assert_eq!(g.selected,0);assert_eq!(g.holds,[5,3,8,0]);}
#[test]fn disabling_other_aura_does_not_erase_current_owner(){let mut g=fixture();take_shared(&mut g,5,NAME);restore_shared(&mut g,"Anchor Aura",true);assert_eq!(g.selected,5);assert_eq!(st(|s|s.weapon.as_ref().unwrap().original),0);restore_weapon(&mut g,true);assert_eq!(g.selected,0);}
#[test]fn emergency_owner_defers_return_then_original_can_be_restored(){for mode in 0..2{let mut g=fixture();take_shared(&mut g,5,NAME);if mode==0{g.using=Some(())}else{sunset_player::GAP.store(true,std::sync::atomic::Ordering::Relaxed)}restore_weapon(&mut g,true);assert_eq!(g.selected,5);assert!(st(|s|s.weapon.is_some()));g.using=None;sunset_player::GAP.store(false,std::sync::atomic::Ordering::Relaxed);restore_weapon(&mut g,true);assert_eq!(g.selected,0);}let mut g=fixture();take_shared(&mut g,5,NAME);*sunset_combat_control::OWNER.lock().unwrap()=Some("Auto Totem");restore_weapon(&mut g,true);assert_eq!(g.selected,5);assert!(st(|s|s.weapon.is_some()));*sunset_combat_control::OWNER.lock().unwrap()=None;restore_weapon(&mut g,true);assert_eq!(g.selected,0);}
#[test]fn physical_slot_override_drops_old_baseline(){let mut g=fixture();take_shared(&mut g,5,NAME);g.selected=4;restore_weapon(&mut g,true);assert_eq!(g.selected,4);assert!(st(|s|s.weapon.is_none()));}
#[test]fn new_connection_or_dimension_cannot_receive_old_return(){for dim in [false,true]{let mut g=fixture();take_shared(&mut g,5,NAME);if dim{g.dim=2}else{g.remote.as_mut().unwrap().session.counters=Arc::new(Mutex::new(None));}restore_weapon(&mut g,true);assert_eq!(g.selected,5);assert!(st(|s|s.weapon.is_none()));}}
#[test]fn idle_window_is_ten_full_ticks_and_owner_specific(){let mut g=fixture();take_shared(&mut g,5,NAME);g.ticks=10;idle_shared(&mut g,NAME,true);assert_eq!(g.selected,5);g.ticks=11;idle_shared(&mut g,"Anchor Aura",true);assert_eq!(g.selected,5);idle_shared(&mut g,NAME,true);assert_eq!(g.selected,0);}
#[test]fn switch_back_false_keeps_hand_and_forgets_baseline(){let mut g=fixture();take_shared(&mut g,5,NAME);restore_weapon(&mut g,false);assert_eq!(g.selected,5);assert!(st(|s|s.weapon.is_none()));}
#[test]fn dead_player_gets_no_old_slot_return(){let mut g=fixture();take_shared(&mut g,5,NAME);g.player.as_mut().unwrap().health.0=0.;restore_weapon(&mut g,true);assert_eq!(g.selected,5);assert!(st(|s|s.weapon.is_none()));}
}}

'''

    harness=harness.replace('POLICY',(source/'macro_policy.rs').as_posix()).replace('HELPERS',helpers)
    with tempfile.TemporaryDirectory(prefix='sunset-aura-hand-') as temp:
        src=Path(temp)/'hand.rs';exe=Path(temp)/'hand.exe';src.write_text(harness)
        subprocess.run(['rustc','--edition=2024','--test',str(src),'-o',str(exe)],check=True)
        subprocess.run([str(exe),'--test-threads=1'],check=True)

if __name__=='__main__':main()
