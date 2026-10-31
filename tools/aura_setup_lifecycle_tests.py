"""Compile real Crystal suspend/reset + Clock + setup Ledger in a narrow shim.

Release/cancel notifications and scene tokens are adapters. The production
lifecycle/state functions are extracted unchanged. Separate source assertions
check actual routing; those assertions are not represented as compiled tests.
No native client, renderer, connection or fixture is started.
"""
from pathlib import Path
import subprocess,tempfile

def function(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1
    for index in range(opening+1,len(text)):
        depth+=(text[index]=='{')-(text[index]=='}')
        if not depth:return text[start:index+1]
    raise AssertionError(signature)

def main():
    root=Path(__file__).resolve().parents[1]/'mixins/sunset'
    crystal=(root/'crystal_aura.rs').read_text(encoding='utf-8')
    shared=(root/'explosive_aura.rs').read_text(encoding='utf-8')
    prepare=function(crystal,'pub(super) fn prepare(')
    assert prepare.index('!same_setup_scene(game, old)')<prepare.index('if !allowed(game)')
    disallowed=function(prepare,'if !allowed(game)')
    timeout=function(prepare,'if st(|s| s.queued_at.is_some_and')
    shared_pause=function(shared,'if !can_act(game)')
    for branch in [disallowed,timeout]:assert 'suspend();' in branch and 'reset();' not in branch
    assert 'sunset_crystal_aura::suspend();' in shared_pause and 'sunset_crystal_aura::reset();' not in shared_pause
    assert 'if crystal.is_none() { super::sunset_crystal_aura::reset(); }' in shared
    assert 'else { reset(); return None; }' in prepare
    packet=function(crystal,'pub(super) fn packet(')
    assert 'C::Login(_) | C::Respawn(_) | C::Disconnect(_)' in packet and '{ reset(); return; }' in packet
    assert 'attempts >= 3' in prepare, 'Production rejected-ID guard must still use the retained retry count'
    print('source-route contract: 1 passed (shared contention, prepare, timeout, explicit boundaries; not compiled)',flush=True)
    state=crystal.split('// BEGIN PURE CRYSTAL STATE\n',1)[1].split('// END PURE CRYSTAL STATE',1)[0]
    suspend=function(crystal,'pub(super) fn suspend(');reset=function(crystal,'pub(super) fn reset(')
    pre=r'''
#![allow(dead_code)]
#[path="POLICY"]mod setup;
mod game {
 pub mod sunset_combat_control {
  pub static RELEASES:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
  pub fn release(owner:&str){assert_eq!(owner,"Crystal Aura");RELEASES.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}
 }
 pub mod sunset {
  pub static CANCELS:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
  pub fn cancel_turns(owner:&str){assert_eq!(owner,"Crystal Aura");CANCELS.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}
 }
 mod aura {
 use super::sunset;use std::{sync::{Mutex,OnceLock},time::Instant};
 const NAME:&str="Crystal Aura";
 STATE
 use state::{Action,Clock};
 struct State { clock:Clock,queued_at:Option<Instant>,plan:Option<()>,attempts:Vec<(u32,u8)>,
  setup:crate::setup::Ledger,setup_scene:Option<u32>,notes:usize }
 impl Default for State {fn default()->Self{Self{clock:Clock::default(),queued_at:None,plan:None,attempts:vec![],
  setup:crate::setup::Ledger::new(),setup_scene:None,notes:0}}}
 static STATE:OnceLock<Mutex<State>>=OnceLock::new();
 fn st<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.get_or_init(||Mutex::new(State::default())).lock().unwrap())}
 fn note(_n:usize){st(|s|s.notes+=1);}
 SUSPEND
 RESET
 #[cfg(test)]mod tests {
 use super::*;use std::sync::atomic::Ordering::SeqCst;
 static SERIAL:Mutex<()>=Mutex::new(());
 const P:[i32;3]=[3,121,0];
 fn fresh(){st(|s|*s=State::default());super::super::sunset_combat_control::RELEASES.store(0,SeqCst);super::super::sunset::CANCELS.store(0,SeqCst);}
 #[test]fn exhausted_unchanged_cell_stays_exhausted_across_repeated_suspends(){
  let _serial=SERIAL.lock().unwrap();fresh();
  st(|s|{s.setup_scene=Some(17);for tick in [0,30,60]{assert!(s.setup.sent(P,0,tick,2));}});
  for _ in 0..20{suspend();st(|s|{s.setup.payload(P,0,false);assert!(!s.setup.can_send(P,1000));assert_eq!(s.setup_scene,Some(17));});}
  assert_eq!(super::super::sunset_combat_control::RELEASES.load(SeqCst),20);
  assert_eq!(super::super::sunset::CANCELS.load(SeqCst),20);
 }
 #[test]fn real_ack_and_cell_delay_survive_suspend(){
  let _serial=SERIAL.lock().unwrap();fresh();st(|s|{s.setup_scene=Some(17);assert!(s.setup.sent(P,0,10,20));
   s.setup.payload(P,49,true);assert!(s.setup.acknowledged(P,true));
   s.clock.sent(Action::Base(P),10,20);s.clock.observe(11,true);});
  suspend();st(|s|{assert!(!s.setup.crystal_allowed(P,29,true));assert!(s.setup.crystal_allowed(P,30,true));assert_eq!(s.setup_scene,Some(17));
   assert!(!s.clock.due(Action::Base(P),29));assert!(s.clock.due(Action::Base(P),30));});
 }
 #[test]fn suspend_invalidates_queued_generation_but_prediction_still_is_not_ack(){
  let _serial=SERIAL.lock().unwrap();fresh();st(|s|{s.setup.sent(P,0,10,20);s.setup_scene=Some(17);
   s.clock.queued=true;s.queued_at=Some(Instant::now());s.plan=Some(());s.attempts.push((7,3));});
  let before=st(|s|s.clock.generation);suspend();
  st(|s|{assert_ne!(s.clock.generation,before);assert!(!s.clock.queued);assert!(s.queued_at.is_none());assert!(s.plan.is_none());
   assert_eq!(s.attempts,vec![(7,3)]);assert!(!s.setup.crystal_allowed(P,50,true));assert!(!s.setup.can_send(P,39));
   s.setup.payload(P,49,true);assert!(s.setup.acknowledged(P,true));assert!(s.setup.crystal_allowed(P,50,true));});
 }
 #[test]fn rejected_crystal_attempts_and_send_deadlines_survive_repeated_temporary_pauses(){
  let _serial=SERIAL.lock().unwrap();fresh();
  st(|s|{s.attempts=vec![(77,3),(88,2)];s.clock.cursor=31;s.clock.crystals=9;
   s.clock.sent(Action::Break(77,[4.5,121.0,0.5]),100,45);s.clock.queued=true;
   s.queued_at=Some(Instant::now());s.plan=Some(());});
  for _ in 0..20{suspend();st(|s|{
   assert_eq!(s.attempts,vec![(77,3),(88,2)]);assert_eq!(s.clock.cursor,31);assert_eq!(s.clock.crystals,9);
   let waiting=s.clock.waiting.unwrap();assert_eq!(waiting.action,Action::Break(77,[4.5,121.0,0.5]));assert_eq!(waiting.sent,100);
   assert_eq!(s.clock.next_break,145);assert!(!s.clock.queued);assert!(s.queued_at.is_none());assert!(s.plan.is_none());
  });}
  st(|s|{s.clock.observe(129,false);assert!(!s.clock.available());s.clock.observe(130,false);assert!(s.clock.available());
   assert!(!s.clock.due(Action::Break(77,[4.5,121.0,0.5]),144));assert!(s.clock.due(Action::Break(77,[4.5,121.0,0.5]),145));
   assert_eq!(s.attempts,vec![(77,3),(88,2)]);});
  assert_eq!(super::super::sunset_combat_control::RELEASES.load(SeqCst),20);
  assert_eq!(super::super::sunset::CANCELS.load(SeqCst),20);
 }
 #[test]fn full_ledger_is_not_evicted_by_contention(){
  let _serial=SERIAL.lock().unwrap();fresh();st(|s|for x in 0..64{assert!(s.setup.sent([x,121,0],0,0,2));});
  suspend();st(|s|{assert_eq!(s.setup.len(),64);assert!(!s.setup.can_send([64,121,0],1000));});
 }
 #[test]fn explicit_reset_discards_scene_pending_ack_history_and_capacity(){
  let _serial=SERIAL.lock().unwrap();fresh();st(|s|{s.setup_scene=Some(17);s.setup.sent(P,0,10,20);s.setup.payload(P,49,true);
   s.clock.sent(Action::Base(P),10,20);s.clock.cursor=8;s.clock.crystals=5;s.clock.queued=true;
   s.clock.next_place=70;s.clock.next_break=80;s.attempts.push((77,3));s.plan=Some(());s.queued_at=Some(Instant::now());});let before=st(|s|s.clock.generation);
  reset();st(|s|{assert_eq!(s.setup.len(),0);assert_eq!(s.setup_scene,None);assert!(s.clock.waiting.is_none());
   assert_ne!(s.clock.generation,before);assert!(s.setup.can_send(P,10));assert!(s.attempts.is_empty());
   assert_eq!((s.clock.cursor,s.clock.crystals),(0,0));assert_eq!((s.clock.next,s.clock.next_base,s.clock.next_place,s.clock.next_break),(0,0,0,0));
   assert!(!s.clock.queued);assert!(s.plan.is_none());assert!(s.queued_at.is_none());});
  assert_eq!(super::super::sunset_combat_control::RELEASES.load(SeqCst),1);assert_eq!(super::super::sunset::CANCELS.load(SeqCst),1);
 }
 }
 }
}
'''
    code=pre.replace('POLICY',(root/'crystal_setup_policy.rs').as_posix()).replace('STATE\n',state+'\n').replace('SUSPEND',suspend).replace('RESET',reset)
    with tempfile.TemporaryDirectory(prefix='sunset-base-lifecycle-') as temp:
        src=Path(temp)/'lifecycle.rs';exe=Path(temp)/'lifecycle.exe';src.write_text(code,encoding='utf-8')
        subprocess.run(['rustc','--edition=2024','--test',str(src),'-o',str(exe)],check=True)
        subprocess.run([str(exe),'--test-threads=1'],check=True)

if __name__=='__main__':main()
