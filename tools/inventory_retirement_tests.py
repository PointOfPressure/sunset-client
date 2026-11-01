"""Compile actual inventory retirement with production Swap and Lease.

Eleven effect-adapter checks and one separate wiring contract. No Game/client,
world, network exchange or actual server ACK is created by this harness.
"""
from pathlib import Path
import argparse,hashlib,json,subprocess,tempfile


def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',type=Path);args=parser.parse_args()
    repo=Path(__file__).resolve().parents[1];source=repo/'mixins/sunset'
    text=(source/'combat_inventory.rs').read_text(encoding='utf-8')
    helpers='\n'.join(function(text,s) for s in ['fn retire_job(','fn poll_retiring(','fn retire_matching('])
    code=r'''
#![allow(dead_code)]
use std::{sync::{Mutex,atomic::{AtomicU64,Ordering}},time::{Duration,Instant}};
#[path="POLICY"]mod policy;
#[path="LEASE"]mod lease;
use policy::{Status,Swap,Failure};
struct Game<'a>{scene:u8,ticks:u64,selected:usize,using:Option<()>,holds:Vec<usize>,closes:usize,life:std::marker::PhantomData<&'a ()>}
fn hold(game:&mut Game<'_>,slot:usize){game.selected=slot;game.holds.push(slot);}
mod sunset_player {pub fn gap_eating()->bool{false}}
mod remote {pub mod sunset_combat_remote{pub fn latency(_: &crate::Game<'_>)->Option<i32>{Some(0)}}}
mod control {
 use super::*;
 static LEDGER:Mutex<Option<lease::Lease>>=Mutex::new(None);
 fn access<R>(f:impl FnOnce(&mut lease::Lease)->R)->R{let mut g=LEDGER.lock().unwrap();f(g.get_or_insert_with(lease::Lease::default))}
 pub struct Finish{owner:&'static str,ticket:u64}
 impl Finish{pub fn current(&self)->bool{access(|g|g.ticket(self.owner)==Some(self.ticket))}}
 impl Drop for Finish{fn drop(&mut self){access(|g|g.finish(self.owner,self.ticket));}}
 pub fn claim(owner:&'static str)->bool{access(|g|g.claim(owner))}
 pub fn original(owner:&'static str)->Finish{assert!(claim(owner));Finish{owner,ticket:ticket(owner).unwrap()}}
 pub fn ticket(owner:&str)->Option<u64>{access(|g|g.ticket(owner))}
 pub fn owner()->Option<&'static str>{access(|g|g.owner)}
 pub fn advance(tick:u64){access(|g|g.begin(tick));}
 pub fn replace(owner:&'static str){access(|g|g.clear());assert!(claim(owner));}
 pub fn reset(){access(|g|{*g=lease::Lease::default();g.begin(100);});}
}
mod inventory {
 use super::*;
 const OWNER:&str="Auto Inventory Totem";
 #[derive(Clone,PartialEq,Eq)]struct Stack(&'static str);
 struct Module{name:&'static str}
 #[derive(Clone,Copy,PartialEq,Eq)]enum Phase{Reaction,SwapDelay,Await,Close,Return}
 struct Job{lease:control::Finish,owner:&'static str,settings:Module,restore:Option<(usize,usize)>,opened:bool,
  phase:Phase,swap:Option<Swap<Stack>>,slots:[usize;2],before:[Option<Stack>;2],began:Instant}
 #[derive(Default)]struct State{connection:Option<u8>,tick:u64,job:Option<Job>,retiring:Option<Job>,
  failures:Vec<(&'static str,Failure<Stack>)>,next:u64}
 static STATE:Mutex<Option<State>>=Mutex::new(None);
 static SWAP_CONFIRMED:AtomicU64=AtomicU64::new(0);static SWAP_FAILED:AtomicU64=AtomicU64::new(0);
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{let mut g=STATE.lock().unwrap();f(g.get_or_insert_with(State::default))}
 fn same_session(game:&Game<'_>,old:&u8)->bool{game.scene==*old}
 HELPERS
 fn setup(phase:Phase)->Game<'static>{
  // Drop prior old-ticket storage before beginning a fresh test ledger.
  state(|s|*s=State::default());control::reset();SWAP_CONFIRMED.store(0,Ordering::Relaxed);SWAP_FAILED.store(0,Ordering::Relaxed);
  let before=[Some(Stack("totem")),None];let swap=(phase==Phase::Await).then(||Swap::new([9,504],before.clone(),100).unwrap());
  let job=Job{lease:control::original(OWNER),owner:OWNER,settings:Module{name:OWNER},restore:Some((2,8)),opened:true,
   phase,swap,slots:[9,504],before,began:Instant::now()};
  state(|s|{s.connection=Some(1);s.tick=100;s.job=Some(job);});
  Game{scene:1,ticks:100,selected:8,using:None,holds:vec![],closes:0,life:std::marker::PhantomData}
 }
 fn observe(slot:usize,item:Option<Stack>){state(|s|s.retiring.as_mut().unwrap().swap.as_mut().unwrap().observe(slot,&item));}
 fn both(){observe(9,None);observe(504,Some(Stack("totem")));}
 #[test]fn unsent_transition_cancels_immediately_and_returns_only_owned_slot(){let mut game=setup(Phase::SwapDelay);
  retire_matching(&mut game,OWNER);state(|s|{assert!(s.job.is_none());assert!(s.retiring.is_none());});
  assert_eq!(game.holds,[2]);assert_eq!(game.closes,0);assert_eq!(control::owner(),None);}
 #[test]fn sent_transition_retains_both_slot_ack_and_exact_reservation(){let mut game=setup(Phase::Await);
  retire_matching(&mut game,OWNER);let ticket=control::ticket(OWNER);assert!(ticket.is_some());
  observe(9,None);assert!(poll_retiring(&game));state(|s|assert!(s.retiring.is_some()));assert_eq!(control::ticket(OWNER),ticket);
  observe(504,Some(Stack("totem")));assert!(poll_retiring(&game));state(|s|assert!(s.retiring.is_none()));
  assert_eq!(SWAP_CONFIRMED.load(Ordering::Relaxed),1);assert_eq!(control::owner(),None);}
 #[test]fn off_on_cannot_revive_followup_refill_or_delayed_close(){let mut game=setup(Phase::Await);
  retire_matching(&mut game,OWNER);retire_matching(&mut game,OWNER);game.selected=4;
  both();assert!(poll_retiring(&game));assert_eq!(game.selected,4);assert_eq!(game.closes,0);
  state(|s|{assert!(s.job.is_none());assert!(s.retiring.is_none());});assert_eq!(game.holds,[2]);}
 #[test]fn unrelated_module_transition_preserves_current_job(){let mut game=setup(Phase::Await);
  retire_matching(&mut game,"Hover Totem");state(|s|{assert!(s.job.is_some());assert!(s.retiring.is_none());});assert!(game.holds.is_empty());}
 #[test]fn original_guard_never_steals_or_releases_replacement_owner(){let mut game=setup(Phase::Await);
  control::replace("Kill Aura");let ticket=control::ticket("Kill Aura");retire_matching(&mut game,OWNER);
  assert!(game.holds.is_empty());poll_retiring(&game);assert_eq!(control::ticket("Kill Aura"),ticket);
  both();poll_retiring(&game);assert_eq!(control::ticket("Kill Aura"),ticket);}
 #[test]fn original_guard_never_releases_new_ticket_of_same_owner(){let mut game=setup(Phase::Await);
  control::replace(OWNER);let ticket=control::ticket(OWNER);retire_matching(&mut game,OWNER);both();poll_retiring(&game);
  assert!(game.holds.is_empty());assert_eq!(control::ticket(OWNER),ticket);}
 #[test]fn invalid_scene_cannot_return_old_hand_or_replay_close(){let mut game=setup(Phase::Await);game.scene=2;
  retire_matching(&mut game,OWNER);assert!(game.holds.is_empty());assert_eq!(game.closes,0);state(|s|assert!(s.retiring.is_none()));}
 #[test]fn manual_slot_override_is_preserved_at_cancellation(){let mut game=setup(Phase::Await);game.selected=5;
  retire_matching(&mut game,OWNER);assert_eq!(game.selected,5);assert!(game.holds.is_empty());}
 #[test]fn deadline_failure_blocks_unchanged_old_operation_without_followup(){let mut game=setup(Phase::Await);retire_matching(&mut game,OWNER);
  for tick in 101..115{control::advance(tick);game.ticks=tick;poll_retiring(&game);}
  control::advance(115);game.ticks=115;poll_retiring(&game);
  assert_eq!(SWAP_FAILED.load(Ordering::Relaxed),1);assert_eq!(control::owner(),None);
  state(|s|{assert!(s.retiring.is_none());assert!(s.failures[0].1.blocks([9,504],&[Some(Stack("totem")),None]));});}
 #[test]fn rejected_authoritative_value_is_not_a_confirmed_swap(){let mut game=setup(Phase::Await);retire_matching(&mut game,OWNER);
  observe(9,Some(Stack("other")));observe(504,Some(Stack("totem")));poll_retiring(&game);
  assert_eq!(SWAP_CONFIRMED.load(Ordering::Relaxed),0);assert_eq!(SWAP_FAILED.load(Ordering::Relaxed),1);}
 #[test]fn module_name_matches_dynamic_bot_owner_without_cancelling_other_job(){let mut game=setup(Phase::Await);
  state(|s|s.job.as_mut().unwrap().settings.name="Crystal PvP Bot");retire_matching(&mut game,"Crystal PvP Bot");
  state(|s|assert!(s.retiring.is_some()));}
}
'''.replace('POLICY',(source/'inventory_policy.rs').as_posix()).replace('LEASE',(source/'combat_lease.rs').as_posix()).replace('HELPERS',helpers)
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-inventory-retire-') as scratch:
        rust=Path(scratch)/'retire.rs';exe=Path(scratch)/'retire.exe';rust.write_text(code,encoding='utf-8')
        for stage,command in [('compile',['rustc','--edition=2024','--test',str(rust),'-o',str(exe)]),('run',[str(exe),'--test-threads=1'])]:
            result=subprocess.run(command,capture_output=True,text=True)
            records.append({'stage':stage,'command':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
            print(result.stdout+result.stderr)
            if result.returncode:raise SystemExit(result.returncode)
    packet=function(text,'pub(super) fn packet(');tick=function(text,'pub(super) fn tick(');responsive=function(text,'pub(super) fn responsive(')
    assert 's.retiring.as_mut()' in packet
    assert tick.index('poll_retiring(game)')<tick.index('!alive(game)')
    assert responsive.index('poll_retiring(game)')<responsive.index('mods.iter().find')
    assert 's.retiring.is_some()' in function(text,'pub(super) fn busy_for(')
    assert 's.retiring.is_some()' in function(text,'fn schedule(')
    assert 'lease: control::Finish' in text and 'let lease=control::finish(owner)' in text
    for forbidden in ['hold(', 'close_container(', 'find_totem(', 'schedule(']:assert forbidden not in function(text,'fn poll_retiring(')
    print('source-route contract: 1 passed (retirement observer, tick/frame, submission exclusion and no late deed; not compiled)')
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':1,
            'source_sha256':{p.relative_to(repo).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in
                             [source/name for name in ['combat_inventory.rs','inventory_policy.rs','combat_lease.rs']]},
            'scope':'Eleven production retirement function adapters with real Swap/Lease, one separate source contract; no Game/network.'},indent=2)+'\n',encoding='utf-8')


if __name__=='__main__':main()
