#!/usr/bin/env python3
"""Do Sunset's settings survive a restart?  restart_check.py BIN [RESOURCES]

BIN is a built version's folder (horizon-ctl and horizon-client in it); RESOURCES the game's resources folder
(or $RES). The game is started twice in a new, empty home (single player, off screen, on $DISPLAY: Xvfb will do).
The first run changes settings with chat commands and quits; the second has to come up with them.

Why this is its own script: every verify.py test runs inside one launch. 2.0.0 read its config through a game call
that answers only for settings registered with the game, so every launch started from the defaults while the file
on disk was whole, and no test saw it. Exit code 0 when every check passes."""
import os, shutil, subprocess, sys, tempfile, time

if len(sys.argv) < 2:
    sys.exit(__doc__)
BIN = os.path.abspath(sys.argv[1])
RES = sys.argv[2] if len(sys.argv) > 2 else os.environ.get("RES", "")
if not RES:
    sys.exit("restart_check.py: give the resources folder (second argument or $RES)")
S = tempfile.mkdtemp(prefix="sunset-restart-")
DATA = S + "/home/horizon-mods/.horizon-mod-data/sunset"
results = []


def check(name, ok, why=""):
    results.append(ok)
    print(("PASS " if ok else "FAIL ") + name + ("" if ok or not why else " - " + why), flush=True)


def ctl(*args, timeout=120):
    r = subprocess.run([BIN + "/horizon-ctl", "--socket", S + "/c.sock", "--timeout", "60", *args],
                       capture_output=True, text=True, timeout=timeout)
    return r.stdout.strip()


def launch():
    env = dict(os.environ, HORIZON_HOME=S + "/home", XDG_DATA_HOME=S + "/data", XDG_CONFIG_HOME=S + "/config",
               HORIZON_NO_AUDIO="1", HORIZON_NO_HEARTBEAT="1", HORIZON_NO_DISCORD="1", HORIZON_TELEMETRY_URL="off",
               HORIZON_LAUNCHER_NO_SELF_UPDATE="1", SUNSET_PRESENCE_BASE="off")
    for d in ("home", "data", "config"):
        os.makedirs(f"{S}/{d}", exist_ok=True)
    subprocess.run([BIN + "/horizon-ctl", "--socket", S + "/c.sock", "--timeout", "300", "launch", "--game",
                    BIN + "/horizon-client", "--world", "RestartCheck", "--creative", "--offscreen", "--wait",
                    "--resources", RES], cwd=S + "/home", env=env, stdout=subprocess.DEVNULL,
                   stderr=subprocess.DEVNULL, timeout=600)
    ctl("wait-ready", "180", timeout=240)
    ctl("wait", "40")


def say(text):
    ctl("key", "t", "tap"); ctl("wait", "4"); ctl("type", text); ctl("key", "enter", "tap"); ctl("wait", "4")


def read(path):
    try:
        return open(path, errors="replace").read()
    except OSError:
        return ""


def pairs(text):
    out = {}
    for line in text.splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            out.setdefault(k, []).append(v.strip())
    return out


def same(a, b):
    if a == b:
        return True
    try:
        return len(a) == len(b) and all(abs(float(x) - float(y)) < 1e-9 for x, y in zip(a, b))
    except ValueError:
        return False


# What the first run sets, as the config keeps it.
WANT = {"sprint.on": "true", "killaura.range": "4.25", "killaura.key": "key.keyboard.r", "friends": "Zed"}
try:
    launch()
    first = pairs(read(DATA + "/config"))
    check("first launch writes a config", any(k.endswith(".on") for k in first), "no config in " + DATA)
    for line in (".t sprint", ".set killaura range 4.25", ".bind killaura r", ".friend add Zed"):
        say(line)
    changed = read(DATA + "/config")
    got = pairs(changed)
    off = {k: got.get(k) for k, v in WANT.items() if got.get(k) != [v]}
    check("changes are on disk at once", not off, f"not as set: {off}")
    ctl("quit"); time.sleep(6)
    check("quitting leaves the config as it was", read(DATA + "/config") == changed)

    launch()
    say(".profile save restartcheck")
    time.sleep(1)
    now = pairs(read(DATA + "/profiles/restartcheck.profile"))
    check("second launch: a profile of the settings in use can be saved", bool(now), "no profiles/restartcheck.profile")
    shared = [k for k in now if k in got]
    lost = [k for k in shared if not same(got[k], now[k])]
    check(f"second launch: the settings in use are the ones on disk ({len(shared)} compared)", bool(shared) and not lost,
          f"{len(lost)} differ, e.g. " + ", ".join(f"{k} disk {got[k][0]} game {now[k][0]}" for k in lost[:4]))
    off = {k: now.get(k) for k, v in WANT.items() if k != "friends" and now.get(k) != [v]}
    check("second launch: what the first run set is still set", not off, f"not as set: {off}")
    after = pairs(read(DATA + "/config"))
    check("second launch: friends are kept", after.get("friends") == ["Zed"], f"friends={after.get('friends')}")
    bak = pairs(read(DATA + "/config.bak"))
    check("second launch: a copy of the config it read is kept (config.bak)", bool(bak) and all(same(bak.get(k, []), v) for k, v in got.items()),
          "missing" if not bak else "not the config that was read")
    ctl("quit"); time.sleep(4)
finally:
    try:
        ctl("quit", timeout=20)
    except Exception:
        pass
    if all(results) and results:
        shutil.rmtree(S, ignore_errors=True)
    else:
        print("kept for a look:", S)
print(f"{sum(results)} passed, {len(results) - sum(results)} failed")
sys.exit(0 if results and all(results) else 1)
