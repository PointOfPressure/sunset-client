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
    """Game commands, sent as the player without opening the chat box."""
    for line in lines:
        ctl("chat", "/" + line.lstrip("/"))
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
        if st.get("screen") != "dead" and "player" in st:
            return
        ctl("click", "respawn")
        wait(30)


def rejoin(test):
    """After a kick: what the screen said, then back in."""
    if "player" in state() or not SERVER:
        return
    shown = ctl("screen")
    kicks.append((test, json.dumps(shown)[:300]))
    for _ in range(3):
        ctl("key", "esc", "tap")
        time.sleep(2)
        ctl("connect", SERVER, "Tester", timeout=120)
        ctl("wait-ready", "90", timeout=120)
        if "player" in state():
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
    say(".panic")
    say(".reset hud")
    on.clear()
    for m in sorted(touched):
        say(f".reset {m}")
    touched.clear()
    # Land first, in whatever game mode, so a fall a test left you in costs nothing.
    cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
    wait(3)
    # A world with real terrain (a server's) has hills round the platform: clear a wide ring.
    cmd("gamemode survival @s", "clear @s", "effect clear @s", "xp set @s 0 levels", "xp set @s 0 points",
        "kill @e[type=!player]", "difficulty peaceful", "time set 6000", "weather clear", "gamerule natural_health_regeneration true",
        f"fill {X0-24} {Y-1} {Z0-24} {X0+24} {Y+4} {Z0+24} minecraft:air",
        f"fill {X0-24} {Y+5} {Z0-24} {X0+24} {Y+12} {Z0+24} minecraft:air",
        f"fill {X0-16} {Y} {Z0-16} {X0+16} {Y} {Z0+16} minecraft:stone",
        f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0",
        "effect give @s minecraft:instant_health 1 10 true", "effect give @s minecraft:saturation 1 10 true")
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


def peak(ticks=24):
    best = pos()[1]
    for _ in range(ticks // 2):
        wait(2)
        best = max(best, pos()[1])
    return best


@test("Parkour")
def _():
    reset(z=14.0)
    only("parkour")
    ctl("key", "w", "down")
    best = peak(30)
    ctl("key", "w", "up")
    only()
    reset(z=14.0)
    ctl("key", "w", "down")
    plain = peak(30)
    ctl("key", "w", "up")
    return best > Y + 1.3 and plain < Y + 1.05, f"at the edge: jumped to {best-Y-1:.2f} blocks up, {plain-Y-1:.2f} off"


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
    from PIL import Image, ImageStat
    path = f"/tmp/{name}.png"
    ctl("screenshot", path)
    time.sleep(2)
    with Image.open(path) as image:
        return sum(ImageStat.Stat(image.convert("RGB")).mean) / (3 * 255)


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
    only("sprint", "fastplace", "fullbright")
    mark = log_mark()
    say(".panic")
    wait(6)
    said = sunset_said(mark)
    sprinting = state()["player"].get("sprinting")
    on.clear()
    # Panic switched the HUD off too: put it back as it starts.
    say(".reset hud")
    return not sprinting and "Everything is off." in said, f"after .panic Sunset says {said}, sprinting: {sprinting}"


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
        with open(found[0]) as f, open(os.path.join(imports, "shared.profile"), "w", newline="\n") as g:
            g.write(f.read())
        with open(os.path.join(imports, "broken.profile"), "w", newline="\n") as g:
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
    status = ctl("status")
    # The framebuffer can differ from the monitor (windowed mode and desktop DPI).
    w, h = status["renderer"]["size"]
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
    if state().get("screen") == "paused":
        ctl("resume")
    wait(3)
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
    items = ",".join(f"{{Slot:{i}b,id:\"minecraft:stone\",count:1}}" for i in range(27))
    cmd(f"setblock {X0+1} {Y+1} {Z0} minecraft:chest[facing=west]{{Items:[{items}]}}")
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
    cmd("clear @s", f"setblock {X0+1} {Y+1} {Z0} minecraft:air", f"setblock {X0+1} {Y+1} {Z0} minecraft:chest[facing=west]{{Items:[{items}]}}")
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
    return gone and kept and bread, f"rotten flesh dropped: {gone}, bread kept: {kept}; bread dropped once listed: {bread}"


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


SPEED_CASES = [("stone_pickaxe", "stone"), ("iron_pickaxe", "stone"), ("diamond_pickaxe", "stone"), ("diamond_pickaxe", "obsidian")]


def speed_mine(mode):
    plain = [break_ticks(p, b, 1 if b == "obsidian" else 3) for p, b in SPEED_CASES]
    setting("speedmine", "mode", mode)
    only("speedmine")
    fast = [break_ticks(p, b, 1 if b == "obsidian" else 3) for p, b in SPEED_CASES]
    only()
    alive = "player" in state()
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
        # A 64-stack avoids exhausting the fast case before its sample ends.
        slot(0, "experience_bottle", 64)
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
    return fast > plain + 4, f"{plain} experience bottles used holding use for 2 s, {fast} with Fast Use"


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


@test("Kill Aura (look-only)")
def _():
    reset("creative")
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    pig(2.5)
    setting("killaura", "entities", "pig")
    setting("killaura", "only on look", "true")
    ctl("look", "180", "0")
    before = state()["swings"]
    only("killaura")
    wait(30)
    away = state()["swings"] - before
    ctl("look-at", str(X0 + 0.5), str(Y + 1.45), str(Z0 + 3.0))
    wait(40)
    facing = state()["swings"] - before
    only()
    return away == 0 and facing > 0, f"look-only swings looking away: {away}, facing the pig: {facing}"


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
    cmd("gamemode survival Bot1")

    def bot_hurt(seconds):
        cmd("effect give Bot1 minecraft:instant_health 1 10 true", f"tp Bot1 {X0+0.5} {Y+1} {Z0+2.0}")
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
    cmd("team add sunsettest", "team join sunsettest Tester", "team join sunsettest Bot1")
    setting("killaura", "ignore teams", "true")
    only("killaura")
    spared = bot_hurt(3)
    setting("killaura", "ignore teams", "false")
    hit = bot_hurt(3)
    only()
    cmd("team remove sunsettest", "gamemode spectator Bot1")
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
    return [Frame(line.split()) for line in lines if line]


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
    alive = "player" in state()
    broken = 54 - count(X0 - 4, Y + 1, Z0 - 10, X0 + 4, Y + 2, Z0 - 8, "dirt")
    placed = count(X0 - 2, Y, Z0 - 30, X0 + 2, Y, Z0 - 17, "stone")
    return ticks > 50 and most <= 1 and alive, (f"{moves} move packets over {ticks} ticks, at most {most} in a tick; Nuker broke {broken} dirt, "
                                                f"Scaffold placed {placed} past the edge; still in the game: {alive}")


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
    if "player" not in st:
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
    say(".reset hud")
    on.clear()
    for m in sorted(touched):
        say(f".reset {m}")
    touched.clear()


def overran(*_):
    raise TimeoutError("the test ran over 10 minutes")


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
