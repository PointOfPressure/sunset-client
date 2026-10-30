#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Generated with OpenAI Codex (GPT-6.1 Sol), for PointOfPressure.
"""Host-free tests of production profiles, model types, serializer and command.

No game, account, native home or cargo target is used. Pass --rustc to pin a compiler.
The small Game/State fixture supplies only host storage and chat; all tested logic
is extracted verbatim from runtime.rs or included directly from profiles.rs.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--rustc', default=os.environ.get('RUSTC', 'rustc'))
args = parser.parse_args()
runtime = (root / 'mixins/solstice/runtime.rs').read_text(encoding='utf-8')

def section(start, end):
    return runtime[runtime.index(start):runtime.index(end, runtime.index(start))]

model = section('#[derive(Clone, Copy, Debug, PartialEq, Eq)]', '// Every module with its defaults')
helpers = section('pub(super) fn on_off(', '// A key typed in a command')
serializer = section('    fn save_text(&self)', '    pub(super) fn module(&self')
command = section('// Settings profiles by OpenAI Codex', "pub(super) fn chat(")
profile_path = str(root / 'mixins/solstice/profiles.rs')
tests_path = str(root / 'tests/profiles.rs')
harness = f'''#![allow(dead_code)]
use std::path::PathBuf;
struct Game<'a> {{ root: PathBuf, messages: Vec<String>, marker: std::marker::PhantomData<&'a ()> }}
impl Game<'_> {{ fn native_data_dir(&self, _: &str) -> PathBuf {{ self.root.clone() }} }}
#[path = r"{profile_path}"] mod solstice_profiles;
mod solstice {{
use super::Game;
use std::sync::Mutex;
const ID: &str = "solstice";
{model}
{helpers}
struct Panel {{ x: f32, y: f32, collapsed: bool, scroll: f32 }}
struct Gui {{ panels: Vec<Panel> }}
struct Waypoint {{ name: String, pos: [f64;3], dim: String }}
pub(super) struct State {{ modules: Vec<Module>, raw: String, friends: Vec<String>, waypoints: Vec<Waypoint>, gui: Gui }}
impl State {{ {serializer} }}
static STATE: Mutex<Option<State>> = Mutex::new(None);
pub(super) fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {{ f(STATE.lock().unwrap().as_mut().unwrap()) }}
fn sync_flags(_: &State) {{}}
fn say(game: &mut Game<'_>, text: &str) {{ game.messages.push(text.to_owned()); }}
{command}
#[path = r"{tests_path}"] mod tests;
}}
'''
with tempfile.TemporaryDirectory(prefix='solstice-profiles-tests-') as temp:
    temp = Path(temp)
    source = temp / 'profiles_tests.rs'
    binary = temp / ('profiles_tests.exe' if os.name == 'nt' else 'profiles_tests')
    source.write_text(harness, encoding='utf-8')
    subprocess.run([args.rustc, '--edition=2024', '--test', str(source), '-o', str(binary)], check=True)
    subprocess.run([str(binary), '--test-threads=1', '--nocapture'], check=True)
