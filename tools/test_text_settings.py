#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Original implementation assisted by OpenAI Codex (GPT-6.1 Sol).
"""Test the production draft and config roundtrip without Horizon or Cargo.

The actual runtime model, helpers, save_text and load_text are extracted verbatim;
the fixture supplies only the unrelated State fields those methods need.
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
navigation = section('fn text_edit_key(', '// The named native-key API')
draft_path = root / 'mixins/solstice/text_edit.rs'
tests_path = root / 'tests/text_settings.rs'
harness = f'''#![allow(dead_code)]
#[path = r"{draft_path}"] mod text_edit;
// Only the host key vocabulary is supplied; the navigation mapping below is
// extracted from production. No physical-code assumptions enter the fixture.
mod winit {{ pub mod keyboard {{
    pub enum NamedKey {{ ArrowLeft, ArrowRight, Home, End, Backspace, Delete, Escape }}
    pub enum Key {{ Named(NamedKey), Character(String), Unidentified }}
}} }}
mod runtime {{
use crate::text_edit::EditKey;
use crate::winit;
{model}
{helpers}
{navigation}
struct Panel {{ x: f32, y: f32, collapsed: bool, scroll: f32 }}
struct Gui {{ panels: Vec<Panel> }}
struct Waypoint {{ name: String, pos: [f64;3], dim: String }}
struct State {{ modules: Vec<Module>, friends: Vec<String>, waypoints: Vec<Waypoint>, gui: Gui }}
impl State {{ {serializer} }}
#[path = r"{tests_path}"] mod tests;
}}
'''
with tempfile.TemporaryDirectory(prefix='solstice-text-tests-') as temp:
    temp = Path(temp)
    source = temp / 'text_tests.rs'
    binary = temp / ('text_tests.exe' if os.name == 'nt' else 'text_tests')
    source.write_text(harness, encoding='utf-8')
    subprocess.run([args.rustc, '--edition=2024', '--test', str(source), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
