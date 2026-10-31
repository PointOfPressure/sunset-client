"""Actual Finish/Lease and City cancellation helpers; no game/native build."""
from pathlib import Path
import subprocess,tempfile
SRC=Path(__file__).resolve().parents[1]/'mixins/sunset'

def body(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1
    for i in range(opening+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[start:i+1]
    raise AssertionError(signature)
city=(SRC/'city_trap.rs').read_text(encoding="utf-8");control=(SRC/'combat_control.rs').read_text(encoding="utf-8")
finish=control[control.index('#[must_use]'):]
helpers='\n'.join(body(city,s) for s in ['fn state<R>','fn restore_tool(','fn city_current(','fn city_generation_current(','fn user_mouse(','fn pause_city(','fn stop_city('])
pre=r'''
#![allow(dead_code)]
use std::{sync::Mutex,time::Instant,marker::PhantomData};
#[path="LEASE"]mod lease;
mod control{
    use super::*;
    struct State{lease:lease::Lease,live:bool}
    static CONTROL:Mutex<Option<State>>=Mutex::new(None);
    fn with<R>(f:impl FnOnce(&mut State)->R)->R{let mut state=CONTROL.lock().unwrap();f(state.get_or_insert_with(||State{lease:lease::Lease::default(),live:true}))}
    pub fn claim(owner:&'static str)->bool{with(|s|s.live&&s.lease.claim(owner))}
    pub fn release(owner:&str){with(|s|s.lease.release(owner));}
    pub fn clear(){with(|s|s.lease.clear());}
    pub fn tick(t:u64){with(|s|s.lease.begin(t));}
'''
middle=r'''
}
const CITY:&str="Auto City";
struct Game<'a>{selected:usize,stops:Vec<[i32;3]>,holds:Vec<usize>,disabled:bool,connected:bool,marker:PhantomData<&'a ()>}
impl Game<'_>{fn native_window(&self)->bool{true}fn native_on(&self,_:&str)->bool{true}fn native_playing(&self)->bool{true}}
mod winit{pub mod event{#[derive(Clone,Copy)]pub enum MouseButton{Left,Right,Middle}}}
fn same(game:&Game<'_>,_:&())->bool{game.connected}
#[derive(Clone)]struct Mining{restore:Option<usize>,tool:usize,block:[i32;3]}
#[derive(Default)]struct State{mine:Option<Mining>,city_finish:Option<control::Finish>,generation:u64,city_queued:Option<Instant>,reason:&'static str,session:Option<()>,manual_cancel:bool}
static STATE:Mutex<Option<State>>=Mutex::new(None);
mod sunset{pub const ID:&str="sunset";pub fn gui_open()->bool{false}pub fn cancel_turns(_:&str){}pub fn say(_:&mut super::Game<'_>,_:&str){}}
mod remote{pub mod sunset_city_mining{pub fn stop(g:&mut crate::Game<'_>,p:[i32;3]){g.stops.push(p);}}}
mod util{pub fn toggle_off(g:&mut crate::Game<'_>,_:&str){g.disabled=true;}}
fn hold(g:&mut Game<'_>,slot:usize){g.selected=slot;g.holds.push(slot);}
mod actual{use super::*;
'''
tests=r'''
    pub fn setup()->Game<'static>{
        state(|s|*s=State::default());control::clear();control::tick(0);assert!(control::claim(CITY));
        state(|s|{s.session=Some(());s.generation=1;s.city_finish=Some(control::finish(CITY));s.mine=Some(Mining{restore:Some(8),tool:0,block:[3,121,0]});});
        Game{selected:0,stops:vec![],holds:vec![],disabled:false,connected:true,marker:PhantomData}
    }
    #[test]fn owned_stop_aborts_and_restores_exactly_once(){let mut g=setup();stop_city(&mut g,"test",false);
        assert_eq!(g.stops,[[3,121,0]]);assert_eq!(g.holds,[8]);stop_city(&mut g,"test",false);assert_eq!(g.holds,[8]);}
    #[test]fn expired_same_name_ticket_cannot_restore_abort_or_release_new_lease(){let mut g=setup();
        control::clear();assert!(control::claim(CITY));let fresh=control::finish(CITY);
        stop_city(&mut g,"stale",false);assert!(g.stops.is_empty());assert!(g.holds.is_empty());assert!(fresh.current());}
    #[test]fn persistent_guard_survives_renewed_progress_and_ack_wait_ticks(){let mut g=setup();
        for tick in 1..=60{control::tick(tick);assert!(city_current());assert!(control::claim(CITY));assert!(city_generation_current(1));}
        assert!(g.stops.is_empty());stop_city(&mut g,"confirmed",false);assert_eq!(g.holds,[8]);}
    #[test]fn new_generation_rejects_old_callback_and_pause_keeps_new_restore_clean(){let mut g=setup();
        pause_city(&mut g);assert!(!city_generation_current(1));assert!(state(|s|s.mine.as_ref().unwrap().restore.is_none()));
        assert!(control::claim(CITY));state(|s|{s.generation+=1;s.city_finish=Some(control::finish(CITY));});
        assert!(!city_generation_current(1));assert!(city_current());}
    #[test]fn captured_trap_guard_cannot_be_recreated_for_a_new_same_name_lease(){let _g=setup();
        control::clear();assert!(control::claim("Auto Trap"));let queued=control::finish("Auto Trap");
        control::clear();assert!(control::claim("Auto Trap"));let fresh=control::finish("Auto Trap");
        assert!(!queued.current());drop(queued);assert!(fresh.current());}
    #[test]fn quick_down_up_between_ticks_invalidates_queue_and_retains_owned_cleanup(){let mut g=setup();
        user_mouse(&g,winit::event::MouseButton::Left,true);user_mouse(&g,winit::event::MouseButton::Left,false);
        assert!(state(|s|s.manual_cancel));assert!(!city_generation_current(1));assert!(city_current());
        stop_city(&mut g,"manual",true);assert_eq!(g.stops,[[3,121,0]]);assert_eq!(g.holds,[8]);assert!(g.disabled);assert!(!state(|s|s.manual_cancel));}
    #[test]fn release_only_middle_idle_and_stale_scene_do_not_create_manual_cancel(){let mut g=setup();
        user_mouse(&g,winit::event::MouseButton::Left,false);user_mouse(&g,winit::event::MouseButton::Middle,true);assert!(!state(|s|s.manual_cancel));
        g.connected=false;user_mouse(&g,winit::event::MouseButton::Right,true);assert!(!state(|s|s.manual_cancel));assert!(city_generation_current(1));
        g.connected=true;state(|s|s.mine=None);user_mouse(&g,winit::event::MouseButton::Left,true);assert!(!state(|s|s.manual_cancel));}
}
'''
with tempfile.TemporaryDirectory(prefix='sunset-city-owner-') as tmp:
    source=Path(tmp)/'test.rs';binary=Path(tmp)/'test.exe'
    source.write_text(pre.replace('LEASE',(SRC/'combat_lease.rs').as_posix())+finish+middle+helpers+tests)
    subprocess.run(['rustc','--edition=2024','--test',str(source),'-o',str(binary)],check=True)
    subprocess.run([str(binary),'--test-threads=1'],check=True)
tick=body(city,'fn city_tick(')
callback=tick[tick.index('move |game|'):]
assert callback.index('city_generation_current(generation)')<callback.index('s.city_queued = None')
assert callback.index('Budget::new(2)')<callback.index('city_score(')<callback.index('sunset_city_mining::step(')
assert 'stop_city(game, "Current health or equipment safety changed", true)' in callback
assert tick.index('control::claim(CITY)')<tick.index('if let Some(waiting)')
trap=body(city,'fn trap_tick(');callback=trap[trap.index('move |game|'):]
assert trap.index('let finish = control::finish(TRAP)')<trap.index('move |game|')
assert callback.index('_finish.current()')<callback.index('s.trap_queued = None')
assert 'control::finish(TRAP)' not in callback
assert 'control::release(CITY)' not in city and 'control::owns(CITY)' not in city
assert 'state(|s| s.manual_cancel)' in tick
print('Actual Finish/Lease + City helper tests7; queue-order/fresh-safety source contracts: PASS')
