#!/usr/bin/env python3
"""Compile and run bind state tests without Horizon, a display, or player data.

Pass --rustc PATH to use an explicit compiler; RUSTC is respected otherwise.
AI-assisted contribution, SPDX-License-Identifier: MIT.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--rustc", default=os.environ.get("RUSTC", "rustc"))
args = parser.parse_args()
source = Path(__file__).resolve().parent.parent / "mixins/solstice/bind_state.rs"
runtime = source.with_name("runtime.rs").read_text()

def section(start, end):
    """Compile the production parser itself, without its Game-dependent module."""
    return runtime[runtime.index(start):runtime.index(end, runtime.index(start))]

setting_code = section("#[derive(Clone, Debug, PartialEq)]\npub(super) enum Value", "#[derive(Clone, Debug, PartialEq)]\npub(super) struct Module")
helpers = section("pub(super) fn on_off", "// A key typed in a command")
error_test = r'''
#[test]
fn unavailable_hold_uses_production_setting_error_path() {
    for name in ["Reach", "Air Place"] {
        let mut setting = Setting {
            name: "Bind activation",
            value: Value::Pick { i: 0, of: super::bind_state::activation_modes(name) },
        };
        assert!(!setting.parse("Hold"), "{name}");
        assert_eq!(setting.stored(), "Toggle", "{name}");
        assert_eq!(setting.shown(), "Bind: Toggle (Hold unavailable)", "{name}");
        assert!(setting.parse("Toggle"), "{name}");
    }
    let mut setting = Setting {
        name: "Bind activation",
        value: Value::Pick { i: 0, of: super::bind_state::activation_modes("Zoom") },
    };
    assert_eq!(setting.shown(), "Bind: Toggle");
    assert!(setting.parse("hold"));
    assert_eq!(setting.stored(), "Hold");
    assert_eq!(setting.shown(), "Bind: Hold");
    assert_eq!(setting.name, "Bind activation");
}
'''
with tempfile.TemporaryDirectory(prefix="solstice-bind-tests-") as directory:
    root = Path(directory)
    harness = root / "tests.rs"
    harness.write_text(f'#[path = r#"{source}"#]\nmod bind_state;\n'
                       + '#[allow(dead_code)]\nmod production_setting {\n'
                       + setting_code + helpers + error_test + '\n}\n')
    executable = root / ("bind-tests.exe" if os.name == "nt" else "bind-tests")
    subprocess.run([args.rustc, "--edition=2024", "--test", str(harness), "-o", str(executable)], check=True)
    subprocess.run([str(executable)], check=True)
