"""Compile actual manual-toggle lifecycle functions with instrumented effects.

Ten production-function adapter checks plus one separate source route contract.
No native Game, client, network or fixture is started.
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
    macro=(source/'combat_macros.rs').read_text(encoding='utf-8')
    inventory=(source/'combat_inventory.rs').read_text(encoding='utf-8')
    melee=(source/'melee_assist.rs').read_text(encoding='utf-8')
    macro_functions='\n'.join(function(macro,s) for s in ['fn same_session(','pub(super) fn reset(','pub(super) fn toggled('])
    inventory_function=function(inventory,'pub(super) fn toggled(')
    cancel_owner=function(melee,'pub(super) fn cancel_owner(')
    code=r'''
#![allow(dead_code)]
use std::{sync::{Arc,Mutex},time::Instant};
#[path="MACRO_POLICY"]mod macro_policy;
#[path="INVENTORY_POLICY"]mod inventory_policy;
struct Session{counters:Arc<Mutex<Option<Arc<()>>>>}
struct Remote{entity_id:i32,session:Session}
struct Game<'a>{dim:u8,remote:Option<Remote>,using:Option<()>,selected:usize,holds:Vec<usize>,
 life:std::marker::PhantomData<&'a ()>}
fn fixture()->Game<'static>{
 *sunset_combat_control::OWNER.lock().unwrap()=None;*sunset::CANCELS.lock().unwrap()=vec![];
 Game{dim:1,remote:Some(Remote{entity_id:7,session:Session{counters:Arc::new(Mutex::new(None))}}),
 using:None,selected:8,holds:vec![],life:std::marker::PhantomData}
}
fn hold(game:&mut Game<'_>,slot:usize){game.selected=slot;game.holds.push(slot);}
mod sunset {pub static CANCELS:std::sync::Mutex<Vec<String>>=std::sync::Mutex::new(vec![]);
 pub fn cancel_turns(owner:&str){CANCELS.lock().unwrap().push(owner.into());}}
mod sunset_player{pub fn gap_eating()->bool{false}}
mod sunset_combat_control {
 pub static OWNER:std::sync::Mutex<Option<&'static str>>=std::sync::Mutex::new(None);
 pub fn owns(owner:&str)->bool{*OWNER.lock().unwrap()==Some(owner)}
 pub fn release(owner:&str){let mut current=OWNER.lock().unwrap();if *current==Some(owner){*current=None;}}
}
mod sunset_melee_assist {
 use super::*;
 #[derive(Clone,Copy)]struct Lease{owner:&'static str}
 struct State{jump_owner:Option<&'static str>,lease:Option<Lease>}
 static STATE:Mutex<State>=Mutex::new(State{jump_owner:None,lease:None});
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.lock().unwrap())}
 fn release_lease(_: &mut Game<'_>,_:bool){state(|s|s.lease=None);}
 CANCEL_OWNER
 pub fn jump(owner:Option<&'static str>){state(|s|s.jump_owner=owner);}
 pub fn pending()->Option<&'static str>{state(|s|s.jump_owner)}
}
mod macros {
 use super::*;use macro_policy as policy;
 const CRYSTAL:&str="Crystal Macro";const ANCHOR:&str="Anchor Macro";
 type SessionToken=Arc<Mutex<Option<Arc<()>>>>;
 struct Settings;impl Settings{fn b(&self,_:&str)->bool{true}}
 struct Run{owner:&'static str,previous:usize,owned:usize,session:SessionToken,entity:i32,dimension:u8,settings:Settings}
 struct State{run:Option<Run>,token:u64,anchor_press:Option<u8>,anchor_latched:bool,retries:policy::Retries,
  last_deed:Option<u64>,pace_gap:u64,main:Option<()>}
 static STATE:Mutex<State>=Mutex::new(State{run:None,token:0,anchor_press:None,anchor_latched:false,
  retries:policy::Retries::new(),last_deed:None,pace_gap:0,main:None});
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.lock().unwrap())}
 FUNCTIONS
 fn setup(owner:&'static str)->Game<'static>{let game=fixture();let remote=game.remote.as_ref().unwrap();
  state(|s|{s.token=7;s.anchor_press=Some(1);s.anchor_latched=true;s.last_deed=Some(1);s.pace_gap=3;s.main=Some(());
   s.run=Some(Run{owner,previous:2,owned:8,session:remote.session.counters.clone(),entity:7,dimension:1,settings:Settings});});
  *sunset_combat_control::OWNER.lock().unwrap()=Some(owner);game
 }
 #[test]fn disable_invalidates_old_deed_generation_and_restores_only_own_hand(){let mut g=setup(CRYSTAL);
  toggled(&mut g,CRYSTAL,false);state(|s|{assert!(s.run.is_none());assert_ne!(s.token,7);});
  assert_eq!(g.holds,[2]);assert_eq!(*sunset_combat_control::OWNER.lock().unwrap(),None);assert_eq!(sunset::CANCELS.lock().unwrap().len(),2);}
 #[test]fn off_on_before_tick_cannot_replay_old_anchor_press_or_latch(){let mut g=setup(ANCHOR);
  toggled(&mut g,ANCHOR,false);toggled(&mut g,ANCHOR,true);state(|s|{assert!(s.run.is_none());assert!(s.anchor_press.is_none());assert!(!s.anchor_latched);assert_ne!(s.token,7);});}
 #[test]fn toggling_other_macro_preserves_active_owner_and_hand(){let mut g=setup(ANCHOR);
  toggled(&mut g,CRYSTAL,false);toggled(&mut g,CRYSTAL,true);state(|s|assert_eq!(s.run.as_ref().unwrap().owner,ANCHOR));
  assert!(g.holds.is_empty());assert!(sunset_combat_control::owns(ANCHOR));assert!(sunset::CANCELS.lock().unwrap().is_empty());}
 #[test]fn disable_never_restores_across_session_replacement(){let mut g=setup(CRYSTAL);g.remote.as_mut().unwrap().session.counters=Arc::new(Mutex::new(None));
  toggled(&mut g,CRYSTAL,false);assert!(g.holds.is_empty());}
 #[test]fn disable_never_reverts_manual_selection(){let mut g=setup(CRYSTAL);g.selected=4;
  toggled(&mut g,CRYSTAL,false);assert_eq!(g.selected,4);assert!(g.holds.is_empty());}
 #[test]fn explicit_reenable_resets_crystal_attempt_and_timing_context(){let mut g=setup(CRYSTAL);
  state(|s|{for tick in 100..103{s.retries.sent(7,tick);}assert!(!s.retries.allows(7,103));});
  toggled(&mut g,CRYSTAL,true);state(|s|{assert!(s.retries.allows(7,103));assert!(s.last_deed.is_none());assert_eq!(s.pace_gap,0);assert!(s.main.is_none());});}
}
mod inventory {
 use super::*;use inventory_policy as policy;use sunset_combat_control as control;
 const AUTO:&str="Auto Totem";const INV:&str="Auto Inventory Totem";const THROW:&str="Key Throw";
 mod util{pub fn toggle_off(_: &mut super::Game<'_>,_:&str){}}
 #[derive(Default)]struct State{press:Option<u8>,pearl:policy::Edge,wind:policy::Edge,queued_use:Option<(&'static str,Instant)>}
 static STATE:Mutex<Option<State>>=Mutex::new(None);
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{let mut g=STATE.lock().unwrap();f(g.get_or_insert_with(State::default))}
 // This manual-throw fixture creates no inventory Job. Retirement's actual
 // implementation is exercised by inventory_retirement_tests, not this shim.
 fn retire_matching(_: &mut Game<'_>,_:&str){}
 INVENTORY_FUNCTION
 fn setup()->Game<'static>{let g=fixture();state(|s|{*s=State::default();s.press=Some(9);s.queued_use=Some((THROW,Instant::now()));s.pearl.update(true);s.wind.update(true);});
  *control::OWNER.lock().unwrap()=Some(THROW);sunset_melee_assist::jump(Some(THROW));g}
 #[test]fn throw_disable_cancels_queued_use_and_owned_wind_jump(){let mut g=setup();toggled(&mut g,THROW,false);
  state(|s|{assert!(s.press.is_none());assert!(s.queued_use.is_none());});assert!(sunset_melee_assist::pending().is_none());assert!(!control::owns(THROW));}
 #[test]fn off_on_before_tick_cannot_replay_buffered_press(){let mut g=setup();toggled(&mut g,THROW,false);toggled(&mut g,THROW,true);
  state(|s|{assert!(s.press.is_none());assert!(s.queued_use.is_none());assert!(s.pearl.update(true));assert!(s.wind.update(true));});}
 #[test]fn unrelated_toggle_preserves_manual_intent_and_jump(){let mut g=setup();toggled(&mut g,"Hover Totem",false);
  state(|s|{assert!(s.press.is_some());assert_eq!(s.queued_use.unwrap().0,THROW);});assert_eq!(sunset_melee_assist::pending(),Some(THROW));assert!(control::owns(THROW));}
 #[test]fn throw_toggle_does_not_cancel_another_owners_use_or_jump(){let mut g=setup();state(|s|s.queued_use=Some(("FastXP",Instant::now())));
  *control::OWNER.lock().unwrap()=Some("FastXP");sunset_melee_assist::jump(Some("Kill Aura"));toggled(&mut g,THROW,false);
  state(|s|assert_eq!(s.queued_use.unwrap().0,"FastXP"));assert_eq!(sunset_melee_assist::pending(),Some("Kill Aura"));assert!(control::owns("FastXP"));}
}
'''.replace('MACRO_POLICY',(source/'macro_policy.rs').as_posix()).replace('INVENTORY_POLICY',(source/'inventory_policy.rs').as_posix())
    code=code.replace('CANCEL_OWNER',cancel_owner).replace('FUNCTIONS',macro_functions).replace('INVENTORY_FUNCTION',inventory_function)
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-manual-toggle-') as scratch:
        rust=Path(scratch)/'toggle.rs';exe=Path(scratch)/'toggle.exe';rust.write_text(code,encoding='utf-8')
        for label,command in [('compile',['rustc','--edition=2024','--test',str(rust),'-o',str(exe)]),('run',[str(exe),'--test-threads=1'])]:
            result=subprocess.run(command,capture_output=True,text=True)
            records.append({'name':label,'command':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
            print(result.stdout+result.stderr)
            if result.returncode:break
    runtime=(source/'runtime.rs').read_text(encoding='utf-8');toggle=function(runtime,'pub(super) fn toggle(')
    wired='sunset_combat_macros::toggled(game, name, on);' in toggle
    if wired:print('source-route contract: 1 passed (runtime sends both module transitions to manual macro lifecycle; not compiled)')
    else:print('Source contract failed: module transitions must reach manual macro lifecycle')
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':int(wired),
            'source_sha256':{p.relative_to(repo).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in
                             [source/name for name in ['combat_macros.rs','combat_inventory.rs','melee_assist.rs','runtime.rs']]},
            'scope':'Ten exact production function adapters, one separate source route contract. No native client/network.'},indent=2)+'\n',encoding='utf-8')
    raise SystemExit(result.returncode or int(not wired))


if __name__=='__main__':main()
