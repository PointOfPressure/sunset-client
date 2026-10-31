"""Exercise the actual deferred editor/commit routines without a renderer.

The fixture retains a real production Owned lease. A stale applied value must
not become the user's chosen value, and controller cleanup must happen outside
the runtime mutex. This does not substitute for native widget tests.
"""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'mixins/sunset/runtime.rs').read_text(encoding='utf-8')


def function(name):
    start = source.index('fn ' + name + '(')
    left = source.index('{', start)
    depth = 0
    for right in range(left, len(source)):
        if source[right] == '{':
            depth += 1
        elif source[right] == '}':
            depth -= 1
            if depth == 0:
                return source[start:right + 1]
    raise AssertionError(name)


fixture = r'''
#![allow(dead_code)]
use std::sync::Mutex;
#[path = "@POLICY@"] mod policy;
#[path = "@DRAFT@"] mod text_edit;
use text_edit::Draft;
#[derive(Clone, Debug, PartialEq)]
enum Value { Text(String), Num { v:f64,min:f64,max:f64,step:f64 }, Bool(bool) }
#[derive(Clone, Debug)] struct Setting { name:&'static str,value:Value }
struct Module { name:&'static str,settings:Vec<Setting> }
struct Editor { module:usize,setting:usize,draft:Result<Draft,&'static str>,list:Option<()> }
#[derive(Default)] struct Gui { editor:Option<Editor>,pending_edits:Vec<(usize,usize,Setting)> }
#[derive(Default)] struct State { modules:Vec<Module>,gui:Gui }
static STATE: Mutex<Option<State>>=Mutex::new(None);
static SERIAL:Mutex<()>=Mutex::new(());
fn with<R>(f:impl FnOnce(&mut State)->R)->R { let mut g=STATE.lock().unwrap();f(g.get_or_insert_with(State::default)) }
struct Game<'a> { lease:Option<policy::Owned<Value>>,calls:usize,marker:std::marker::PhantomData<&'a ()> }
mod sunset_sword_bot {
    pub(super) fn editing(game:&mut super::Game<'_>,name:&str) {
        // Taking the same mutex proves the caller released it before cleanup.
        game.calls+=1;
        if name=="Kill Aura" { if let Some(lease)=game.lease.take() {
            super::with(|s|lease.restore(&mut s.modules[0].settings[0].value));
        }}
    }
}
mod sunset_crystal_bot { pub(super) fn editing(_: &mut super::Game<'_>,_:&str) {} }
fn round_to(n:f64,step:f64)->f64 { (n/step).round()*step }
mod actual {
use super::*;
@COMMIT@
@APPLY@
pub(super) fn commit(g:&mut Game<'_>){commit_edits(g)}
pub(super) fn apply(s:&mut State){apply_editor(s)}
}
fn number(v:f64)->Value{Value::Num{v,min:0.,max:8.,step:0.5}}
fn setup(before:Value,applied:Value)->Game<'static>{
    with(|s|*s=State{modules:vec![Module{name:"Kill Aura",settings:vec![Setting{name:"Range",value:applied.clone()}]}],gui:Gui::default()});
    Game{lease:Some(policy::Owned::new(before,applied)),calls:0,marker:std::marker::PhantomData}
}
#[test] fn same_value_editor_is_explicit_user_intent(){
    let _serial=SERIAL.lock().unwrap();let mut g=setup(number(6.),number(3.));
    with(|s|{s.gui.editor=Some(Editor{module:0,setting:0,draft:Draft::new("3"),list:None});actual::apply(s);assert_eq!(s.modules[0].settings[0].value,number(3.));assert_eq!(s.gui.pending_edits.len(),1)});
    actual::commit(&mut g);
    assert_eq!(g.calls,1);assert!(g.lease.is_none());with(|s|assert_eq!(s.modules[0].settings[0].value,number(3.)));
}
#[test] fn desired_toggle_precedes_restoration(){
    let _serial=SERIAL.lock().unwrap();let mut g=setup(Value::Bool(false),Value::Bool(true));
    with(|s|{let mut wanted=s.modules[0].settings[0].clone();if let Value::Bool(v)=&mut wanted.value{*v=!*v};s.gui.pending_edits.push((0,0,wanted));});
    actual::commit(&mut g);with(|s|assert_eq!(s.modules[0].settings[0].value,Value::Bool(false)));assert!(g.lease.is_none());
}
#[test] fn rejected_numeric_draft_keeps_owner_and_editor(){
    let _serial=SERIAL.lock().unwrap();let mut g=setup(number(6.),number(3.));
    with(|s|{s.gui.editor=Some(Editor{module:0,setting:0,draft:Draft::new("NaN"),list:None});actual::apply(s);assert!(s.gui.editor.is_some());assert!(s.gui.pending_edits.is_empty())});
    actual::commit(&mut g);assert_eq!(g.calls,0);assert!(g.lease.is_some());
}
#[test] fn stale_setting_index_cannot_cancel_owner(){
    let _serial=SERIAL.lock().unwrap();let mut g=setup(number(6.),number(3.));
    with(|s|{let wanted=s.modules[0].settings[0].clone();s.gui.pending_edits.push((8,0,wanted))});
    actual::commit(&mut g);assert_eq!(g.calls,0);assert!(g.lease.is_some());
}
'''
fixture = fixture.replace('@POLICY@', (ROOT/'mixins/sunset/sword_policy.rs').as_posix())
fixture = fixture.replace('@DRAFT@', (ROOT/'mixins/sunset/text_edit.rs').as_posix())
fixture = fixture.replace('@COMMIT@', function('commit_edits')).replace('@APPLY@', function('apply_editor'))
with tempfile.TemporaryDirectory(prefix='sunset-gui-intent-', dir=os.environ.get('TEMP')) as directory:
    path = Path(directory)/'tests.rs'
    path.write_text(fixture, encoding='utf-8')
    exe = path.with_suffix('.exe' if os.name == 'nt' else '')
    subprocess.run(['rustc', '--edition=2024', '--test', str(path), '-o', str(exe)], check=True)
    subprocess.run([str(exe), '--test-threads=1'], check=True)
