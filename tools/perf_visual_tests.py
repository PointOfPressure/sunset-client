#!/usr/bin/env python3
"""Test actual Sweep/ray code without booting Horizon or modifying a running game.

Usage: python tools/perf_visual_tests.py [--rustc PATH]
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rustc", default=os.environ.get("RUSTC", "rustc"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    source = (root / "mixins/sunset/util.rs").read_text(encoding="utf-8")
    # Compile the production implementations, not copies maintained by tests.
    sweep = source[source.index("const SWEEP_BUDGET:"):source.index("// ---- names and lists")]
    ray = source[source.index("pub(super) fn clear_line("):source.index("// Meteor's `isOutOfRange`")]
    distance = source[source.index("pub(super) fn dist("):source.index("pub(super) fn player_pos(")]
    render = (root / "mixins/sunset/render.rs").read_text(encoding="utf-8")
    tunnel = render[render.index("fn through("):render.index("// Walkable on two opposite sides")]
    tunnel += "\npub(super) fn side_signature(game: &Game<'_>, p: [i32; 3]) -> u8 { match side(game, p) { Side::Walkable => 0, Side::Blocked => 1, Side::Partial => 2 } }\n"
    visual = (root / "mixins/sunset/visual.rs").read_text(encoding="utf-8")
    boxes = visual[visual.index("const EDGES:"):visual.index("// How a module draws")]
    boxes += visual[visual.index("pub(super) struct Look {"):visual.index("impl Look {")]
    boxes += visual[visual.index("// Potato: one small square"):]
    with tempfile.TemporaryDirectory(prefix="sunset-perf-visual-") as directory:
        fragment = Path(directory) / "util_fragment.rs"
        fragment.write_text(sweep + distance + ray + tunnel, encoding="utf-8")
        binary = Path(directory) / ("tests.exe" if os.name == "nt" else "tests")
        env = os.environ.copy()
        env["SUNSET_PERF_UTIL_FRAGMENT"] = str(fragment)
        visual_fragment = Path(directory) / "visual_fragment.rs"
        visual_fragment.write_text(boxes, encoding="utf-8")
        env["SUNSET_PERF_VISUAL_FRAGMENT"] = str(visual_fragment)
        subprocess.run([args.rustc, "--edition=2024", "--test", "-O", str(root / "tools/perf_visual_tests.rs"), "-o", str(binary)], env=env, check=True)
        subprocess.run([str(binary), "--nocapture"], check=True)


if __name__ == "__main__":
    main()
