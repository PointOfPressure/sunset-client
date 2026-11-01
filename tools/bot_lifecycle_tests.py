"""Compile the real production suspend/stop routines against instrumented effects.

This is an adapter lifecycle test, not a native Game or network execution test.
"""
from pathlib import Path
import subprocess,tempfile

def function(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1
    for i in range(opening+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[start:i+1]
    raise AssertionError(signature)

def main():
    root=Path(__file__).resolve().parent
    source=(root.parent/'mixins/sunset/crystal_bot.rs').read_text(encoding='utf-8')
    extracted='\n'.join(function(source,s) for s in ['fn tactical_suspend(','fn stop_mine(','fn release_pause_lease(','fn refresh_pause_lease('])
    code=r'''
#![allow(dead_code)]
mod game {
 use std::{marker::PhantomData,time::Instant};
 pub struct Game<'a>{pub selected:usize,pub scene:u64,pub ticks:u64,pub stops:Vec<[i32;3]>,pub restored:Vec<usize>,pub life:PhantomData<&'a()>}
 pub mod remote{pub mod sunset_city_mining{pub fn stop(g:&mut super::super::Game<'_>,p:[i32;3]){g.stops.push(p);}}}
 mod bot {
 use super::Game;use std::{cell::RefCell,time::Instant};
 const BOT_HAND:&str="hands";const BOT_MINE:&str="mining";const BOT_PAUSE:&str="pause";
 #[path="POLICY"]mod tactics;
 mod control{use std::cell::Cell;thread_local!{pub static FINISHES:Cell<u64>=Cell::new(0);pub static CLAIMS:Cell<u64>=Cell::new(0);}pub struct Finish(pub bool,pub u64);impl Finish{pub fn current(&self)->bool{self.0}}pub fn release(_: &str){}pub fn claim(_: &str)->bool{CLAIMS.with(|n|n.set(n.get()+1));true}pub fn finish(_: &str)->Finish{let id=FINISHES.with(|n|{n.set(n.get()+1);n.get()});Finish(true,id)}}
 mod sunset{pub fn cancel_turns(_: &str){}}
 fn hold(g:&mut Game<'_>,slot:usize){g.selected=slot;g.restored.push(slot);}
 fn same(g:&Game<'_>,scene:&u64)->bool{g.scene==*scene}
 struct BotMine{done:control::Finish,cell:[i32;3],slot:usize,previous:usize,began:u64,applied:bool}
 #[derive(Default)]struct Fight{launching_until:Option<u64>}
 #[derive(Default)]struct Tactics{hold_guard:Option<control::Finish>,pause:tactics::PauseClock,generation:u64,queued:Option<Instant>,steer:Option<()>,flight_tick:Option<u64>,pearl:Option<()>,pearl_search:Option<()>,drop_y:Option<f64>,fight:Fight,attack_wait:Option<()>,mine:Option<BotMine>}
 #[derive(Default)]struct State{session:Option<u64>,tactics:Tactics}
 thread_local!{static STATE:RefCell<State>=RefCell::new(State::default());}
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{STATE.with(|s|f(&mut s.borrow_mut()))}
 EXTRACTED
 fn fixture(scene:u64,selected:usize,current:bool)->Game<'static>{state(|s|{*s=State::default();s.session=Some(1);s.tactics.generation=7;s.tactics.queued=Some(Instant::now());s.tactics.steer=Some(());s.tactics.flight_tick=Some(100);s.tactics.mine=Some(BotMine{done:control::Finish(current,100),cell:[1,2,3],slot:1,previous:8,began:0,applied:true});});Game{selected,scene,ticks:1,stops:vec![],restored:vec![],life:std::marker::PhantomData}}
 #[test]fn real_suspend_invalidates_queue_and_restores_only_live_owned_slot(){let mut g=fixture(1,1,true);tactical_suspend(&mut g);assert_eq!(g.stops,[[1,2,3]]);assert_eq!(g.restored,[8]);state(|s|{assert_eq!(s.tactics.generation,8);assert!(s.tactics.mine.is_none());assert!(s.tactics.queued.is_none());assert!(s.tactics.steer.is_none());assert!(s.tactics.flight_tick.is_none());});}
 #[test]fn real_suspend_never_reverts_user_slot_change(){let mut g=fixture(1,2,true);tactical_suspend(&mut g);assert_eq!(g.stops,[[1,2,3]]);assert_eq!(g.selected,2);assert!(g.restored.is_empty());}
 #[test]fn real_suspend_never_stops_or_restores_new_world_from_old_guard(){let mut g=fixture(2,1,true);tactical_suspend(&mut g);assert!(g.stops.is_empty());assert!(g.restored.is_empty());assert_eq!(g.selected,1);}
 #[test]fn real_stop_never_stops_replacement_owner_or_changes_their_slot(){let mut g=fixture(1,1,false);stop_mine(&mut g);assert!(g.stops.is_empty());assert!(g.restored.is_empty());state(|s|assert!(s.tactics.mine.is_none()));}
 #[test]fn real_hold_refresh_retains_exact_guard_and_heartbeats(){let g=fixture(1,1,true);state(|s|s.tactics.pause.context(1,false));refresh_pause_lease(&g);let id=state(|s|s.tactics.hold_guard.as_ref().unwrap().1);refresh_pause_lease(&g);refresh_pause_lease(&g);assert_eq!(state(|s|s.tactics.hold_guard.as_ref().unwrap().1),id);assert_eq!(control::CLAIMS.with(|n|n.get()),3);assert_eq!(control::FINISHES.with(|n|n.get()),1);}
 #[test]fn real_hold_reacquires_after_expired_exact_guard(){let g=fixture(1,1,true);state(|s|{s.tactics.pause.context(1,false);s.tactics.hold_guard=Some(control::Finish(false,100));});refresh_pause_lease(&g);assert_ne!(state(|s|s.tactics.hold_guard.as_ref().unwrap().1),100);}
 #[test]fn real_hold_guard_drops_on_go_yield_and_explicit_cancel(){let mut g=fixture(1,1,true);state(|s|s.tactics.pause.context(1,false));refresh_pause_lease(&g);g.ticks=31;refresh_pause_lease(&g);assert!(state(|s|s.tactics.hold_guard.is_none()));state(|s|s.tactics.pause.context(31,false));refresh_pause_lease(&g);g.ticks=61;state(|s|s.tactics.pause.manual(61));refresh_pause_lease(&g);assert!(state(|s|s.tactics.hold_guard.is_none()));state(|s|s.tactics.pause.context(61,false));refresh_pause_lease(&g);release_pause_lease();assert!(state(|s|s.tactics.hold_guard.is_none()));}
 }
}
'''.replace('EXTRACTED',extracted).replace('POLICY',(root.parent/'mixins/sunset/bot_tactics_policy.rs').as_posix())
    with tempfile.TemporaryDirectory(prefix='sunset-bot-owned-') as temp:
        src=Path(temp)/'tests.rs';exe=Path(temp)/'tests.exe';src.write_text(code,encoding='utf-8')
        subprocess.run(['rustc','--edition=2024','--test',str(src),'-o',str(exe)],check=True)
        subprocess.run([str(exe)],check=True)

if __name__=='__main__':main()
