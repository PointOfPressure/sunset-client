"""Compile the actual macro press observer with renderer-free input adapters."""
from pathlib import Path
import subprocess
import tempfile

source = (Path(__file__).resolve().parents[1]/'mixins/sunset/combat_macros.rs').read_text(encoding='utf-8')
def function(start):
    begin = source.index(start)
    brace = source.index('{', begin)
    depth, end = 1, brace+1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[begin:end]

code = r'''
#![allow(dead_code)]
use std::sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}};
#[derive(PartialEq)]enum MouseButton{Right,Left}
struct Session{counters:Arc<Mutex<Option<()>>>}
struct Remote{session:Session,entity_id:i32}
struct Game<'a>{ticks:u64,dim:u8,remote:Option<Remote>,playing:bool,on:bool,allowed:bool,marker:std::marker::PhantomData<&'a ()>}
impl Game<'_>{fn native_playing(&self)->bool{self.playing}fn native_on(&self,_:&str)->bool{self.on}}
mod sunset{pub const ID:&str="sunset";}
mod sunset_combat_control{
 pub static BUSY:super::AtomicBool=super::AtomicBool::new(false);
 pub fn owner()->Option<&'static str>{BUSY.load(super::Ordering::Relaxed).then_some("other")}
}
mod macros{
use super::*;
const CRYSTAL:&str="Crystal Macro";const ANCHOR:&str="Anchor Macro";
type Session=Arc<Mutex<Option<()>>>;
struct AnchorPress{tick:u64,session:Session,entity:i32,dimension:u8,start_allowed:bool}
struct State{anchor_press:Option<AnchorPress>}
static STATE:Mutex<State>=Mutex::new(State{anchor_press:None});
static USER_RIGHT:AtomicBool=AtomicBool::new(false);
static GUI:AtomicBool=AtomicBool::new(false);
fn state<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.lock().unwrap())}
fn gui_open()->bool{GUI.load(Ordering::Relaxed)}
fn allowed(g:&Game<'_>)->bool{g.allowed}
'''
code += function('pub(super) fn user_mouse(')+function('pub(super) fn focus_lost()')
code += r'''
#[cfg(test)]mod tests{
use super::*;
fn game()->Game<'static>{Game{ticks:4,dim:1,remote:Some(Remote{session:super::super::Session{counters:Arc::new(Mutex::new(None))},entity_id:7}),playing:true,on:true,allowed:true,marker:std::marker::PhantomData}}
#[test]fn paused_or_leased_quick_edge_can_guard_but_cannot_replay_as_start(){
 let mut g=game();focus_lost();g.allowed=false;
 user_mouse(&g,MouseButton::Right,true);user_mouse(&g,MouseButton::Right,false);
 assert!(!USER_RIGHT.load(Ordering::Relaxed));g.allowed=true;
 state(|s|{let p=s.anchor_press.take().unwrap();assert!(!p.start_allowed);assert_eq!(p.tick,4);assert_eq!(p.entity,7);assert!(Arc::ptr_eq(&p.session,&g.remote.as_ref().unwrap().session.counters));});
 sunset_combat_control::BUSY.store(true,Ordering::Relaxed);
 user_mouse(&g,MouseButton::Right,true);state(|s|assert!(!s.anchor_press.take().unwrap().start_allowed));
 sunset_combat_control::BUSY.store(false,Ordering::Relaxed);focus_lost();
}
#[test]fn eligible_edge_can_start_but_master_menu_world_and_focus_boundaries_do_not_latch(){
 let mut g=game();focus_lost();user_mouse(&g,MouseButton::Right,true);
 state(|s|assert!(s.anchor_press.take().unwrap().start_allowed));focus_lost();
 g.on=false;user_mouse(&g,MouseButton::Right,true);state(|s|assert!(s.anchor_press.is_none()));
 g.on=true;GUI.store(true,Ordering::Relaxed);user_mouse(&g,MouseButton::Right,true);state(|s|assert!(s.anchor_press.is_none()));
 GUI.store(false,Ordering::Relaxed);g.playing=false;user_mouse(&g,MouseButton::Right,true);state(|s|assert!(s.anchor_press.is_none()));
 g.playing=true;user_mouse(&g,MouseButton::Right,true);focus_lost();state(|s|assert!(s.anchor_press.is_none()));assert!(!USER_RIGHT.load(Ordering::Relaxed));
}
}
}
'''
with tempfile.TemporaryDirectory(prefix='sunset-loot-press-') as directory:
    rust, binary = Path(directory)/'press.rs', Path(directory)/'press.exe'
    rust.write_text(code, encoding='utf-8')
    subprocess.run(['rustc','--edition=2024','--test',str(rust),'-o',str(binary)],check=True)
    subprocess.run([str(binary),'--test-threads=1'],check=True)
