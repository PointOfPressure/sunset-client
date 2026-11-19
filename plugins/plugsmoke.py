#!/usr/bin/env python3
# Plugin SDK smoke test, part of every release: builds Sunset (the .hmod client, client/sunset) with the SDK's example
# plugins (hello, afk from sunset-plugins) and the test plugin (plugins/plugcheck), each wrapped as an .hmod plugin
# package the way the launcher's Plugins tab builds them in (see plugins/README.md), then runs verify.py's
# "Plugin API:" checks on single player (and any other target given). Exits non-zero on a plugin that needs a newer
# plugin API, a plugin left out of the build, a compiler warning, a .plugins header with another API version, or a
# check that failed, was skipped or has no single result; so a broken SDK, pin or plugin API stops the release.
#
#   plugins/plugsmoke.py [--no-run] [target...]      targets default to: sp
#
# Environment (defaults are the test machine's; see the hmod rig notes):
#   SDK=~/dev/sunset-plugins                          the sunset-plugins checkout whose examples are built
#   LAUNCHER=/var/tmp/sunset-prebuilt/launcher-0.2.78/horizon-launcher   a stock horizon-launcher that builds .hmod
#   LAUNCHER_HOME=/var/tmp/sunset-fork/home           the HOME it builds in (its versions/ hold the pinned Nightly)
#   FORK_SDK=~/sunset-ops/tmp/fork-sdk                Sunset's fork SDK (client/scripts/fork-sdk.sh), seeded first
#   RIG=~/hzmeteor                                    the test rig: runall-hmod.sh (RUNALL=) and the servers
# The build folder is RIG/mods-plug/ (sunset, probe, plugcheck, hello, afk), the game RIG/bin-plug/, the run's logs
# RIG/plug-<time>/. --no-run stops after the build (a compile check of the SDK against this Sunset).
import json, os, pathlib, re, shutil, subprocess, sys, time, tomllib

sys.stdout.reconfigure(line_buffering=True)

HERE = pathlib.Path(__file__).resolve().parent  # plugins/
ROOT = HERE.parent / "client"                   # Sunset: sunset/ (the .hmod client), tools/
HOME = pathlib.Path.home()
SDK = pathlib.Path(os.environ.get("SDK", HOME / "dev" / "sunset-plugins"))
LAUNCHER = pathlib.Path(os.environ.get("LAUNCHER", "/var/tmp/sunset-prebuilt/launcher-0.2.78/horizon-launcher"))
LHOME = pathlib.Path(os.environ.get("LAUNCHER_HOME", "/var/tmp/sunset-fork/home"))
FORK_SDK = pathlib.Path(os.environ.get("FORK_SDK", HOME / "sunset-ops" / "tmp" / "fork-sdk"))
RIG = pathlib.Path(os.environ.get("RIG", HOME / "hzmeteor"))
INSTANCE = "sunset-plug"
args = [a for a in sys.argv[1:] if not a.startswith("--")]
targets = args or ["sp"]


def fail(msg):
    sys.exit(f"plugsmoke: {msg}")


def wrap(dest, src):
    """An .hnmod plugin folder (horizon-mixin.toml: its modules and a HEAD tick hook) as an .hmod plugin package on
    Sunset's pins: its modules beside Sunset's in the game module, its tick on the client tick."""
    mod = tomllib.loads((src / "horizon-mixin.toml").read_text())
    pid, version = mod["mod"]["id"], mod["mod"]["version"]
    ticks = [i for i in mod.get("injection", []) if i["id"] == "tick" and i["kind"] == "HEAD"]
    if len(ticks) != 1 or len(mod.get("injection", [])) != 1:
        fail(f"{src}: expected one HEAD tick injection and no other")
    names = [m["name"] for m in mod["module"]]
    tick_fn = re.search(r"crate::game::(\w+)::tick\(game\)", (src / ticks[0]["helper"]).read_text())
    if not tick_fn or tick_fn.group(1) not in names:
        fail(f"{src}: its tick helper doesn't call one of its modules' tick")
    (dest / "src" / "game").mkdir(parents=True)
    (dest / "src" / "mixins").mkdir()
    for m in mod["module"]:
        shutil.copy(src / m["source"], dest / "src" / "game" / m["source"])
    (dest / "src" / "mixins" / "tick.rs").write_text(
        f"fn tick(game: &mut crate::game::Game<'_>) {{\n    crate::game::{tick_fn.group(1)}::tick(game);\n}}\n")
    (dest / "src" / "lib.rs").write_text("#[derive(Default)]\nstruct Plugin;\n\n"
                                         "#[mod_api::horizon_mod]\nimpl mod_api::Hooks for Plugin {}\n")
    (dest / "Cargo.toml").write_text(
        f'[package]\nname = "sunset-plugin-{pid}"\nversion = "{version}"\nedition = "2024"\npublish = false\n\n'
        '[lib]\npath = "src/lib.rs"\n\n[dependencies]\n'
        'mod_api = { package = "horizon-mod-api", path = ".horizon/sdk/api/crates/horizon-mod-api" }\n')
    modules = "".join(f'\n[[native.module]]\nid = "{m["id"]}"\nfile = "{m["file"]}"\nname = "{m["name"]}"\n'
                      f'source = "src/game/{m["source"]}"\n' for m in mod["module"])
    (dest / "horizon-mod.toml").write_text(
        f'id = "{pid}"\nname = "{pid}"\nversion = "{version}"\ndescription = "{pid}, wrapped by plugsmoke.py"\n'
        f'authors = ["Sunset contributors"]\napi = "{sunset["api"]}"\nside = "client"\ncapabilities = ["registries"]\n\n'
        f'[native]\ngame = "{native["game"]}"\nhorizon = "{native["horizon"]}"\nmixins = "{native["mixins"]}"\n\n'
        f'[dependencies]\nsunset = "^{".".join(sunset["version"].split(".")[:2])}"\n{modules}\n'
        '[[native.mixin]]\nid = "tick"\n'
        "target = \"horizon_client::game::native_hooks::client_tick_post::Game<'_>::native_client_tick_post\"\n"
        'kind = "HEAD"\nhelper = "src/mixins/tick.rs"\n'
        "bindings = [{ name = \"game\", ty = \"&mut crate::game::Game<'_>\", value = \"&mut *self\" }]\n")
    needs = [int(n) for f in (dest / "src" / "game").glob("*.rs") for n in re.findall(r"api::needs\((\d+)\)", f.read_text())]
    if any(n > api for n in needs):
        fail(f"{pid} needs plugin API {max(needs)} (Sunset has {api})")


def rsync(src, dest):
    subprocess.run(["rsync", "-a", "--exclude", "/.horizon", "--exclude", "/target", f"{src}/", f"{dest}/"], check=True)


def results(lines):
    """verify.py's output as {test: [PASS/FAIL lines]} and its other problems (SKIP, KICKED, stopped early)."""
    by_name = {}
    for l in lines:
        if l.startswith(("PASS ", "FAIL ")):
            by_name.setdefault(l[5:].split(" - ")[0], []).append(l)
    other = [l for l in lines if l.startswith(("SKIP", "KICKED"))]
    if lines and not any(re.match(r"\d+/\d+ passed", l) for l in lines):
        other.append(f"verify.py stopped early: {lines[-1]}")
    return by_name, other


def judge(tests, first, rerun):
    """Which checks passed, from the run's output lines and the rerun's: a check passes with exactly one result, a PASS,
    or exactly one FAIL whose rerun gives exactly one PASS. Gives (passed, passed on the rerun, problems)."""
    got, other = results(first)
    again, other_again = results(rerun)
    ok, retried, bad = [], [], []
    for name in tests:
        lines, rerun_lines = got.get(name, []), again.get(name, [])
        if len(lines) == 1 and lines[0].startswith("PASS ") and not rerun_lines:
            ok.append(name)
        elif len(lines) == 1 and lines[0].startswith("FAIL ") and len(rerun_lines) == 1 and rerun_lines[0].startswith("PASS "):
            ok.append(name)
            retried.append(name)
        elif not lines:
            bad.append(name + " - no result")
        else:
            bad.append(next((l for l in lines + rerun_lines if l.startswith("FAIL ")), f"{name} - {len(lines)} results"))
    return ok, retried, bad + other + other_again


def read_lines(path):
    return path.read_text(errors="replace").splitlines() if path.exists() else []


if __name__ == "__main__":
    sunset = tomllib.loads((ROOT / "sunset" / "horizon-mod.toml").read_text())
    native = sunset["native"]
    api = int(re.search(r"const API_VERSION: u32 = (\d+);", (ROOT / "sunset/src/game/api.rs").read_text()).group(1))
    BUILD = RIG / "mods-plug"
    shutil.rmtree(BUILD, ignore_errors=True)
    BUILD.mkdir(parents=True)
    rsync(ROOT / "sunset", BUILD / "sunset")
    rsync(ROOT / "tools" / "probe-hmod", BUILD / "probe")
    wrap(BUILD / "plugcheck", HERE / "plugcheck")
    for pid in ("hello", "afk"):
        example = SDK / "examples" / f"{pid}-plugin"
        if not (example / "mixins" / pid).is_dir():
            fail(f"no {example}/mixins/{pid}: set SDK to a sunset-plugins checkout")
        wrap(BUILD / pid, example / "mixins" / pid)
        # The SDK's own check, as a plugin author's build.py runs it, against this checkout.
        r = subprocess.run([sys.executable, str(SDK / "tools" / "build.py"), str(example), "--check", "--sunset", str(ROOT)],
                           capture_output=True, text=True)
        if r.returncode != 0:
            fail(f"the SDK's build.py --check refused {pid}: {(r.stderr or r.stdout).strip()}")
    sdk_rev = subprocess.run(["git", "-C", str(SDK), "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    print(f"Staged Sunset {sunset['version']} (plugin API {api}, Horizon {native['horizon'][:7]}) with hello, afk "
          f"(SDK {sdk_rev or '?'}), plugcheck, probe")

    data = LHOME / ".local/share/horizon-launcher"
    env = {**os.environ, "HOME": str(LHOME), "HORIZON_SDK_BASE": str(FORK_SDK), "HORIZON_LAUNCHER_NO_SELF_UPDATE": "1"}
    seeded = data / "native" / ".fork-seeded"
    if FORK_SDK.is_dir() and (not seeded.exists() or seeded.read_text() != (FORK_SDK / "fork-commit").read_text()):
        subprocess.run([str(ROOT / "scripts" / "fork-sdk.sh"), "seed", str(LHOME)], env={**os.environ, "SDK": str(FORK_SDK)},
                       check=True, stdout=subprocess.DEVNULL)
    tag = next((v.name for v in sorted((data / "versions").iterdir()) if v.name.startswith("nightly-")
                and v.name.endswith("-" + native["horizon"][:7])), None)
    if not tag:
        fail(f"no Nightly {native['horizon'][:7]} installed in {data / 'versions'}")
    subprocess.run([str(LAUNCHER), "instance", "create", INSTANCE], env=env, capture_output=True)
    subprocess.run([str(LAUNCHER), "instance", "version", INSTANCE, tag], env=env, capture_output=True, check=True)

    log = RIG / "plugsmoke-build.log"
    print(f"Building (a few minutes); output in {log}")
    started = time.time()
    with open(log, "w") as out:
        r = subprocess.run([str(LAUNCHER), "native-build", str(BUILD), "--instance", INSTANCE, "--trust-native-code"],
                           env=env, stdout=out, stderr=subprocess.STDOUT)
    text = log.read_text(errors="replace")
    built = re.findall(r"(native-[0-9a-f]+-[0-9a-f]+) built", text)
    if r.returncode != 0 or not built:
        errors = [l for l in text.splitlines() if l.startswith("error") or "-->" in l][:8]
        fail("the build failed:\n  " + "\n  ".join(errors or text.splitlines()[-5:]) +
             f"\nFull log: {log} (and {data}/native/build.log)")
    # Every mod must be in the build, on Sunset's Horizon (the launcher can leave a mod out and still succeed).
    game = data / "versions" / built[-1]
    record = json.loads((game / "native-mods.json").read_text())
    ids = sorted(m["id"] for m in record.get("mods", []))
    if record.get("horizon") != native["horizon"] or ids != sorted(["sunset", "probe", "plugcheck", "hello", "afk"]):
        fail(f"{built[-1]} has Horizon {str(record.get('horizon'))[:7]} and mods {ids}: expected Horizon "
             f"{native['horizon'][:7]} with sunset, probe, plugcheck, hello and afk")
    # Compiler warnings: a warning: line with its --> location on the next lines. An up-to-date build compiles nothing
    # and leaves the last build's log, so only a log written by this build counts.
    cargo_log = data / "native/build.log"
    cargo = cargo_log.read_text(errors="replace").splitlines() if cargo_log.stat().st_mtime >= started else []
    warnings = [f"{l} {cargo[i + 1].strip()}" for i, l in enumerate(cargo[:-1])
                if l.startswith("warning:") and cargo[i + 1].strip().startswith("-->")]
    if warnings:
        fail(f"{len(warnings)} compiler warning(s):\n  " + "\n  ".join(warnings[:10]))
    BIN = RIG / "bin-plug"
    shutil.rmtree(BIN, ignore_errors=True)
    shutil.copytree(game, BIN, symlinks=True)  # a real copy: the client loads its libraries from beside itself
    print(f"Built {built[-1]}, {'no warnings' if cargo else 'up to date (nothing compiled)'}; game in {BIN}")
    if "--no-run" in sys.argv:
        sys.exit(0)

    verify = (ROOT / "tools" / "verify.py").read_text()
    tests = re.findall(r'@test\("(Plugin API:[^"]+)"\)', verify)
    shutil.copy(ROOT / "tools" / "verify.py", RIG / "verify-plug.py")
    run = f"plug-{time.strftime('%m%d-%H%M')}"
    print(f"Running {len(tests)} plugin checks on {', '.join(targets)} (logs in {RIG / run})")
    # RETRY=1: a check that failed runs once more on its own (the send check can miss its line after the others under
    # load); it passes only if that run passes, and is named either way.
    res = data / "versions" / tag / ".local/horizon-client" / native["game"]
    subprocess.run([str(RIG / os.environ.get("RUNALL", "runall-hmod.sh")), run, "|".join(tests), *targets], cwd=RIG,
                   env={**os.environ, "BIN": str(BIN), "RES": str(res), "VERIFY": "verify-plug.py", "RETRY": "1",
                        "SUNSET_PLUGIN_API_VERSION": str(api)}, stdout=subprocess.DEVNULL)

    bad = []
    for t in targets:
        ok, retried, problems = judge(tests, read_lines(RIG / run / f"{t}.log"), read_lines(RIG / run / f"{t}.retry.log"))
        print(f"{t}: {len(ok)}/{len(tests)} plugin checks passed"
              + (f" ({len(retried)} on the rerun: {'; '.join(retried)})" if retried else ""))
        bad += [f"{t}: {l[:200]}" for l in problems]
    if bad:
        fail("plugin checks failed:\n  " + "\n  ".join(bad))
    print("Plugin SDK smoke test passed")
