#!/usr/bin/env python3
"""Direct production adapter/wire tests against fixture host boundaries; no game/network.

The runtime, evidence model, config, wire parser, and UI Settings/settings() are
the actual production source. Only Game, snapshot delivery, and editor state are
fixtures. Real horizon_protocol Reader/Writer and packet types come from the
pinned dependency rlib. No shared Cargo target is written.
"""
import os
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUSTC = os.environ.get("SUNSET_TEST_RUSTC", os.environ.get("RUSTC", "rustc"))


def item(source: str, signature: str) -> str:
    """Extract a complete production item, preserving its implementation verbatim."""
    start = source.index(signature)
    opening = source.index("{", start)
    depth = 1
    cursor = opening + 1
    while depth:
        if source[cursor] == "{":
            depth += 1
        elif source[cursor] == "}":
            depth -= 1
        cursor += 1
    return source[start:cursor]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--deps", type=Path, default=os.environ.get("SUNSET_TEST_DEPS"),
                        help="Pinned staged host's target/fast/deps (or SUNSET_TEST_DEPS)")
    args = parser.parse_args()
    if args.deps is None:
        parser.error("--deps is required; build the staged host before running adapter tests")
    libraries = list(args.deps.glob("libhorizon_protocol-*.rlib"))
    if len(libraries) != 1:
        raise SystemExit(f"Expected exactly one pinned horizon_protocol rlib in {args.deps}; found {len(libraries)}")
    ui = (ROOT / "mixins/sunset/watch_ui.rs").read_text(encoding="utf-8")
    settings = "\n\n".join(item(ui, signature) for signature in (
        "pub(super) struct Settings", "pub(super) fn settings("))
    template = (ROOT / "tools/watch_runtime_tests.rs").read_text(encoding="utf-8")
    source = template.replace("__ROOT__", ROOT.as_posix()).replace("__SETTINGS_SOURCE__", settings)
    with tempfile.TemporaryDirectory(prefix="sunset-watch-runtime-") as temporary:
        directory = Path(temporary)
        harness = directory / "watch_runtime_tests.rs"
        binary = directory / ("watch_runtime_tests.exe" if os.name == "nt" else "watch_runtime_tests")
        harness.write_text(source, encoding="utf-8")
        subprocess.run([RUSTC, "--edition=2024", "--test", str(harness),
                        "--extern", f"horizon_protocol={libraries[0]}", "-L", f"dependency={args.deps}",
                        "-o", str(binary)], check=True, cwd=ROOT)
        # Fixtures reset all runtime/UI global state per scenario. Serialize
        # tests because those globals intentionally match the production shape.
        subprocess.run([str(binary), "--test-threads=1", "--nocapture"], check=True, cwd=ROOT)


if __name__ == "__main__":
    main()
