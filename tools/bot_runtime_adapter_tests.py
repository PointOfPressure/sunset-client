"""Compiled production predicates/invocation adapters; no runtime/game access."""
from pathlib import Path
import argparse,json,subprocess,tempfile

def block(text,start):
 a=text.index(start);b=text.index('{',a);depth=1
 for i in range(b+1,len(text)):
  depth+=(text[i]=='{')-(text[i]=='}')
  if not depth:return text[a:i+1]
 raise AssertionError(start)
def main():
 p=argparse.ArgumentParser();p.add_argument('engine',type=Path);a=p.parse_args();root=Path(__file__).resolve().parent;repository=root.parent;production=repository/'mixins/sunset'
 s=(production/'crystal_bot.rs').read_text(encoding='utf-8');policy=(production/'bot_tactics_policy.rs').read_text(encoding='utf-8')
 guard=block(s,'pub(super) fn suppress_idle_abort(');body=s[s.index('    let food_distance ='):s.index('    if game.using.is_some() && !eating')]
 # Actual production call block, retaining original gating order; only its IO dependencies are instrumented.
 food='fn food_adapter(game:&mut Game,feet:f64,box_distance:f64)->(bool,bool){let own=[0.;3];let target=Target{pos:[feet,0.,0.]};let distance=box_distance;let life=3.;let m=Module;let using=game.using.is_some();let view=View{totems:0};'+body+'(eating,critical_retreat)}'
 critical=block(policy,'pub fn critical_food_retreat(');view=block(policy,'pub struct CriticalFoodView')
 source=r"""
#![allow(dead_code,unused_variables)]
use std::cell::RefCell;
struct Game<'a>{ticks:u64,selected:usize,ready:bool,scene:u64,progress:Option<([i32;3],f32)>,using:Option<()>,_life:std::marker::PhantomData<&'a()>}
struct Finish(bool);impl Finish{fn current(&self)->bool{self.0}}
struct Mine{done:Finish,cell:[i32;3],slot:usize,began:u64,applied:bool}
struct Pause(bool);impl Pause{fn state(&self,_:u64)->tactics::Pause{if self.0{tactics::Pause::Go}else{tactics::Pause::Hold}}}
struct Fight{needs_food:bool}struct Tactics{pause:Pause,mine:Option<Mine>,fight:Fight}
struct State{session:Option<u64>,tactics:Tactics,food:Option<()>,stops:u32}
thread_local!{static STATE:RefCell<State>=RefCell::new(fresh());}
fn fresh()->State{State{session:Some(9),tactics:Tactics{pause:Pause(true),mine:Some(Mine{done:Finish(true),cell:[4,121,0],slot:0,began:10,applied:true}),fight:Fight{needs_food:true}},food:None,stops:0}}
fn state<R>(f:impl FnOnce(&mut State)->R)->R{STATE.with(|s|f(&mut s.borrow_mut()))}
fn game()->Game<'static>{state(|s|*s=fresh());Game{ticks:20,selected:0,ready:true,scene:9,progress:Some(([4,121,0],0.2)),using:None,_life:std::marker::PhantomData}}
fn action_ready(g:&Game)->bool{g.ready}fn same(g:&Game,s:&u64)->bool{g.scene==*s}
mod remote{pub mod sunset_city_mining{pub fn progress(g:&super::super::Game,p:[i32;3])->Option<f32>{g.progress.filter(|(cell,_)|*cell==p).map(|(_,v)|v)}}}
mod tactics{#[derive(PartialEq)]pub enum Pause{Go,Hold}VIEW CRITICAL}
mod util{pub fn dist(a:[f64;3],b:[f64;3])->f64{a.iter().zip(b).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()}}
struct Target{pos:[f64;3]}struct Module;impl Module{fn n(&self,_:&str)->f64{14.}}struct View{totems:u32}impl View{fn offhand_totem(&self)->bool{false}}
fn food_tick(_: &mut Game,want:bool)->bool{let owned=state(|s|s.food.is_some());if want||owned{state(|s|s.food=Some(()));true}else{false}}
fn stop_food(_: &mut Game,_:bool){state(|s|{s.food=None;s.stops+=1;});}
fn hand_has(_: &Game,_:&str,_:bool)->bool{true}
mod bot{use super::*;GUARD FOOD
#[test]fn owned_native_idle_abort_is_suppressed(){let g=game();assert!(suppress_idle_abort(&g,false));assert!(!suppress_idle_abort(&g,true));}
#[test]fn manual_or_unready_yields(){let mut g=game();g.ready=false;assert!(!suppress_idle_abort(&g,false));}
#[test]fn exact_scene_and_live_finish_required(){let mut g=game();g.scene=10;assert!(!suppress_idle_abort(&g,false));g.scene=9;state(|s|s.tactics.mine.as_mut().unwrap().done.0=false);assert!(!suppress_idle_abort(&g,false));}
#[test]fn borrowed_tool_and_accepted_step_required(){let mut g=game();g.selected=2;assert!(!suppress_idle_abort(&g,false));g.selected=0;state(|s|s.tactics.mine.as_mut().unwrap().applied=false);assert!(!suppress_idle_abort(&g,false));}
#[test]fn pause_or_clock_boundary_cannot_extend_mine(){let mut g=game();state(|s|s.tactics.pause.0=false);assert!(!suppress_idle_abort(&g,false));state(|s|s.tactics.pause.0=true);g.ticks=9;assert!(!suppress_idle_abort(&g,false));g.ticks=90;assert!(suppress_idle_abort(&g,false));g.ticks=91;assert!(!suppress_idle_abort(&g,false));}
#[test]fn authoritative_native_cell_must_match(){let mut g=game();g.progress=None;assert!(!suppress_idle_abort(&g,false));g.progress=Some(([5,121,0],0.3));assert!(!suppress_idle_abort(&g,false));state(|s|s.tactics.mine=None);assert!(!suppress_idle_abort(&g,false));}
#[test]fn feet_eight_starts_food_even_when_eye_box_is_less(){let mut g=game();assert_eq!(food_adapter(&mut g,8.,7.7),(true,false));assert_eq!(state(|s|s.stops),0);}
#[test]fn below_eight_keeps_critical_retreat(){let mut g=game();assert_eq!(food_adapter(&mut g,7.99,7.69),(false,true));}
#[test]fn started_native_food_survives_one_tick_boundary_jitter(){let mut g=game();assert_eq!(food_adapter(&mut g,8.01,7.71),(true,false));g.using=Some(());assert_eq!(food_adapter(&mut g,7.99,7.69),(true,false));assert_eq!(state(|s|s.stops),0);}
}
"""
 # View.offhand_totem is a real field in production, preserving expression unchanged.
 source=source.replace('struct View{totems:u32}','struct View{totems:u32,offhand_totem:bool}').replace('View{totems:0}','View{totems:0,offhand_totem:false}')
 source=source.replace('VIEW',view).replace('CRITICAL',critical).replace('GUARD',guard).replace('FOOD',food).replace('View{totems:0}','View{totems:0,offhand_totem:false}')
 with tempfile.TemporaryDirectory(prefix='bot-native-guard-')as tmp:
  f=Path(tmp)/'test.rs';f.write_text(source,encoding='utf-8');binary=Path(tmp)/'test.exe'
  r=subprocess.run(['rustc','--edition=2024','--test',str(f),'-o',str(binary)],capture_output=True,text=True,encoding='utf-8');assert r.returncode==0,r.stderr
  r=subprocess.run([str(binary)],capture_output=True,text=True,encoding='utf-8');print(r.stdout);assert r.returncode==0,r.stderr
 action=''.join(block(s,'fn action_ready(').split());assert 'game.physical_buttons' in action and 'game.physical_keys' in action and '!sunset::gui_open()' in action
 remote=(a.engine/'crates/horizon-client/src/game/remote.rs').read_text(encoding='utf-8');native=block(remote,'fn remote_continue_attack(');assert 'self.stop_remote_mining();' in native and 'self.mining = None;' in native
 hook=(production/'city_continue_hook.rs').read_text(encoding='utf-8');assert '|| crate::game::sunset_crystal_bot::suppress_idle_abort(game, down)' in hook
 assert 'if food_distance < 8. { -1 } else { 0 }' in s
 print('source-route contract: 4 passed')
if __name__=='__main__':main()
