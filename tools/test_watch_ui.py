#!/usr/bin/env python3
"""Run actual watchlist UI/config and profile-load/save integration without a game.

The fixture supplies only the host Game/module surfaces. Production watchlist
sources are copied unchanged; runtime profile validation, load and save bodies
are extracted on each run so deleting their safety hooks breaks the regression.
Each scenario runs in a separate process because production uses a singleton.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "mixins/sunset"
RUSTC = os.environ.get("SUNSET_TEST_RUSTC", os.environ.get("RUSTC", "rustc"))


def between(source, start, end):
    return source[source.index(start):source.index(end, source.index(start))].rstrip()


def main():
    runtime = (SOURCE / "runtime.rs").read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="sunset-watch-ui-") as directory:
        stage = Path(directory)
        for name in ["watch_config.rs", "watch_ui.rs", "profiles.rs"]:
            shutil.copyfile(SOURCE / name, stage / name)
        shutil.copyfile(ROOT / "tools/watch_ui_tests.rs", stage / "tests.rs")
        methods = "\n".join([
            between(runtime, "    fn set_field(", "    // The modules' states, settings and keys"),
            between(runtime, "    fn load_profile(", "    // A profile file read whole"),
            between(runtime, "    fn check_profile<", "    pub(super) fn module("),
        ])
        (stage / "runtime_methods.rs").write_text("impl State {\n" + methods + "\n}\n", encoding="utf-8")
        integration = "\n".join([
            between(runtime, "pub(super) fn save(", "// ---------------------------------------------------------------- helpers"),
            between(runtime, "fn profile_command(", "pub(super) fn run("),
        ])
        (stage / "runtime_integration.rs").write_text(integration, encoding="utf-8")
        binary = stage / ("watch-ui-tests.exe" if os.name == "nt" else "watch-ui-tests")
        subprocess.run([RUSTC, "--edition=2024", "--test", str(stage / "tests.rs"), "-o", str(binary)], check=True, cwd=ROOT)
        for scenario in [
            "actual_ui_controller_preserves_watchlists_and_reverts_failed_edits",
            "unreadable_startup_file_blocks_edits_and_network_settings",
        ]:
            subprocess.run([str(binary), "--exact", scenario, "--nocapture"], check=True, cwd=ROOT)


if __name__ == "__main__":
    main()
