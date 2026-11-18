#!/usr/bin/env python3
# Plugin SDK smoke test, part of every release: builds Sunset with the SDK's example plugins (hello, afk from
# sunset-plugins) and the test plugin (plugins/plugcheck), then runs verify.py's "Plugin API:" checks on single player
# and Horizon's server. Exits non-zero on any failure, so a broken SDK, pin or plugin API stops the release.
#
#   plugins/plugsmoke.py [--no-run] [target...]      targets default to: sp horizon
#
# Environment (defaults are the test machine's):
#   SDK=~/dev/sunset-plugins             the sunset-plugins checkout whose examples are built
#   LAUNCHER_HOME=~/sunset-hp-build      holds horizon-launcher and the HOME it builds in (home/)
#   RIG=~/hzmeteor                       the test rig: runall.sh, servers.sh and the servers
# The build folder is RIG/mods-plug/ (sunset, probe, plugcheck, hello, afk), the game RIG/bin-plug/, the run's logs
# RIG/plug-<time>/. --no-run stops after the build (a compile check of the SDK against this Sunset).
import json, os, pathlib, re, shutil, subprocess, sys, time

sys.stdout.reconfigure(line_buffering=True)

HERE = pathlib.Path(__file__).resolve().parent  # plugins/
ROOT = HERE.parent / "client"                   # Sunset: horizon-native.json, mixins/, tools/
HOME = pathlib.Path.home()
SDK = pathlib.Path(os.environ.get("SDK", HOME / "dev" / "sunset-plugins"))
LHOME = pathlib.Path(os.environ.get("LAUNCHER_HOME", HOME / "sunset-hp-build"))
RIG = pathlib.Path(os.environ.get("RIG", HOME / "hzmeteor"))
args = [a for a in sys.argv[1:] if not a.startswith("--")]
targets = args or ["sp", "horizon"]


def fail(msg):
    sys.exit(f"plugsmoke: {msg}")


pins = json.loads((ROOT / "horizon-native.json").read_text())
api = int(re.search(r"const API_VERSION: u32 = (\d+);", (ROOT / "mixins/sunset/api.rs").read_text()).group(1))


def stage(dest, pid, src, manifest=None):
    """Copies a mod's .rs/.toml files to dest/pid/mixins/pid/ beside its horizon-native.json."""
    d = dest / pid / "mixins" / pid
    d.mkdir(parents=True)
    for f in sorted(src.iterdir()):
        if f.suffix in (".rs", ".toml"):
            shutil.copy(f, d / f.name)
    m = manifest or {"schema": 1, "id": pid, "version": "0.1.0", "game": pins["game"],
                     "horizon": pins["horizon"], "mixins": pins["mixins"]}
    (dest / pid / "horizon-native.json").write_text(json.dumps(m, indent=2) + "\n")


def check(pid):
    """A staged plugin must pin Sunset's Horizon and mixins and need no newer plugin API than Sunset's."""
    base = BUILD / pid
    m = json.loads((base / "horizon-native.json").read_text())
    toml = (base / "mixins" / pid / "horizon-mixin.toml").read_text()
    rev = re.search(r'horizon_revision\s*=\s*"([0-9a-f]+)"', toml)
    problems = []
    for key in ("horizon", "mixins"):
        if m.get(key) != pins[key]:
            problems.append(f"horizon-native.json {key} {m.get(key)} (Sunset: {pins[key]})")
    if not rev or rev.group(1) != pins["horizon"]:
        problems.append(f"horizon_revision {rev.group(1) if rev else 'missing'} (Sunset: {pins['horizon']})")
    needs = [int(n) for f in (base / "mixins" / pid).glob("*.rs") for n in re.findall(r"api::needs\((\d+)\)", f.read_text())]
    if any(n > api for n in needs):
        problems.append(f"needs plugin API {max(needs)} (Sunset has {api})")
    if problems:
        fail(f"{pid} doesn't match this Sunset: " + "; ".join(problems))


BUILD = RIG / "mods-plug"
shutil.rmtree(BUILD, ignore_errors=True)
BUILD.mkdir(parents=True)
stage(BUILD, "sunset", ROOT / "mixins" / "sunset", pins)
stage(BUILD, "probe", ROOT / "tools" / "probe")
stage(BUILD, "plugcheck", HERE / "plugcheck")
for pid in ("hello", "afk"):
    example = SDK / "examples" / f"{pid}-plugin"
    if not (example / "mixins" / pid).is_dir():
        fail(f"no {example}/mixins/{pid}: set SDK to a sunset-plugins checkout")
    stage(BUILD, pid, example / "mixins" / pid, json.loads((example / "horizon-native.json").read_text()))
for pid in ("plugcheck", "hello", "afk"):
    check(pid)
    # The SDK's own check, as a plugin author's build.py runs it, against this checkout.
    if pid != "plugcheck":
        r = subprocess.run([sys.executable, str(SDK / "tools" / "build.py"), str(SDK / "examples" / f"{pid}-plugin"),
                            "--check", "--sunset", str(ROOT)], capture_output=True, text=True)
        if r.returncode != 0:
            fail(f"the SDK's build.py --check refused {pid}: {(r.stderr or r.stdout).strip()}")
sdk_rev = subprocess.run(["git", "-C", str(SDK), "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
print(f"Staged Sunset (plugin API {api}, Horizon {pins['horizon'][:7]}) with hello, afk (SDK {sdk_rev or '?'}), plugcheck, probe")

log = LHOME / "plugsmoke-build.log"
data = LHOME / "home/.local/share/horizon-launcher"
print(f"Building (a few minutes); output in {log}")
started = time.time()
with open(log, "w") as out:
    r = subprocess.run([str(LHOME / "horizon-launcher"), "native-build", str(BUILD), "--trust-native-code"],
                       cwd=LHOME, env={**os.environ, "HOME": str(LHOME / "home")}, stdout=out, stderr=subprocess.STDOUT)
text = log.read_text(errors="replace")
built = re.search(r"(native-\S+) built", text)
if r.returncode != 0 or not built:
    errors = [l for l in text.splitlines() if l.startswith("error") or "-->" in l][:8]
    fail("the build failed:\n  " + "\n  ".join(errors or text.splitlines()[-5:]) +
         f"\nFull log: {log} (and {data}/native/build.log)")
# Every mod must be in the build, on Sunset's Horizon. (The launcher may first try the Horizon the instance plays,
# leave the mods out there, and then build them all on the one they pin: its log has both.)
game = data / "versions" / built.group(1)
record = json.loads((game / "native-mods.json").read_text())
ids = sorted(m["id"] for m in record.get("mods", []))
if record.get("horizon") != pins["horizon"] or ids != sorted(["sunset", "probe", "plugcheck", "hello", "afk"]):
    fail(f"{built.group(1)} has Horizon {str(record.get('horizon'))[:7]} and mods {ids}: expected Horizon "
         f"{pins['horizon'][:7]} with sunset, probe, plugcheck, hello and afk")
# Compiler warnings: a warning: line with its --> location on the next line (the launcher's own notes have none).
# An up-to-date build compiles nothing and leaves the last build's log, so only a log written by this build counts.
cargo_log = data / "native/build.log"
cargo = cargo_log.read_text(errors="replace").splitlines() if cargo_log.stat().st_mtime >= started else []
warnings = [f"{l} {cargo[i + 1].strip()}" for i, l in enumerate(cargo[:-1])
            if l.startswith("warning:") and cargo[i + 1].strip().startswith("-->")]
if warnings:
    fail(f"{len(warnings)} compiler warning(s):\n  " + "\n  ".join(warnings[:10]))
BIN = RIG / "bin-plug"
BIN.mkdir(exist_ok=True)
for exe in ("horizon-client", "horizon-ctl"):
    shutil.copy(game / exe, BIN / exe)
print(f"Built {built.group(1)}, {'no warnings' if cargo else 'up to date (nothing compiled)'}; game in {BIN}")
if "--no-run" in sys.argv:
    sys.exit(0)

verify = (ROOT / "tools" / "verify.py").read_text()
tests = re.findall(r'@test\("(Plugin API:[^"]+)"\)', verify)
shutil.copy(ROOT / "tools" / "verify.py", RIG / "verify-plug.py")
run = f"plug-{time.strftime('%m%d-%H%M')}"
print(f"Running {len(tests)} plugin checks on {', '.join(targets)} (logs in {RIG / run})")
# RETRY=1: a check that failed runs once more on its own (the send check can miss its line after the others under
# load); it passes only if that run passes, and is named either way.
subprocess.run([str(RIG / "runall.sh"), run, "|".join(tests), *targets], cwd=RIG,
               env={**os.environ, "BIN": str(BIN), "VERIFY": "verify-plug.py", "RETRY": "1"}, stdout=subprocess.DEVNULL)


def results(path):
    lines = path.read_text(errors="replace").splitlines() if path.exists() else []
    return ({l[5:].split(" - ")[0]: l for l in lines if l.startswith("PASS ")},
            {l[5:].split(" - ")[0]: l for l in lines if l.startswith("FAIL ")},
            [l for l in lines if l.startswith(("SKIP", "KICKED"))]
            + ([] if not lines or any(re.match(r"\d+/\d+ passed", l) for l in lines)
               else [f"verify.py stopped early: {lines[-1]}"]))


bad = []
for t in targets:
    passed, failed, other = results(RIG / run / f"{t}.log")
    again, _, other_again = results(RIG / run / f"{t}.retry.log")
    retried = [name for name in failed if name in again]
    ok = [name for name in tests if name in passed or name in retried]
    print(f"{t}: {len(ok)}/{len(tests)} plugin checks passed"
          + (f" ({len(retried)} on the rerun: {'; '.join(retried)})" if retried else ""))
    bad += [f"{t}: {failed.get(name, name + ' - no result')[:200]}" for name in tests if name not in ok]
    bad += [f"{t}: {l[:200]}" for l in other + other_again]
if bad:
    fail("plugin checks failed:\n  " + "\n  ".join(bad))
print("Plugin SDK smoke test passed")
