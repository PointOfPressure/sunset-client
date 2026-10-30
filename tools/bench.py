#!/usr/bin/env python3
# What each drawing module costs in a crowded scene (200 mobs, 100 chests, a field of ore under the platform), off, in
# its default mode, its outline mode and Potato: frames a second and the game's main thread's CPU time a frame (the
# median of four 2 s samples, and in brackets what it adds to the "off" sample just before). Runs on the test
# machine, next to verify.py, whose helpers it uses.
# Usage: bench.py CTL_PATH [module names...]
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import verify as v  # noqa: E402

# Module, its .t name, the modes to time (default first), and what it needs set up.
RUNS = [
    ("ESP", "esp", ["3D box", "Outline", "Potato"]),
    ("Tracers", "tracers", ["Lines", "Potato"]),
    ("Nametags", "nametags", ["Full", "Potato"]),
    ("Storage ESP", "storageesp", ["3D box", "Outline", "Potato"]),
    ("Block ESP", "blockesp", ["Box", "Outline", "Potato"]),
    ("Hole ESP", "holeesp", ["3D box", "Outline", "Potato"]),
    ("Breadcrumbs", "breadcrumbs", ["Line", "Potato"]),
    ("Waypoints", "waypoints", ["Full", "Potato"]),
    ("Trajectories", "trajectories", ["Line", "Potato"]),
    ("Light Overlay", "lightoverlay", ["Cross", "Potato"]),
    ("Tunnel ESP", "tunnelesp", ["Box", "Outline", "Potato"]),
    ("Void ESP", "voidesp", ["Box", "Outline", "Potato"]),
    ("Radar", "radar", ["Round", "Potato"]),
]
ROUNDS = 4
# Settings that make the scene show everything: every kind of mob, mobs' name tags.
EXTRA = {"tracers": [("hostiles", "true"), ("passive", "true")], "nametags": [("mobs", "true")]}


def game_thread():
    """The game's main thread's schedstat file: the frame and the HUD are drawn on it."""
    sock = os.path.join(v.session_dir(), "c.sock").encode()
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                args = f.read().split(b"\0")
        except OSError:
            continue
        if args[0].endswith(b"horizon-client") and sock in args:
            return f"/proc/{pid}/task/{pid}/schedstat"
    raise RuntimeError("no game process")


def cpu_ns(path):
    with open(path) as f:
        return int(f.read().split()[0])


def sample(path, seconds=2.0):
    """Frames a second, and the main thread's CPU milliseconds a frame (steady while other work shares the
    machine, unlike the frame rate)."""
    a, ca = v.ctl("status"), cpu_ns(path)
    time.sleep(seconds)
    b, cb = v.ctl("status"), cpu_ns(path)
    frames = max(b["frames"] - a["frames"], 1)
    return frames / max(b["uptime_seconds"] - a["uptime_seconds"], 1e-6), (cb - ca) / 1e6 / frames


def med(xs):
    return sorted(xs)[len(xs) // 2]


def settle():
    v.wait(30)


def scene():
    x0, y, z0 = v.X0, v.Y, v.Z0
    root = os.path.join(v.world_dir(), "datapacks", "sunset_bench")
    os.makedirs(os.path.join(root, "data", "sunset_bench", "function"), exist_ok=True)
    with open(os.path.join(root, "pack.mcmeta"), "w") as f:
        json.dump({"pack": {"description": "Sunset bench", "min_format": 121, "max_format": 121}}, f)
    lines = [f"fill {x0-16} {y-6} {z0-16} {x0+16} {y-2} {z0+16} minecraft:diamond_ore"]
    for i in range(200):
        kind = "husk" if i % 2 else "pig"
        lines.append(f"summon {kind} {x0 - 10 + i % 20 + 0.5} {y + 1} {z0 + 6 + i // 20} {v.HUSK}")
    for i in range(100):
        lines.append(f"setblock {x0 - 9 + (i % 10) * 2} {y + 4} {z0 + 6 + (i // 10) * 2} minecraft:chest")
    with open(os.path.join(root, "data", "sunset_bench", "function", "scene.mcfunction"), "w") as f:
        f.write("\n".join(lines) + "\n")
    with open(os.path.join(root, "data", "sunset_bench", "function", "clear.mcfunction"), "w") as f:
        f.write(f"fill {x0-16} {y-6} {z0-16} {x0+16} {y-2} {z0+16} minecraft:air\nkill @e[type=!player]\n")
    v.cmd("reload")
    v.wait(20)
    v.cmd("difficulty easy", "gamemode creative", "function sunset_bench:scene", "item replace entity @s hotbar.0 with minecraft:snowball 16")
    v.wait(40)
    # A trail for Breadcrumbs and a few waypoints.
    v.toggle("breadcrumbs", True)
    for i in range(60):
        v.cmd(f"tp @s {x0 + 0.5 + (i % 12) - 6} {y + 1} {z0 + 0.5 - i // 12}")
    v.toggle("breadcrumbs", False)
    for i in range(5):
        v.cmd(f"tp @s {x0 + 0.5 + i * 3 - 6} {y + 1} {z0 + 20.5}")
        v.say(f".waypoint add bench{i}")
    v.cmd(f"tp @s {x0 + 0.5} {y + 1} {z0 + 0.5} 0 15")
    v.wait(40)


def unscene():
    for i in range(5):
        v.say(f".waypoint remove bench{i}")
    v.cmd("function sunset_bench:clear", "difficulty peaceful")


if __name__ == "__main__":
    only = sys.argv[2:]
    v.arena()
    v.ctl("options-set", "maxFps", "260")
    v.ctl("options-set", "enableVsync", "false")
    scene()
    # The HUD's own text off, so only the module timed draws.
    v.say(".t hud")
    path = game_thread()
    for name, key, modes in RUNS:
        if only and name not in only:
            continue
        for k, val in EXTRA.get(key, []):
            v.setting(key, k, val)
        # Off and each mode in turn, ROUNDS times: other work on the machine comes and goes over minutes, so each
        # mode is set against the "off" just before it.
        got = {m: [] for m in ["off", *modes]}
        for _ in range(ROUNDS):
            for mode in got:
                v.toggle(key, mode != "off")
                if mode != "off":
                    v.setting(key, "mode", mode)
                settle()
                got[mode].append(sample(path))
        v.toggle(key, False)
        v.say(f".reset {key}")
        cells = []
        for mode in got:
            fps = med([f for f, _ in got[mode]])
            ms = med([m for _, m in got[mode]])
            extra = med([got[mode][i][1] - got["off"][i][1] for i in range(ROUNDS)])
            cells.append(f"{mode} {fps:.0f} fps {ms:.2f} ms" + ("" if mode == "off" else f" ({extra:+.2f})"))
        print(f"{name}: " + ", ".join(cells), flush=True)
    unscene()
    v.say(".reset hud")
