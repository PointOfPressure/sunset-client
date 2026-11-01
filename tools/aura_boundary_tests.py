"""Actual Aura revalidation/scene-boundary regressions, no native game.

Expensive damage assessment and Game/scene notifications are narrow adapters.
Production helper/reset/control bodies plus Lease, Clock and Ledger are compiled
unchanged. Source-route audits are not counted as additional compiled tests.
"""
from pathlib import Path
import argparse,hashlib,json,subprocess,tempfile

def block(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1;cursor=opening+1
    while depth:depth+=(text[cursor]=='{')-(text[cursor]=='}');cursor+=1
    return text[start:cursor]

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',type=Path);args=parser.parse_args()
    source=Path(__file__).resolve().parents[1]/'mixins/sunset'
    anchor=(source/'anchor_aura.rs').read_text();crystal=(source/'crystal_aura.rs').read_text();control=(source/'combat_control.rs').read_text()
    staging = r'''
#![allow(dead_code)]
use std::{marker::PhantomData,sync::Mutex};
#[derive(Clone,Copy,PartialEq,Debug)]enum Step{Place,Charge,Detonate}
#[derive(Clone,Copy,PartialEq,Debug)]struct Rating(u32);
#[derive(Clone,Copy,PartialEq)]struct BlockStateId(u32);
type Pair=Option<(Option<(u32,Rating)>,Option<(u32,Rating)>)>;
struct Game<'a>{ticks:u64,seen:BlockStateId,pair:Pair,life:PhantomData<&'a ()>}
struct Module;struct Target;struct Budget;
struct State{failed:Vec<([i32;3],BlockStateId,u64)>}
static STATE:Mutex<State>=Mutex::new(State{failed:Vec::new()});
static SERIAL:Mutex<()>=Mutex::new(());
fn state<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.lock().unwrap())}
fn block(game:&Game<'_>,_:[i32;3])->BlockStateId{game.seen}
fn damage_pair(game:&mut Game<'_>,_:&Module,_:[i32;3],_:&[Target],_:&[Target],_:Step,_:&mut Budget)->Pair{game.pair}
fn game(pair:Pair)->Game<'static>{Game{ticks:10,seen:BlockStateId(5),pair,life:PhantomData}}
FIXED
RETRY
#[test]fn staged_place_survives_the_actual_deferred_revalidation_adapter(){let mut g=game(Some((None,Some((7,Rating(1))))));assert_eq!(damage_ok(&mut g,&Module,[0;3],&[],&[],Step::Place,&mut Budget),Some((7,Rating(1))));}
#[test]fn charge_and_detonation_cannot_borrow_a_future_opening(){for step in [Step::Charge,Step::Detonate]{let mut g=game(Some((None,Some((7,Rating(1))))));assert_eq!(damage_ok(&mut g,&Module,[0;3],&[],&[],step,&mut Budget),None);}}
#[test]fn current_target_retains_precedence_over_a_different_opening_target(){let mut g=game(Some((Some((8,Rating(2))),Some((7,Rating(1))))));assert_eq!(damage_ok(&mut g,&Module,[0;3],&[],&[],Step::Place,&mut Budget),Some((8,Rating(2))));}
#[test]fn failed_fresh_safety_assessment_rejects_every_action(){for step in [Step::Place,Step::Charge,Step::Detonate]{let mut g=game(None);assert_eq!(damage_ok(&mut g,&Module,[0;3],&[],&[],step,&mut Budget),None);}}
#[test]fn skipped_cell_stays_blocked_only_before_its_deadline(){let _s=SERIAL.lock().unwrap();state(|s|s.failed=vec![([0;3],BlockStateId(5),40)]);let mut g=game(None);assert!(retry_blocked(&g,[0;3]));g.ticks=40;assert!(!retry_blocked(&g,[0;3]));}
#[test]fn authoritative_state_change_allows_retry_without_a_retoggle(){let _s=SERIAL.lock().unwrap();state(|s|s.failed=vec![([0;3],BlockStateId(5),40)]);let mut g=game(None);g.seen=BlockStateId(6);assert!(!retry_blocked(&g,[0;3]));}
#[test]fn skipped_anchor_does_not_block_another_cell(){let _s=SERIAL.lock().unwrap();state(|s|s.failed=vec![([0;3],BlockStateId(5),40)]);assert!(!retry_blocked(&game(None),[1,0,0]));}

'''

    staging=staging.replace('FIXED',block(anchor,'fn damage_ok(')).replace('RETRY',block(anchor,'fn retry_blocked('))
    clock=crystal.split('// BEGIN PURE CRYSTAL STATE\n',1)[1].split('// END PURE CRYSTAL STATE',1)[0]
    controls='\n'.join(block(control,s) for s in ['pub(super) fn reset(','pub(super) fn release(','pub(super) struct Finish','impl Finish {','impl Drop for Finish {','pub(super) fn finish('])
    scene = r'''
#![allow(dead_code)]
#[path="LEASE"]mod lease;
#[path="SETUP"]mod setup;
mod game {
 pub mod sunset_explosive_aura {pub fn clear_view(_:bool){}}
 pub mod sunset {pub fn cancel_turns(_:&str){}}
 pub mod sunset_combat_control {
  use std::sync::Mutex;use crate::lease::Lease;
  #[derive(Default)]struct State{lease:Lease,session:Option<u32>,dim:Option<u32>,entity:Option<i32>,live:bool}
  static STATE:Mutex<Option<State>>=Mutex::new(None);
  fn with<R>(f:impl FnOnce(&mut State)->R)->R{let mut s=STATE.lock().unwrap();f(s.get_or_insert_with(State::default))}
  pub fn claim(){with(|s|{s.live=true;s.session=Some(1);s.dim=Some(1);s.entity=Some(7);s.lease.begin(1);assert!(s.lease.claim("Anchor Aura"));});}
  CONTROL
 }
 pub mod sunset_anchor_aura {
  use std::sync::Mutex;use super::sunset::cancel_turns;
  const NAME:&str="Anchor Aura";
  #[derive(Default)]pub struct State {pub token:u64,pub sent_count:u64,pub ack_count:u64,pub last_sent:Option<u64>,pub last_ack:Option<u64>,pub last_active_phase:&'static str,pub last_active_reason:&'static str,pub last_prediction:Option<(f64,f64,f64)>,pub phase:&'static str,pub reason:&'static str,pub pending:Option<()>}
  static STATE:Mutex<Option<State>>=Mutex::new(None);
  pub fn state<R>(f:impl FnOnce(&mut State)->R)->R{let mut s=STATE.lock().unwrap();f(s.get_or_insert_with(State::default))}
  ANCHOR_RESET
 }
 mod prediction{pub fn reset_receipts(){}}
 mod crystal {
  use super::{sunset,prediction};use std::sync::Mutex;use std::time::Instant;
  const NAME:&str="Crystal Aura";
  CLOCK
  use state::{Action,Clock};
  struct State{clock:Clock,queued_at:Option<Instant>,plan:Option<()>,lookahead:Option<()>,opening:f64,weapon:Option<usize>,attempts:Vec<(u32,u8)>,setup:crate::setup::Ledger,setup_scene:Option<u32>}
  static STATE:Mutex<Option<State>>=Mutex::new(None);
  fn st<R>(f:impl FnOnce(&mut State)->R)->R{let mut s=STATE.lock().unwrap();f(s.get_or_insert_with(||State{clock:Clock::default(),queued_at:None,plan:None,lookahead:None,opening:0.0,weapon:None,attempts:Vec::new(),setup:crate::setup::Ledger::new(),setup_scene:None}))}
  fn note(_:usize){}
  SUSPEND
  CRYSTAL_RESET
  DISCARD
  #[cfg(test)]mod tests {
   use super::*;use super::super::{sunset_combat_control as control,sunset_anchor_aura as anchor};
   static SERIAL:Mutex<()>=Mutex::new(());
   fn fixture()->control::Finish{control::reset();control::claim();st(|s|{s.weapon=Some(8);s.clock.queued=true;s.clock.sent(Action::Break(77,[0.0;3]),1,2);s.clock.queued=true;s.plan=Some(());s.lookahead=Some(());s.opening=3.0;});anchor::state(|s|{s.pending=Some(());s.token=4;s.sent_count=10;s.ack_count=9;s.last_active_phase="charge sent";});control::finish("Anchor Aura")}
   #[test]fn same_connection_same_dimension_same_id_boundary_clears_old_deeds_and_hand(){let _s=SERIAL.lock().unwrap();let old=fixture();assert!(old.current());discard_scene();assert!(!old.current());st(|s|{assert!(s.weapon.is_none());assert!(!s.clock.queued);assert!(s.clock.waiting.is_none());assert!(s.plan.is_none());assert!(s.lookahead.is_none());});anchor::state(|s|{assert!(s.pending.is_none());assert_ne!(s.token,4);});}
   #[test]fn dropped_old_deed_cannot_release_new_same_owner_lease(){let _s=SERIAL.lock().unwrap();let old=fixture();discard_scene();control::claim();let new=control::finish("Anchor Aura");assert!(new.current());drop(old);assert!(new.current());}
   #[test]fn numeric_diagnostics_survive_context_discard(){let _s=SERIAL.lock().unwrap();let old=fixture();discard_scene();anchor::state(|s|{assert_eq!(s.sent_count,10);assert_eq!(s.ack_count,9);assert_eq!(s.last_active_phase,"charge sent");assert_eq!(s.phase,"idle");});drop(old);}
  }
 }
}

'''

    scene=scene.replace('LEASE',(source/'combat_lease.rs').as_posix()).replace('SETUP',(source/'crystal_setup_policy.rs').as_posix())
    scene=scene.replace('CONTROL',controls).replace('ANCHOR_RESET',block(anchor,'pub(super) fn reset(')).replace('CLOCK',clock)
    scene=scene.replace('SUSPEND',block(crystal,'pub(super) fn suspend(')).replace('CRYSTAL_RESET',block(crystal,'pub(super) fn reset(')).replace('DISCARD',block(crystal,'pub(super) fn discard_scene('))
    prepare=block(anchor,'pub(super) fn prepare(')
    precache=prepare[prepare.index('for &cell in candidates.iter().take(64)'):prepare.index('if stage.new_anchor()')]
    assert precache.index('retry_blocked(game,cell)')<precache.index('damage_pair(')
    assert 'if retry_blocked(game,p)' in prepare
    packet=block(crystal,'pub(super) fn packet(')
    assert 'C::Login(_) | C::Respawn(_) | C::Disconnect(_)) { discard_scene(); return; }' in packet
    discard=block(crystal,'pub(super) fn discard_scene(')
    assert 'cleanup_shared' not in discard and 'Game' not in discard
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-aura-boundary-') as scratch:
        for label,code in [('staged-revalidation',staging),('scene-boundary',scene)]:
            src=Path(scratch)/(label+'.rs');exe=Path(scratch)/(label+'.exe');src.write_text(code)
            for command in [['rustc','--edition=2024','--test',str(src),'-o',str(exe)],[str(exe),'--test-threads=1']]:
                result=subprocess.run(command,capture_output=True,text=True)
                records.append({'name':label,'command':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
                print(result.stdout,end='');print(result.stderr,end='')
                if result.returncode:raise SystemExit(result.returncode)
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'source_sha256':{name:hashlib.sha256((source/name).read_bytes()).hexdigest() for name in ['anchor_aura.rs','crystal_aura.rs','combat_control.rs','combat_lease.rs','crystal_setup_policy.rs']},
          'scope':'10 extracted production revalidation/lifecycle checks. Damage pairs and scene notification/Game fields are explicit adapters, not native world or network evidence. Four source-route assertions are uncounted.'},indent=2)+'\n')

if __name__=='__main__':main()
