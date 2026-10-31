#!/usr/bin/env python3
"""Production watchlist logic, standalone rustc tests; no cargo/game/network.

Set SUNSET_TEST_RUSTC or RUSTC to select a toolchain; otherwise uses rustc on PATH.
"""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUSTC = os.environ.get("SUNSET_TEST_RUSTC", os.environ.get("RUSTC", "rustc"))


def main():
    with tempfile.TemporaryDirectory(prefix="sunset-watch-config-") as directory:
        binary = Path(directory) / ("watch-config-tests.exe" if os.name == "nt" else "watch-config-tests")
        subprocess.run([RUSTC, "--edition=2024", "--test", str(ROOT / "tools/watch_config_tests.rs"), "-o", str(binary)], check=True, cwd=ROOT)
        subprocess.run([str(binary), "--nocapture"], check=True, cwd=ROOT)


if __name__ == "__main__":
    main()
