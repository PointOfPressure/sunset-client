"""Compile production Friend Guard feedback with instrumented names/action bar.

This checks the bounded feedback queue and flush; actual attack protection is
verified separately with the native player fixture.
"""
from pathlib import Path
import subprocess,tempfile,os
ROOT=Path(__file__).resolve().parents[1]
source=(ROOT/'mixins/sunset/melee_assist.rs').read_text()
def function(signature):
    start=source.index(signature);left=source.index('{',start);depth=1;end=left+1
    while depth:depth+=(source[end]=='{')-(source[end]=='}');end+=1
    return source[start:end]
helpers='\n'.join(function(s)for s in ['pub(super) fn reset_friend_feedback(','fn hint_ready(','fn queue_friend_hint(','fn flush_friend_hint('])
fixture=r'''
#![allow(dead_code)]
use std::{sync::Mutex,time::{Duration,Instant}};
const FRIEND:&str="Friend Guard";
#[derive(Default)]struct State{friend_hint_at:Option<Instant>,friend_hint:Option<u32>}
static STATE:Mutex<Option<State>>=Mutex::new(None);
static SERIAL:Mutex<()>=Mutex::new(());
static ON:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(true);
fn state<R>(f:impl FnOnce(&mut State)->R)->R{let mut s=STATE.lock().unwrap();f(s.get_or_insert_with(State::default))}
static NOTIFY:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(true);
struct Module;impl Module{fn b(&self,_:&str)->bool{NOTIFY.load(std::sync::atomic::Ordering::Relaxed)}}
fn module(_: &str)->Option<Module>{ON.load(std::sync::atomic::Ordering::Relaxed).then_some(Module)}
fn alive(g:&Game<'_>)->bool{g.live}
fn known_friend(g:&Game<'_>,id:u32,_:bool)->bool{g.friend&&id==7}
mod horizon_client{pub mod text{#[derive(Debug,PartialEq)]pub struct Text(pub String);impl Text{pub fn literal(s:impl Into<String>)->Self{Self(s.into())}}}}
#[derive(Default)]struct Titles{overlay:Option<horizon_client::text::Text>,rainbow:bool,calls:usize}
impl Titles{fn set_overlay(&mut self,t:horizon_client::text::Text,r:bool){self.overlay=Some(t);self.rainbow=r;self.calls+=1;}}
struct Game<'a>{live:bool,friend:bool,name:Option<&'a str>,titles:Titles}
impl<'a>Game<'a>{fn remote_player_name(&self,id:u32)->Option<&str>{(id==7).then_some(self.name).flatten()}}
@HELPERS@
fn game()->Game<'static>{state(|s|*s=State::default());ON.store(true,std::sync::atomic::Ordering::Relaxed);NOTIFY.store(true,std::sync::atomic::Ordering::Relaxed);Game{live:true,friend:true,name:Some("CodexTarget"),titles:Titles::default()}}
#[test]fn cooldown_has_exact_boundary_and_rejects_rewound_time(){let t=Instant::now();assert!(hint_ready(None,t));assert!(!hint_ready(Some(t),t+Duration::from_millis(1499)));assert!(hint_ready(Some(t),t+Duration::from_millis(1500)));assert!(!hint_ready(Some(t+Duration::from_millis(1)),t));}
#[test]fn named_feedback_is_deferred_and_consumed_once(){let _s=SERIAL.lock().unwrap();let mut g=game();queue_friend_hint(&g,7);assert_eq!(g.titles.calls,0);flush_friend_hint(&mut g);assert_eq!(g.titles.overlay,Some(horizon_client::text::Text("Held back your hit on CodexTarget".into())));assert!(!g.titles.rainbow);flush_friend_hint(&mut g);assert_eq!(g.titles.calls,1);}
#[test]fn repeated_contacts_do_not_spam_the_action_bar(){let _s=SERIAL.lock().unwrap();let mut g=game();queue_friend_hint(&g,7);flush_friend_hint(&mut g);for _ in 0..20{queue_friend_hint(&g,7);flush_friend_hint(&mut g);}assert_eq!(g.titles.calls,1);}
#[test]fn unknown_player_has_no_fabricated_name_or_queue(){let _s=SERIAL.lock().unwrap();let mut g=game();g.name=None;queue_friend_hint(&g,7);flush_friend_hint(&mut g);assert_eq!(g.titles.calls,0);assert!(state(|s|s.friend_hint_at.is_none()));}
#[test]fn disabling_clears_pending_feedback_and_cooldown(){let _s=SERIAL.lock().unwrap();let mut g=game();queue_friend_hint(&g,7);ON.store(false,std::sync::atomic::Ordering::Relaxed);flush_friend_hint(&mut g);assert_eq!(g.titles.calls,0);assert!(state(|s|s.friend_hint.is_none()&&s.friend_hint_at.is_none()));}
#[test]fn_friend_removal_between_guard_and_tick_drops_message(){let _s=SERIAL.lock().unwrap();let mut g=game();queue_friend_hint(&g,7);g.friend=false;flush_friend_hint(&mut g);assert_eq!(g.titles.calls,0);}
#[test]fn_lost_player_context_drops_message(){let _s=SERIAL.lock().unwrap();let mut g=game();queue_friend_hint(&g,7);g.live=false;flush_friend_hint(&mut g);assert_eq!(g.titles.calls,0);}
#[test]fn disabling_notifications_keeps_the_bar_clear(){let _s=SERIAL.lock().unwrap();let mut g=game();NOTIFY.store(false,std::sync::atomic::Ordering::Relaxed);queue_friend_hint(&g,7);flush_friend_hint(&mut g);assert_eq!(g.titles.calls,0);}
#[test]fn explicit_transition_clears_pending_feedback(){let _s=SERIAL.lock().unwrap();let mut g=game();queue_friend_hint(&g,7);reset_friend_feedback();flush_friend_hint(&mut g);assert_eq!(g.titles.calls,0);assert!(state(|s|s.friend_hint_at.is_none()));}
'''
fixture=fixture.replace('@HELPERS@',helpers.replace('pub(super) fn reset_friend_feedback','fn reset_friend_feedback')).replace('fn_friend','fn friend').replace('fn_lost','fn lost')
with tempfile.TemporaryDirectory(prefix='sunset-friend-feedback-',dir=os.environ.get('TEMP'))as temp:
    path=Path(temp)/'tests.rs';exe=Path(temp)/'tests.exe';path.write_text(fixture)
    subprocess.run(['rustc','--edition=2024','--test',str(path),'-o',str(exe)],check=True)
    subprocess.run([str(exe),'--test-threads=1'],check=True)
assert 'queue_friend_hint(game, id)'in function('pub(super) fn guard_attack(')
assert 'queue_friend_hint(game, id)'in function('pub(super) fn guard_stab_at(')
assert 'flush_friend_hint(game)'in function('pub(super) fn tick(')
print('source-route contract: 3 passed')
