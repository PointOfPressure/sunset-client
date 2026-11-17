#!/usr/bin/env python3
"""Build and exercise the plugin SDK before packing a Sunset release.

Uses a fresh, isolated single-player world. No launcher instance or personal save is used.
Run with the pinned Rust toolchain, cargo and native compiler on PATH.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import zipfile


TESTS = (
    "Plugin API: .plugins and the refused names",
    "Plugin API: Hello module, settings and enable/disable",
    "Plugin API: Hello command and timers",
    "Plugin API: any module's settings and toggles",
    "Plugin API: refused sends, a refused replace, and a panicking plugin",
    "Plugin API: events (ticks, toggles, keys, chat, packets, hurt, low health, death, priorities)",
    "Plugin API: scheduler, data, stop everything, reads and friends",
    "AFK Fisher: starts Auto Fish, stops on danger",
)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs(roots):
    result = {}
    for label, root in roots.items():
        if root.is_file():
            result[label] = sha(root)
        else:
            result.update({f"{label}/{p.relative_to(root).as_posix()}": sha(p)
                           for p in sorted(root.rglob("*")) if p.is_file()})
    return result


def validate_package(path, manifest):
    with zipfile.ZipFile(path) as archive:
        if json.loads(archive.read("horizon-native.json")) != manifest:
            raise RuntimeError("packed manifest does not match the tested host")


def require_pristine(path):
    if subprocess.run(["git", "-C", str(path), "diff", "--quiet", "HEAD"], check=False).returncode:
        raise RuntimeError("the pinned Horizon checkout has tracked changes")


def retain_times(stage, previous):
    """Keep Cargo's unchanged-source timestamps after regenerating a cached stage."""
    for path in stage.rglob("*"):
        old = previous / path.relative_to(stage)
        if path.is_file() and old.is_file() and sha(path) == sha(old):
            stat = old.stat()
            os.utime(path, ns=(stat.st_atime_ns, stat.st_mtime_ns))


def require_passes(output, expected=TESTS):
    """Exit status alone cannot establish that all selected checks actually ran."""
    for name in expected:
        outcomes = re.findall(r"^(PASS|FAIL|SKIP) " + re.escape(name) + r" -", output, re.M)
        if outcomes != ["PASS"]:
            raise RuntimeError(f"{name}: expected exactly one PASS, got {outcomes}")
    if re.search(r"^FAIL ", output, re.M):
        raise RuntimeError("verifier reported a failure")


def revision(path):
    return subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()


def run(args):
    root = Path(__file__).resolve().parent.parent
    manifest = json.loads((root / "horizon-native.json").read_text())
    horizon = args.horizon.resolve()
    sdk = args.sdk.resolve()
    args.mixins = args.mixins.resolve()
    out = args.out.resolve()
    stage = args.stage_dir.resolve() if args.stage_dir else out / "stage"
    if not args.mixins.is_file():
        raise RuntimeError("--mixins must name the built horizon-mixins executable")
    if revision(horizon) != manifest["horizon"]:
        raise RuntimeError("Horizon checkout does not match horizon-native.json")
    require_pristine(horizon)
    for name in ("hello-plugin", "afk-plugin"):
        m = json.loads((sdk / "examples" / name / "horizon-native.json").read_text())
        if any(m.get(k) != manifest[k] for k in ("game", "horizon", "mixins")):
            raise RuntimeError(f"SDK {name} pins do not match Sunset")
    api = (root / "mixins/sunset/api.rs").read_text()
    version = re.search(r"const API_VERSION: u32 = (\d+);", api)
    if not version or int(version[1]) < 2:
        raise RuntimeError("Sunset plugin API 2 or later is required")
    if not args.resources.is_dir():
        raise RuntimeError("--resources must name the installed game resources directory")
    if out.exists():
        raise RuntimeError("--out must be a new directory (existing runs are preserved)")
    if out == stage or out.is_relative_to(stage):
        raise RuntimeError("--out must stay outside the cached stage")
    if any(out.is_relative_to(p.resolve()) for p in
           (horizon, root / "mixins", root / "tools/plugcheck", sdk / "examples")):
        raise RuntimeError("--out must stay outside the source directories being staged")
    if any(stage.is_relative_to(p.resolve()) for p in
           (horizon, root / "mixins", root / "tools/plugcheck", sdk / "examples")):
        raise RuntimeError("--stage-dir must stay outside the source directories")
    out.mkdir(parents=True)
    report = {"host_commit": revision(root), "horizon": manifest["horizon"],
              "mixins": manifest["mixins"], "api": int(version[1]), "stage": str(stage),
              "tests": list(TESTS), "passed": False}
    roots = {"sunset": root / "mixins", "plugcheck": root / "tools/plugcheck",
             "hello": sdk / "examples/hello-plugin", "afk": sdk / "examples/afk-plugin",
             "manifest": root / "horizon-native.json", "packer": root / "tools/pack.py",
             "verifier": root / "tools/verify.py", "builder": args.mixins}
    before = inputs(roots)
    report["source_sha256"] = before
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(args.target_dir.resolve())
    quiet = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0

    def step(name, command, cwd=root, timeout=1800):
        print(name, flush=True)
        with (out / (name + ".log")).open("w", encoding="utf-8") as log:
            result = subprocess.run([str(x) for x in command], cwd=cwd, env=env,
                                    stdout=log, stderr=subprocess.STDOUT, timeout=timeout, creationflags=quiet)
        if result.returncode:
            raise RuntimeError(f"{name} failed ({result.returncode}); see {out / (name + '.log')}")

    game = None
    game_log = None
    try:
        mods = out / "mods"
        shutil.copytree(root / "mixins", mods)
        shutil.copytree(root / "tools/plugcheck", mods / "plugcheck")
        for name, pid in (("hello-plugin", "hello"), ("afk-plugin", "afk")):
            shutil.copytree(sdk / "examples" / name / "mixins" / pid, mods / pid)
        previous = None
        if stage.exists():
            if not (stage / ".horizon-mixins-stage.json").is_file() or (stage / ".git").exists():
                raise RuntimeError("--stage-dir must be a generated horizon-mixins stage, not a source checkout")
            previous = out / "previous-stage"
            # Both absolute paths were checked against source roots above. Retain the old generation;
            # the builder cannot atomically replace an existing directory on Windows.
            stage.rename(previous)
        try:
            step("stage", [args.mixins, "apply", "--workspace", horizon, "--mods", mods, "--out", stage])
        except Exception:
            if previous is not None and not stage.exists():
                previous.rename(stage)
            raise
        if previous is not None:
            retain_times(stage, previous)
        step("build", ["cargo", "build", "--locked", "--profile", args.profile,
                       "-p", "horizon-client", "-p", "horizon-client-lib",
                       "--bin", "horizon-client", "--bin", "horizon-ctl"], cwd=stage)
        binary = args.target_dir.resolve() / args.profile
        suffix = ".exe" if os.name == "nt" else ""
        frozen = out / "bin"
        frozen.mkdir()
        for name in ("horizon-client", "horizon-ctl"):
            shutil.copy2(binary / (name + suffix), frozen / (name + suffix))
        client, ctl = (frozen / (name + suffix) for name in ("horizon-client", "horizon-ctl"))
        report["binary_sha256"] = {p.name: sha(p) for p in frozen.iterdir()}
        session = out / "session"
        home = session / "home"
        home.mkdir(parents=True)
        socket = session / "c.sock"
        env.update({"HORIZON_HOME": str(home), "HORIZON_LAUNCHER_HOME": str(session / "launcher"),
                    "XDG_DATA_HOME": str(session / "data"), "XDG_CONFIG_HOME": str(session / "config"),
                    "XDG_CACHE_HOME": str(session / "cache"), "HORIZON_NO_AUDIO": "1",
                    "HORIZON_NO_HEARTBEAT": "1", "HORIZON_NO_DISCORD": "1", "HORIZON_TELEMETRY_URL": "off",
                    "SUNSET_PRESENCE_BASE": "off", "SUNSET_VERIFY_SESSION": str(session),
                    "SUNSET_VERIFY_SOCKET": str(socket), "SUNSET_PLUGIN_API_VERSION": version[1],
                    "SUNSET_PROBE": str(session / "probe.log"),
                    "SHOTS": str(out / "shots")})
        # The gate is a single-player check, even if the caller previously ran a server matrix.
        for key in ("CTL2", "SERVER", "WORLD", "PROBE"):
            env.pop(key, None)
        (out / "shots").mkdir()
        game_log = (out / "game.log").open("w", encoding="utf-8")
        game = subprocess.Popen([str(client), "--game-dir", str(home), "--world", "SunsetPluginRelease",
                                 "--creative", "--name", "Tester", "--offscreen", "--resources", str(args.resources.resolve()),
                                 "--render-distance", "4", "--width", "960", "--height", "540", "--control-socket", str(socket)],
                                cwd=home, env=env, stdout=game_log, stderr=subprocess.STDOUT, creationflags=quiet)
        control = [str(ctl), "--socket", str(socket), "--timeout", "180"]
        deadline = time.monotonic() + 180
        while not socket.exists():
            if game.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError("test game did not create its control socket; see game.log")
            time.sleep(0.25)
        step("ready", [*control, "wait-ready", "160"], timeout=180)
        step("settle", [*control, "wait", "60"], timeout=90)
        step("verify", [sys.executable, root / "tools/verify.py", ctl, *TESTS], timeout=1200)
        require_passes((out / "verify.log").read_text(encoding="utf-8"))
        step("quit", [*control, "quit"], timeout=30)
        game.wait(timeout=30)
        if game.returncode != 0:
            raise RuntimeError(f"test game exited with {game.returncode}")
        if inputs(roots) != before:
            raise RuntimeError("plugin or Sunset source changed during validation; rerun from a stable snapshot")
        if {p.name: sha(p) for p in frozen.iterdir()} != report["binary_sha256"]:
            raise RuntimeError("test binaries changed during validation")
        step("pack", [sys.executable, root / "tools/pack.py"])
        package = root / "dist" / f"sunset-{manifest['version']}.hnmod"
        if inputs(roots) != before or revision(horizon) != manifest["horizon"]:
            raise RuntimeError("release inputs changed while packing")
        require_pristine(horizon)
        validate_package(package, manifest)
        shutil.copy2(package, out / package.name)
        validate_package(out / package.name, manifest)
        report.update({"passed": True, "package": package.name, "package_sha256": sha(out / package.name)})
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        if game is not None and game.poll() is None:
            try:
                subprocess.run([str(ctl), "--socket", str(socket), "--timeout", "15", "quit"],
                               env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               timeout=20, creationflags=quiet)
                game.wait(timeout=30)
            except (OSError, subprocess.TimeoutExpired):
                game.terminate()
                game.wait(timeout=15)
        if game is not None:
            report["game_exit"] = game.returncode
        if game_log is not None:
            game_log.close()
        (out / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Plugin release gate passed: {out / 'result.json'}", flush=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("horizon", "mixins", "sdk", "resources", "out", "target-dir"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--profile", choices=("fast", "release"), default="fast")
    p.add_argument("--stage-dir", type=Path, help="Reuse a dedicated generated source stage to retain cargo cache paths")
    try:
        run(p.parse_args())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        p.exit(1, f"Plugin release gate failed: {error}\n")


if __name__ == "__main__":
    main()
