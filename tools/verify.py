#!/usr/bin/env python3
# Runs each Sunset module in a flat test arena in a headless Horizon (ctl =
# horizon-ctl on the test session) and measures what it does: the numbers go in
# each PASS/FAIL line so runs can be compared. Usage: verify.py CTL_PATH [test names...]
#
# Every test starts from the same place (clean): every module off at its
# defaults, the arena bare, nothing held or open, the player healthy in
# survival in the middle of the platform. So the full run gives what each test
# gives alone.
#
# Tests that need a second player (on a server) use the control wrapper in the
# CTL2 environment variable and are skipped without it; it plays Bot1.
# The per-frame Aim Assist tests read the test probe's file (tools/probe,
# staged next to Sunset on the test machine; the game writes it to the path in
# SUNSET_PROBE): PROBE names it, else <session>/probe.log. They move mobs with
# a small data pack written into the world folder: WORLD names it, else the
# single player world of the session or ~/hzmeteor/srv/world on a server.
import json, math, os, subprocess, sys, time

CTL = sys.argv[1]
CTL2 = os.environ.get("CTL2")
# The server joined (HOST:PORT), to join again when it kicks you; the kicks are listed at the end.
SERVER = os.environ.get("SERVER")
kicks = []
ONLY = sys.argv[2:]
on = set()
# Modules whose settings a test changed: clean() puts them back to their defaults.
touched = set()
results = []


def ctl(*args, timeout=90):
    r = subprocess.run([CTL, *args], capture_output=True, text=True, timeout=timeout)
    out = r.stdout.strip()
    try:
        return json.loads(out) if out else {}
    except json.JSONDecodeError:
        return {"raw": out}


def ctl2(*args):
    r = subprocess.run([CTL2, *args], capture_output=True, text=True, timeout=90)
    return json.loads(r.stdout) if r.stdout.strip() else {}


def wait(n):
    ctl("wait", str(n))


def say(text):
    """A line typed into the chat box: Sunset's . commands."""
    ctl("key", "t", "tap")
    wait(1)
    ctl("type", text)
    ctl("key", "enter", "tap")
    wait(1)


def cmd(*lines):
    """Game commands, sent as the player without opening the chat box. The chat box takes 256 characters and
    Paper-family servers kick a longer command packet, so a longer line is a test mistake."""
    for line in lines:
        line = line.lstrip("/")
        if len(line) > 256:
            raise ValueError(f"command over 256 characters: {line[:40]}...")
        ctl("chat", "/" + line)
    wait(2)


def state():
    return ctl("state")


def pos():
    return state()["player"]["position"]


def hp():
    return state()["player"]["health"]


def log_mark():
    """Where the game's log ends now (Sunset's own chat lines are logged there, not in the server's chat)."""
    return os.path.getsize(os.path.join(session_dir(), "home", "logs", "latest.log"))


def sunset_said(mark):
    with open(os.path.join(session_dir(), "home", "logs", "latest.log"), errors="replace") as f:
        f.seek(mark)
        return [line.split("[CHAT] [Sunset] ", 1)[1].strip() for line in f if "[CHAT] [Sunset] " in line]


def chat_first():
    c = ctl("chat-log", "1").get("chat", [])
    return c[0] if c else ""


def check(command):
    cmd(command)
    wait(1)
    return "passed" in chat_first()


def toggle(name, want):
    key = name.lower().replace(" ", "")
    if (key in on) != want:
        say(f".t {key}")
        (on.add if want else on.discard)(key)


def only(*names):
    for n in list(on):
        say(f".t {n}")
        on.discard(n)
    for n in names:
        toggle(n, True)


def setting(mod, name, value):
    touched.add(mod)
    say(f".set {mod} {name} {value}")


X0 = Z0 = 0
Y = 120
HUSK = "{NoAI:1b,Invulnerable:1b,PersistenceRequired:1b,Silent:1b}"
NOAI = "{NoAI:1b,PersistenceRequired:1b}"


def arena():
    global X0, Z0
    p = pos()
    X0, Z0 = int(p[0] // 1), int(p[2] // 1)
    # Unattended, the game drops to 30 frames a second after a minute ("afk"): keep the full rate.
    ctl("options-set", "inactivityFpsLimit", "minimized")
    clean()


def ensure_alive():
    for _ in range(3):
        st = state()
        if st.get("screen") != "dead" and st.get("player"):
            return
        ctl("click", "respawn")
        wait(30)


def rejoin(test):
    """After a kick: what the screen said, then back in."""
    if state().get("player") or not SERVER:
        return
    shown = ctl("screen")
    kicks.append((test, json.dumps(shown)[:300]))
    for _ in range(3):
        ctl("key", "esc", "tap")
        time.sleep(2)
        ctl("connect", SERVER, "Tester", timeout=120)
        ctl("wait-ready", "90", timeout=120)
        if state().get("player"):
            ctl("wait", "40")
            return


def clean():
    """Back to where every test starts."""
    ctl("release")
    ensure_alive()
    ctl("key", "1", "tap")
    screen = state().get("screen")
    if screen == "paused":
        ctl("resume")
    elif screen not in ("playing", None):
        ctl("key", "esc", "tap")
        wait(2)
        if state().get("screen") not in ("playing", None):
            ctl("close")
    # Every module off, whatever a test left on (a toggle typed while dead never arrives), then the HUD back on.
    # .panic hides Sunset too; the second one brings it back.
    say(".panic")
    say(".panic")
    say(".reset hud")
    on.clear()
    for m in sorted(touched):
        say(f".reset {m}")
    touched.clear()
    # Land first, in whatever game mode, so a fall a test left you in costs nothing.
    cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
    wait(3)
    # A world with real terrain (a server's) has hills round the platform: clear a wide ring, high enough for No
    # Fall's 12-block drop (else the player is teleported into a hill and suffocates for 1 health).
    cmd("gamemode survival @s", "clear @s", "effect clear @s", "xp set @s 0 levels", "xp set @s 0 points",
        "kill @e[type=!player]", "difficulty peaceful", "time set 6000", "weather clear", "gamerule natural_health_regeneration true",
        f"fill {X0-24} {Y-1} {Z0-24} {X0+24} {Y+4} {Z0+24} minecraft:air",
        f"fill {X0-24} {Y+5} {Z0-24} {X0+24} {Y+16} {Z0+24} minecraft:air",
        f"fill {X0-16} {Y} {Z0-16} {X0+16} {Y} {Z0+16} minecraft:stone",
        f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0",
        "effect give @s minecraft:instant_health 1 10 true", "effect give @s minecraft:saturation 1 10 true")
    # A moment in water: any fall the server still counts (one Sunset kept from it) is gone.
    cmd(f"setblock {X0} {Y+1} {Z0} minecraft:water")
    wait(3)
    cmd(f"setblock {X0} {Y+1} {Z0} minecraft:air")
    if CTL2:
        # The second player waits out of the way, where nothing targets it.
        ctl2("release")
        cmd("gamemode spectator Bot1", f"tp Bot1 {X0 + 0.5} {Y + 30} {Z0 + 60}")
    wait(10)


def reset(mode="survival", x=0.5, z=0.5, y=Y + 1):
    ensure_alive()
    cmd(f"gamemode {mode}", "clear @s", "effect clear @s", "effect give @s minecraft:instant_health 1 10 true",
        "effect give @s minecraft:saturation 1 10 true", "kill @e[type=!player]", f"tp @s {X0 + x} {y} {Z0 + z} 0 0")
    wait(20)
    ctl("look", "0", "0")


def flat(a, b):
    return math.hypot(b[0] - a[0], b[2] - a[2])


def held_keys(keys, ticks):
    for k in keys:
        ctl("key", k, "down")
    wait(ticks)
    for k in keys:
        ctl("key", k, "up")


def rate(keys=("w",), ticks=20, warm=12, up=False):
    """Blocks a second, across the ground (or up, with up=True), while the keys are held, once up to speed."""
    for k in keys:
        ctl("key", k, "down")
    wait(warm)
    a = state()
    wait(ticks)
    b = state()
    for k in keys:
        ctl("key", k, "up")
    wait(5)
    pa, pb = a["player"]["position"], b["player"]["position"]
    gone = (pb[1] - pa[1]) if up else flat(pa, pb)
    return gone / max(b["ticks"] - a["ticks"], 1) * 20


def fall_rate(ticks=20, warm=10):
    """Blocks a second downward, with nothing held."""
    wait(warm)
    a = state()
    wait(ticks)
    b = state()
    return (a["player"]["position"][1] - b["player"]["position"][1]) / max(b["ticks"] - a["ticks"], 1) * 20


def walk(ticks, key="w", extra=None):
    ctl("key", key, "down")
    if extra:
        ctl("key", extra, "down")
    wait(ticks)
    ctl("key", key, "up")
    if extra:
        ctl("key", extra, "up")
    wait(3)


def count(x1, y1, z1, x2, y2, z2, block):
    return ctl("blocks", *map(str, (x1, y1, z1, x2, y2, z2))).get("counts", {}).get(f"minecraft:{block}", 0)


def mobs(kind=None):
    return [e for e in ctl("entities").get("entities", []) if e.get("kind") == "mob" and (kind is None or e.get("type") == f"minecraft:{kind}")]


def slot(i, item, n=1):
    cmd(f"item replace entity @s hotbar.{i} with minecraft:{item} {n}")


def offhand():
    """The item id in the offhand ("" for none)."""
    return (ctl("inventory").get("offhand") or {}).get("item") or ""


def held():
    return state().get("held") or {}


def test(name):
    def deco(fn):
        fn.test_name = name
        TESTS.append(fn)
        return fn
    return deco


TESTS = []

# ---- movement

@test("Speed")
def _():
    plain = rate()
    only("speed")
    reset(z=-14.5)
    vanilla = rate()
    setting("speed", "vanilla speed", "10")
    reset(z=-14.5)
    ten = rate()
    reset(z=-14.5)
    sneaking = rate(("w", "shift"))
    setting("speed", "when sneaking", "true")
    reset(z=-14.5)
    sneak_fast = rate(("w", "shift"))
    # Without the move keys Vanilla stands still at once.
    reset(z=-14.5)
    ctl("key", "w", "down")
    wait(10)
    ctl("key", "w", "up")
    a = pos()
    wait(1)
    b = pos()
    stop = flat(a, b)
    setting("speed", "mode", "Strafe")
    reset(z=-14.5)
    strafe = rate(ticks=30)
    setting("speed", "mode", "Vanilla")
    setting("speed", "timer", "2")
    t0, c0 = state()["ticks"], time.time()
    held_keys(("w",), 20)
    tps = (state()["ticks"] - t0) / (time.time() - c0)
    only()
    reset(z=-14.5)
    back = rate()
    ok = (abs(vanilla - 5.6) < 0.2 and abs(ten - 10) < 0.3 and sneaking < 1.6 and abs(sneak_fast - 10) < 0.3 and stop < 0.02
          and strafe > plain * 1.2 and tps > 30 and abs(back - plain) < 0.2)
    return ok, (f"walking {plain:.2f} blocks/s; Vanilla speed 5.6: {vanilla:.2f}, 10: {ten:.2f}; sneaking {sneaking:.2f}, {sneak_fast:.2f} with When sneaking; "
                f"moves {stop:.3f} the tick after letting go; Strafe {strafe:.2f}; Timer 2 while moving: {tps:.0f} ticks/s; off again {back:.2f}")


@test("Sprint")
def _():
    reset(z=-14.5)
    plain = rate()
    only("sprint")
    reset(z=-14.5)
    sprint = rate()
    only()
    reset(z=-14.5)
    back = rate()
    # Vanilla sprinting is 1.3 times walking.
    return 1.25 < sprint / plain < 1.35 and abs(back - plain) < 0.3, f"walking {plain:.2f} blocks/s, {sprint:.2f} with Sprint ({sprint/plain:.2f}x), off again {back:.2f}"


@test("Auto Walk")
def _():
    reset(z=-14.5)
    only("autowalk")
    wait(12)
    a = state()
    wait(20)
    b = state()
    only()
    wait(10)
    c = pos()
    wait(10)
    d = pos()
    pa, pb = a["player"]["position"], b["player"]["position"]
    speed = flat(pa, pb) / (b["ticks"] - a["ticks"]) * 20
    straight = abs(pb[0] - pa[0]) < 0.05 and pb[2] > pa[2]
    stopped = flat(c, d)
    return 4.0 < speed < 4.6 and straight and stopped < 0.05, f"{speed:.2f} blocks/s straight ahead, {stopped:.2f} blocks after it went off"


@test("Flight")
def _():
    def fly(**sets):
        reset(y=Y + 3)
        for k, v in sets.items():
            setting("flight", k.replace("_", " "), v)
        only("flight")
        wait(5)
        up = rate(("space",), ticks=6, warm=4, up=True)
        wait(10)
        a = pos()
        wait(40)
        drift = pos()[1] - a[1]
        reset(z=-14.5, y=Y + 4)
        across = rate(("w",), ticks=10, warm=40)
        only()
        return up, drift, across

    # Abilities (creative flight) at Speed 0.1, twice vanilla's 0.05; Velocity: Speed x 10 across, x 5 up (x 10 matched).
    ab = fly()
    vanilla = fly(speed="0.05")
    vel = fly(mode="Velocity", speed="0.1")
    match = fly(mode="Velocity", speed="0.1", vertical_speed_match="true")
    cmd(f"tp @s {X0 + 0.5} {Y + 16} {Z0 + 0.5}")
    falling = fall_rate(ticks=6, warm=4)
    ok = (abs(ab[2] - 2 * vanilla[2]) < 1.5 and abs(vanilla[2] - 10.9) < 1 and abs(vel[2] - 20) < 1 and abs(vel[0] - 10) < 1
          and abs(match[0] - 20) < 2 and max(abs(r[1]) for r in (ab, vanilla, vel, match)) < 0.05 and falling > 3)
    return ok, (f"Abilities, Speed 0.1: across {ab[2]:.1f} blocks/s, up {ab[0]:.1f}; Speed 0.05: across {vanilla[2]:.1f}, up {vanilla[0]:.1f}; "
                f"Velocity, Speed 0.1: across {vel[2]:.1f}, up {vel[0]:.1f}, {match[0]:.1f} with Vertical speed match; "
                f"hovering moves at most {max(abs(r[1]) for r in (ab, vanilla, vel, match)):.3f} in 2 s; off: falls at {falling:.1f}")


@test("Flight (no fall damage, level hover)")
def _():
    only("flight")
    ctl("key", "space", "down")
    wait(60)
    ctl("key", "space", "up")
    wait(6)
    y0 = pos()[1]
    wait(100)
    drift = pos()[1] - y0
    ctl("key", "shift", "down")
    wait(120)
    ctl("key", "shift", "up")
    wait(10)
    health = hp()
    only()
    return abs(drift) < 0.05 and health >= 20, f"hovered {drift:+.3f} blocks in 5 s, health {health} after flying up {y0 - Y - 1:.0f} and sneaking down"


def drop(height):
    """Health lost falling `height` blocks onto the platform (with natural regeneration off, so none comes back)."""
    reset()
    cmd("gamerule natural_health_regeneration false", f"tp @s {X0 + 0.5} {Y + 1 + height} {Z0 + 0.5}")
    wait(20 + 3 * height)
    lost = 20 - hp()
    cmd("gamerule natural_health_regeneration true")
    return lost


@test("No Fall")
def _():
    plain = drop(12)
    small = drop(3)
    only("nofall")
    mine = drop(12)
    only()
    back = drop(12)
    return plain >= 3 and small == 0 and mine == 0 and back == plain, f"12-block fall costs {plain:.0f} health without, {mine:.0f} with, {back:.0f} with it off again; a 3-block fall {small:.0f}"


@test("No Fall (No ground)")
def _():
    setting("nofall", "mode", "No ground")
    only("nofall")
    mine = drop(12)
    return mine == 0, f"12-block fall costs {mine:.0f} health with No ground on"


@test("No Fall (Air place)")
def _():
    setting("nofall", "mode", "Air place")
    setting("nofall", "air place mode", "Before damage")
    only("nofall")
    reset()
    slot(0, "cobblestone", 16)
    cmd("gamerule natural_health_regeneration false", f"tp @s {X0 + 0.5} {Y + 13} {Z0 + 0.5}")
    wait(40)
    lost, stood = 20 - hp(), pos()[1] - (Y + 1)
    placed = held().get("count", 16)
    cmd("gamerule natural_health_regeneration true")
    only()
    return lost == 0 and stood > 1 and placed < 16, f"lost {lost:.0f} health, came to rest {stood:.1f} blocks up on {16 - placed} placed block(s)"


@test("No Fall (Place: water bucket)")
def _():
    setting("nofall", "mode", "Place")
    only("nofall")
    reset()
    slot(1, "water_bucket")
    cmd("gamerule natural_health_regeneration false", f"tp @s {X0 + 0.5} {Y + 13} {Z0 + 0.5}")
    wait(60)
    lost = 20 - hp()
    inv = ctl("inventory").get("counts", {})
    water = count(X0 - 2, Y + 1, Z0 - 2, X0 + 2, Y + 2, Z0 + 2, "water")
    cmd("gamerule natural_health_regeneration true", f"fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+2} {Z0+3} minecraft:air")
    only()
    back = "minecraft:water_bucket" in inv
    return lost == 0 and back and water == 0, f"lost {lost:.0f} health; water picked back up: {back} ({water} water left)"


@test("No Fall (after Flight)")
def _():
    only("flight", "nofall")
    ctl("key", "space", "down")
    wait(60)
    ctl("key", "space", "up")
    top = pos()[1] - (Y + 1)
    only("nofall")
    wait(80)
    health = hp()
    landed = pos()[1] - (Y + 1)
    only()
    return top > 10 and abs(landed) < 0.01 and health >= 20, f"health {health} after Flight went off {top:.0f} blocks up"


# ---- No Fall, rock solid: nothing from any height, onto anything, after anything, at any ping; and with No Fall off,
# Sunset never leaves the server a fall you did not take (the fall count tests).

def fall_ticks(height):
    """Ticks a fall of `height` blocks takes (gravity 0.08 and drag 0.98 a tick), and a second more to land."""
    v = y = t = 0
    while y < height:
        v = (v + 0.08) * 0.98
        y += v
        t += 1
    return t + 20


def server_fall():
    """The server's count of your fall, from /data (None where it doesn't say)."""
    cmd("data get entity @s fall_distance")
    for line in ctl("chat-log", "4").get("chat", []):
        if "entity data: " in line:
            try:
                return float(line.split("entity data: ")[1].rstrip("df"))
            except ValueError:
                return None
    return None


def heal():
    if hp() < 20:
        cmd("effect give @s minecraft:instant_health 1 10 true")
        wait(5)


def health_lost(before):
    """Health lost since `before`: all of it on a death (a dead player's health reads as anything), and back alive
    after, so the next step's toggles are not typed into the death screen."""
    st = state()
    if st.get("screen") == "dead" or not st.get("player"):
        ensure_alive()
        return before
    return before - st["player"]["health"]


def dropped(height, dx=0.5, dz=0.5, during=None, settle=0):
    """Health lost dropping `height` blocks onto the platform at (dx, dz), regeneration off (reset() first);
    `during` runs as the fall starts."""
    cmd("gamerule natural_health_regeneration false")
    heal()
    before = hp()
    ctl("chat", f"/tp @s {X0 + dx} {Y + 1 + height} {Z0 + dz}")
    if during:
        during()
    wait(fall_ticks(height) + settle)
    lost = health_lost(before)
    cmd("gamerule natural_health_regeneration true")
    return lost


def worst(lost):
    return ", ".join(f"{k}: {v:.0f}" for k, v in lost.items())


@test("No Fall (heights)")
def _():
    reset()
    only("nofall")
    lost = {h: dropped(h) for h in (4, 8, 16, 32, 64, 128, 200, 300)}
    only()
    return max(lost.values()) == 0, f"health lost from each height: {worst(lost)}"


@test("No Fall (surfaces)")
def _():
    reset()
    only("nofall")
    lost = {}
    cmd(f"setblock {X0} {Y+1} {Z0} minecraft:smooth_stone_slab")
    lost["slab"] = dropped(40)
    cmd(f"setblock {X0} {Y+1} {Z0} minecraft:air", f"setblock {X0+1} {Y+1} {Z0} minecraft:water")
    lost["beside water"] = dropped(40, dx=0.75)
    lost["shallow water"] = dropped(40, dx=1.5)
    cmd(f"setblock {X0+1} {Y+1} {Z0} minecraft:air", f"fill {X0} {Y+1} {Z0+1} {X0} {Y+3} {Z0+1} minecraft:stone",
        f"fill {X0} {Y+1} {Z0} {X0} {Y+2} {Z0} minecraft:ladder[facing=north]")
    lost["ladder"] = dropped(40)
    cmd(f"fill {X0} {Y+1} {Z0} {X0} {Y+3} {Z0+1} minecraft:air", f"setblock {X0} {Y+1} {Z0} minecraft:stone")
    lost["block edge"] = dropped(40, dx=1.25)
    cmd(f"setblock {X0} {Y+1} {Z0} minecraft:air")
    only()
    return max(lost.values()) == 0, f"health lost landing from 40 blocks: {worst(lost)}"


def hurt(act, settle=20):
    """Health lost while `act` runs and `settle` ticks after, regeneration off, from full health."""
    cmd("gamerule natural_health_regeneration false")
    heal()
    before = hp()
    act()
    wait(settle)
    lost = health_lost(before)
    cmd("gamerule natural_health_regeneration true")
    return lost


@test("No Fall (after other modules)")
def _():
    lost = {}
    reset()
    only("flight", "nofall")
    lost["Flight off 30 blocks up"] = hurt(lambda: (held_keys(("space",), 50), only("nofall"), wait(fall_ticks(30))))
    # Each on through a fall (Blink holds the whole fall and lets it go in one move as you land).
    setting("blink", "max ticks", "0")
    for mod in ("speed", "velocity", "blink", "antihunger", "criticals", "step"):
        only("nofall", mod)
        lost[mod] = dropped(40, during=lambda: held_keys(("w",), 10))
    only("nofall")
    # A teleport in mid-fall (as a server's correction is one): the fall goes on from where it puts you.
    lost["teleport mid-fall"] = dropped(30, during=lambda: (wait(15), ctl("chat", f"/tp @s {X0 + 0.5} {Y + 41} {Z0 + 0.5}")), settle=40)
    # Elytra: glide down across the platform from 60 blocks up, and land.
    reset()
    only("nofall")
    cmd("item replace entity @s armor.chest with minecraft:elytra")

    # The glide's path clear of what earlier runs left above the arena (a server's world keeps it).
    cmd(f"fill {X0-3} {Y+5} {Z0-15} {X0+3} {Y+63} {Z0+3} minecraft:air")

    def glide():
        ctl("chat", f"/tp @s {X0 + 0.5} {Y + 61} {Z0 - 12.5} 0 70")
        wait(8)
        held_keys(("space",), 2)
        wait(2 * fall_ticks(60))
    lost["elytra"] = hurt(glide, settle=40)
    p = state()["player"]
    landed = not p.get("fall_flying") and p.get("on_ground")
    only()
    return max(lost.values()) == 0 and landed, f"health lost: {worst(lost)}; the glide landed {p['position'][1] - Y - 1:+.1f} blocks from the platform's top"


@test("No Fall (respawn and other worlds)")
def _():
    reset()
    only("nofall")
    lost = {}
    cmd("kill @s")
    wait(20)
    ensure_alive()
    wait(20)
    lost["right after respawn"] = dropped(40)
    # The Nether's roof (bedrock at 127) from 160, then back.

    def nether():
        ctl("chat", "/execute in minecraft:the_nether run tp @s 0.5 160 0.5")
        wait(fall_ticks(33) + 40)
    lost["the Nether"] = hurt(nether)
    where = state().get("dimension")
    cmd(f"execute in minecraft:overworld run tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5}")
    wait(60)
    lost["back, at once"] = dropped(40)
    only()
    return max(lost.values()) == 0 and where == "minecraft:the_nether", f"health lost: {worst(lost)} (was in {where})"


class Lag:
    """A relay between the game and SERVER that holds everything `ms` milliseconds each way, and all of it while
    stalled: high ping and lag spikes for this one connection."""

    def __init__(self, ms):
        import socket, threading
        self.delay, self.stalled_until = ms / 1000, 0.0
        self.listener = socket.create_server(("127.0.0.1", 0))
        self.port = self.listener.getsockname()[1]
        host, port = SERVER.rsplit(":", 1)

        def pipe(src, dst):
            import queue
            q = queue.Queue()

            def read():
                try:
                    while chunk := src.recv(65536):
                        q.put((time.time() + self.delay, chunk))
                except OSError:
                    pass
                q.put(None)

            def write():
                try:
                    while (item := q.get()) is not None:
                        while (left := max(item[0], self.stalled_until) - time.time()) > 0:
                            time.sleep(left)
                        dst.sendall(item[1])
                    dst.shutdown(socket.SHUT_WR)
                except OSError:
                    pass
            for f in (read, write):
                threading.Thread(target=f, daemon=True).start()

        def serve():
            while True:
                try:
                    game, _ = self.listener.accept()
                    server = socket.create_connection((host, int(port)))
                except OSError:
                    return
                pipe(game, server)
                pipe(server, game)
        threading.Thread(target=serve, daemon=True).start()

    def stall(self, seconds):
        self.stalled_until = time.time() + seconds


def join(address):
    ctl("disconnect")
    time.sleep(2)
    ctl("connect", address, "Tester", timeout=120)
    ctl("wait-ready", "90", timeout=120)
    wait(40)


@test("No Fall (high ping and lag spikes)")
def _():
    if not SERVER:
        return None, "servers only"
    lag = Lag(150)
    join(f"127.0.0.1:{lag.port}")
    lost = {}
    try:
        reset()
        only("nofall")
        for h in (16, 64, 128):
            lost[f"{h} at 300 ms"] = dropped(h)
        # A second and a half with nothing through, once in mid-fall and once just before landing.
        lost["spike mid-fall"] = dropped(64, during=lambda: (wait(10), lag.stall(1.5)), settle=40)
        lost["spike at landing"] = dropped(32, during=lambda: (wait(fall_ticks(32) - 24), lag.stall(1.5)), settle=40)
        only()
    finally:
        join(SERVER)
    return max(lost.values()) == 0, f"health lost (300 ms there and back): {worst(lost)}"


@test("No Fall (200 drops)")
def _():
    # Heights from 4 to 300 blocks, some with Flight, Speed or Blink just before, a teleport or a jump in the fall.
    # About half an hour: past the usual ten minutes a test gets.
    import random, signal
    signal.alarm(3600)
    started = time.time()
    rng = random.Random(7)
    reset()
    only("nofall")
    worst_drop, hurt, total = (0, ""), 0, 0
    befores = ["", "", "flight", "speed", "blink", "teleport", "jump"]
    for i in range(200):
        h = round(4 * (75 ** rng.random()))
        how = rng.choice(befores)
        if how in ("flight", "speed", "blink"):
            toggle(how, True)
        during = {"speed": lambda: held_keys(("w",), 10), "flight": lambda: held_keys(("space",), 5),
                  "teleport": lambda: (wait(5), ctl("chat", f"/tp @s {X0 + 0.5} {Y + 1 + h} {Z0 + 0.5}")),
                  "jump": lambda: held_keys(("space",), 3)}.get(how)
        if how in ("flight", "blink"):
            # Flight and Blink go off as the fall begins. Blink held through the whole fall would show a vanilla
            # server no move for longer than its 80 ticks of hovering (it kicks: "Flying is not enabled"), and let it
            # go as one move of more than 10 blocks, which it refuses as moving too quickly.
            key = "space" if how == "flight" else "w"
            during = lambda: (held_keys((key,), 5), toggle(how, False))
        lost = dropped(h, during=during, settle=30 if how == "teleport" else 0)
        for m in ("flight", "speed", "blink"):
            toggle(m, False)
        total += 1
        if lost > 0:
            hurt += 1
        if lost > worst_drop[0]:
            worst_drop = (lost, f"{h} blocks {how or 'plain'}")
        ensure_alive()
    only()
    return hurt == 0, (f"{total} drops in {(time.time() - started) / 60:.0f} min, {hurt} hurt; worst: {worst_drop[0]:.0f} "
                       f"health ({worst_drop[1] or 'none'})")


@test("Fall count (No Fall off)")
def _():
    # With No Fall off, a module that hid the ground or claimed it in the air must not leave the server a fall to
    # make you pay later: the jumps Anti Hunger hid, a fall No ground hid, a fall Flight or No Fall caught in the
    # air. A plain 12-block fall still costs what it costs.
    lost = {}
    reset()
    lost["plain 12 blocks"] = dropped(12)

    def hunger():
        slot(0, "diamond_pickaxe")
        cmd(f"setblock {X0} {Y+1} {Z0+2} minecraft:stone")
        only("antihunger")
        for _ in range(8):
            held_keys(("space",), 3)
            wait(12)
        ctl("look", "0", "30")
        ctl("mouse", "left", "down")
        wait(30)
        ctl("mouse", "left", "up")
        only()
        held_keys(("space",), 3)
    reset()
    lost["Anti Hunger: 8 jumps, a block broken, off"] = hurt(hunger, settle=30)

    def no_ground(then):
        def act():
            setting("nofall", "mode", "No ground")
            only("nofall")
            ctl("chat", f"/tp @s {X0 + 0.5} {Y + 13} {Z0 + 0.5}")
            wait(fall_ticks(12))
            then()
            walk(20)
            held_keys(("space",), 3)
        return act
    reset()
    lost["No ground: 12 blocks, then off"] = hurt(no_ground(lambda: only()), settle=30)
    reset()
    lost["No ground: 12 blocks, then Flight"] = hurt(no_ground(lambda: (only("flight"), held_keys(("space",), 10), only())), settle=60)
    reset()
    lost["Flight on 20 blocks into a fall"] = dropped(40, during=lambda: (wait(fall_ticks(20) - 20), only("flight"), wait(20), only()), settle=40)
    setting("nofall", "mode", "Packet")
    lost["No Fall on 20 blocks into a fall"] = dropped(40, during=lambda: (wait(fall_ticks(20) - 20), only("nofall")))
    only()
    count = server_fall()
    ok = lost["plain 12 blocks"] == 9 and all(v == 0 for k, v in lost.items() if k != "plain 12 blocks")
    return ok, f"health lost: {worst(lost)}; the server's count after: {count}"


def wall(height, at=3):
    cmd(f"fill {X0-3} {Y+1} {Z0+at} {X0+3} {Y+height} {Z0+at} minecraft:stone")


def unwall():
    cmd(f"fill {X0-3} {Y+1} {Z0+1} {X0+3} {Y+12} {Z0+6} minecraft:air")


@test("Step")
def _():
    wall(1)
    walk(40)
    base = pos()[2] - Z0
    reset()
    only("step")
    walk(40)
    one = pos()[2] - Z0
    cmd(f"fill {X0-3} {Y+2} {Z0+3} {X0+3} {Y+2} {Z0+3} minecraft:stone")
    reset()
    walk(40)
    two = pos()[2] - Z0
    cmd(f"fill {X0-3} {Y+2} {Z0+3} {X0+3} {Y+2} {Z0+3} minecraft:air")
    only()
    reset()
    walk(40)
    back = pos()[2] - Z0
    # Height 1.25 climbs a block, not two; vanilla's 0.6 climbs neither.
    return one > 4 and base < 3 and two < 3 and back < 3, f"z {base:.1f} blocked by a block, {one:.1f} with Step, {two:.1f} at a 2-block wall, {back:.1f} off again"


@test("Step height")
def _():
    wall(2)
    only("step")
    setting("step", "height", "2.25")
    walk(40)
    high = pos()[2] - Z0
    setting("step", "active when", "Sneaking")
    reset()
    walk(40)
    held = pos()[2] - Z0
    reset()
    walk(100, extra="shift")
    sneaking = pos()[2] - Z0
    only()
    return high > 4 and held < 3 and sneaking > 4, f"z {high:.1f} over 2 blocks at Height 2.25; Active when Sneaking: {held:.1f} walking, {sneaking:.1f} sneaking"


@test("Glide")
def _():
    # In creative, so the falls cost nothing.
    reset("creative", y=Y + 60)
    free = fall_rate(ticks=10, warm=10)
    reset("creative", y=Y + 60)
    only("glide")
    slow = fall_rate()
    setting("glide", "fall speed", "5")
    reset("creative", y=Y + 60)
    faster = fall_rate()
    reset("creative", y=Y + 30)
    only()
    back = fall_rate(ticks=10, warm=10)
    return abs(slow - 2.5) < 0.2 and abs(faster - 5) < 0.3 and free > 10 and back > 10, f"falls at {slow:.2f} blocks/s with Fall speed 2.5, {faster:.2f} with 5; {free:.1f} without, {back:.1f} off again"


@test("Auto Jump")
def _():
    wall(1)
    only("autojump")
    walk(40)
    mine = pos()[2] - Z0
    only()
    reset()
    walk(40)
    back = pos()[2] - Z0
    return mine > 4 and back < 3, f"reached z {mine:.1f} past a 1-block wall, {back:.1f} off"


@test("Spider")
def _():
    wall(12, at=1)
    only("spider")
    climb = rate(ticks=10, warm=6, up=True)
    setting("spider", "climb speed", "0.4")
    reset()
    faster = rate(ticks=10, warm=6, up=True)
    only()
    reset()
    back = rate(ticks=10, warm=6, up=True)
    # Climb speed is blocks a tick.
    return abs(climb - 4) < 0.4 and abs(faster - 8) < 0.8 and abs(back) < 0.1, f"climbs {climb:.1f} blocks/s with Climb speed 0.2, {faster:.1f} with 0.4, {back:.1f} off"


def jump_peak():
    """How high one jump takes you, sampled every tick."""
    y0 = pos()[1]
    ctl("key", "space", "down")
    wait(2)
    ctl("key", "space", "up")
    best = y0
    for _ in range(40):
        wait(1)
        best = max(best, pos()[1])
    return best - y0


def jump_height(v0):
    """The peak of a jump that starts at `v0` blocks a tick (vanilla's step: move, then gravity and drag)."""
    y = best = 0.0
    v = v0
    while v > 0:
        y += v
        best = max(best, y)
        v = (v - 0.08) * 0.98
    return best


@test("High Jump")
def _():
    base = jump_peak()
    only("highjump")
    reset()
    two = jump_peak()
    setting("highjump", "multiplier", "3")
    reset()
    three = jump_peak()
    only()
    reset()
    back = jump_peak()
    want2, want3 = jump_height(0.84), jump_height(1.26)
    ok = abs(base - 1.25) < 0.05 and abs(two - want2) < want2 * 0.1 and abs(three - want3) < want3 * 0.1 and abs(back - base) < 0.05
    return ok, f"jump {base:.2f} blocks high; Multiplier 2: {two:.2f} (a 0.84 jump: {want2:.2f}), 3: {three:.2f} ({want3:.2f}); off again {back:.2f}"


def jump_length(*keys):
    """How far one jump from walking (or with `keys`) carries you across, takeoff to landing."""
    reset(z=-14.5)
    for k in ("w", *keys):
        ctl("key", k, "down")
    wait(12)
    start = pos()
    ctl("key", "space", "down")
    wait(2)
    ctl("key", "space", "up")
    landed = None
    for _ in range(30):
        wait(1)
        s = state()["player"]
        if s["on_ground"]:
            landed = s["position"]
            break
    for k in ("w", *keys):
        ctl("key", k, "up")
    return flat(start, landed) if landed else 0.0


@test("Long Jump")
def _():
    base = jump_length()
    only("longjump")
    mine = jump_length()
    only()
    back = jump_length()
    return mine > base * 1.3 and abs(back - base) < 0.3, f"one walking jump covers {base:.2f} blocks, {mine:.2f} with Long Jump, {back:.2f} off again"


def air_jump():
    """Blocks gained over 4 ticks by a jump 8 ticks into a fall."""
    cmd(f"tp @s {X0 + 0.5} {Y + 12} {Z0 + 0.5}")
    wait(8)
    y0 = pos()[1]
    ctl("key", "space", "down")
    wait(1)
    ctl("key", "space", "up")
    wait(4)
    return pos()[1] - y0


@test("Air Jump")
def _():
    only("airjump")
    mine = air_jump()
    only()
    wait(40)
    plain = air_jump()
    return mine > 0 and plain < -0.5, f"a jump in mid-air: {mine:+.2f} blocks in 4 ticks with Air Jump, {plain:+.2f} off"


@test("Safe Walk")
def _():
    reset(z=15.5)
    only("safewalk")
    walk(40)
    p = pos()
    only()
    reset(z=15.5)
    walk(20)
    q = pos()
    # The platform ends at z 17; sneaking you may lean 0.3 past an edge.
    kept = p[1] > Y + 0.99 and p[2] - Z0 < 17.35
    return kept and q[1] < Y, f"walking at the edge: stopped at z {p[2]-Z0:.2f}, y {p[1]-Y:+.2f}; off: fell to y {q[1]-Y:+.1f}"


def parkour_run(gap, sprint=False):
    """Walks (or sprints, Ctrl held) at a gap `gap` blocks wide in the platform, its near edge at z +4, a floor 3
    blocks down. How far past the edge you were on your last tick on the ground (None: a tick went unseen there, or no
    jump), and whether you landed beyond the gap."""
    reset(z=-6.5)
    edge = Z0 + 4
    cmd(f"fill {X0-2} {Y} {edge} {X0+2} {Y} {edge+8} minecraft:stone", f"fill {X0-2} {Y-3} {edge} {X0+2} {Y-3} {edge+gap-1} minecraft:stone",
        f"fill {X0-2} {Y-2} {edge} {X0+2} {Y} {edge+gap-1} minecraft:air")
    wait(5)
    ctl("key", "w", "down")
    if sprint:
        ctl("key", "ctrl", "down")
    last, takeoff, landed = None, None, False
    for _ in range(80):
        st = state()
        p, tick = st["player"], st["ticks"]
        z, y = p["position"][2], p["position"][1]
        if p["on_ground"]:
            if takeoff is not None:
                landed = y > Y + 0.99 and z > edge + gap
                break
            last = (tick, z)
        elif takeoff is None and last and last[1] > edge - 1:
            takeoff = round(last[1] - edge, 3) if tick - last[0] == 1 else None
            if takeoff is None:
                break
        if y < Y - 1:
            break
        wait(1)
    ctl("key", "ctrl", "up")
    ctl("key", "w", "up")
    wait(3)
    return takeoff, landed


@test("Parkour")
def _():
    # The jump comes on the last tick you stand on the block: walking or sprinting, your middle 0 to 0.3 past its edge
    # (half your width).
    # Then the widest gap cleared walking, sprinting and sprinting with Speed (its default and 7 blocks/s), and Jump
    # when Early's take-off.
    def widest(sprint, gaps):
        best, takeoffs = 0, []
        for gap in gaps:
            t, landed = parkour_run(gap, sprint)
            takeoffs.append(t)
            if landed:
                best = gap
        return best, takeoffs

    plain = parkour_run(1)
    only("parkour")
    walk_gap, walk_off = widest(False, (1, 2, 3))
    sprint_gap, sprint_off = widest(True, (2, 3, 4))
    only("parkour", "speed")
    speed_gap, speed_off = widest(True, (3, 4))
    # Speed's default (5.6 blocks/s, sprinting's own) holds you to it in the air too, which a sprint jump outruns.
    setting("speed", "vanilla speed", "7")
    fast_gap, fast_off = widest(True, (4, 5))
    only("parkour")
    setting("parkour", "jump when", "Early")
    early = parkour_run(1)
    setting("parkour", "jump when", "Last tick")
    setting("parkour", "only while sprinting", "true")
    walked, sprinted = parkour_run(1), parkour_run(1, True)
    only()
    # Faster than 0.3 blocks a tick, the last tick on the block can leave your middle short of the edge.
    last_tick = (all(t is not None and 0 < t < 0.3 for t in walk_off + sprint_off)
                 and all(t is not None and -0.3 < t < 0.3 for t in speed_off + fast_off))
    ok = (not plain[1] and last_tick and walk_gap >= 1 and sprint_gap == 3 and speed_gap >= 3 and fast_gap == 4
          and early[0] is not None and early[0] < 0 and not walked[1] and sprinted[1])
    return ok, (f"off: fell off {plain[0]} past the edge, landed {plain[1]}; past the edge at take-off walking {walk_off}, sprinting {sprint_off}, with "
                f"Speed {speed_off}, at 7 b/s {fast_off}; widest gap cleared walking {walk_gap}, sprinting {sprint_gap}, with "
                f"Speed {speed_gap}, at 7 b/s {fast_gap}; "
                f"Early takes off {early[0]} past the edge; Only while sprinting: a 1-block gap walking cleared {walked[1]}, "
                f"sprinting {sprinted[1]}")


@test("Anchor")
def _():
    hx, hz = X0 + 5, Z0
    cmd(f"setblock {hx} {Y} {hz} minecraft:air", f"setblock {hx} {Y-1} {hz} minecraft:obsidian")
    cmd(*(f"setblock {hx+dx} {Y} {hz+dz} minecraft:obsidian" for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1))))

    def over_hole():
        cmd(f"tp @s {X0+5.1} {Y+6} {Z0+0.15}")
        wait(60)
        return pos()

    only("anchor")
    p = over_hole()
    only()
    q = over_hole()
    into = abs(p[0] - (hx + 0.5)) < 0.3 and abs(p[2] - (hz + 0.5)) < 0.3 and p[1] < Y + 0.1
    return into and q[1] > Y + 0.9, f"from above a hole's edge: landed {p[0]-hx-0.5:+.2f},{p[2]-hz-0.5:+.2f} from its middle, {p[1]-Y:+.2f} high; off: landed beside it at {q[1]-Y:+.2f}"


def knocked():
    """Blocks a husk's hit (/damage ... by it) pushes you across, and how high."""
    reset()
    cmd("difficulty easy", f"summon husk {X0 + 2.5} {Y + 1} {Z0 + 0.5} {HUSK}")
    wait(10)
    p0 = pos()
    cmd("damage @s 1 minecraft:mob_attack by @e[type=husk,limit=1]")
    top = p0[1]
    for _ in range(10):
        wait(1)
        top = max(top, pos()[1])
    wait(10)
    return flat(p0, pos()), top - p0[1]


@test("Velocity (knockback)")
def _():
    plain = knocked()
    only("velocity")
    none = knocked()
    setting("velocity", "knockback horizontal", "0.5")
    setting("velocity", "knockback vertical", "0.5")
    half = knocked()
    setting("velocity", "knockback", "false")
    full = knocked()
    only()
    back = knocked()
    ok = (plain[0] > 0.5 and none[0] < 0.05 and none[1] < 0.05 and 0.3 < half[0] / plain[0] < 0.7 and abs(full[0] - plain[0]) < 0.2
          and abs(back[0] - plain[0]) < 0.2)
    return ok, (f"a husk's hit pushes you {plain[0]:.2f} blocks ({plain[1]:.2f} up); Velocity at 0: {none[0]:.2f} ({none[1]:.2f}); "
                f"at 0.5: {half[0]:.2f} ({half[1]:.2f}); Knockback off: {full[0]:.2f}; off again {back[0]:.2f}")


def drift(ticks=20):
    """Blocks you drift across in `ticks` with nothing held."""
    a = pos()
    wait(ticks)
    return flat(a, pos())


@test("Velocity (liquids, entities, sinking)")
def _():
    # A channel of flowing water one block deep, its source at the far end.
    def in_stream():
        reset()
        cmd(f"fill {X0-1} {Y} {Z0-6} {X0+1} {Y} {Z0+6} minecraft:air", f"fill {X0-1} {Y-1} {Z0-6} {X0+1} {Y-1} {Z0+6} minecraft:stone",
            f"fill {X0-1} {Y} {Z0+6} {X0+1} {Y} {Z0+6} minecraft:water", f"tp @s {X0+0.5} {Y} {Z0+0.5}")
        wait(30)
        return drift()

    stream = in_stream()
    only("velocity")
    held = in_stream()
    setting("velocity", "liquids horizontal", "0.5")
    half = in_stream()
    only()
    cmd(f"fill {X0-1} {Y-1} {Z0-6} {X0+1} {Y} {Z0+6} minecraft:air", f"fill {X0-1} {Y} {Z0-6} {X0+1} {Y} {Z0+6} minecraft:stone")

    # A pig standing half inside you.
    def shoved():
        reset()
        cmd(f"summon pig {X0 + 0.7} {Y + 1} {Z0 + 0.5} {NOAI}")
        return drift(10)

    pushed = shoved()
    only("velocity")
    stood = shoved()
    only()

    # Deep water, nothing held.
    def sunk():
        reset()
        cmd(f"fill {X0-2} {Y+1} {Z0-2} {X0+2} {Y+8} {Z0+2} minecraft:water", f"tp @s {X0+0.5} {Y+5} {Z0+0.5}")
        wait(10)
        y0 = pos()[1]
        wait(20)
        return y0 - pos()[1]

    sinks = sunk()
    only("velocity")
    setting("velocity", "sinking", "true")
    level = sunk()
    only()
    ok = stream > 0.15 and held < 0.05 and 0.3 < half / stream < 0.7 and pushed > 0.1 and stood < 0.01 and sinks > 0.2 and abs(level) < 0.05
    return ok, (f"flowing water carries you {stream:.2f} blocks in 1 s, {held:.2f} with Velocity, {half:.2f} at Liquids horizontal 0.5; "
                f"a pig inside you shoves you {pushed:.2f} in 0.5 s, {stood:.2f} with Velocity; in deep water you sink {sinks:.2f} in 1 s, "
                f"{level:+.2f} with Sinking")


@test("Timer")
def _():
    def rate_now():
        t0, c0 = state()["ticks"], time.time()
        time.sleep(3)
        return (state()["ticks"] - t0) / (time.time() - c0)

    base = rate_now()
    only("timer")
    two = rate_now()
    setting("timer", "multiplier", "3")
    three = rate_now()
    only()
    back = rate_now()
    return abs(two / base - 2) < 0.2 and abs(three / base - 3) < 0.3 and abs(back - base) < 1.5, f"{base:.1f} ticks/s; Multiplier 2: {two:.1f}, 3: {three:.1f}; off again {back:.1f}"

# ---- screens, menus and rendering

def brightness(name):
    path = f"/tmp/{name}.png"
    ctl("screenshot", path)
    time.sleep(2)
    out = subprocess.run(["identify", "-format", "%[fx:mean]", path], capture_output=True, text=True, timeout=60).stdout
    return float(out)


@test("No Render (blindness)")
def _():
    reset("creative")
    cmd("effect give @s minecraft:blindness 90 0 true")
    wait(40)
    dark = brightness("nr_blind")
    only("norender")
    wait(30)
    clear = brightness("nr_clear")
    setting("norender", "hide blindness", "false")
    wait(30)
    kept = brightness("nr_kept")
    only()
    wait(30)
    back = brightness("nr_back")
    ok = clear > dark * 1.5 and kept < clear * 0.7 and back < clear * 0.7
    return ok, f"brightness {dark:.2f} blinded, {clear:.2f} with No Render, {kept:.2f} with Hide blindness off, {back:.2f} with it off"


@test("No Render (water)")
def _():
    reset("creative")
    cmd(f"fill {X0-2} {Y+1} {Z0-2} {X0+2} {Y+6} {Z0+2} minecraft:water", f"tp @s {X0+0.5} {Y+3} {Z0+0.5}")
    wait(40)
    wet = brightness("nr_wet")
    only("norender")
    wait(30)
    dry = brightness("nr_dry")
    only()
    wait(30)
    back = brightness("nr_wet_back")
    return dry > wet * 1.2 and back < dry * 0.9, f"brightness {wet:.2f} in water, {dry:.2f} with No Render, {back:.2f} with it off"


def inventory_brightness(name):
    ctl("open-inventory")
    wait(10)
    value = brightness(name)
    ctl("close")
    return value


@test("Menu dim (inventory)")
def _():
    ctl("look", "0", "10")
    setting("clickgui", "menu dim", "true")
    dim = inventory_brightness("md_dark")
    setting("clickgui", "menu dim", "false")
    clear = inventory_brightness("md_clear")
    return clear > dim * 1.3, f"brightness {dim:.2f} behind the inventory with Menu dim on, {clear:.2f} off"


def floor(name):
    """The mean colour of a patch of the platform floor clear of the pause menu's buttons and the chat."""
    from PIL import Image, ImageStat
    path = f"/tmp/{name}.png"
    ctl("screenshot", path)
    time.sleep(2)
    return sum(ImageStat.Stat(Image.open(path).convert("RGB").crop((40, 150, 300, 320))).mean) / 3


@test("Menu dim (pause menu)")
def _():
    reset("creative")
    ctl("look", "0", "50")
    wait(5)
    plain = floor("pm_plain")
    setting("clickgui", "menu dim", "true")
    ctl("pause")
    wait(10)
    dim = floor("pm_dim")
    ctl("resume")
    wait(5)
    setting("clickgui", "menu dim", "false")
    ctl("pause")
    wait(10)
    clear = floor("pm_clear")
    ctl("resume")
    return dim < plain * 0.85 and abs(clear - plain) < plain * 0.03, f"floor {plain:.0f}, {dim:.0f} behind the pause menu with Menu dim on, {clear:.0f} off"


@test("Menu dim (GUI scales)")
def _():
    scale = ctl("options-get", "guiScale")["options"]["guiScale"]["value"]
    notes = []
    ok = True
    for n in (2, 4):
        ctl("options-set", "guiScale", str(n))
        reset("survival")
        ctl("look", "0", "10")
        setting("clickgui", "menu dim", "true")
        dim = inventory_brightness("md_scale_dark")
        setting("clickgui", "menu dim", "false")
        clear = inventory_brightness("md_scale_clear")
        ok = ok and clear > dim * 1.3
        notes.append(f"scale {n}: {dim:.2f} -> {clear:.2f}")
    ctl("options-set", "guiScale", str(scale))
    return ok, ", ".join(notes)


def walked_in(screen_open, screen_close, key="w", ticks=30):
    start = pos()
    screen_open()
    wait(6)
    ctl("key", key, "down")
    wait(ticks)
    ctl("key", key, "up")
    end = pos()
    screen_close()
    wait(6)
    return flat(start, end)


@test("GUI Move")
def _():
    reset(z=-14.5)
    only("guimove")
    ctl("open-inventory")
    wait(6)
    speed = rate()
    ctl("close")
    only()
    reset(z=-14.5)
    ctl("open-inventory")
    wait(6)
    plain = rate()
    ctl("close")
    return 4.0 < speed < 4.6 and plain < 0.05, f"walks {speed:.2f} blocks/s with the inventory open (the game's walking speed), {plain:.2f} off"


@test("GUI Move (pause menu)")
def _():
    only("guimove")
    ctl("pause")
    wait(4)
    stopped = state().get("paused")
    ctl("resume")
    if stopped:
        return None, "single player stops the world behind the pause menu: nothing can move there"
    wait(4)
    all_menus = walked_in(lambda: ctl("pause"), lambda: ctl("resume"))
    setting("guimove", "screens", "Inventory")
    reset()
    inventory_only = walked_in(lambda: ctl("pause"), lambda: ctl("resume"))
    return all_menus > 2.0 and inventory_only < 0.05, f"walked {all_menus:.1f} blocks in the pause menu, {inventory_only:.1f} with Screens on Inventory"


@test("GUI Move (not in chat)")
def _():
    only("guimove")
    moved = walked_in(lambda: ctl("key", "t", "tap"), lambda: ctl("key", "esc", "tap"))
    return moved < 0.05, f"walked {moved:.2f} blocks with the chat open"


def walk_look(menu):
    """Walking forward from standing (a second, then a second jumping), with the inventory open (menu) or not: per
    frame, the view bob, the own model's limb swing speed, and the walk distance a tick (the camera's and hand's bob
    follow it); and Bot1's view of Tester's limb swing speed. The probe's "walk" lines."""
    cmd(f"fill {X0-2} {Y} {Z0-15} {X0+2} {Y} {Z0+15} minecraft:stone", f"fill {X0-2} {Y+1} {Z0-15} {X0+2} {Y+3} {Z0+15} minecraft:air")
    reset(z=-14.5)
    wait(20)
    if menu:
        ctl("open-inventory")
        wait(6)
    paths = [probe_path()] + ([os.path.join(os.path.dirname(CTL2), "session2")] if CTL2 else [])
    if CTL2:
        with open(paths[1]) as f:
            paths[1] = os.path.join(f.read().strip(), "probe.log")
    marks = [os.path.getsize(p) if os.path.exists(p) else 0 for p in paths]
    ctl("key", "w", "down")
    wait(20)
    ctl("key", "space", "down")
    wait(20)
    ctl("key", "space", "up")
    ctl("key", "w", "up")
    wait(5)
    if menu:
        ctl("close")
    rows = []
    for path, mark in zip(paths, marks):
        with open(path) as f:
            f.seek(mark)
            rows.append([line.split() for line in f.read().split("\n")[:-1] if line.startswith("walk ")])
    own = rows[0]
    mean = lambda xs: sum(xs) / max(len(xs), 1)
    ticks = max(int(own[-1][1]) - int(own[0][1]), 1)
    seen = [float(w[w.index(p) + 1]) for w in (rows[1] if CTL2 else []) for p in w if p.startswith("p")]
    return {"bob": mean([float(w[3]) for w in own]), "walk": abs(float(own[-1][4]) - float(own[0][4])) / ticks,
            "limbs": mean([float(w[5]) for w in own if len(w) > 5]), "seen": mean(seen) if seen else None}


@test("GUI Move walks like walking")
def _():
    # Walking with the inventory open looks as walking without it, from the first step and through jumps: the same
    # view bob and walk distance (first person), the same limb swing (third person), and the same as Bot1 sees it.
    if not probe_path():
        return None, "needs the probe"
    only("guimove")
    plain, menu = walk_look(False), walk_look(True)
    only()
    near = lambda a, b: a is None or b is None or abs(a - b) <= 0.03 * max(abs(a), abs(b), 1e-6)
    ok = all(near(plain[k], menu[k]) for k in plain)
    show = lambda d: ", ".join(f"{k} {v:.4f}" if v is not None else f"{k} -" for k, v in d.items())
    return ok, f"without the menu: {show(plain)}; with it: {show(menu)}"


@test("GUI Move (with Flight)")
def _():
    reset(z=-14.5, y=Y + 3)
    only("guimove", "flight")
    ctl("open-inventory")
    wait(6)
    speed = rate(ticks=10, warm=4)
    ctl("close")
    setting("flight", "mode", "Velocity")
    reset(z=-14.5, y=Y + 3)
    ctl("open-inventory")
    wait(6)
    velocity = rate(ticks=10, warm=4)
    ctl("close")
    return abs(speed - 21.8) < 2 and abs(velocity - 20) < 2, f"flew {speed:.1f} blocks/s with the inventory open (Flight's Abilities at Speed 0.1), {velocity:.1f} in Velocity mode"


@test("GUI Move (sprinting)")
def _():
    def server_sprinting():
        """Whether the server has you sprinting (an inline predicate); None where the server cannot say (Horizon's
        server knows no inline predicates)."""
        cmd("execute if predicate {type:'minecraft:entity_properties',entity:'this',predicate:{flags:{is_sprinting:true}}}")
        wait(1)
        said = chat_first()
        return True if "passed" in said else False if "failed" in said else None

    def in_inventory(how):
        """Blocks a second walking forward with the inventory open after sprinting in: with Ctrl held throughout, already
        sprinting (Ctrl let go first, as toggle sprint is: a screen lets go of its key), or by the Sprint module. Then
        whether the game and the server still have you sprinting."""
        reset(z=-14.5)
        ctl("key", "w", "down")
        if how != "module":
            ctl("key", "ctrl", "down")
        wait(8)
        if how == "already":
            ctl("key", "ctrl", "up")
            wait(2)
        ctl("open-inventory")
        wait(8)
        a = state()
        wait(20)
        b = state()
        client = b["player"].get("sprinting")
        server = server_sprinting()
        ctl("key", "ctrl", "up")
        ctl("key", "w", "up")
        ctl("close")
        wait(4)
        pa, pb = a["player"]["position"], b["player"]["position"]
        return flat(pa, pb) / max(b["ticks"] - a["ticks"], 1) * 20, client, server

    only("guimove")
    ctrl = in_inventory("ctrl")
    already = in_inventory("already")
    only("guimove", "sprint")
    module = in_inventory("module")
    only("guimove")
    setting("guimove", "sprint", "false")
    walking = in_inventory("already")
    only()
    runs = (ctrl, already, module)
    ok = all(5.3 < r < 5.9 and c and s is not False for r, c, s in runs) and 4.0 < walking[0] < 4.6 and not walking[1]
    return ok, ("blocks/s with the inventory open (sprinting / the server says so, None: it cannot say): " +
                ", ".join(f"{name} {r:.2f} ({c}/{s})" for name, (r, c, s) in zip(("Ctrl held", "already sprinting", "Sprint module"), runs)) +
                f"; GUI Move's Sprint off: {walking[0]:.2f} ({walking[1]}/{walking[2]})")


@test("Search + Enter")
def _():
    say(".menu timer")
    wait(6)
    ctl("key", "enter", "tap")
    wait(4)
    t0, c0 = ctl("status")["ticks"], time.time()
    time.sleep(2)
    t1, c1 = ctl("status")["ticks"], time.time()
    rate_now = (t1 - t0) / (c1 - c0)
    for _ in range(2):
        ctl("key", "esc", "tap")
        time.sleep(1)
    if state().get("screen") == "paused":
        ctl("key", "esc", "tap")
    on.add("timer")
    return rate_now > 30, f"{rate_now:.0f} ticks a second after Enter (20 is normal, Timer doubles it)"


@test("Panic")
def _():
    # .panic turns everything off and hides Sunset on screen: nothing said, no HUD; .panic again brings it back
    # (everything still off). Sunset stays listed in Options > Mods throughout.
    only("sprint", "fastplace", "fullbright")
    shown = watermark()
    mark = log_mark()
    say(".panic")
    wait(6)
    said = sunset_said(mark)
    sprinting = state()["player"].get("sprinting")
    on.clear()
    hidden = watermark()
    mark = log_mark()
    say(".panic")
    wait(6)
    back_said = sunset_said(mark)
    # Panic switched the HUD module off too: put it back as it starts.
    say(".reset hud")
    back = watermark()
    ok = not sprinting and shown and not said and not hidden and back and back_said
    return ok, (f"HUD before {shown}; after .panic Sunset says {said}, sprinting: {sprinting}, HUD {hidden}; "
                f"after .panic again it says {back_said}, HUD {back}")


@test("Profiles")
def _():
    def said(line):
        mark = log_mark()
        say(line)
        wait(4)
        return sunset_said(mark)

    def range_shown():
        return next((c for c in said(".settings killaura") if c.startswith("Range:")), "")

    setting("killaura", "range", "3")
    say(".profile save verify")
    setting("killaura", "range", "5")
    say(".profile load verify")
    loaded = range_shown()
    # The settings before the load come back from the backup.
    say(".profile load before-load")
    undone = range_shown()
    # Export, then import the exported file under another name; a broken file and a bad name are refused.
    exported = said(".profile export verify")
    found = [os.path.join(r, f) for r, _, fs in os.walk(os.path.join(session_dir(), "home")) for f in fs if f == "verify.profile" and "exports" in r]
    imported = refused = bad_name = []
    if found:
        imports = os.path.join(os.path.dirname(os.path.dirname(found[0])), "imports")
        with open(found[0]) as f, open(os.path.join(imports, "shared.profile"), "w") as g:
            g.write(f.read())
        with open(os.path.join(imports, "broken.profile"), "w") as g:
            g.write("sunset-profile=1\nkillaura.range=lots\n")
        imported = said(".profile import shared")
        refused = said(".profile import broken")
    bad_name = said(".profile save ../evil")
    listed = said(".profile list")
    for n in ("verify", "shared"):
        say(f".profile delete {n}")
    ok = (loaded == "Range: 3" and undone == "Range: 5" and any("Imported profile shared" in l for l in imported)
          and any("killaura.range" in l for l in refused) and any("name is" in l for l in bad_name) and any("shared" in l for l in listed))
    return ok, (f"loaded: {loaded!r}, after .profile load before-load: {undone!r}; export: {exported[:1]}; import: {imported[:1]}; "
                f"broken file: {refused[:1]}; bad name: {bad_name[:1]}; list: {listed[:1]}")


def gui_size():
    """The GUI's size in its own pixels: the window over the game's GUI scale (auto: the largest that keeps 320x240)."""
    d = ctl("status")["display"]
    w, h = d["width"], d["height"]
    scale = int(ctl("options-get").get("guiScale", {}).get("value", 0) or 0)
    if scale <= 0:
        scale = max(1, min(w // 320, h // 240))
    return w / scale, h / scale


def menu_row(panel, row):
    """The middle of a row of a panel (Combat 0, Movement 1, Player 2...) as the menu first lays them out at the
    defaults (Comfortable spacing): row 0 is the header, then rows of 14."""
    w, _ = gui_size()
    width = min(100, (w - 4 - 3 * 4) // 5)
    return 2 + panel * (width + 3) + 30, 32 + 16 + 14 * (row - 1) + 7 if row else 32 + 8


def editor_buttons():
    """The middle of the text editor's Apply and Cancel buttons, in GUI pixels (as runtime.rs lays them out)."""
    w, h = gui_size()
    width = min(w - 8, 320)
    x, y = (w - width) / 2, max((h - 76) / 2, 0)
    button = min(max((width - 18) / 2, 1), 56)
    return (x + 6 + button / 2, y + 54 + 7.5), (x + 12 + button * 1.5, y + 54 + 7.5)


def picker_row(n):
    """The middle of row n of the list picker, in GUI pixels (as picker.rs lays it out)."""
    w, h = gui_size()
    width, height = min(max(w - 8, 1), 300), min(max(h - 16, 1), 280)
    x, y = (w - width) / 2, max((h - height) / 2, 0)
    return x + 40, y + 42 + 12 * n + 6


def open_picker(search, shot=None):
    """The menu at Xray, its settings in the panel, a click on its Blocks row (the picker opens), `search` typed; a
    screenshot to `shot` if given."""
    say(".menu xray")
    wait(5)
    ctl("click-at", *map(str, menu_row(3, 1)), "right")
    wait(3)
    ctl("click-at", *map(str, menu_row(3, 4)))
    wait(3)
    for k in search:
        ctl("key", k, "tap")
    wait(3)
    if shot:
        ctl("screenshot", shot)
        time.sleep(2)


def close_menu():
    for _ in range(2):
        ctl("key", "esc", "tap")
        wait(3)
        if state().get("screen") == "paused":
            ctl("resume")
            wait(3)


@test("F1 with the menu open")
def _():
    # F1 leaves the HUD (and the menu drawn with it) shown while the menu is open; closed, F1 hides the HUD again.
    ctl("look", "0", "0")
    wait(5)
    shut = brightness("f1_shut")
    say(".menu")
    wait(5)
    shown = brightness("f1_open")
    ctl("key", "f1", "tap")
    wait(5)
    after = brightness("f1_after")
    close_menu()
    ctl("key", "f1", "tap")
    wait(5)
    hidden = brightness("f1_hidden")
    ctl("key", "f1", "tap")
    wait(5)
    return abs(after - shown) < abs(after - shut) / 10 and abs(hidden - shut) > 0.001, (
        f"mean brightness: menu shut {shut:.4f}, open {shown:.4f}, after F1 {after:.4f}; F1 with it shut {hidden:.4f}")


@test("List picker")
def _():
    # Xray's Blocks chosen in the picker: coal ore listed first, "diamond" typed, the first block it finds clicked in,
    # coal ore clicked out, then Esc; and .set still sets the list.
    def blocks():
        mark = log_mark()
        say(".settings xray")
        wait(4)
        return next((c for c in sunset_said(mark) if c.startswith("Blocks:")), "")

    setting("clickgui", "settings", "Inline")
    setting("xray", "list", "Advanced")
    setting("xray", "blocks", "coal_ore")
    open_picker("diamond", "/tmp/picker.png")
    ctl("click-at", *map(str, picker_row(1)))
    wait(2)
    ctl("click-at", *map(str, picker_row(0)))
    wait(2)
    close_menu()
    picked = blocks()
    setting("xray", "blocks", "iron_ore,gold_ore")
    typed = blocks()
    ok = picked.startswith("Blocks: ") and "diamond" in picked and "coal" not in picked and "," not in picked and typed == "Blocks: iron_ore,gold_ore"
    return ok, f"after clicking a diamond block in and coal ore out: {picked!r}; after .set: {typed!r}"


@test("Xray Simple list")
def _():
    # Simple: the short list of ores and valuables, a row standing for all its blocks. A click on Diamond (row 0, both
    # chosen) takes both away, on Coal (row 9, half chosen) completes it, on Spawner (row 11) adds both spawners.
    def blocks():
        mark = log_mark()
        say(".settings xray")
        wait(4)
        return next((c for c in sunset_said(mark) if c.startswith("Blocks:")), "")

    setting("clickgui", "settings", "Inline")
    setting("xray", "list", "Simple")
    start = "diamond_ore,deepslate_diamond_ore,coal_ore"
    setting("xray", "blocks", start)
    # A right click on Xray opens or closes its settings in the panel: a second try if the first found them open.
    for _ in range(2):
        open_picker("", "/tmp/picker_simple.png")
        for row in (0, 9, 11):
            ctl("click-at", *map(str, picker_row(row)))
            wait(2)
        close_menu()
        got = blocks()
        if got != f"Blocks: {start}":
            break
    ok = set(got.removeprefix("Blocks: ").split(",")) == {"coal_ore", "deepslate_coal_ore", "spawner", "trial_spawner"}
    return ok, f"Diamond (all chosen), Coal (half) and Spawner (none) clicked: {got!r}"


@test("Xray water, lava and placed blocks")
def _():
    # Xray listing only diamond ore, looking at a stone box: water inside it shows only with Show water, lava only
    # with Show lava. A stone block you place (ctl place) shows with Show placed blocks; one set by a command beside
    # it stays hidden. Counted as the pixels that change between two screenshots with the switch off and on.
    from PIL import Image, ImageChops

    def shot(name):
        # Long enough for the whole view to be meshed again.
        wait(80)
        path = f"/tmp/{name}.png"
        ctl("screenshot", path)
        time.sleep(2)
        return Image.open(path).convert("L")

    def changed(a, b):
        return sum(1 for d in ImageChops.difference(a, b).getdata() if d > 30) / (a.width * a.height)

    reset("creative")
    cmd(f"fill {X0-3} {Y+1} {Z0+3} {X0+3} {Y+5} {Z0+5} minecraft:stone", f"fill {X0-2} {Y+2} {Z0+4} {X0+2} {Y+4} {Z0+4} minecraft:water")
    ctl("look", "0", "0")
    setting("xray", "blocks", "diamond_ore")
    toggle("xray", True)
    dry = shot("xray_water_off")
    setting("xray", "show water", "true")
    wet = shot("xray_water_on")
    cmd(f"fill {X0-2} {Y+2} {Z0+4} {X0+2} {Y+4} {Z0+4} minecraft:lava")
    setting("xray", "show water", "false")
    no_lava = shot("xray_lava_off")
    setting("xray", "show lava", "true")
    lava = shot("xray_lava_on")
    toggle("xray", False)
    cmd(f"fill {X0-3} {Y+1} {Z0+3} {X0+3} {Y+5} {Z0+5} minecraft:air", "item replace entity @s hotbar.0 with minecraft:stone 64")
    setting("xray", "show placed blocks", "true")
    ctl("select", "1")
    for dx in (-1, 0, 1):
        for dy in (1, 2):
            ctl("place", str(X0 + dx), str(Y + dy), str(Z0 + 3))
            wait(2)
    cmd(f"fill {X0+3} {Y+3} {Z0+6} {X0+5} {Y+4} {Z0+6} minecraft:stone")
    wait(20)
    stone = lambda x, y, z: ctl("block", str(x), str(y), str(z)).get("block") == "minecraft:stone"
    blocks = sum(stone(X0 + dx, Y + dy, Z0 + 3) + stone(X0 + 4 + dx, Y + 2 + dy, Z0 + 6) for dx in (-1, 0, 1) for dy in (1, 2))
    toggle("xray", True)
    shown = shot("xray_placed_on")
    setting("xray", "show placed blocks", "false")
    hidden = shot("xray_placed_off")
    w, l, p = changed(dry, wet), changed(no_lava, lava), changed(hidden, shown)
    # The command's blocks are east and up, on the left of a view south above the chat: that part must not change.
    box = (0, hidden.height * 15 // 100, hidden.width // 3, hidden.height * 55 // 100)
    left = changed(hidden.crop(box), shown.crop(box))
    toggle("xray", False)
    cmd(f"fill {X0-3} {Y+1} {Z0+3} {X0+5} {Y+5} {Z0+6} minecraft:air")
    ok = w > 0.03 and l > 0.03 and p > 0.01 and left < 0.02 and blocks == 12
    return ok, (f"screen changed by Show water {w:.1%}, Show lava {l:.1%}, Show placed blocks {p:.1%} "
                f"(where the command's blocks are: {left:.2%}); {blocks}/12 stone blocks there")


@test("Text setting editor")
def _():
    def messages():
        mark = log_mark()
        say(".settings spam")
        wait(4)
        return next((c for c in sunset_said(mark) if c.startswith("Messages:")), "")

    def esc():
        """Esc as horizon-ctl presses it, which the game's own Esc gets too (the real keyboard's doesn't): resumed."""
        ctl("key", "esc", "tap")
        wait(3)
        if state().get("screen") == "paused":
            ctl("resume")
            wait(3)

    def edit(keys, finish, expand=False):
        """Opens the menu at Spam, clicks its Messages row, types (clear: Backspace over all of it; horizon-ctl's Ctrl
        never reaches the window's modifiers, so Ctrl+A is the real keyboard's) and finishes with the Apply or Cancel
        button or the Enter or Esc key; then closes the menu and reads the setting."""
        say(".menu spam")
        wait(5)
        if expand:
            ctl("click-at", *map(str, menu_row(2, 1)), "right")
            wait(3)
        ctl("click-at", *map(str, menu_row(2, 3)))
        wait(3)
        for k in keys:
            for _ in range(40 if k == "clear" else 1):
                ctl("key", "backspace" if k == "clear" else k, "tap")
        wait(2)
        apply, cancel = editor_buttons()
        if finish in ("apply", "cancel"):
            ctl("click-at", *map(str, apply if finish == "apply" else cancel))
        elif finish == "esc":
            esc()
        else:
            ctl("key", finish, "tap")
        wait(3)
        # Esc closes the editor first, the menu after.
        esc()
        return messages()

    # The settings open in the panel (not their own window), where menu_row finds the rows.
    setting("clickgui", "settings", "Inline")
    before = messages()
    applied = edit(["clear", "h", "i"], "apply", expand=True)
    if applied != "Messages: hi":
        # Spam's settings were open already, so the right click closed them: once more.
        applied = edit(["clear", "h", "i"], "apply", expand=True)
    cancelled = edit(["x", "y", "z"], "cancel")
    entered = edit(["backspace", "o", "k"], "enter")
    escaped = edit(["clear", "n", "o"], "esc")
    touched.add("spam")
    ok = (applied, cancelled, entered, escaped) == ("Messages: hi", "Messages: hi", "Messages: hok", "Messages: hok")
    return ok, (f"{before!r}; cleared, hi, Apply: {applied!r}; xyz, Cancel: {cancelled!r}; Backspace, ok, Enter: {entered!r}; "
                f"cleared, no, Esc: {escaped!r}")


def watermark():
    """Gold pixels where the HUD writes its name, top left: whether the HUD is on."""
    from PIL import Image
    path = "/tmp/sunset-watermark.png"
    ctl("screenshot", path)
    time.sleep(1)
    im = Image.open(path).convert("RGB")
    return sum(1 for r, g, b in im.crop((0, 0, im.width // 5, im.height // 14)).getdata() if r > 220 and 130 < g < 210 and b < 90) > 20


@test("Binds (mouse buttons, Hold)")
def _():
    def binds():
        mark = log_mark()
        say(".binds")
        wait(3)
        return [line for line in sunset_said(mark) if line.startswith("HUD:")]

    # A mouse button from the chat, then one chosen in the menu (a right click while its Bind row waits), the settings
    # open in the panel.
    setting("clickgui", "settings", "Inline")
    say(".bind hud mouse4")
    by_name = binds()
    say(".menu hud")
    wait(5)
    ctl("click-at", *map(str, menu_row(3, 1)), "right")
    wait(2)
    ctl("click-at", *map(str, menu_row(3, 2)))
    wait(2)
    ctl("click-at", *map(str, menu_row(3, 2)), "right")
    wait(2)
    ctl("key", "esc", "tap")
    wait(3)
    if state().get("screen") == "paused":
        ctl("resume")
    chosen = binds()
    # A middle button bind toggles the HUD from the world.
    say(".bind hud mouse3")
    wait(5)
    shown = watermark()
    ctl("mouse", "middle", "click")
    wait(5)
    middle_off = not watermark()
    ctl("mouse", "middle", "click")
    wait(5)
    middle_on = watermark()
    # Hold: on only while the key is down, then back as it was (off here).
    say(".bind hud j")
    setting("hud", "bind mode", "Hold")
    say(".t hud")
    wait(5)
    off = not watermark()
    ctl("key", "j", "down")
    wait(5)
    held = watermark()
    ctl("key", "j", "up")
    wait(5)
    let_go = not watermark()
    # Held over a screen: the chat box opening ends the hold.
    ctl("key", "j", "down")
    wait(3)
    ctl("key", "t", "tap")
    wait(3)
    ctl("key", "esc", "tap")
    wait(3)
    ended = not watermark()
    ctl("key", "j", "up")
    say(".t hud")
    ok = (by_name == ["HUD: Mouse Back"] and chosen == ["HUD: Mouse Right"] and shown and middle_off and middle_on
          and off and held and let_go and ended)
    return ok, (f".bind mouse4: {by_name}; chosen with a right click: {chosen}; middle button toggles: {shown}, {middle_off}, "
                f"{middle_on}; Hold: off {off}, on while held {held}, off when let go {let_go}, ended by a screen {ended}")


@test("Click GUI (settings window, typed numbers)")
def _():
    def esc():
        """horizon-ctl's Esc, which the game gets too (its pause menu closes the GUI): resumed."""
        ctl("key", "esc", "tap")
        wait(3)
        if state().get("screen") == "paused":
            ctl("resume")
            wait(3)

    def said(command, prefix):
        mark = log_mark()
        say(command)
        wait(3)
        return [line for line in sunset_said(mark) if line.startswith(prefix)]

    # A module with no settings (Auto Walk) gets a window too, centred 40 down, holding its bind: title, one line of
    # description, the Bind group's name, the Bind row.
    w, _ = gui_size()
    panel = min(100, (w - 4 - 3 * 4) // 5)
    ww = min(max(panel * 1.6, 150), w - 8)
    x = (w - ww) / 2
    bind_row = (x + ww / 2, 40 + 16 + 14 + 14 + 7)
    say(".menu auto walk")
    wait(5)
    ctl("click-at", *map(str, menu_row(1, 1)), "right")
    wait(3)
    ctl("click-at", *map(str, bind_row))
    wait(2)
    ctl("key", "k", "tap")
    wait(2)
    # The close button; then nothing is under the Bind row's place.
    ctl("click-at", str(x + ww - 8), "48")
    wait(2)
    ctl("click-at", *map(str, bind_row))
    wait(2)
    ctl("key", "m", "tap")
    esc()
    bound = said(".binds", "Auto Walk:")
    # Typed numbers, inline (ClickGUI Settings: Inline): Step's Height, 0.5 to 5 in steps of 0.25 (horizon-ctl has no
    # "." key: a real keyboard types one through the window's text, and the number is rounded to the step as a slider's).
    setting("clickgui", "settings", "Inline")

    def typed(keys, first=False):
        say(".menu step")
        wait(5)
        if first:
            ctl("click-at", *map(str, menu_row(1, 1)), "right")
            wait(2)
        right = 2 + (panel + 3) + panel - 7 - 3
        ctl("click-at", str(right), str(32 + 16 + 28 + 5))
        wait(2)
        for k in keys:
            ctl("key", k, "tap")
        wait(2)
        esc()
        return said(".settings step", "Height:")

    typed_in = typed(["3", "enter"], first=True)
    clamped = typed(["9", "9", "enter"])
    low = typed(["0", "enter"])
    refused = typed(["a", "b", "enter", "esc"])
    cancelled = typed(["1", "esc"])
    touched.add("step")
    ok = (bound == ["Auto Walk: K"] and typed_in == ["Height: 3"] and clamped == ["Height: 5"] and low == ["Height: 0.5"]
          and refused == ["Height: 0.5"] and cancelled == ["Height: 0.5"])
    return ok, (f"bind set in Auto Walk's window: {bound}; typed 3: {typed_in}, 99: {clamped}, 0: {low}, ab: {refused}, "
                f"1 then Esc: {cancelled}")


def is_on(name):
    """Whether Sunset has the module on, from .modules (its on modules end in *)."""
    mark = log_mark()
    say(".modules")
    wait(3)
    return any(f"{name}*" in [x.strip() for x in line.split(":", 1)[-1].split(",")] for line in sunset_said(mark))


def turned_itself_off(key, name):
    """A module that acts once and turns itself off: off now (and so out of the toggles to undo)."""
    off = not is_on(name)
    if off:
        on.discard(key)
    return off


@test("Self Web")
def _():
    def webbed(double=False, floor=None, offhand=False):
        """The webs at feet and head height after one go, and whether it turned itself off."""
        reset()
        feet = Y + 1
        if floor:
            # Stood on a partial block: the feet are in the block above it.
            block, height = floor
            cmd(f"setblock {X0} {Y+1} {Z0} minecraft:{block}", f"tp @s {X0 + 0.5} {Y + 1 + height} {Z0 + 0.5}")
            feet = Y + 2
        if offhand:
            cmd("item replace entity @s weapon.offhand with minecraft:cobweb 8")
        else:
            slot(3, "cobweb", 8)
        setting("selfweb", "double place", "true" if double else "false")
        wait(10)
        only("selfweb")
        wait(10)
        got = (check(f"execute if block {X0} {feet} {Z0} minecraft:cobweb"), check(f"execute if block {X0} {feet+1} {Z0} minecraft:cobweb"))
        off = turned_itself_off("selfweb", "Self Web")
        only()
        cmd(f"fill {X0} {Y+1} {Z0} {X0} {Y+3} {Z0} minecraft:air")
        return got, off

    single, double = webbed(), webbed(double=True)
    slab, soul = webbed(floor=("stone_slab", 0.5)), webbed(floor=("soul_sand", 0.875))
    offhand = webbed(offhand=True)
    reset()
    mark = log_mark()
    only("selfweb")
    wait(5)
    told = sunset_said(mark)
    turned_itself_off("selfweb", "Self Web")
    only()
    ok = (single == ((True, False), True) and double == ((True, True), True) and slab[0][0] and soul[0][0] and offhand[0][0]
          and any("cobweb" in t.lower() for t in told))
    return ok, (f"(feet, head) webbed, off after: {single}; Double place {double}; on a slab {slab}; on soul sand {soul}; "
                f"webs in the offhand {offhand}; with no webs Sunset says {told[-1:]}")


@test("Self Trap")
def _():
    def trapped(slab=False, blocks=True):
        reset()
        feet = Y + 1
        if slab:
            cmd(f"setblock {X0} {Y+1} {Z0} minecraft:stone_slab", f"tp @s {X0 + 0.5} {Y + 1.5} {Z0 + 0.5}")
            feet = Y + 2
        if blocks:
            slot(2, "obsidian", 16)
        wait(10)
        only("selftrap")
        wait(20)
        top = check(f"execute if block {X0} {feet+2} {Z0} minecraft:obsidian")
        off = turned_itself_off("selftrap", "Self Trap")
        only()
        cmd(f"fill {X0-1} {Y+1} {Z0-1} {X0+1} {Y+5} {Z0+1} minecraft:air")
        return top, off

    flat_, slab_, none = trapped(), trapped(slab=True), trapped(blocks=False)
    return flat_ == (True, True) and slab_ == (True, True) and none == (False, True), (
        f"block over your head, off after: {flat_}; stood on a slab {slab_}; with no blocks {none}")


@test("Surround (Toggle on complete)")
def _():
    slot(0, "obsidian", 16)
    setting("surround", "toggle on complete", "true")
    only("surround")
    wait(30)
    four = count(X0 - 1, Y + 1, Z0 - 1, X0 + 1, Y + 1, Z0 + 1, "obsidian")
    off = turned_itself_off("surround", "Surround")
    only()
    return four == 4 and off, f"{four} obsidian round your feet before it turned itself off: {off}"


@test("Auto Trap")
def _():
    if not CTL2:
        return None, "needs a second player"
    cmd("gamemode survival Bot1", f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5}")
    slot(0, "obsidian", 32)
    wait(10)
    only("autotrap")
    wait(60)
    boxed = count(X0 - 1, Y + 1, Z0 + 1, X0 + 1, Y + 3, Z0 + 3, "obsidian")
    off = turned_itself_off("autotrap", "Auto Trap")
    only()
    return boxed == 5 and off, f"{boxed} obsidian round Bot1's head (5 is the Full top); off after: {off}"


@test("Burrow")
def _():
    slot(0, "ender_chest", 4)
    wait(5)
    only("burrow")
    wait(40)
    chest = check(f"execute if block {X0} {Y+1} {Z0} minecraft:ender_chest")
    off = turned_itself_off("burrow", "Burrow")
    only()
    return chest and off, f"ender chest where you stood: {chest}; off after: {off}; you at {[round(v, 2) for v in pos()]}"


@test("Hole Filler")
def _():
    # A one-block hole in obsidian in the floor beside you (Smart off: every hole near you, not only near other players).
    cmd(f"fill {X0+1} {Y-1} {Z0-1} {X0+3} {Y} {Z0+1} minecraft:obsidian", f"setblock {X0+2} {Y} {Z0} minecraft:air")
    slot(0, "obsidian", 16)
    setting("holefiller", "smart", "false")
    only("holefiller")
    wait(20)
    filled = check(f"execute if block {X0+2} {Y} {Z0} minecraft:obsidian")
    only()
    return filled, f"the hole beside you filled: {filled}"


@test("Chest Swap")
def _():
    cmd("item replace entity @s armor.chest with minecraft:iron_chestplate", "item replace entity @s inventory.5 with minecraft:elytra")
    wait(5)
    only("chestswap")
    wait(10)
    elytra = check("execute if items entity @s armor.chest minecraft:elytra")
    off = turned_itself_off("chestswap", "Chest Swap")
    only()
    return elytra and off, f"elytra on after: {elytra}; off after: {off}"


# ---- player and world

def nuker_soak(mode):
    """Nuker on for 60 s in a field (stone, dirt, iron and coal ore, two high) round the player, who strafes left and
    right: six 10 s rounds of five 2 s steps, the field filled again before each step. Blocks broken a round must not
    fall off."""
    reset(mode)
    slot(0, "diamond_pickaxe")
    ctl("key", "1", "tap")
    field = (X0 - 6, Y + 1, Z0 + 1, X0 + 6, Y + 2, Z0 + 15)
    def fill():
        x1, y1, z1, x2, y2, z2 = field
        cmd(f"fill {x1} {y1} {z1} {x2} {y2} {z2} minecraft:stone", f"fill {x1} {y1} {z1+4} {x2} {y1} {z1+6} minecraft:dirt",
            f"fill {x1} {y2} {z1+8} {x2} {y2} {z1+9} minecraft:iron_ore", f"fill {x1} {y1} {z1+11} {x2} {y2} {z1+11} minecraft:coal_ore",
            f"fill {X0} {y1} {Z0+8} {X0} {y2} {Z0+8} minecraft:air")
    def left():
        return sum(ctl("blocks", *map(str, field)).get("counts", {}).values())
    only("nuker")
    rounds = []
    for _ in range(6):
        broken = 0
        for step in range(5):
            cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 8.5} 0 0")
            fill()
            wait(3)
            full = left()
            held_keys(("a" if step % 2 else "d",), 40)
            broken += full - left()
        rounds.append(broken)
    only()
    cmd(f"fill {field[0]} {field[1]} {field[2]} {field[3]} {field[4]} {field[5]} minecraft:air")
    ok = rounds[0] > 8 and min(rounds) >= 0.6 * rounds[0]
    return ok, f"blocks broken each 10 s ({mode}): {rounds}"


@test("Nuker soak (survival, 60 s)")
def _():
    return nuker_soak("survival")


@test("Nuker soak (creative, 60 s)")
def _():
    return nuker_soak("creative")


@test("Nuker (survival)")
def _():
    slot(0, "iron_pickaxe")
    ctl("key", "1", "tap")
    box = (X0 - 1, Y + 1, Z0 + 1, X0 + 1, Y + 2, Z0 + 2)
    cmd(f"fill {' '.join(map(str, box))} minecraft:stone")
    wait(4)
    only("nuker")
    wait(60)
    broken = 12 - count(*box, "stone")
    only()
    cmd(f"fill {' '.join(map(str, box))} minecraft:stone")
    wait(40)
    left = count(*box, "stone")
    return broken >= 6 and left == 12, f"{broken} of 12 stone blocks broken in 3 s with an iron pickaxe, none once it is off"


@test("Nuker (creative shred)")
def _():
    box = (X0 - 3, Y + 1, Z0 - 3, X0 + 3, Y + 4, Z0 + 3)

    def shred(**sets):
        reset("creative")
        cmd(f"fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+4} {Z0+3} minecraft:stone", f"fill {X0-3} {Y+3} {Z0-3} {X0+3} {Y+3} {Z0+3} minecraft:dirt",
            f"fill {X0} {Y+1} {Z0} {X0} {Y+2} {Z0} minecraft:air", f"tp @s {X0+0.5} {Y+1} {Z0+0.5}")
        wait(20)
        for k, v in sets.items():
            setting("nuker", k.replace("_", " "), v)
        only("nuker")
        wait(20)
        only()
        # What the server took in that second, once its answers are all back.
        wait(10)
        left = count(*box, "stone") + count(*box, "dirt")
        floor_ = 289 - count(X0 - 8, Y, Z0 - 8, X0 + 8, Y, Z0 + 8, "stone")
        dirt = 49 - count(*box, "dirt")
        cmd(f"fill {X0-16} {Y} {Z0-16} {X0+16} {Y} {Z0+16} minecraft:stone")
        touched.discard("nuker")
        say(".reset nuker")
        return 194 - left, floor_, dirt

    wide = shred()
    near = shred(range="2")
    slow = shred(max_blocks_per_tick="1")
    every = shred(mode="All")
    dirt_only = shred(list_mode="Whitelist", whitelist="dirt")
    cube = shred(shape="Cube")
    ok = (wide[0] >= 100 and wide[1] == 0 and near[0] < wide[0] * 0.5 and slow[0] <= 21 and every[1] > 0
          and dirt_only[0] == dirt_only[2] > 0 and 0 < cube[0] < wide[0])
    return ok, (f"in 1 s: {wide[0]} of 194 blocks at Range 4 (floor kept, Flatten), {near[0]} at Range 2, {slow[0]} at 1 block a tick, "
                f"{every[1]} floor blocks too in mode All, {dirt_only[0]} (all dirt: {dirt_only[2]}) with only dirt whitelisted, {cube[0]} in the 3x4x3 Cube")


@test("Auto Steal")
def _():
    def chest():
        cmd(f"setblock {X0+1} {Y+1} {Z0} minecraft:chest[facing=west]",
            *(f"item replace block {X0+1} {Y+1} {Z0} container.{i} with minecraft:stone" for i in range(27)))

    chest()
    wait(6)

    def taken(per_tick):
        """Ticks from the first item taken to the last, and how many were taken."""
        setting("autosteal", "items per tick", per_tick)
        only("autosteal")
        ctl("interact", str(X0 + 1), str(Y + 1), str(Z0))
        first = last = None
        got = 0
        for _ in range(60):
            wait(1)
            got = ctl("inventory").get("counts", {}).get("minecraft:stone", 0)
            now = state()["ticks"]
            if got and first is None:
                first = now
            if got == 27:
                last = now
                break
        ctl("close")
        only()
        return (last - first if first is not None and last is not None else 99), got

    slow = taken("1")
    cmd("clear @s", f"setblock {X0+1} {Y+1} {Z0} minecraft:air")
    chest()
    fast = taken("9")
    return slow[1] == 27 and fast[1] == 27 and slow[0] > fast[0] * 2, f"27 stacks taken in {slow[0]} ticks at 1 a tick, {fast[0]} at 9"


@test("Inventory Cleaner")
def _():
    cmd("item replace entity @s inventory.3 with minecraft:rotten_flesh 12", "item replace entity @s inventory.4 with minecraft:bread 4")
    only("inventorycleaner")
    ctl("open-inventory")
    wait(30)
    ctl("close")
    counts = ctl("inventory").get("counts", {})
    gone, kept = "minecraft:rotten_flesh" not in counts, counts.get("minecraft:bread") == 4
    setting("inventorycleaner", "items", "bread")
    ctl("open-inventory")
    wait(30)
    ctl("close")
    bread = "minecraft:bread" not in ctl("inventory").get("counts", {})
    # List mode Blacklist: everything but the listed items goes (the clean() after the test empties the rest).
    cmd("clear @s", "item replace entity @s inventory.3 with minecraft:rotten_flesh 12", "item replace entity @s inventory.4 with minecraft:bread 4")
    setting("inventorycleaner", "list mode", "Blacklist")
    ctl("open-inventory")
    wait(30)
    ctl("close")
    counts = ctl("inventory").get("counts", {})
    black = counts.get("minecraft:bread") == 4 and "minecraft:rotten_flesh" not in counts
    cmd("kill @e[type=item]")
    return gone and kept and bread and black, (f"rotten flesh dropped: {gone}, bread kept: {kept}; bread dropped once listed: {bread}; "
                                               f"Blacklist of bread: only the bread kept: {black}")


@test("Auto Respawn")
def _():
    only("autorespawn")
    cmd("kill @s")
    t0 = state()["ticks"]
    back = None
    for _ in range(60):
        wait(2)
        s = state()
        if s.get("screen") == "playing" and s["player"]["health"] > 0:
            back = s["ticks"] - t0
            break
    return back is not None, f"respawned {back} ticks after dying" if back else "still on the death screen"


@test("Anti AFK")
def _():
    setting("antiafk", "jump", "false")
    setting("antiafk", "spin mode", "Client")

    def spun():
        only("antiafk")
        y0 = ctl("status").get("yaw", 0)
        wait(20)
        y1 = ctl("status").get("yaw", 0)
        only()
        return abs(y1 - y0)

    seven = spun()
    setting("antiafk", "speed", "14")
    fourteen = spun()
    # Speed is degrees a tick.
    return abs(seven - 140) < 20 and abs(fourteen - 280) < 40, f"turned {seven:.0f} degrees in 20 ticks at Speed 7, {fourteen:.0f} at 14"


@test("Anti AFK (defaults: Jump on)")
def _():
    # Jump on froze the game once (issue #3): it has to keep ticking, and jump now and then (1 in 100 ticks).
    only("antiafk")
    t0, c0 = state()["ticks"], time.time()
    time.sleep(5)
    tps = (state()["ticks"] - t0) / (time.time() - c0)
    ys, c0 = [], time.time()
    while time.time() - c0 < 25 and (not ys or max(ys) - min(ys) < 0.5):
        ys.append(pos()[1])
        wait(2)
    only()
    rose = max(ys) - min(ys) if ys else 0
    return tps > 15 and rose >= 0.5, f"{tps:.0f} ticks/s over 5 s with it on; rose {rose:.2f} blocks within {time.time() - c0:.0f} s"


@test("Auto Replenish")
def _():
    def refilled(n):
        reset()
        cmd(f"item replace entity @s hotbar.0 with minecraft:stone {n}", "item replace entity @s inventory.0 with minecraft:stone 64")
        only("autoreplenish")
        wait(40)
        c = held().get("count", 0)
        only()
        return c

    low, high = refilled(3), refilled(12)
    setting("autoreplenish", "threshold", "16")
    high16 = refilled(12)
    return low == 64 and high == 12 and high16 == 64, f"a stack of 3 goes to {low}, of 12 stays {high} (Threshold 8); at Threshold 16 the 12 goes to {high16}"


@test("Client Sort")
def _():
    # Five stacks in the inventory's first row, sorted with Horizon's sort button: Order Alphabet puts them by name,
    # Quantity by how many of each there are (the two dirt stacks merged first).
    def sort(order):
        cmd("clear @s", "item replace entity @s inventory.0 with minecraft:dirt 3", "item replace entity @s inventory.1 with minecraft:diamond 1",
            "item replace entity @s inventory.2 with minecraft:dirt 60", "item replace entity @s inventory.3 with minecraft:apple 5",
            "item replace entity @s inventory.4 with minecraft:stone 10")
        setting("clientsort", "order", order)
        ctl("open-inventory")
        wait(4)
        w, h = gui_size()
        ctl("click-at", str((w - 176) / 2 + 164), str((h - 166) / 2 + 75))
        wait(20)
        main = ctl("inventory")["main"]
        ctl("close")
        return [(s["item"].split(":")[1], s["count"]) for s in main if s.get("item")]

    reset()
    only("clientsort")
    alphabet, quantity = sort("Alphabet"), sort("Quantity")
    only()
    cmd("clear @s")
    ok = alphabet == [("apple", 5), ("diamond", 1), ("dirt", 63), ("stone", 10)] and quantity == [("dirt", 63), ("stone", 10), ("apple", 5), ("diamond", 1)]
    return ok, f"Alphabet: {alphabet}; Quantity: {quantity}"


@test("Shulker Peek")
def _():
    # A red shulker box holding diamonds and gold in the hotbar, hovered in the inventory: with Shulker Peek its grid
    # shows above the tooltip (red frame, the diamonds' and gold's icons); a middle click pins it in the middle of
    # the screen; another click lets it go.
    red = lambda r, g, b: 140 < r < 185 and 25 < g < 55 and 20 < b < 50
    diamond = lambda r, g, b: r < 120 and g > 170 and b > 170
    gold = lambda r, g, b: r > 220 and 170 < g < 235 and b < 90
    reset()
    cmd("time set 6000", "item replace entity @s hotbar.0 with minecraft:red_shulker_box[container=[{slot:0,item:{id:\"minecraft:diamond\",count:5}},"
        "{slot:13,item:{id:\"minecraft:gold_block\",count:64}}]]")

    def look(name):
        # Counted where the grid goes: above and right of the hovered slot (hotbar slot 0 of the inventory).
        from PIL import Image
        ctl("open-inventory")
        wait(4)
        # The pointer onto an empty slot first, so the click on the box hovers it rather than pinning a grid shown
        # for it before.
        ctl("click", "slot-9", "middle")
        wait(3)
        ctl("click", "slot-0", "middle")
        wait(5)
        ctl("screenshot", f"/tmp/{name}.png")
        time.sleep(2)
        img = Image.open(f"/tmp/{name}.png").convert("RGB")
        w, h = gui_size()
        k = img.width / w
        cx, cy = (w - 176) / 2 + 16, (h - 166) / 2 + 150
        part = list(img.crop((int((cx + 8) * k), int((cy - 80) * k), int((cx + 200) * k), int((cy - 8) * k))).getdata())
        return [sum(1 for px in part if f(*px)) for f in (red, diamond, gold)]

    def lit(name):
        # How bright the world is round the inventory (its left side, below the HUD's text): the pinned view dims it.
        from PIL import Image
        ctl("screenshot", f"/tmp/{name}.png")
        time.sleep(2)
        img = Image.open(f"/tmp/{name}.png").convert("L")
        part = list(img.crop((0, img.height // 4, img.width // 8, img.height * 3 // 4)).getdata())
        return sum(part) / len(part)

    off = look("peek_off")
    ctl("close")
    only("shulkerpeek")
    on_ = look("peek_on")
    shown = lit("peek_shown")
    ctl("click", "slot-0", "middle")
    wait(5)
    pinned = lit("peek_pin")
    ctl("click", "slot-0", "middle")
    wait(5)
    let_go = lit("peek_free")
    ctl("close")
    only()
    cmd("item replace entity @s hotbar.0 with minecraft:air")
    items = on_[1] > off[1] + 30 and on_[2] > off[2] + 30
    # Horizon's own server (single player's too) sends a shulker box without its contents, so there is nothing to show.
    horizon = not SERVER or SERVER.endswith(":25599")
    ok = on_[0] > off[0] + 200 and (items or horizon) and pinned < shown * 0.7 and let_go > pinned * 1.4
    return ok, (f"red, diamond and gold pixels hovering the box: off {off}, on {on_}; brightness round it {shown:.0f}, "
                f"pinned {pinned:.0f}, let go {let_go:.0f}" + ("" if items else " (no item icons: Horizon's server sends no contents)"))


@test("Auto Tool")
def _():
    slot(0, "diamond_sword")
    slot(1, "diamond_pickaxe")
    slot(2, "diamond_shovel")
    ctl("key", "1", "tap")
    wait(2)
    only("autotool")

    def mining(block):
        cmd(f"setblock {X0} {Y+1} {Z0+2} minecraft:{block}")
        ctl("look-at-block", str(X0), str(Y + 1), str(Z0 + 2))
        wait(2)
        ctl("mouse", "left", "down")
        wait(3)
        item = held().get("item", "")
        ctl("mouse", "left", "up")
        wait(5)
        return item, held().get("item", "")

    dirt, kept = mining("dirt")
    stone, _ = mining("stone")
    setting("autotool", "switch back", "true")
    ctl("key", "1", "tap")
    _, after = mining("dirt")
    # Blacklisted, the shovel is passed over; Anti break passes over a pickaxe about to break.
    setting("autotool", "blacklist", "diamond_shovel")
    ctl("key", "1", "tap")
    listed, _ = mining("dirt")
    cmd("item replace entity @s hotbar.1 with minecraft:diamond_pickaxe[damage=1550]")
    setting("autotool", "anti break", "true")
    ctl("key", "1", "tap")
    worn, _ = mining("stone")
    only()
    return ("shovel" in dirt and "pickaxe" in stone and "shovel" in kept and "sword" in after and "sword" in listed and "sword" in worn), (
        f"mining dirt: {dirt}, stone: {stone}; let go: {kept} kept, {after} with Switch back; dirt with the shovel blacklisted: {listed}; "
        f"stone with Anti break and a worn pickaxe: {worn}")


@test("Auto Tool (cobweb)")
def _():
    # A cobweb, the pickaxe held and a sword beside it: Auto Tool takes the sword (the game breaks a cobweb fastest
    # with it) and the web goes in a few ticks; without it, the pickaxe has not broken it after 60 ticks.
    reset()
    slot(0, "diamond_sword")
    slot(1, "diamond_pickaxe")

    def mining():
        ctl("key", "2", "tap")
        cmd(f"setblock {X0} {Y+1} {Z0+2} minecraft:cobweb")
        ctl("look-at-block", str(X0), str(Y + 1), str(Z0 + 2))
        wait(2)
        ctl("mouse", "left", "down")
        wait(3)
        item = held().get("item", "")
        ticks = 3
        while ticks < 60 and "cobweb" in json.dumps(ctl("block", str(X0), str(Y + 1), str(Z0 + 2))):
            wait(1)
            ticks += 1
        ctl("mouse", "left", "up")
        wait(5)
        return item, ticks

    only("autotool")
    tool, fast = mining()
    only()
    plain, slow = mining()
    cmd(f"setblock {X0} {Y+1} {Z0+2} minecraft:air")
    return "sword" in tool and fast < 20 and slow >= 60, (
        f"Auto Tool took {tool} and broke the cobweb in {fast} ticks; the {plain} alone: "
        + (f"{slow} ticks" if slow < 60 else "not broken in 60 ticks"))


@test("Auto Eat")
def _():
    slot(2, "bread", 8)
    slot(3, "golden_apple", 2)
    # Hunger drains the food bar fast (not in peaceful); Auto Eat eats bread (golden apples are blacklisted) until it
    # is past 16.
    cmd("difficulty easy", "effect give @s minecraft:hunger 6 255 true")
    wait(130)
    low = state()["player"]["food"]
    only("autoeat")
    wait(200)
    food = state()["player"]["food"]
    counts = ctl("inventory").get("counts", {})
    only()
    bread, apples = counts.get("minecraft:bread", 0), counts.get("minecraft:golden_apple", 0)
    return low <= 16 and food > 16 and bread < 8 and apples == 2, f"food {low} -> {food} with it on; {8 - bread} bread eaten, golden apples left {apples}"


def hungry(seconds=6, level=255):
    """The hunger effect drains saturation, then the food bar; easy, as peaceful fills the bar back up (clean() puts
    peaceful back)."""
    cmd("difficulty easy", f"effect give @s minecraft:hunger {seconds} {level} true")
    wait(seconds * 20 + 10)
    p = state()["player"]
    return p["food"], p.get("saturation", 0)


@test("Auto Eat (blocks in hand)")
def _():
    # Blocks in the hand, looking at the floor: Auto Eat eats the bread and never places a block, though the hand is
    # put back on the blocks every two seconds while it eats (as your keys, Auto Tool or Scaffold do); then it eats
    # again the next time you are hungry. From the off hand with blocks in the main hand too.
    slot(0, "cobblestone", 64)
    slot(2, "bread", 8)
    ctl("key", "1", "tap")
    ctl("look", "0", "60")
    low = hungry()[0]
    only("autoeat")
    for _ in range(8):
        wait(40)
        ctl("key", "1", "tap")
    first = state()["player"]["food"]
    again = hungry()[0]
    wait(200)
    second = state()["player"]["food"]
    only()
    counts = ctl("inventory").get("counts", {})
    placed = 64 - counts.get("minecraft:cobblestone", 0)
    bread = 8 - counts.get("minecraft:bread", 0)
    reset()
    slot(0, "cobblestone", 64)
    cmd("item replace entity @s weapon.offhand with minecraft:bread 8")
    ctl("key", "1", "tap")
    ctl("look", "0", "60")
    off_low = hungry()[0]
    only("autoeat")
    wait(200)
    off_food = state()["player"]["food"]
    only()
    held_after = held().get("item")
    counts = ctl("inventory").get("counts", {})
    off_placed = 64 - counts.get("minecraft:cobblestone", 0)
    ok = (low <= 16 and first > 16 and again <= 16 and second > 16 and placed == 0 and off_low <= 16 and off_food > 16
          and off_placed == 0 and held_after == "minecraft:cobblestone")
    return ok, (f"food {low} -> {first}, then {again} -> {second}; {bread} bread eaten, {placed} blocks placed; from the off "
                f"hand: food {off_low} -> {off_food}, {off_placed} blocks placed, holding {held_after} after")


@test("Auto Eat (saturation)")
def _():
    # Saturation drained to 0, the food bar still above Hunger threshold (16): Food mode Hunger waits, Saturation eats.
    # (A server tells saturation only as it reaches 0 or as health or food change.)
    slot(2, "bread", 8)
    food, sat = hungry(4, 209)
    only("autoeat")
    wait(60)
    waited = state()["player"]["food"]
    setting("autoeat", "food mode", "Saturation")
    wait(80)
    p = state()["player"]
    only()
    return food > 16 and sat <= 5 and waited == food and p.get("saturation", 0) > sat, (
        f"food {food}, saturation {sat:.1f}: Food mode Hunger left food at {waited}, Saturation ate to {p['food']} "
        f"(saturation {p.get('saturation', 0):.1f})")


def break_ticks(pick, block, tries=3):
    """Ticks to break `block` by hand with `pick` (the game's own mining; the least of `tries`), and whether the
    server had it gone every time."""
    slot(0, pick)
    ctl("key", "1", "tap")
    at = (X0, Y + 1, Z0 + 2)
    ticks, gone = [], True
    for _ in range(tries):
        cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:{block}")
        wait(10)
        ticks.append(ctl("break", *map(str, at), "--timeout", "30", timeout=60).get("ticks", -1))
        wait(10)
        gone = gone and check(f"execute if block {at[0]} {at[1]} {at[2]} minecraft:air")
    cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:air")
    return min(ticks), gone


@test("Plain mining (held button)")
def _():
    return plain_mining()


def plain_mining():
    """Survival, the left button held on a block as a player does: each of three stone blocks breaks the first time
    (gone on the server too) in the time an iron pickaxe takes, and nothing comes back. Every module off unless a test
    turned some on."""
    slot(0, "iron_pickaxe")
    ctl("key", "1", "tap")
    at = (X0, Y + 1, Z0 + 2)
    took = []
    for _ in range(3):
        cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:stone")
        wait(10)
        ctl("look-at-block", *map(str, at))
        wait(3)
        t0 = state()["ticks"]
        ctl("mouse", "left", "down")
        ticks = -1
        for _ in range(80):
            wait(1)
            if ctl("block", *map(str, at)).get("block") == "minecraft:air":
                ticks = state()["ticks"] - t0
                break
        ctl("mouse", "left", "up")
        wait(20)
        kept = ctl("block", *map(str, at)).get("block") == "minecraft:air"
        took.append((ticks, kept and check(f"execute if block {at[0]} {at[1]} {at[2]} minecraft:air")))
    return all(0 < t <= 20 and gone for t, gone in took), f"(ticks to break, still gone after) for three stone blocks: {took}"


@test("Plain mining (Anti Hunger)")
def _():
    # Anti Hunger hides the ground from the server, but not while you break a block (the server refuses the break of a
    # player off the ground and the block comes back).
    only("antihunger")
    ok, note = plain_mining()
    only()
    return ok, note


SPEED_CASES = [("stone_pickaxe", "stone"), ("iron_pickaxe", "stone"), ("diamond_pickaxe", "stone"), ("diamond_pickaxe", "obsidian")]


def speed_mine(mode):
    # The game waits 5 ticks (while you keep attacking) between one block's breaking and the next, and every timed
    # break but the very first follows another: a first break makes them all wait the same.
    break_ticks("stone_pickaxe", "stone", 1)
    plain = [break_ticks(p, b, 1 if b == "obsidian" else 3) for p, b in SPEED_CASES]
    setting("speedmine", "mode", mode)
    only("speedmine")
    fast = [break_ticks(p, b, 1 if b == "obsidian" else 3) for p, b in SPEED_CASES]
    only()
    alive = bool(state().get("player"))
    ok = alive and all(g for _, g in plain + fast) and all(0 < f[0] < p[0] for p, f in zip(plain, fast))
    cells = ", ".join(f"{p.split('_')[0]}/{b} {pt}->{ft}{'' if g else ' (ghost)'}" for (p, b), (pt, _), (ft, g) in zip(SPEED_CASES, plain, fast))
    return ok, f"ticks by hand without -> with {mode}: {cells}"


for _mode in ("Damage", "Normal", "Haste"):
    test(f"Speed Mine ({_mode})")(lambda _m=_mode: speed_mine(_m))


def swap_break(first, then, swap_at=6):
    """Ticks from pressing to the stone being gone (here) when the held pickaxe changes after `swap_at` ticks, and
    whether the server has it gone."""
    slot(0, first)
    slot(1, then)
    ctl("key", "1", "tap")
    at = (X0, Y + 1, Z0 + 2)
    cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:stone")
    wait(10)
    ctl("look-at-block", *map(str, at))
    wait(3)
    t0 = state()["ticks"]
    ctl("mouse", "left", "down")
    wait(swap_at)
    ctl("key", "2", "tap")
    took = -1
    for _ in range(80):
        wait(1)
        if ctl("block", *map(str, at)).get("block") == "minecraft:air":
            took = state()["ticks"] - t0
            break
    ctl("mouse", "left", "up")
    wait(10)
    gone = check(f"execute if block {at[0]} {at[1]} {at[2]} minecraft:air")
    cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:air")
    return took, gone


@test("No Break Reset")
def _():
    # A stone pickaxe for 6 ticks (over half the stone), then a diamond one: the game starts again, this keeps on.
    plain = swap_break("stone_pickaxe", "diamond_pickaxe")
    only("nobreakreset")
    kept = swap_break("stone_pickaxe", "diamond_pickaxe")
    setting("nobreakreset", "only to better tools", "true")
    better = swap_break("stone_pickaxe", "diamond_pickaxe")
    worse = swap_break("diamond_pickaxe", "stone_pickaxe", swap_at=3)
    only()
    ok = all(g for _, g in (plain, kept, better, worse)) and 0 < kept[0] < plain[0] and 0 < better[0] < plain[0] and worse[0] > kept[0]
    return ok, (f"stone then diamond pickaxe: {plain[0]} ticks, {kept[0]} with it on, {better[0]} with Only to better tools; diamond then "
                f"stone with Only to better tools: {worse[0]} (started again); server has every block gone: {all(g for _, g in (plain, kept, better, worse))}")


@test("No Slow (eating, Ground %)")
def _():
    def eating_rate():
        """Blocks a second walking while eating a golden apple (32 ticks: long enough for the measure)."""
        reset(z=-14.5)
        slot(0, "golden_apple", 64)
        ctl("key", "1", "tap")
        ctl("mouse", "right", "down")
        r = rate(ticks=20, warm=8)
        ctl("mouse", "right", "up")
        return r

    slow = eating_rate()
    only("noslow")
    full = eating_rate()
    setting("noslow", "ground %", "50")
    half = eating_rate()
    only()
    return slow < 2.0 and full > 3.8 and full * 0.35 < half < full * 0.75, (
        f"walking while eating: {slow:.2f} blocks/s off, {full:.2f} with No Slow, {half:.2f} at Ground 50%")


@test("Fast Place (speeds)")
def _():
    # A stone at eye height 5 blocks ahead: holding use builds a row of 4 back towards you. Ticks until all 4 are in.
    def row(speed):
        reset()
        cmd(f"setblock {X0} {Y+2} {Z0+5} minecraft:stone")
        slot(0, "cobblestone", 64)
        ctl("key", "1", "tap")
        if speed:
            setting("fastplace", "speed", str(speed))
            only("fastplace")
        ctl("look", "0", "0")
        wait(5)
        t0 = state()["ticks"]
        ctl("mouse", "right", "down")
        took = -1
        for _ in range(60):
            wait(1)
            if count(X0, Y + 2, Z0 + 1, X0, Y + 2, Z0 + 4, "cobblestone") >= 4:
                took = state()["ticks"] - t0
                break
        ctl("mouse", "right", "up")
        only()
        cmd(f"fill {X0} {Y+2} {Z0+1} {X0} {Y+2} {Z0+5} minecraft:air")
        return took

    plain, one, five = row(None), row(1), row(5)
    return 0 < five < one < plain or (0 < five < plain and one <= plain), f"4 blocks placed in {plain} ticks without, {one} at Speed 1, {five} at Speed 5"


@test("Middle Click Extra")
def _():
    slot(4, "ender_pearl", 4)
    ctl("key", "3", "tap")
    only("middleclickextra")
    ctl("look", "0", "-30")
    ctl("mouse", "middle", "click")
    wait(10)
    left = ctl("inventory").get("counts", {}).get("minecraft:ender_pearl", 0)
    selected = state().get("selected")
    return left == 3 and selected == 3, f"{left} pearls left after a middle click from slot 3, slot {selected} selected after"


@test("Auto Totem")
def _():
    cmd("item replace entity @s inventory.4 with minecraft:totem_of_undying 1")
    only("autototem")
    wait(30)
    smart = check("execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    setting("autototem", "health", "36")
    wait(30)
    low = check("execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    cmd("item replace entity @s weapon.offhand with minecraft:air", "item replace entity @s inventory.4 with minecraft:totem_of_undying 1")
    setting("autototem", "health", "10")
    setting("autototem", "mode", "Strict")
    wait(40)
    strict = check("execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    return strict and low and not smart, (f"at full health: offhand {'filled' if smart else 'empty'} in Smart; Smart with Health 36: "
                                         f"{'filled' if low else 'empty'}; Strict: {'filled' if strict else 'empty'}")


@test("Auto Shearer")
def _():
    def sheared(distance):
        reset()
        slot(0, "shears")
        cmd(f"summon sheep {X0+0.5} {Y+1} {Z0+3.0} {NOAI}")
        setting("autoshearer", "distance", distance)
        only("autoshearer")
        wait(40)
        done = check("execute if entity @e[type=sheep,nbt={Sheared:1b}]")
        only()
        return done

    near, far = sheared("5"), sheared("2")
    return near and not far, f"a sheep 2.5 blocks away: sheared at Distance 5: {near}, at 2: {far}"


@test("Auto Nametag")
def _():
    def named(rng):
        reset()
        cmd("item replace entity @s hotbar.0 with minecraft:name_tag[custom_name='\"Bob\"']", f"summon pig {X0+0.5} {Y+1} {Z0+3.5} {NOAI}")
        wait(10)
        setting("autonametag", "entities", "pig")
        setting("autonametag", "range", rng)
        only("autonametag")
        wait(40)
        done = check("execute if entity @e[type=pig,nbt={CustomName:'\"Bob\"'}]")
        only()
        return done

    near, far = named("5"), named("2")
    return near and not far, f"a pig 3 blocks away named Bob at Range 5: {near}, at 2: {far}"


@test("Auto Mount")
def _():
    cmd(f"summon horse {X0+1.5} {Y+1} {Z0+1.5} {{Tame:1b,NoAI:1b,PersistenceRequired:1b,equipment:{{saddle:{{id:\"minecraft:saddle\",count:1}}}}}}")
    wait(10)
    only("automount")
    wait(40)
    riding = state().get("riding") or ctl("status").get("riding")
    only()
    ctl("key", "shift", "down")
    wait(5)
    ctl("key", "shift", "up")
    return riding is not None, f"riding: {riding}"


@test("Flamethrower")
def _():
    def burning(distance):
        reset()
        # No fire left from the run before.
        cmd(f"fill {X0-2} {Y+1} {Z0+1} {X0+2} {Y+1} {Z0+5} minecraft:air")
        slot(0, "flint_and_steel")
        cmd(f"summon cow {X0+0.5} {Y+1} {Z0+3.0} {NOAI}")
        wait(10)
        setting("flamethrower", "entities", "cow")
        setting("flamethrower", "distance", distance)
        only("flamethrower")
        wait(40)
        cmd("data get entity @e[type=cow,limit=1] Fire")
        wait(2)
        out = chat_first()
        only()
        fire = out.rsplit(":", 1)[-1].strip().rstrip("s")
        try:
            return int(fire) > 0
        except ValueError:
            return False

    near, far = burning("5"), burning("2")
    return near and not far, f"a cow 2.5 blocks away set alight at Distance 5: {near}, at 2: {far}"


@test("Liquid Filler")
def _():
    # A pool sunk into the platform, so the water stays put.
    cmd(f"fill {X0+2} {Y-1} {Z0-1} {X0+3} {Y-1} {Z0+1} minecraft:stone", f"fill {X0+2} {Y} {Z0-1} {X0+3} {Y} {Z0+1} minecraft:water")
    slot(0, "cobblestone", 20)
    ctl("key", "1", "tap")
    wait(10)
    only("liquidfiller")
    wait(80)
    left = count(X0 + 2, Y, Z0 - 1, X0 + 3, Y, Z0 + 1, "water")
    filled = count(X0 + 2, Y, Z0 - 1, X0 + 3, Y, Z0 + 1, "cobblestone")
    only()
    return left == 0 and filled == 6, f"{filled} of 6 water blocks filled with cobblestone, {left} left"


@test("Spawn Proofer")
def _():
    cmd(f"fill {X0-4} {Y+1} {Z0-4} {X0+4} {Y+4} {Z0+4} minecraft:stone hollow", f"tp @s {X0+0.5} {Y+1} {Z0+0.5}", "time set 18000")
    slot(0, "torch", 64)
    ctl("key", "1", "tap")
    wait(15)
    only("spawnproofer")
    wait(60)
    dark = 64 - held().get("count", 64)
    setting("spawnproofer", "light level", "7")
    wait(60)
    dim = 64 - held().get("count", 64)
    only()
    # Light level is the most light a spot may have and still get one.
    return 1 <= dark < dim, f"{dark} torches light a dark 7x7 room to above 0, {dim} to above 7"


@test("Surround")
def _():
    slot(0, "obsidian", 16)
    only("surround")
    wait(30)
    four = count(X0 - 1, Y + 1, Z0 - 1, X0 + 1, Y + 2, Z0 + 1, "obsidian")
    setting("surround", "double height", "true")
    wait(30)
    eight = count(X0 - 1, Y + 1, Z0 - 1, X0 + 1, Y + 2, Z0 + 1, "obsidian")
    only()
    return four == 4 and eight == 8, f"{four} obsidian round your feet, {eight} with Double height"


@test("Scaffold")
def _():
    def walk_off(**sets):
        reset(z=15.0)
        cmd(f"fill {X0-2} {Y-1} {Z0+17} {X0+2} {Y} {Z0+26} minecraft:air")
        slot(0, "stone", 32)
        wait(10)
        for k, v in sets.items():
            setting("scaffold", k.replace("_", " "), v)
        only("scaffold")
        walk(40)
        p = pos()
        placed = count(X0 - 2, Y - 1, Z0 + 17, X0 + 2, Y, Z0 + 26, "stone")
        only()
        return p, placed

    on_, placed = walk_off()
    listed, kept = walk_off(blocks_filter="Whitelist", blocks="dirt")
    setting("scaffold", "blocks filter", "Blacklist")
    setting("scaffold", "blocks", "")
    wide, wide_placed = walk_off(air_place="true", radius="1.5")
    # Fast tower: jump held in place, a stack of blocks under you.
    reset()
    slot(0, "stone", 32)
    setting("scaffold", "air place", "false")
    setting("scaffold", "fast tower", "true")
    only("scaffold")
    held_keys(("space",), 40)
    tower = pos()[1] - (Y + 1)
    only()
    cmd(f"fill {X0} {Y+1} {Z0} {X0} {Y+30} {Z0} minecraft:air")
    reset()
    slot(0, "stone", 32)
    only("scaffold")
    setting("scaffold", "fast tower", "false")
    held_keys(("space",), 40)
    climb = pos()[1] - (Y + 1)
    only()
    ok = (on_[2] - Z0 > 19 and on_[1] > Y + 0.99 and placed >= 3 and listed[1] < Y and kept == 0 and wide[1] > Y + 0.99
          and wide_placed > placed and tower > climb * 1.5 and climb > 1)
    return ok, (f"walked out to z {on_[2]-Z0:.1f} past the edge (17) on {placed} blocks; with only dirt allowed: fell, {kept} placed; "
                f"Air place, Radius 1.5: {wide_placed} placed; jumping 2 s: {climb:.1f} blocks up, {tower:.1f} with Fast tower")


@test("Offhand")
def _():
    slot(3, "end_crystal", 2)
    cmd("item replace entity @s inventory.5 with minecraft:totem_of_undying 1")
    setting("offhand", "hotbar", "true")
    only("offhand")
    wait(40)
    crystal = check("execute if items entity @s weapon.offhand minecraft:end_crystal")
    setting("offhand", "item", "Totem")
    wait(40)
    totem = check("execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    # Always gap on sword: a sword in hand brings the enchanted golden apple.
    setting("offhand", "item", "Crystal")
    setting("offhand", "always gap on sword", "true")
    slot(0, "diamond_sword")
    cmd("item replace entity @s inventory.6 with minecraft:enchanted_golden_apple 1")
    ctl("key", "1", "tap")
    wait(40)
    gap = check("execute if items entity @s weapon.offhand minecraft:enchanted_golden_apple")
    # At Min health 19 with less health, the totem comes whatever the Item.
    setting("offhand", "min health", "19")
    cmd("damage @s 6 minecraft:generic")
    # Before the regeneration of a full food bar heals it back past 19 and the gapple returns.
    wait(4)
    health = hp()
    low = check("execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    only()
    return crystal and totem and gap and low, (f"offhand holds the crystal with Item Crystal: {crystal}, the totem with Item Totem: {totem}, "
                                              f"the gapple with a sword and Always gap on sword: {gap}, the totem at {health} health: {low}")


@test("Auto Armor")
def _():
    cmd("give @s minecraft:iron_chestplate", "give @s minecraft:diamond_chestplate", "give @s minecraft:leather_boots")
    only("autoarmor")
    wait(60)
    chest = check("execute if items entity @s armor.chest minecraft:diamond_chestplate")
    boots = check("execute if items entity @s armor.feet minecraft:leather_boots")
    return chest and boots, f"wears the diamond chestplate over the iron one: {chest}, and the boots: {boots}"


@test("Fast Use")
def _():
    def thrown():
        reset()
        slot(0, "snowball", 16)
        ctl("key", "1", "tap")
        wait(4)
        ctl("look", "0", "-30")
        start = held().get("count", 0)
        ctl("mouse", "right", "down")
        wait(40)
        ctl("mouse", "right", "up")
        return start - held().get("count", 0)

    plain = thrown()
    only("fastuse")
    fast = thrown()
    only()
    return fast > plain + 4, f"{plain} snowballs thrown holding use for 2 s, {fast} with Fast Use"


@test("Break Delay")
def _():
    row = f"fill {X0} {Y+2} {Z0+1} {X0} {Y+2} {Z0+4} minecraft:dirt"
    slot(0, "stone_shovel")
    ctl("key", "1", "tap")

    def dug():
        cmd(row)
        ctl("look", "0", "0")
        wait(2)
        ctl("mouse", "left", "down")
        wait(24)
        ctl("mouse", "left", "up")
        return 4 - count(X0, Y + 2, Z0 + 1, X0, Y + 2, Z0 + 4, "dirt")

    plain = dug()
    only("breakdelay")
    fast = dug()
    setting("breakdelay", "cooldown", "5")
    slow = dug()
    only()
    return fast > plain and slow < fast, f"{plain} of 4 blocks in 1.2 s, {fast} with Break Delay at 0 ticks, {slow} at 5 (the game's own)"


# ---- combat

def pig(dz=2.0, dx=0.0, tag=NOAI):
    cmd(f"summon pig {X0 + 0.5 + dx} {Y + 1} {Z0 + 0.5 + dz} {tag}")
    wait(10)
    return mobs("pig")[0]["id"]


@test("Trigger Bot")
def _():
    def hit(passive):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        pid = pig()
        ctl("look-at-entity", str(pid))
        setting("triggerbot", "passive", passive)
        only("triggerbot")
        wait(40)
        m = mobs("pig")
        only()
        return m[0]["health"] if m else 0

    hurt, spared = hit("true"), hit("false")
    return hurt < 10 and spared == 10, f"pig on the crosshair: health {hurt} with Passive on, {spared} with it off"


@test("Entities list on top of the kinds")
def _():
    # Trigger Bot with Passive off: the pig on the crosshair is hit when Entities lists pig, not when it lists cow.
    def hit(entities):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        pid = pig()
        ctl("look-at-entity", str(pid))
        setting("triggerbot", "passive", "false")
        setting("triggerbot", "entities", entities)
        only("triggerbot")
        wait(40)
        m = mobs("pig")
        only()
        return m[0]["health"] if m else 0

    listed, other = hit("pig"), hit("cow")
    return listed < 10 and other == 10, f"pig on the crosshair, Passive off: health {listed} with Entities pig, {other} with cow"


@test("Kill Aura")
def _():
    def attacked(dz, walled=False):
        """Health the pig lost, and the swings Kill Aura made at it."""
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        pig(dz)
        if walled:
            cmd(f"fill {X0-2} {Y+1} {Z0+2} {X0+2} {Y+3} {Z0+2} minecraft:stone")
        setting("killaura", "entities", "pig")
        swings = state()["swings"]
        only("killaura")
        wait(60)
        swings = state()["swings"] - swings
        m = mobs("pig")
        only()
        if walled:
            cmd(f"fill {X0-2} {Y+1} {Z0+2} {X0+2} {Y+3} {Z0+2} minecraft:air")
        return 10 - (m[0]["health"] if m else 0), swings

    near = attacked(2.5)
    setting("killaura", "range", "1.5")
    short_range = attacked(2.5)
    setting("killaura", "range", "4.5")
    # Behind a wall, 3.4 blocks from your eyes to its feet: inside Walls range 3.5, outside 2. (Whether the
    # server lets a hit through a wall is the server's business: the swings say what Kill Aura tried.)
    walled = attacked(3.0, walled=True)
    setting("killaura", "walls range", "2")
    blocked = attacked(3.0, walled=True)
    return near[0] > 0 and short_range == (0, 0) and walled[1] > 0 and blocked[1] == 0, (
        f"pig 2.5 blocks away lost {near[0]} health at Range 4.5, {short_range[0]} at Range 1.5; behind a wall: {walled[1]} swings, "
        f"{walled[0]} health lost at Walls range 3.5, {blocked[1]} swings at 2")


@test("Criticals")
def _():
    # What the first hit Kill Aura, Trigger Bot or a click makes at full attack cooldown takes from a pig with 1000
    # health and no armour, without and with Criticals (a critical hit is 1.5 times): a sword (7) and an axe (9) in
    # Packet mode, a sword in Jump and Mini jump, a mace (6, and a smash adds 4 a block fallen), the fist (1) with
    # Only with a weapon, a click with Only Kill Aura. On a server the probe counts the move packets in a tick
    # through all of it (never two).
    tough = "{NoAI:1b,PersistenceRequired:1b,Silent:1b,Health:1000f,attributes:[{id:'minecraft:max_health',base:1000.0}]}"
    mark = probe_mark() if SERVER and probe_path() else None

    def hit(item, how, crits=True):
        reset()
        if item:
            slot(0, item)
        ctl("key", "1", "tap")
        pid = pig(tag=tough)
        ctl("look-at-entity", str(pid))
        wait(30)
        before = mobs("pig")[0]["health"]
        only(*(("criticals",) if crits else ()), *((how,) if how != "click" else ()))
        if how == "click":
            ctl("mouse", "left", "click")
        lost = 0
        for _ in range(40):
            wait(1)
            m = mobs("pig")
            if m and m[0]["health"] < before:
                lost = before - m[0]["health"]
                break
        only()
        return round(lost, 2)

    setting("killaura", "entities", "pig")
    setting("triggerbot", "passive", "true")
    packet = {(item, how): (hit(item, how, False), hit(item, how))
              for item in ("diamond_sword", "diamond_axe") for how in ("killaura", "triggerbot", "click")}
    modes = {}
    for mode in ("Jump", "Mini jump"):
        setting("criticals", "mode", mode)
        modes[mode] = (hit("diamond_sword", "killaura"), hit("diamond_sword", "click"))
    setting("criticals", "mode", "Packet")
    setting("criticals", "additional height", "0.5")
    higher = hit("diamond_sword", "click")
    setting("criticals", "additional height", "0")
    mace = (hit("mace", "click", False), hit("mace", "click"))
    setting("criticals", "additional height", "1")
    mace_high = hit("mace", "click")
    setting("criticals", "additional height", "0")
    fist = hit(None, "click")
    setting("criticals", "only with a weapon", "true")
    fist_weapon_only = hit(None, "click")
    setting("criticals", "only with a weapon", "false")
    setting("criticals", "only kill aura", "true")
    click_ka_only = hit("diamond_sword", "click")
    most = wire_stats(mark)[1] if mark is not None else None
    # Packet mode's hop is packets only: a server that judges the fall by its own physics (Horizon's, single player's
    # built-in one) takes no crit from it. Jump and Mini jump really fall, so they crit everywhere.
    crit = lambda off, on: off > 0 and abs(on / off - 1.5) < 0.02
    packet_crits = all(crit(*v) for v in packet.values())
    base = {"diamond_sword": 7, "diamond_axe": 9}
    ok = (all(abs(v[0] - base[i]) < 0.05 for (i, _), v in packet.items()) and all(abs(d - 10.5) < 0.05 for v in modes.values() for d in v)
          and (not packet_crits or (abs(higher - 10.5) < 0.05 and abs(fist - 1.5) < 0.05)) and abs(fist_weapon_only - 1) < 0.05
          and abs(click_ka_only - 7) < 0.05 and most in (None, 0, 1))
    names = {"diamond_sword": "sword", "diamond_axe": "axe", "killaura": "Kill Aura", "triggerbot": "Trigger Bot", "click": "click"}
    return ok, (f"Packet crits: {packet_crits}; damage off/on, Packet: " + ", ".join(f"{names[i]} {names[h]} {a}/{b}" for (i, h), (a, b) in packet.items()) +
                "; sword Kill Aura/click: " + ", ".join(f"{k} {a}/{b}" for k, (a, b) in modes.items()) +
                f"; Packet with Additional height 0.5: {higher}; mace click off/on {mace[0]}/{mace[1]}, {mace_high} at Additional "
                f"height 1; fist {fist}, {fist_weapon_only} with Only with a weapon; a click with Only Kill Aura {click_ka_only}; "
                f"most move packets in a tick: {most}")


def crit_pig():
    """A pig with 1000 health, no AI and no knockback 2 blocks in front, in reach at a jump's top too."""
    tough = ("{NoAI:1b,PersistenceRequired:1b,Silent:1b,Health:1000f,attributes:[{id:'minecraft:max_health',base:1000.0},"
             "{id:'minecraft:knockback_resistance',base:1.0}]}")
    cmd("kill @e[type=pig]", f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 2.5} {tough}")
    wait(10)
    return mobs("pig")[0]["id"]


def crit_run(hits, act=None, keys=(), look=True):
    """Hits on the pig until `hits` have landed (Kill Aura or Trigger Bot make them, or `act`, run every 3 ticks,
    does), looking at it first if `look`: how many were critical (a diamond sword's 7 made 10.5), and how many times
    the server put you back."""
    pid = crit_pig()
    if look:
        ctl("look-at-entity", str(pid))
    for k in keys:
        ctl("key", k, "down")
    mark = probe_mark() if SERVER and probe_path() else None
    landed = crits = idle = 0
    last = mobs("pig")[0]["health"]
    while landed < hits and idle < 200:
        if act:
            act()
        wait(3)
        idle += 3
        m = mobs("pig")
        if not m:
            pid = crit_pig()
            last = mobs("pig")[0]["health"]
            continue
        drop = last - m[0]["health"]
        last = m[0]["health"]
        if drop > 0.5:
            landed, idle = landed + 1, 0
            crits += drop > 8.5
        if last < 100:
            pid = crit_pig()
            last = mobs("pig")[0]["health"]
    for k in keys:
        ctl("key", k, "up")
    teleports = wire_stats(mark)[2] if mark is not None else None
    return landed, crits, teleports


@test("Criticals (200 hits)")
def _():
    # Every hit at full cooldown is critical: 200 by Kill Aura in each mode, and 50 each by your own clicks, Trigger
    # Bot, Kill Aura while you sprint past it, and Kill Aura while you jump. The server never puts you back. About
    # 20 minutes: past the usual ten a test gets.
    import signal
    signal.alarm(3600)
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    setting("killaura", "entities", "pig")
    setting("triggerbot", "passive", "true")
    rates = {}
    for mode in ("Packet", "Jump", "Mini jump"):
        setting("criticals", "mode", mode)
        only("criticals", "killaura")
        rates[f"{mode}, Kill Aura"] = crit_run(200)
    setting("criticals", "mode", "Packet")
    only("criticals")
    tick = {"n": 0}

    def click():
        tick["n"] += 1
        if tick["n"] % 5 == 0:
            ctl("mouse", "left", "click")
    rates["Packet, clicks"] = crit_run(50, click)
    only("criticals", "triggerbot")
    rates["Packet, Trigger Bot"] = crit_run(50)
    # Sprinting past it, east along the line 2 blocks short of it, back to the start each time.
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    setting("killaura", "range", "3")

    # The test's own teleports back to the start are not the server putting you back.
    back = {"n": 0}

    def run_past():
        if pos()[0] > X0 + 5:
            back["n"] += 1
            ctl("chat", f"/tp @s {X0 - 4.5} {Y + 1} {Z0 + 0.5} -90 0")
    ctl("chat", f"/tp @s {X0 - 4.5} {Y + 1} {Z0 + 0.5} -90 0")
    only("criticals", "killaura", "sprint")
    n, c, t = crit_run(50, run_past, keys=("w",), look=False)
    rates["Packet, Kill Aura sprinting"] = (n, c, t if t is None else t - back["n"])
    # Jumping in place in front of it, at Kill Aura's usual range.
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    setting("killaura", "range", "4.5")
    only("criticals", "killaura")
    rates["Packet, Kill Aura jumping"] = crit_run(50, keys=("space",))
    only()
    ok = all(n > 0 and c == n and t in (None, 0) for n, c, t in rates.values())
    return ok, "; ".join(f"{k}: {c}/{n} critical" + (f", {t} put back" if t else "") for k, (n, c, t) in rates.items())


def shot_pixels(name, want):
    """Pixels of a screenshot that `want` (r, g, b) accepts."""
    from PIL import Image
    path = f"/tmp/{name}.png"
    ctl("screenshot", path)
    time.sleep(2)
    return sum(1 for px in Image.open(path).convert("RGB").getdata() if want(*px))


@test("3D box sides (no flood)")
def _():
    # Storage ESP's 3D box with Sides fills the chest's own faces, never the screen rectangle round it: a chest close
    # beside the view, one partly behind the camera, and the camera inside one.
    orange = lambda r, g, b: r > 220 and 130 < g < 190 and b < 100
    setting("storageesp", "shape mode", "Sides")
    setting("storageesp", "fill opacity", "1")
    setting("storageesp", "fade distance", "0")
    only("storageesp")
    def seen(mode, x, y, z, name):
        cmd(f"fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+3} {Z0+3} minecraft:air", f"setblock {X0+x} {Y+y} {Z0+z} minecraft:chest")
        setting("storageesp", "mode", mode)
        ctl("look", "0", "0")
        wait(10)
        return shot_pixels(name, orange)
    near3d, near2d = seen("3D box", 1, 1, 1, "box_near3d"), seen("2D box", 1, 1, 1, "box_near2d")
    behind = seen("3D box", 1, 1, 0, "box_behind")
    inside = seen("3D box", 0, 2, 0, "box_inside")
    cmd(f"fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+3} {Z0+3} minecraft:air")
    only()
    screen = 1280 * 720
    return 5000 < near3d < near2d and behind < 0.3 * screen and inside < 0.02 * screen, (
        f"orange pixels: chest close beside the view {near3d} with 3D sides, {near2d} as a 2D box; partly behind the "
        f"camera {behind}; camera inside it {inside} (of {screen})")


@test("Time Changer and Weather Changer")
def _():
    # The time and weather you see, sampled every few ticks for 5 s: always the ones asked for, though the server's
    # own come in between; Time Changer keeps the server's day (so the moon's phase), and turning Weather Changer off
    # brings the server's rain straight back.
    cmd("time set 100000", "weather rain")
    wait(40)
    setting("timechanger", "time", "18000")
    setting("weatherchanger", "weather", "Clear")
    only("timechanger", "weatherchanger")
    seen = []
    for _ in range(25):
        wait(4)
        st = state()
        seen.append((st["time"]["day_time"], st["weather"]["raining"]))
    only("timechanger")
    wait(2)
    rain_back = state()["weather"]["raining"]
    only()
    wait(2)
    time_back = state()["time"]["day_time"]
    cmd("weather clear", "time set 6000")
    times = {t for t, _ in seen}
    steady = times == {4 * 24000 + 18000} and not any(r for _, r in seen)
    return steady and rain_back and 100000 <= time_back < 101000, (
        f"times seen {sorted(times)} (want {4 * 24000 + 18000}), raining in {sum(r for _, r in seen)} of {len(seen)} samples; "
        f"rain back at once after Weather Changer off: {rain_back}; time after Time Changer off: {time_back}")


def frame(name):
    """The next frame, greyscale (the PNG stays in /tmp to look at)."""
    from PIL import Image
    ctl("screenshot", f"/tmp/{name}.png")
    time.sleep(2)
    return Image.open(f"/tmp/{name}.png").convert("L")


def colour_frame(name):
    """The next frame in colour (the PNG stays in /tmp to look at)."""
    from PIL import Image
    ctl("screenshot", f"/tmp/{name}.png")
    time.sleep(2)
    return Image.open(f"/tmp/{name}.png").convert("RGB")


def recoloured(a, b):
    """Pixels whose colour differs between two colour frames by more than 12 in any channel."""
    from PIL import ImageChops
    return sum(1 for px in ImageChops.difference(a, b).getdata() if max(px) > 12)


def changed(a, b, box=None):
    """Pixels that differ between two frames (in `box`, (left, top, right, bottom), if given)."""
    from PIL import ImageChops
    d = ImageChops.difference(a, b)
    if box:
        d = d.crop(box)
    return sum(1 for v in d.getdata() if v > 40)


@test("Crosshair, Block Overlay, Target ESP")
def _():
    # Each changes the frame where it should: the crosshair in the middle (Cross and Dot differ), Block Overlay on the
    # fence looked at, Target ESP round Kill Aura's husk. Looking down (no moving clouds), and every frame taken once
    # the chat lines a toggle prints have faded.
    reset()
    cmd("difficulty easy", f"setblock {X0} {Y+1} {Z0+1} minecraft:oak_fence", f"summon husk {X0 + 1.5} {Y + 1} {Z0 + 2.5} {HUSK}")
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    ctl("look-at-block", str(X0), str(Y + 1), str(Z0 + 1))
    w, h = 1280, 720
    middle = (w // 2 - 40, h // 2 - 40, w // 2 + 40, h // 2 + 40)

    def shot(name, *mods):
        only(*mods)
        wait(225)
        return frame(name)
    plain = shot("vis_off")
    cross = shot("vis_cross", "crosshair")
    setting("crosshair", "mode", "Dot")
    dot = shot("vis_dot", "crosshair")
    block = shot("vis_block", "blockoverlay")
    setting("killaura", "entities", "husk")
    setting("targetesp", "target", "Combat")
    aura = shot("vis_aura", "killaura")
    target = shot("vis_target", "killaura", "targetesp")
    only()
    cmd(f"setblock {X0} {Y+1} {Z0+1} minecraft:air")
    cross_mid, cross_out = changed(plain, cross, middle), changed(plain, cross) - changed(plain, cross, middle)
    dot_vs_cross = changed(cross, dot, middle)
    block_px, target_px = changed(plain, block), changed(aura, target)
    ok = cross_mid > 20 and cross_out < 0.005 * w * h and dot_vs_cross > 10 and block_px > 200 and target_px > 50
    return ok, (f"pixels changed: Crosshair {cross_mid} in the middle ({cross_out} elsewhere), Dot against Cross {dot_vs_cross}; "
                f"Block Overlay on a fence {block_px}; Target ESP on Kill Aura's husk {target_px} (frames in /tmp/vis_*.png)")


@test("Atmosphere")
def _():
    # At night, looking a little up (the aurora hangs low over the horizon): each sky changes the colour of the frame
    # from the game's own sky, and the frame rate holds. The aurora is faint, so a pixel counts as changed by any
    # visible shift of its colour, not only a jump in brightness.
    reset()
    cmd("time set 18000")
    ctl("look", "0", "-15")

    def sky(name, mode=None):
        if mode:
            setting("atmosphere", "mode", mode)
            only("atmosphere")
        wait(225)
        a = ctl("status")
        wait(40)
        b = ctl("status")
        fps = (b["frames"] - a["frames"]) / max(b["uptime_seconds"] - a["uptime_seconds"], 1e-6)
        return colour_frame(name), fps
    plain, fps0 = sky("atmo_off")
    # What the game's own sky changes by itself in that time (stars, clouds): each sky must change more than that.
    noise = recoloured(plain, sky("atmo_off2")[0]) / (1280 * 720)
    seen = {m: sky(f"atmo_{m.split()[0].lower()}", m) for m in ("Space", "Aurora", "Ringed Planet")}
    only()
    cmd("time set 6000")
    share = {m: recoloured(plain, f) / (1280 * 720) for m, (f, _) in seen.items()}
    ok = all(v > noise + 0.005 for v in share.values()) and all(fps > 0.5 * fps0 for _, fps in seen.values())
    return ok, (f"share of the frame recoloured: " + ", ".join(f"{m} {v:.1%} at {seen[m][1]:.0f} fps" for m, v in share.items())
                + f"; the game's sky {fps0:.0f} fps, {noise:.1%} by itself (frames in /tmp/atmo_*.png)")


@test("Storage ESP (double chests)")
def _():
    # A double chest (and a trapped one) is one box: from in front of the seam, the outline has no line down the
    # middle (the screen's centre column holds only the top and bottom edges), as two single chests side by side do.
    orange = lambda r, g, b: r > 220 and 130 < g < 190 and b < 100
    setting("storageesp", "mode", "2D box")
    setting("storageesp", "shape mode", "Lines")
    setting("storageesp", "fade distance", "0")
    only("storageesp")

    def seam(kind, left, right, name):
        cmd(f"fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+3} {Z0+5} minecraft:air", f"setblock {X0} {Y+1} {Z0+4} minecraft:{kind}[facing=north,type={left}]",
            f"setblock {X0+1} {Y+1} {Z0+4} minecraft:{kind}[facing=north,type={right}]", f"tp @s {X0 + 1.0} {Y + 1} {Z0 + 0.5} 0 20")
        wait(10)
        shot_pixels(name, orange)
        from PIL import Image
        img = Image.open(f"/tmp/{name}.png").convert("RGB")
        w, h = img.size
        return sum(1 for x in range(w // 2 - 8, w // 2 + 8) for y in range(h) if orange(*img.getpixel((x, y))))
    double = seam("chest", "left", "right", "dchest")
    trapped = seam("trapped_chest", "left", "right", "dtrapped")
    singles = seam("chest", "single", "single", "schests")
    cmd(f"fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+3} {Z0+5} minecraft:air")
    only()
    return double < singles / 3 and trapped < singles / 3, (
        f"outline pixels down the middle: double chest {double}, trapped double chest {trapped}, two single chests {singles}")


@test("Zoom (hold V, wheel, sensitivity)")
def _():
    # A gold block 12 ahead: V held covers about 11 times its pixels (a third of the field of view: tan 35 over tan 11.7,
    # squared); the wheel up while held zooms further in; the mouse turns a third as far at 3x; V let go, back to 1x.
    gold = lambda r, g, b: r > 200 and g > 170 and b < 90
    reset("creative")
    # A clear line to the block at eye height (a world where the arena sits in terrain), the block really there, and
    # the chat faded with no toggle lines after it (the count is of the middle of the frame only, where the chat and
    # the HUD's own gold would be counted too), and lit (Paper's test world has the arena under ground).
    setting("clickgui", "toggle messages", "false")
    cmd("time set 6000", "effect give @s minecraft:night_vision 60 0 true", f"fill {X0-2} {Y+1} {Z0+1} {X0+2} {Y+6} {Z0+13} minecraft:air", f"setblock {X0} {Y+2} {Z0+12} minecraft:gold_block")
    ctl("look", "0", "0")
    wait(210)
    if "gold_block" not in json.dumps(ctl("block", str(X0), str(Y + 2), str(Z0 + 12))):
        return False, "the gold block was not placed"

    def shot_pixels(name, want):
        from PIL import Image
        ctl("screenshot", f"/tmp/{name}.png")
        time.sleep(2)
        img = Image.open(f"/tmp/{name}.png").convert("RGB")
        w, h = img.size
        return sum(1 for px in img.crop((w // 4, h // 4, w * 3 // 4, h * 3 // 4)).getdata() if want(*px))

    def turned():
        y0 = ctl("status")["yaw"]
        ctl("mouse-move", "200", "0")
        wait(3)
        y1 = ctl("status")["yaw"]
        ctl("look", "0", "0")
        wait(3)
        return abs(y1 - y0)

    base, turn1 = shot_pixels("zoom_off", gold), turned()
    ctl("key", "v", "down")
    wait(10)
    z3, turn3 = shot_pixels("zoom_3", gold), turned()
    w, h = gui_size()
    ctl("scroll-at", str(w // 2), str(h // 2), "10")
    wait(10)
    z8 = shot_pixels("zoom_8", gold)
    ctl("key", "v", "up")
    wait(10)
    after = shot_pixels("zoom_after", gold)
    cmd(f"setblock {X0} {Y+2} {Z0+12} minecraft:air")
    r3, r8, back, slow = z3 / max(base, 1), z8 / max(base, 1), after / max(base, 1), turn3 / max(turn1, 1e-6)
    ok = base > 20 and 8 < r3 < 16 and r8 > 2 * r3 and 0.8 < back < 1.25 and 0.25 < slow < 0.42
    return ok, (f"gold pixels: 1x {base}, held x{r3:.1f}, after 10 notches x{r8:.1f}, let go x{back:.2f}; "
                f"mouse turn held/unheld {slow:.2f} ({turn3:.2f} / {turn1:.2f} degrees)")


@test("Freecam (start, tracer, move player)")
def _():
    # Start Above with the view tipped down: W held 20 ticks leaves the body where it was; the tracer draws a line to
    # the body (accent pixels with it on, none off); with Move player W walks the body instead.
    accent = lambda r, g, b: r > 230 and 140 < g < 200 and b < 80
    reset("creative")
    ctl("look", "0", "75")
    setting("freecam", "start", "Above")
    setting("freecam", "tracer", "false")
    toggle("freecam", True)
    wait(10)
    off = shot_pixels("fc_tracer_off", accent)
    setting("freecam", "tracer", "true")
    wait(5)
    on_ = shot_pixels("fc_tracer_on", accent)
    p0 = pos()
    ctl("key", "w", "down")
    wait(20)
    ctl("key", "w", "up")
    still = math.dist(p0, pos())
    setting("freecam", "move player", "true")
    ctl("key", "w", "down")
    wait(20)
    ctl("key", "w", "up")
    walked = math.dist(p0, pos())
    toggle("freecam", False)
    ok = on_ > off + 30 and still < 0.01 and walked > 1.5
    return ok, f"tracer pixels off {off}, on {on_}; body moved {still:.3f} flying, {walked:.2f} with Move player"


@test("Hand View (size, no sway)")
def _():
    # Looking at the sky so only the hand is in the lower right: Right scale 0.4 changes that corner with stone in
    # the hand and with the bare arm; turning makes the hand lag for a moment (the frame just after a turn and the
    # settled one differ), not with No sway.
    from PIL import Image, ImageChops

    def corner(name):
        ctl("screenshot", f"/tmp/{name}.png")
        time.sleep(2)
        img = Image.open(f"/tmp/{name}.png").convert("L")
        return img.crop((img.width * 55 // 100, img.height // 2, img.width, img.height))

    def changed(a, b):
        return sum(1 for d in ImageChops.difference(a, b).getdata() if d > 30) / (a.width * a.height)

    def swayed():
        # The share of the hand's corner where the held stone (grey, against the blue sky) is in one frame and not the
        # other: the frame just after a turn and the settled one. Clouds and the sky's own shading are not grey.
        stone = lambda r, g, b: abs(r - g) < 14 and abs(g - b) < 14 and 50 < r < 175
        ctl("look", "0", "-40")
        wait(10)
        ctl("mouse-move", "800", "0")
        frames = []
        for name in ("hv_moving", "hv_settled"):
            ctl("screenshot", f"/tmp/{name}.png")
            time.sleep(2)
            img = Image.open(f"/tmp/{name}.png").convert("RGB")
            frames.append([stone(*px) for px in img.crop((img.width * 55 // 100, img.height // 2, img.width, img.height)).getdata()])
            wait(15)
        return sum(a != b for a, b in zip(*frames)) / len(frames[0])

    reset("creative")
    cmd("item replace entity @s hotbar.0 with minecraft:stone")
    ctl("look", "0", "-40")
    wait(15)
    held_off = corner("hv_held_off")
    setting("handview", "right scale", "0.4")
    toggle("handview", True)
    wait(5)
    held_on = corner("hv_held_on")
    sway_on_off = swayed()
    setting("handview", "right scale", "1")
    setting("handview", "no sway", "true")
    sway_still = swayed()
    cmd("item replace entity @s hotbar.0 with minecraft:air")
    setting("handview", "right scale", "0.4")
    ctl("look", "0", "-40")
    wait(10)
    arm_on = corner("hv_arm_on")
    toggle("handview", False)
    wait(5)
    arm_off = corner("hv_arm_off")
    held, arm = changed(held_off, held_on), changed(arm_off, arm_on)
    ok = held > 0.02 and arm > 0.02 and sway_on_off > 0.005 and sway_still < sway_on_off / 3
    return ok, (f"lower right changed by Right scale 0.4: stone held {held:.1%}, bare arm {arm:.1%}; "
                f"just after a turn vs settled: swaying {sway_on_off:.2%}, No sway {sway_still:.2%}")


@test("No Render particles")
def _():
    # 2000 flame and 2000 happy villager particles forced in view, counted by the game: No Render off, all made; Hide
    # all particles, none; Particles = flame, no flame and the villager ones still made.
    def counts():
        p = ctl("status").get("particles", {})
        return p.get("flame", 0), p.get("happy_villager", 0)

    def made():
        """The particles of each the game made (its counts are since the start) for the two commands."""
        before = counts()
        cmd(f"particle minecraft:flame {X0} {Y+2} {Z0+4} 1 1 1 0 2000 force",
            f"particle minecraft:happy_villager {X0} {Y+2} {Z0+4} 1 1 1 0 2000 force")
        wait(5)
        return tuple(a - b for a, b in zip(counts(), before))

    reset("creative")
    shown = made()
    setting("norender", "hide all particles", "true")
    toggle("norender", True)
    none = made()
    setting("norender", "hide all particles", "false")
    setting("norender", "particles", "flame")
    listed = made()
    toggle("norender", False)
    ok = min(shown) > 500 and none == (0, 0) and listed[0] == 0 and listed[1] > 500
    return ok, f"(flame, villager) made: off {shown}, Hide all {none}, Particles = flame {listed}"


@test("Chat feedback switches")
def _():
    # ClickGUI's Toggle messages off: Fullbright turned on and off by its key writes no line, .t still answers; Module
    # messages off: Self Web with no cobwebs writes no "No cobwebs" line (on, it does).
    reset()
    say(".bind fullbright k")

    def lines(action):
        mark = log_mark()
        action()
        wait(5)
        return sunset_said(mark)

    key = lambda: ctl("key", "k", "tap")
    keyed_on = lines(key)
    setting("clickgui", "toggle messages", "false")
    keyed_off = lines(lambda: (key(), wait(3), key()))
    typed = lines(lambda: say(".t fullbright"))
    say(".bind fullbright none")
    web_on = lines(lambda: toggle("selfweb", True))
    toggle("selfweb", False)
    setting("clickgui", "module messages", "false")
    web_off = lines(lambda: toggle("selfweb", True))
    toggle("selfweb", False)
    cobwebs = lambda got: any("cobweb" in l for l in got)
    ok = keyed_on == ["Fullbright on"] and keyed_off == [] and len(typed) == 1 and typed[0].startswith("Fullbright") \
        and cobwebs(web_on) and not cobwebs(web_off)
    return ok, (f"key with Toggle messages on {keyed_on}, off {keyed_off}; .t {typed}; Self Web with Module messages on "
                f"{cobwebs(web_on)}, off {cobwebs(web_off)}")


@test("Friends list in the menu")
def _():
    # .friend and ClickGUI's Friends (the menu's player picker edits it) are one list: .friend add shows in the
    # setting, .set of the setting shows in .friend.
    def friends_setting():
        mark = log_mark()
        say(".settings clickgui")
        wait(4)
        return next((c for c in sunset_said(mark) if c.startswith("Friends:")), "")

    def friend_list():
        mark = log_mark()
        say(".friend")
        wait(4)
        return next((c for c in sunset_said(mark) if c.startswith("Friends:")), "")

    touched.add("clickgui")
    say(".friend add Steve123")
    added = friends_setting()
    setting("clickgui", "friends", "Alex_9,Steve123")
    listed = friend_list()
    say(".friend remove Alex_9")
    say(".friend remove Steve123")
    gone = friends_setting()
    ok = added == "Friends: Steve123" and listed == "Friends: Alex_9, Steve123" and gone == "Friends: (empty)"
    return ok, f".friend add -> setting {added!r}; .set -> .friend {listed!r}; both removed -> {gone!r}"


@test("Middle click Friend")
def _():
    # Middle Click Extra's Friend: a middle click on Bot1 makes it a friend, a second one takes it away.
    if not CTL2:
        return None, "needs a second player"
    reset("creative")
    cmd("gamemode survival Bot1", f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5}")
    setting("middleclickextra", "item", "Friend")
    toggle("middleclickextra", True)
    ctl("look", "0", "10")
    wait(10)

    def click():
        mark = log_mark()
        ctl("mouse", "middle", "down")
        wait(2)
        ctl("mouse", "middle", "up")
        wait(4)
        return sunset_said(mark)

    first, second = click(), click()
    toggle("middleclickextra", False)
    cmd("gamemode spectator Bot1", f"tp Bot1 {X0 + 0.5} {Y + 30} {Z0 + 60}")
    ok = first == ["Bot1 is a friend."] and second == ["Bot1 is no longer a friend."]
    return ok, f"first middle click {first}, second {second}"


@test("Menu dim on every screen")
def _():
    # ClickGUI's Menu dim off: the world behind the pause menu, the options screens (the game's and Horizon's), the
    # inventory and the chat box is as bright as with no screen; on, it is dimmed. Looking at the sky.
    def screen_shot(name, open_):
        open_()
        wait(10)
        b = brightness(name)
        ctl("close")
        wait(3)
        if state().get("screen") == "paused":
            ctl("resume")
            wait(3)
        return b

    screens = {
        "pause": lambda: ctl("pause"),
        "options": lambda: ctl("options"),
        "video": lambda: ctl("options", "video"),
        "horizon": lambda: ctl("options", "horizon"),
        "inventory": lambda: ctl("open-inventory"),
    }
    reset("creative")
    ctl("look", "0", "-60")
    wait(10)
    plain = brightness("dim_none")
    got = {}
    for dim in ("false", "true"):
        setting("clickgui", "menu dim", dim)
        for name, open_ in screens.items():
            got[(name, dim)] = screen_shot(f"dim_{name}_{dim}", open_)
    # Dimmed: a screen's mean brightness drops well below the plain view; undimmed it stays nearer it than dimmed.
    bad = [n for n in screens if not got[(n, "true")] < got[(n, "false")] - 0.03]
    return not bad, (f"mean brightness with no screen {plain:.3f}; " + ", ".join(
        f"{n} {got[(n, 'false')]:.3f}/{got[(n, 'true')]:.3f}" for n in screens) + " (Menu dim off/on)"
        + (f"; not dimmed less with it off: {bad}" if bad else ""))


@test("Freecam (F5)")
def _():
    # F5 in Freecam changes nothing: the frame (the husk's ESP box with it) is the same before and after, and no hand
    # comes into view.
    reset()
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 6.5} {HUSK}")
    only("esp", "freecam")
    ctl("key", "space", "down")
    wait(10)
    ctl("key", "space", "up")
    ctl("look", "0", "20")
    # The chat lines fade first.
    wait(220)
    from PIL import Image, ImageChops

    def frame(name):
        ctl("screenshot", f"/tmp/{name}.png")
        time.sleep(2)
        return Image.open(f"/tmp/{name}.png").convert("L")
    before = frame("fc_before")
    changed = []
    for i in range(3):
        ctl("key", "f5", "tap")
        wait(10)
        after = frame(f"fc_f5_{i}")
        changed.append(sum(1 for v in ImageChops.difference(before, after).getdata() if v > 40))
    only()
    return max(changed) < 0.01 * 1280 * 720, f"pixels changed by each of three F5 presses: {changed} (of {1280 * 720})"


@test("ESP (item names, tracers, filter)")
def _():
    # A stack of 5 diamonds off to the side ahead: its box, then its name, then a line to it, then none of it with
    # diamonds blacklisted. In pink, which nothing else on screen is (Sunset's chat lines are orange).
    pink = lambda r, g, b: r > 230 and 80 < g < 150 and b > 160
    cmd("kill @e[type=item]")
    cmd(f"summon item {X0+3.5} {Y+1} {Z0+5.5} {{Item:{{id:\"minecraft:diamond\",count:5}},PickupDelay:32767,Age:-32768}}")
    setting("esp", "items", "true")
    setting("esp", "players", "false")
    setting("esp", "items color", "Pink")
    only("esp")
    ctl("look", "0", "20")
    wait(10)
    plain = shot_pixels("itemesp0", pink)
    setting("esp", "item names", "true")
    wait(5)
    named = shot_pixels("itemesp1", pink)
    setting("esp", "item names", "false")
    setting("esp", "item tracers", "true")
    wait(5)
    traced = shot_pixels("itemesp2", pink)
    setting("esp", "item filter", "Blacklist")
    setting("esp", "item list", "diamond")
    wait(5)
    hidden = shot_pixels("itemesp3", pink)
    only()
    cmd("kill @e[type=item]")
    return plain > 20 and named > plain + 30 and traced > plain + 30 and hidden < plain / 2, (
        f"pink pixels: box {plain}, with its name {named}, with a tracer {traced}, diamonds blacklisted {hidden}")


@test("Spawner Alerts")
def _():
    # A zombie spawner and a trial spawner ahead: a chat line each, once, and their labels.
    gold = lambda r, g, b: r > 230 and 150 < g < 190 and b < 40
    cmd(f"setblock {X0+2} {Y+1} {Z0+5} minecraft:spawner{{SpawnData:{{entity:{{id:\"minecraft:zombie\"}}}}}}",
        f"setblock {X0-2} {Y+1} {Z0+5} minecraft:trial_spawner")
    ctl("look", "0", "10")
    wait(5)
    before = shot_pixels("spawners0", gold)
    # Within 16 blocks: a server's world can have spawners of its own further out.
    setting("spawneralerts", "range", "16")
    mark = log_mark()
    only("spawneralerts")
    wait(40)
    labelled = shot_pixels("spawners1", gold)
    said = [line for line in sunset_said(mark) if "spawner" in line]
    only()
    cmd(f"setblock {X0+2} {Y+1} {Z0+5} minecraft:air", f"setblock {X0-2} {Y+1} {Z0+5} minecraft:air")
    zombie = any(line.startswith(f"zombie spawner at {X0+2} {Y+1} {Z0+5}") for line in said)
    trial = any(line.startswith("unknown trial spawner") for line in said)
    return zombie and trial and len(said) == 2 and labelled > before + 50, (
        f"chat: {said}; label pixels {before} -> {labelled}")


@test("Anti Vanish")
def _():
    # Bot1 leaves and comes back with the server's leave and join messages: nothing said. Then it leaves and comes back
    # with those messages pushed out of the recent chat (as if a vanish plugin had sent none): a line each time. The
    # completion check (/msg) finds nobody hidden.
    if not CTL2:
        return None, "needs a second player (CTL2)"
    setting("antivanish", "interval", "200")
    setting("antivanish", "completion check", "true")

    def bot(away, flood):
        """Bot1 leaves (or comes back) between two of Anti Vanish's checks; what it said about Bot1."""
        only("antivanish")
        wait(210)
        mark = log_mark()
        if away:
            ctl2("disconnect")
        else:
            ctl2("key", "esc", "tap")
            time.sleep(2)
            ctl2("connect", SERVER, "Bot1")
            ctl2("wait-ready", "50")
        if flood:
            cmd(*(f"say {i}" for i in range(21)))
        wait(210)
        only()
        return [line for line in sunset_said(mark) if "Bot1" in line]

    told = [bot(True, False), bot(False, False), bot(True, True), bot(False, True)]
    cmd("gamemode spectator Bot1")
    ok = not told[0] and not told[1] and any("left the tab list" in x for x in told[2]) and any("came back" in x for x in told[3])
    hidden = any("not in the tab list" in x for t in told for x in t)
    return ok and not hidden, f"lines about Bot1: with messages {told[0] + told[1]}; without {told[2] + told[3]}"


@test("Hole ESP")
def _():
    # An obsidian hole, a bedrock one, and a double (two side by side, obsidian), each a block deep, in front.
    def hole(x, z, block, wide=1):
        cmd(f"fill {x-1} {Y} {z-1} {x+wide} {Y+1} {z+1} minecraft:{block}", f"fill {x} {Y+1} {z} {x+wide-1} {Y+1} {z} minecraft:air")
    hole(X0 - 3, Z0 + 4, "obsidian")
    hole(X0, Z0 + 4, "bedrock")
    hole(X0 + 3, Z0 + 4, "obsidian", wide=2)
    ctl("look", "0", "40")
    wait(5)
    red = lambda r, g, b: r > 200 and g < 110 and b < 110
    lime = lambda r, g, b: g > 200 and 110 < r < 190 and b < 110
    setting("holeesp", "mode", "2D box")
    setting("holeesp", "shape mode", "Sides")
    setting("holeesp", "fill opacity", "1")
    before = (shot_pixels("hole0", red), shot_pixels("hole0", lime))
    only("holeesp")
    wait(30)
    on = (shot_pixels("hole1", red), shot_pixels("hole1", lime))
    setting("holeesp", "doubles", "false")
    wait(30)
    single = shot_pixels("hole2", red)
    only()
    return on[0] > before[0] + 200 and on[1] > before[1] + 100 and on[0] - single > 100, (
        f"obsidian red pixels {before[0]} -> {on[0]} ({single} with Doubles off), bedrock lime {before[1]} -> {on[1]}")


@test("Target HUD")
def _():
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    pig(tag="{NoAI:1b,Invulnerable:1b,PersistenceRequired:1b}")
    setting("killaura", "entities", "pig")
    only("killaura")
    wait(10)
    ctl("screenshot", "/tmp/targethud.png")
    time.sleep(2)
    only()
    from PIL import Image
    img = Image.open("/tmp/targethud.png").convert("RGB")
    w, h = img.size
    # The box sits right of and below the crosshair: count the health bar's green and the box's dark fill.
    region = img.crop((w // 2 + 20, h // 2 + 20, w // 2 + 20 + 260, h // 2 + 20 + 80))
    green = sum(1 for px in region.getdata() if px[1] > 110 and px[1] - max(px[0], px[2]) > 60)
    return green > 150, f"{green} health-bar pixels beside the crosshair"


@test("Kill Aura (Anti bot, Ignore teams, friends)")
def _():
    if not CTL2:
        return None, "needs a second player"
    bot_respawned()
    cmd("gamemode survival Bot1")

    def bot_hurt(seconds):
        # Resistance IV takes 80% off, so Bot1 lives through 3 s of sword hits and its health says what they did.
        cmd("effect give Bot1 minecraft:instant_health 1 10 true", "effect give Bot1 minecraft:resistance 10 3 true",
            f"tp Bot1 {X0+0.5} {Y+1} {Z0+2.0}")
        wait(seconds * 20)
        return 20 - ctl2("state")["player"]["health"]

    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    setting("killaura", "entities", "player")
    setting("killaura", "anti bot", "true")
    only("killaura")
    real = bot_hurt(3)
    only()
    say(".friend add Bot1")
    only("killaura")
    friend = bot_hurt(3)
    only()
    say(".friend remove Bot1")
    # Friendly fire on: Kill Aura leaves a teammate it could not hurt anyway. You join as @s: single player names you
    # by its own player name, not Tester.
    cmd("team add sunsettest", "team modify sunsettest friendlyFire true", "team join sunsettest @s", "team join sunsettest Bot1")
    setting("killaura", "ignore teams", "true")
    only("killaura")
    spared = bot_hurt(3)
    setting("killaura", "ignore teams", "false")
    hit = bot_hurt(3)
    only()
    cmd("team remove sunsettest", "gamemode spectator Bot1", "gamerule natural_health_regeneration true")
    return real > 0 and friend == 0 and spared == 0 and hit > 0, f"player in the list hurt {real}, as a friend {friend}, as a teammate {spared}, teammate without Ignore teams {hit}"


def corner(name):
    """The bottom-left quarter of a screenshot, where the HUD places the radar: its pixels of the radar's dark blue
    and its near-white ones."""
    from PIL import Image
    path = f"/tmp/{name}.png"
    ctl("screenshot", path)
    time.sleep(2)
    img = Image.open(path).convert("RGB")
    w, h = img.size
    region = img.crop((0, h // 2, w // 2, h))
    px = list(region.getdata())
    return sum(1 for p in px if max(p) < 80 and p[2] - p[0] >= 8), sum(1 for p in px if min(p) > 230)


@test("Radar")
def _():
    reset("creative")
    ctl("look", "0", "0")
    # The chat sits over the radar's corner: shrink it away for the pictures.
    ctl("options-set", "chatScale", "0")
    wait(10)
    plain = corner("radar_off")
    only("radar")
    wait(10)
    shown = corner("radar_on")
    note = f"{plain[0]} dark blue pixels in the bottom-left quarter without the radar, {shown[0]} with it"
    ok = shown[0] > plain[0] + 5000
    if CTL2:
        # A second player 10 blocks off: a white dot on the radar; 100 blocks off (past Range 64): none.
        cmd("gamemode creative Bot1")
        cmd(f"tp Bot1 {X0 + 10.5} {Y + 1} {Z0 + 0.5}")
        wait(20)
        near = corner("radar_bot_near")
        cmd(f"tp Bot1 {X0 + 100.5} {Y + 1} {Z0 + 0.5}")
        wait(20)
        far = corner("radar_bot_far")
        ok = ok and near[1] > far[1] + 10
        note += f"; white pixels with Bot1 10 blocks off {near[1]}, 100 off {far[1]}"
    only()
    ctl("options-set", "chatScale", "1")
    return ok, note


@test("Crystal Optimizer")
def _():
    # Its near side 2.5 blocks from your eyes, inside the 3 blocks a server lets you hit from.
    cmd(f"summon end_crystal {X0+0.5} {Y+1} {Z0+4.0} {{ShowBottom:0b}}")
    wait(20)
    only("crystaloptimizer")
    ctl("look", "0", "10")
    wait(3)
    ctl("mouse", "left", "click")
    wait(10)
    done = check("execute unless entity @e[type=end_crystal]")
    return done, "crystal hit and gone" if done else "crystal still there"


# ---- Aim Assist (Duelkit's): real mouse motion (mouse-move), and for moving targets the probe's frames

def session_dir():
    with open(os.path.join(os.path.dirname(CTL), "session")) as f:
        return f.read().strip()


def probe_path():
    path = os.environ.get("PROBE") or os.path.join(session_dir(), "probe.log")
    return path if os.path.exists(path) else None


def world_dir():
    if os.environ.get("WORLD"):
        return os.environ["WORLD"]
    if ctl("status")["server"]["address"] == "local":
        return os.path.join(session_dir(), "home", "saves", ctl("worlds")["worlds"][0]["folder"])
    return os.path.expanduser("~/hzmeteor/srv/world")


def aim_pack():
    """A data pack that, every tick, steps husks tagged aim_e 0.1 blocks east and aim_w 0.1 west."""
    root = os.path.join(world_dir(), "datapacks", "sunset_verify")
    os.makedirs(os.path.join(root, "data", "sunset_verify", "function"), exist_ok=True)
    os.makedirs(os.path.join(root, "data", "minecraft", "tags", "function"), exist_ok=True)
    with open(os.path.join(root, "pack.mcmeta"), "w") as f:
        json.dump({"pack": {"description": "Sunset verify", "min_format": 121, "max_format": 121}}, f)
    with open(os.path.join(root, "data", "sunset_verify", "function", "tick.mcfunction"), "w") as f:
        f.write("execute as @e[tag=aim_e] at @s run tp @s ~0.1 ~ ~\nexecute as @e[tag=aim_w] at @s run tp @s ~-0.1 ~ ~\n")
    with open(os.path.join(root, "data", "minecraft", "tags", "function", "tick.json"), "w") as f:
        json.dump({"values": ["sunset_verify:tick"]}, f)
    cmd("reload")
    wait(20)


def probe_mark():
    return os.path.getsize(probe_path())


class Frame:
    def __init__(self, w):
        self.t, self.tick, self.yaw, self.pitch, self.alpha = float(w[0]), int(w[1]), float(w[2]), float(w[3]), float(w[4])
        self.eye = tuple(map(float, w[5:8]))
        self.mobs = {int(w[i]): (tuple(map(float, w[i + 1:i + 4])), float(w[i + 4]), float(w[i + 5])) for i in range(8, len(w) - 5, 6)}


def probe_frames(mark):
    with open(probe_path()) as f:
        f.seek(mark)
        lines = f.read().split("\n")[:-1]
    # The frame lines start with the time; the wire probe's "send" lines share the file.
    return [Frame(line.split()) for line in lines if line[:1].isdigit()]


def wrap(d):
    return (d + 180.0) % 360.0 - 180.0


def view(yaw, pitch):
    y, p = math.radians(yaw), math.radians(pitch)
    return (-math.sin(y) * math.cos(p), -math.sin(p), math.cos(y) * math.cos(p))


def angle(a, b):
    n = math.sqrt(sum(x * x for x in a)) * math.sqrt(sum(x * x for x in b))
    return math.degrees(math.acos(max(-1.0, min(1.0, sum(x * y for x, y in zip(a, b)) / max(n, 1e-12)))))


def off_centre(f, eid):
    """Degrees from the crosshair to the middle of the mob's box, where the frame draws it."""
    c = f.mobs[eid][0]
    return angle(view(f.yaw, f.pitch), [c[i] - f.eye[i] for i in range(3)])


def aim_gap(f, eid, inset=0.15):
    """Duelkit's gap: degrees from the crosshair to the nearest point of the box shrunk by the inset."""
    c, hw, hh = f.mobs[eid]
    half = (hw - inset, hh - inset, hw - inset)
    look = view(f.yaw, f.pitch)
    d = math.sqrt(sum((c[i] - f.eye[i]) ** 2 for i in range(3)))
    near = [min(max(f.eye[i] + look[i] * d, c[i] - abs(half[i])), c[i] + abs(half[i])) for i in range(3)]
    return angle(look, [near[i] - f.eye[i] for i in range(3)])


def aim_defaults():
    say(".reset aimassist")
    on.discard("aimassist")


def aim_setup(dx=0.0, dz=3.0, dy=0.0, weapon=True, mode="survival"):
    """Aim Assist at its defaults (off), a sword in hand (or nothing), one NoAI husk at (dx, dy, dz) from your
    feet; its id."""
    reset(mode)
    aim_defaults()
    cmd("difficulty easy")
    if weapon:
        slot(0, "diamond_sword", 1)
    ctl("key", "1", "tap")
    cmd(f"summon husk {X0 + 0.5 + dx} {Y + 1 + dy} {Z0 + 0.5 + dz} {HUSK}")
    wait(10)
    return next(e["id"] for e in ctl("entities")["entities"] if e.get("type") == "minecraft:husk")


def aim_view():
    s = ctl("status")
    return s["yaw"], s["pitch"]


def face(eid, yaw_off=0.0):
    """Looks at the mob's middle, then `yaw_off` degrees to its side (negative: it is on your right)."""
    ctl("look-at-entity", str(eid))
    wait(2)
    yaw, pitch = aim_view()
    ctl("look", str(yaw + yaw_off), str(pitch))
    wait(2)


def mouse_turns(units, n):
    """The yaw each of n mouse moves of `units` turned you by, Aim Assist included."""
    turns = []
    yaw = aim_view()[0]
    for _ in range(n):
        now = ctl("mouse-move", str(units), "0")["yaw"]
        turns.append(wrap(now - yaw))
        yaw = now
    return turns


def husk_box(eid):
    e = next(e for e in ctl("entities")["entities"] if e["id"] == eid)
    x, y, z = e["pos"]
    return (x - 0.3, y, z - 0.3), (x + 0.3, y + 1.95, z + 0.3)


def on_box(lo, hi):
    """Whether the crosshair is over the box (a ray from the eyes; you stand still)."""
    s = state()
    p = s["player"]["position"]
    eye = (p[0], p[1] + 1.62, p[2])
    look = view(*aim_view())
    t0, t1 = 0.0, 1e9
    for i in range(3):
        if abs(look[i]) < 1e-12:
            if not lo[i] <= eye[i] <= hi[i]:
                return False
            continue
        a, b = (lo[i] - eye[i]) / look[i], (hi[i] - eye[i]) / look[i]
        t0, t1 = max(t0, min(a, b)), min(t1, max(a, b))
    return t0 <= t1


def steps_onto(eid, units=10, limit=40):
    """Mouse moves of `units` until the crosshair is on the mob."""
    lo, hi = husk_box(eid)
    for i in range(1, limit + 1):
        ctl("mouse-move", str(units), "0")
        if on_box(lo, hi):
            return i
    return limit + 1


def first_turn(eid, yaw_off, units=10, mods=("aimassist",)):
    """With the mob `yaw_off` degrees to the side, the turn of one mouse move toward it: (plain, with mods).
    The modules go on only once you face that way, so nothing is a target kept from before."""
    only()
    face(eid, yaw_off)
    plain = mouse_turns(units, 1)[0]
    face(eid, yaw_off)
    only(*mods)
    mine = mouse_turns(units, 1)[0]
    only()
    return plain, mine


def aim_done():
    aim_defaults()
    only()
    cmd("difficulty peaceful")


@test("Aim Assist (pull)")
def _():
    eid = aim_setup()
    only()
    face(eid, -12)
    plain = steps_onto(eid)
    face(eid, -12)
    only("aimassist")
    pulled = steps_onto(eid)
    first = first_turn(eid, -12)
    # Away from it: slowed by Stickiness while on it (by less over the last degree), never held back beyond that.
    face(eid)
    only("aimassist")
    away = mouse_turns(-10, 6)
    only()
    face(eid)
    free = mouse_turns(-10, 6)
    aim_done()
    step = abs(free[0])
    slowed = all(step * 0.75 - 0.01 <= abs(t) <= step + 0.01 for t in away) and abs(away[0]) < step * 0.8
    note = (f"onto a husk 12 deg off in {plain} mouse moves of 10 without, {pulled} with (first move {first[0]:.2f} -> {first[1]:.2f} deg); "
            f"moving off it: {', '.join(f'{abs(t):.2f}' for t in away)} deg a move (plain {step:.2f})")
    return pulled < plain and first[1] > first[0] * 1.3 and slowed, note


@test("Aim Assist (leaves you alone)")
def _():
    notes, ok = [], True

    def case(name, plain_mine, pulled=False):
        nonlocal ok
        plain, mine = plain_mine
        good = (mine > plain + 0.2) if pulled else abs(mine - plain) < 0.01
        ok = ok and good
        notes.append(f"{name} {plain:.2f}->{mine:.2f}")

    eid = aim_setup()
    case("in front (pulls)", first_turn(eid, -8), pulled=True)
    case("outside the cone", first_turn(eid, -25))
    cmd("effect give @e[type=husk] minecraft:invisibility 60 0 true")
    case("invisible", first_turn(eid, -8))
    eid = aim_setup()
    # Its raycast is the collision shape's (ClipContext.Block.COLLIDER): cobwebs let it through, stone does not.
    cmd(f"fill {X0-3} {Y+1} {Z0+2} {X0+3} {Y+3} {Z0+2} minecraft:cobweb")
    case("behind cobwebs (pulls)", first_turn(eid, -8), pulled=True)
    cmd(f"fill {X0-3} {Y+1} {Z0+2} {X0+3} {Y+3} {Z0+2} minecraft:stone")
    case("behind a wall", first_turn(eid, -8))
    unwall()
    eid = aim_setup(dz=7.0)
    case("out of range", first_turn(eid, -5))
    eid = aim_setup(weapon=False)
    case("no weapon", first_turn(eid, -8))
    setting("aimassist", "only with a weapon", "false")
    case("no weapon, Only with a weapon off (pulls)", first_turn(eid, -8), pulled=True)
    eid = aim_setup()
    setting("aimassist", "hostiles", "false")
    case("Hostiles off", first_turn(eid, -8))
    # Held still on a target that does not move, nothing turns you.
    setting("aimassist", "hostiles", "true")
    only("aimassist")
    face(eid, -2)
    before = aim_view()
    wait(40)
    after = aim_view()
    still = abs(wrap(after[0] - before[0])) < 1e-4 and abs(after[1] - before[1]) < 1e-4
    ok = ok and still
    notes.append(f"held still 2 s beside it: turned {abs(wrap(after[0] - before[0])):.4f} deg")
    aim_done()
    return ok, "; ".join(notes)


@test("Aim Assist (settings)")
def _():
    notes, ok = [], True

    def pair(name, a, b, want):
        nonlocal ok
        good = want(a, b)
        ok = ok and good
        notes.append(f"{name}: {a:.2f} vs {b:.2f}")

    eid = aim_setup(dz=5.0)
    setting("aimassist", "range", "6")
    wide = first_turn(eid, -6)[1]
    setting("aimassist", "range", "4")
    narrow = first_turn(eid, -6)[1]
    pair("Range 6 vs 4 (husk 5 away, first move)", wide, narrow, lambda a, b: a > b + 0.2)
    eid = aim_setup()
    setting("aimassist", "cone", "30")
    small = first_turn(eid, -22)[1]
    setting("aimassist", "cone", "60")
    big = first_turn(eid, -22)[1]
    pair("Cone 30 vs 60 (husk 22 deg off)", small, big, lambda a, b: b > a + 0.2)
    setting("aimassist", "cone", "30")
    plain, low = first_turn(eid, -12)
    setting("aimassist", "pull", "1")
    high = first_turn(eid, -12)[1]
    pair("Pull 0.4 vs 1 (as a share of the plain move)", low / plain - 1, high / plain - 1, lambda a, b: abs(a - 0.4) < 0.05 and abs(b - 1.0) < 0.05)
    setting("aimassist", "pull", "0.4")

    def away(stick):
        setting("aimassist", "stickiness", stick)
        face(eid)
        only("aimassist")
        t = abs(mouse_turns(-10, 1)[0])
        only()
        return t

    pair("Stickiness 0.25 vs 0.6 (first move off it, share kept)", away("0.25") / plain, away("0.6") / plain,
         lambda a, b: abs(a - 0.75) < 0.03 and abs(b - 0.4) < 0.03)
    setting("aimassist", "stickiness", "0.25")
    # Five out and five up: 6.5 away in a straight line, under 5 across.
    cmd(f"setblock {X0} {Y+5} {Z0+5} minecraft:stone")
    eid = aim_setup(dz=5.0, dy=5.0)
    sphere = first_turn(eid, -5)[1]
    setting("aimassist", "range shape", "Column")
    column = first_turn(eid, -5)[1]
    setting("aimassist", "column height", "2")
    capped = first_turn(eid, -5)[1]
    pair("Range shape Sphere vs Column (husk 5 across, 5 up)", sphere, column, lambda a, b: b > a + 0.2)
    pair("Column height 0 vs 2", column, capped, lambda a, b: a > b + 0.2)
    cmd(f"setblock {X0} {Y+5} {Z0+5} minecraft:air")
    aim_done()
    return ok, "; ".join(notes)


def track_run(eid, frames):
    """What the frames of a husk walking past show: degrees off its middle at the start, at the end and at most;
    the view's turn per frame over the husk's (its median share, and how many frames stalled while it moved)."""
    moving = [i for i in range(1, len(frames)) if abs(frames[i].mobs[eid][0][0] - frames[i - 1].mobs[eid][0][0]) > 1e-6]
    if len(moving) < 10:
        return None
    win = frames[moving[0] - 1:moving[-1] + 1]
    errs = [off_centre(f, eid) for f in win]
    shares, stalls, still = [], 0, 0
    for a, b in zip(win, win[1:]):
        ca, cb = a.mobs[eid][0], b.mobs[eid][0]
        dt = math.degrees(math.atan2(-(cb[0] - b.eye[0]), cb[2] - b.eye[2]) - math.atan2(-(ca[0] - a.eye[0]), ca[2] - a.eye[2]))
        dy = wrap(b.yaw - a.yaw)
        if abs(dt) < 1e-5:
            still += 1
            continue
        shares.append(dy / dt)
        stalls += abs(dy) < 0.1 * abs(dt)
    shares.sort()
    fps = (len(win) - 1) / max(win[-1].t - win[0].t, 1e-6)
    return {"start": errs[0], "end": errs[-1], "max": max(errs), "share": shares[len(shares) // 2] if shares else 0.0,
            "stalled": stalls / max(len(shares), 1), "still": still / (len(win) - 1), "fps": fps, "frames": len(win)}


def track(tracking, start_dx=-1.5, tag="aim_e", wall=False):
    eid = aim_setup(dx=start_dx)
    if wall:
        cmd(f"fill {X0-4} {Y+1} {Z0+2} {X0+4} {Y+3} {Z0+2} minecraft:stone")
    setting("aimassist", "tracking", tracking)
    only("aimassist")
    face(eid)
    wait(5)
    mark = probe_mark()
    cmd(f"tag @e[type=husk] add {tag}")
    wait(30)
    cmd(f"tag @e[type=husk] remove {tag}")
    wait(5)
    frames = [f for f in probe_frames(mark) if eid in f.mobs]
    only()
    if wall:
        cmd(f"fill {X0-4} {Y+1} {Z0+2} {X0+4} {Y+3} {Z0+2} minecraft:air")
    return track_run(eid, frames), frames


@test("Aim Assist (tracking)")
def _():
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    aim_pack()
    runs = {}
    for t in ("1", "0.5", "0"):
        runs[t] = track(t)[0]
    walled, frames = track("1", wall=True)
    turned = max(abs(wrap(f.yaw - frames[0].yaw)) for f in frames) if frames else 0
    aim_done()
    if not all(runs.values()):
        return False, "the husk did not move (data pack not loaded?)"
    one, half, off = runs["1"], runs["0.5"], runs["0"]
    note = "; ".join(f"Tracking {t}: off its middle {r['start']:.2f} -> {r['end']:.2f} deg (max {r['max']:.2f}), view turned {r['share']:.2f} of the husk's turn per frame, {r['stalled']*100:.0f}% frames stalled, husk still in {r['still']*100:.0f}% of frames, {r['fps']:.0f} fps"
                     for t, r in runs.items())
    note += f"; behind a wall the view turned {turned:.3f} deg"
    ok = (one["max"] < 1.0 and 0.85 < one["share"] < 1.15 and one["stalled"] < 0.05
          and off["end"] > 10 and half["end"] < off["end"] - 3 and turned < 1e-3)
    return ok, note


@test("Aim Assist (mace dive)")
def _():
    if not probe_path():
        return None, "needs the probe (tools/probe)"

    def dive(on_):
        eid = aim_setup(dz=4.0, mode="creative")
        setting("aimassist", "mace dive", "true" if on_ else "false")
        only("aimassist")
        cmd(f"tp @s {X0 + 0.5} {Y + 13} {Z0 + 0.5} 0 30")
        mark = probe_mark()
        wait(40)
        frames = [f for f in probe_frames(mark) if eid in f.mobs]
        only()
        # The frames of the fall past the 3 blocks it waits for, until landing: how far off the hitbox the crosshair
        # starts, how long it takes to get within a degree, and the share of the frames after that within a degree.
        fall = [f for f in frames if f.eye[1] < Y + 13 + 1.62 - 3.2 and f.eye[1] > Y + 2.7]
        if not fall:
            return None
        gaps = [aim_gap(f, eid) for f in fall]
        on_ = next((i for i, g in enumerate(gaps) if g < 1.0), len(gaps))
        after = gaps[on_:]
        return {"start": gaps[0], "time": fall[min(on_, len(fall) - 1)].t - fall[0].t, "kept": sum(g < 1.0 for g in after) / max(len(after), 1),
                "near": sum(g < 1.0 for g in gaps) / len(gaps), "frames": len(fall)}

    with_dive = dive(True)
    without = dive(False)
    aim_done()
    if not with_dive or not without:
        return False, "no frames of the fall"
    note = (f"falling onto a husk 4 blocks out ({with_dive['frames']} frames): {with_dive['start']:.1f} deg off the hitbox, on it within "
            f"{with_dive['time']:.2f} s and on it for {with_dive['kept']*100:.0f}% of the fall after that with Mace dive; without it "
            f"{without['start']:.1f} deg off and on it in {without['near']*100:.0f}% of the frames")
    return with_dive["start"] > 5.0 and with_dive["time"] < 0.5 and with_dive["kept"] > 0.9 and without["near"] < 0.3, note


@test("Aim Assist (players)")
def _():
    if not CTL2:
        return None, "needs a second player"
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset("survival")
    aim_defaults()
    slot(0, "diamond_sword", 1)
    ctl("key", "1", "tap")
    cmd("gamemode survival Bot1", f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 3.5} 180 0")
    wait(20)
    # The entity list leaves players out: its id is the crosshair's.
    ctl("look", "0", "10")
    wait(2)
    bot = ctl("looking-at").get("entity", {}).get("id")
    if bot is None:
        return False, "Bot1 not under the crosshair"

    def strafe():
        cmd(f"tp Bot1 {X0 + 2.0} {Y + 1} {Z0 + 3.5} 180 0")
        wait(20)
        only("aimassist")
        ctl("look-at-entity", str(bot))
        wait(5)
        mark = probe_mark()
        # Facing you, sneaking to its left: east to west across your view at sneaking speed.
        ctl2("key", "shift", "down")
        ctl2("key", "a", "down")
        wait(30)
        ctl2("key", "a", "up")
        ctl2("key", "shift", "up")
        wait(5)
        frames = [f for f in probe_frames(mark) if bot in f.mobs]
        only()
        return track_run(bot, frames)

    setting("aimassist", "tracking", "1")
    real = strafe()
    say(".friend add Bot1")
    friend = strafe()
    say(".friend remove Bot1")
    aim_done()
    if not real or not friend:
        return False, "Bot1 did not move"
    note = (f"Bot1 sneak-strafing 3 blocks away, Tracking 1: off its middle {real['start']:.2f} -> {real['end']:.2f} deg (max {real['max']:.2f}), "
            f"view turned {real['share']:.2f} of its turn per frame, {real['stalled']*100:.0f}% frames stalled; as a friend: {friend['start']:.2f} -> {friend['end']:.2f} deg")
    return real["max"] < 1.5 and real["stalled"] < 0.05 and friend["end"] > 5.0, note




@test("One move packet a tick (Nuker, Kill Aura, Scaffold)")
def _():
    # The probe logs each move packet and each end of a client tick on the wire: never two moves in one tick, and no
    # kick, with the three that turn the player's look for the server on at once (Rotate on).
    if not probe_path() or ctl("status")["server"]["address"] == "local":
        return None, "servers only, with the probe (in single player its log would hold the built-in server's packets too)"
    # Dirt in front for Nuker, a husk behind for Kill Aura, and the platform's edge behind that for Scaffold.
    reset(z=-12.5)
    cmd(f"fill {X0-4} {Y+1} {Z0-10} {X0+4} {Y+2} {Z0-8} minecraft:dirt", f"summon husk {X0+0.5} {Y+1} {Z0-15.5} {HUSK}")
    slot(0, "stone", 64)
    slot(1, "diamond_sword")
    wait(10)
    mark = probe_mark()
    only("nuker", "killaura", "scaffold")
    wait(40)
    walk(50, "s")
    walk(50, "w")
    only()
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        sent = [line.split()[1] for line in f if line.startswith("send ")]
    ticks, most, moves, now = 0, 0, 0, 0
    for kind in sent:
        if kind == "tickend":
            ticks, most, now = ticks + 1, max(most, now), 0
        else:
            moves, now = moves + 1, now + 1
    alive = bool(state().get("player"))
    broken = 54 - count(X0 - 4, Y + 1, Z0 - 10, X0 + 4, Y + 2, Z0 - 8, "dirt")
    placed = count(X0 - 2, Y, Z0 - 30, X0 + 2, Y, Z0 - 17, "stone")
    return ticks > 50 and most <= 1 and alive, (f"{moves} move packets over {ticks} ticks, at most {most} in a tick; Nuker broke {broken} dirt, "
                                                f"Scaffold placed {placed} past the edge; still in the game: {alive}")


def order_stats(mark):
    """From the probe's order lines since `mark`: hits (attacks and spear stabs), the hits that went after their tick's
    move packet, the ticks, and the most move packets in a tick."""
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        sent = [line.split()[1] for line in f if line.startswith("order ")]
    hits = late = ticks = most = moves = 0
    for kind in sent:
        if kind == "tickend":
            ticks, most, moves = ticks + 1, max(most, moves), 0
        elif kind == "move":
            moves += 1
        else:
            hits += 1
            late += moves > 0
    return hits, late, ticks, most


@test("Hits before the move packet (vanilla order)")
def _():
    # Vanilla sends a hit with the tick's input, before that tick's move packet. Against Bot1 (else a pig with 1000
    # health), put back in place every 2 ticks (Horizon's server ignores knockback resistance and knocks it out of
    # reach), strafing left and right so every tick has a move packet: Kill Aura, Trigger Bot and your own clicks, each
    # without and with Criticals (Packet). The probe logs the client's own hits, move packets and tick ends in the
    # order they go: no hit may follow its tick's move packet, and no tick has two move packets.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    if CTL2:
        bot_ready(2.5, 180)
        target = next(e["id"] for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:player")
        kind, health = "Bot1", bot_health
        setting("killaura", "entities", "player")
    else:
        target = pig(2.5, tag=TOUGH_PIG)
        kind, health = "a pig", lambda: mobs("pig")[0]["health"]
        setting("killaura", "entities", "pig")
        setting("triggerbot", "passive", "true")
    setting("criticals", "mode", "Packet")

    def run(mods, clicks):
        back = f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 3.0} 180 0"
        if CTL2:
            # Healed for each run: Horizon's server takes no max health attribute, so Bot1 has 20.
            bot_respawned()
            cmd("effect give Bot1 minecraft:instant_health 1 28 true", "effect give Bot1 minecraft:resistance 600 3 true")
        cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5}", *((back,) if CTL2 else ()))
        wait(5)
        only(*mods)
        before, mark = health(), probe_mark()
        for step in range(40):
            if step % 5 == 0:
                ctl("key", "ad"[step // 5 % 2], "down")
                ctl("key", "da"[step // 5 % 2], "up")
            if clicks or "triggerbot" in mods:
                ctl("look-at-entity", str(target))
            if clicks and step % 7 == 0:
                ctl("mouse", "left", "click")
            # The command takes the 2 ticks of the step.
            if CTL2:
                cmd(back)
            else:
                wait(2)
        ctl("key", "a", "up")
        ctl("key", "d", "up")
        only()
        wait(5)
        return (*order_stats(mark), round(before - health(), 1))

    runs = {}
    for name, mods, clicks in (("Kill Aura", ("killaura",), False), ("Trigger Bot", ("triggerbot",), False), ("click", (), True)):
        runs[name] = run(mods, clicks)
        runs[name + " + Criticals"] = run(mods + ("criticals",), clicks)
    if CTL2:
        bot_done()
    ok = all(hits > 0 and late == 0 and most <= 1 and ticks > 60 for hits, late, ticks, most, _ in runs.values())
    return ok, f"against {kind}: " + "; ".join(
        f"{name} {hits} hits, {late} after the move packet, {lost} health lost, at most {most} moves a tick"
        for name, (hits, late, ticks, most, lost) in runs.items())

# ---- sweeps: every mode of every module that talks to the server, through the same short stretch of play, and every
# render mode once in single player. On a server each one counts, from the probe's wire log, the move packets in a
# tick (never two) and the times the server put the player back (a teleport accepted); a kick fails it with the
# server's reason. The measured tests above check what the modules do; these check that the server takes it.

def wire_stats(mark):
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        sent = [line.split()[1] for line in f if line.startswith("send ")]
    ticks, most, now, teleports = 0, 0, 0, 0
    for kind in sent:
        if kind == "tickend":
            ticks, most, now = ticks + 1, max(most, now), 0
        elif kind == "teleport":
            teleports += 1
        else:
            now += 1
    return ticks, most, teleports


HOTBAR = [("diamond_sword", 1), ("obsidian", 64), ("cobblestone", 64), ("bread", 16), ("diamond_pickaxe", 1),
          ("water_bucket", 1), ("ender_pearl", 16), ("totem_of_undying", 1), ("experience_bottle", 64)]


def sweep_stage():
    reset()
    cmd(f"fill {X0-3} {Y+1} {Z0+5} {X0+3} {Y+2} {Z0+5} minecraft:stone", f"setblock {X0+2} {Y+1} {Z0+2} minecraft:stone",
        f"summon husk {X0-1.5} {Y+1} {Z0+2.5} {HUSK}", f"summon cow {X0+2.5} {Y+1} {Z0+3.5} {NOAI}",
        "item replace entity @s inventory.0 with minecraft:end_crystal 16", "item replace entity @s inventory.1 with minecraft:golden_apple 8",
        "item replace entity @s inventory.2 with minecraft:cobweb 16", "item replace entity @s armor.chest with minecraft:iron_chestplate")
    for i, (item, n) in enumerate(HOTBAR):
        slot(i, item, n)
    ctl("key", "1", "tap")
    wait(5)


def sweep_play():
    """About six seconds of play: walk to the wall, jump, strafe, sneak, dig, use, hold jump, walk back."""
    walk(20)
    for _ in range(2):
        ctl("key", "space", "tap")
        wait(10)
    held_keys(("a",), 10)
    held_keys(("d",), 10)
    held_keys(("shift",), 10)
    ctl("mouse", "left", "down")
    wait(20)
    ctl("mouse", "left", "up")
    ctl("mouse", "right", "down")
    wait(10)
    ctl("mouse", "right", "up")
    held_keys(("space",), 20)
    held_keys(("s",), 15)
    wait(10)


def sweep(keys, sets):
    sweep_stage()
    for key, k, v in sets:
        setting(key, k, v)
    on_server = SERVER is not None and probe_path() is not None
    mark = probe_mark() if on_server else 0
    t0 = state()["ticks"]
    only(*keys)
    sweep_play()
    only()
    st = state()
    if not st.get("player"):
        return False, f"kicked or gone: {json.dumps(ctl('screen'))[:200]}"
    moved = st["ticks"] - t0
    if not on_server:
        return moved > 100, f"{moved} ticks played, still in the game"
    ticks, most, teleports = wire_stats(mark)
    return most <= 1 and teleports == 0, f"{ticks} ticks: at most {most} move packet(s) in a tick, put back {teleports} time(s), not kicked"


def light(fn):
    fn.light = True
    return fn


SWEEPS = [
    (("flight",), [("flight", "mode", m), ("flight", "anti kick mode", a)]) for m in ("Abilities", "Velocity") for a in ("Packet", "Normal", "None")
] + [
    (("speed",), [("speed", "mode", m)]) for m in ("Vanilla", "Strafe")
] + [
    (("nofall",), [("nofall", "mode", m)]) for m in ("Packet", "No ground", "Air place", "Place")
] + [
    (("step",), [("step", "active when", m)]) for m in ("Always", "Sneaking", "Not sneaking")
] + [
    (("guimove",), [("guimove", "screens", m)]) for m in ("All menus", "Inventory")
] + [
    (("killaura",), [("killaura", "rotate", m)]) for m in ("Always", "On hit", "None")
] + [
    (("killaura", "criticals"), [("killaura", "rotate", m)]) for m in ("Always", "None")
] + [
    (("killaura",), [("killaura", "attack when holding", "All")]),
] + [
    (("surround",), [("surround", "center", m)]) for m in ("Never", "On activate", "Always", "Incomplete")
] + [
    (("selftrap",), [("selftrap", "top mode", m)]) for m in ("Top", "Anti face place", "Full", "None")
] + [
    (("selfweb",), [("selfweb", "mode", m)]) for m in ("Normal", "Smart")
] + [
    (("burrow",), [("burrow", "block", m)]) for m in ("Obsidian", "Held")
] + [
    (("offhand",), [("offhand", "item", m)]) for m in ("Crystal", "Totem", "Gapple", "Enchanted gapple")
] + [
    (("autoexp",), [("autoexp", "mode", m)]) for m in ("Both", "Armor", "Hands")
] + [
    (("autototem",), [("autototem", "mode", m)]) for m in ("Smart", "Strict")
] + [
    (("fastuse",), [("fastuse", "mode", m)]) for m in ("All", "Some")
] + [
    (("speedmine",), [("speedmine", "mode", m)]) for m in ("Damage", "Normal", "Haste")
] + [
    (("antiafk",), [("antiafk", "spin mode", m)]) for m in ("Server", "Client")
] + [
    (("autoeat",), [("autoeat", "threshold mode", m)]) for m in ("Any", "Health", "Hunger", "Both")
] + [
    (("autotool",), [("autotool", "prefer", m)]) for m in ("Fortune", "None", "Silk touch")
] + [
    (("nuker",), [("nuker", "shape", m)]) for m in ("Sphere", "Cube", "Uniform cube")
] + [
    (("nuker",), [("nuker", "mode", m)]) for m in ("All", "Smash")
] + [
    (("nuker",), [("nuker", "sort mode", m)]) for m in ("None", "Furthest", "Top down")
] + [
    (("nuker",), [("nuker", "packet mine", "true")]),
] + [
    (("liquidfiller",), [("liquidfiller", "place in", m)]) for m in ("Both", "Water", "Lava")
] + [
    (("spawnproofer",), [("spawnproofer", "mode", m)]) for m in ("Both", "Always", "Potential")
] + [
    (("anti" + k,), [("anti" + k, "sprint" if k == "hunger" else "jump", "true")]) for k in ("hunger",)
] + [
    ((k,), []) for k in ("velocity", "blink", "glide", "longjump", "highjump", "airjump", "spider", "parkour", "safewalk", "sprint",
                         "autowalk", "reversestep", "anchor", "fastclimb", "autojump", "sneak", "noslow", "nojumpdelay", "antivoid",
                         "elytraboost", "chestswap", "scaffold", "timer", "fastplace", "breakdelay", "autoclicker", "autoweapon",
                         "triggerbot", "autoarmor", "autoreplenish", "autogap", "airplace", "reach", "autotrap", "autoweb",
                         "holefiller", "aimassist", "bowspam", "crystaloptimizer", "flamethrower", "autoshearer", "automount",
                         "autosteal", "inventorycleaner", "middleclickextra", "autofish")
]

for _keys, _sets in SWEEPS:
    _label = " + ".join(_keys) + (" (" + ", ".join(f"{k}: {v}" for _, k, v in _sets) + ")" if _sets else "")
    test(f"Sweep: {_label}")(light(lambda _k=_keys, _s=_sets: sweep(_k, _s)))

RENDER_MODES = [("esp", ["3D box", "2D box", "Outline", "Potato"]), ("tracers", ["Lines", "Potato"]), ("nametags", ["Full", "Potato"]),
                ("storageesp", ["3D box", "2D box", "Outline", "Potato"]), ("blockesp", ["Box", "Outline", "Potato"]),
                ("holeesp", ["3D box", "2D box", "Outline", "Potato"]), ("breadcrumbs", ["Line", "Potato"]), ("waypoints", ["Full", "Potato"]),
                ("trajectories", ["Line", "Potato"]), ("lightoverlay", ["Cross", "Potato"]), ("tunnelesp", ["Box", "Outline", "Potato"]),
                ("voidesp", ["Box", "Outline", "Potato"]), ("radar", ["Round", "Potato"]), ("logoutspots", ["Box", "Outline", "Potato"])]


def render_mode(key, mode):
    if SERVER:
        return None, "client only: single player"
    sweep_stage()
    cmd(f"setblock {X0+1} {Y+1} {Z0+3} minecraft:chest")
    setting(key, "mode", mode)
    only(key)
    a = ctl("status")
    wait(40)
    b = ctl("status")
    only()
    fps = (b["frames"] - a["frames"]) / max(b["uptime_seconds"] - a["uptime_seconds"], 1e-6)
    return b["ticks"] - a["ticks"] >= 38 and fps > 5, f"drawn for 2 s at {fps:.0f} fps"


# ---- Crystal Aura, Anchor Aura, Crystal Macro and Anchor Macro (Rust's PR #16), against Bot1 (CTL2) on a server.
# The arena is obsidian (a crystal needs it under, and nothing round it blows away), Bot1 stands 3 blocks south with a
# thousand health and no knockback, so every hit can be counted; you wear netherite with Blast Protection IV.

ARMOR = [("head", "netherite_helmet"), ("chest", "netherite_chestplate"), ("legs", "netherite_leggings"),
         ("feet", "netherite_boots")]


def blast_arena(bot=True):
    cmd(f"fill {X0-5} {Y-1} {Z0-4} {X0+5} {Y} {Z0+8} minecraft:obsidian", "difficulty normal",
        "gamerule natural_health_regeneration false", "gamerule fire_damage false")
    for part, item in ARMOR:
        cmd(f"item replace entity @s armor.{part} with minecraft:{item}[enchantments={{blast_protection:4}}]")
    if bot:
        cmd("gamemode survival Bot1", "attribute Bot1 minecraft:max_health base set 1000",
            "attribute Bot1 minecraft:knockback_resistance base set 1", "attribute Bot1 minecraft:explosion_knockback_resistance base set 1",
            f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 3.5}", "effect give Bot1 minecraft:instant_health 1 28 true")
        ctl2("release")
    wait(20)


def blast_done():
    cmd("difficulty peaceful", "gamerule natural_health_regeneration true", "gamerule fire_damage true",
        "attribute Bot1 minecraft:max_health base reset", "attribute @s minecraft:max_health base reset",
        "gamemode spectator Bot1", f"tp Bot1 {X0 + 0.5} {Y + 30} {Z0 + 60}",
        f"fill {X0-5} {Y-1} {Z0-4} {X0+5} {Y} {Z0+8} minecraft:stone", f"fill {X0-5} {Y+1} {Z0-4} {X0+5} {Y+4} {Z0+8} minecraft:air",
        "kill @e[type=minecraft:end_crystal]")


def bot_respawned():
    """Bot1 back from the death screen (a server keeps a player who died in an earlier run dead when they join), with
    PvP on (a single player world opened to LAN may have it off)."""
    if CTL2 and ctl2("state").get("screen") == "dead":
        ctl2("click-button", "Respawn")
        wait(20)
    cmd("gamerule pvp true", "gamerule natural_health_regeneration false")


def bot_health():
    p = ctl2("state").get("player") or {}
    return p.get("health", 0) + p.get("absorption", 0)


def own_health():
    p = state()["player"]
    return p["health"] + p.get("absorption", 0)


def crystals_left():
    """End crystals in the world (the kill command counts them, and clears them)."""
    cmd("kill @e[type=minecraft:end_crystal]")
    for line in ctl("chat-log", "3").get("chat", []):
        if line.startswith("Killed "):
            return int(line.split()[1])
        if "No entity was found" in line:
            return 0
    return None


def hand_crystal():
    """Damage Bot1 takes from an end crystal you break by hand beside it, with Sunset off: what the server deals a
    player for a crystal another player set off (Horizon's own server deals none)."""
    cmd(f"summon minecraft:end_crystal {X0 + 0.5} {Y + 1} {Z0 + 2.5} {{ShowBottom:0b}}", "effect give Bot1 minecraft:instant_health 1 28 true")
    wait(10)
    before = bot_health()
    ctl("look-at", str(X0 + 0.5), str(Y + 2), str(Z0 + 2.5))
    ctl("mouse", "left", "click")
    wait(20)
    lost = before - bot_health()
    if crystals_left():
        return None
    cmd("effect give Bot1 minecraft:instant_health 1 28 true", "effect give @s minecraft:instant_health 1 28 true")
    wait(10)
    return lost


def blast_run(module, items, ticks, health=None, totem=False):
    """`module` on for `ticks` with `items` (item, count) in the hotbar from slot 0 and a sword held in slot 8: what
    was used of each, damage Bot1 took, damage you took, totems popped, the most move packets in a tick, kept in the
    game, frames a second, and the slot held after it went off."""
    for i, (item, n) in enumerate(items):
        slot(i, item, n)
    slot(8, "diamond_sword")
    ctl("key", "9", "tap")
    if health:
        cmd(f"attribute @s minecraft:max_health base set {health}", "effect give @s minecraft:instant_health 1 28 true")
    if totem:
        cmd("item replace entity @s weapon.offhand with minecraft:totem_of_undying")
    wait(10)
    mark = probe_mark() if SERVER and probe_path() else None
    before_inv = ctl("inventory").get("counts", {})
    bot0, own0 = bot_health(), own_health()
    a = ctl("status")
    only(module)
    wait(ticks)
    only()
    b = ctl("status")
    wait(10)
    after_inv = ctl("inventory").get("counts", {})
    used = {item: before_inv.get(f"minecraft:{item}", 0) - after_inv.get(f"minecraft:{item}", 0) for item, _ in items}
    pops = int(totem and "minecraft:totem_of_undying" not in after_inv and offhand() != "minecraft:totem_of_undying")
    st = state()
    alive = st.get("screen") != "dead" and bool(st.get("player"))
    taken = own0 - (own_health() if alive else 0)
    most = wire_stats(mark)[1] if mark is not None else None
    fps = (b["frames"] - a["frames"]) / max(b["uptime_seconds"] - a["uptime_seconds"], 1e-6)
    return {"used": used, "dealt": bot0 - bot_health(), "taken": taken, "pops": pops, "alive": alive, "most": most,
            "fps": fps, "held": (held().get("item") or "").removeprefix("minecraft:"), "seconds": ticks / 20}


@test("Crystal Aura")
def _():
    if not CTL2:
        return None, "needs a second player (CTL2)"
    reset()
    blast_arena()
    control = hand_crystal()
    # Crystals per second, damage dealt and taken, with room in the self damage limit and the health for it.
    setting("crystalaura", "max self damage", "20")
    fast = blast_run("crystalaura", [("end_crystal", 64), ("obsidian", 64)], 200, health=200)
    ensure_alive()
    # At 20 health with a totem and the usual limit (8): it never pops you or kills you.
    setting("crystalaura", "max self damage", "8")
    cmd("effect give Bot1 minecraft:instant_health 1 28 true")
    safe = blast_run("crystalaura", [("end_crystal", 64), ("obsidian", 64)], 200, health=20, totem=True)
    ensure_alive()
    blast_done()
    rate = fast["used"]["end_crystal"] / fast["seconds"]
    # Where the server deals another player nothing for a crystal you set off (the control), only rate and safety count.
    dealt = fast["dealt"] > fast["taken"] or control == 0
    ok = (rate >= 1 and dealt and fast["alive"] and fast["most"] in (None, 0, 1)
          and safe["pops"] == 0 and safe["alive"] and safe["taken"] <= 8 * max(1, safe["used"]["end_crystal"])
          and fast["held"] == "diamond_sword" and fast["fps"] > 20)
    return ok, (f"a crystal broken by hand beside Bot1 costs it {control}; "
                f"{rate:.1f} crystals a second ({fast['used']['end_crystal']} placed, {fast['used']['obsidian']} obsidian), "
                f"Bot1 took {fast['dealt']:.0f}, you {fast['taken']:.0f} (200 health, limit 20); at 20 health with a totem "
                f"and limit 8: {safe['used']['end_crystal']} placed, Bot1 took {safe['dealt']:.0f}, you {safe['taken']:.0f}, "
                f"{safe['pops']} totems popped, alive {safe['alive']}; most move packets in a tick {fast['most']}; "
                f"{fast['fps']:.0f} fps; holding {fast['held']} after")


@test("Anchor Aura")
def _():
    if not CTL2:
        return None, "needs a second player (CTL2)"
    reset()
    blast_arena()
    control = hand_crystal()
    cmd("effect give @s minecraft:fire_resistance 120 0 true")
    setting("anchoraura", "max self damage", "20")
    fast = blast_run("anchoraura", [("respawn_anchor", 64), ("glowstone", 64)], 200, health=200)
    ensure_alive()
    setting("anchoraura", "max self damage", "6")
    cmd("effect give Bot1 minecraft:instant_health 1 28 true", "effect give @s minecraft:fire_resistance 120 0 true")
    safe = blast_run("anchoraura", [("respawn_anchor", 64), ("glowstone", 64)], 200, health=20, totem=True)
    ensure_alive()
    blast_done()
    rate = fast["used"]["respawn_anchor"] / fast["seconds"]
    dealt = fast["dealt"] > fast["taken"] or control == 0
    ok = (rate >= 0.5 and dealt and fast["alive"] and fast["most"] in (None, 0, 1)
          and safe["pops"] == 0 and safe["alive"] and fast["held"] == "diamond_sword" and fast["fps"] > 20)
    return ok, (f"a crystal broken by hand beside Bot1 costs it {control}; {rate:.1f} anchors a second ({fast['used']['respawn_anchor']} anchors, {fast['used']['glowstone']} "
                f"glowstone), Bot1 took {fast['dealt']:.0f}, you {fast['taken']:.0f} (200 health, limit 20); at 20 health "
                f"with a totem and limit 6: {safe['used']['respawn_anchor']} anchors, you {safe['taken']:.0f}, "
                f"{safe['pops']} totems popped, alive {safe['alive']}; most move packets in a tick {fast['most']}; "
                f"{fast['fps']:.0f} fps; holding {fast['held']} after")


def macro_run(module, items, look_at, hold_ticks, health=200):
    """`module` on, `items` in the hotbar from slot 0 (the first held), right click held `hold_ticks` looking at the
    block `look_at`: what was used of each, the ticks it ran, damage you took, alive, most move packets in a tick."""
    for i, (item, n) in enumerate(items):
        slot(i, item, n)
    ctl("key", "1", "tap")
    cmd(f"attribute @s minecraft:max_health base set {health}", "effect give @s minecraft:instant_health 1 28 true",
        "effect give @s minecraft:fire_resistance 120 0 true")
    only(module)
    ctl("look-at", *(str(v) for v in look_at))
    wait(10)
    mark = probe_mark() if SERVER and probe_path() else None
    before_inv = ctl("inventory").get("counts", {})
    own0 = own_health()
    ctl("mouse", "right", "down")
    wait(hold_ticks)
    ctl("mouse", "right", "up")
    wait(20)
    only()
    after_inv = ctl("inventory").get("counts", {})
    used = {item: before_inv.get(f"minecraft:{item}", 0) - after_inv.get(f"minecraft:{item}", 0) for item, _ in items}
    st = state()
    alive = st.get("screen") != "dead" and bool(st.get("player"))
    most = wire_stats(mark)[1] if mark is not None else None
    return used, own0 - (own_health() if alive else 0), alive, most


@test("Crystal Macro and Anchor Macro")
def _():
    reset()
    blast_arena(bot=False)
    # A crystal held on the obsidian 3 blocks ahead: placed and broken over and over for 5 s.
    cmd(f"setblock {X0} {Y+1} {Z0+3} minecraft:obsidian")
    setting("crystalmacro", "max self damage", "20")
    crystals, taken, alive, most = macro_run("crystalmacro", [("end_crystal", 64)], (X0 + 0.5, Y + 2.0, Z0 + 3.5), 100)
    left = crystals_left()
    per = 100 / max(crystals["end_crystal"], 1)
    # An anchor held at the floor 3 blocks ahead: placed, charged, set off, once a click (the block in between guards).
    cmd(f"fill {X0-1} {Y+1} {Z0+1} {X0+1} {Y+4} {Z0+5} minecraft:air", "kill @e[type=minecraft:end_crystal]")
    anchors, anchor_taken, anchor_alive, anchor_most = 0, 0, True, None
    times = []
    for _ in range(5):
        used, t, a, m = macro_run("anchormacro", [("respawn_anchor", 16), ("glowstone", 16)], (X0 + 0.5, Y + 1.0, Z0 + 3.5), 3)
        anchors += used["respawn_anchor"]
        anchor_taken += t
        anchor_alive &= a
        anchor_most = max(anchor_most or 0, m or 0) if m is not None else anchor_most
        cmd(f"fill {X0-1} {Y+1} {Z0+1} {X0+1} {Y+4} {Z0+5} minecraft:air")
    blast_done()
    ok = (crystals["end_crystal"] >= 5 and left in (0, 1) and alive and most in (None, 0, 1) and anchors >= 4 and anchor_alive
          and anchor_most in (None, 0, 1))
    return ok, (f"Crystal Macro: {crystals['end_crystal']} crystals in 5 s ({per:.1f} ticks each), {left} left standing, "
                f"you took {taken:.0f} of 200; Anchor Macro: {anchors} of 5 clicks set an anchor off, you took "
                f"{anchor_taken:.0f}; most move packets in a tick {most}/{anchor_most}")


for _key, _modes in RENDER_MODES:
    for _mode in _modes:
        test(f"Render: {_key} ({_mode})")(light(lambda _k=_key, _m=_mode: render_mode(_k, _m)))


def light_clean():
    """Between sweeps: let go, everything off and the touched settings back; each sweep sets its own stage."""
    ctl("release")
    ensure_alive()
    if state().get("screen") not in ("playing", None):
        ctl("key", "esc", "tap")
        wait(2)
    say(".panic")
    say(".panic")
    say(".reset hud")
    on.clear()
    for m in sorted(touched):
        say(f".reset {m}")
    touched.clear()


def overran(*_):
    raise TimeoutError("the test ran over 10 minutes")



# Rust's PR #16, melee and inventory: measured against Bot1 (or a tough pig), each module off and then on.
TOUGH_PIG = ("{NoAI:1b,PersistenceRequired:1b,Silent:1b,Health:1000f,attributes:[{id:'minecraft:max_health',base:1000.0},"
             "{id:'minecraft:knockback_resistance',base:1.0}]}")


def bot_ready(z=2.5, yaw=180, *extra):
    """Bot1 in survival with 100 health and Resistance IV, `z` blocks in front of you, facing `yaw`."""
    bot_respawned()
    cmd("gamemode survival Bot1", "clear Bot1", "effect clear Bot1", "attribute Bot1 minecraft:max_health base set 100",
        "effect give Bot1 minecraft:resistance 600 3 true", "effect give Bot1 minecraft:instant_health 1 28 true",
        f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + z} {yaw} 0", *extra)
    wait(10)


def bot_done():
    ctl2("release")
    cmd("attribute Bot1 minecraft:max_health base set 20", "effect clear Bot1", "clear Bot1", "gamemode spectator Bot1",
        "gamerule natural_health_regeneration true")


def poll(ticks, every=2, fn=None):
    """Calls `fn` every `every` ticks for `ticks` ticks; the values it returned."""
    out = []
    for _ in range(ticks // every):
        wait(every)
        out.append(fn())
    return out


@test("Shield Breaker")
def _():
    # Bot1 holds a shield up at you, put back in front every 2 ticks; Kill Aura with a sword, an axe in slot 3. Off,
    # Kill Aura's hits land on the shield and do nothing; on, the axe disables it and the sword's hits land. 10 s each.
    if not CTL2:
        return None, "needs a second player"
    reset()
    slot(0, "diamond_sword")
    slot(2, "diamond_axe")
    ctl("key", "1", "tap")
    setting("killaura", "entities", "player")
    runs = {}
    for name, mods in (("off", ("killaura",)), ("on", ("killaura", "shieldbreaker"))):
        # In Bot1's hand: Horizon's server does not take /item replace for another player's offhand.
        bot_ready(2.5, 180, "give Bot1 minecraft:shield")
        ctl2("select", "1")
        ctl2("mouse", "right", "down")
        wait(10)
        before = bot_health()
        only(*mods)
        back = f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5} 180 0"
        # Each poll's command takes 2 ticks more: 200 ticks in all.
        held_items = poll(100, 2, lambda: (cmd(back), (state().get("held") or {}).get("item", ""))[1])
        only()
        axe_hits = sum(1 for a, b in zip(held_items, held_items[1:]) if "axe" in b and "axe" not in a)
        runs[name] = (round(before - bot_health(), 1), axe_hits, (state().get("held") or {}).get("item", ""))
        ctl2("mouse", "right", "up")
    bot_done()
    off, on = runs["off"], runs["on"]
    # The axe is in hand a tick at a time, so polling every 2 ticks may miss it: the damage is the measure.
    return off[0] == 0 and on[0] > 0 and "sword" in on[2], (
        f"Bot1 behind a raised shield lost {off[0]} health in 10 s without Shield Breaker, {on[0]} with it; the axe "
        f"was seen in hand {on[1]} times and the hand went back to {on[2] or 'nothing'}")


@test("Mace")
def _():
    # Falling 7 blocks past a pig with 1000 health, Kill Aura holding a sword, a mace in slot 3: the first hit's
    # damage without and with Mace (a smash adds 4 a block for the first 3 fallen, 2 for the next 5).
    reset()
    setting("killaura", "entities", "pig")
    lost = {}
    for name, mods in (("off", ("killaura",)), ("on", ("killaura", "mace"))):
        reset()
        slot(0, "diamond_sword")
        slot(2, "mace")
        ctl("key", "1", "tap")
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 1.5} {TOUGH_PIG}")
        wait(10)
        only(*mods)
        cmd(f"tp @s {X0 + 0.5} {Y + 8} {Z0 + 0.5} 0 60")
        hit = 0
        for _ in range(60):
            wait(1)
            m = mobs("pig")
            if m and m[0]["health"] < 1000:
                hit = 1000 - m[0]["health"]
                break
        only()
        lost[name] = (round(hit, 1), (state().get("held") or {}).get("item", ""))
    return lost["on"][0] > lost["off"][0] * 1.5 and "sword" in lost["on"][1], (
        f"first hit falling past the pig: {lost['off'][0]} with the sword alone, {lost['on'][0]} with Mace; hand after: "
        f"{lost['on'][1]}")


@test("Spear Swap (Reach poke)")
def _():
    # A pig 4.5 blocks off (past the 3 blocks of reach), a sword in hand and an iron spear in slot 3: your clicks.
    lost = {}
    for name, mods in (("off", ()), ("on", ("spearswap",))):
        reset()
        slot(0, "diamond_sword")
        slot(2, "iron_spear")
        ctl("key", "1", "tap")
        if mods:
            setting("spearswap", "reach poke", "true")
        pid = None
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 5.0} {TOUGH_PIG}")
        wait(10)
        pid = mobs("pig")[0]["id"]
        ctl("look-at-entity", str(pid))
        only(*mods)
        for _ in range(5):
            ctl("mouse", "left", "click")
            wait(25)
        only()
        m = mobs("pig")
        lost[name] = (round(1000 - m[0]["health"], 1) if m else None, (state().get("held") or {}).get("item", ""))
    return lost["off"][0] == 0 and (lost["on"][0] or 0) > 0 and "sword" in lost["on"][1], (
        f"5 clicks at a pig 4.5 blocks off: {lost['off'][0]} health lost without Spear Swap, {lost['on'][0]} with Reach "
        f"poke; hand after: {lost['on'][1]}")


def after_hits(mods, ticks, sample, hurt, act=None, ticks_after=6):
    """Runs `mods` for `ticks` while you sprint forward (W and Ctrl held), sampling `sample()` (a position along z)
    and `hurt()` (the health of whoever is hit) every tick, `act` first if given: for each hit (a drop in that health)
    with the sample still over the platform, how far along z it moved in the `ticks_after` ticks after it."""
    only(*mods)
    ctl("key", "ctrl", "down")
    ctl("key", "w", "down")
    zs, hs = [], []
    for t in range(ticks):
        if act:
            act(t)
        wait(1)
        zs.append(sample())
        hs.append(hurt())
    ctl("key", "w", "up")
    ctl("key", "ctrl", "up")
    only()
    hits = [i for i in range(1, len(hs) - ticks_after) if hs[i] < hs[i - 1] - 0.01 and zs[i + ticks_after] < Z0 + 16]
    return [round(zs[i + ticks_after] - zs[i - 1], 2) for i in hits]


def mean(xs):
    return round(sum(xs) / len(xs), 2) if xs else 0.0


def heard(sound):
    """How many times the game has played `sound` (minecraft:...) so far."""
    return ctl("status").get("heard", {}).get(sound, 0)


@test("W Tap")
def _():
    # Bot1 stands still 3 blocks in front; you sprint at it with Kill Aura and a sword for 3 s. A hit the server takes
    # as a sprint hit plays the knockback sound and knocks further; it also stops your sprint on the server, so without
    # W Tap only the first hit is one. Counted: sprint hits (that sound), and how far Bot1 went along z in the 3 ticks
    # after each hit while still on the platform.
    if not CTL2:
        return None, "needs a second player"
    setting("killaura", "entities", "player")
    runs = {}
    for name, mods in (("off", ()), ("on", ("wtap",)), ("S Tap", ("wtap",))):
        if name == "S Tap":
            setting("wtap", "mode", "S Tap")
        reset(z=-13.5)
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        bot_ready(-10.5, 180)
        sound = heard("minecraft:entity.player.attack.knockback")
        pushes = after_hits(("killaura",) + mods, 60, lambda: ctl2("state")["player"]["position"][2], bot_health, ticks_after=3)
        runs[name] = (heard("minecraft:entity.player.attack.knockback") - sound, len(pushes), pushes)
    bot_done()
    note = "sprint hits / hits, and Bot1's push per hit (blocks in 3 ticks): " + "; ".join(
        f"{k} {sprints}/{hits} {pushes}" for k, (sprints, hits, pushes) in runs.items())
    # Horizon's server (single player's too) gives no sprint hit after the first, whatever the sprint commands say.
    if runs["off"][1] >= 3 and runs["on"][1] >= 3 and runs["off"][0] <= 1 and runs["on"][0] <= 1:
        return None, "this server gives no sprint hit after the first, with W Tap or without: " + note
    ok = runs["off"][1] >= 3 and runs["on"][1] >= 3 and runs["on"][0] > runs["off"][0]
    return ok, note


@test("Jump Reset")
def _():
    # Six times each, off and on: you sprint (W and Ctrl) from 2.5 blocks at Bot1, who stands still, and after 8 ticks
    # it hits you once with a Knockback I stick. How far back the hit put you: the most your z fell below where you
    # were as it landed, over the next 10 ticks. Jump Reset jumps as the knockback comes, and a sprint jump pushes you
    # forward, so you go back less.
    if not CTL2:
        return None, "needs a second player"
    back, ups = {}, {}
    for name, mods in (("off", ()), ("on", ("jumpreset",))):
        reset(z=-13.5)
        cmd("effect give @s minecraft:resistance 600 3 true")
        bot_ready(-11.0, 180, "give Bot1 minecraft:stick[enchantments={knockback:1}]")
        ctl2("select", "1")
        tester = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
        only(*mods)
        back[name], ups[name] = [], 0
        for _ in range(6):
            cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 - 13.5} 0 0", f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 - 11.0} 180 0")
            wait(10)
            ctl("key", "ctrl", "down")
            ctl("key", "w", "down")
            wait(8)
            health = own_health()
            ctl2("look-at-entity", str(tester))
            ctl2("mouse", "left", "click")
            at = state()["player"]["position"]
            samples = poll(10, 1, lambda: state()["player"]["position"])
            ctl("key", "w", "up")
            ctl("key", "ctrl", "up")
            if own_health() < health - 0.01:
                back[name].append(round(at[2] - min(p[2] for p in samples), 2))
                ups[name] += any(p[1] > at[1] + 0.3 for p in samples)
        only()
    bot_done()
    ok = len(back["off"]) >= 4 and len(back["on"]) >= 4 and mean(back["on"]) < mean(back["off"]) - 0.15
    return ok, (f"knocked back (blocks) by each hit: off {mean(back['off'])} {back['off']}, on {mean(back['on'])} {back['on']}; "
                f"rose 0.3 blocks or more after {ups['off']}/{ups['on']} hits off/on")


@test("Friend Guard")
def _():
    # Bot1 is a friend; 10 clicks on it with a sword, without and with Friend Guard.
    if not CTL2:
        return None, "needs a second player"
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    say(".friend add Bot1")
    lost = {}
    for name, mods in (("off", ()), ("on", ("friendguard",))):
        bot_ready(2.0, 180)
        before = bot_health()
        only(*mods)
        for _ in range(10):
            ctl("look", "0", "10")
            ctl("mouse", "left", "click")
            wait(13)
        only()
        lost[name] = round(before - bot_health(), 1)
    say(".friend remove Bot1")
    bot_done()
    return lost["off"] > 0 and lost["on"] == 0, f"10 clicks on friend Bot1: {lost['off']} health lost without Friend Guard, {lost['on']} with it"


@test("Auto Inventory Totem")
def _():
    # 20 totem pops: ticks until the offhand holds a totem again, with totems in the main inventory only.
    reset()
    cmd("item replace entity @s weapon.offhand with minecraft:totem_of_undying")
    for i in range(9, 30):
        cmd(f"item replace entity @s inventory.{i - 9} with minecraft:totem_of_undying")
    only("autoinventorytotem")
    wait(10)
    times = []
    for _ in range(20):
        cmd("damage @s 1000 minecraft:generic")
        n = 0
        while n < 100:
            wait(1)
            n += 1
            if "totem" in offhand() and n > 2:
                break
        times.append(n)
        cmd("effect give @s minecraft:instant_health 1 10 true")
        wait(5)
    only()
    alive = state()["player"]["health"] > 0
    ok = alive and max(times) < 100
    return ok, f"re-totem after a pop: {sorted(times)} ticks (median {sorted(times)[10]}); alive after 20 pops: {alive}"


def slot_centre(slot):
    """The GUI point at the middle of inventory `slot` (9-35 main, 0-8 hotbar) with the inventory open, found by
    picking up the dirt this puts in slot 35 at each GUI scale until one takes it, then putting it back."""
    d = ctl("status")["display"]
    cmd("item replace entity @s inventory.26 with minecraft:dirt")
    wait(5)
    pos = lambda i, w, h: ((w - 176) // 2 + 8 + (i - 9) % 9 * 18 + 8, (h - 166) // 2 + 84 + (i - 9) // 9 * 18 + 8) if i >= 9 \
        else ((w - 176) // 2 + 8 + i * 18 + 8, (h - 166) // 2 + 142 + 8)
    for scale in (3, 2, 4, 1):
        w, h = d["width"] / scale, d["height"] / scale
        x, y = pos(35, w, h)
        ctl("click-at", str(x), str(y))
        wait(2)
        if (ctl("inventory").get("carried") or {}).get("item") == "minecraft:dirt":
            # Half a second first: a second click on the slot sooner is a double click, which keeps the stack carried.
            wait(10)
            ctl("click-at", str(x), str(y))
            for _ in range(10):
                wait(2)
                if not ctl("inventory").get("carried"):
                    return pos(slot, w, h)
            return None
    return None


@test("Hover Totem")
def _():
    # A totem in the main inventory (slot 14) and the offhand empty: with the inventory open, the pointer is moved over
    # the totem (a mouse wheel turn of 0 steps there, so nothing is clicked). Off it stays put; on it goes to the offhand.
    got = {}
    for name, mods in (("off", ()), ("on", ("hovertotem",))):
        reset()
        cmd("item replace entity @s inventory.5 with minecraft:totem_of_undying")
        only(*mods)
        ctl("open-inventory")
        wait(5)
        at = slot_centre(14)
        if at is None:
            only()
            ctl("close")
            return False, "found no GUI scale where a click picks up and puts back the dirt in the inventory's slot 35"
        ctl("scroll-at", str(at[0]), str(at[1]), "0")
        n = 0
        while n < 40 and "totem" not in offhand():
            wait(2)
            n += 2
        got[name] = (offhand(), n)
        ctl("close")
        only()
    ok = "totem" not in got["off"][0] and "totem" in got["on"][0]
    return ok, f"offhand after the pointer rests on the totem: off {got['off'][0] or 'empty'}, on {got['on'][0] or 'empty'} (in {got['on'][1]} ticks)"


@test("FastXP and Key Throw")
def _():
    # Bottles thrown in 2 s of holding use, without and with FastXP; pearls thrown by 5 presses of Key Throw's key,
    # the hand back on its slot after.
    used = {}
    for name, mods in (("off", ()), ("on", ("fastxp",))):
        reset()
        slot(0, "experience_bottle", 64)
        ctl("key", "1", "tap")
        only(*mods)
        ctl("look", "0", "-60")
        ctl("mouse", "right", "down")
        wait(40)
        ctl("mouse", "right", "up")
        only()
        used[name] = 64 - ((state().get("held") or {}).get("count", 0))
    reset()
    slot(0, "diamond_sword")
    slot(4, "ender_pearl", 16)
    ctl("key", "1", "tap")
    setting("keythrow", "pearl key", "G")
    only("keythrow")
    for _ in range(5):
        ctl("key", "g", "tap")
        wait(30)
    only()
    inv = ctl("inventory")
    left = sum(s.get("count", 0) for s in inv.get("hotbar") or [] if s and "ender_pearl" in (s.get("item") or ""))
    back = (state().get("held") or {}).get("item", "")
    return used["on"] > used["off"] and left == 11 and "sword" in back, (
        f"bottles in 2 s: {used['off']} without FastXP, {used['on']} with it; Key Throw: {16 - left} pearls for 5 presses, hand "
        f"back on {back}")



def bot_probe_frames(mark):
    """Bot1's probe frames since `mark` (its own SUNSET_PROBE file, in its session)."""
    with open(os.path.join(os.path.dirname(CTL2), "session2")) as f:
        path = os.path.join(f.read().strip(), "probe.log")
    with open(path) as f:
        f.seek(mark)
        lines = f.read().split("\n")[:-1]
    return [Frame(line.split()) for line in lines if line[:1].isdigit()]


def bot_probe_mark():
    with open(os.path.join(os.path.dirname(CTL2), "session2")) as f:
        return os.path.getsize(os.path.join(f.read().strip(), "probe.log"))


def seen_late(own, seen):
    """How late (ms) the x positions in `seen` [(t, x)] follow `own` [(t, x)]: the shift that fits them best."""
    def at(t):
        for (t0, x0), (t1, x1) in zip(own, own[1:]):
            if t0 <= t <= t1:
                return x0 + (x1 - x0) * (t - t0) / max(t1 - t0, 1e-9)
        return None
    best = None
    for ms in range(0, 1500, 10):
        gaps = [abs(x - y) for t, x in seen if (y := at(t - ms / 1000)) is not None]
        if len(gaps) > len(seen) // 2 and (best is None or sum(gaps) / len(gaps) < best[1]):
            best = (ms, sum(gaps) / len(gaps))
    return best[0] if best else None


@test("Fake Lag")
def _():
    # You strafe left and right in front of Bot1 for 3 s, without and with Fake Lag (Always, 300 ms). From both
    # clients' probes: your x as you see it and as Bot1 sees you, and the shift in time that fits them best (how late
    # the server and Bot1 have you). The wire probe: never two move packets in a client tick, no kick.
    if not CTL2:
        return None, "needs a second player"
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    bot_ready(4.5, 180)
    me = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
    setting("fakelag", "mode", "Always")
    setting("fakelag", "delay", "300")
    late, wire = {}, {}
    for name, mods in (("off", ()), ("on", ("fakelag",))):
        cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
        wait(10)
        only(*mods)
        wait(10)
        mark, bmark = probe_mark(), bot_probe_mark()
        for step in range(6):
            ctl("key", "ad"[step % 2], "down")
            wait(10)
            ctl("key", "ad"[step % 2], "up")
        wait(20)
        only()
        own = [(f.t, f.eye[0]) for f in probe_frames(mark)]
        seen = [(f.t, f.mobs[me][0][0]) for f in bot_probe_frames(bmark) if me in f.mobs]
        late[name] = seen_late(own, seen)
        wire[name] = order_stats(mark)
    bot_done()
    if late["off"] is None or late["on"] is None:
        return False, f"no fit: {late}"
    extra = late["on"] - late["off"]
    note = (f"Bot1 sees you {late['off']} ms late without Fake Lag, {late['on']} ms with it (+{extra} ms for 300 ms); most "
            f"move packets in a client tick: {wire['off'][3]} off, {wire['on'][3]} on, over {wire['on'][2]} ticks")
    return 220 <= extra <= 420 and wire["on"][3] <= 1 and wire["on"][2] > 40, note


@test("Knockback Delay")
def _():
    # Bot1 hits you 5 times off and 5 times on (Chance 100%, ground delay 400 ms); from your probe, the time from its
    # click to the first frame you move: how late the knockback lands.
    if not CTL2:
        return None, "needs a second player"
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("knockbackdelay", "chance", "100")
    setting("knockbackdelay", "ground delay min", "400")
    setting("knockbackdelay", "ground delay max", "400")
    reset()
    cmd("effect give @s minecraft:resistance 600 4 true")
    bot_ready(2.5, 180)
    tester = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
    late = {}
    for name, mods in (("off", ()), ("on", ("knockbackdelay",))):
        only(*mods)
        late[name] = []
        for _ in range(5):
            cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0", f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5} 180 0", "effect give @s minecraft:instant_health 1 3 true")
            wait(20)
            ctl2("look-at-entity", str(tester))
            mark, t0 = probe_mark(), time.time()
            ctl2("mouse", "left", "click")
            wait(30)
            frames = probe_frames(mark)
            still = frames[0].eye if frames else None
            moved = next((f.t for f in frames if still and f.t > t0 and math.dist(f.eye[::2], still[::2]) > 0.05), None)
            if moved:
                late[name].append(round((moved - t0) * 1000))
        only()
    bot_done()
    if len(late["off"]) < 4 or len(late["on"]) < 4:
        return False, f"knocked back too seldom: {late}"
    off, on = sorted(late["off"])[len(late["off"]) // 2], sorted(late["on"])[len(late["on"]) // 2]
    return 300 <= on - off <= 550, f"knockback lands (ms after Bot1's click, median): off {off} {late['off']}, on {on} {late['on']}"


@test("Auto City and Auto Trap (rates)")
def _():
    # Auto City, 5 times: Bot1 stands in a ring of obsidian 3 blocks in front, you hold an Efficiency V netherite
    # pickaxe; it has to mine the ring block nearest you and turn off. Auto Trap, 5 times with Bot1 still and 3 times
    # with it walking sideways: obsidian placed round it in 3 s.
    if not CTL2:
        return None, "needs a second player"
    reset()
    cmd("give @s minecraft:netherite_pickaxe[enchantments={efficiency:5}]", "give @s minecraft:obsidian 64")
    wait(5)
    mined, ticks = 0, []
    for _ in range(5):
        cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 2} {X0 + 1} {Y + 1} {Z0 + 4} minecraft:obsidian", f"setblock {X0} {Y + 1} {Z0 + 3} minecraft:air")
        bot_ready(3.5, 180)
        cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
        wait(5)
        only("autocity")
        for t in range(0, 120, 5):
            wait(5)
            if not is_on("Auto City"):
                break
        turned_itself_off("autocity", "Auto City")
        only()
        if not check(f"execute if block {X0} {Y + 1} {Z0 + 2} minecraft:obsidian"):
            mined += 1
            ticks.append(t + 5)
    cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 2} {X0 + 1} {Y + 1} {Z0 + 4} minecraft:air")
    placed = {"still": [], "walking": []}
    for name, runs in (("still", 5), ("walking", 3)):
        for _ in range(runs):
            cmd(f"fill {X0 - 3} {Y + 1} {Z0 + 1} {X0 + 3} {Y + 3} {Z0 + 4} minecraft:air", "give @s minecraft:obsidian 64")
            bot_ready(2.5, 180)
            ctl("key", "2", "tap")
            if name == "walking":
                ctl2("key", "a", "down")
            only("autotrap")
            wait(60)
            ctl2("key", "a", "up")
            turned_itself_off("autotrap", "Auto Trap")
            only()
            placed[name].append(count(X0 - 3, Y + 1, Z0 + 1, X0 + 3, Y + 3, Z0 + 4, "obsidian"))
    bot_done()
    cmd(f"fill {X0 - 3} {Y + 1} {Z0 + 1} {X0 + 3} {Y + 3} {Z0 + 4} minecraft:air")
    trapped = sum(n >= 5 for n in placed["still"])
    return mined >= 4 and trapped >= 4, (f"Auto City mined the wall {mined}/5 times, in {ticks} ticks; Auto Trap boxed Bot1 (5+ "
                                         f"obsidian) {trapped}/5 times standing {placed['still']}, walking it placed {placed['walking']}")


@test("Sword PvP Bot")
def _():
    # Bot1 (20 health, no armour) 8 blocks away, 3 times standing still and 3 times strafing left and right; you have
    # a diamond sword and Sword PvP Bot on: it has to kill Bot1 within 20 s. After each fight, once it is off, you must
    # stand still (no key left held).
    if not CTL2:
        return None, "needs a second player"
    won, stuck = {"still": [], "strafing": []}, 0
    for name in ("still", "still", "still", "strafing", "strafing", "strafing"):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        bot_respawned()
        cmd("gamemode survival Bot1", "clear Bot1", "effect clear Bot1", "effect give Bot1 minecraft:instant_health 1 10 true",
            f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 8.5} 180 0")
        wait(10)
        only("swordpvpbot")
        start, dead = time.time(), False
        for step in range(100):
            if name == "strafing" and step % 5 == 0:
                ctl2("key", "ad"[step // 5 % 2], "down")
                ctl2("key", "da"[step // 5 % 2], "up")
            wait(4)
            if ctl2("state").get("screen") == "dead" or bot_health() <= 0:
                dead = True
                break
        ctl2("key", "a", "up")
        ctl2("key", "d", "up")
        took = round(time.time() - start, 1)
        only()
        wait(20)
        a = pos()
        wait(20)
        stuck += math.dist(a, pos()) > 0.1
        won[name].append(took if dead else None)
    bot_respawned()
    bot_done()
    wins = sum(t is not None for ts in won.values() for t in ts)
    return wins >= 5 and stuck == 0, (f"Bot1 killed {wins}/6 times; seconds per fight (None: alive after 20 s): still {won['still']}, "
                                      f"strafing {won['strafing']}; still moving after it turned off: {stuck}")


@test("Key Throw (Hotbar keys)")
def _():
    # Duelkit's Quick use: holding a sword, 3 presses each of the pearls' hotbar key (5) and the wind charges' (6) with
    # Hotbar keys on: each throws one and the sword stays in hand. Off, the key selects the slot as usual.
    reset()
    slot(0, "diamond_sword")
    slot(4, "ender_pearl", 16)
    slot(5, "wind_charge", 16)
    ctl("key", "1", "tap")
    ctl("look", "0", "-80")
    setting("keythrow", "hotbar keys", "true")
    only("keythrow")
    kept = 0
    for key in "555666":
        ctl("key", key, "tap")
        wait(25)
        kept += "sword" in held().get("item", "")
    only()
    hotbar = ctl("inventory").get("hotbar") or []
    left = {k: sum(s.get("count", 0) for s in hotbar if s and k in (s.get("item") or "")) for k in ("ender_pearl", "wind_charge")}
    ctl("key", "5", "tap")
    wait(2)
    off = held().get("item", "")
    setting("keythrow", "hotbar keys", "false")
    return left == {"ender_pearl": 13, "wind_charge": 13} and kept == 6 and "pearl" in off, (
        f"after 3 presses of each key: {left} left (13 each wanted), sword in hand after {kept}/6; Key Throw off, key 5 "
        f"selects {off}")


@test("FastXP (Fast throw)")
def _():
    # Duelkit's Fast throw through FastXP: wind charges (10-tick cooldown, the game retries use every 4 ticks) held on
    # use for 3 s, and a stack of 16 snowballs for 1.5 s, without and with FastXP (Snowballs and eggs ticked).
    setting("fastxp", "snowballs and eggs", "true")
    used = {}
    for item, n, ticks in (("wind_charge", 64, 60), ("snowball", 16, 30)):
        for name, mods in (("off", ()), ("on", ("fastxp",))):
            reset()
            slot(0, item, n)
            ctl("key", "1", "tap")
            ctl("look", "0", "-60")
            only(*mods)
            ctl("mouse", "right", "down")
            wait(ticks)
            ctl("mouse", "right", "up")
            only()
            used[item, name] = n - ((state().get("held") or {}).get("count", 0))
    setting("fastxp", "snowballs and eggs", "false")
    ok = used["wind_charge", "on"] > used["wind_charge", "off"] and used["snowball", "on"] > used["snowball", "off"]
    return ok, "thrown in 3 s, off/on: " + ", ".join(f"{i} {used[i, 'off']}/{used[i, 'on']}" for i in ("wind_charge", "snowball"))


@test("Wind Jump")
def _():
    # 3 wind charges thrown straight down from the ground, without and with Wind Jump: the highest your feet rise
    # above the ground in the next 40 ticks.
    peaks = {}
    for name, mods in (("off", ()), ("on", ("windjump",))):
        peaks[name] = []
        for _ in range(3):
            reset()
            slot(0, "wind_charge", 16)
            ctl("key", "1", "tap")
            ctl("look", "0", "90")
            only(*mods)
            wait(5)
            ground = pos()[1]
            ctl("mouse", "right", "click")
            peaks[name].append(round(max(poll(40, 1, lambda: pos()[1])) - ground, 2))
            only()
    ok = mean(peaks["on"]) > mean(peaks["off"]) + 0.5
    return ok, f"rise from a wind charge thrown down (blocks): off {mean(peaks['off'])} {peaks['off']}, on {mean(peaks['on'])} {peaks['on']}"


@test("Hold to Eat")
def _():
    # A sword held, golden apples in slot 4: with Hold to Eat, key 4 held for 40 ticks eats one and the sword comes
    # back; off, the key only selects the apples.
    got = {}
    for name, mods in (("off", ()), ("on", ("holdtoeat",))):
        reset()
        slot(0, "diamond_sword")
        slot(3, "golden_apple", 8)
        ctl("key", "1", "tap")
        only(*mods)
        ctl("key", "4", "down")
        wait(40)
        ctl("key", "4", "up")
        wait(5)
        only()
        hotbar = ctl("inventory").get("hotbar") or []
        apples = sum(s.get("count", 0) for s in hotbar if s and "golden_apple" in (s.get("item") or ""))
        got[name] = (apples, held().get("item", ""))
    ok = got["off"][0] == 8 and got["on"][0] == 7 and "sword" in got["on"][1]
    return ok, f"apples left and the item in hand after: off {got['off']}, on {got['on']}"


# Horizon's own server (a dedicated one on 25599, or single player's) runs the hit again along your look with 3
# blocks of reach and blocks in the way: hits past that or through walls are its to refuse.
HORIZON_SERVER = SERVER is None or SERVER.endswith(":25599")
REFUSED = "Horizon's server refuses hits past 3 blocks or through walls (it hits along your look itself)"


def pig_clicks(mods, z, settings=(), wall=None, n=5):
    """A pig with 1000 health `z` blocks in front, a sword in hand (an iron spear in slot 3): health it lost to `n`
    clicks at it with `mods` on (`settings` set first), and the item in hand after. `wall`: a stone wall that far
    in front, between you and it."""
    reset()
    slot(0, "diamond_sword")
    slot(2, "iron_spear")
    ctl("key", "1", "tap")
    for mod, name, value in settings:
        setting(mod, name, value)
    if wall:
        cmd(f"fill {X0 - 2} {Y + 1} {Z0 + wall} {X0 + 2} {Y + 3} {Z0 + wall} minecraft:stone")
    cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + z} {TOUGH_PIG}")
    wait(10)
    ctl("look-at-entity", str(mobs("pig")[0]["id"]))
    only(*mods)
    for _ in range(n):
        ctl("mouse", "left", "click")
        wait(25)
    only()
    m = mobs("pig")
    if wall:
        cmd(f"fill {X0 - 2} {Y + 1} {Z0 + wall} {X0 + 2} {Y + 3} {Z0 + wall} minecraft:air")
    return (round(1000 - m[0]["health"], 1) if m else None), held().get("item", "")


@test("Reach (Entity reach)")
def _():
    # A pig 4.5 blocks off (its box 4.05 from you, past the 3 blocks of reach): 5 clicks, without and with Reach.
    off = pig_clicks((), 5.0)
    on = pig_clicks(("reach",), 5.0)
    if HORIZON_SERVER and on[0] == 0:
        return None, f"{REFUSED}: off {off[0]}, on {on[0]}"
    return off[0] == 0 and (on[0] or 0) > 0, f"5 clicks at a pig 4.5 blocks off: {off[0]} health lost without Reach, {on[0]} with Entity reach 4.5"


@test("Spear Swap (Spear reach)")
def _():
    # The same pig, a sword in hand and an iron spear in slot 3: Spear reach (When out of reach) reaches it with the
    # sword, passing through the spear's slot; the sword stays in hand.
    off = pig_clicks((), 5.0)
    on = pig_clicks(("spearswap",), 5.0, (("spearswap", "spear reach", "when out of reach"),))
    if HORIZON_SERVER and on[0] == 0:
        return None, f"{REFUSED}: off {off[0]}, on {on[0]}"
    return off[0] == 0 and (on[0] or 0) > 0 and "sword" in on[1], (
        f"5 clicks at a pig 4.5 blocks off: {off[0]} health lost without Spear reach, {on[0]} with it; hand after: {on[1]}")


@test("Ghost Hand (Hits)")
def _():
    # A pig 2.5 blocks off behind a stone wall 1.5 blocks off: 5 clicks, without and with Ghost Hand's Hits.
    off = pig_clicks((), 3.0, wall=1)
    on = pig_clicks(("ghosthand",), 3.0, (("ghosthand", "hits", "true"),), wall=1)
    if HORIZON_SERVER and on[0] == 0:
        return None, f"{REFUSED}: off {off[0]}, on {on[0]}"
    return off[0] == 0 and (on[0] or 0) > 0, f"5 clicks at a pig behind a wall: {off[0]} health lost without Ghost Hand, {on[0]} with Hits"


@test("Ghost Hand (Blocks)")
def _():
    # A chest 2.5 blocks off behind a stone wall: a right click at it, without and with Ghost Hand.
    opened = {}
    for name, mods in (("off", ()), ("on", ("ghosthand",))):
        reset()
        cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 2} {X0 + 1} {Y + 3} {Z0 + 2} minecraft:stone", f"setblock {X0} {Y + 1} {Z0 + 3} minecraft:chest")
        wait(5)
        ctl("look-at", str(X0 + 0.5), str(Y + 1.5), str(Z0 + 3.5))
        only(*mods)
        ctl("mouse", "right", "click")
        wait(10)
        opened[name] = state().get("screen")
        if opened[name] != "playing":
            ctl("key", "esc", "tap")
            wait(5)
        only()
        cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 2} {X0 + 1} {Y + 3} {Z0 + 3} minecraft:air")
    return opened["off"] == "playing" and opened["on"] not in ("playing", None), (
        f"right click at a chest behind a wall: screen {opened['off']} without Ghost Hand, {opened['on']} with it")


@test("Key Throw (Lunge spears)")
def _():
    # A sword held, an iron spear with Lunge III in slot 5: key 5 with Hotbar keys on jabs with it (the lunge dashes
    # you forward) and the sword stays; off, the key selects the spear.
    got = {}
    for name, mods in (("off", ()), ("on", ("keythrow",))):
        reset()
        slot(0, "diamond_sword")
        slot(4, "iron_spear[enchantments={lunge:3}]")
        ctl("key", "1", "tap")
        if mods:
            setting("keythrow", "hotbar keys", "true")
        only(*mods)
        wait(5)
        a = pos()
        ctl("key", "5", "tap")
        wait(15)
        got[name] = (round(flat(a, pos()), 2), held().get("item", ""))
        only()
    return got["on"][0] > got["off"][0] + 0.5 and "sword" in got["on"][1] and "spear" in got["off"][1], (
        f"moved and the item in hand after key 5: off {got['off']}, on {got['on']}")


@test("Pearl Catch")
def _():
    # A pearl thrown looking 80 degrees up, wind charges in slot 3: the tick the pearl takes you (you move more than
    # 3 blocks in a tick: a blast's push is less), the highest you are in the 100 ticks after the throw, and wind
    # charges used, without and with Pearl Catch (Auto).
    got = {}
    for name, mods in (("off", ()), ("on", ("pearlcatch",))):
        reset()
        slot(0, "ender_pearl", 4)
        slot(2, "wind_charge", 16)
        ctl("key", "1", "tap")
        ctl("look", "0", "-80")
        only(*mods)
        wait(5)
        a = pos()
        ground = a[1]
        ctl("mouse", "right", "click")
        flight, top = None, ground
        for t in range(100):
            wait(1)
            b = pos()
            if flight is None and math.dist(a, b) > 3:
                flight = t
            top, a = max(top, b[1]), b
        only()
        hotbar = ctl("inventory").get("hotbar") or []
        got[name] = (flight, round(top - ground, 1), 16 - sum(s.get("count", 0) for s in hotbar if s and "wind_charge" in (s.get("item") or "")))
        cmd("effect give @s minecraft:instant_health 1 10 true")
    ok = got["on"][2] == 1 and got["off"][2] == 0 and got["on"][1] > got["off"][1] + 5
    return ok, f"(tick the pearl took you, highest you got, wind charges used): off {got['off']}, on {got['on']}"


@test("Loadouts")
def _():
    # A sword in slot 1 and pearls in slot 2 saved as a loadout; then the sword and pearls moved into the inventory and
    # dirt put in their slots: .loadout test, and the Menu key (K) held with 1 pressed, each put them back (the slot
    # you hold stays).
    reset()
    slot(0, "diamond_sword")
    slot(1, "ender_pearl", 8)
    only("loadouts")
    say(".loadout delete test")
    say(".loadout save test")
    got = []
    for how in ("command", "key"):
        cmd("clear @s", "item replace entity @s inventory.4 with minecraft:diamond_sword",
            "item replace entity @s inventory.9 with minecraft:ender_pearl 8", "item replace entity @s hotbar.0 with minecraft:dirt 3",
            "item replace entity @s hotbar.4 with minecraft:cobblestone 3")
        ctl("key", "5", "tap")
        wait(5)
        if how == "command":
            say(".loadout test")
        else:
            ctl("key", "k", "down")
            ctl("key", "1", "tap")
            ctl("key", "k", "up")
        wait(10)
        hotbar = ctl("inventory").get("hotbar") or []
        item = lambda i: ((hotbar[i] or {}).get("item") or "") if i < len(hotbar) else ""
        got.append((how, item(0), item(1), held().get("item", "")))
    say(".loadout delete test")
    only()
    ok = all("diamond_sword" in g[1] and "ender_pearl" in g[2] and "cobblestone" in g[3] for g in got)
    return ok, "after equipping (how, slot 1, slot 2, held): " + ", ".join(map(str, got))


@test("Auto Shield")
def _():
    # A sword in hand and a shield in the offhand. First Bot1 hits you with a sword every 15 ticks, 10 times: health
    # you lost without and with Auto Shield. Then your Kill Aura hits Bot1 for 10 s: health it lost, its hits, and the
    # ticks your shield was down per hit, without and with Auto Shield.
    if not CTL2:
        return None, "needs a second player"
    setting("killaura", "entities", "player")
    took, gave = {}, {}
    for name, mods in (("off", ()), ("on", ("autoshield",))):
        reset()
        slot(0, "diamond_sword")
        cmd("item replace entity @s weapon.offhand with minecraft:shield", "effect give @s minecraft:health_boost 600 9 true",
            "effect give @s minecraft:instant_health 1 28 true", "effect give @s minecraft:resistance 600 1 true")
        ctl("key", "1", "tap")
        bot_ready(2.5, 180, "give Bot1 minecraft:diamond_sword")
        ctl2("select", "1")
        only(*mods)
        wait(20)
        mine = own_health()
        for _ in range(10):
            cmd(f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5} 180 0")
            ctl2("look-at", str(X0 + 0.5), str(Y + 1.6), str(Z0 + 0.5))
            wait(1)
            ctl2("mouse", "left", "click")
            wait(12)
        took[name] = round(mine - own_health(), 1)
        only(*mods, "killaura")
        bot, down, hits, last = bot_health(), 0, 0, bot_health()
        for t in range(200):
            if t % 10 == 0:
                cmd(f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5} 180 0")
            wait(1)
            u = state().get("using") or {}
            down += u.get("slot") != 40
            h = bot_health()
            hits += h < last - 0.01
            last = h
        only()
        gave[name] = (round(bot - bot_health(), 1), hits, round(down / max(hits, 1), 1))
    bot_done()
    ok = took["on"] < took["off"] and gave["on"][0] > 0
    return ok, (f"health you lost to 10 hits: off {took['off']}, on {took['on']}; Kill Aura for 10 s (Bot1 lost, hits, "
                f"ticks unshielded per hit): off {gave['off']}, on {gave['on']}")


@test("Bow Spam (Crossbows)")
def _():
    # A crossbow and 64 arrows, use held for 5 s looking up: arrows shot without and with Bow Spam.
    shot = {}
    for name, mods in (("off", ()), ("on", ("bowspam",))):
        reset()
        slot(0, "crossbow")
        slot(8, "arrow", 64)
        ctl("key", "1", "tap")
        ctl("look", "0", "-45")
        only(*mods)
        ctl("mouse", "right", "down")
        wait(100)
        ctl("mouse", "right", "up")
        only()
        hotbar = ctl("inventory").get("hotbar") or []
        shot[name] = 64 - sum(s.get("count", 0) for s in hotbar if s and "arrow" in (s.get("item") or ""))
    return shot["on"] >= shot["off"] + 2, f"arrows shot holding use for 5 s: off {shot['off']}, on {shot['on']}"


@test("Death Location")
def _():
    # Killed with Death Location on: its chat line and Death waypoint; back at the spot, it clears.
    reset()
    only("deathlocation")
    mark = log_mark()
    cmd(f"tp @s {X0 + 4.5} {Y + 1} {Z0 + 4.5}")
    wait(5)
    cmd("kill @s")
    wait(10)
    died = [l for l in sunset_said(mark) if l.startswith("You died at")]
    ensure_alive()
    mark = log_mark()
    say(".waypoint list")
    listed = [l for l in sunset_said(mark) if "Death" in l]
    cmd(f"tp @s {X0 + 4.5} {Y + 1} {Z0 + 4.5}")
    wait(10)
    reached = [l for l in sunset_said(mark) if l.startswith("Reached")]
    only()
    return bool(died) and bool(listed) and bool(reached), f"said {died}, waypoints {listed}, back there: {reached}"


@test("Shield Breaker (Stun slam)")
def _():
    # Falling 7 blocks onto Bot1, who holds a shield up at you, with Kill Aura (a sword), an axe in slot 3 and a mace
    # in slot 4, Shield Breaker and Mace on: Bot1's health lost by the time you land, Stun slam off and on.
    if not CTL2:
        return None, "needs a second player"
    setting("killaura", "entities", "player")
    lost = {}
    for name in ("false", "true"):
        reset()
        slot(0, "diamond_sword")
        slot(2, "diamond_axe")
        slot(3, "mace")
        ctl("key", "1", "tap")
        setting("shieldbreaker", "stun slam", name)
        bot_ready(2.0, 180, "give Bot1 minecraft:shield")
        ctl2("select", "1")
        ctl2("mouse", "right", "down")
        wait(10)
        before = bot_health()
        only("killaura", "shieldbreaker", "mace")
        cmd(f"tp @s {X0 + 0.5} {Y + 8} {Z0 + 0.5} 0 60")
        for _ in range(40):
            wait(1)
            if state()["player"].get("on_ground"):
                break
        wait(5)
        only()
        lost[name] = round(before - bot_health(), 1)
        ctl2("mouse", "right", "up")
    bot_done()
    return lost["true"] > lost["false"], f"Bot1's health lost to the fall: Stun slam off {lost['false']}, on {lost['true']}"


def x_window():
    """The X window of this session's game (its process has the session's probe path), for real key presses with
    xdotool on the test display; None without xdotool or a window."""
    want = f"SUNSET_PROBE={session_dir()}/probe.log".encode()
    for pid in subprocess.run(["pgrep", "horizon-client"], capture_output=True, text=True).stdout.split():
        try:
            with open(f"/proc/{pid}/environ", "rb") as f:
                if want not in f.read().split(b"\0"):
                    continue
        except OSError:
            continue
        try:
            found = subprocess.run(["xdotool", "search", "--pid", pid], capture_output=True, text=True,
                                   env={**os.environ, "DISPLAY": ":97"}).stdout.split()
        except FileNotFoundError:
            return None
        return found[-1] if found else None
    return None


def x_keys(window, *keys):
    """Real presses (XTEST) of `keys` with the game's window focused, a few ticks apart; "type:<text>" types text."""
    env = {**os.environ, "DISPLAY": ":97"}
    subprocess.run(["xdotool", "windowfocus", "--sync", window], env=env, capture_output=True)
    for k in keys:
        if k.startswith("type:"):
            subprocess.run(["xdotool", "type", "--delay", "80", k[5:]], env=env, capture_output=True)
        else:
            subprocess.run(["xdotool", "key", k], env=env, capture_output=True)
        wait(8)


@test("RF Click")
def _():
    # Real R and F presses (xdotool on the test display): R three times at a pig 2 blocks off, F three times holding
    # snowballs, then R and F typed into the chat box. Without RF Click, R does nothing and F swaps hands; with it, R
    # hits the pig and F throws a snowball, and the chat line still reads "rf".
    window = x_window()
    if not window:
        return None, "needs xdotool and the game's window"
    got = {}
    for name, mods in (("off", ()), ("on", ("rfclick",))):
        reset()
        slot(0, "snowball", 16)
        ctl("key", "1", "tap")
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 2.5} {TOUGH_PIG}")
        wait(10)
        ctl("look-at-entity", str(mobs("pig")[0]["id"]))
        only(*mods)
        x_keys(window, "r", "r", "r")
        hit = round(1000 - mobs("pig")[0]["health"], 1)
        x_keys(window, "f", "f", "f")
        inv = ctl("inventory")
        snow = sum(s.get("count", 0) for s in (inv.get("hotbar") or []) + [inv.get("offhand")] if s and "snowball" in (s.get("item") or ""))
        ctl("key", "t", "tap")
        wait(2)
        x_keys(window, "type:rf")
        ctl("key", "enter", "tap")
        wait(10)
        typed = any(line.endswith("rf") for line in ctl("chat-log", "3").get("chat", []))
        got[name] = (hit, 16 - snow, typed, round(1000 - mobs("pig")[0]["health"], 1) == hit)
        only()
    ok = got["off"][0] == 0 and got["off"][1] == 0 and got["on"][0] > 0 and got["on"][1] >= 2 and got["on"][2] and got["on"][3]
    return ok, ("(pig health lost to 3 R, snowballs thrown by 3 F, chat line 'rf' sent, no hit while typing): "
                f"off {got['off']}, on {got['on']}")

if __name__ == "__main__":
    import signal
    signal.signal(signal.SIGALRM, overran)
    arena()
    for fn in TESTS:
        if ONLY and fn.test_name not in ONLY:
            continue
        signal.alarm(600)
        try:
            ok, note = fn()
        except Exception as e:  # noqa: BLE001
            ok, note = False, f"error: {e!r}"
        signal.alarm(0)
        rejoin(fn.test_name)
        if ok is not None:
            results.append((fn.test_name, ok, note))
        print("SKIP" if ok is None else "PASS" if ok else "FAIL", fn.test_name, "-", note, flush=True)
        (light_clean if getattr(fn, "light", False) else clean)()
    p = sum(1 for _, ok, _ in results if ok)
    print(f"\n{p}/{len(results)} passed")
    for test, screen in kicks:
        print(f"KICKED during {test}: {screen}")
