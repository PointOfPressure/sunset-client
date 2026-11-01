"""Production macro configured-input and sprint adapters with actual KeyBinds.

No client, synthetic packet or RPC. Input source-route assertions are separate.
"""
from pathlib import Path
import argparse,hashlib,json,subprocess,tempfile
REPO=Path(__file__).resolve().parents[1]
SOURCE=REPO/'mixins/sunset'
def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)
def paired_winit(deps,native):
    """Read Cargo's exact transitive fingerprint; never try mismatched types."""
    fingerprint=deps.parent/'.fingerprint'
    suffix=native.stem.removeprefix('libhorizon_client_input-')
    info=json.loads((fingerprint/('horizon-client-input-'+suffix)/'lib-horizon_client_input.json').read_text())
    wanted,=[row[3]for row in info['deps']if row[1]=='winit']
    candidates=[]
    for library in deps.glob('libwinit-*.rlib'):
        suffix=library.stem.removeprefix('libwinit-')
        marker=fingerprint/('winit-'+suffix)/'lib-winit'
        if marker.exists()and int.from_bytes(bytes.fromhex(marker.read_text().strip()),'little')==wanted:candidates.append(library)
    assert len(candidates)==1,'Pinned horizon-client-input requires one exact winit Cargo fingerprint'
    return candidates[0]


def native_search(deps,extra):
    """Use Cargo's recorded native-library paths, including Windows import libs."""
    directories={Path(p).resolve()for p in extra}
    for output in (deps.parent/'build').glob('*/output'):
        for line in output.read_text(encoding='utf-8',errors='replace').splitlines():
            for prefix in ['cargo:rustc-link-search=','cargo::rustc-link-search=']:
                if line.startswith(prefix):
                    value=line[len(prefix):]
                    if '='in value:value=value.split('=',1)[1]
                    path=Path(value)
                    if path.is_dir():directories.add(path.resolve())
    return sorted(directories)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('engine',type=Path);parser.add_argument('--dependency-dir',required=True,type=Path);parser.add_argument('--native-lib-dir',type=Path,action='append',default=[]);parser.add_argument('--out',type=Path);args=parser.parse_args()
    assert(args.engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    macro=(SOURCE/'combat_macros.rs').read_text(encoding='utf-8');remote=(SOURCE/'combat_remote.rs').read_text(encoding='utf-8')
    helpers='\n'.join(function(macro,s)for s in ['pub(super) fn user_mouse(','pub(super) fn focus_lost(','fn physical_use(','fn down(','fn anchor_can_click('])
    sprint=function(remote,'pub(crate) fn sprint_synced(')
    code=r'''
#![allow(dead_code)]
extern crate horizon_client_input as horizon_client;
use winit::{event::MouseButton,keyboard::KeyCode};
use std::{sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},collections::HashSet};
struct Session{counters:Arc<Mutex<Option<()>>>}struct Remote{session:Session,entity_id:i32}
struct Settings{keys:horizon_client::keybinds::KeyBinds}struct Player{sprinting:bool}
struct Game<'a>{settings:Settings,physical_keys:HashSet<KeyCode>,physical_buttons:HashSet<MouseButton>,keys:HashSet<KeyCode>,right:bool,ticks:u64,dim:u8,remote:Option<Remote>,playing:bool,on:bool,allowed:bool,player:Option<Player>,sent:bool,life:std::marker::PhantomData<&'a ()>}
impl Game<'_>{fn native_playing(&self)->bool{self.playing}fn native_on(&self,_:&str)->bool{self.on}}
mod game{pub(super)type Game<'a>=super::Game<'a>;pub mod sunset_combat_wire{pub fn sent_sprinting(g:&super::Game<'_>)->bool{g.sent}}}
mod bridge{SPRINT}
mod sunset{pub const ID:&str="sunset";}
mod sunset_combat_control{pub static OWNER:super::Mutex<Option<&'static str>>=super::Mutex::new(None);pub fn owner()->Option<&'static str>{*OWNER.lock().unwrap()}}
mod macros{
 use super::*;
 const CRYSTAL:&str="Crystal Macro";const ANCHOR:&str="Anchor Macro";
 struct AnchorPress{tick:u64,session:Arc<Mutex<Option<()>>>,entity:i32,dimension:u8,start_allowed:bool}
 struct State{anchor_press:Option<AnchorPress>}
 static STATE:Mutex<State>=Mutex::new(State{anchor_press:None});static USER_RIGHT:AtomicBool=AtomicBool::new(false);static GUI:AtomicBool=AtomicBool::new(false);
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{f(&mut STATE.lock().unwrap())}
 fn gui_open()->bool{GUI.load(Ordering::Relaxed)}fn allowed(g:&Game<'_>)->bool{g.allowed}
 HELPERS
 fn fixture()->Game<'static>{focus_lost();GUI.store(false,Ordering::Relaxed);*sunset_combat_control::OWNER.lock().unwrap()=None;
  Game{settings:Settings{keys:Default::default()},physical_keys:HashSet::new(),physical_buttons:HashSet::new(),keys:HashSet::new(),right:false,ticks:4,dim:0,remote:Some(Remote{session:Session{counters:Arc::new(Mutex::new(None))},entity_id:7}),playing:true,on:true,allowed:true,player:Some(Player{sprinting:false}),sent:false,life:std::marker::PhantomData}}
 fn translated_key(g:&Game<'_>,key:KeyCode,pressed:bool){for target in g.settings.keys.translate_all(key){if let Err(button)=target{user_mouse(g,button,pressed);}}}
 fn translated_mouse(g:&Game<'_>,button:MouseButton,pressed:bool){for target in g.settings.keys.translate_mouse_all(button){if let Err(button)=target{user_mouse(g,button,pressed);}}}
 #[test]fn actual_default_use_mouse_physical_fallback(){let mut g=fixture();g.physical_buttons.insert(MouseButton::Right);assert!(down(&g));}
 #[test]fn actual_rebound_keyboard_physical_use(){let mut g=fixture();g.settings.keys.set("key.use","key.keyboard.r");g.physical_keys.insert(KeyCode::KeyR);assert!(down(&g));}
 #[test]fn moved_use_does_not_poll_former_right_button(){let mut g=fixture();g.settings.keys.set("key.use","key.keyboard.r");g.physical_buttons.insert(MouseButton::Right);assert!(!down(&g));}
 #[test]fn actual_rebound_extra_mouse_button_is_polled(){let mut g=fixture();g.settings.keys.set("key.use",&horizon_client::keybinds::mouse_name(MouseButton::Back));g.physical_buttons.insert(MouseButton::Back);assert!(down(&g));}
 #[test]fn unbound_use_does_not_turn_arbitrary_input_into_intent(){let mut g=fixture();g.settings.keys.set("key.use","key.keyboard.unknown");g.physical_keys.insert(KeyCode::KeyR);g.physical_buttons.insert(MouseButton::Right);assert!(!down(&g));}
 #[test]fn native_keyboard_translation_already_reaches_canonical_observer(){let mut g=fixture();g.settings.keys.set("key.use","key.keyboard.r");translated_key(&g,KeyCode::KeyR,true);assert!(down(&g));translated_key(&g,KeyCode::KeyR,false);assert!(!down(&g));state(|s|assert!(s.anchor_press.as_ref().unwrap().start_allowed));}
 #[test]fn native_rebound_mouse_translation_already_reaches_canonical_observer(){let mut g=fixture();g.settings.keys.set("key.use",&horizon_client::keybinds::mouse_name(MouseButton::Back));translated_mouse(&g,MouseButton::Back,true);assert!(down(&g));translated_mouse(&g,MouseButton::Back,false);assert!(!down(&g));}
 #[test]fn toggle_or_automation_held_right_alone_is_not_physical_use(){let mut g=fixture();g.right=true;assert!(!down(&g));}
 #[test]fn other_automation_owner_creates_only_guard_edge_without_held_start(){let g=fixture();*sunset_combat_control::OWNER.lock().unwrap()=Some("Auto Eat");user_mouse(&g,MouseButton::Right,true);assert!(!down(&g));state(|s|assert!(!s.anchor_press.as_ref().unwrap().start_allowed));}
 #[test]fn own_macro_keeps_real_use_intent(){let g=fixture();*sunset_combat_control::OWNER.lock().unwrap()=Some(ANCHOR);user_mouse(&g,MouseButton::Right,true);assert!(down(&g));state(|s|assert!(s.anchor_press.as_ref().unwrap().start_allowed));}
 #[test]fn gui_press_cannot_latch_macro_intent(){let g=fixture();GUI.store(true,Ordering::Relaxed);user_mouse(&g,MouseButton::Right,true);assert!(!down(&g));state(|s|assert!(s.anchor_press.is_none()));}
 #[test]fn focus_loss_clears_observed_hold_and_quick_edge(){let g=fixture();user_mouse(&g,MouseButton::Right,true);focus_lost();assert!(!down(&g));state(|s|assert!(s.anchor_press.is_none()));}
 #[test]fn quick_rebound_release_preserves_single_start_edge_not_crystal_hold(){let mut g=fixture();g.settings.keys.set("key.use","key.keyboard.r");translated_key(&g,KeyCode::KeyR,true);translated_key(&g,KeyCode::KeyR,false);assert!(!down(&g));state(|s|{assert!(s.anchor_press.take().unwrap().start_allowed);assert!(s.anchor_press.is_none());});}
 #[test]fn local_sprint_start_waits_for_actual_sent_state(){let mut g=fixture();g.player.as_mut().unwrap().sprinting=true;assert!(!bridge::sprint_synced(&g));g.sent=true;assert!(bridge::sprint_synced(&g));}
 #[test]fn local_sprint_stop_waits_for_actual_sent_state(){let mut g=fixture();g.sent=true;assert!(!bridge::sprint_synced(&g));g.sent=false;assert!(bridge::sprint_synced(&g));}
 #[test]fn missing_local_player_cannot_establish_strike_sync(){let mut g=fixture();g.player=None;assert!(!bridge::sprint_synced(&g));}
}
'''.replace('HELPERS',helpers).replace('SPRINT',sprint)
    native,=args.dependency_dir.glob('libhorizon_client_input-*.rlib')
    winit=paired_winit(args.dependency_dir,native)
    native_dirs=native_search(args.dependency_dir,args.native_lib_dir)
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-macro-input-') as scratch:
        rust=Path(scratch)/'input.rs';exe=Path(scratch)/'input.exe';rust.write_text(code,encoding='utf-8')
        command=['rustc','--edition=2024','--test',str(rust),'-L','dependency='+str(args.dependency_dir),'--extern','horizon_client_input='+str(native),'--extern','winit='+str(winit),'-o',str(exe)]
        for directory in native_dirs:command+=['-L','native='+str(directory)]
        for stage,command in [('compile',command),('run',[str(exe),'--test-threads=1'])]:
            result=subprocess.run(command,capture_output=True,text=True);records.append({'stage':stage,'command':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr});print(result.stdout+result.stderr)
            if result.returncode:raise SystemExit(result.returncode)
    next_action=function(macro,'fn next_action(');breaking=next_action.split('Step::CrystalBreak =>',1)[1].split('Step::AnchorPlace =>',1)[0]
    assert breaking.index('sprint_synced(game)')<breaking.index('safe_blast(')
    assert 'next_action(game, &run' in function(macro,'fn queue(')
    host=(args.engine/'crates/horizon-client/src/game.rs').read_text();ctl=(args.engine/'crates/horizon-client/src/game/control.rs').read_text()
    assert 'self.settings.keys.translate_all(key)'in host and 'self.play_mouse(button, pressed)'in host
    assert 'Some(Err(button))'in ctl and 'self.play_mouse(button, true)'in ctl
    print('source-route contract: 1 passed (native canonical translation and fresh strike gate; not compiled)')
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':1,'paired_native_winit':str(winit),
            'source_sha256':{p.relative_to(REPO).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()for p in [SOURCE/name for name in ['combat_macros.rs','combat_remote.rs']]},
            'scope':'Sixteen production observer/physical-input/sprint adapter checks, actual pinned KeyBinds/winit; one separate source route. Explicit Game/owner adapter, no client/network.'},indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
