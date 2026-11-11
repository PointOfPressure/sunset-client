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
import json, math, os, re, subprocess, sys, time, traceback

CTL = sys.argv[1]
CTL2 = os.environ.get("CTL2")
# The server joined (HOST:PORT), to join again when it kicks you; the kicks are listed at the end.
SERVER = os.environ.get("SERVER")
# Where the screenshots go (runs side by side each need their own).
SHOTS = os.environ.get("SHOTS", "/tmp")
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
    # check() reads the command's answer: a server world a run left without command feedback answers nothing.
    cmd("gamerule send_command_feedback true")
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
    # Every module a test turned on back to its defaults (off), whatever it left (a toggle typed while dead never
    # arrives), then the HUD as it starts. Not .panic: that stops Sunset until the game restarts.
    for m in sorted(on | touched):
        say(f".reset {m}")
    say(".reset hud")
    on.clear()
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
    # Max health back too: a run stopped in a test that raised it (the explosive auras') leaves it raised in the world.
    cmd(f"gamemode {mode}", "clear @s", "effect clear @s", "attribute @s minecraft:max_health base reset",
        "effect give @s minecraft:instant_health 1 10 true",
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


@test("Speed (Bhop)")
def _():
    # Bhop holding W: the speed across the ground with Hop speed 11 and 15 (a hop on each landing), how high a hop
    # goes with Hop height 0.42 and 0.3, and the move the tick after letting go (Stop on release).
    plain = rate()
    only("speed")
    setting("speed", "mode", "Bhop")
    reset(z=-14.5)
    hop = rate(ticks=24)
    setting("speed", "hop speed", "15")
    reset(z=-14.5)
    fast = rate(ticks=24)
    setting("speed", "hop speed", "11")

    def peak():
        reset(z=-14.5)
        y0 = pos()[1]
        ctl("key", "w", "down")
        ys = poll(24, 1, lambda: pos()[1])
        ctl("key", "w", "up")
        wait(5)
        return max(ys) - y0

    high = peak()
    setting("speed", "hop height", "0.3")
    low = peak()
    setting("speed", "hop height", "0.42")
    reset(z=-14.5)
    ctl("key", "w", "down")
    wait(15)
    ctl("key", "w", "up")
    a = pos()
    wait(1)
    stop = flat(a, pos())
    setting("speed", "mode", "Vanilla")
    only()
    ok = hop > plain * 1.4 and fast > hop * 1.2 and 1.0 < high < 1.45 and low < high - 0.3 and stop < 0.02
    return ok, (f"walking {plain:.2f} blocks/s; Bhop Hop speed 11: {hop:.2f}, 15: {fast:.2f}; hop height {high:.2f} "
                f"(Hop height 0.42), {low:.2f} (0.3); moved {stop:.3f} the tick after letting go")


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


class BrandTap:
    """A relay between the game and SERVER that notes the client brand (minecraft:brand) of each connection made
    through it, as the server gets it, and keeps what the game sent (the first megabyte): an offline server's stream is
    unencrypted, and packets this small go uncompressed."""

    def __init__(self):
        import socket, threading
        self.brands = []
        self.streams = []
        self.listener = socket.create_server(("127.0.0.1", 0))
        self.port = self.listener.getsockname()[1]
        host, port = SERVER.rsplit(":", 1)

        def pipe(src, dst, stream=None):
            found = stream is None
            try:
                while chunk := src.recv(65536):
                    if stream is not None and len(stream) < 1 << 20:
                        stream += chunk
                        if not found and (brand := BrandTap.read(bytes(stream))) is not None:
                            self.brands.append(brand)
                            found = True
                    dst.sendall(chunk)
                dst.shutdown(socket.SHUT_WR)
            except OSError:
                pass

        def serve():
            while True:
                try:
                    game, _ = self.listener.accept()
                    server = socket.create_connection((host, int(port)))
                except OSError:
                    return
                self.streams.append(bytearray())
                threading.Thread(target=pipe, args=(game, server, self.streams[-1]), daemon=True).start()
                threading.Thread(target=pipe, args=(server, game), daemon=True).start()
        threading.Thread(target=serve, daemon=True).start()

    @staticmethod
    def read(buf):
        """The string after the channel name, once all of it is there."""
        at = buf.find(b"\x0fminecraft:brand")
        if at < 0:
            return None
        i, n, shift = at + 16, 0, 0
        while i < len(buf):
            b = buf[i]
            i += 1
            n |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                return buf[i:i + n].decode("utf-8", "replace") if len(buf) >= i + n else None
        return None


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
    path = f"{SHOTS}/{name}.png"
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
    path = f"{SHOTS}/{name}.png"
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


@test("Snap Tap")
def _():
    # Facing south: A held, then D as well, then D let go; then W held and S as well. With Snap Tap the key pressed
    # last moves you (right, then left again; back), without it A and D cancel out. Blocks moved in 10 ticks each
    # (x: + is left), the most move packets in a tick (probe, on a server) and kicks.
    def run(mods):
        reset()
        only(*mods)
        kicked = len(kicks)
        mark = probe_mark() if probe_path() and SERVER else None

        def moved(axis):
            wait(4)
            at = pos()[axis]
            wait(10)
            return round(pos()[axis] - at, 2)

        ctl("key", "a", "down")
        a = moved(0)
        ctl("key", "d", "down")
        both = moved(0)
        ctl("key", "d", "up")
        back = moved(0)
        ctl("key", "a", "up")
        ctl("key", "w", "down")
        wait(10)
        ctl("key", "s", "down")
        ws = moved(2)
        ctl("key", "s", "up")
        ctl("key", "w", "up")
        only()
        return a, both, back, ws, wire_stats(mark)[1] if mark is not None else None, len(kicks) - kicked

    off, on = run(()), run(("snaptap",))
    # Behind the inventory with GUI Move: A, then D as well.
    reset()
    only("snaptap", "guimove")
    ctl("open-inventory")
    wait(5)
    ctl("key", "a", "down")
    wait(10)
    ctl("key", "d", "down")
    wait(4)
    at = pos()[0]
    wait(10)
    screen = round(pos()[0] - at, 2)
    ctl("key", "d", "up")
    ctl("key", "a", "up")
    ctl("close")
    only()
    ok = (on[0] > 0.5 and on[1] < -0.5 and on[2] > 0.5 and on[3] < -0.5 and abs(off[1]) < 0.1 and abs(off[3]) < 0.1
          and screen < -0.5 and all((r[4] or 0) <= 1 and r[5] == 0 for r in (off, on)))
    return ok, (f"(A, then A+D, then D let go, then W+S; blocks moved, most moves a tick, kicks): off {off}, on {on}; "
                f"A+D behind the inventory with GUI Move: {screen}")


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


def inventory_wire(mark):
    with open(probe_path(), errors="replace") as stream:
        stream.seek(mark)
        return stream.read().splitlines()


@test("GUI Move (GrimAC prediction and packet order)")
def _():
    if not probe_path():
        return None, "needs tools/probe"
    outcomes = []
    for extra in ((), ("blink",), ("fakelag",)):
        reset(z=-10.5)
        slot(0, "stone", 8)
        setting("guimove", "mode", "GrimAC")
        if extra == ("fakelag",):
            setting("fakelag", "mode", "Delay")
        only("guimove", *extra)
        ctl("open-inventory")
        wait(4)
        mark = probe_mark()
        ctl("key", "w", "down")
        ctl("key", "ctrl", "down")
        wait(8)
        ctl("click-slot", "0")
        picked = (ctl("inventory").get("carried") or {}).get("count", 0)
        # An unrelated authoritative slot change must not remove the predicted cursor stack.
        cmd("item replace entity @s hotbar.2 with minecraft:torch 3")
        wait(5)
        rebased = (ctl("inventory").get("carried") or {}).get("count", 0)
        ctl("click-slot", "9")
        wait(3)
        held = [line for line in inventory_wire(mark) if line.startswith("inventory ")]
        ctl("close")
        wait(25)
        ctl("release")
        wait(4)
        lines = inventory_wire(mark)
        flags, sprint, safe, actions = 0, False, True, []
        for line in lines:
            if line.startswith("input "):
                flags = int(line.split()[1])
            elif line.startswith("sprint "):
                sprint = line.endswith("1")
            elif line.startswith("inventory "):
                actions.append(line)
                safe = safe and flags == 0 and not sprint
        same = check('execute if data entity @s Inventory[{Slot:9b,id:"minecraft:stone",count:8}]')
        click_slots = [int(line.split()[4]) for line in actions if line.startswith("inventory click ")]
        ok = picked == 8 and rebased == 8 and not held and safe and same and click_slots == [36, 9]
        ok = ok and actions[-1:] == ["inventory close 0"]
        outcomes.append((extra or ("plain",), ok, picked, rebased, actions, safe, same))
        only()
    return all(row[1] for row in outcomes), str(outcomes)


@test("GUI Move (GrimAC shift and empty close)")
def _():
    if not probe_path():
        return None, "needs tools/probe"
    reset(z=-10.5)
    slot(0, "stone", 8)
    setting("guimove", "mode", "GrimAC")
    only("guimove")
    ctl("open-inventory")
    mark = probe_mark()
    ctl("key", "w", "down")
    ctl("key", "shift", "down")
    wait(4)
    ctl("click-slot", "0", "shift")
    picked = (ctl("inventory").get("carried") or {}).get("count", 0)
    ctl("click-slot", "9")
    ctl("close")
    wait(25)
    ctl("release")
    wait(5)
    modes = [int(line.split()[-1]) for line in inventory_wire(mark) if line.startswith("inventory click ")]
    ctl("open-inventory")
    mark = probe_mark()
    ctl("close")
    wait(25)
    empty_close = "inventory close 0" in inventory_wire(mark)
    only()
    return picked == 8 and modes == [0, 0] and empty_close, f"moving Shift picked {picked}, click modes {modes}, empty close forwarded {empty_close}"


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


def inputs_sent(mark):
    """The PlayerInput flags the client sent since mark (the probe's "input" lines): forward 1, backward 2, left 4,
    right 8, jump 16, sneak 32, sprint 64."""
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        return [int(line.split()[1]) for line in f if line.startswith("input ")]


@test("GUI Move tells the server its keys")
def _():
    # Behind a screen the game sends no keys in PlayerInput; the server (Grim) predicts the move from those, so GUI
    # Move's must be there: forward and jump while jumping in the inventory, forward and sneak while sneaking, none
    # once let go; with its Jump setting off it neither jumps nor says it does.
    def run(jump=True):
        reset(z=-14.5)
        wait(10)
        ctl("open-inventory")
        wait(6)
        y0, mark = pos()[1], os.path.getsize(probe_path())
        ctl("key", "w", "down")
        wait(5)
        ctl("key", "space", "down")
        rise = 0.0
        for _ in range(5):
            wait(2)
            rise = max(rise, pos()[1] - y0)
        ctl("key", "space", "up")
        wait(15)
        if jump:
            ctl("key", "shift", "down")
            wait(10)
            ctl("key", "shift", "up")
        ctl("key", "w", "up")
        wait(5)
        ctl("close")
        return inputs_sent(mark), rise

    only("guimove")
    sent, rise = run()
    setting("guimove", "jump", "false")
    unjumped, flat_rise = run(jump=False)
    ok = {1, 17, 33} <= set(sent) and sent[-1:] == [0] and rise > 0.3
    ok = ok and 17 not in unjumped and 1 in unjumped and flat_rise < 0.05
    return ok, f"PlayerInput {sent} (rose {rise:.2f}); Jump off: {unjumped} (rose {flat_rise:.2f})"


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


@test("Panic (ends Sunset for the session, runs last)")
def _():
    # Hard Panic stops Sunset until the game restarts: no HUD or chat line of Sunset's, every module off (sprint let
    # go), its row gone from Options > Mods, its commands the server's chat (.panic again brings nothing back), no
    # plugin handler or task runs, and the player stays in the same server session.
    def mods_page():
        ctl("options", "mods")
        wait(3)
        labels = json.dumps(ctl("ui").get("widgets") or [])
        ctl("close")
        wait(2)
        return "Sunset" in labels

    plugins = "plugcheck events" in " ".join(said_since(log_mark(), ".plugcheck events"))
    if plugins:
        say(".plugcheck reset")
        say(".plugcheck watch")
    only("sprint", "noslow", "fullbright")
    listed = mods_page()
    shown = watermark()
    kicked = len(kicks)
    mark = log_mark()
    say(".panic")
    wait(6)
    sprinting = state()["player"].get("sprinting")
    on.clear()
    hidden = watermark()
    say(".panic")
    say(".modules")
    wait(6)
    said = sunset_said(mark)
    gone = not mods_page()
    wait(210)
    with open(os.path.join(session_dir(), "home", "logs", "latest.log"), errors="replace") as f:
        f.seek(mark)
        logged = [l.strip() for l in f if "plugcheck panic frozen" in l]
    expected = "plugcheck panic frozen true changed false stops 1 stale false tasks 0"
    plugin_ok = not plugins or any(expected in l for l in logged)
    connected = bool(state().get("player")) and len(kicks) == kicked
    ok = listed and shown and not sprinting and not hidden and not said and gone and plugin_ok and connected
    return ok, (f"listed before {listed}, HUD before {shown}; after .panic: sprinting {sprinting}, HUD {hidden}, "
                f"Sunset said {said}, Options > Mods without it {gone}, plugin bus {logged[-1:] if plugins else 'not staged'}, "
                f"still connected {connected}")


_.ends_session = True


@test("Panic: lifecycle and stale work")
def _():
    import pathlib, shutil, tempfile
    compiler = shutil.which("rustc")
    if not compiler:
        return None, "needs rustc for the standalone lifecycle tests"
    source = pathlib.Path(__file__).resolve().parents[1] / "mixins" / "sunset" / "lifecycle.rs"
    with tempfile.TemporaryDirectory(prefix="sunset-panic-") as directory:
        binary = pathlib.Path(directory) / ("tests.exe" if os.name == "nt" else "tests")
        compiled = subprocess.run([compiler, "--edition=2024", "--test", str(source), "-o", str(binary)], capture_output=True, text=True, timeout=120)
        if compiled.returncode:
            return False, compiled.stderr[-1500:]
        result = subprocess.run([str(binary)], capture_output=True, text=True, timeout=60)
        return result.returncode == 0, (result.stdout + result.stderr)[-1500:]


@test("Profiles (module)")
def _():
    # A profile picked in the list the Profiles module's row opens loads; the HUD's label (.profile list's "In use")
    # follows: the name, "(modified)" after a change, Custom once it is deleted.
    def listed():
        mark = log_mark()
        say(".profile list")
        wait(4)
        return next((c for c in sunset_said(mark) if c.startswith("Profiles:")), "")

    def range_shown():
        mark = log_mark()
        say(".settings killaura")
        wait(4)
        return next((c for c in sunset_said(mark) if c.startswith("Range:")), "")

    setting("killaura", "range", "3")
    mark = log_mark()
    say(".profile save picked")
    wait(4)
    said = sunset_said(mark)
    saved = listed()
    setting("killaura", "range", "5")
    modified = listed()
    names = saved.removeprefix("Profiles: ").split(". In use")[0].split(", ")
    if "picked" not in names:
        return False, f"not saved: {said} then {saved!r}"
    w, h = gui_size()
    ww, hh = min(w - 16, 300), min(min(max(len(names), 1), 10) * 18 + 47, h - 16)
    x, y = (w - ww) / 2, (h - hh) / 2
    say(".menu profiles")
    wait(5)
    ctl("click-at", *map(str, menu_row(5, 1)))
    wait(3)
    ctl("click-at", str(x + 30), str(y + 25 + 18 * names.index("picked") + 9))
    wait(3)
    close_menu()
    loaded, after = range_shown(), listed()
    say(".profile delete picked")
    gone = listed()
    # With it gone and no other profile, the list has no "Profiles:" line.
    ok = (saved.endswith("In use: picked") and modified.endswith("In use: picked (modified)") and loaded == "Range: 3"
          and after.endswith("In use: picked") and "picked" not in gone.split(". In use")[0] and (not gone or gone.endswith("In use: Custom")))
    return ok, f"saved: {saved!r}; changed: {modified!r}; picked in the menu: {loaded!r}, {after!r}; deleted: {gone!r}"


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


def said_after(line, wait_ticks=4):
    """What Sunset says in the chat after `line` is typed."""
    mark = log_mark()
    say(line)
    wait(wait_ticks)
    return sunset_said(mark)


def modules_on():
    """The modules .modules marks as on."""
    names = []
    for line in said_after(".modules"):
        if ": " in line:
            names += [n[:-1] for n in line.split(": ", 1)[1].split(", ") if n.endswith("*")]
    return names


@test("Modules")
def _():
    # Jesus is hidden by default: the menu's search leaves it out (Enter on "jesus" turns nothing on) until its switch
    # in Modules is on; .t reaches it either way. Pearl Catch shows by default, and Kill Aura's switch off leaves it out.
    # A left click on Modules' row opens its switches (a screenshot) instead of turning it on.
    say(".reset modules")
    say(".menu")
    wait(6)
    ctl("screenshot", f"{SHOTS}/menu.png")
    time.sleep(2)
    close_menu()
    say(".menu jesus")
    wait(6)
    ctl("key", "enter", "tap")
    wait(3)
    close_menu()
    hidden = "Jesus" in modules_on()
    say(".t jesus")
    by_command = "Jesus" in modules_on()
    say(".t jesus")
    setting("modules", "jesus", "true")
    say(".menu jesus")
    wait(6)
    ctl("key", "enter", "tap")
    wait(3)
    close_menu()
    listed = "Jesus" in modules_on()
    if listed:
        say(".t jesus")

    def enter_on(query, name):
        say(f".menu {query}")
        wait(6)
        ctl("key", "enter", "tap")
        wait(3)
        close_menu()
        on = name in modules_on()
        if on:
            say(f".t {query}")
        return on

    pearl = enter_on("pearl catch", "Pearl Catch")
    setting("modules", "kill aura", "false")
    aura_hidden = enter_on("kill aura", "Kill Aura")
    say(".menu modules")
    wait(6)
    ctl("click-at", *map(str, menu_row(5, 1)))
    wait(5)
    ctl("screenshot", f"{SHOTS}/modules.png")
    time.sleep(2)
    close_menu()
    switch = "Modules" in modules_on()
    say(".reset modules")
    ok = not hidden and by_command and listed and pearl and not aura_hidden and not switch
    return ok, (f"Enter on 'jesus' while hidden turned it on: {hidden}; .t jesus while hidden: {by_command}; Enter once listed: "
                f"{listed}; Pearl Catch shown by default: {pearl}; Kill Aura switched off still turned on: {aura_hidden}; "
                f"a click on Modules turned it on: {switch} (shot {SHOTS}/modules.png)")


@test("Old profile converts")
def _():
    # A profile from before 0.9: Auto Steal and No Mining Trace by their old names, Reverse Step on its own, Potion
    # Saver. Imported and loaded: Chest Stealer and Mine Through Mobs have their settings, Step is on with only its
    # Reverse step section, Potion Saver's lines are gone, and Modules shows the hidden modules it used.
    found = [os.path.join(r, f) for r, d, fs in os.walk(os.path.join(session_dir(), "home")) for f in d if f == "imports"]
    if not found:
        say(".profile list")
        found = [os.path.join(r, f) for r, d, fs in os.walk(os.path.join(session_dir(), "home")) for f in d if f == "imports"]
    if not found:
        return False, "no imports folder"
    with open(os.path.join(found[0], "oldcfg.profile"), "w") as f:
        f.write("sunset-profile=1\nautosteal.on=true\nautosteal.itemspertick=3\nreversestep.on=true\nreversestep.speed=2\n"
                "step.on=false\npotionsaver.on=true\nnominingtrace.on=true\nnominingtrace.onlywithapickaxe=false\n")
    imported = said_after(".profile import oldcfg")
    loaded = said_after(".profile load oldcfg")
    stealer = said_after(".settings cheststealer")
    step = said_after(".settings step")
    mine = said_after(".settings minethroughmobs")
    extras = said_after(".settings modules")
    now_on = modules_on()
    say(".menu")
    wait(6)
    ctl("screenshot", f"{SHOTS}/old_profile_loaded.png")
    time.sleep(2)
    close_menu()
    say(".profile load before-load")
    say(".profile delete oldcfg")
    for m in ("cheststealer", "step", "minethroughmobs", "modules"):
        touched.add(m)
    ok = (any("Imported" in l for l in imported) and any("Loaded" in l for l in loaded) and "Items per tick: 3" in stealer
          and "Step up: OFF" in step and "Reverse step: ON" in step and "Fall speed: 2" in step
          and "Only with a pickaxe: OFF" in mine and "Step: ON" in extras and "Mine Through Mobs: ON" in extras
          and {"Chest Stealer", "Step", "Mine Through Mobs"} <= set(now_on))
    return ok, (f"import {imported[:1]}, load {loaded[:1]}; Chest Stealer {[l for l in stealer if 'tick' in l]}; Step "
                f"{[l for l in step if 'step' in l.lower() or 'Fall' in l]}; Mine Through Mobs {mine[-1:]}; Modules on: "
                f"{[l for l in extras if l.endswith(': ON')]}; on: {[n for n in now_on if n in ('Chest Stealer', 'Step', 'Mine Through Mobs')]}")


@test("Presets")
def _():
    # .preset pvp-crystal says what it turns on and off and changes nothing; .preset pvp-crystal apply turns Crystal
    # Aura and Fullbright on and Kill Aura off (HUD stays); .profile load before-load undoes it. Then in the menu: the
    # Profiles row, the Presets tab, the first preset's preview (a screenshot), Enter applies it.
    say(".reset killaura")
    toggle("killaura", True)
    listing = said_after(".preset")
    preview = said_after(".preset pvp-crystal")
    # PvP: mace (Sayer's mace setup) reads whole: its preview turns Aim Assist and Wind Jump on.
    mace = said_after(".preset pvp-mace")
    mace_ok = any(l.startswith("Turns on:") and "Aim Assist" in l and "Wind Jump" in l for l in mace)
    unchanged = "Crystal Aura" not in modules_on()
    taken = said_after(".preset pvp-crystal apply")
    after = modules_on()
    applied = {"Crystal Aura", "Fullbright", "HUD"} <= set(after) and "Kill Aura" not in after
    say(".profile load before-load")
    undone = "Kill Aura" in modules_on() and "Crystal Aura" not in modules_on()
    # The menu: with no profile saved it opens on Presets; with one, the tab is clicked.
    w, h = gui_size()
    ww = min(w - 16, 300)
    saved = said_after(".profile list")
    say(".menu profiles")
    wait(5)
    ctl("click-at", *map(str, menu_row(5, 1)))
    wait(3)
    count = len(next((l for l in listing if l.startswith("Presets:")), "Presets: x.").split(". ")[0].split(", "))
    hh = min(count * 18 + 47, h - 16)
    x, y = (w - ww) / 2, (h - hh) / 2
    if saved and saved[0].startswith("Profiles:"):
        names = saved[0].removeprefix("Profiles: ").split(". In use")[0].split(", ")
        sh = min(min(max(len(names), 1), 10) * 18 + 47, h - 16)
        ctl("click-at", str(x + ww - 58), str((h - sh) / 2 + 11))
        wait(3)
    ctl("screenshot", f"{SHOTS}/presets_list.png")
    time.sleep(2)
    ctl("click-at", str(x + 30), str(y + 25 + 9))
    wait(4)
    ctl("screenshot", f"{SHOTS}/preset_preview.png")
    time.sleep(2)
    ctl("key", "enter", "tap")
    wait(4)
    ctl("screenshot", f"{SHOTS}/preset_applied.png")
    time.sleep(2)
    close_menu()
    by_menu = modules_on()
    menu_applied = "Crystal Aura" in by_menu and "Kill Aura" not in by_menu
    say(".profile load before-load")
    toggle("killaura", False)
    ok = (any("pvp-crystal" in l for l in listing) and any(l.startswith("Turns on:") and "Crystal Aura" in l for l in preview)
          and any(l.startswith("Turns off:") and "Kill Aura" in l for l in preview) and unchanged and applied and undone
          and menu_applied and any("Took preset" in l for l in taken) and mace_ok)
    return ok, (f"list {listing[:1]}; preview {preview[:3]}; mace preview {mace[:3]}; unchanged before apply: {unchanged}; applied: {applied} ({taken[:1]}); "
                f"undone: {undone}; from the menu: {menu_applied} (shot {SHOTS}/preset_preview.png)")


def gui_size():
    """The GUI's size in its own pixels: the window over the game's GUI scale (auto: the largest that keeps 320x240)."""
    d = ctl("status")["display"]
    w, h = d["width"], d["height"]
    scale = int(ctl("options-get").get("guiScale", {}).get("value", 0) or 0)
    if scale <= 0:
        scale = max(1, min(w // 320, h // 240))
    return w / scale, h / scale


def panel_width():
    """A panel's width at the defaults: 100, or less so that the six (Combat to Client) fit in a row."""
    w, _ = gui_size()
    return min(100, (w - 4 - 3 * 5) // 6)


def menu_row(panel, row):
    """The middle of a row of a panel (Combat 0, Movement 1, Player 2...) as the menu first lays them out at the
    defaults (Comfortable spacing): row 0 is the header, then rows of 14."""
    width = panel_width()
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


@test("ClickGUI Dashboard (look and frame cost)")
def _():
    # Style Dashboard at GUI scale 2 and 4: screenshots (dash2.png, dash4.png), and its #121214 surface fills the
    # middle of the screen. Then the game thread's CPU ms a frame with the menu shut, open as panels and open as the
    # Dashboard (the median of 4 samples of 2 s): the Dashboard should cost under 0.3 ms over the panels.
    from PIL import Image
    sock = os.path.join(session_dir(), "c.sock").encode()
    stat = None
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                args = f.read().split(b"\0")
        except OSError:
            continue
        if args[0].endswith(b"horizon-client") and sock in args:
            stat = f"/proc/{pid}/task/{pid}/schedstat"

    def cpu():
        with open(stat) as f:
            return int(f.read().split()[0])

    def sample():
        a, ca = ctl("status"), cpu()
        time.sleep(2)
        b, cb = ctl("status"), cpu()
        return (cb - ca) / 1e6 / max(b["frames"] - a["frames"], 1)

    setting("clickgui", "style", "Dashboard")
    surface = {}
    for scale in (2, 4):
        ctl("options-set", "guiScale", str(scale))
        wait(5)
        say(".menu")
        wait(10)
        ctl("screenshot", f"{SHOTS}/dash{scale}.png")
        time.sleep(2)
        img = Image.open(f"{SHOTS}/dash{scale}.png").convert("RGB")
        middle = img.crop((img.width // 2 - 40, img.height * 3 // 4, img.width // 2 + 40, img.height * 3 // 4 + 20))
        surface[scale] = sum(1 for px in middle.getdata() if px == (0x12, 0x12, 0x14)) / (80 * 20)
        close_menu()
    ctl("options-set", "guiScale", "0")
    ms = {"shut": [], "Dropdown": [], "Dashboard": []}
    if stat:
        for _ in range(4):
            for state in ms:
                if state != "shut":
                    setting("clickgui", "style", state)
                    say(".menu")
                wait(20)
                ms[state].append(sample())
                if state != "shut":
                    close_menu()
    setting("clickgui", "style", "Dropdown")
    med = {k: sorted(v)[len(v) // 2] if v else None for k, v in ms.items()}
    extra = med["Dashboard"] - med["Dropdown"] if stat else None
    ok = all(v > 0.5 for v in surface.values()) and extra is not None and extra < 0.3
    return ok, (f"surface share in the middle: {surface}; game thread ms a frame: shut {med['shut']:.3f}, panels "
                f"{med['Dropdown']:.3f}, Dashboard {med['Dashboard']:.3f} (Dashboard over panels {extra:+.3f})" if stat else "no game thread found")


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
    open_picker("diamond", f"{SHOTS}/picker.png")
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
        open_picker("", f"{SHOTS}/picker_simple.png")
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
        path = f"{SHOTS}/{name}.png"
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


@test("Clear Water")
def _():
    # A lake 4 deep over diamond ore in a stone rim, seen from a barrier 6 blocks above it and from inside it. Each
    # frame (a patch of the lake clear of the chat) is compared with the same view drained: Clear Water at 0% must
    # look nearly drained, at 30% much closer to drained than plain water, at 100% with no tint like plain water, and
    # under water (its fog hidden) closer than plain. The frame rate with it on holds against off.
    from PIL import ImageStat

    def patch(name, settle=80):
        wait(settle)
        f = colour_frame(name)
        w, h = f.size
        return ImageStat.Stat(f.crop((w * 2 // 5, h // 5, w * 7 // 10, h * 9 // 20))).mean

    def apart(a, b):
        return sum(abs(x - y) for x, y in zip(a, b)) / 3

    def fps():
        a = ctl("status")
        wait(60)
        b = ctl("status")
        return (b["frames"] - a["frames"]) / max(b["uptime_seconds"] - a["uptime_seconds"], 1e-6)

    def up():
        cmd(f"tp @s {X0+0.5} {Y+11} {Z0+0.5}")
        ctl("look", "0", "90")

    def down():
        cmd(f"tp @s {X0+0.5} {Y+2} {Z0-2.5}")
        ctl("look", "0", "30")

    reset("creative")
    # The frame rate uncapped, so a cost would show.
    cap = ctl("options-get", "maxFps")["options"]["maxFps"]["value"]
    ctl("options-set", "maxFps", "260")
    lake = f"{X0-3} {Y+2} {Z0-3} {X0+3} {Y+5} {Z0+3}"
    cmd(f"fill {X0-4} {Y+1} {Z0-4} {X0+4} {Y+5} {Z0+4} minecraft:stone", f"fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+1} {Z0+3} minecraft:diamond_ore",
        f"fill {lake} minecraft:air", f"setblock {X0} {Y+10} {Z0} minecraft:barrier", "effect give @s minecraft:night_vision 600 0 true")
    down()
    dry_under = patch("cw_drained_under")
    up()
    dry = patch("cw_drained")
    cmd(f"fill {lake} minecraft:water")
    plain = apart(dry, patch("cw_plain"))
    fps_off = fps()
    setting("clearwater", "opacity", "0")
    toggle("clear water", True)
    gone = apart(dry, patch("cw_0"))
    fps_on = fps()
    setting("clearwater", "opacity", "30")
    faint = apart(dry, patch("cw_30"))
    setting("clearwater", "opacity", "100")
    setting("clearwater", "lighter tint", "0")
    full = apart(dry, patch("cw_100"))
    setting("clearwater", "opacity", "30")
    setting("clearwater", "lighter tint", "50")
    down()
    under = apart(dry_under, patch("cw_under"))
    toggle("clear water", False)
    under_plain = apart(dry_under, patch("cw_under_plain"))
    up()
    back = apart(dry, patch("cw_back"))
    cmd(f"fill {X0-4} {Y+1} {Z0-4} {X0+4} {Y+10} {Z0+4} minecraft:air", "effect clear @s")
    ctl("options-set", "maxFps", str(cap))
    ok = (gone < plain * 0.25 and faint < plain * 0.6 and abs(full - plain) < plain * 0.15 + 2 and under < under_plain * 0.6
          and abs(back - plain) < plain * 0.15 + 2 and fps_on > fps_off * 0.85)
    return ok, (f"colour off the drained lake: plain water {plain:.1f}, 0% {gone:.1f}, 30% {faint:.1f}, 100% no tint {full:.1f}, "
                f"off again {back:.1f}; under water {under:.1f} on, {under_plain:.1f} off; {fps_off:.0f} fps off, {fps_on:.0f} on "
                f"(frames in {SHOTS}/cw_*.png)")


@test("Ore Sim")
def _():
    # The world's seed from /seed, then Ore Sim on with it: when the chunks round you are done it says how many ores
    # it worked out and how many of those the server already shows as ore. No test server hides ores (no
    # anti-xray), so with the right seed most must show (deep ores all of them; coal and copper near the surface
    # less, where the arena's builds and trees change the floor the generator measured); with the seed one off,
    # hardly any.
    reset("creative")
    mark = log_mark()
    cmd("seed")
    wait(5)
    found = [re.search(r"Seed: \[(-?\d+)\]", line) for line in chat_since(mark)]
    seed = next((int(f.group(1)) for f in found if f), None)
    if seed is None:
        return False, f"no seed in chat: {chat_since(mark)[-3:]}"
    setting("oresim", "chunks", "2")

    def run(value):
        setting("oresim", "seed", str(value))
        mark = log_mark()
        toggle("ore sim", True)
        line = ""
        for _ in range(60):
            wait(10)
            line = next((l for l in sunset_said(mark) if l.startswith("Ore Sim:")), "")
            if line:
                break
        if value == seed:
            # The boxes through the ground below the arena, for the record.
            ctl("look", "0", "70")
            wait(20)
            ctl("screenshot", f"{SHOTS}/ore_sim.png")
            time.sleep(2)
        toggle("ore sim", False)
        m = re.search(r"(\d+) ores in (\d+) chunks, (\d+) of them", line)
        return (int(m.group(1)), int(m.group(2)), int(m.group(3))) if m else None, line

    right, line = run(seed)
    wrong, line2 = run(seed + 1)
    ok = (right is not None and wrong is not None and right[0] >= 100 and right[2] >= 0.6 * right[0]
          and wrong[2] <= 0.1 * max(wrong[0], 1))
    return ok, f"seed {seed}: {line!r}; seed {seed + 1}: {line2!r}"


@test("Base Finder")
def _():
    # A "base" (5 chests, 4 beds, 3 signs, a crafting table and a furnace: 14 listed blocks) is built east, in a
    # chunk the client has not loaded. Base
    # Finder on, the player goes there: the chunk is looked at on its own thread as it loads, and said in chat with
    # its place, what is in it and a waypoint. Then a second base built in a loaded chunk is found when Base Finder
    # goes on again. The arena's chunks (none of the base blocks in numbers) are not said.
    # The client sees 4 chunks while it runs, so on a server a chunk 120 blocks east is loaded there (where /fill
    # works) and not on the client; single player's server follows the client's distance: 400 east, kept loaded with
    # /forceload while the commands run.
    bx, by, bz = X0 + (120 if SERVER else 400), Y + 1, Z0 + 8
    view = ctl("options-get", "renderDistance")["options"]["renderDistance"]["value"]

    def build(x, y, z, block="minecraft:air"):
        if block == "minecraft:air":
            return [f"fill {x} {y} {z} {x+4} {y+2} {z} minecraft:air"]
        return [f"fill {x} {y} {z} {x+4} {y} {z} minecraft:chest", f"fill {x} {y+1} {z} {x+3} {y+1} {z} minecraft:red_bed[part=foot]",
                f"fill {x} {y+2} {z} {x+2} {y+2} {z} minecraft:oak_sign", f"setblock {x+3} {y+2} {z} minecraft:crafting_table",
                f"setblock {x+4} {y+2} {z} minecraft:furnace"]

    def said_base(mark, x, z):
        return [line for line in sunset_said(mark) if line.startswith("Base?") and f"chunk {x >> 4} {z >> 4} " in line]

    reset("creative")
    ctl("options-set", "renderDistance", "4")
    wait(60)
    cmd(f"forceload add {bx} {bz}")
    wait(20)
    cmd(*build(bx, by, bz, "base"))
    wait(5)
    cmd(f"forceload remove {bx} {bz}")
    wait(5)
    mark = log_mark()
    # Every line at once: the world's own builds (a server's test world has some) don't hold this one back.
    setting("basefinder", "message cooldown", "0")
    toggle("base finder", True)
    wait(40)
    ax, az = X0 >> 4, Z0 >> 4
    near = [line for line in sunset_said(mark) if line.startswith("Base?")
            and any(f"chunk {ax + dx} {az + dz} " in line for dx in range(-2, 3) for dz in range(-2, 3))]
    far_before = said_base(mark, bx, bz)
    cmd(f"tp @s {bx + 0.5} {by + 4} {bz - 4.5}")
    t0 = time.time()
    far = []
    for _ in range(30):
        wait(10)
        far = said_base(mark, bx, bz)
        if far:
            break
    took = time.time() - t0
    cmd(*build(bx, by, bz))
    cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5}")
    ctl("options-set", "renderDistance", str(view))
    wait(60)
    # A base in a chunk already loaded: found when Base Finder goes on again.
    cmd(*build(X0 + 3, Y + 1, Z0 + 6, "base"))
    wait(10)
    mark2 = log_mark()
    toggle("base finder", False)
    toggle("base finder", True)
    wait(40)
    again = said_base(mark2, X0 + 3, Z0 + 6)

    def base_points():
        mark = log_mark()
        say(".waypoint list")
        wait(4)
        return [line for line in sunset_said(mark) if line.startswith(("Base ", "Base:"))]

    # Off: its waypoints go (a waypoint of your own called Base stays), nothing more is said, and it stays off.
    say(".waypoint add Base")
    wait(4)
    points_on = base_points()
    mark3 = log_mark()
    toggle("base finder", False)
    cmd(*build(X0 + 3, Y + 1, Z0 + 6, "base"))
    wait(40)
    points_off = base_points()
    said_off = [line for line in sunset_said(mark3) if line.startswith("Base?")]
    say(".waypoint remove Base")
    cmd(*build(X0 + 3, Y + 1, Z0 + 6))
    line = far[0] if far else ""
    off_ok = len(points_on) >= 2 and [p.split(":")[0] for p in points_off] == ["Base"] and not said_off
    ok = bool(far) and not far_before and bool(again) and not near and all(k in line for k in ("5 chest", "4 bed", "3 sign")) and off_ok
    return ok, (f"fresh chunk said {took:.1f} s after the teleport: {line!r}; before loading: {len(far_before)}; "
                f"loaded chunk on turning on: {len(again)}; arena chunks said: {len(near)}; waypoints on: {points_on}, "
                f"after off: {points_off}; said after off: {said_off}")


@test("Base Finder signs")
def _():
    # Base Finder v2's single signs, each alone in its own loaded chunk near the arena: one shulker box (List 1 takes
    # one), a written sign, a soul sand bubble column, bedrock above the floor, a spawner with nothing of a dungeon
    # around it, a block above the terrain (y 300), a name-tagged cow and a boat. Turned on, Base Finder says each
    # chunk with its reason (cooldown 0 here so they come at once).
    ax, az = X0 >> 4, Z0 >> 4

    def at(dx, dz):
        return (ax + dx) * 16 + 8, Y + 1, (az + dz) * 16 + 8

    spots = {
        "shulker box": ((3, -3), lambda x, y, z: [f"setblock {x} {y} {z} minecraft:shulker_box"]),
        "a written sign": ((3, -1), lambda x, y, z: [f"setblock {x} {y} {z} minecraft:oak_sign{{front_text:{{messages:['\"hi\"','\"\"','\"\"','\"\"']}}}}"]),
        "a soul sand bubble column": ((3, 1), lambda x, y, z: [f"setblock {x} {y} {z} minecraft:soul_sand",
                                                               f"fill {x} {y+1} {z} {x} {y+3} {z} minecraft:bubble_column[drag=false]"]),
        "bedrock where none grows": ((3, 3), lambda x, y, z: [f"setblock {x} {y} {z} minecraft:bedrock"]),
        "a spawner with none of its structure": ((-3, -3), lambda x, y, z: [f"setblock {x} {y} {z} minecraft:spawner"]),
        "a build above the terrain": ((-3, -1), lambda x, y, z: [f"setblock {x} 300 {z} minecraft:stone"]),
        "a name-tagged mob": ((1, 1), lambda x, y, z: [f"summon minecraft:cow {x} {y} {z} {{NoAI:1b,PersistenceRequired:1b,CustomName:'\"Bessie\"'}}"]),
        "a boat": ((-1, -1), lambda x, y, z: [f"summon minecraft:oak_boat {x} {y} {z}"]),
    }
    reset("creative")
    toggle("base finder", False)
    setting("basefinder", "message cooldown", "0")
    for (d, place) in spots.values():
        x, y, z = at(*d)
        cmd(f"fill {x-1} {y} {z-1} {x+1} {y+4} {z+1} minecraft:air", f"setblock {x} 300 {z} minecraft:air")
        cmd(*place(x, y, z))
    wait(20)
    mark = log_mark()
    toggle("base finder", True)
    wait(60)
    said = [line for line in sunset_said(mark) if line.startswith("Base?")]
    toggle("base finder", False)
    for (d, _) in spots.values():
        x, y, z = at(*d)
        cmd(f"fill {x-1} {y} {z-1} {x+1} {y+4} {z+1} minecraft:air", f"setblock {x} 300 {z} minecraft:air")
    cmd("kill @e[type=minecraft:cow,name=Bessie]", "kill @e[type=minecraft:oak_boat]")
    missing = []
    for why, (d, _) in spots.items():
        x, _, z = at(*d)
        line = next((l for l in said if f"chunk {x >> 4} {z >> 4} " in l), None)
        if line is None or why not in line:
            missing.append(f"{why}: {line!r}")
    return not missing, f"{len(said)} chunks said; wrong or missing: {missing}"


def chunk_spot(dx, dz, y=None):
    """The middle of the chunk `dx`, `dz` chunks from the arena's."""
    return ((X0 >> 4) + dx) * 16 + 8, Y + 1 if y is None else y, ((Z0 >> 4) + dz) * 16 + 8


@test("Stash Finder")
def _():
    # Five chests in a chunk beside the arena: found when Stash Finder turns on, once, with what they are.
    x, y, z = chunk_spot(0, 3)
    reset("creative")
    cmd(f"fill {x-2} {y} {z} {x+2} {y} {z} minecraft:chest")
    wait(10)
    mark = log_mark()
    only("stashfinder")
    wait(40)
    said = [l for l in sunset_said(mark) if l.startswith("Stash?")]
    only()
    cmd(f"fill {x-2} {y} {z} {x+2} {y} {z} minecraft:air")
    line = next((l for l in said if f"chunk {x >> 4} {z >> 4} " in l), "")
    return "5 chest" in line, f"said: {said}"


@test("Cave Disturbance and Portal Pattern")
def _():
    # Two pockets of cave air up in the sky beside the arena: one with a single plain air block in the middle (a
    # disturbance), one with a portal's shape of plain air across it (an imprint). Each detector finds its own.
    cx, cy, cz = chunk_spot(3, 0, Y + 12)
    px, py, pz = chunk_spot(-3, 0, Y + 12)
    reset("creative")
    # A server's /fill can take air and cave air for the same block ("No blocks were filled"): by way of stone.
    cmd(f"fill {cx-3} {cy-3} {cz-3} {cx+3} {cy+3} {cz+3} minecraft:stone", f"fill {cx-3} {cy-3} {cz-3} {cx+3} {cy+3} {cz+3} minecraft:cave_air",
        f"setblock {cx} {cy} {cz} minecraft:air")
    cmd(f"fill {px-3} {py-1} {pz-1} {px+2} {py+5} {pz+1} minecraft:stone", f"fill {px-3} {py-1} {pz-1} {px+2} {py+5} {pz+1} minecraft:cave_air")
    cmd(f"fill {px-2} {py} {pz} {px+1} {py+4} {pz} minecraft:stone")
    cmd(f"fill {px-2} {py} {pz} {px+1} {py+4} {pz} minecraft:air")
    wait(10)
    mark = log_mark()
    only("cavedisturbancedetector", "portalpatternfinder")
    wait(40)
    said = sunset_said(mark)
    only()
    for x1, y1, z1, x2, y2, z2 in ((cx-3, cy-3, cz-3, cx+3, cy+3, cz+3), (px-3, py-1, pz-1, px+2, py+5, pz+1)):
        cmd(f"fill {x1} {y1} {z1} {x2} {y2} {z2} minecraft:stone", f"fill {x1} {y1} {z1} {x2} {y2} {z2} minecraft:air")
    cave = [l for l in said if l.startswith("Cave disturbance at")]
    portal = [l for l in said if l.startswith("Portal imprint at")]
    ok = f"Cave disturbance at {cx} {cy} {cz}" in cave and f"Portal imprint at {px-2} {py} {pz}" in portal
    return ok, f"disturbances: {cave}; imprints: {portal}"


@test("Activated Spawner Detector")
def _():
    # Two spawners far enough off that the server does not run them: one whose countdown has moved (Delay 150,
    # as one a player came near), one as generated (Delay 20). Only the first is said.
    ax, ay, az = chunk_spot(-3, 2)
    bx, by, bz = chunk_spot(3, 2)
    reset("creative")
    cmd(f"setblock {ax} {ay} {az} minecraft:spawner{{Delay:150s}}", f"setblock {bx} {by} {bz} minecraft:spawner{{Delay:20s}}")
    wait(10)
    mark = log_mark()
    only("activatedspawnerdetector")
    wait(40)
    said = [l for l in sunset_said(mark) if l.startswith("Activated")]
    only()
    cmd(f"setblock {ax} {ay} {az} minecraft:air", f"setblock {bx} {by} {bz} minecraft:air")
    ok = any(f"at {ax} {ay} {az}" in l for l in said) and not any(f"at {bx} {by} {bz}" in l for l in said)
    return ok, f"said: {said}"


@test("Gear ESP")
def _():
    # A husk in a netherite chestplate and a plain one: the first is said and boxed.
    reset("creative")
    cmd("difficulty easy")
    cmd(f"summon minecraft:husk {X0+3} {Y+1} {Z0+5} {{NoAI:1b,PersistenceRequired:1b,Silent:1b,Tags:[\"gear\"],"
        f"equipment:{{chest:{{id:\"minecraft:netherite_chestplate\",count:1}}}}}}")
    cmd(f"summon minecraft:husk {X0-3} {Y+1} {Z0+5} {{NoAI:1b,PersistenceRequired:1b,Silent:1b,Tags:[\"gear\"]}}")
    ctl("look", "0", "10")
    wait(10)
    pink = lambda r, g, b: r > 230 and 90 < g < 140 and b > 170
    before = shot_pixels("gear0", pink)
    mark = log_mark()
    only("gearesp")
    wait(30)
    boxed = shot_pixels("gear1", pink)
    said = [l for l in sunset_said(mark) if l.startswith("A husk")]
    only()
    cmd("kill @e[tag=gear]")
    ok = len(said) == 1 and "netherite chestplate" in said[0] and boxed > before + 50
    return ok, f"said: {said}; pink pixels {before} -> {boxed}"


def new_chunks_file():
    """New Chunks' saved chunks for this world and dimension: {(x, z): 'n' or 'o'}."""
    out = {}
    for root, _, files in os.walk(session_dir()):
        for name in files:
            if os.path.basename(root) == "newchunks" and name.endswith("overworld.txt"):
                with open(os.path.join(root, name)) as f:
                    for line in f:
                        x, z, v = line.split()
                        out[(int(x), int(z))] = v
    return out


@test("New Chunks")
def _():
    # New Chunks on where the world has been played (the arena: old chunks, read from its save), then far off where
    # nobody has been: the chunks generated for the visit are new. What it judged is in its saved file.
    # The arena's chunks are sent again (the view made smaller and back) with New Chunks on. Horizon's server packs
    # palettes in block order and sends new chunks with liquid already flowing, so there (single player too) only
    # liquid starting to flow answers: some new chunks, no old ones.
    reset("creative")
    only("newchunks")
    view = ctl("options-get", "renderDistance")["options"]["renderDistance"]["value"]
    ctl("options-set", "renderDistance", "2")
    wait(60)
    ctl("options-set", "renderDistance", str(view))
    wait(100)
    here = new_chunks_file()
    far = X0 + 30000 + (int(time.time()) % 100000) * 16
    cmd(f"tp @s {far} 200 {Z0}")
    wait(200)
    there = {k: v for k, v in new_chunks_file().items() if abs(k[0] - (far >> 4)) <= 3 and abs(k[1] - (Z0 >> 4)) <= 3}
    ctl("look", "0", "60")
    wait(20)
    red = lambda r, g, b: r > 120 and g < 80 and b < 80
    tinted = shot_pixels("newchunks", red)
    cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5}")
    only()
    wait(40)
    old_here = sum(1 for v in here.values() if v == "o")
    new_there = sum(1 for v in there.values() if v == "n")
    if not SERVER or SERVER.endswith(":25599"):
        # Horizon's server: only liquid starting to flow tells, and only of new chunks.
        ok = old_here == 0 and new_there >= 1 and new_there == len(there) and tinted > 0
    else:
        ok = old_here >= len(here) * 0.8 and len(here) > 20 and new_there >= len(there) * 0.8 and len(there) >= 20 and tinted > 2000
    return ok, (f"arena: {old_here} old of {len(here)} judged; far off: {new_there} new of {len(there)} judged "
                f"(chunks within 3); red pixels looking down there {tinted}")


@test("Player Activity Detector")
def _():
    # Bot1 keeps the arena loaded on the server while Tester is far off; a block put down and taken away there
    # leaves its state in the chunk's palette. When Tester comes back the chunk is said (Recent visit 5 s, so
    # Tester's own earlier visit doesn't count). Horizon's server packs its palettes: not there.
    if not CTL2:
        return None, "needs a second player (CTL2)"
    if not SERVER or SERVER.endswith(":25599"):
        return None, "Horizon's server packs its palettes"
    reset("creative")
    cmd("gamemode spectator Bot1", f"tp Bot1 {X0 + 0.5} {Y + 30} {Z0 + 0.5}")
    setting("playeractivitydetector", "recent visit (s)", "5")
    only("playeractivitydetector")
    far = X0 + 20000 + (int(time.time()) % 1000) * 16
    cmd(f"tp @s {far} 200 {Z0}")
    wait(200)
    bx, by, bz = X0 + 3, Y + 1, Z0 + 3
    cmd(f"setblock {bx} {by} {bz} minecraft:diamond_block")
    wait(5)
    cmd(f"setblock {bx} {by} {bz} minecraft:air")
    wait(5)
    mark = log_mark()
    cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5}")
    wait(100)
    said = [l for l in sunset_said(mark) if l.startswith("Player activity?")]
    only()
    cmd("gamemode spectator Bot1", f"tp Bot1 {X0 + 0.5} {Y + 30} {Z0 + 60}")
    ok = any(l.startswith(f"Player activity? chunk {bx >> 4} {bz >> 4} ") for l in said)
    return ok, f"said: {said}"


def chat_since(mark):
    """Every chat line the game logged since `mark` (the server's feedback included)."""
    with open(os.path.join(session_dir(), "home", "logs", "latest.log"), errors="replace") as f:
        f.seek(mark)
        return [line.split("[CHAT] ", 1)[1].strip() for line in f if "[CHAT] " in line]


@test("Auto Breed")
def _():
    # Two adult cows and a calf near, wheat in the hotbar: the two adults are fed once each (two wheat used), the calf
    # not (Age Adult), and nobody again (Continuous breeding off).
    reset("survival")
    slot(0, "wheat", 10)
    for dx, extra in ((2, ""), (-2, ""), (0, ",Age:-24000")):
        cmd(f"summon minecraft:cow {X0 + dx + 0.5} {Y + 1} {Z0 + 2.5} {{NoAI:1b,PersistenceRequired:1b,Tags:[\"breed\"]{extra}}}")
    wait(10)
    only("autobreed")
    wait(40)
    first = ctl("inventory").get("counts", {}).get("minecraft:wheat", 0)
    wait(60)
    later = ctl("inventory").get("counts", {}).get("minecraft:wheat", 0)
    only()
    cmd("kill @e[tag=breed]")
    return first == 8 and later == 8, f"wheat 10 -> {first}, then {later}"


@test("Click TP")
def _():
    # A right click with an empty hand on the floor 6 blocks ahead: you stand there. One 6 blocks further (over Max
    # distance 8) is left alone.
    reset("creative")
    eye = Y + 1 + 1.62
    only("clicktp")

    def click_at(dz):
        pitch = math.degrees(math.atan2(eye - (Y + 1), dz))
        ctl("look", "0", f"{pitch:.3f}")
        wait(2)
        ctl("mouse", "right", "down")
        wait(2)
        ctl("mouse", "right", "up")
        wait(10)
        return pos()

    near = click_at(6.0)
    reset("creative")
    far = click_at(12.0)
    only()
    ok = abs(near[2] - (Z0 + 6.5)) < 0.6 and abs(near[1] - (Y + 1)) < 0.1 and abs(far[2] - (Z0 + 0.5)) < 0.6
    return ok, f"after a click 6 ahead: {near}; 12 ahead: {far}"


@test("Build Height")
def _():
    # Stone at the build limit (y 319), you on top of it, stone in hand: a click on its top places nothing without
    # Build Height (the server refuses a block above the limit) and the block under it with (the click is turned into
    # one on its underside).
    def placed(on):
        reset("creative")
        cmd(f"setblock {X0} 319 {Z0} minecraft:stone", f"setblock {X0} 318 {Z0} minecraft:air", f"tp @s {X0 + 0.5} 320 {Z0 + 0.5} 0 90")
        cmd("gamemode survival")
        slot(0, "cobblestone", 8)
        wait(20)
        only("buildheight") if on else only()
        ctl("look", "0", "90")
        ctl("mouse", "right", "down")
        wait(2)
        ctl("mouse", "right", "up")
        wait(15)
        under = count(X0, 318, Z0, X0, 318, Z0, "cobblestone")
        cmd(f"setblock {X0} 319 {Z0} minecraft:air", f"setblock {X0} 318 {Z0} minecraft:air")
        return under

    plain, built = placed(False), placed(True)
    only()
    reset("creative")
    return plain == 0 and built == 1, f"cobblestone under the top block: off {plain}, on {built}"


@test("Enderman Look")
def _():
    # An enderman 8 blocks ahead, looked in the eyes: with Look mode Away, the server is told you look at your feet.
    reset("survival")
    cmd("difficulty easy", f"summon minecraft:enderman {X0 + 0.5} {Y + 1} {Z0 + 8.5} {{NoAI:1b,PersistenceRequired:1b,Tags:[\"ender\"]}}")
    eye = Y + 1 + 1.62
    pitch = -math.degrees(math.atan2(Y + 1 + 2.55 - eye, 8.0))
    ctl("look", "0", f"{pitch:.3f}")
    wait(10)
    only("endermanlook")
    wait(10)
    mark = log_mark()
    cmd("data get entity @s Rotation[1]")
    wait(5)
    said = [l for l in chat_since(mark) if "entity data" in l]
    only()
    cmd("kill @e[tag=ender]", "difficulty peaceful")
    looked = said[0].rsplit(":", 1)[-1].strip().rstrip("f") if said else ""
    try:
        down = abs(float(looked) - 90.0) < 0.5
    except ValueError:
        down = False
    return down, f"server's pitch for you: {said}"


@test("Trident Boost")
def _():
    # Standing in water with a Riptide III trident, a throw's spin: how far you go in the 10 ticks after, with Boost 2
    # much further than without (the push is given more of it from the tick after the throw).
    def flown(on):
        reset("creative")
        cmd(f"fill {X0-2} {Y+1} {Z0-2} {X0+2} {Y+3} {Z0+2} minecraft:water")
        cmd("gamemode survival", "item replace entity @s hotbar.0 with minecraft:trident[enchantments={riptide:3}]")
        only("tridentboost") if on else only()
        ctl("look", "0", "-30")
        wait(10)
        ctl("mouse", "right", "down")
        wait(15)
        start = pos()
        ctl("mouse", "right", "up")
        steps, last = [], start
        for _ in range(10):
            wait(1)
            p = pos()
            steps.append(round(math.dist(p, last), 2))
            last = p
        cmd("gamemode creative", f"fill {X0-2} {Y+1} {Z0-2} {X0+2} {Y+3} {Z0+2} minecraft:air")
        wait(20)
        return math.dist(last, start), steps

    (plain, a), (boosted, b) = flown(False), flown(True)
    only()
    reset("creative")
    return boosted > plain * 1.4, f"flown in 10 ticks: {plain:.1f} off {a}, {boosted:.1f} on {b}"


@test("Auto Sign")
def _():
    # The first sign written by hand ("hello"), then a second put down: its editor closes by itself and the second
    # sign says the same once Delay has passed.
    reset("creative")
    slot(0, "oak_sign", 4)
    only("autosign")
    texts = []
    for yaw, dz in ((0, 2), (180, -2)):
        pitch = math.degrees(math.atan2(1.62, abs(dz)))
        ctl("look", str(yaw), f"{pitch:.3f}")
        wait(2)
        ctl("mouse", "right", "down")
        wait(2)
        ctl("mouse", "right", "up")
        wait(10)
        if yaw == 0:
            ctl("type", "hello")
            wait(2)
            ctl("key", "esc", "tap")
        wait(30)
        mark = log_mark()
        cmd(f"data get block {X0} {Y + 1} {Z0 + dz} front_text.messages[0]")
        wait(5)
        texts.append([l for l in chat_since(mark) if "block data" in l or "found" in l.lower()])
    screen = state().get("screen")
    only()
    cmd(f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:air", f"setblock {X0} {Y + 1} {Z0 - 2} minecraft:air")
    ok = all(t and "hello" in t[0] for t in texts) and screen == "playing"
    return ok, f"first and second sign: {texts}; screen after {screen}"


@test("Jesus")
def _():
    # Dropped onto a pool: with Jesus you stand on the water's top, without you go in.
    def height(on):
        reset("creative")
        cmd(f"fill {X0-3} {Y-2} {Z0+2} {X0+3} {Y} {Z0+8} minecraft:water", "gamemode survival")
        only("jesus") if on else only()
        cmd(f"tp @s {X0 + 0.5} {Y + 2} {Z0 + 5.5}")
        wait(40)
        y = pos()[1]
        cmd(f"fill {X0-3} {Y-2} {Z0+2} {X0+3} {Y} {Z0+8} minecraft:stone", f"fill {X0-3} {Y-2} {Z0+2} {X0+3} {Y-1} {Z0+8} minecraft:air")
        return y

    plain, on = height(False), height(True)
    only()
    reset("creative")
    return abs(on - (Y + 1)) < 0.1 and plain < Y + 0.8, f"height after the drop: off {plain:.2f}, on {on:.2f} (water's top {Y + 1})"


@test("Slippy")
def _():
    # A run forward, then the key let go: with Slippy (Friction 0.98, ice's) you slide on much further.
    def slid(on):
        reset("creative")
        cmd("gamemode survival")
        setting("slippy", "friction", "0.98")
        only("slippy") if on else only()
        walk(15)
        a = pos()
        wait(20)
        return math.dist(a, pos())

    plain, on = slid(False), slid(True)
    only()
    reset("creative")
    return on > plain * 3 and on > 0.4, f"slid after letting go: off {plain:.2f}, on {on:.2f} blocks"


@test("Elytra Fly")
def _():
    # An elytra on, a jump into a glide high up: with Elytra Fly and no key held you hang there (a slow sink), with
    # W held you go Horizontal speed (1 block a tick); without it the glide drops.
    def glide(on, key=None):
        reset("creative")
        cmd("gamemode survival", "item replace entity @s armor.chest with minecraft:elytra", f"tp @s {X0 + 0.5} {Y + 40} {Z0 + 0.5} 0 0")
        only("elytrafly") if on else only()
        wait(5)
        held_keys(("space",), 3)
        wait(5)
        a = pos()
        if key:
            held_keys((key,), 20)
        else:
            wait(20)
        b = pos()
        cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5}", "clear @s")
        wait(10)
        return a[1] - b[1], math.dist((a[0], a[2]), (b[0], b[2]))

    (drop_off, _), (drop_on, _), (_, ahead) = glide(False), glide(True), glide(True, "w")
    only()
    reset("creative")
    return drop_on < 1.0 and drop_off > 3.0 and ahead > 12.0, (
        f"drop in 20 ticks: off {drop_off:.2f}, on {drop_on:.2f}; with W held {ahead:.1f} blocks")


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
    """Accent (orange or gold) pixels where the HUD writes its name, top left: whether the HUD is on."""
    from PIL import Image
    path = f"{SHOTS}/sunset-watermark.png"
    ctl("screenshot", path)
    time.sleep(1)
    im = Image.open(path).convert("RGB")
    return sum(1 for r, g, b in im.crop((0, 0, im.width // 5, im.height // 14)).getdata() if r > 200 and 60 < g < 210 and b < 90) > 20


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
    panel = panel_width()
    ww = min(max(panel * 1.6, 150), w - 8)
    x = (w - ww) / 2
    bind_row = (x + ww / 2, 40 + 16 + 14 + 14 + 7)
    setting("clickgui", "settings", "Window")
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


@test("Chest Swap While Moving")
def _():
    cmd("item replace entity @s armor.chest with minecraft:iron_chestplate", "item replace entity @s inventory.5 with minecraft:elytra")
    wait(5)
    ctl("key", "w", "down")
    wait(3)
    only("chestswap")
    wait(5)
    held = check("execute if items entity @s armor.chest minecraft:iron_chestplate")
    ctl("key", "w", "up")
    wait(8)
    swapped = check("execute if items entity @s armor.chest minecraft:elytra")
    off = turned_itself_off("chestswap", "Chest Swap")
    only()
    return held and swapped and off, f"chestplate while W held: {held}; elytra after release: {swapped}; off after: {off}"


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


@test("Chest Stealer")
def _():
    def chest():
        cmd(f"setblock {X0+1} {Y+1} {Z0} minecraft:chest[facing=west]",
            *(f"item replace block {X0+1} {Y+1} {Z0} container.{i} with minecraft:stone" for i in range(27)))

    chest()
    wait(6)

    def taken(per_tick):
        """Ticks from the first item taken to the last, and how many were taken."""
        setting("cheststealer", "items per tick", per_tick)
        only("cheststealer")
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


@test("Chest Stealer (carts from shulkers, close when done)")
def _():
    # Take Carts from shulkers: of a shulker box's TNT minecarts and stone only the carts come out, and with Close
    # when done the box shuts after; a chest beside it is left alone in that mode.
    box = (X0 + 1, Y + 1, Z0)
    cmd(f"setblock {box[0]} {box[1]} {box[2]} minecraft:shulker_box",
        *(f"item replace block {box[0]} {box[1]} {box[2]} container.{i} with minecraft:{'tnt_minecart' if i < 3 else 'stone'}"
          for i in range(6)))
    wait(6)
    setting("cheststealer", "take", "Carts from shulkers")
    setting("cheststealer", "close when done", "true")
    only("cheststealer")
    ctl("interact", str(box[0]), str(box[1]), str(box[2]))
    wait(30)
    counts = ctl("inventory").get("counts", {})
    shut = state().get("screen") in (None, "", "playing", "Playing")
    only()
    cmd("clear @s", f"setblock {box[0]} {box[1]} {box[2]} minecraft:air", f"setblock {X0-1} {Y+1} {Z0} minecraft:chest",
        f"item replace block {X0-1} {Y+1} {Z0} container.0 with minecraft:tnt_minecart")
    wait(6)
    only("cheststealer")
    ctl("interact", str(X0 - 1), str(Y + 1), str(Z0))
    wait(20)
    chest_carts = ctl("inventory").get("counts", {}).get("minecraft:tnt_minecart", 0)
    ctl("close")
    only()
    cmd("clear @s", f"setblock {X0-1} {Y+1} {Z0} minecraft:air")
    say(".reset cheststealer")
    carts, stone = counts.get("minecraft:tnt_minecart", 0), counts.get("minecraft:stone", 0)
    ok = carts == 3 and stone == 0 and shut and chest_carts == 0
    return ok, f"from the shulker box: {carts} carts, {stone} stone, closed after: {shut}; from a chest: {chest_carts} carts"


@test("Spam (Once)")
def _():
    # Once with a /command as its message: the command runs one time as Spam turns on, and Spam goes off.
    setting("spam", "messages", "/say sunset-once")
    setting("spam", "delay (ticks)", "5")
    setting("spam", "once", "true")
    mark = log_mark()
    say(".t spam")
    wait(40)
    said = [line for line in chat_since(mark) if "sunset-once" in line and "Messages" not in line]
    say(".reset spam")
    return len(said) == 1, f"lines saying it in 2 s: {said}"


def _():
    # Preset PvP: of two swords, two pickaxes and a helmet like the one worn only the best of each kind stays; 3
    # stacks of 64 stone keep 128 (Blocks); 3 stacks of 64 arrows keep 64; dirt (Junk) goes; a torch and bread stay.
    # Then Blocks set to 64: one more stone stack goes.
    def clean():
        ctl("open-inventory")
        wait(60)
        ctl("close")
        return ctl("inventory").get("counts", {})

    reset()
    setting("inventorymanager", "preset", "Custom")
    only("inventorymanager")
    cmd("clear @s", "item replace entity @s armor.head with minecraft:iron_helmet")
    wait(5)
    setting("inventorymanager", "preset", "PvP")
    wait(5)
    give = {0: "diamond_sword", 1: "iron_sword", 2: "stone_pickaxe", 3: "netherite_pickaxe", 4: "iron_helmet", 5: "stone 64",
            6: "stone 64", 7: "stone 64", 8: "arrow 64", 9: "arrow 64", 10: "arrow 64", 11: "dirt 32", 12: "torch 16", 13: "bread 8"}
    cmd(*[f"item replace entity @s inventory.{i} with minecraft:{v}" for i, v in give.items()])
    got = clean()
    setting("inventorymanager", "blocks", "64")
    fewer = clean().get("minecraft:stone")
    only()
    setting("inventorymanager", "preset", "Custom")
    cmd("clear @s", "kill @e[type=item]")
    want = {"minecraft:diamond_sword": 1, "minecraft:netherite_pickaxe": 1, "minecraft:stone": 128, "minecraft:arrow": 64,
            "minecraft:torch": 16, "minecraft:bread": 8}
    wrong = {k: got.get(k) for k in set(want) | set(got) if k != "minecraft:iron_helmet" and got.get(k) != want.get(k)}
    ok = not wrong and got.get("minecraft:iron_helmet", 0) <= 1 and fewer == 64
    return ok, f"PvP kept {got}; not as wanted: {wrong}; stone with Blocks 64: {fewer}"


@test("Inventory Manager")
def _():
    cmd("item replace entity @s inventory.3 with minecraft:rotten_flesh 12", "item replace entity @s inventory.4 with minecraft:bread 4")
    only("inventorymanager")
    ctl("open-inventory")
    wait(30)
    ctl("close")
    counts = ctl("inventory").get("counts", {})
    gone, kept = "minecraft:rotten_flesh" not in counts, counts.get("minecraft:bread") == 4
    setting("inventorymanager", "items", "bread")
    ctl("open-inventory")
    wait(30)
    ctl("close")
    bread = "minecraft:bread" not in ctl("inventory").get("counts", {})
    # List mode Blacklist: everything but the listed items goes (the clean() after the test empties the rest).
    cmd("clear @s", "item replace entity @s inventory.3 with minecraft:rotten_flesh 12", "item replace entity @s inventory.4 with minecraft:bread 4")
    setting("inventorymanager", "list mode", "Blacklist")
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
    setting("antiafk", "mode", "Meteor")
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
    setting("antiafk", "mode", "Meteor")
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
    # The inventory's buttons (three down its right edge by the main inventory: Sort, Fill Stacks, Restock). Sort with
    # Ctrl puts five stacks by name, with Shift by count (the dirt merged first); Fill Stacks tops up a dirt stack from
    # the hotbar's; Restock moves the stone the hotbar already has into it and leaves the apples.
    def press(op, key=None, setup=()):
        cmd("clear @s", *setup)
        ctl("open-inventory")
        wait(4)
        ctl("screenshot", f"{SHOTS}/client_sort{op}.png")
        time.sleep(2)
        w, h = gui_size()
        # A server can have the recipe book open, which moves the inventory 77 to the right: its dark list is then
        # where the inventory's left side would be.
        from PIL import Image
        img = Image.open(f"{SHOTS}/client_sort{op}.png").convert("L")
        k = img.width / w
        shift = 77 if img.getpixel((int(((w - 176) / 2 + 10) * k), int(((h - 166) / 2 + 60) * k))) < 90 else 0
        if key:
            ctl("key", key, "down")
        ctl("click-at", str((w - 176) / 2 + shift + 176.5), str((h - 166) / 2 + 75.5 + 10 * op))
        if key:
            ctl("key", key, "up")
        wait(30)
        inv = ctl("inventory")
        ctl("close")
        return ([(s["item"].split(":")[1], s["count"]) for s in inv["main"] if s.get("item")],
                [(s["item"].split(":")[1], s["count"]) for s in inv["hotbar"] if s and s.get("item")])

    five = ["item replace entity @s inventory.0 with minecraft:dirt 3", "item replace entity @s inventory.1 with minecraft:diamond 1",
            "item replace entity @s inventory.2 with minecraft:dirt 60", "item replace entity @s inventory.3 with minecraft:apple 5",
            "item replace entity @s inventory.4 with minecraft:stone 10"]
    reset()
    only("clientsort")
    alphabet, _ = press(0, "ctrl", five)
    quantity, _ = press(0, "shift", five)
    filled = press(1, None, ["item replace entity @s inventory.0 with minecraft:dirt 10", "item replace entity @s hotbar.4 with minecraft:dirt 30"])
    restocked = press(2, None, ["item replace entity @s inventory.0 with minecraft:stone 10", "item replace entity @s inventory.1 with minecraft:apple 5",
                                "item replace entity @s hotbar.4 with minecraft:stone 1"])
    only()
    cmd("clear @s")
    ok = (alphabet == [("apple", 5), ("diamond", 1), ("dirt", 63), ("stone", 10)]
          and quantity == [("dirt", 63), ("stone", 10), ("apple", 5), ("diamond", 1)]
          and filled == ([("dirt", 40)], []) and restocked == ([("apple", 5)], [("stone", 11)]))
    return ok, f"Ctrl: {alphabet}; Shift: {quantity}; Fill Stacks (main, hotbar): {filled}; Restock: {restocked}"


@test("Client Sort (settings from before 0.9)")
def _():
    # A profile with the orders and lists Client Sort had before: it loads, and Client Sort is on.
    shown = old_profile("verifysort", ["clientsort.on=true", "clientsort.key=none", "clientsort.order=Alphabet",
                                       "clientsort.startitems=diamond", "clientsort.clickspertick=8"], "clientsort")
    if shown is None:
        return False, "no profile file written"
    loaded = any(line.startswith("Loaded profile verifysort") for line in shown)
    on = any("Client Sort*" in line for line in shown)
    return loaded and on, f"loaded: {loaded}, Client Sort on: {on}; said {shown[:2]}"


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
        ctl("screenshot", f"{SHOTS}/{name}.png")
        time.sleep(2)
        img = Image.open(f"{SHOTS}/{name}.png").convert("RGB")
        w, h = gui_size()
        k = img.width / w
        cx, cy = (w - 176) / 2 + 16, (h - 166) / 2 + 150
        part = list(img.crop((int((cx + 8) * k), int((cy - 80) * k), int((cx + 200) * k), int((cy - 8) * k))).getdata())
        return [sum(1 for px in part if f(*px)) for f in (red, diamond, gold)]

    def lit(name):
        # How bright the world is round the inventory (its left side, below the HUD's text): the pinned view dims it.
        from PIL import Image
        ctl("screenshot", f"{SHOTS}/{name}.png")
        time.sleep(2)
        img = Image.open(f"{SHOTS}/{name}.png").convert("L")
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


@test("Auto Tool (Switch delay)")
def _():
    # Mining obsidian with a sword in hand and a pickaxe in slot 2: with Switch delay 6 the sword stays the first ticks
    # and the pickaxe comes in about 6 ticks after the tool is wanted (with a delay it never came before); the switch
    # goes at the start of a tick, before its dig packets, as a hotbar key does.
    slot(0, "diamond_sword")
    slot(1, "diamond_pickaxe")
    ctl("key", "1", "tap")
    wait(2)
    only("autotool")
    setting("autotool", "switch delay", "6")
    cmd(f"setblock {X0} {Y+1} {Z0+2} minecraft:obsidian")
    ctl("look-at-block", str(X0), str(Y + 1), str(Z0 + 2))
    wait(2)
    ctl("mouse", "left", "down")
    items = poll(16, 1, lambda: held().get("item", "").replace("minecraft:", ""))
    ctl("mouse", "left", "up")
    wait(5)
    setting("autotool", "switch delay", "0")
    only()
    first = next((i + 1 for i, item in enumerate(items) if "pickaxe" in item), None)
    ok = first is not None and 4 <= first <= 10 and "sword" in items[0]
    return ok, f"ticks until the pickaxe was held: {first} (held each tick: {items})"


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
    # Skip (the default) and Scale at 100% walk as fast; Scale at Ground 50% half as fast.
    skip = eating_rate()
    setting("noslow", "mode", "Scale")
    full = eating_rate()
    setting("noslow", "ground %", "50")
    half = eating_rate()
    only()
    return slow < 2.0 and skip > 3.8 and full > 3.8 and full * 0.35 < half < full * 0.75, (
        f"walking while eating: {slow:.2f} blocks/s off, {skip:.2f} with No Slow (Skip), {full:.2f} Scale 100%, "
        f"{half:.2f} at Ground 50%")


@test("No Slow (GrimAC cycle)")
def _():
    def eating_rate():
        reset(z=-14.5)
        slot(0, "golden_apple", 64)
        ctl("key", "1", "tap")
        ctl("mouse", "right", "down")
        speed = rate(ticks=18, warm=8)
        ctl("mouse", "right", "up")
        return speed

    slow = eating_rate()
    only("noslow")
    setting("noslow", "mode", "GrimAC")
    grim = eating_rate()
    setting("noslow", "mode", "Skip")
    skip = eating_rate()
    only()
    return slow + 0.2 < grim < skip - 0.2, (
        f"walking while eating: {slow:.2f} blocks/s vanilla, {grim:.2f} GrimAC, {skip:.2f} Skip")


@test("Fast Place (speeds)")
def _():
    # A stone at eye height 5 blocks ahead: holding use builds a row of 4 back towards you. Ticks until all 4 are in.
    def row(speed, clicker=False):
        reset()
        cmd(f"setblock {X0} {Y+2} {Z0+5} minecraft:stone")
        slot(0, "cobblestone", 64)
        ctl("key", "1", "tap")
        if speed:
            setting("fastplace", "speed", str(speed))
            if clicker:
                for key, value in (("left", "false"), ("right", "true"), ("right own cps", "true"),
                                   ("right min cps", "1"), ("right max cps", "1")):
                    setting("autoclicker", key, value)
            only("fastplace", *(["autoclicker"] if clicker else []))
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

    plain, one, five, together = row(None), row(1), row(5), row(5, True)
    ok = (0 < five < one < plain or (0 < five < plain and one <= plain)) and 0 < together <= five + 3
    return ok, f"4 blocks placed in {plain} ticks without, {one} at Speed 1, {five} at Speed 5, {together} with Auto Clicker right at 1 CPS"


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


@test("Offhand (Auto Totem)")
def _():
    # Offhand's Item Totem: no totem with Item None; with Item Totem one always, by a swap click in
    # the inventory it opens and closes again (the screen after: playing); the first totem, not a random one, with
    # Random slot off (slot 9 taken, 29 left); with Inventory only none until you open the inventory; with Silent open
    # one with no screen ever shown.
    def totem():
        return check("execute if items entity @s weapon.offhand minecraft:totem_of_undying")

    def fresh(*slots):
        cmd("item replace entity @s weapon.offhand with minecraft:air")
        for i in slots:
            cmd(f"item replace entity @s inventory.{i} with minecraft:totem_of_undying 1")

    fresh(4)
    setting("offhand", "item", "None")
    only("offhand")
    wait(30)
    none = totem()
    cmd("item replace entity @s inventory.4 with minecraft:air")
    setting("offhand", "random slot", "false")
    setting("offhand", "item", "Totem")
    fresh(0, 20)
    wait(40)
    first = totem() and not check("execute if items entity @s inventory.0 minecraft:totem_of_undying") \
        and check("execute if items entity @s inventory.20 minecraft:totem_of_undying")
    closed = state().get("screen") == "playing"
    setting("offhand", "inventory only", "true")
    fresh(4)
    wait(30)
    waited = not totem()
    ctl("open-inventory")
    wait(20)
    opened = totem()
    ctl("close")
    setting("offhand", "inventory only", "false")
    setting("offhand", "silent open", "true")
    fresh(4)
    screens = set()
    for _ in range(20):
        wait(2)
        screens.add(state().get("screen"))
    silent = totem() and screens <= {"playing"}
    setting("offhand", "silent open", "false")
    setting("offhand", "random slot", "true")
    only()
    ok = not none and first and closed and waited and opened and silent
    return ok, (f"Item None: offhand {'filled' if none else 'empty'}; Item Totem: first slot's totem {first}, inventory closed after "
                f"{closed}; Inventory only: waited {waited}, then filled in the open inventory {opened}; Silent open: filled with "
                f"no screen {silent} (screens {sorted(str(x) for x in screens)})")


def old_profile(name, lines, mod):
    """Loads a profile with these lines (as an older Sunset saved them): what .modules, .binds and .settings of `mod`
    say after; the settings before come back after."""
    def said(line):
        mark = log_mark()
        say(line)
        wait(4)
        return sunset_said(mark)

    say(f".profile save {name}")
    found = [os.path.join(r, f) for r, _, fs in os.walk(os.path.join(session_dir(), "home")) for f in fs if f == f"{name}.profile"]
    if not found:
        return None
    with open(found[0], "w") as f:
        f.write("sunset-profile=1\n" + "\n".join(lines) + "\n")
    loaded = said(f".profile load {name}")
    shown = said(".modules") + said(".binds") + said(f".settings {mod}")
    say(".profile load before-load")
    say(f".profile delete {name}")
    return loaded + shown


@test("Offhand (settings from before 0.9)")
def _():
    # A profile from before Offhand took in Auto Totem, Auto Inventory Totem and Hover Totem: Auto Totem on (Smart,
    # Health 14, bound to G), Auto Inventory Totem on (Hotbar Swap), the old Offhand off.
    shown = old_profile("verifyold", ["autototem.on=true", "autototem.key=key.keyboard.g", "autototem.mode=Smart", "autototem.health=14",
                                      "offhand.on=false", "offhand.key=none", "offhand.item=Crystal", "offhand.minhealth=10",
                                      "autoinventorytotem.on=true", "autoinventorytotem.key=none", "autoinventorytotem.mode=Hotbar Swap",
                                      "hovertotem.on=false", "hovertotem.key=none"], "offhand")
    if shown is None:
        return False, "no profile file written"
    # Item None was a totem at Health or below, now Item Totem; Hotbar Swap moved it with no screen, now Silent open.
    want = ["Item: Totem", "Silent open: ON", "Hover: OFF"]
    on = any("Offhand*" in line for line in shown)
    bound = any(line.startswith("Offhand: G") for line in shown)
    got = [w for w in want if any(line.startswith(w) for line in shown)]
    return on and bound and got == want, f"Offhand on: {on}, bound to G: {bound}, settings {got} of {want}; said {shown[:2]}"


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

    # Normal: its silent look turns to the side of the last block and the game's own click places there.
    on_, placed = walk_off()
    listed, kept = walk_off(blocks_filter="Whitelist", blocks="dirt")
    setting("scaffold", "blocks filter", "Blacklist")
    setting("scaffold", "blocks", "")
    # Same Y's Auto jump with Backwards rotations: jumping all the way, every block still goes in the row walked off.
    hopped, hopped_placed = walk_off(same_y="Auto jump", rotations="Backwards")
    above = count(X0 - 2, Y + 1, Z0 + 17, X0 + 2, Y + 3, Z0 + 26, "stone")
    setting("scaffold", "same y", "Off")
    setting("scaffold", "rotations", "Normal")
    # Fast tower: jump held in place, a stack of blocks under you.
    reset()
    slot(0, "stone", 32)
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
    ok = (on_[2] - Z0 > 19 and on_[1] > Y + 0.99 and placed >= 3 and listed[1] < Y and kept == 0 and hopped[1] > Y + 0.99
          and hopped_placed >= 3 and above == 0 and tower > climb * 1.5 and climb > 1)
    return ok, (f"walked out to z {on_[2]-Z0:.1f} past the edge (17) on {placed} blocks; with only dirt allowed: fell, {kept} placed; "
                f"Same Y Auto jump (Backwards): {hopped_placed} in the row, {above} above it; "
                f"jumping 2 s: {climb:.1f} blocks up, {tower:.1f} with Fast tower")


def bridge_backwards(mode, keys, look, jump=None, yaw="180"):
    """Bridging backwards off the platform's edge for 6 s: facing the platform, stone in hand, `keys` held, Scaffold in
    `mode`. The blocks past the edge as the server has them (and whether they run on without a gap), how far you got,
    the lowest your feet went (a block the server takes back drops you a level), whether you sneaked, the most move
    packets in a tick (probe, on a server) and kicks. `jump`: the 0.15 s step at which jump is tapped once; then the
    highest the feet stood after it comes last. `yaw`: the camera's (180 faces the platform, 0 faces the drop)."""
    reset(z=15.5)
    cmd(f"fill {X0-2} {Y-1} {Z0+17} {X0+2} {Y} {Z0+40} minecraft:air")
    slot(0, "stone", 64)
    ctl("key", "1", "tap")
    ctl("look", yaw, str(look))
    setting("scaffold", "mode", mode)
    setting("scaffold", "activation blocks", "0")
    only("scaffold")
    kicked = len(kicks)
    mark = probe_mark() if probe_path() and SERVER else None
    low, sneaked = pos()[1], False
    for k in keys:
        ctl(*k, "down")
    top = low
    for step in range(40):
        if step == jump:
            held_keys(("space",), 2)
        wait(3)
        st = state().get("player") or {}
        at = (st.get("position") or [0, low, 0])[1]
        low = min(low, at)
        if jump is not None and step > jump and st.get("on_ground", True):
            top = max(top, at)
        sneaked = sneaked or bool(st.get("sneaking"))
    for k in keys:
        ctl(*k, "up")
    far = pos()[2] - Z0
    only()
    setting("scaffold", "mode", "Normal")
    wait(20)
    stone = [check(f"execute if block {X0} {Y} {Z0 + z} minecraft:stone") for z in range(17, 41)]
    kept = sum(stone)
    gapless = all(stone[:kept])
    most = wire_stats(mark)[1] if mark is not None else None
    return kept, gapless, far, low, sneaked, most, len(kicks) - kicked, top


def bridged(kept, gapless, far, low, sneaked, most, kicked, top, sneak):
    """`sneak`: whether the mode must sneak (None: either, as Normal's Ledge sneak does when its look is late)."""
    ok = (kept >= 4 and gapless and far > 16 + kept and low > Y + 0.99 and (sneak is None or sneaked == sneak)
          and kicked == 0 and (most or 0) <= 1)
    return ok, (f"{kept} blocks past the edge kept by the server, without a gap: {gapless}, walked out to z {far:.1f} "
                f"(edge 17), lowest feet {low - Y - 1:+.2f}, sneaked: {sneaked}, most moves a tick: {most}, kicks: {kicked}")


@test("Scaffold (Telly sprint jumps and Keep Y)")
def _():
    if not probe_path():
        return None, "needs tools/probe"
    reset(z=12.5)
    cmd(f"fill {X0-8} {Y-1} {Z0+17} {X0+8} {Y+3} {Z0+42} minecraft:air")
    slot(0, "stone", 64)
    ctl("look", "0", "15")
    setting("scaffold", "mode", "Telly")
    only("scaffold")
    mark = probe_mark()
    low, high = pos()[1], pos()[1]
    ctl("key", "w", "down")
    for _ in range(40):
        wait(2)
        y = pos()[1]
        low, high = min(low, y), max(high, y)
    ctl("key", "w", "up")
    end = pos()
    only()
    wait(12)
    lines = inventory_wire(mark)
    cameras = [(float(p[2]), float(p[3])) for line in lines if (p := line.split()) and p[0][0].isdigit() and len(p) > 4]
    camera_still = bool(cameras) and all(abs(yaw) < 0.1 and abs(pitch-15) < 0.1 for yaw, pitch in cameras)
    last_pitch, moved, late, clicks, most, in_tick = None, False, 0, 0, 0, 0
    for line in lines:
        if line.startswith("look "):
            last_pitch = float(line.split()[2])
        elif line == "order move":
            moved = True
            in_tick += 1
        elif line == "order place":
            clicks += 1
            late += int(moved or last_pitch is None or last_pitch < 20)
        elif line == "order tickend":
            most = max(most, in_tick)
            moved, in_tick = False, 0
    blocks = count(X0-8, Y, Z0+17, X0+8, Y, Z0+42, "stone")
    raised = count(X0-8, Y+1, Z0+17, X0+8, Y+3, Z0+42, "stone")
    ok = end[2] > Z0+21 and low >= Y+0.99 and high > Y+2 and blocks >= 4 and raised == 0
    ok = ok and camera_still and clicks >= 4 and late == 0 and most <= 1
    return ok, f"end {end}, low/high {low:.3f}/{high:.3f}, blocks {blocks}, raised {raised}, silent {camera_still}, clicks {clicks}, invalid order {late}, moves/tick {most}"


@test("Scaffold (Telly diagonal tower and reset)")
def _():
    reset()
    slot(0, "stone", 64)
    setting("scaffold", "mode", "Telly")
    only("scaffold")
    ctl("look", "37", "15")
    start = pos()
    held_keys(("w", "d"), 30)
    end = pos()
    # A vanilla jump tower can rise only after clearing its own block's top; no vertical velocity override.
    reset()
    slot(0, "stone", 64)
    ctl("look", "0", "15")
    held_keys(("w", "space"), 30)
    tower = pos()[1]
    only()
    wait(15)
    ctl("key", "w", "up")
    ctl("key", "space", "up")
    settled = pos()
    wait(12)
    drift = flat(pos(), settled)
    return end[0] < start[0] and end[2] > start[2] and tower > Y+2 and drift < 0.05, f"diagonal {start}->{end}, tower feet {tower:.3f}, disabled drift {drift:.4f}"


@test("Scaffold (Legit, backwards)")
def _():
    # Legit only sneaks: the test holds use with the crosshair low enough to meet the edge block's side.
    return bridged(*bridge_backwards("Legit", [("key", "s"), ("mouse", "right")], 80), sneak=True)


@test("Scaffold (Normal, backwards)")
def _():
    # Normal places by itself from its own silent look: the test only holds S, the camera stays on the platform.
    return bridged(*bridge_backwards("Normal", [("key", "s")], 30), sneak=None)


@test("Scaffold (Normal, ray cast)")
def _():
    # Each Ray cast mode bridges backwards: Strict clicks with the look the server already has, Normal with the look the
    # tick's move packet carries, Off with the planned side. All keep one move packet a tick and stay up.
    runs = []
    for ray in ("Strict", "Normal", "Off"):
        setting("scaffold", "ray cast", ray)
        kept, gapless, far, low, sneaked, most, kicked, _top = bridge_backwards("Normal", [("key", "s")], 30)
        runs.append((ray, kept, gapless, low, most, kicked))
    setting("scaffold", "ray cast", "Strict")
    ok = all(k >= 4 and g and lo > Y + 0.99 and (mo or 0) <= 1 and ki == 0 for _r, k, g, lo, mo, ki in runs)
    return ok, "; ".join(f"{r}: {k} kept, gapless {g}, lowest feet {lo - Y - 1:+.2f}, moves a tick {mo}, kicks {ki}"
                         for r, k, g, lo, mo, ki in runs)


@test("Scaffold (Normal, no ledge sneak)")
def _():
    # Ledge sneak off: each click goes in the first ticks past an edge, with the look aimed a move ahead and turned
    # before the edge. Never sneaks, never drops: backwards (S, Backwards and Normal rotations) and forwards (W, the look
    # turning round on the way to the edge).
    setting("scaffold", "ledge sneak", "false")
    runs = []
    for name, rotations, keys, yaw in (("S, Backwards", "Backwards", [("key", "s")], "180"),
                                       ("S, Normal", "Normal", [("key", "s")], "180"),
                                       ("W, Normal", "Normal", [("key", "w")], "0")):
        setting("scaffold", "rotations", rotations)
        runs.append((name, bridged(*bridge_backwards("Normal", keys, 30, yaw=yaw), sneak=False)))
    setting("scaffold", "rotations", "Normal")
    setting("scaffold", "ledge sneak", "true")
    return all(ok for _n, (ok, _d) in runs), "; ".join(f"{n}: {d}" for n, (_ok, d) in runs)


@test("Scaffold (Normal, diagonal)")
def _():
    # Off the platform's side at 45 degrees with W held: the cells go in as a staircase, each beside the one before, so
    # every block had a side to go against, and you stay up.
    reset(z=15.0)
    cmd(f"fill {X0-2} {Y-1} {Z0+17} {X0+14} {Y} {Z0+32} minecraft:air")
    slot(0, "stone", 64)
    ctl("key", "1", "tap")
    ctl("look", "-45", "0")
    only("scaffold")
    walk(50)
    p = pos()
    placed = count(X0 - 2, Y, Z0 + 17, X0 + 14, Y, Z0 + 32, "stone")
    lower = count(X0 - 2, Y - 1, Z0 + 17, X0 + 14, Y - 1, Z0 + 32, "stone")
    only()
    ok = p[1] > Y + 0.99 and p[2] - Z0 > 19 and p[0] - X0 > 3 and placed >= 4 and lower == 0
    return ok, (f"walked to x {p[0]-X0:.1f}, z {p[2]-Z0:.1f} (edge at z 17), feet {p[1] - Y - 1:+.2f}; {placed} blocks in the row, "
                f"{lower} below it")


@test("Scaffold (Godbridge, backwards)")
def _():
    # Godbridge turns the look and clicks by itself, without sneaking: the test only holds S.
    return bridged(*bridge_backwards("Godbridge", [("key", "s")], 60), sneak=False)


@test("Scaffold (Godbridge, jump)")
def _():
    # A jump 3 s into a Godbridge: the bridge's look clicks the top of the block under you as you clear it, so you
    # land a block up and bridge on from there, never a level down.
    kept, gapless, far, low, sneaked, most, kicked, top = bridge_backwards("Godbridge", [("key", "s")], 60, jump=20)
    ok = top > Y + 1.99 and low > Y + 0.99 and kept >= 4 and kicked == 0 and (most or 0) <= 1
    return ok, (f"highest feet stood after the jump {top - Y - 1:+.2f}, lowest {low - Y - 1:+.2f}, {kept} blocks past the edge on the "
                f"first level, most moves a tick: {most}, kicks: {kicked}")


@test("Offhand (item)")
def _():
    slot(3, "end_crystal", 2)
    cmd("item replace entity @s inventory.5 with minecraft:totem_of_undying 1")
    setting("offhand", "hotbar", "true")
    setting("offhand", "item", "Crystal")
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
    only()
    setting("offhand", "always gap on sword", "false")
    return crystal and totem and gap, (f"offhand holds the crystal with Item Crystal: {crystal}, the totem with Item Totem: {totem}, "
                                       f"the gapple with a sword and Always gap on sword: {gap}")


@test("Offhand (totem on low health)")
def _():
    # Item Shield with Totem on low health: the shield in the offhand, a totem once health falls to Totem health (5),
    # and the shield again once it is back up.
    reset("survival")
    cmd("item replace entity @s inventory.5 with minecraft:totem_of_undying 1", "item replace entity @s inventory.6 with minecraft:shield 1")
    setting("offhand", "item", "Shield")
    setting("offhand", "totem on low health", "true")
    only("offhand")
    wait(40)
    first = offhand()
    # Health comes back quickly (full food): what the offhand held at any time in the 2 s after the hit.
    cmd("damage @s 17")
    seen = set()
    for _ in range(20):
        wait(2)
        seen.add(offhand())
    low = "minecraft:totem_of_undying" if "minecraft:totem_of_undying" in seen else ",".join(sorted(seen))
    cmd("effect give @s minecraft:instant_health 1 3 true")
    wait(40)
    healed = offhand()
    only()
    say(".reset offhand")
    ok = first.endswith("shield") and low.endswith("totem_of_undying") and healed.endswith("shield")
    return ok, f"offhand: {first} at full health, {low} at 4, {healed} healed"


@test("Auto Armor")
def _():
    cmd("give @s minecraft:iron_chestplate", "give @s minecraft:diamond_chestplate", "give @s minecraft:leather_boots")
    only()
    setting("inventorymanager", "auto armor", "true")
    wait(60)
    chest = check("execute if items entity @s armor.chest minecraft:diamond_chestplate")
    boots = check("execute if items entity @s armor.feet minecraft:leather_boots")
    return chest and boots, f"wears the diamond chestplate over the iron one: {chest}, and the boots: {boots}"


@test("Auto Armor (modes, avoid cursed)")
def _():
    # Mode Key: nothing is worn until Armor key is held. Avoid cursed: an iron helmet goes on over a diamond one with
    # Curse of Binding.
    cmd("give @s minecraft:diamond_chestplate")
    setting("inventorymanager", "armor mode", "Key")
    setting("inventorymanager", "armor key", "G")
    only()
    setting("inventorymanager", "auto armor", "true")
    wait(40)
    early = check("execute if items entity @s armor.chest minecraft:diamond_chestplate")
    ctl("key", "g", "down")
    wait(40)
    ctl("key", "g", "up")
    keyed = check("execute if items entity @s armor.chest minecraft:diamond_chestplate")
    only()
    setting("inventorymanager", "armor mode", "Always")
    setting("inventorymanager", "avoid cursed", "true")
    cmd('give @s minecraft:diamond_helmet[enchantments={"minecraft:binding_curse":1}]', "give @s minecraft:iron_helmet")
    only()
    setting("inventorymanager", "auto armor", "true")
    wait(60)
    iron = check("execute if items entity @s armor.head minecraft:iron_helmet")
    only()
    say(".reset inventorymanager")
    return not early and keyed and iron, f"chestplate worn before the key: {early}, with it held: {keyed}; iron helmet over the cursed diamond: {iron}"


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


@test("Trigger Bot (crits only, sticky target)")
def _():
    # Crits only: a pig on the crosshair is never hit while you stand, and is once you jump (as you fall). Sticky
    # target: after hitting one pig, a second under the crosshair is left alone while the first lives, and hit once
    # it is gone.
    def ready():
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        setting("triggerbot", "passive", "true")

    ready()
    pid = pig(1.5)
    ctl("look-at-entity", str(pid))
    setting("triggerbot", "crits only", "true")
    only("triggerbot")
    wait(40)
    standing = mobs("pig")[0]["health"] if mobs("pig") else 0
    for _ in range(4):
        ctl("jump")
        wait(15)
    jumped = mobs("pig")[0]["health"] if mobs("pig") else 0
    only()
    setting("triggerbot", "crits only", "false")

    ready()
    tagged = "{NoAI:1b,PersistenceRequired:1b,Tags:[\"sticky_a\"]}"
    cmd(f"summon pig {X0 - 0.5} {Y + 1} {Z0 + 2.5} {tagged}", f"summon pig {X0 + 1.5} {Y + 1} {Z0 + 2.5} {NOAI}")
    wait(10)
    pigs = sorted(mobs("pig"), key=lambda e: e["pos"][0])
    first, second = pigs[0]["id"], pigs[1]["id"]
    health = lambda eid: next((e["health"] for e in mobs("pig") if e["id"] == eid), 0)
    setting("triggerbot", "sticky target", "true")
    only("triggerbot")
    ctl("look-at-entity", str(first))
    wait(8)
    first_hit = health(first)
    ctl("look-at-entity", str(second))
    wait(30)
    held_off = health(second)
    cmd("kill @e[tag=sticky_a]")
    wait(30)
    after = health(second)
    only()
    setting("triggerbot", "sticky target", "false")
    ok = standing == 10 and jumped < 10 and first_hit < 10 and held_off == 10 and after < 10
    return ok, (f"Crits only: pig at {standing} standing, {jumped} after jumps; Sticky target: first pig {first_hit}, "
                f"second {held_off} while the first lived, {after} after it went")


@test("Trigger Bot (cooldown, miss chance)")
def _():
    # Hits on an unhurt husk: Shortest and Longest cooldown 50% hit about twice as often as the full cooldown. Miss
    # chance 25: of 16 times the husk comes under the crosshair, some are let go with no hit at all (and most not).
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 2.5} {HUSK}")
    wait(10)
    eid = next(e["id"] for e in ctl("entities")["entities"] if e.get("type") == "minecraft:husk")
    setting("triggerbot", "hostiles", "true")

    def hits(ticks):
        mark = probe_mark()
        wait(ticks)
        return sum(t.count("order attack") for t in probe_ticks(mark))

    ctl("look-at-entity", str(eid))
    only("triggerbot")
    full = hits(60)
    setting("triggerbot", "shortest cooldown (%)", "50")
    setting("triggerbot", "longest cooldown (%)", "50")
    wait(5)
    half = hits(60)
    only()
    say(".reset triggerbot")
    setting("triggerbot", "hostiles", "true")
    setting("triggerbot", "miss chance (%)", "25")
    only("triggerbot")
    windows = []
    for _ in range(16):
        ctl("look", "180", "0")
        wait(3)
        ctl("look-at-entity", str(eid))
        windows.append(hits(16))
    only()
    say(".reset triggerbot")
    missed = sum(1 for w in windows if w == 0)
    ok = full >= 3 and half >= full * 1.5 and 1 <= missed <= 12
    return ok, f"hits in 3 s: {full} at the full cooldown, {half} at 50%; Miss chance 25: {missed} of 16 looks let go ({windows})"


@test("Velocity (knockback chance)")
def _():
    # Knockback chance 0: every knockback is taken whole, as with Velocity off.
    plain = knocked()
    setting("velocity", "knockback chance (%)", "0")
    only("velocity")
    whole = knocked()
    setting("velocity", "knockback chance (%)", "100")
    cut = knocked()
    only()
    ok = plain[0] > 0.5 and abs(whole[0] - plain[0]) < 0.2 and cut[0] < 0.05
    return ok, f"pushed {plain[0]:.2f} blocks plain, {whole[0]:.2f} at chance 0, {cut[0]:.2f} at chance 100"


@test("Auto Clicker (clock simulations)")
def _():
    import pathlib, shutil, tempfile
    compiler = shutil.which("rustc")
    if not compiler:
        return None, "needs rustc for the standalone repeat-clock scenarios"
    source = pathlib.Path(__file__).resolve().parents[1] / "mixins" / "sunset" / "click_clock.rs"
    with tempfile.TemporaryDirectory(prefix="sunset-click-clock-") as directory:
        wrapper = pathlib.Path(directory) / "tests.rs"
        wrapper.write_text("#![allow(dead_code)]\n#[path = " + json.dumps(source.as_posix()) + "]\nmod click_clock;\n")
        binary = pathlib.Path(directory) / ("tests.exe" if os.name == "nt" else "tests")
        compiled = subprocess.run([compiler, "--edition=2024", "--test", str(wrapper), "-o", str(binary)], capture_output=True, text=True, timeout=120)
        if compiled.returncode:
            return False, compiled.stderr[-1500:]
        result = subprocess.run([str(binary)], capture_output=True, text=True, timeout=60)
        return result.returncode == 0, (result.stdout + result.stderr)[-1500:]


@test("Auto Clicker (CPS accuracy and air misses)")
def _():
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    measured = {}
    for target in ("entity", "air", "air-respect"):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        if target == "entity":
            cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 2.5} {HUSK}")
            wait(8)
            eid = next(e["id"] for e in ctl("entities")["entities"] if e.get("type") == "minecraft:husk")
            ctl("look-at-entity", str(eid))
        else:
            ctl("look", "0", "-80")
        for key, value in (("mode", "CPS"), ("min cps", "12"), ("max cps", "12")):
            setting("autoclicker", key, value)
        setting("autoclicker", "respect miss cooldown", "true" if target == "air-respect" else "false")
        only("autoclicker")
        mark = probe_mark()
        ctl("mouse", "left", "down")
        wait(60)
        ctl("mouse", "left", "up")
        only()
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            measured[target] = sum(line.strip() == f"pkt {PUNCH}" for line in f)
    ok = all(33 <= measured[name] <= 39 for name in ("entity", "air")) and 4 <= measured["air-respect"] <= 8
    return ok, f"12 CPS for 3 seconds, outgoing clicks: {measured}"


@test("Auto Clicker (release, toggle input and menus)")
def _():
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    ctl("look", "0", "-80")
    for key, value in (("mode", "CPS"), ("min cps", "12"), ("max cps", "12")):
        setting("autoclicker", key, value)
    def punches(mark):
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            return sum(line.strip() == f"pkt {PUNCH}" for line in f)
    def buttons(mark):
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            return [list(map(int, line.split()[2:6])) for line in f if line.startswith("buttons ")]
    try:
        ctl("options-set", "toggleAttack", "true")
        only("autoclicker")
        ctl("mouse", "left", "click")
        wait(4)
        mark = probe_mark()
        wait(30)
        held = buttons(mark)
        count = punches(mark)
        ctl("mouse", "left", "click")
        wait(3)
        release_mark = probe_mark()
        wait(20)
        released = buttons(release_mark)
        late = punches(release_mark)
        ctl("options-set", "toggleAttack", "false")
        ctl("mouse", "left", "down")
        wait(5)
        say(".menu autoclicker")
        wait(3)
        menu_mark = probe_mark()
        wait(20)
        menu_clicks = punches(menu_mark)
        close_menu()
        ctl("mouse", "left", "up")
        # The headless controller's synthetic keys do not drive physical binds.
        # Re-establish the held input after each command closes its chat screen.
        only()
        ctl("mouse", "left", "down")
        wait(3)
        off_mark = probe_mark()
        wait(20)
        off = punches(off_mark)
        only("autoclicker")
        again_mark = probe_mark()
        ctl("mouse", "left", "down")
        wait(40)
        again = punches(again_mark)
        ctl("mouse", "left", "up")
        ok = bool(held and released) and all(row[0] == row[2] == 1 for row in held)
        ok &= all(row[0] == row[2] == 0 for row in released) and late == off == 0
        ok &= 15 <= count <= 20 and menu_clicks <= 2 and 21 <= again <= 26
        return ok, f"toggle held clicks={count}, menu={menu_clicks}, released={late}, disabled={off}, restarted={again}; held parity={all(r[0] == r[2] for r in held)}"
    finally:
        ctl("options-set", "toggleAttack", "false")
        ctl("mouse", "left", "up")
        close_menu()
        say(".reset autoclicker")
        on.discard("autoclicker")


@test("Auto Clicker (independent buttons and low right CPS)")
def _():
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    slot(0, "snowball", 16)
    ctl("key", "1", "tap")
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 2.5} {HUSK}")
    wait(8)
    eid = next(e["id"] for e in ctl("entities")["entities"] if e.get("type") == "minecraft:husk")
    ctl("look-at-entity", str(eid))
    for key, value in (("mode", "CPS"), ("min cps", "12"), ("max cps", "12"), ("right", "true"),
                       ("right own cps", "true"), ("right min cps", "2"), ("right max cps", "2")):
        setting("autoclicker", key, value)
    try:
        only("autoclicker")
        mark = probe_mark()
        ctl("mouse", "left", "down")
        ctl("mouse", "right", "down")
        wait(60)
        ctl("mouse", "left", "up")
        ctl("mouse", "right", "up")
        wait(4)
        ticks = probe_ticks(mark)
        attacks = sum(t.count("order attack") for t in ticks)
        uses = sum(t.count(f"pkt {USE_ITEM}") for t in ticks)
        late = sum(t.count("order attack") + t.count(f"pkt {USE_ITEM}") for t in ticks if "order move" in t
                   and any(line in ("order attack", f"pkt {USE_ITEM}") for line in t[t.index("order move") + 1:]))
        return 33 <= attacks <= 39 and 5 <= uses <= 8 and late == 0, f"3 s: left attacks={attacks}, right uses={uses}, late interaction ticks={late}"
    finally:
        ctl("mouse", "left", "up")
        ctl("mouse", "right", "up")
        only()


@test("Auto Clicker (CPS, only with a weapon)")
def _():
    # Left held on an unhurt husk: CPS mode at 10 clicks about twice as often as Delay mode's 4 ticks (5 a second);
    # Only with a weapon: no clicks with an empty hand.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 2.5} {HUSK}")
    wait(10)
    eid = next(e["id"] for e in ctl("entities")["entities"] if e.get("type") == "minecraft:husk")
    ctl("look-at-entity", str(eid))

    def clicks(item):
        if item:
            slot(0, item)
        else:
            cmd("item replace entity @s hotbar.0 with minecraft:air")
        ctl("key", "1", "tap")
        only("autoclicker")
        wait(5)
        mark = probe_mark()
        ctl("mouse", "left", "down")
        wait(40)
        ctl("mouse", "left", "up")
        only()
        return sum(t.count("order attack") for t in probe_ticks(mark))

    setting("autoclicker", "mode", "Delay")
    delay = clicks("diamond_sword")
    setting("autoclicker", "mode", "CPS")
    setting("autoclicker", "min cps", "10")
    setting("autoclicker", "max cps", "10")
    cps = clicks("diamond_sword")
    setting("autoclicker", "only with a weapon", "true")
    bare = clicks(None)
    say(".reset autoclicker")
    ok = delay >= 6 and cps >= delay * 1.5 and bare <= 1
    return ok, f"clicks in 2 s: {delay} at Delay 4 ticks, {cps} at 10 CPS, {bare} bare-handed with Only with a weapon"


@test("Auto Clicker (mining and held item use)")
def _():
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    got = {}
    def held_buttons(mark, index):
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            rows = [list(map(int, line.split()[2:6])) for line in f if line.startswith("buttons ")]
        return bool(rows) and all(row[index] == row[index + 2] == 1 for row in rows)
    try:
        for stop in (False, True):
            reset()
            slot(0, "diamond_pickaxe")
            ctl("key", "1", "tap")
            cmd("gamemode survival", f"setblock {X0} {Y+2} {Z0+2} minecraft:stone")
            ctl("look-at", "block", str(X0), str(Y + 2), str(Z0 + 2))
            setting("autoclicker", "stop on blocks", str(stop).lower())
            only("autoclicker")
            ctl("mouse", "left", "down")
            wait(3)
            mark = probe_mark()
            wait(24)
            parity = held_buttons(mark, 0)
            ctl("mouse", "left", "up")
            wait(3)
            broke = count(X0, Y + 2, Z0 + 2, X0, Y + 2, Z0 + 2, "stone") == 0
            got[f"mine stop={stop}"] = broke and parity
        reset()
        slot(0, "shield")
        ctl("key", "1", "tap")
        for key, value in (("left", "false"), ("right", "true"), ("right own cps", "true"),
                           ("right min cps", "20"), ("right max cps", "20")):
            setting("autoclicker", key, value)
        only("autoclicker")
        ctl("mouse", "right", "down")
        wait(5)
        mark = probe_mark()
        wait(24)
        got["shield remains held"] = state().get("using") is not None and held_buttons(mark, 1)
        ctl("mouse", "right", "up")
        wait(3)
        got["shield released"] = state().get("using") is None
        return all(got.values()), str(got)
    finally:
        ctl("mouse", "left", "up")
        ctl("mouse", "right", "up")
        only()


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
    # The Range and Walls range rules, in Snap mode (Silent aura also keeps to vanilla reach: its own test).
    setting("killaura", "rotation mode", "Snap")

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
    path = f"{SHOTS}/{name}.png"
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
    """The next frame, greyscale (the PNG stays in SHOTS to look at)."""
    from PIL import Image
    ctl("screenshot", f"{SHOTS}/{name}.png")
    time.sleep(2)
    return Image.open(f"{SHOTS}/{name}.png").convert("L")


def colour_frame(name):
    """The next frame in colour (the PNG stays in SHOTS to look at)."""
    from PIL import Image
    ctl("screenshot", f"{SHOTS}/{name}.png")
    time.sleep(2)
    return Image.open(f"{SHOTS}/{name}.png").convert("RGB")


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
                f"Block Overlay on a fence {block_px}; Target ESP on Kill Aura's husk {target_px} (frames in {SHOTS}/vis_*.png)")


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
                + f"; the game's sky {fps0:.0f} fps, {noise:.1%} by itself (frames in {SHOTS}/atmo_*.png)")


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
        img = Image.open(f"{SHOTS}/{name}.png").convert("RGB")
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
        ctl("screenshot", f"{SHOTS}/{name}.png")
        time.sleep(2)
        img = Image.open(f"{SHOTS}/{name}.png").convert("RGB")
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
    # The menu's accent (#DB5000), as the tracer and the chat's [Sunset] draw it.
    accent = lambda r, g, b: r > 170 and 30 < g < 120 and b < 60
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
        ctl("screenshot", f"{SHOTS}/{name}.png")
        time.sleep(2)
        img = Image.open(f"{SHOTS}/{name}.png").convert("L")
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
            ctl("screenshot", f"{SHOTS}/{name}.png")
            time.sleep(2)
            img = Image.open(f"{SHOTS}/{name}.png").convert("RGB")
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


@test("Hand View totem pop")
def _():
    # A totem popped while looking at the sky: the totem's brown in the screen 12 ticks in (about a block out, facing
    # you), leaving out the HUD's corner, the hotbar and the arm (the pop's particles are green and yellow, the sky
    # blue). Totem pop scale 0.4 leaves well under half of it; Totem pop X 0.5 and Y 0.4 move it up and right of the
    # middle; Totem pop speed 4 has the pop gone by 14 ticks (its 40 ticks in 10).
    from PIL import Image

    def brown(name, wait_ticks=12):
        cmd("item replace entity @s weapon.offhand with minecraft:totem_of_undying", "effect give @s minecraft:instant_health 1 10 true")
        wait(5)
        ctl("chat", "/damage @s 1000 minecraft:generic")
        wait(wait_ticks)
        ctl("screenshot", f"{SHOTS}/{name}.png")
        time.sleep(2)
        img = Image.open(f"{SHOTS}/{name}.png").convert("RGB")
        w, h = img.size
        x0, y0 = w // 5, h // 20
        box = img.crop((x0, y0, w * 19 // 20, h * 4 // 5))
        at = []
        for i, (r, g, b) in enumerate(box.getdata()):
            x, y = x0 + i % box.width, y0 + i // box.width
            if not (x > w * 3 // 5 and y > h // 2) and 45 <= r <= 150 and g <= 0.8 * r and b <= 0.55 * r:
                at.append((x, y))
        up_right = sum(1 for x, y in at if x >= w * 0.55 and y < h // 2)
        return len(at) / (box.width * box.height), up_right / max(1, len(at))

    reset("survival")
    ctl("look", "0", "-70")
    wait(10)
    full, _ = brown("totem_pop_off")
    wait(40)
    toggle("handview", True)
    setting("handview", "totem pop scale", "0.4")
    small, _ = brown("totem_pop_small")
    wait(40)
    setting("handview", "totem pop scale", "1")
    setting("handview", "totem pop x", "0.5")
    setting("handview", "totem pop y", "0.4")
    moved, up_right = brown("totem_pop_moved")
    wait(40)
    setting("handview", "totem pop x", "0")
    setting("handview", "totem pop y", "0")
    setting("handview", "totem pop speed", "4")
    fast, _ = brown("totem_pop_fast", 14)
    toggle("handview", False)
    ok = full > 0.03 and 0.003 < small < full * 0.35 and moved > 0.02 and up_right > 0.8 and fast < full * 0.15
    return ok, (f"totem brown mid-pop: normal {full:.2%}, scale 0.4 {small:.2%}, X 0.5 Y 0.4 {moved:.2%} "
                f"({up_right:.0%} up and right), speed 4 after 14 ticks {fast:.2%}")


@test("Hand View no shadow")
def _():
    # Shut in a stone room (no light reaches the eye block): the held stone and the bare arm in the lower right are
    # dark; No hand shadow draws them at full light (block 15, sky 15), so much more of that corner is bright.
    from PIL import Image

    def corner(name):
        ctl("screenshot", f"{SHOTS}/{name}.png")
        time.sleep(2)
        img = Image.open(f"{SHOTS}/{name}.png").convert("L")
        box = img.crop((img.width * 60 // 100, img.height * 55 // 100, img.width, img.height))
        return sum(1 for v in box.getdata() if v > 60) / (box.width * box.height)

    reset("creative")
    cmd(f"fill {X0-4} {Y} {Z0-4} {X0+4} {Y+5} {Z0+4} minecraft:stone hollow",
        f"setblock {X0+2} {Y+1} {Z0+3} minecraft:cobblestone", f"setblock {X0-3} {Y+1} {Z0+2} minecraft:andesite")
    wait(40)
    eye = ctl("light", *[str(v) for v in (X0, Y + 2, Z0) * 2])
    ctl("look", "0", "10")
    cmd("item replace entity @s hotbar.0 with minecraft:stone")
    wait(10)
    held_off = corner("hv_dark_held_off")
    toggle("handview", True)
    setting("handview", "no hand shadow", "true")
    wait(5)
    held_on = corner("hv_dark_held_on")
    cmd("item replace entity @s hotbar.0 with minecraft:air")
    # The bare arm rises into view for a moment after the stone goes.
    wait(25)
    arm_on = corner("hv_dark_arm_on")
    toggle("handview", False)
    wait(5)
    arm_off = corner("hv_dark_arm_off")
    ok = held_on > 0.05 and held_off < held_on / 3 and arm_on > 0.05 and arm_off < arm_on / 3
    return ok, (f"bright share of the lower right in the dark room: stone held {held_off:.1%} -> {held_on:.1%}, "
                f"bare arm {arm_off:.1%} -> {arm_on:.1%} with No hand shadow; eye block sky {eye.get('sky')} block {eye.get('block')}")


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
        ctl("screenshot", f"{SHOTS}/{name}.png")
        time.sleep(2)
        return Image.open(f"{SHOTS}/{name}.png").convert("L")
    before = frame("fc_before")
    changed = []
    for i in range(3):
        ctl("key", "f5", "tap")
        wait(10)
        after = frame(f"fc_f5_{i}")
        changed.append(sum(1 for v in ImageChops.difference(before, after).getdata() if v > 40))
    only()
    return max(changed) < 0.01 * 1280 * 720, f"pixels changed by each of three F5 presses: {changed} (of {1280 * 720})"


@test("ESP, Tracers, Nametags (Range All)")
def _():
    # A ghast 150 blocks south and 40 up (120 on a server, whose view distance is 8 chunks; a ghast's tracking range
    # is 10): ESP and Tracers at their default Range (All) draw it in pink above the chat, at Range 64 they don't
    # (pink counted in the top 45% of the frame: tracers to cave mobs below go down). Then 300 husks around the arena:
    # the frame rate (uncapped) with ESP, Tracers and Nametags all on at All against all off.
    from PIL import Image
    pink = lambda r, g, b: r > 230 and 80 < g < 150 and b > 160

    def top_pink(name):
        path = f"{SHOTS}/{name}.png"
        ctl("screenshot", path)
        time.sleep(2)
        im = Image.open(path).convert("RGB")
        return sum(1 for px in im.crop((0, 0, im.width, im.height * 45 // 100)).getdata() if pink(*px))

    far = 120 if SERVER else 150
    reset("creative")
    cmd("kill @e[type=!player]", "difficulty easy")
    wait(5)
    cmd(f"summon ghast {X0+0.5} {Y+40} {Z0+far+0.5} {HUSK}")
    wait(60)
    seen = ctl("entities", "200", "ghast").get("entities", [])
    if not seen:
        cmd("kill @e[type=!player]")
        return None, f"the server did not send a ghast {far} blocks away"
    for mod in ("esp", "tracers"):
        setting(mod, "hostiles", "true")
        setting(mod, "players", "false")
        setting(mod, "passive", "false")
        setting(mod, "hostiles color", "Pink")
    ctl("look", "0", "0")
    only("esp")
    wait(10)
    esp_all = top_pink("range_esp_all")
    setting("esp", "range", "64")
    wait(5)
    esp_64 = top_pink("range_esp_64")
    only("tracers")
    wait(5)
    tracer_all = top_pink("range_tracer_all")
    setting("tracers", "range", "64")
    wait(5)
    tracer_64 = top_pink("range_tracer_64")
    setting("esp", "range", "0")
    setting("tracers", "range", "0")
    only()
    cap = ctl("options-get", "maxFps")["options"]["maxFps"]["value"]
    ctl("options-set", "maxFps", "260")
    cmd("kill @e[type=!player]")
    wait(5)
    for dx in range(-15, 15):
        cmd(f"summon husk {X0 + dx + 0.5} {Y+1} {Z0 + 8.5} {HUSK}", *[f"summon husk {X0 + dx + 0.5} {Y+1} {Z0 + 8.5 + 2 * k} {HUSK}" for k in range(1, 10)])
    wait(60)
    count = len(ctl("entities", "64", "husk").get("entities", []))
    ctl("look", "0", "10")

    def fps():
        wait(20)
        a = ctl("status")
        wait(100)
        b = ctl("status")
        return (b["frames"] - a["frames"]) / max(b["uptime_seconds"] - a["uptime_seconds"], 1e-6)
    off = fps()
    setting("nametags", "mobs", "true")
    only("esp", "tracers", "nametags")
    on_all = fps()
    only()
    ctl("options-set", "maxFps", str(cap))
    cmd("kill @e[type=!player]")
    ok = esp_all > 10 and esp_64 < esp_all / 4 and tracer_all > 20 and tracer_64 < tracer_all / 4 and count >= 300 and on_all > off * 0.5
    return ok, (f"ghast {far} blocks away, pink pixels: ESP {esp_all} at All, {esp_64} at 64; Tracers {tracer_all} at All, "
                f"{tracer_64} at 64; {count} husks: {off:.0f} fps with none on, {on_all:.0f} with ESP + Tracers + Nametags at All")


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
    ctl("screenshot", f"{SHOTS}/targethud.png")
    time.sleep(2)
    only()
    from PIL import Image
    img = Image.open(f"{SHOTS}/targethud.png").convert("RGB")
    w, h = img.size
    # The box sits right of and below the crosshair: count the health bar's green and the box's dark fill.
    region = img.crop((w // 2 + 20, h // 2 + 20, w // 2 + 20 + 260, h // 2 + 20 + 80))
    green = sum(1 for px in region.getdata() if px[1] > 110 and px[1] - max(px[0], px[2]) > 60)
    return green > 150, f"{green} health-bar pixels beside the crosshair"


@test("Kill Aura (Target strafe)")
def _():
    # A pig with 1000 health 3 blocks ahead, Kill Aura on it with a sword, holding W for 60 ticks: with Target strafe
    # (Strafe radius 2) you go round the pig near the radius (the angle round it turns well past 120 degrees); without
    # it you walk on.
    setting("killaura", "entities", "pig")
    pig = (X0 + 0.5, Z0 + 3.5)
    got = {}
    for name, on in (("off", False), ("on", True)):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        cmd(f"summon pig {pig[0]} {Y + 1} {pig[1]} {TOUGH_PIG}")
        wait(10)
        setting("killaura", "target strafe", "true" if on else "false")
        setting("killaura", "strafe radius", "2")
        only("killaura")
        ctl("key", "w", "down")
        points = poll(60, 2, pos)
        ctl("key", "w", "up")
        only()
        tail = points[10:]
        dists = [math.hypot(q[0] - pig[0], q[2] - pig[1]) for q in tail]
        angles = [math.atan2(q[2] - pig[1], q[0] - pig[0]) for q in tail]
        turn = sum(abs((b - a + math.pi) % (2 * math.pi) - math.pi) for a, b in zip(angles, angles[1:]))
        got[name] = (round(min(dists), 2), round(max(dists), 2), round(math.degrees(turn)))
    setting("killaura", "target strafe", "false")
    ok = 1.3 < got["on"][0] and got["on"][1] < 2.9 and got["on"][2] > 120 and got["off"][2] < 120
    return ok, f"(closest, furthest, degrees round the pig) after the first 20 ticks: without Target strafe {got['off']}, with it {got['on']}"


@test("Kill Aura (Silent aura)")
def _():
    # The default mode (Silent aura): it hits a pig within vanilla reach without turning your view, and
    # one within Range but beyond vanilla reach (3 blocks from the eyes) not at all.
    def attacked(dz):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        pig(dz)
        setting("killaura", "entities", "pig")
        look0 = aim_view()
        swings = state()["swings"]
        only("killaura")
        wait(60)
        swings = state()["swings"] - swings
        m = mobs("pig")
        turned = abs(wrap(aim_view()[0] - look0[0]))
        only()
        return 10 - (m[0]["health"] if m else 0), swings, turned

    near = attacked(2.5)
    far = attacked(4.2)
    return near[0] > 0 and near[2] < 0.01 and far[1] == 0, (
        f"pig 2.5 blocks away lost {near[0]} health, your view turned {near[2]:.2f} deg; 4.2 blocks away (Range 4.5): "
        f"{far[1]} swings")


@test("Anti AFK (Wander)")
def _():
    # The default mode: after the start delay with no move key held, small turns (Max yaw change 10) and
    # short steps; holding a move key starts the delay again.
    reset()
    setting("antiafk", "shortest start delay (s)", "1")
    setting("antiafk", "longest start delay (s)", "1")
    setting("antiafk", "frequency", "5")
    p0, last = pos(), aim_view()[0]
    only("antiafk")
    turned, most_step, moved = 0.0, 0.0, 0.0
    for _ in range(40):
        wait(5)
        yaw = aim_view()[0]
        step = abs(wrap(yaw - last))
        turned, most_step, last = turned + step, max(most_step, step), yaw
        moved = max(moved, flat(p0, pos()))
    only()
    # Each turn is at most Max yaw change (10), at up to 10 degrees a tick; one sample is 5 ticks, so up to two turns.
    return turned > 0.0 and most_step <= 20.0 and moved > 0.05, (
        f"turned {turned:.1f} deg in all, at most {most_step:.1f} in 5 ticks; stepped up to {moved:.2f} blocks in 10 s")


@test("Aim Assist (Simple)")
def _():
    # The default mode: with attack held it turns you toward a husk 25 degrees off on its own; with
    # attack let go (Require mouse down) it does nothing.
    eid = aim_setup()
    say(".reset aimassist")

    def gap_after(hold):
        face(eid, -25)
        only("aimassist")
        if hold:
            ctl("mouse", "left", "down")
        wait(40)
        if hold:
            ctl("mouse", "left", "up")
        only()
        yaw = aim_view()[0]
        ctl("look-at-entity", str(eid))
        wait(2)
        return abs(wrap(aim_view()[0] - yaw))

    held = gap_after(True)
    free = gap_after(False)
    aim_done()
    return held < 15.0 and free > 20.0, f"25 deg off: {held:.1f} deg off after 2 s holding attack, {free:.1f} without"


@test("Aim Assist (Simple, pulling away)")
def _():
    # Simple with attack held, a husk on the crosshair: the mouse pulls away 20 counts a tick for a second. The view
    # goes the way you pull, tick after tick, never back toward the husk while you pull (pulling against you, the
    # assist flickered the view back and forth); once you stop it may pull you back.
    eid = aim_setup()
    say(".reset aimassist")
    face(eid, 0)
    only("aimassist")
    ctl("mouse", "left", "down")
    wait(10)
    yaws = []
    for _ in range(20):
        ctl("mouse-move", "20", "0")
        wait(1)
        yaws.append(aim_view()[0])
    ctl("mouse", "left", "up")
    only()
    aim_done()
    steps = [wrap(b - a) for a, b in zip(yaws, yaws[1:])]
    back = sum(1 for d in steps if d < -0.01)
    pulled = wrap(yaws[-1] - yaws[0])
    ok = back == 0 and pulled > 20.0
    return ok, f"pulled {pulled:.1f} deg in a second, {back} ticks back toward the husk (each tick: {[round(d, 2) for d in steps]})"


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
    path = f"{SHOTS}/{name}.png"
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


@test("Crystal Optimizer (skip with weakness)")
def _():
    # Weak (Weakness) and bare-handed a hit does not break a crystal. With Skip with weakness it stays on your screen,
    # so a second click hits it again (two attacks); without, the first click takes it off your screen though the
    # server kept it, and the second hits nothing (one attack).
    if not probe_path():
        return None, "needs the probe (tools/probe)"

    def hits(skip):
        reset()
        cmd("item replace entity @s hotbar.0 with minecraft:air", "effect give @s minecraft:weakness 60 0 true")
        ctl("key", "1", "tap")
        cmd(f"summon end_crystal {X0+0.5} {Y+1} {Z0+4.0} {{ShowBottom:0b}}")
        wait(20)
        setting("crystaloptimizer", "skip with weakness", skip)
        only("crystaloptimizer")
        ctl("look", "0", "10")
        wait(3)
        mark = probe_mark()
        ctl("mouse", "left", "click")
        wait(15)
        kept = check("execute if entity @e[type=end_crystal]")
        ctl("mouse", "left", "click")
        wait(10)
        attacks = sum(t.count("order attack") for t in probe_ticks(mark))
        only()
        cmd("kill @e[type=end_crystal]", "effect clear @s")
        return attacks, kept

    skipped, plain = hits("true"), hits("false")
    say(".reset crystaloptimizer")
    if not plain[1]:
        return None, "this server breaks a crystal hit under Weakness (Horizon's own server does)"
    ok = skipped[0] == 2 and plain[0] == 1
    return ok, f"attacks from two weak clicks: {skipped[0]} with Skip with weakness, {plain[0]} without"


# ---- Anti Bot: test-only incoming packets, real target selection and outgoing hits

ANTIBOT_DECOY, ANTIBOT_REAL = 900001, 900002


@test("Anti Bot (combat settings)")
def _():
    modules = said_after(".modules")
    settings = {name: said_after(f".settings {name}") for name in ("killaura", "aimassist", "triggerbot")}
    standalone = any("Anti Bot" in line for line in modules)
    embedded = all(any(line.startswith("Anti bot sensitivity:") for line in lines) for lines in settings.values())
    return not standalone and embedded, f"standalone={standalone}, sensitivity in combat settings={embedded}"


@test("Anti Bot (classifier simulations)")
def _():
    import pathlib, shutil, tempfile
    compiler = shutil.which("rustc")
    if not compiler:
        return None, "needs rustc for the standalone classifier scenarios"
    source = pathlib.Path(__file__).resolve().parents[1] / "mixins" / "sunset" / "antibot_state.rs"
    with tempfile.TemporaryDirectory(prefix="sunset-antibot-") as directory:
        wrapper = pathlib.Path(directory) / "tests.rs"
        wrapper.write_text("#![allow(dead_code)]\n#[path = " + json.dumps(source.as_posix()) + "]\nmod antibot_state;\n")
        binary = pathlib.Path(directory) / ("tests.exe" if os.name == "nt" else "tests")
        compiled = subprocess.run([compiler, "--edition=2024", "--test", str(wrapper), "-o", str(binary)], capture_output=True, text=True, timeout=120)
        if compiled.returncode:
            return False, compiled.stderr[-1500:]
        result = subprocess.run([str(binary)], capture_output=True, text=True, timeout=60)
        return result.returncode == 0, (result.stdout + result.stderr)[-1500:]


def antibot_fixture(entities, ticks=4, **control):
    """Only the separately staged probe reads this file; the shipped mod has no fixture controls."""
    path = os.environ.get("SUNSET_ANTIBOT_FIXTURE")
    if not path:
        raise RuntimeError("needs tools/probe and SUNSET_ANTIBOT_FIXTURE in game and verifier")
    revision = time.time_ns()
    pending = path + ".next"
    with open(pending, "w") as f:
        json.dump({"revision": revision, "entities": entities} | control, f)
    os.replace(pending, path)
    wait(ticks)
    for _ in range(10):
        try:
            with open(path + ".state") as f:
                result = json.load(f)
            if result.get("revision") == revision:
                if result.get("error"):
                    raise RuntimeError(result["error"])
                return result
        except (OSError, json.JSONDecodeError):
            pass
        wait(2)
    raise RuntimeError("Anti Bot fixture did not acknowledge command")


def antibot_state():
    path = os.environ["SUNSET_ANTIBOT_FIXTURE"] + ".state"
    for _ in range(10):
        try:
            with open(path) as f:
                return json.load(f)
        except (OSError, json.JSONDecodeError):
            wait(1)
    raise RuntimeError("Anti Bot fixture status unreadable")


def antibot_entity(eid=ANTIBOT_DECOY, motion="plain", **fields):
    return dict(id=eid, generation=1, name="FixtureDecoy" if eid == ANTIBOT_DECOY else "FixtureReal",
                motion=motion, x=0.0 if eid == ANTIBOT_DECOY else 0.8, y=0.0, z=2.0 if eid == ANTIBOT_DECOY else 2.7,
                scale=1.0, listed=True, mode=0) | fields


def antibot_prepare():
    reset()
    antibot_fixture([])
    for consumer in ("killaura", "aimassist", "triggerbot"):
        say(f".reset {consumer}")
        on.discard(consumer)
    say(".friend remove FixtureReal")
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    setting("killaura", "anti bot", "true")
    setting("killaura", "range", "4.5")
    setting("aimassist", "anti bot", "true")
    setting("aimassist", "require mouse down", "false")
    setting("aimassist", "only while turning", "false")
    for consumer in ("killaura", "aimassist", "triggerbot"):
        setting(consumer, "anti bot sensitivity", "Balanced")
    only()


def antibot_hits(mark):
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        return [int(line.split()[1]) for line in f if line.startswith("attack ")]


def antibot_only(*consumers):
    """Switch consumers while retaining the shared internal observations."""
    desired = set(consumers)
    for name in list(on):
        if name not in desired:
            toggle(name, False)
    for name in sorted(desired):
        toggle(name, True)


@test("Anti Bot (listed decoy and stationary player)")
def _():
    # Removing either consumer's shared classifier check must expose the closer, fully listed decoy.
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    try:
        antibot_prepare()
        antibot_fixture([antibot_entity(motion="orbit"), antibot_entity(ANTIBOT_REAL, listed=False, scale=3.0)], 100)
        status = antibot_state()
        entities = {e["id"]: e for e in status["entities"]}
        classified = entities[ANTIBOT_DECOY]["blocked"] and not entities[ANTIBOT_REAL]["blocked"]
        antibot_only("killaura")
        mark = probe_mark()
        wait(60)
        hits = antibot_hits(mark)
        antibot_only()
        ctl("look", "0", "0")
        antibot_only("aimassist")
        wait(30)
        aim = antibot_state()["aim"]
        ok = classified and ANTIBOT_REAL in hits and ANTIBOT_DECOY not in hits and aim == ANTIBOT_REAL
        return ok, f"classified={classified}, Aura hits={hits}, Aim Assist target={aim}; {entities[ANTIBOT_DECOY]['diagnostics']}"
    finally:
        only()
        antibot_fixture([])


@test("Anti Bot (cached targets revalidated)")
def _():
    # A target becoming a decoy must leave each consumer's cached target, without respawning it.
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    observations = {}
    try:
        antibot_prepare()
        setting("killaura", "target lock", "true")
        for consumer, field, mode in (("killaura", "aura", None), ("aimassist", "aim", "Simple"), ("aimassist", "aim", "Duelkit")):
            antibot_only()
            ctl("mouse", "left", "up")
            if mode:
                setting("aimassist", "mode", mode)
                setting("aimassist", "require mouse down", "true" if mode == "Simple" else "false")
            antibot_fixture([antibot_entity()], 30)
            ctl("look", "0", "0")
            antibot_only(consumer)
            if mode == "Simple":
                ctl("mouse", "left", "down")
            wait(30)
            before = antibot_state()[field]
            # Same spawn generation and UUID: this tests revalidation, not destruction cleanup.
            antibot_fixture([antibot_entity(motion="orbit"), antibot_entity(ANTIBOT_REAL)], 100)
            mark = probe_mark()
            ctl("look", "0", "0")
            wait(40)
            after = antibot_state()[field]
            hits = antibot_hits(mark) if consumer == "killaura" else []
            say(".friend add FixtureReal")
            wait(4)
            friend = antibot_state()[field]
            say(".friend remove FixtureReal")
            observations[f"{consumer} {mode}"] = (before, after, hits, friend)
            antibot_only()
            antibot_fixture([])
        ok = all(before == ANTIBOT_DECOY and after == ANTIBOT_REAL and ANTIBOT_DECOY not in hits and friend is None
                 for before, after, hits, friend in observations.values())
        return ok, f"cached target before/after and late hits: {observations}"
    finally:
        ctl("mouse", "left", "up")
        say(".friend remove FixtureReal")
        only()
        antibot_fixture([])


@test("Anti Bot (consumer switches and sensitivity)")
def _():
    # Each consumer's toggle and sensitivity control targeting without a master switch.
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    observed = {}
    try:
        antibot_prepare()
        antibot_fixture([antibot_entity(motion="orbit")], 100)
        for consumer, field in (("killaura", "aura"), ("aimassist", "aim")):
            antibot_only()
            wait(100)
            for label, own, sensitivity in (("protected", "true", "Balanced"), ("consumer off", "false", "Balanced"), ("conservative", "true", "Conservative")):
                antibot_only()
                setting(consumer, "anti bot", own)
                setting(consumer, "anti bot sensitivity", sensitivity)
                ctl("look", "0", "0")
                antibot_only(consumer)
                seen = []
                for _ in range(20):
                    wait(4)
                    seen.append(antibot_state()[field])
                observed[f"{consumer} {label}"] = ANTIBOT_DECOY in seen
        ok = all(value == ("protected" not in name) for name, value in observed.items())
        return ok, f"decoy selected: {observed}"
    finally:
        only()
        antibot_fixture([])


@test("Anti Bot (Clicks only Silent aim and Hit flick)")
def _():
    # The alternate click paths must not issue a hit at a rejected entity, and their opt-out must still work.
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    observed = {}
    try:
        antibot_prepare()
        setting("aimassist", "mode", "Clicks only")
        setting("aimassist", "flick needs sprint", "false")
        # Orbit raises confidence first; briefly hovering holds the player still for each actual click.
        antibot_fixture([antibot_entity(motion="orbit")], 100)
        for path in ("silent aim", "hit flick"):
            antibot_only()
            setting("aimassist", "silent aim", "true" if path == "silent aim" else "false")
            setting("aimassist", "hit flick", "true" if path == "hit flick" else "false")
            for protected in (True, False):
                setting("aimassist", "anti bot", "true" if protected else "false")
                antibot_only("aimassist")
                mark = probe_mark()
                trace = []
                for _ in range(8):
                    # Pause at its current position, avoiding artificial jumps
                    # across the observer that legitimately weaken orbit evidence.
                    at = next(e["pos"] for e in ctl("entities")["entities"] if e["id"] == ANTIBOT_DECOY)
                    antibot_fixture([antibot_entity(motion="hover", x=at[0] - X0 - 0.5,
                        y=at[1] - Y - 1, z=at[2] - Z0 - 0.5)], 2)
                    ctl("look-at-entity", str(ANTIBOT_DECOY))
                    if path == "silent aim":
                        yaw, pitch = aim_view()
                        ctl("look", str(yaw + 30), str(pitch))
                    blocked = next(e["blocked"] for e in antibot_state()["entities"] if e["id"] == ANTIBOT_DECOY)
                    click_mark = probe_mark()
                    ctl("mouse", "left", "click")
                    wait(4)
                    trace.append((blocked, antibot_hits(click_mark)))
                    antibot_fixture([antibot_entity(motion="orbit")], 30)
                observed[f"{path} protected={protected}"] = (antibot_hits(mark).count(ANTIBOT_DECOY), trace)
                antibot_only()
        ok = all((count == 0 and all(blocked for blocked, _ in trace)) if "=True" in key else count > 0
                 for key, (count, trace) in observed.items())
        counts = {key: count for key, (count, _) in observed.items()}
        return ok, f"wire hits at rejected player: {counts}" + (f"; trace={observed}" if not ok else "")
    finally:
        only()
        antibot_fixture([])


@test("Anti Bot (entity ID reuse clears rejection)")
def _():
    # Reusing a rejected ID for a different real player must create a new identity and restore eligibility.
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    try:
        antibot_prepare()
        before = antibot_fixture([antibot_entity(motion="orbit")], 100)["entities"][0]
        after = antibot_fixture([antibot_entity(generation=2, uuid=str((1 << 64) | ANTIBOT_DECOY), name="ReplacementReal")], 20)["entities"][0]
        antibot_only("killaura")
        mark = probe_mark()
        wait(40)
        hits = antibot_hits(mark)
        ok = before["blocked"] and not after["blocked"] and before["token"] != after["token"] and ANTIBOT_DECOY in hits
        return ok, f"rejection {before['blocked']}->{after['blocked']}, token {before['token']}->{after['token']}, replacement hits={hits}"
    finally:
        only()
        antibot_fixture([])


# ---- Aim Assist (Duelkit mode): real mouse motion (mouse-move), and for moving targets the probe's frames

@test("Anti Bot (world reset preserves player-info)")
def _():
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    try:
        antibot_prepare()
        before = antibot_fixture([antibot_entity(motion="orbit")], 100)["entities"][0]
        # The world hook resets motion/tickets, while current player-info survives
        # normal respawns. The fixture does not resend mode after this boundary.
        after = antibot_fixture([antibot_entity(motion="orbit")], 100, world_reset=True)["entities"][0]
        return before["blocked"] and after["blocked"] and before["token"] != after["token"], \
            f"before={before}, after world reset={after}"
    finally:
        only()
        antibot_fixture([])


@test("Anti Bot (Trigger Bot delayed target revalidated)")
def _():
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    try:
        antibot_prepare()
        setting("triggerbot", "anti bot", "true")
        antibot_fixture([antibot_entity()], 20)
        ctl("look-at-entity", str(ANTIBOT_DECOY))
        mark = probe_mark()
        antibot_only("triggerbot")
        wait(40)
        control = antibot_hits(mark).count(ANTIBOT_DECOY)
        antibot_only()
        status = antibot_fixture([antibot_entity(generation=2, evade_pending=True)], 20)
        before = status["evasions"]
        ctl("look-at-entity", str(ANTIBOT_DECOY))
        mark = probe_mark()
        antibot_only("triggerbot")
        wait(40)
        hits = antibot_hits(mark)
        after = antibot_state()["evasions"]
        return control > 0 and after == before + 1 and ANTIBOT_DECOY not in hits, \
            f"control hits={control}, evasions={before}->{after}, delayed hits={hits}"
    finally:
        only()
        antibot_fixture([])

@test("Anti Bot (delayed Hit flick identity)")
def _():
    if not os.environ.get("SUNSET_ANTIBOT_FIXTURE") or not probe_path():
        return None, "needs the test probe and SUNSET_ANTIBOT_FIXTURE"
    try:
        antibot_prepare()
        setting("aimassist", "mode", "Clicks only")
        setting("aimassist", "hit flick", "true")
        setting("aimassist", "flick needs sprint", "false")
        antibot_fixture([antibot_entity()], 20)
        antibot_only("aimassist")
        ctl("look-at-entity", str(ANTIBOT_DECOY))
        mark = probe_mark()
        ctl("mouse", "left", "click")
        wait(20)
        control = antibot_hits(mark).count(ANTIBOT_DECOY)
        before = antibot_state()["replacements"]
        antibot_fixture([antibot_entity(replace_on_turn=True)], 4)
        ctl("look-at-entity", str(ANTIBOT_DECOY))
        mark = probe_mark()
        ctl("mouse", "left", "click")
        wait(20)
        hits = antibot_hits(mark)
        after = antibot_state()["replacements"]
        return control > 0 and after == before + 1 and ANTIBOT_DECOY not in hits, \
            f"control hits={control}, replacements={before}->{after}, stale deed hits={hits}"
    finally:
        only()
        antibot_fixture([])

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
    """The Duelkit mode's gap: degrees from the crosshair to the nearest point of the box shrunk by the inset."""
    c, hw, hh = f.mobs[eid]
    half = (hw - inset, hh - inset, hw - inset)
    look = view(f.yaw, f.pitch)
    d = math.sqrt(sum((c[i] - f.eye[i]) ** 2 for i in range(3)))
    near = [min(max(f.eye[i] + look[i] * d, c[i] - abs(half[i])), c[i] + abs(half[i])) for i in range(3)]
    return angle(look, [near[i] - f.eye[i] for i in range(3)])


def aim_defaults():
    """Aim Assist at its defaults in Duelkit mode (the tests below exercise Duelkit mode; Simple has its own)."""
    say(".reset aimassist")
    say(".set aimassist mode Duelkit")
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
    only()
    say(".reset aimassist")
    cmd("difficulty peaceful")


@test("Aim Assist (pause while mining, only while turning)")
def _():
    # Simple mode, no mouse button needed, a husk 30 degrees to your right: it turns you toward it; Only while
    # turning: not while the mouse is still; Pause while mining: not while you dig the stone in front of you.
    eid = aim_setup()
    say(".set aimassist mode Simple")
    setting("aimassist", "require mouse down", "false")
    # Facing south the husk on your right, you look east of it: at this stone.
    cmd(f"fill {X0+1} {Y+1} {Z0+2} {X0+1} {Y+3} {Z0+2} minecraft:stone")

    def turned(dig=False):
        face(eid, -30)
        yaw = aim_view()[0]
        only("aimassist")
        if dig:
            # Pressed after the toggle: typing it opens the chat, which lets go of the mouse.
            ctl("mouse", "left", "down")
            wait(1)
            yaw = aim_view()[0]
        wait(30)
        moved = abs(aim_view()[0] - yaw)
        only()
        if dig:
            ctl("mouse", "left", "up")
        return moved

    plain = turned()
    setting("aimassist", "only while turning", "true")
    still = turned()
    setting("aimassist", "only while turning", "false")
    setting("aimassist", "pause while mining", "true")
    digging = turned(dig=True)
    cmd(f"fill {X0+1} {Y+1} {Z0+2} {X0+1} {Y+3} {Z0+2} minecraft:air")
    aim_done()
    ok = plain > 2.0 and still < 0.5 and digging < 0.5
    return ok, f"degrees turned in 1.5 s: {plain:.2f} plain, {still:.2f} with Only while turning, {digging:.2f} digging with Pause while mining"


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
    (("step",), [("step", "step up", "false"), ("step", "reverse step", "true")]),
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
    (("offhand",), [("offhand", "item", m)]) for m in ("Crystal", "Totem", "Gapple", "Enchanted gapple", "None")
] + [
    (("offhand",), [("offhand", "silent open", m)]) for m in ("false", "true")
] + [
    (("autoexp",), [("autoexp", "mode", m)]) for m in ("Both", "Armor", "Hands")
] + [
    (("fastuse",), [("fastuse", "mode", m)]) for m in ("All", "Some")
] + [
    (("fastuse",), [("fastuse", "throwables", "true"), ("fastuse", "throw mode", m)]) for m in ("Delay", "Every tick", "Burst")
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
                         "autowalk", "fastclimb", "autojump", "sneak", "noslow", "nojumpdelay", "antivoid",
                         "elytraboost", "chestswap", "scaffold", "timer", "fastplace", "breakdelay", "autoclicker", "autoweapon",
                         "triggerbot", "autoreplenish", "autogap", "airplace", "reach", "autotrap", "autoweb",
                         "holefiller", "aimassist", "crystaloptimizer", "flamethrower", "autoshearer", "automount",
                         "cheststealer", "inventorymanager", "middleclickextra", "autofish")
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


# ---- Crystal Aura and Anchor Aura, and their Macro sections (Rust's Crystal Macro and Anchor Macro, PR #16), against Bot1
# (CTL2) on a server.
# The arena is obsidian (a crystal needs it under, and nothing round it blows away), Bot1 stands 3 blocks south with a
# thousand health and no knockback, so every hit can be counted; you wear netherite with Blast Protection IV.

ARMOR = [("head", "netherite_helmet"), ("chest", "netherite_chestplate"), ("legs", "netherite_leggings"),
         ("feet", "netherite_boots")]


def blast_arena(bot=True, floor="obsidian", enchant="blast_protection", depth=1):
    cmd(f"fill {X0-5} {Y-depth} {Z0-4} {X0+5} {Y} {Z0+8} minecraft:{floor}", "difficulty normal",
        "gamerule natural_health_regeneration false", "gamerule fire_damage false")
    for part, item in ARMOR:
        cmd(f"item replace entity @s armor.{part} with minecraft:{item}[enchantments={{{enchant}:4}}]")
    if bot:
        blast_bot()
    wait(20)


def blast_bot():
    """Bot1 back from the death screen (a run can take all of its thousand health), 3 blocks south, healed."""
    bot_respawned()
    cmd("gamemode survival Bot1", "attribute Bot1 minecraft:max_health base set 1000",
        "attribute Bot1 minecraft:knockback_resistance base set 1", "attribute Bot1 minecraft:explosion_knockback_resistance base set 1",
        f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 3.5}", "effect give Bot1 minecraft:instant_health 1 28 true")
    ctl2("release")


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
            # "Killed 3 entities", or for one: "Killed End Crystal".
            n = line.split()[1]
            return int(n) if n.isdigit() else 1
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
    blast_bot()
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
    blast_bot()
    cmd("effect give @s minecraft:fire_resistance 120 0 true")
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


@test("Crystal Aura from any hand")
def _():
    """vs.'s report (3 Nov 2026): walking up to a player, Crystal Aura at its defaults did nothing. Holding a pickaxe,
    food or blocks it never took the hand; holding a sword on plain ground it put down obsidian and one crystal, then
    turned toward a spot by its feet for good (aimed from eyes that fell with gravity while standing). Here: plain
    stone ground (obsidian goes down first), you in Protection IV netherite, Bot1 3 blocks south, defaults, 5 s with
    each item held: crystals placed, damage dealt, the item back in hand after. On Horizon's server it also covers the
    third cause: clean() sets Peaceful and blast_arena() Normal, and that server never tells you the change, so the
    damage maths saw Peaceful (no damage to anyone) and nothing was placed at all."""
    if not CTL2:
        return None, "needs a second player (CTL2)"
    reset()
    ground = f"fill {X0-5} {Y-5} {Z0-4} {X0+5} {Y} {Z0+8} minecraft:stone"
    blast_arena(floor="stone", enchant="protection", depth=5)
    control = hand_crystal()
    runs = {}
    for item in ("diamond_pickaxe", "cooked_beef", "cobblestone"):
        ensure_alive()
        cmd("kill @e[type=minecraft:end_crystal]", f"fill {X0-5} {Y+1} {Z0-4} {X0+5} {Y+4} {Z0+8} minecraft:air", ground,
            f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0", "effect give @s minecraft:instant_health 1 28 true")
        blast_bot()
        slot(0, item, 1 if item == "diamond_pickaxe" else 32)
        slot(1, "end_crystal", 64)
        slot(2, "obsidian", 64)
        ctl("key", "1", "tap")
        wait(20)
        mark = probe_mark() if SERVER and probe_path() else None
        before = ctl("inventory").get("counts", {})
        bot0 = bot_health()
        only("crystalaura")
        wait(100)
        only()
        wait(10)
        after = ctl("inventory").get("counts", {})
        runs[item] = {"crystals": before.get("minecraft:end_crystal", 0) - after.get("minecraft:end_crystal", 0),
                      "obsidian": before.get("minecraft:obsidian", 0) - after.get("minecraft:obsidian", 0),
                      "dealt": bot0 - bot_health(), "held": (held().get("item") or "").removeprefix("minecraft:"),
                      "most": wire_stats(mark)[1] if mark is not None else None}
    blast_done()
    cmd(f"fill {X0-5} {Y-5} {Z0-4} {X0+5} {Y-2} {Z0+8} minecraft:air")
    ok = all(r["crystals"] >= 3 and r["obsidian"] >= 1 and (r["dealt"] > 0 or control == 0) and r["held"] == item
             and r["most"] in (None, 0, 1) for item, r in runs.items())
    return ok, (f"a crystal broken by hand beside Bot1 costs it {control}; " + "; ".join(
        f"{item}: {r['crystals']} crystals, {r['obsidian']} obsidian, Bot1 took {r['dealt']:.0f}, holding {r['held']} "
        f"after, most move packets in a tick {r['most']}" for item, r in runs.items()))


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


@test("Crystal Aura and Anchor Aura: Macro")
def _():
    reset()
    blast_arena(bot=False)
    # A crystal held on the obsidian 3 blocks ahead: placed and broken over and over for 5 s.
    cmd(f"setblock {X0} {Y+1} {Z0+3} minecraft:obsidian")
    for name, value in (("aura", "false"), ("macro", "true"), ("max self damage", "20")):
        setting("crystalaura", name, value)
    crystals, taken, alive, most = macro_run("crystalaura", [("end_crystal", 64)], (X0 + 0.5, Y + 2.0, Z0 + 3.5), 100)
    left = crystals_left()
    per = 100 / max(crystals["end_crystal"], 1)
    # An anchor held at the floor 3 blocks ahead: placed, charged, set off, once a click (the block in between guards).
    cmd(f"fill {X0-1} {Y+1} {Z0+1} {X0+1} {Y+4} {Z0+5} minecraft:air", "kill @e[type=minecraft:end_crystal]")
    anchors, anchor_taken, anchor_alive, anchor_most = 0, 0, True, None
    times = []
    setting("anchoraura", "aura", "false")
    setting("anchoraura", "macro", "true")
    for _ in range(5):
        used, t, a, m = macro_run("anchoraura", [("respawn_anchor", 16), ("glowstone", 16)], (X0 + 0.5, Y + 1.0, Z0 + 3.5), 3)
        anchors += used["respawn_anchor"]
        anchor_taken += t
        anchor_alive &= a
        anchor_most = max(anchor_most or 0, m or 0) if m is not None else anchor_most
        cmd(f"fill {X0-1} {Y+1} {Z0+1} {X0+1} {Y+4} {Z0+5} minecraft:air")
    blast_done()
    ok = (crystals["end_crystal"] >= 5 and left in (0, 1) and alive and most in (None, 0, 1) and anchors >= 4 and anchor_alive
          and anchor_most in (None, 0, 1))
    return ok, (f"Crystal Aura's Macro: {crystals['end_crystal']} crystals in 5 s ({per:.1f} ticks each), {left} left standing, "
                f"you took {taken:.0f} of 200; Anchor Aura's Macro: {anchors} of 5 clicks set an anchor off, you took "
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
    for m in sorted(on | touched):
        say(f".reset {m}")
    say(".reset hud")
    on.clear()
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


def weapon(*sections):
    """Auto Weapon with only these of its sections on (best weapon, axe on shields)."""
    for section in ("best weapon", "axe on shields"):
        setting("autoweapon", section, "true" if section in sections else "false")


@test("Auto Weapon (Axe on shields)")
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
    weapon("axe on shields")
    runs = {}
    for name, mods in (("off", ("killaura",)), ("on", ("killaura", "autoweapon"))):
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
        f"Bot1 behind a raised shield lost {off[0]} health in 10 s without Axe on shields, {on[0]} with it; the axe "
        f"was seen in hand {on[1]} times and the hand went back to {on[2] or 'nothing'}")


@test("Auto Weapon swaps before the move")
def _():
    # Kill Aura with a sword at Bot1 behind a raised shield, an axe in slot 3, Auto Weapon's Axe on shields: every
    # slot change (SetCarriedItem) goes before its tick's move packet, as a hotbar key does (one after flags Grim's
    # Post), one a tick, never one hit's swap twice; the axe goes in a tick before its hit (Horizon's server hits
    # with what was held as its tick began) and the sword comes back; one move packet a tick (no extra look).
    if not CTL2:
        return None, "needs a second player"
    reset()
    slot(0, "diamond_sword")
    slot(2, "diamond_axe")
    ctl("key", "1", "tap")
    setting("killaura", "entities", "player")
    weapon("axe on shields")
    bot_ready(2.5, 180, "give Bot1 minecraft:shield")
    ctl2("select", "1")
    ctl2("mouse", "right", "down")
    wait(10)
    before, mark = bot_health(), probe_mark()
    only("killaura", "autoweapon")
    back = f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5} 180 0"
    poll(60, 2, lambda: cmd(back))
    only()
    wait(10)
    lost = round(before - bot_health(), 1)
    ctl2("mouse", "right", "up")
    bot_done()
    ticks = probe_ticks(mark)
    swaps = late = doubled = crowded = 0
    for t in ticks:
        n = t.count(f"pkt {SET_CARRIED_ITEM}")
        swaps += n
        doubled += n > 1
        moves = [i for i, line in enumerate(t) if line == "order move"]
        crowded += len(moves) > 1
        late += any(line == f"pkt {SET_CARRIED_ITEM}" and moves and i > moves[0] for i, line in enumerate(t))
    # The axe's tick has no hit; the hit's tick is the next one.
    ahead = [i for i, t in enumerate(ticks) if f"pkt {SET_CARRIED_ITEM}" in t and "order attack" not in t
             and i + 1 < len(ticks) and "order attack" in ticks[i + 1]]
    hand = (state().get("held") or {}).get("item", "")
    ok = swaps >= 2 and not late and not doubled and not crowded and ahead and lost > 0 and "sword" in hand
    return ok, (f"{swaps} slot changes in {len(ticks)} ticks: after the move packet {late}, two in a tick {doubled}, "
                f"ticks with two moves {crowded}, axe a tick before its hit {len(ahead)}; Bot1 lost {lost}, the hand "
                f"went back to {hand or 'nothing'}")


# ---- Cart (TNT minecart PvP)

USE_ITEM_ON = 66


def cart(*sections, **values):
    """Cart with only these of its sections on (aura, auto cart, xbow, pearls, cover, refill) and settings set."""
    for section in ("aura", "auto cart", "xbow", "pearls", "cover", "refill"):
        setting("cart", section, "true" if section in sections else "false")
    for name, value in values.items():
        setting("cart", name.replace("_", " "), value)


def tnt_carts():
    """TNT minecarts in the world, as the server counts them."""
    cmd("execute if entity @e[type=minecraft:tnt_minecart]")
    wait(1)
    line = chat_first()
    if "passed" not in line:
        return 0
    digits = "".join(c for c in line.rsplit(":", 1)[-1] if c.isdigit())
    return int(digits) if digits else 1


def cart_items():
    """TNT minecart items lying about (a cart broken, not set off)."""
    return sum(1 for e in ctl("entities").get("entities", []) if e.get("kind") == "item" and "tnt_minecart" in json.dumps(e))


def cart_order(mark):
    """From the wire probe: actions (slot changes, block and item uses, hits) after their tick's move packet, ticks with
    two slot changes, ticks with two move packets."""
    late = doubled = crowded = 0
    acts = (f"pkt {SET_CARRIED_ITEM}", f"pkt {USE_ITEM_ON}", f"pkt {USE_ITEM}", "order attack")
    for t in probe_ticks(mark):
        moves = [i for i, line in enumerate(t) if line == "order move"]
        crowded += len(moves) > 1
        doubled += t.count(f"pkt {SET_CARRIED_ITEM}") > 1
        late += any(line in acts and moves and i > moves[0] for i, line in enumerate(t))
    return late, doubled, crowded


def immune():
    cmd("effect give @s minecraft:resistance 120 4 true", "effect give @s minecraft:fire_resistance 120 0 true")


@test("Cart Aura")
def _():
    # A TNT cart summoned on a rail 2 blocks ahead (someone else's) is broken with the sword from slot 3, swapped in a
    # tick before the hit, and the hand goes back; a cart you put down yourself is left alone. Broken standing still,
    # a TNT cart drops as an item and does not go off.
    reset()
    cart("aura")
    slot(0, "dirt", 16)
    slot(2, "diamond_sword")
    ctl("key", "1", "tap")
    cmd(f"setblock {X0} {Y+1} {Z0+2} minecraft:rail", f"summon tnt_minecart {X0+0.5} {Y+1.06} {Z0+2.5}")
    wait(5)
    floor = count(X0 - 3, Y, Z0 - 1, X0 + 3, Y, Z0 + 5, "stone")
    mark = probe_mark()
    only("cart")
    took = None
    for t in range(30):
        if tnt_carts() == 0:
            took = (t + 1) * 3
            break
    wait(15)
    hand = (held().get("item") or "")
    late, doubled, crowded = cart_order(mark)
    blasted = floor - count(X0 - 3, Y, Z0 - 1, X0 + 3, Y, Z0 + 5, "stone")
    # Your own cart: placed by your right click on the rail, then 3 s for Aura to leave it be.
    cmd("kill @e[type=item]")
    slot(0, "tnt_minecart")
    ctl("look", "0", "40")
    wait(5)
    ctl("mouse", "right", "down")
    wait(3)
    ctl("mouse", "right", "up")
    wait(3)
    put = tnt_carts()
    wait(55)
    kept = tnt_carts()
    only()
    ok = took is not None and blasted == 0 and "dirt" in hand and not late and not doubled and not crowded and kept == 1
    return ok, (f"enemy cart broken after {took} ticks, floor blocks blown away {blasted}, hand back to {hand or 'nothing'}; "
                f"actions after the move {late}, two slot changes in a tick {doubled}, two moves {crowded}; "
                f"your own cart placed {put}, left {kept}")


@test("Cart after a bow shot")
def _():
    # A Flame bow drawn full and shot almost straight up (the arrow comes down about 3 blocks ahead some 5 s later):
    # Auto cart puts a rail and a TNT cart where it will land and the bow goes back in hand. Whether the falling arrow
    # then hits the cart is down to the bow's random spread over 5 s of flight: shown, not required (Cart Auto hit
    # placed checks a cart set off).
    reset()
    cart("auto cart")
    immune()
    slot(0, "bow[enchantments={flame:1}]")
    slot(1, "rail", 16)
    slot(2, "tnt_minecart")
    cmd("item replace entity @s inventory.0 with minecraft:arrow 16")
    ctl("key", "1", "tap")
    ctl("look", "0", "-89")
    wait(5)
    mark = probe_mark()
    only("cart")
    ctl("mouse", "right", "down")
    wait(22)
    ctl("mouse", "right", "up")
    seen = 0
    rails = 0
    for _ in range(12):
        seen = max(seen, tnt_carts())
        rails = count(X0 - 5, Y + 1, Z0 - 5, X0 + 5, Y + 1, Z0 + 5, "rail")
        if seen:
            break
    hand = held().get("item") or ""
    gone = False
    for _ in range(50):
        if seen and tnt_carts() == 0:
            gone = True
            break
    wait(10)
    items = cart_items()
    late, doubled, crowded = cart_order(mark)
    only()
    ok = rails >= 1 and seen >= 1 and "bow" in hand and not late and not doubled and not crowded
    return ok, (f"rail placed {rails}, cart placed {seen >= 1}, set off by the arrow {gone} ({items} cart items left), "
                f"hand {hand or 'nothing'}; actions after the move {late}, two slot changes in a tick {doubled}, "
                f"two moves {crowded}")


@test("Cart Auto hit placed")
def _():
    # A TNT cart on a rail 6 blocks ahead; you hold a Flame bow drawn: Auto hit placed aims an arc at the cart (the
    # server's look only) and lets go once it is drawn full; the burning arrow sets the cart off.
    reset()
    cart("auto cart", on_bow_shot="false")
    immune()
    slot(0, "bow[enchantments={flame:1}]")
    cmd("item replace entity @s inventory.0 with minecraft:arrow 16")
    ctl("key", "1", "tap")
    cmd(f"setblock {X0} {Y+1} {Z0+6} minecraft:rail", f"summon tnt_minecart {X0+0.5} {Y+1.06} {Z0+6.5}")
    wait(5)
    mark = probe_mark()
    only("cart")
    ctl("mouse", "right", "down")
    gone = None
    for t in range(25):
        if tnt_carts() == 0:
            gone = (t + 1) * 3
            break
    ctl("mouse", "right", "up")
    wait(10)
    items = cart_items()
    late, doubled, crowded = cart_order(mark)
    only()
    ok = gone is not None and items == 0 and not late and not doubled and not crowded
    return ok, (f"cart set off after {gone} ticks drawn ({items} cart items left); actions after the move {late}, two "
                f"slot changes in a tick {doubled}, two moves {crowded}")


@test("Cart Xbow")
def _():
    # Xbow key with the crosshair on the floor 4 blocks ahead: a rail and a cart there, a fire on the line to it, then a
    # loaded crossbow fired through the fire: the burning arrow sets the cart off.
    reset()
    cart("xbow", xbow_key="g")
    immune()
    slot(0, "rail", 16)
    slot(1, "tnt_minecart")
    slot(2, "flint_and_steel")
    slot(3, 'crossbow[charged_projectiles=[{id:"minecraft:arrow",count:1}]]')
    slot(4, "dirt", 16)
    ctl("key", "5", "tap")
    ctl("look", "0", "22")
    wait(5)
    mark = probe_mark()
    only("cart")
    ctl("key", "g", "tap")
    seen = fired = 0
    gone = False
    for _ in range(30):
        n = tnt_carts()
        seen = max(seen, n)
        fired = max(fired, count(X0 - 4, Y + 1, Z0, X0 + 4, Y + 2, Z0 + 6, "fire"))
        if seen and n == 0:
            gone = True
            break
    wait(10)
    items = cart_items()
    rails = count(X0 - 4, Y + 1, Z0, X0 + 4, Y + 1, Z0 + 6, "rail")
    hand = held().get("item") or ""
    late, doubled, crowded = cart_order(mark)
    only()
    ok = seen >= 1 and gone and items == 0 and "dirt" in hand and not late and not doubled and not crowded
    return ok, (f"cart placed {seen >= 1}, fire seen {fired >= 1}, set off {gone} ({items} cart items left, rail left "
                f"{rails}), hand back to {hand or 'nothing'}; actions after the move {late}, two slot changes in a tick "
                f"{doubled}, two moves {crowded}")


@test("Cart Cover")
def _():
    # Someone else's TNT cart 3 blocks ahead would hurt you badly: Cover puts obsidian from slot 4 between (Silent,
    # Normal swap) and stops at Max blocks (2).
    reset()
    cart("cover")
    slot(3, "obsidian", 16)
    slot(0, "dirt", 4)
    ctl("key", "1", "tap")
    cmd(f"setblock {X0} {Y+1} {Z0+3} minecraft:rail", f"summon tnt_minecart {X0+0.5} {Y+1.06} {Z0+3.5}")
    wait(5)
    mark = probe_mark()
    only("cart")
    placed = 0
    for _ in range(25):
        placed = count(X0 - 3, Y + 1, Z0 - 2, X0 + 3, Y + 3, Z0 + 5, "obsidian")
        if placed >= 2:
            break
    wait(30)
    placed = count(X0 - 3, Y + 1, Z0 - 2, X0 + 3, Y + 3, Z0 + 5, "obsidian")
    late, doubled, crowded = cart_order(mark)
    only()
    ok = 1 <= placed <= 2 and not late and not doubled and not crowded
    return ok, (f"obsidian placed {placed} (Max blocks 2); actions after the move {late}, two slot changes in a tick "
                f"{doubled}, two moves {crowded}")


@test("Cart Refill")
def _():
    # With the inventory open, slot 9 is filled with TNT carts from the main inventory (one swap click).
    reset()
    cart("refill")
    cmd("item replace entity @s inventory.10 with minecraft:tnt_minecart")
    wait(5)
    only("cart")
    ctl("key", "e", "tap")
    wait(20)
    nine = (ctl("inventory").get("hotbar") or [{}] * 9)[8] or {}
    ctl("key", "esc", "tap")
    wait(3)
    only()
    got = "tnt_minecart" in json.dumps(nine)
    return got, f"slot 9 after the inventory opened: {nine.get('item') or 'empty'}"


@test("Cart Pearls")
def _():
    # Bot1, 42 blocks ahead, throws an ender pearl toward you (it lands about 4 blocks ahead 30 ticks later): Pearls
    # puts a cart near the landing and sets it off with a Flame bow as the pearl comes down; Bot1 lands in the blast.
    # You have Resistance IV and 100 health (the blast is in your reach) and Pearl self damage at its most.
    if not CTL2:
        return None, "needs a second player"
    reset()
    cart("pearls", prefer_xbow="false", pearl_self_damage="30", pearl_min_damage="4", switch_delay="1")
    # Explosions hurt players nothing on Peaceful.
    cmd("difficulty normal", "attribute @s minecraft:max_health base set 100", "effect give @s minecraft:instant_health 1 28 true",
        "effect give @s minecraft:resistance 120 3 true", "effect give @s minecraft:fire_resistance 120 0 true")
    slot(0, "bow[enchantments={flame:1}]")
    slot(1, "rail", 16)
    slot(2, "tnt_minecart")
    slot(4, "dirt", 4)
    cmd("item replace entity @s inventory.0 with minecraft:arrow 16")
    ctl("key", "5", "tap")
    bot_respawned()
    cmd("gamemode survival Bot1", "clear Bot1", "effect clear Bot1", "attribute Bot1 minecraft:max_health base set 100",
        "effect give Bot1 minecraft:instant_health 1 28 true",
        f"fill {X0-1} {Y} {Z0+41} {X0+1} {Y} {Z0+43} minecraft:stone", f"fill {X0-1} {Y+1} {Z0+41} {X0+1} {Y+3} {Z0+43} minecraft:air",
        f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 42.5} 180 -15", "give Bot1 minecraft:ender_pearl 4")
    wait(20)
    ctl2("select", "1")
    wait(5)
    before = bot_health()
    mark = probe_mark()
    only("cart")
    wait(5)
    ctl2("mouse", "right", "down")
    wait(3)
    ctl2("mouse", "right", "up")
    seen = 0
    for _ in range(20):
        seen = max(seen, tnt_carts())
    wait(20)
    lost = round(before - bot_health(), 1)
    left = tnt_carts()
    late, doubled, crowded = cart_order(mark)
    only()
    # What this server deals another player for a TNT cart going off 2 blocks away (single player's server deals its
    # LAN guest nothing).
    cmd(f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 3.5} 180 0", "effect give Bot1 minecraft:instant_health 1 28 true")
    wait(10)
    was = bot_health()
    cmd(f"summon tnt_minecart {X0 + 0.5} {Y + 1} {Z0 + 5.5} {{fuse:1}}")
    wait(20)
    control = round(was - bot_health(), 1)
    bot_done()
    # The pearl's own landing costs 5: the blast is what is past that, where the server deals any. Single player's server
    # deals its LAN guest nothing for this blast (it does for a cart beside it standing; cause not found): shown only.
    hurt = lost > 5 or control == 0 or not SERVER
    ok = seen >= 1 and left == 0 and hurt and not late and not doubled and not crowded
    return ok, (f"a TNT cart going off beside Bot1 costs it {control}; cart placed {seen >= 1}, set off "
                f"{seen >= 1 and left == 0}, Bot1 lost {lost} (5 of it the pearl's); actions after the move "
                f"{late}, two slot changes in a tick {doubled}, two moves {crowded}")


@test("Search finds Auto Weapon as shield breaker")
def _():
    # Shield Breaker is Auto Weapon's Axe on shields now: searching its old name and pressing Enter turns Auto Weapon on.
    only()
    say(".menu shield breaker")
    wait(6)
    ctl("key", "enter", "tap")
    wait(4)
    for _ in range(2):
        ctl("key", "esc", "tap")
        time.sleep(1)
    if state().get("screen") == "paused":
        ctl("key", "esc", "tap")
    found = is_on("Auto Weapon")
    if found:
        on.add("autoweapon")
    only()
    return found, f"Auto Weapon on after searching \"shield breaker\" and Enter: {found}"


@test("Auto Mace (Kill Aura)")
def _():
    # Falling 11 blocks past a pig with 1000 health, Kill Aura holding a sword, a mace in slot 3, Auto Mace's Min fall
    # at 3 and Swap back on: the first hit's damage without and with Auto Mace (a smash adds 4 a block for the first 3
    # fallen, 2 for the next 5, 1 after), and the sword back in hand after it.
    setting("killaura", "entities", "pig")
    setting("automace", "min fall", "3")
    setting("automace", "swap back", "true")
    lost = {}
    for name, mods in (("off", ("killaura",)), ("on", ("killaura", "automace"))):
        reset()
        slot(0, "diamond_sword")
        slot(2, "mace")
        ctl("key", "1", "tap")
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 1.5} {TOUGH_PIG}")
        wait(10)
        only(*mods)
        cmd(f"tp @s {X0 + 0.5} {Y + 12} {Z0 + 0.5} 0 60")
        hit = 0
        for _ in range(60):
            wait(1)
            m = mobs("pig")
            if m and m[0]["health"] < 1000:
                hit = 1000 - m[0]["health"]
                break
        wait(10)
        only()
        lost[name] = (round(hit, 1), (state().get("held") or {}).get("item", ""))
    setting("automace", "min fall", "10")
    setting("automace", "swap back", "false")
    return lost["on"][0] > lost["off"][0] * 1.5 and "sword" in lost["on"][1], (
        f"first hit falling past the pig: {lost['off'][0]} with the sword alone, {lost['on'][0]} with Auto Mace; hand "
        f"after: {lost['on'][1]}")


@test("Auto Mace (Swap back)")
def _():
    # Falling 11 blocks past a pig with 1000 health, a sword in hand and a mace in slot 3, your own clicks at the pig
    # with Criticals on (it may hold the hit for its hop): Auto Mace (Min fall 3, Swap back, no Trigger) puts the mace
    # in for the click, the hit is the mace's smash (far more than the sword's), and the sword is back after it.
    setting("automace", "min fall", "3")
    setting("automace", "swap back", "true")
    setting("automace", "trigger", "false")
    got = {}
    for name, mods in (("off", ("criticals",)), ("on", ("criticals", "automace"))):
        reset()
        slot(0, "diamond_sword")
        slot(2, "mace")
        ctl("key", "1", "tap")
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 1.5} {TOUGH_PIG}")
        wait(10)
        target = str(mobs("pig")[0]["id"])
        only(*mods)
        cmd(f"tp @s {X0 + 0.5} {Y + 12} {Z0 + 0.5} 0 60")
        hit = 0
        for _ in range(60):
            ctl("look-at-entity", target)
            ctl("mouse", "left", "click")
            wait(1)
            m = mobs("pig")
            if m and m[0]["health"] < 1000:
                hit = 1000 - m[0]["health"]
                break
        wait(10)
        only()
        got[name] = (round(hit, 1), held().get("item", "").replace("minecraft:", ""))
    setting("automace", "min fall", "10")
    setting("automace", "swap back", "false")
    setting("automace", "trigger", "true")
    ok = got["on"][0] > got["off"][0] * 1.5 and got["on"][1] == "diamond_sword"
    return ok, f"(first hit, hand after) the sword alone {got['off']}, with Auto Mace {got['on']}"


@test("Auto Weapon (Holding, Back after a delay)")
def _():
    # Kill Aura at a pig with 1000 health, a diamond sword in slot 3, Best weapon alone, holding the item: a wooden
    # sword (Holding lists it: the diamond sword swaps in) and a stick (Holding sword,axe: no swap, a weak hit); then
    # with Back after A delay (Shortest 5, Longest 5), the ticks the diamond sword stays in hand after the hit (Same
    # tick: 1).
    setting("killaura", "entities", "pig")
    setting("killaura", "attack when holding", "All")
    setting("autoweapon", "holding", "sword,axe")
    weapon("best weapon")

    def hit_with(item):
        reset()
        slot(0, item)
        slot(2, "diamond_sword")
        ctl("key", "1", "tap")
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 2.0} {TOUGH_PIG}")
        wait(10)
        only("killaura", "autoweapon")
        hit, held_for = 0, 0
        for _ in range(60):
            wait(1)
            m = mobs("pig")
            if m and m[0]["health"] < 1000:
                hit = 1000 - m[0]["health"]
                break
        while held_for < 20 and "diamond_sword" in held().get("item", ""):
            wait(1)
            held_for += 1
        only()
        return round(hit, 1), held_for, held().get("item", "").replace("minecraft:", "")

    sword, stick = hit_with("wooden_sword"), hit_with("stick")
    setting("autoweapon", "back after", "A delay")
    setting("autoweapon", "shortest delay", "5")
    setting("autoweapon", "longest delay", "5")
    delayed = hit_with("wooden_sword")
    setting("autoweapon", "back after", "Same tick")
    weapon("best weapon", "axe on shields")
    ok = sword[0] > stick[0] * 1.5 and delayed[0] > stick[0] * 1.5 and delayed[1] >= sword[1] + 2 and delayed[2] == "wooden_sword"
    return ok, (f"(first hit, ticks the diamond sword stayed after it, hand after) holding a wooden sword: {sword}; a "
                f"stick: {stick}; a wooden sword, Back after a 5-tick delay: {delayed}")


@test("Auto Spear (Reach poke)")
def _():
    # A pig 4.5 blocks off (past the 3 blocks of reach), a sword in hand and an iron spear in slot 3: your clicks.
    lost = {}
    for name, mods in (("off", ()), ("on", ("autospear",))):
        reset()
        slot(0, "diamond_sword")
        slot(2, "iron_spear")
        ctl("key", "1", "tap")
        if mods:
            setting("autospear", "reach poke", "true")
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
        f"5 clicks at a pig 4.5 blocks off: {lost['off'][0]} health lost without Reach poke, {lost['on'][0]} with Reach "
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


@test("S Tap (Adaptive)")
def _():
    # W Tap's S Tap with Adaptive on and Longest hold 4, as W Tap's test: you sprint at Bot1 with Kill Aura. Each tap
    # ends by itself (the Debug Report's W Tap line says how). In a second run (Reaction 3 ticks) you press S yourself
    # as soon as a hit shows: the tap ends as it starts. After either run, with your keys let go, nothing moves you.
    if not CTL2:
        return None, "needs a second player"
    setting("killaura", "entities", "player")
    setting("wtap", "mode", "S Tap")
    setting("wtap", "adaptive", "true")
    setting("wtap", "longest hold", "4")

    def tap_line():
        _, text = debug_report(lambda: say(".debug 60"))
        return next((l for l in (text or "").splitlines() if l.startswith("- W Tap:")), "no W Tap line")

    def still():
        wait(5)
        a = pos()
        wait(10)
        b = pos()
        return abs(a[0] - b[0]) + abs(a[2] - b[2]) < 0.05

    def run(act=None):
        reset(z=-13.5)
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        bot_ready(-10.5, 180)
        return after_hits(("killaura", "wtap"), 60, lambda: ctl2("state")["player"]["position"][2], bot_health, act=act,
                          ticks_after=3)

    pushes = run()
    stopped = still()
    line = tap_line()
    natural = any(w in line for w in ("spacing reached", "no back needed", "Longest hold"))
    seen = {"health": None, "pressed": None}

    def press_back(t):
        h = bot_health()
        if seen["pressed"] is None and seen["health"] is not None and h < seen["health"] - 0.01:
            # Held to the end of the run: with W and S both held you don't sprint, so no later tap replaces this one.
            ctl("key", "s", "down")
            seen["pressed"] = t
        seen["health"] = h

    setting("wtap", "reaction ticks", "3")
    run(press_back)
    ctl("key", "s", "up")
    stopped_after_back = still()
    back_line = tap_line()
    bot_done()
    ok = len(pushes) >= 3 and natural and stopped and stopped_after_back and "you pressed back" in back_line
    return ok, (f"{len(pushes)} hits {pushes}; adaptive run: {line}; still after: {stopped}; with S pressed: {back_line};"
                f" still after: {stopped_after_back}")


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


@test("Offhand (re-totem after a pop)")
def _():
    # 20 totem pops each way: ticks until the offhand holds a totem again, with totems in the main inventory only (Item
    # Totem: Auto Totem opens the inventory, clicks and closes it, a Totem delay apart; then Silent open, which
    # clicks with no screen), and no kick.
    reset()
    cmd("item replace entity @s weapon.offhand with minecraft:totem_of_undying")
    for i in range(9, 30):
        cmd(f"item replace entity @s inventory.{i - 9} with minecraft:totem_of_undying")
    setting("offhand", "item", "Totem")
    only("offhand")
    kicked = len(kicks)
    wait(10)
    times = {}
    for silent in ("false", "true"):
        setting("offhand", "silent open", silent)
        times[silent] = []
        for _ in range(20):
            cmd("damage @s 1000 minecraft:generic")
            n = 0
            while n < 100:
                wait(1)
                n += 1
                if "totem" in offhand() and n > 2:
                    break
            times[silent].append(n)
            cmd("effect give @s minecraft:instant_health 1 10 true")
            wait(5)
        for i in range(9, 30):
            cmd(f"item replace entity @s inventory.{i - 9} with minecraft:totem_of_undying")
    setting("offhand", "silent open", "false")
    only()
    alive = state()["player"]["health"] > 0
    kicked = len(kicks) - kicked
    ok = alive and kicked == 0 and all(max(t) < 100 for t in times.values())
    shown = lambda t: f"{sorted(t)} ticks (median {sorted(t)[10]})"
    return ok, (f"re-totem after a pop, inventory opened: {shown(times['false'])}; Silent open: {shown(times['true'])}; "
                f"alive after 40 pops: {alive}, kicks: {kicked}")


def slot_centre(slot):
    """The GUI point at the middle of inventory `slot` (9-35 main, 0-8 hotbar) with the inventory open, found by
    picking up the dirt this puts in slot 35 at each GUI scale until one takes it, then putting it back. An open recipe
    book (a server can keep it open) moves the inventory 77 to the right."""
    d = ctl("status")["display"]
    cmd("item replace entity @s inventory.26 with minecraft:dirt")
    wait(5)
    book = 77 if (ctl("screen").get("recipe_book") or {}).get("open") else 0
    pos = lambda i, w, h: ((w - 176) // 2 + book + 8 + (i - 9) % 9 * 18 + 8, (h - 166) // 2 + 84 + (i - 9) // 9 * 18 + 8) if i >= 9 \
        else ((w - 176) // 2 + book + 8 + i * 18 + 8, (h - 166) // 2 + 142 + 8)
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


@test("Offhand (Hover)")
def _():
    # A totem in the main inventory (slot 14) and the offhand empty: with the inventory open, the pointer is moved over
    # the totem (a mouse wheel turn of 0 steps there, so nothing is clicked). Offhand with Item None, without and with
    # Hover (Rust's Hover Totem): off it stays put; on it goes to the offhand.
    setting("offhand", "item", "None")
    got = {}
    for name, value in (("off", "false"), ("on", "true")):
        setting("offhand", "hover", value)
        reset()
        cmd("item replace entity @s inventory.5 with minecraft:totem_of_undying")
        only("offhand")
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


@test("Fast Use (Throwables) and Key Throw")
def _():
    # Bottles thrown in 2 s of holding use, without and with Fast Use's Throwables (FastXP); pearls thrown by 5 presses
    # of Key Throw's key, the hand back on its slot after.
    setting("fastuse", "throwables", "true")
    used = {}
    for name, mods in (("off", ()), ("on", ("fastuse",))):
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
        f"bottles in 2 s: {used['off']} without Throwables, {used['on']} with it; Key Throw: {16 - left} pearls for 5 presses, hand "
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
    # You strafe left and right in front of Bot1 for 3 s, without and with Fake Lag (Delay, 300 ms). From both
    # clients' probes: your x as you see it and as Bot1 sees you, and the shift in time that fits them best (how late
    # the server and Bot1 have you). The wire probe: never two move packets in a client tick, no kick.
    if not CTL2:
        return None, "needs a second player"
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    bot_ready(4.5, 180)
    me = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
    setting("fakelag", "mode", "Delay")
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


@test("Server Scan (Passive sends nothing)")
def _():
    # Passive only reads what the server sends: with .scan and .scan run typed, no command suggestion request (15, the
    # only packet Server Scan sends) and no Query. Extended's .scan run asks the server at most once and sends Query only
    # to the address joined (none in single player).
    if not probe_path():
        return None, "needs the probe (tools/probe)"

    def window(*commands):
        mark, said = probe_mark(), log_mark()
        for c in commands:
            say(c)
            wait(10)
        wait(40)
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            ids = [int(line.split()[1]) for line in f if line.startswith("pkt ")]
        return ids, sunset_said(said)

    only()
    off, _ = window(".scan")
    only("serverscan")
    setting("serverscan", "mode", "Passive")
    passive, found = window(".scan", ".scan run")
    setting("serverscan", "mode", "Extended")
    extended, ran = window(".scan run")
    only()
    # Server Scan's only packet is a command suggestion request (15); the rest (movement, keep-alives) come and go.
    extra = sorted(set(passive) - set(off))
    target = next((line for line in ran if "Query to" in line), "")
    queried = any("Query to" in line for line in found)
    ok = 15 not in passive and not queried and extended.count(15) <= 1 and (not SERVER or SERVER in target)
    return ok, (f"Passive: Query sent {queried}, packet kinds beyond the module off {extra}, suggestion requests {passive.count(15)}, "
                f"{len(found)} lines ({found[1] if len(found) > 1 else ''!r}); Extended run: suggestion requests "
                f"{extended.count(15)}, {target!r}")


@test("Server Scan (command tree)")
def _():
    # Server Scan reads the command tree the server sent (taken where Horizon builds the chat's tree from it, since
    # 66f3981 keeps only that merged tree): the roots, /weather's subcommands, an unknown root, and /studio, which
    # Horizon adds to the chat's tree on the client, never counts as one of the server's commands.
    def ask(c):
        said = log_mark()
        say(c)
        wait(20)
        return sunset_said(said)

    only("serverscan")
    setting("serverscan", "mode", "Passive")
    wait(20)
    roots = ask(".scan commands")
    sub = ask(".scan commands weather")
    nope = ask(".scan commands sunsetnope")
    studio = ask(".scan commands studio")
    only()
    off = ask(".scan commands")
    seen = any("Server Scan — Commands (" in l and "visible" in l for l in roots)
    ok = (seen and any("/weather clear" in l for l in sub) and any("no visible command /sunsetnope" in l for l in nope)
          and any("no visible command /studio" in l for l in studio))
    return ok, f"roots {roots[:2]!r}; weather {sub[:3]!r}; unknown {nope[:1]!r}; studio {studio[:1]!r}; module off {off[:1]!r}"


@test("Streamer Mode")
def _():
    # A visual: with Streamer Mode on, your name shows as "You" in chat (a /say of yours, which names you twice), in
    # the tab list (Tab held) and over an armor stand named after you; nowhere your own name (a screenshot).
    reset()
    me = ctl("status").get("name") or "Tester"
    only("streamermode")
    wait(10)
    cmd(f"say hi, {me} here", f"summon armor_stand {X0 + 0.5} {Y + 1} {Z0 + 2.5} "
        f"{{CustomName:\"{me}\",CustomNameVisible:1b,NoGravity:1b}}")
    ctl("look-at", str(X0 + 0.5), str(Y + 2.5), str(Z0 + 2.5))
    wait(10)
    ctl("key", "tab", "down")
    wait(5)
    path = f"{SHOTS}/streamer-mode.png"
    ctl("screenshot", path)
    wait(2)
    ctl("key", "tab", "up")
    only()
    cmd("kill @e[type=minecraft:armor_stand]")
    say(".reset streamermode")
    return None, f"look at {path}: 'You' in chat, the tab list and over the armor stand, and no '{me}'"


@test("Client Spoofer")
def _():
    # Through a relay that reads what reaches the server: the client brand with the module off (Horizon's own), with
    # Client Fabric, with Custom, with LabyMod (and its VersionLoginPacket on labymod:neo once in), and with Per server
    # naming NeoForge for the relay's address over Client Lunar Client; Diagnostics says what went out.
    if not SERVER:
        return None, "servers only"
    tap = BrandTap()
    relay = f"127.0.0.1:{tap.port}"
    got, said, laby = {}, [], False
    try:
        for name, mods, settings in (
                ("off", (), ()),
                ("Fabric", ("clientspoofer",), (("client", "Fabric"),)),
                ("Custom", ("clientspoofer",), (("client", "Custom"), ("custom brand", "Test Brand 1"))),
                ("LabyMod", ("clientspoofer",), (("client", "LabyMod"), ("labymod version", "4.2.60"))),
                ("Per server", ("clientspoofer",), (("client", "Lunar Client"), ("per server", "127.0.0.1=NeoForge")))):
            only(*mods)
            for k, v in settings:
                setting("clientspoofer", k, v)
            before, mark = len(tap.brands), log_mark()
            join(relay)
            got[name] = tap.brands[before] if len(tap.brands) > before else None
            if name == "Fabric":
                said = [line for line in sunset_said(mark) if "Client Spoofer" in line]
            if name == "LabyMod":
                laby = b"\x0blabymod:neo\x00\x064.2.60" in bytes(tap.streams[-1])
    finally:
        only()
        join(SERVER)
    want = {"off": "horizon", "Fabric": "fabric", "Custom": "Test Brand 1", "LabyMod": "labymod", "Per server": "neoforge"}
    ok = got == want and laby and any('"fabric" (Fabric' in line for line in said)
    return ok, f"brands the server got: {got}; LabyMod's version packet: {laby}; Diagnostics: {said[:1]}"


@test("Velocity (knockback delay)")
def _():
    # Bot1 hits you 5 times off and 5 times on (Chance 100%, ground delay 400 ms); from your probe, the time from its
    # click to the first frame you move: how late the knockback lands.
    if not CTL2:
        return None, "needs a second player"
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("velocity", "knockback delay", "true")
    setting("velocity", "delay chance", "100")
    setting("velocity", "ground delay min", "400")
    setting("velocity", "ground delay max", "400")
    for name in ("knockback", "explosions", "liquids", "entity push", "sinking"):
        setting("velocity", name, "false")
    reset()
    cmd("effect give @s minecraft:resistance 600 4 true")
    bot_ready(2.5, 180)
    tester = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
    late = {}
    for name, mods in (("off", ()), ("on", ("velocity",))):
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


@test("PvP Bot (Sword)")
def _():
    # Bot1 (20 health, no armour) 8 blocks away, 3 times standing still and 3 times strafing left and right; you have
    # a diamond sword and PvP Bot on in Sword mode: it has to kill Bot1 within 20 s. After each fight, once it is off,
    # you must stand still (no key left held).
    if not CTL2:
        return None, "needs a second player"
    setting("pvpbot", "mode", "Sword")
    won, stuck = {"still": [], "strafing": []}, 0
    for name in ("still", "still", "still", "strafing", "strafing", "strafing"):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        bot_respawned()
        cmd("gamemode survival Bot1", "clear Bot1", "effect clear Bot1", "effect give Bot1 minecraft:instant_health 1 10 true",
            f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 8.5} 180 0")
        wait(10)
        only("pvpbot")
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


# The modules PvP Bot's Crystal mode drives.
BOT_HANDS = ("Crystal Aura", "Anchor Aura", "Offhand", "Auto Gap", "Surround", "Self Trap", "Auto City", "Key Throw")


def crystal_bot_arena(dist=7.0, bot_hp=1000, bot_totem=False, kit=True):
    """Obsidian floor, you in Blast Protection IV, Bot1 `dist` blocks south (healed, no knockback), and a crystal kit
    in the hotbar: crystals, obsidian, anchors, glowstone, golden apples, pearls, a sword held, totems in the
    inventory. Crystal mode at its defaults, Learn rivals on."""
    reset()
    blast_arena(bot=False)
    bot_respawned()
    cmd("gamemode survival Bot1", "clear Bot1", "effect clear Bot1", f"attribute Bot1 minecraft:max_health base set {bot_hp}",
        "attribute Bot1 minecraft:knockback_resistance base set 1", "attribute Bot1 minecraft:explosion_knockback_resistance base set 1",
        f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + dist + 0.5} 180 0", "effect give Bot1 minecraft:instant_health 1 28 true")
    if bot_totem:
        cmd("item replace entity Bot1 weapon.offhand with minecraft:totem_of_undying")
    ctl2("release")
    if kit:
        for i, (item, n) in enumerate([("end_crystal", 64), ("obsidian", 64), ("respawn_anchor", 16), ("glowstone", 64),
                                       ("golden_apple", 16), ("ender_pearl", 4)]):
            slot(i, item, n)
        for i in range(4):
            cmd(f"item replace entity @s inventory.{i} with minecraft:totem_of_undying")
    slot(8, "diamond_sword")
    ctl("key", "9", "tap")
    for name, value in (("mode", "Crystal"), ("crystal range", "4.5"), ("use anchors", "true"), ("hole up", "true"),
                        ("self trap below hp", "12"), ("city", "true"), ("pearl below hp", "8"), ("learn rivals", "true")):
        setting("pvpbot", name, value)
    wait(10)


def crystal_control():
    """hand_crystal(), once more after a wait when it reads nothing (Bot1 can still be in its respawn grace)."""
    control = hand_crystal()
    if not control:
        wait(60)
        control = hand_crystal()
    return control


def rival_files():
    """The rival history files PvP Bot keeps: {name: text}."""
    out = {}
    for r, d, fs in os.walk(os.path.join(session_dir(), "home")):
        if os.path.basename(r) == "rivals" and "sunset" in r:
            for f in fs:
                with open(os.path.join(r, f)) as fh:
                    out[f] = fh.read()
    return out


def clear_rivals():
    for r, d, fs in os.walk(os.path.join(session_dir(), "home")):
        if os.path.basename(r) == "rivals" and "sunset" in r:
            for f in fs:
                os.remove(os.path.join(r, f))


def saved_config():
    """Sunset's saved config, {key: value}."""
    for r, d, fs in os.walk(os.path.join(session_dir(), "home")):
        if r.endswith(os.path.join(".horizon-mod-data", "sunset")) and "config" in fs:
            with open(os.path.join(r, "config")) as f:
                return dict(l.split("=", 1) for l in f.read().splitlines() if "=" in l)
    return {}


def bot_hands_on():
    return sorted(set(modules_on()) & set(BOT_HANDS))


@test("PvP Bot (Crystal)")
def _():
    # Bot1 (1000 health) 7 blocks south, you with the crystal kit and PvP Bot on in Crystal mode for 15 s: it walks
    # into crystal range, its modules are on 2.5 s in, Crystal Aura and Anchor Aura deal damage (where the server
    # deals another player's blasts any), you live, one move packet a tick at most, and the config saved while it runs has the modules it turned on
    # saved as off. Off: none of them stays on, no key stays held. Then with Crystal Aura on by hand first: the bot
    # leaves it on when it goes off.
    if not CTL2:
        return None, "needs a second player"
    reset()
    blast_arena()
    control = crystal_control()
    crystal_bot_arena()
    mark = probe_mark() if SERVER and probe_path() else None
    bot0, own0 = bot_health(), own_health()
    only("pvpbot")
    closest, cfg, during = 99.0, {}, []
    for step in range(30):
        wait(10)
        b = (ctl2("state").get("player") or {}).get("position")
        if b:
            closest = min(closest, flat(pos(), [b["x"], b["y"], b["z"]] if isinstance(b, dict) else b))
        if step == 5:
            during = bot_hands_on()
        if step == 20:
            cfg = saved_config()
    dealt = bot0 - bot_health()
    st = state()
    alive = st.get("screen") != "dead" and bool(st.get("player"))
    most = wire_stats(mark)[1] if mark is not None else None
    only()
    wait(10)
    after = bot_hands_on()
    a = pos()
    wait(20)
    still = math.dist(a, pos()) < 0.1
    # Crystal Aura yours before the bot.
    ensure_alive()
    crystal_bot_arena()
    toggle("crystalaura", True)
    toggle("pvpbot", True)
    wait(60)
    toggle("pvpbot", False)
    wait(10)
    kept = is_on("Crystal Aura")
    only()
    blast_done()
    bot_done()
    saved_off = cfg.get("crystalaura.on") == "false" and cfg.get("offhand.on") == "false" and cfg.get("pvpbot.on") == "true"
    hit = dealt > 0 or control == 0
    ok = (hit and closest <= 5.5 and alive and most in (None, 0, 1) and "Crystal Aura" in during and after == []
          and still and kept and saved_off)
    return ok, (f"a crystal broken by hand beside Bot1 costs it {control}; Bot1 took {dealt:.0f}, you "
                f"{own0 - (own_health() if alive else 0):.0f}, alive {alive}; closest {closest:.1f} blocks; on while it ran "
                f"{during}, after {after}; standing still after {still}; most move packets in a tick {most}; config "
                f"while running: crystalaura.on={cfg.get('crystalaura.on')} offhand.on={cfg.get('offhand.on')} "
                f"pvpbot.on={cfg.get('pvpbot.on')}; Crystal Aura on by hand kept on after: {kept}")


@test("PvP Bot (Crystal) pops and kill")
def _():
    # Bot1 at 20 health, no armour, a totem in its offhand, 5 blocks south: the bot has to pop the totem and kill it
    # within 20 s; once Bot1 is dead the bot lets go (none of its modules on, no key held) though it stays on, and the
    # rival history file for Bot1 has the fight and the pop. Then Learn rivals off, another fight: no file changes.
    if not CTL2:
        return None, "needs a second player"
    reset()
    blast_arena()
    control = crystal_control()
    if control == 0:
        blast_done()
        bot_done()
        return None, "the server deals no damage for another player's crystal (Horizon's own)"
    clear_rivals()
    crystal_bot_arena(dist=5.0, bot_hp=20, bot_totem=True)
    only("pvpbot")
    popped = dead = False
    took = None
    start = time.time()
    for _ in range(100):
        wait(4)
        bst = ctl2("state")
        if bst.get("screen") == "dead" or bot_health() <= 0:
            dead = True
            took = round(time.time() - start, 1)
            break
        if (ctl2("inventory").get("offhand") or {}).get("item") != "minecraft:totem_of_undying":
            popped = True
    wait(30)
    after = bot_hands_on()
    a = pos()
    wait(20)
    still = math.dist(a, pos()) < 0.1
    still_on = is_on("PvP Bot")
    only()
    wait(10)
    files = rival_files()
    record = json.loads(next(iter(files.values()))) if len(files) == 1 else {}
    # Learn rivals off.
    setting("pvpbot", "learn rivals", "false")
    crystal_bot_arena(dist=5.0, bot_hp=1000)
    setting("pvpbot", "learn rivals", "false")
    only("pvpbot")
    wait(100)
    only()
    wait(10)
    unchanged = rival_files() == files
    setting("pvpbot", "learn rivals", "true")
    blast_done()
    bot_done()
    ok = (popped or dead) and dead and after == [] and still and still_on and record.get("fights", 0) >= 1 \
        and record.get("pops", 0) >= 1 and unchanged
    return ok, (f"totem popped {popped or dead}, Bot1 dead {dead} after {took} s; after it died: modules on {after}, "
                f"standing still {still}, bot still on {still_on}; rival files {len(files)}: {record}; Learn rivals off "
                f"left them unchanged: {unchanged}")


def pearls():
    """Ender pearls anywhere in the inventory."""
    return ctl("inventory").get("counts", {}).get("minecraft:ender_pearl", 0)


@test("PvP Bot (Crystal) hole, trap and pearl")
def _():
    # (1) A hole dug in the floor 2 blocks east, Bot1 4 blocks south: Hole up walks you into it; off, you stay put.
    # (2) Standing with Self trap below HP at 36 and obsidian: Surround puts obsidian round your feet and Self Trap
    # over your head; at 0, nothing over your head. (3) 10 health, no totem, pearls, Pearl below HP 12: one pearl thrown
    # in 7 s, you end farther from Bot1 and alive (it lands clear of walls); with a totem in the inventory no pearl goes.
    if not CTL2:
        return None, "needs a second player"
    out = {}
    for hole_up in (True, False):
        crystal_bot_arena(dist=4.0, kit=False)
        cmd(f"setblock {X0 + 2} {Y} {Z0} minecraft:air")
        setting("pvpbot", "hole up", str(hole_up).lower())
        only("pvpbot")
        wait(80)
        p = pos()
        only()
        out[f"hole {hole_up}"] = [math.floor(p[0]) - X0, math.floor(p[1] + 0.01) - Y, math.floor(p[2]) - Z0]
    setting("pvpbot", "hole up", "true")
    for below in (36, 0):
        crystal_bot_arena(dist=4.0, kit=False)
        slot(1, "obsidian", 64)
        setting("pvpbot", "self trap below hp", str(below))
        only("pvpbot")
        wait(80)
        only()
        f = [math.floor(v) for v in pos()]
        is_obs = lambda x, y, z: ctl("block", str(x), str(y), str(z)).get("block") == "minecraft:obsidian"
        sides = sum(is_obs(f[0] + dx, f[1], f[2] + dz) for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)))
        out[f"trap {below}"] = (sides, is_obs(f[0], f[1] + 2, f[2]))
        cmd(f"fill {X0-5} {Y+1} {Z0-4} {X0+5} {Y+4} {Z0+8} minecraft:air")
    setting("pvpbot", "self trap below hp", "12")
    for totem in (False, True):
        crystal_bot_arena(dist=4.0, kit=False)
        slot(5, "ender_pearl", 4)
        if totem:
            cmd("item replace entity @s inventory.0 with minecraft:totem_of_undying")
        cmd("attribute @s minecraft:max_health base set 10")
        setting("pvpbot", "pearl below hp", "12")
        wait(10)
        b0 = ctl2("state").get("player", {}).get("position")
        d0 = flat(pos(), [b0["x"], b0["y"], b0["z"]] if isinstance(b0, dict) else b0) if b0 else None
        only("pvpbot")
        timeline = []
        for _ in range(10):
            wait(10)
            timeline.append(pearls())
        only()
        wait(40)
        out[f"timeline {totem}"] = timeline
        left = sum(s.get("count", 0) for s in (ctl("inventory").get("hotbar") or []) if s and "ender_pearl" in (s.get("item") or ""))
        b1 = ctl2("state").get("player", {}).get("position")
        d1 = flat(pos(), [b1["x"], b1["y"], b1["z"]] if isinstance(b1, dict) else b1) if b1 else None
        out[f"pearl totem={totem}"] = (left, d0 and round(d0, 1), d1 and round(d1, 1), state().get("screen") != "dead")
        cmd("attribute @s minecraft:max_health base reset")
    setting("pvpbot", "pearl below hp", "8")
    blast_done()
    bot_done()
    away = out["pearl totem=False"]
    ok = (out["hole True"] == [2, 0, 0] and out["hole False"] != [2, 0, 0] and out["trap 36"][0] >= 3
          and out["trap 36"][1] and not out["trap 0"][1] and away[0] == 3 and away[1] is not None
          and away[2] is not None and away[2] >= away[1] + 3 and away[3] and out["pearl totem=True"][0] == 4
          and out["pearl totem=True"][1] is not None and out["pearl totem=True"][1] < 6)
    return ok, (f"feet after 4 s, Hole up on/off (hole at [2, 0, 0]): {out['hole True']} / {out['hole False']}; obsidian "
                f"sides and over the head at Self trap 36 / 0: {out['trap 36']} / {out['trap 0']}; pearls left and blocks "
                f"from Bot1 before/after, no totem: {away} (pearls each 10 ticks {out['timeline False']}), with a totem: "
                f"{out['pearl totem=True']}")


def say2(text):
    """A line typed into Bot1's chat box (its own Sunset's . commands)."""
    ctl2("key", "t", "tap")
    wait(1)
    ctl2("type", text)
    ctl2("key", "enter", "tap")
    wait(1)


@test("PvP Bot (Crystal) survives a crystal fight")
def _():
    # Bot1 (1000 health, Resistance IV so it lasts) 5 blocks south fights back with its own Crystal Aura; you at 20 health in
    # Blast Protection IV with 4 totems in the inventory and golden apples, PvP Bot in Crystal mode for 20 s: you are
    # alive at the end (Offhand re-totems, Auto Gap eats), totems popped counted. Where the server deals no damage for
    # another player's crystal (Horizon's own, single player) it only checks you are alive.
    if not CTL2:
        return None, "needs a second player"
    reset()
    blast_arena()
    control = crystal_control()
    crystal_bot_arena(dist=5.0)
    cmd("item replace entity Bot1 hotbar.0 with minecraft:end_crystal 64", "item replace entity Bot1 hotbar.1 with minecraft:obsidian 64",
        "effect give Bot1 minecraft:resistance 60 3 true")
    say2(".t crystalaura")
    only("pvpbot")
    alive_for, lowest = 0, 99.0
    for _ in range(40):
        wait(10)
        st = state()
        if st.get("screen") == "dead" or not st.get("player"):
            break
        lowest = min(lowest, own_health())
        alive_for += 0.5
    only()
    say2(".t crystalaura")
    wait(10)
    used = 64 - ctl2("inventory").get("counts", {}).get("minecraft:end_crystal", 0)
    bot_alive = ctl2("state").get("screen") != "dead"
    counts = ctl("inventory").get("counts", {})
    left = counts.get("minecraft:totem_of_undying", 0)
    alive = state().get("screen") != "dead"
    ensure_alive()
    blast_done()
    bot_done()
    return alive and alive_for >= 20, (f"a crystal broken by hand beside Bot1 costs it {control}; alive {alive_for} of 20 s, "
                                       f"totems left {left} of 4 (popped {4 - left}), lowest health {lowest:.1f}; Bot1 placed {used} crystals, "
                                       f"alive at the end {bot_alive}")


@test("PvP Bot old config converts")
def _():
    # A profile from before the Crystal mode, with Sword PvP Bot by its old name: imported and loaded, PvP Bot is on in
    # Sword mode with its Style.
    found = [os.path.join(r, f) for r, d, fs in os.walk(os.path.join(session_dir(), "home")) for f in d if f == "imports"]
    if not found:
        say(".profile list")
        found = [os.path.join(r, f) for r, d, fs in os.walk(os.path.join(session_dir(), "home")) for f in d if f == "imports"]
    if not found:
        return False, "no imports folder"
    with open(os.path.join(found[0], "oldbot.profile"), "w") as f:
        f.write("sunset-profile=1\nswordpvpbot.on=true\nswordpvpbot.style=Aggressive\nswordpvpbot.fightrange=12\n")
    say(".profile import oldbot")
    loaded = said_after(".profile load oldbot")
    bot = said_after(".settings pvpbot")
    on = "PvP Bot" in modules_on()
    say(".menu pvp bot")
    wait(6)
    ctl("screenshot", f"{SHOTS}/pvp_bot_menu.png")
    time.sleep(2)
    close_menu()
    say(".profile load before-load")
    say(".profile delete oldbot")
    touched.add("pvpbot")
    ok = any("Loaded" in l for l in loaded) and on and "Mode: Sword" in bot and "Style: Aggressive" in bot and "Fight range: 12" in bot
    return ok, f"load {loaded[:1]}; on {on}; settings {[l for l in bot if l.split(':')[0] in ('Mode', 'Style', 'Fight range')]}"


@test("Key Throw (Hotbar keys)")
def _():
    # Hotbar Quick use: holding a sword, 3 presses each of the pearls' hotbar key (5) and the wind charges' (6) with
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


@test("Fast Use (Fast throw)")
def _():
    # Fast throw through Fast Use's Throwables: wind charges (10-tick cooldown, the game retries use every 4
    # ticks) held on use for 3 s, and a stack of 16 snowballs for 1.5 s, without and with it (Snowballs and eggs ticked).
    setting("fastuse", "throwables", "true")
    setting("fastuse", "snowballs and eggs", "true")
    used = {}
    for item, n, ticks in (("wind_charge", 64, 60), ("snowball", 16, 30)):
        for name, mods in (("off", ()), ("on", ("fastuse",))):
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
    ok = used["wind_charge", "on"] > used["wind_charge", "off"] and used["snowball", "on"] > used["snowball", "off"]
    return ok, "thrown in 3 s, off/on: " + ", ".join(f"{i} {used[i, 'off']}/{used[i, 'on']}" for i in ("wind_charge", "snowball"))


def xp_total():
    p = state().get("player") or {}
    return (p.get("xp") or state().get("xp") or {}).get("total", 0)


def packets_a_tick(mark):
    """From the probe since `mark`: the most packets the client sent in one tick (its own, not single player's server)."""
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        lines = [line.split() for line in f if line.startswith(("pkt ", "order tickend"))]
    most = now = 0
    for w in lines:
        if w[0] == "order":
            most, now = max(most, now), 0
        else:
            now += 1
    return most


def vehicle_moves_a_tick(mark):
    """From the probe since `mark`: the most MoveVehicle packets (serverbound id 34) the client sent in one tick."""
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        lines = [line.split() for line in f if line.startswith(("pkt 34", "order tickend"))]
    most = now = 0
    for w in lines:
        if w[0] == "order":
            most, now = max(most, now), 0
        else:
            now += 1
    return max(most, now)


@test("Entity Control (boat)")
def _():
    # An oak boat on the platform, ridden with a right click, driven with W for 1 s: blocks a second without Entity
    # Control, with Speed 10 and 20 (where you look, east; each from the middle so it stays on the platform). Fly: Jump for 1 s lifts it (Vertical speed 6 = 0.3 a
    # tick), then 6 s with no key holds the height (Anti fly kick sinks 0.0313 every 40 ticks; vanilla kicks a vehicle
    # floating for 80), Sprint brings it down,
    # and landing costs no health. A rubber band (the server putting the boat back) shows as lost distance or height.
    # The probe: one MoveVehicle a tick at most; no kick.
    kicked = len(kicks)
    mark = probe_mark()

    def boat():
        reset("survival")
        cmd("gamerule naturalRegeneration false", f"summon oak_boat {X0+0.5} {Y+1} {Z0+0.5}")
        wait(10)
        b = next(o for o in ctl("status")["objects"] if o["kind"].endswith("boat"))
        x, y, z = b["position"]
        ctl("look-at", str(x), str(y + 0.3), str(z))
        ctl("mouse", "right", "click")
        wait(10)
        ctl("look", "-90", "0")
        wait(2)
        return ctl("status").get("riding") is not None

    def speed(ticks=20):
        a = pos()
        held_keys(["w"], ticks)
        b = pos()
        return flat(a, b) * 20 / ticks

    def leave():
        ctl("key", "shift", "down")
        wait(5)
        ctl("key", "shift", "up")
        cmd("kill @e[type=!player]")
        wait(5)

    rode = boat()
    plain = speed()
    leave()
    setting("entitycontrol", "horizontal speed", "10")
    toggle("entity control", True)
    rode = boat() and rode
    ten = speed()
    leave()
    setting("entitycontrol", "horizontal speed", "20")
    rode = boat() and rode
    twenty = speed()
    leave()
    setting("entitycontrol", "fly", "true")
    rode = boat() and rode
    y0 = pos()[1]
    held_keys(["space"], 20)
    y1 = pos()[1]
    wait(120)
    y2 = pos()[1]
    before = hp()
    held_keys(["ctrl"], int((y2 - y0) / 0.3) + 20)
    wait(20)
    y3 = pos()[1]
    lost = health_lost(before)
    most = vehicle_moves_a_tick(mark)
    leave()
    only()
    cmd("gamerule naturalRegeneration true")
    ok = (rode and plain < 4 and 8 < ten < 11 and 16 < twenty < 21 and y1 - y0 > 4.5 and -0.2 < y2 - y1 <= 0.01
          and y3 - y0 < 0.6 and lost == 0 and most == 1 and len(kicks) == kicked)
    return ok, (f"blocks a second {plain:.1f} off, {ten:.1f} at Speed 10, {twenty:.1f} at 20; Fly: up {y1 - y0:.2f} in 1 s, "
                f"held {y2 - y1:+.3f} over 6 s, down to {y3 - y0:+.2f}, {lost:g} health lost landing; most MoveVehicle in "
                f"a tick {most}; kicks {len(kicks) - kicked}")


@test("Fast Use (Throw modes)")
def _():
    # 64 experience bottles held on use for 1 s looking down: thrown, and experience gained 3 s later, without Fast
    # Use and with Throwables in each Throw mode (Burst 3 and 10). The probe: the most packets sent in a tick (Burst
    # keeps a tick to 15); no kick.
    setting("fastuse", "throwables", "true")
    kicked = len(kicks)
    runs = [("off", None, None), ("Delay 2", "Delay", None), ("Every tick", "Every tick", None), ("Burst 3", "Burst", "3"), ("Burst 10", "Burst", "10")]
    got = {}
    for name, mode, burst in runs:
        reset()
        cmd("xp set @s 0 levels", "xp set @s 0 points")
        slot(0, "experience_bottle", 64)
        ctl("key", "1", "tap")
        ctl("look", "0", "70")
        if mode:
            setting("fastuse", "throw mode", mode)
        if burst:
            setting("fastuse", "burst", burst)
        only(*(("fastuse",) if mode else ()))
        mark = probe_mark() if probe_path() else None
        ctl("mouse", "right", "down")
        wait(20)
        ctl("mouse", "right", "up")
        only()
        thrown = 64 - held().get("count", 0)
        wait(60)
        got[name] = (thrown, xp_total(), packets_a_tick(mark) if mark is not None else None)
    rates = [got[n][0] for n, _, _ in runs]
    most = max((g[2] or 0) for g in got.values())
    # Paper takes at most about 16 uses a second however they come (13 to 16 measured), the rest about 20 a tick.
    ok = (rates[0] < rates[1] < rates[2] <= rates[3] <= rates[4] and got["Every tick"][0] >= 12 and most <= 16
          and len(kicks) == kicked and got["Burst 10"][1] > got["off"][1])
    return ok, "(bottles thrown in 1 s, experience 3 s later, most packets in a tick) " + ", ".join(f"{n} {got[n]}" for n, _, _ in runs)


@test("Fast Use (settings from before 0.9)")
def _():
    # A profile from before Fast Use took in FastXP: FastXP on (Delay 1, Off hand off, bound to J), Fast Use off.
    shown = old_profile("verifyxp", ["fastxp.on=true", "fastxp.key=key.keyboard.j", "fastxp.delay=1", "fastxp.offhand=false",
                                     "fastuse.on=false", "fastuse.key=none", "fastuse.mode=All"], "fastuse")
    if shown is None:
        return False, "no profile file written"
    want = ["Mode: Some", "Items: (empty)", "Throwables: ON", "Throw delay: 3", "Off hand: OFF"]
    on = any("Fast Use*" in line for line in shown)
    bound = any(line.startswith("Fast Use: J") for line in shown)
    got = [w for w in want if any(line.startswith(w) for line in shown)]
    return on and bound and got == want, f"Fast Use on: {on}, bound to J: {bound}, settings {got} of {want}"


@test("Packet Mine")
def _():
    # Each block 2 in front at head height: ticks to break it holding the right tool and the button, then with Packet
    # Mine holding a sword: one click, then looking away and walking back half a block; the block's ticks until gone and
    # the item in hand after (the sword: the tool is held only for the STOP tick). Then two blocks queued by two clicks.
    # No kick; one move packet a tick (probe, on a server).
    tools = {"stone": ("diamond_pickaxe", 2), "deepslate": ("diamond_pickaxe", 2), "oak_log": ("stone_axe", 3), "dirt": ("wooden_shovel", 4)}
    kicked = len(kicks)
    at = (X0, Y + 2, Z0 + 2)

    def setup():
        reset()
        slot(0, "diamond_sword")
        for item, i in set(tools.values()):
            slot(i, item)
        ctl("key", "1", "tap")

    def gone(block, limit):
        n = 0
        while n < limit and ctl("block", *map(str, at)).get("block") == f"minecraft:{block}":
            wait(1)
            n += 1
        return n

    got = {}
    for block, (tool, i) in tools.items():
        setup()
        cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:{block}")
        ctl("look-at", str(at[0] + 0.5), str(at[1] + 0.5), str(at[2] + 0.5))
        ctl("key", str(i + 1), "tap")
        wait(2)
        ctl("mouse", "left", "down")
        plain = gone(block, 100)
        ctl("mouse", "left", "up")
        setup()
        cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:{block}")
        ctl("look-at", str(at[0] + 0.5), str(at[1] + 0.5), str(at[2] + 0.5))
        only("packetmine")
        mark = probe_mark() if probe_path() and SERVER else None
        ctl("mouse", "left", "click")
        wait(2)
        ctl("look", "90", "0")
        ctl("key", "s", "down")
        wait(3)
        ctl("key", "s", "up")
        mined = gone(block, 100)
        wait(3)
        hand = held().get("item", "")
        only()
        got[block] = (plain, mined, hand.replace("minecraft:", ""), wire_stats(mark)[1] if mark is not None else None)
    # Two blocks queued: both go, one after the other.
    setup()
    cmd(f"setblock {at[0]} {at[1]} {at[2]} minecraft:stone", f"setblock {at[0] + 1} {at[1]} {at[2]} minecraft:stone")
    only("packetmine")
    for dx in (0, 1):
        ctl("look-at", str(at[0] + dx + 0.5), str(at[1] + 0.5), str(at[2] + 0.5))
        ctl("mouse", "left", "click")
        wait(1)
    wait(40)
    queued = 2 - count(at[0], at[1], at[2], at[0] + 1, at[1], at[2], "stone")
    only()
    ok = (all(0 < mined < 100 and mined <= plain + 1 and "sword" in hand and (most or 0) <= 1 for plain, mined, hand, most in got.values())
          and queued == 2 and len(kicks) == kicked)
    return ok, ("(ticks plain with the tool, ticks with Packet Mine holding a sword, in hand after, most moves a tick) "
                f"{got}; two clicks queued, broken: {queued}")


@test("Unlock All Recipes")
def _():
    # Four oak planks, every recipe taken away (/recipe take): how many recipes the inventory's book shows without and
    # with Unlock All Recipes; then "crafting table" typed in its search, the recipe clicked (the server did not give
    # it: laid in the grid with slot clicks) and the result shift-clicked out. The book's layout is vanilla's: the
    # search box 54 right of the first recipe button and 11 up, the result slot 299 right and 5 down.
    reset()
    cmd("item replace entity @s inventory.0 with minecraft:oak_planks 4")
    wait(10)
    cmd("recipe take @s *")
    wait(10)

    def book():
        return ctl("screen").get("recipe_book") or {}

    def centre(r):
        return str(r[0] + r[2] / 2), str(r[1] + r[3] / 2)

    def opened():
        ctl("open-inventory")
        wait(5)
        b = book()
        if b and not b.get("open"):
            ctl("click-at", *centre(b["targets"][0]))
            wait(5)
        return book().get("known", 0)

    before = opened()
    ctl("close")
    wait(3)
    only("unlockallrecipes")
    after = opened()
    targets = book().get("targets") or []
    made = laid = 0
    if len(targets) > 1:
        first = targets[1]
        ctl("click-at", str(first[0] + 54), str(first[1] - 11))
        wait(2)
        ctl("type", "crafting table")
        wait(5)
        ctl("click-at", *centre(book()["targets"][1]))
        wait(10)
        laid = sum(1 for slot in ctl("screen").get("slots") or [] if 100 <= slot.get("slot", 0) < 104 and "oak_planks" in (slot.get("item") or ""))
        ctl("key", "shift", "down")
        ctl("click-at", str(first[0] + 299), str(first[1] + 5))
        ctl("key", "shift", "up")
        wait(5)
    ctl("close")
    wait(5)
    made = ctl("inventory").get("counts", {}).get("minecraft:crafting_table", 0)
    only()
    ok = after > before + 100 and laid == 4 and made == 1
    return ok, f"recipes shown: {before} without, {after} with it; the crafting table from the book: planks laid {laid}, tables made {made}"


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


def probe_ticks(mark):
    """The wire probe's lines since mark, a client tick at a time (each ends with its "order tickend")."""
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        lines = [line.strip() for line in f if not line[:1].isdigit()]
    ticks, tick = [], []
    for line in lines:
        tick.append(line)
        if line == "order tickend":
            ticks.append(tick)
            tick = []
    return ticks


USE_ITEM, SET_CARRIED_ITEM = 67, 54


@test("Wind Jump jumps before the blast")
def _():
    # A wind charge thrown straight down from the ground: with Wind Jump, the first move off the ground after the
    # throw is a jump's 0.42, in a tick whose PlayerInput says jump (a jump the server (Grim) predicts, no velocity
    # written), and the blast then adds in the air: a rise above the throw without it. Off: no jump bit at all. The
    # throws measured are Wind Jump's second and third (the first shows it how long this server's blast takes); a
    # blast a tick sooner than any before it comes before the jump (no jump, the plain blast's rise), so one of two.
    def run(mods):
        reset()
        slot(0, "wind_charge", 16)
        ctl("key", "1", "tap")
        ctl("look", "0", "90")
        only(*mods)
        wait(5)
        ground, mark = pos()[1], probe_mark()
        ctl("mouse", "right", "click")
        rise = round(max(poll(40, 1, lambda: pos()[1])) - ground, 2)
        only()
        ticks = probe_ticks(mark)
        throw = next((i for i, t in enumerate(ticks) if f"pkt {USE_ITEM}" in t), len(ticks))
        jump = up = None
        for i, t in enumerate(ticks[throw:]):
            moves = [line.split() for line in t if line.startswith("send pos") and len(line.split()) == 4]
            if moves and moves[0][3] == "0":
                jump = any(int(line.split()[1]) & 16 for line in t if line.startswith("input "))
                up = (i, round(float(moves[0][2]) - ground, 3))
                break
        return jump, up, rise

    off, _ = run(()), run(("windjump",))
    on = [run(("windjump",)) for _ in range(2)]
    jumped = [r for r in on if r[0] and r[1] is not None and abs(r[1][1] - 0.42) < 0.01 and r[2] > off[2] + 0.5]
    ok = not off[0] and bool(jumped) and all(r[0] or r[1] is None or r[1][1] > 1.0 for r in on)
    return ok, (f"the first move off the ground after the throw: jump in its PlayerInput, (ticks after the throw, "
                f"rise); then the highest rise: off {off}, on {on}")


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
    if n == 0:
        wait(125)
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


@test("Hitboxes")
def _():
    # A pig with 1000 health 2.5 blocks in front, your look passing 0.75 beside its middle (0.3 past its box): health
    # it lost to 5 clicks without Hitboxes, with it (Entities pig, Expand 0.5) and with it listing only players.
    lost = {}
    for name, entities in (("off", None), ("on", "pig"), ("players", "@player")):
        reset()
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        cmd("kill @e[type=pig]", f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 3.0} {TOUGH_PIG}")
        wait(10)
        if entities:
            setting("hitboxes", "entities", entities)
            only("hitboxes")
        ctl("look-at", str(X0 + 0.5 + 0.75), str(Y + 1.45), str(Z0 + 3.0))
        wait(2)
        for _ in range(5):
            ctl("mouse", "left", "click")
            wait(14)
        only()
        m = mobs("pig")
        lost[name] = round(1000 - m[0]["health"], 1) if m else None
    cmd("kill @e[type=pig]")
    ok = lost["off"] == 0 and (lost["on"] or 0) > 0 and lost["players"] == 0
    return ok, f"health the pig lost to 5 clicks 0.3 beside its box: off {lost['off']}, on {lost['on']}, on for players only {lost['players']}"


@test("Auto Spear (Spear reach)")
def _():
    # The same pig, a sword in hand and an iron spear in slot 3: Spear reach (When out of reach) reaches it with the
    # sword, passing through the spear's slot; the sword stays in hand.
    off = pig_clicks((), 5.0)
    on = pig_clicks(("autospear",), 5.0, (("autospear", "spear reach", "when out of reach"),))
    setting("autospear", "spear reach", "off")
    if HORIZON_SERVER and on[0] == 0:
        return None, f"{REFUSED}: off {off[0]}, on {on[0]}"
    return off[0] == 0 and (on[0] or 0) > 0 and "sword" in on[1], (
        f"5 clicks at a pig 4.5 blocks off: {off[0]} health lost without Spear reach, {on[0]} with it; hand after: {on[1]}")


@test("Auto Spear (Extended reach)")
def _():
    # The same pig 4.5 blocks off, a sword in hand and an iron spear (no Lunge) in slot 3: Extended reach stabs it with
    # the spear on your clicks (the sword back two ticks after each), and with Trigger Bot (Works with Trigger Bot) on
    # its own while you look at it.
    off = pig_clicks((), 5.0)
    clicks = pig_clicks(("autospear",), 5.0, (("autospear", "extended reach", "true"),))
    trigger = pig_clicks(("autospear", "triggerbot"), 5.0, (("triggerbot", "passive", "true"),), n=0)
    setting("autospear", "extended reach", "false")
    setting("triggerbot", "passive", "false")
    if HORIZON_SERVER and clicks[0] == 0:
        return None, f"{REFUSED}: off {off[0]}, clicks {clicks[0]}, Trigger Bot {trigger[0]}"
    ok = off[0] == 0 and (clicks[0] or 0) > 0 and "sword" in clicks[1] and (trigger[0] or 0) > 0 and "sword" in trigger[1]
    return ok, (f"(health lost, hand after) at a pig 4.5 blocks off: 5 clicks without Extended reach {off}, with it "
                f"{clicks}; Trigger Bot with it {trigger}")


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
    # charges used, without and with Pearl Catch (Auto). The air above is cleared first: a server's world keeps what
    # earlier runs left up there, and a block 21 up and 12 ahead caught the plain pearl on Paper and vanilla.
    got = {}
    for name, mods in (("off", ()), ("on", ("pearlcatch",))):
        reset()
        cmd(f"fill {X0-5} {Y+1} {Z0-3} {X0+5} {Y+40} {Z0+24} minecraft:air")
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


@test("Loadouts (panel)")
def _():
    # The Loadouts panel, K held: a sword named Duel saved in slot 1; then a plain sword and the named one in the
    # inventory and dirt in slot 1: a click on the row takes the named one (the item and its name matched first); a
    # right click deletes the row; Save current keeps what you hold as "Loadout 1". A screenshot of the panel.
    def listed():
        mark = log_mark()
        say(".loadout list")
        wait(4)
        return sunset_said(mark)

    reset()
    only("loadouts")
    cmd("clear @s", 'item replace entity @s hotbar.0 with minecraft:diamond_sword[custom_name="Duel"]')
    say(".loadout save named")
    cmd("clear @s", "item replace entity @s inventory.4 with minecraft:diamond_sword",
        'item replace entity @s inventory.9 with minecraft:diamond_sword[custom_name="Duel"]', "item replace entity @s hotbar.0 with minecraft:dirt 3")
    wait(5)
    w, h = gui_size()
    top = max(20, h / 2 - 40 / 2 - 10)
    row = (w / 2 - 91 + 12 + 40, top + 20)
    ctl("key", "k", "down")
    wait(5)
    ctl("screenshot", f"{SHOTS}/loadouts.png")
    time.sleep(2)
    ctl("click-at", *map(str, row))
    wait(10)
    main = ctl("inventory").get("main") or []
    item = lambda i: ((main[i] or {}).get("item") or "") if i < len(main) else ""
    picked = (item(4), item(9))
    ctl("click-at", *map(str, row), "right")
    wait(5)
    ctl("key", "k", "up")
    wait(5)
    after_delete = listed()
    ctl("key", "k", "down")
    wait(5)
    top = max(20, h / 2 - 10)
    ctl("click-at", str(w / 2), str(top + 2 + 7))
    wait(5)
    ctl("key", "k", "up")
    saved = listed()
    say(".loadout delete 1")
    only()
    ok = "diamond_sword" in picked[0] and "dirt" in picked[1] and any("No loadouts" in l for l in after_delete) and any("Loadout 1" in l for l in saved)
    return ok, f"after the click (inventory 5, inventory 10): {picked}; after the right click: {after_delete[:1]}; Save current: {saved[:1]}"


@test("Loadouts (inventory profiles)")
def _():
    # A kit with Armour and Full layout and slots 1 and 10 locked: a sword and a bow in slots 1 and 2, stone and apples
    # in the main inventory's first and sixth, a totem in the off hand, an iron helmet and a diamond chestplate worn.
    # Everything moved elsewhere: .loadout kit puts it all back. Then the locked slots: Inventory Manager throws the
    # dirt of slot 11 but not of 1 and 10. Refill broken tools: the sword of slot 1 breaks on a pig, the spare from
    # the inventory takes its place. Export, then import the file under another name.
    def said(command):
        mark = log_mark()
        say(command)
        wait(4)
        return sunset_said(mark)

    reset()
    only("loadouts")
    setting("loadouts", "armour", "true")
    setting("loadouts", "full layout", "true")
    setting("loadouts", "locked slots", "1 10")
    say(".loadout delete kit")
    say(".loadout delete shared")
    cmd("clear @s", "item replace entity @s hotbar.0 with minecraft:diamond_sword", "item replace entity @s hotbar.1 with minecraft:bow",
        "item replace entity @s inventory.0 with minecraft:stone 10", "item replace entity @s inventory.5 with minecraft:apple 3",
        "item replace entity @s weapon.offhand with minecraft:totem_of_undying", "item replace entity @s armor.head with minecraft:iron_helmet",
        "item replace entity @s armor.chest with minecraft:diamond_chestplate")
    wait(5)
    say(".loadout save kit")
    setting("loadouts", "locked slots", "")
    cmd("clear @s", "item replace entity @s inventory.20 with minecraft:diamond_sword", "item replace entity @s hotbar.5 with minecraft:bow",
        "item replace entity @s inventory.3 with minecraft:iron_helmet", "item replace entity @s inventory.11 with minecraft:diamond_chestplate",
        "item replace entity @s hotbar.3 with minecraft:stone 10", "item replace entity @s inventory.25 with minecraft:apple 3",
        "item replace entity @s inventory.7 with minecraft:totem_of_undying", "item replace entity @s hotbar.0 with minecraft:dirt 2")
    wait(5)
    say(".loadout kit")
    wait(10)
    inv = ctl("inventory")
    name = lambda part, i: (((inv.get(part) or [])[i] or {}).get("item") or "").replace("minecraft:", "")
    layout = (name("hotbar", 0), name("hotbar", 1), name("main", 0), name("main", 5), offhand().replace("minecraft:", ""),
              check("execute if items entity @s armor.head minecraft:iron_helmet"),
              check("execute if items entity @s armor.chest minecraft:diamond_chestplate"))
    locked = next((c for c in said(".settings loadouts") if c.startswith("Locked slots:")), "")
    # The locked slots (the kit's 1 and 10) keep their dirt.
    setting("inventorymanager", "preset", "Custom")
    setting("inventorymanager", "items", "dirt")
    cmd("clear @s", "item replace entity @s hotbar.0 with minecraft:dirt 2", "item replace entity @s inventory.0 with minecraft:dirt 3",
        "item replace entity @s inventory.1 with minecraft:dirt 4")
    only("loadouts", "inventorymanager")
    ctl("open-inventory")
    wait(30)
    ctl("close")
    only("loadouts")
    inv = ctl("inventory")
    cleaned = (name("hotbar", 0), name("main", 0), name("main", 1))
    cmd("kill @e[type=item]")
    # Refill broken tools: one hit left in the sword.
    setting("loadouts", "locked slots", "")
    cmd("clear @s", "item replace entity @s hotbar.0 with minecraft:diamond_sword[damage=1560]",
        "item replace entity @s inventory.4 with minecraft:diamond_sword")
    ctl("key", "1", "tap")
    pid = crit_pig()
    ctl("look-at-entity", str(pid))
    wait(25)
    ctl("mouse", "left", "click")
    wait(10)
    inv = ctl("inventory")
    refilled = (name("hotbar", 0), name("main", 4))
    cmd("kill @e[type=pig]")
    # Export and import.
    exported = said(".loadout export kit")
    found = [os.path.join(r, f) for r, _, fs in os.walk(os.path.join(session_dir(), "home")) for f in fs if f == "kit.loadout" and "exports" in r]
    imported = listed = []
    if found:
        imports = os.path.join(os.path.dirname(os.path.dirname(found[0])), "imports")
        with open(found[0]) as f, open(os.path.join(imports, "shared.loadout"), "w") as g:
            g.write("shared" + f.read()[3:])
        imported = said(".loadout import shared")
        listed = said(".loadout list")
    say(".loadout delete kit")
    say(".loadout delete shared")
    only()
    ok = (layout == ("diamond_sword", "bow", "stone", "apple", "totem_of_undying", True, True) and locked == "Locked slots: 1 10"
          and cleaned == ("dirt", "dirt", "") and refilled == ("diamond_sword", "") and any("2: shared" in l for l in listed))
    return ok, (f"after .loadout kit (slot 1, 2, 10, 15, off hand, helmet, chestplate): {layout}, {locked!r}; Inventory Manager "
                f"on dirt in slots 1, 10 (locked) and 11: {cleaned}; after the sword broke (slot 1, spare's slot): {refilled}; "
                f"export {exported[:1]}, import {imported[:1]}, list {listed}")


def shield_run(module):
    """A sword in hand, a shield in the offhand, `module` (killaura or triggerbot) on, with its Shield off then on:
    health you lost to 10 sword hits from Bot1 (every 15 ticks), then for 10 s with Bot1 in front: health Bot1 lost,
    your hits, and the ticks your shield was down per hit."""
    took, gave = {}, {}
    setting(module, "entities", "player")
    for name in ("off", "on"):
        reset()
        slot(0, "diamond_sword")
        cmd("item replace entity @s weapon.offhand with minecraft:shield", "effect give @s minecraft:health_boost 600 9 true",
            "effect give @s minecraft:instant_health 1 28 true", "effect give @s minecraft:resistance 600 1 true")
        ctl("key", "1", "tap")
        bot_ready(2.5, 180, "give Bot1 minecraft:diamond_sword")
        ctl2("select", "1")
        setting(module, "shield", "true" if name == "on" else "false")
        ctl("look-at", str(X0 + 0.5), str(Y + 1.6), str(Z0 + 2.5))
        # Bot1 a friend while it hits you: the module leaves it alone and the shield stays up.
        say(".friend add Bot1")
        only(module)
        wait(20)
        mine = own_health()
        for _ in range(10):
            cmd(f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5} 180 0")
            ctl2("look-at", str(X0 + 0.5), str(Y + 1.6), str(Z0 + 0.5))
            wait(1)
            ctl2("mouse", "left", "click")
            wait(12)
        took[name] = round(mine - own_health(), 1)
        only()
        say(".friend remove Bot1")
        bot_ready(2.5, 180)
        only(module)
        bot, down, hits, last = bot_health(), 0, 0, bot_health()
        for t in range(200):
            if t % 10 == 0:
                cmd(f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 2.5} 180 0")
                ctl("look-at", str(X0 + 0.5), str(Y + 1.6), str(Z0 + 2.5))
            wait(1)
            u = state().get("using") or {}
            h = bot_health()
            # Once Bot1 is dead (Horizon's server keeps it at 20 health) there is no one to shield from.
            down += u.get("slot") != 40 and h > 0
            hits += h < last - 0.01
            last = h
        only()
        gave[name] = (round(bot - bot_health(), 1), hits, round(down / max(hits, 1), 1))
    setting(module, "shield", "false")
    bot_done()
    # The shield comes down for a hit's own tick and no longer.
    ok = took["on"] < took["off"] and gave["on"][0] > 0 and gave["on"][2] <= 1.5
    return ok, (f"health you lost to 10 hits: Shield off {took['off']}, on {took['on']}; for 10 s (Bot1 lost, hits, "
                f"ticks unshielded per hit): off {gave['off']}, on {gave['on']}")


@test("Shield (Kill Aura)")
def _():
    if not CTL2:
        return None, "needs a second player"
    return shield_run("killaura")


@test("Shield (Trigger Bot)")
def _():
    if not CTL2:
        return None, "needs a second player"
    return shield_run("triggerbot")


@test("Shield (settings from before 0.9)")
def _():
    # A profile from before Kill Aura and Trigger Bot took in Auto Shield: Auto Shield on, Range 9, Re-raise delay 2.
    shown = old_profile("verifyshield", ["autoshield.on=true", "autoshield.key=none", "autoshield.range=9",
                                         "autoshield.reraisedelay=2", "autoshield.onlywithaweapon=false"], "killaura")
    if shown is None:
        return False, "no profile file written"
    want = ["Shield: ON", "Shield only with a weapon: OFF", "Shield range: 9", "Shield re-raise delay: 2"]
    got = [w for w in want if any(line.startswith(w) for line in shown)]
    return got == want, f"Kill Aura settings {got} of {want}; said {shown[:2]}"


@test("Auto Weapon (settings from before 0.9)")
def _():
    # A profile from before Auto Weapon took in Shield Breaker, Mace and Spear Swap: Shield Breaker on (Auto, Holding
    # sword, bound to G, Switch back off), Mace on (Breach swap: dropped, Auto Mace has the mace now), Spear Swap and
    # Auto Weapon off.
    shown = old_profile("verifyweapon", ["shieldbreaker.on=true", "shieldbreaker.key=key.keyboard.g", "shieldbreaker.mode=Auto",
                                         "shieldbreaker.holding=sword", "shieldbreaker.switchback=false", "mace.on=true", "mace.key=none",
                                         "mace.breachswap=true", "spearswap.on=false", "spearswap.key=none", "autoweapon.on=false",
                                         "autoweapon.key=none"], "autoweapon")
    if shown is None:
        return False, "no profile file written"
    want = ["Best weapon: OFF", "Axe on shields: ON", "Shield mode: Auto", "Holding: sword", "Swap back: OFF"]
    on = any("Auto Weapon*" in line for line in shown)
    bound = any(line.startswith("Auto Weapon: G") for line in shown)
    got = [w for w in want if any(line.startswith(w) for line in shown)]
    return on and bound and got == want, f"Auto Weapon on: {on}, bound to G: {bound}, settings {got} of {want}; said {shown[:2]}"


@test("Rotations")
def _():
    # Rotations' Lock, Simple (yaw 90, pitch 30), holds the look while you turn; Smart snaps it to the nearest 45 and
    # 30 degrees. No rotate alone (its default): a /tp that sets yaw 180 and pitch 45 leaves your look as it was (off:
    # it takes it).
    reset()
    setting("rotations", "lock", "true")
    setting("rotations", "yaw angle", "90")
    setting("rotations", "pitch angle", "30")
    only("rotations")
    ctl("look", "10", "-20")
    wait(5)
    simple = tuple(round(v) for v in aim_view())
    setting("rotations", "yaw lock", "Smart")
    setting("rotations", "pitch lock", "Smart")
    ctl("look", "50", "-20")
    wait(5)
    smart = tuple(round(v) % 360 if i == 0 else round(v) for i, v in enumerate(aim_view()))
    only()
    say(".reset rotations")
    turned = {}
    for name, mods in (("off", ()), ("on", ("rotations",))):
        ctl("look", "0", "0")
        only(*mods)
        wait(3)
        cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 180 45")
        wait(10)
        turned[name] = tuple(round(v) % 360 if i == 0 else round(v) for i, v in enumerate(aim_view()))
        only()
    ok = simple == (90, 30) and smart == (45, -30) and turned["off"] == (180, 45) and turned["on"] == (0, 0)
    return ok, f"Lock Simple {simple}, Smart from (50, -20) {smart}; look after a /tp to (180, 45): off {turned['off']}, No rotate {turned['on']}"


def active_effects():
    d = ctl("effects")
    return (d.get("effects") or d).get("active", [])


@test("No Status Effects")
def _():
    # Levitation for 3 s: you rise without No Status Effects and stay on the floor with it.
    risen = {}
    for name, mods in (("off", ()), ("on", ("nostatuseffects",))):
        reset()
        only(*mods)
        cmd("effect give @s minecraft:levitation 3 1")
        wait(40)
        risen[name] = round(pos()[1] - (Y + 1), 2)
        only()
    ok = risen["off"] > 1 and abs(risen["on"]) < 0.1
    return ok, f"risen with levitation: off {risen['off']}, on {risen['on']}"


@test("Mine Through Mobs")
def _():
    # A pig with 1000 health 1.5 blocks in front of a stone block, a pickaxe in hand, survival: the left button held
    # 2 s at the stone through the pig hits the pig without Mine Through Mobs and mines the stone with it.
    got = {}
    for name, mods in (("off", ()), ("on", ("minethroughmobs",))):
        reset()
        slot(0, "netherite_pickaxe")
        ctl("key", "1", "tap")
        cmd(f"setblock {X0} {Y + 1} {Z0 + 3} minecraft:stone", f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 1.7} {TOUGH_PIG}")
        wait(10)
        only(*mods)
        ctl("look-at", str(X0 + 0.5), str(Y + 1.5), str(Z0 + 3.0))
        ctl("mouse", "left", "down")
        wait(40)
        ctl("mouse", "left", "up")
        only()
        m = mobs("pig")
        got[name] = (round(1000 - m[0]["health"], 1) if m else None, count(X0, Y + 1, Z0 + 3, X0, Y + 1, Z0 + 3, "stone"))
    cmd("kill @e[type=pig]", f"setblock {X0} {Y + 1} {Z0 + 3} minecraft:air")
    ok = got["off"][0] > 0 and got["off"][1] == 1 and got["on"][0] == 0 and got["on"][1] == 0
    return ok, f"(pig's health lost, stone left) off {got['off']}, Mine Through Mobs {got['on']}"


@test("Anvil (Self Anvil, Anti Anvil)")
def _():
    # In an obsidian hole with walls 3 high: Anvil's Self Anvil alone puts an anvil 2 over your feet (it falls into the
    # hole) and Anvil turns off; its Anti Anvil puts obsidian over your head against an anvil falling from 7 over your feet.
    def hole():
        reset()
        cmd(f"fill {X0 - 1} {Y} {Z0 - 1} {X0 + 1} {Y + 3} {Z0 + 1} minecraft:obsidian", f"fill {X0} {Y + 1} {Z0} {X0} {Y + 8} {Z0} minecraft:air",
            f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
        wait(10)

    def at(dy):
        return (ctl("blocks", str(X0), str(Y + 1 + dy), str(Z0), str(X0), str(Y + 1 + dy), str(Z0)).get("counts") or {})

    hole()
    slot(0, "anvil")
    setting("anvil", "anti anvil", "false")
    setting("anvil", "self anvil", "true")
    only("anvil")
    wait(30)
    self_anvil = (any("minecraft:anvil" in at(dy) for dy in (0, 1, 2)), not turned_itself_off("anvil", "Anvil"))
    only()
    hole()
    slot(0, "obsidian", 8)
    only("anvil")
    cmd(f"setblock {X0} {Y + 8} {Z0} minecraft:anvil")
    wait(30)
    anti_anvil = "minecraft:obsidian" in at(2) and "minecraft:anvil" in at(3)
    only()
    cmd(f"fill {X0 - 1} {Y + 1} {Z0 - 1} {X0 + 1} {Y + 8} {Z0 + 1} minecraft:air", f"fill {X0 - 1} {Y} {Z0 - 1} {X0 + 1} {Y} {Z0 + 1} minecraft:stone")
    ok = self_anvil == (True, False) and anti_anvil
    return ok, f"Self Anvil (anvil in the hole, still on): {self_anvil}; Anti Anvil obsidian over your head, the anvil on it: {anti_anvil}"


@test("Portals and Multitask")
def _():
    # Portals: the inventory open, you step into a nether portal (survival: it takes 4 s to go through): after 1 s the
    # inventory is still open with Portals, closed without. Multitask: eating a golden apple, 3 clicks at a pig in front
    # hit it with Multitask (and the eating goes on), not without.
    kept = {}
    for name, mods in (("off", ()), ("on", ("portals",))):
        reset()
        cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 2} {X0 + 2} {Y + 5} {Z0 + 2} minecraft:obsidian",
            f"fill {X0} {Y + 2} {Z0 + 2} {X0 + 1} {Y + 4} {Z0 + 2} minecraft:nether_portal[axis=x]")
        wait(5)
        only(*mods)
        ctl("open-inventory")
        wait(5)
        cmd(f"tp @s {X0 + 1.0} {Y + 2} {Z0 + 2.5} 0 0")
        wait(20)
        kept[name] = state().get("screen")
        ctl("close")
        only()
        cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 2} {X0 + 2} {Y + 5} {Z0 + 2} minecraft:air", f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
        wait(5)
    hit = {}
    for name, mods in (("off", ()), ("on", ("multitask",))):
        reset()
        slot(0, "golden_apple", 8)
        ctl("key", "1", "tap")
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 2.0} {TOUGH_PIG}")
        wait(10)
        pid = mobs("pig")[0]["id"]
        ctl("look-at-entity", str(pid))
        only(*mods)
        ctl("mouse", "right", "down")
        wait(5)
        using = []
        for _ in range(3):
            ctl("mouse", "left", "click")
            wait(5)
            using.append(bool(state().get("using")))
        ctl("mouse", "right", "up")
        only()
        m = mobs("pig")
        hit[name] = (round(1000 - m[0]["health"], 1) if m else None, all(using))
    cmd("kill @e[type=pig]")
    ok = kept["off"] != "inventory" and kept["on"] == "inventory" and hit["off"][0] == 0 and hit["on"][0] > 0 and hit["on"][1]
    return ok, f"screen 1 s into a portal: off {kept['off']}, Portals {kept['on']}; (pig's health lost, still eating) to 3 clicks while eating: off {hit['off']}, Multitask {hit['on']}"


@test("Quiver")
def _():
    # In an obsidian hole, a bow in slot 1, a stack of arrows in slot 2 and two tipped arrows of strength in the
    # inventory: Quiver shoots one straight up at you (Check effects: only one), so you have strength, and turns off.
    reset()
    cmd(f"fill {X0 - 1} {Y} {Z0 - 1} {X0 + 1} {Y + 1} {Z0 + 1} minecraft:obsidian", f"setblock {X0} {Y + 1} {Z0} minecraft:air",
        f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0", "item replace entity @s hotbar.0 with minecraft:bow",
        "item replace entity @s hotbar.1 with minecraft:arrow 16",
        'item replace entity @s inventory.5 with minecraft:tipped_arrow[potion_contents={potion:"minecraft:strength"}] 2')
    wait(10)
    only("quiver")
    wait(60)
    strong = any("strength" in e.get("id", "") for e in active_effects())
    still = is_on("Quiver")
    only()
    counts = ctl("inventory").get("counts", {})
    cmd(f"fill {X0 - 1} {Y + 1} {Z0 - 1} {X0 + 1} {Y + 1} {Z0 + 1} minecraft:air", f"fill {X0 - 1} {Y} {Z0 - 1} {X0 + 1} {Y} {Z0 + 1} minecraft:stone",
        "kill @e[type=arrow]")
    left = counts.get("minecraft:tipped_arrow", 0)
    ok = strong and not still and left == 1 and counts.get("minecraft:arrow") == 16
    # Horizon's server (single player's too) hits you with your own tipped arrow but gives no effect from it (a
    # summoned one's it does).
    if HORIZON_SERVER and not strong and left == 1:
        return None, f"Horizon's server gives no effect from your own tipped arrow: shot 1 of 2, still on {still}"
    return ok, f"strength after Quiver: {strong}; still on: {still}; tipped arrows left {left} of 2, plain arrows {counts.get('minecraft:arrow')}"


@test("Arrow Dodge")
def _():
    # Three arrows shot at you from 10 blocks south, one a second: health lost and where you are after, without and
    # with Arrow Dodge.
    got = {}
    for name, mods in (("off", ()), ("on", ("arrowdodge",))):
        reset()
        only(*mods)
        start = own_health()
        for _ in range(3):
            cmd(f"summon arrow {X0 + 0.5} {Y + 2.3} {Z0 + 10.5} {{Motion:[0.0d,0.0d,-2.0d]}}")
            wait(20)
        got[name] = (round(start - own_health(), 1), round(flat(pos(), (X0 + 0.5, 0, Z0 + 0.5)), 1))
        only()
        cmd("kill @e[type=arrow]")
    ok = got["off"][0] > 0 and got["on"][0] == 0
    return ok, f"(health lost, blocks moved) to 3 arrows: off {got['off']}, Arrow Dodge {got['on']}"


@test("Liquid Interact")
def _():
    # A pool 5 wide and 6 deep in front (its floor and sides out of reach), stone in hand, the crosshair on the
    # water: a right click places nothing without Liquid Interact and puts the stone on the water with it.
    got = {}
    for name, mods in (("off", ()), ("on", ("liquidinteract",))):
        reset()
        cmd(f"fill {X0 - 3} {Y - 7} {Z0 + 1} {X0 + 3} {Y - 1} {Z0 + 7} minecraft:stone",
            f"fill {X0 - 2} {Y - 6} {Z0 + 1} {X0 + 2} {Y} {Z0 + 6} minecraft:water")
        slot(0, "stone", 8)
        ctl("key", "1", "tap")
        wait(10)
        only(*mods)
        ctl("look-at", str(X0 + 0.5), str(Y + 1.0), str(Z0 + 3.5))
        wait(3)
        ctl("mouse", "right", "click")
        wait(10)
        only()
        got[name] = count(X0 - 2, Y - 6, Z0 + 1, X0 + 2, Y, Z0 + 6, "stone")
        cmd(f"fill {X0 - 3} {Y - 7} {Z0 + 1} {X0 + 3} {Y - 1} {Z0 + 7} minecraft:air", f"fill {X0 - 3} {Y} {Z0 + 1} {X0 + 3} {Y} {Z0 + 7} minecraft:stone")
    return got["off"] == 0 and got["on"] == 1, f"stone in the pool after a right click on it: off {got['off']}, Liquid Interact {got['on']}"


@test("No Interact")
def _():
    # A chest and a pig in front: a right click on the chest opens it, and 3 clicks on the pig hurt it, without No
    # Interact; with it listing the chest (Use blocks) and pigs (Hit entities), neither.
    got = {}
    for name, mods in (("off", ()), ("on", ("nointeract",))):
        reset()
        setting("nointeract", "use blocks", "chest")
        setting("nointeract", "hit entities", "pig")
        cmd(f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:chest", f"summon pig {X0 + 2.5} {Y + 1} {Z0 + 2.0} {TOUGH_PIG}")
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        wait(10)
        only(*mods)
        ctl("look-at", str(X0 + 0.5), str(Y + 1.5), str(Z0 + 2.5))
        wait(3)
        ctl("mouse", "right", "click")
        wait(10)
        opened = state().get("screen") not in ("playing", None)
        ctl("close")
        wait(3)
        ctl("look-at-entity", str(mobs("pig")[0]["id"]))
        wait(3)
        for _ in range(3):
            ctl("mouse", "left", "click")
            wait(14)
        only()
        m = mobs("pig")
        got[name] = (opened, round(1000 - m[0]["health"], 1) if m else None)
        cmd("kill @e[type=pig]", f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:air")
    ok = got["off"][0] and got["off"][1] > 0 and not got["on"][0] and got["on"][1] == 0
    return ok, f"(chest opened, pig's health lost) off {got['off']}, No Interact {got['on']}"


@test("Anvil: Auto Anvil")
def _():
    # Bot1 3 blocks in front, anvils and stone buttons in your hotbar: in 3 s Anvil's Auto Anvil drops anvils on Bot1 (how many
    # left your hotbar, and the health it lost); without it, none.
    if not CTL2:
        return None, "needs a second player"
    got = {}
    for name, mods in (("off", ()), ("on", ("anvil",))):
        reset()
        setting("anvil", "anti anvil", "false")
        setting("anvil", "auto anvil", "true")
        slot(0, "diamond_sword")
        slot(1, "anvil", 16)
        slot(2, "stone_button", 16)
        ctl("key", "1", "tap")
        bot_ready(3.0, 180)
        before = bot_health()
        only(*mods)
        wait(60)
        only()
        got[name] = (16 - ctl("inventory").get("counts", {}).get("minecraft:anvil", 0), round(before - bot_health(), 1))
        cmd(f"fill {X0 - 2} {Y + 1} {Z0 + 1} {X0 + 2} {Y + 8} {Z0 + 5} minecraft:air", "kill @e[type=item]")
    bot_done()
    ok = got["off"][0] == 0 and got["on"][0] > 0
    return ok, f"(anvils placed, Bot1's health lost) in 3 s: off {got['off']}, Auto Anvil {got['on']}"


@test("Fake Player")
def _():
    # Fake Player on where you stand, then you step 3 blocks back: a player is there in your world (a screenshot of
    # it), Kill Aura (players) sends no hit at it for 2 s, and it is gone once Fake Player goes off. On a server it
    # goes straight off, saying why, and no player appears.
    reset()
    if SERVER:
        mark = log_mark()
        say(".t fakeplayer")
        wait(10)
        said = sunset_said(mark)
        fakes = [e for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:player" and e.get("id") == 0x7FFFFFF0]
        refused = any("single player only" in line for line in said)
        return refused and not fakes, f"on a server: said {said[-2:]}; fake players in the world: {len(fakes)}"
    only("fakeplayer")
    wait(5)
    cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 - 2.5} 0 0")
    wait(10)
    ctl("look-at", str(X0 + 0.5), str(Y + 1.6), str(Z0 + 0.5))
    wait(5)
    fakes = lambda: [e for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:player" and e.get("id") == 0x7FFFFFF0]
    there = len(fakes())
    ctl("screenshot", f"{SHOTS}/fake_player.png")
    time.sleep(2)
    setting("killaura", "entities", "player")
    mark = probe_mark() if probe_path() else None
    only("fakeplayer", "killaura")
    wait(40)
    hits = None
    if mark is not None:
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            hits = sum(1 for line in f if line.startswith("order attack"))
    only()
    wait(5)
    gone = len(fakes()) == 0
    return there == 1 and hits in (0, None) and gone, f"fake players in the world: {there}; hits sent at it by Kill Aura in 2 s: {hits}; gone when off: {gone} (shot {SHOTS}/fake_player.png)"


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


@test("Auto Mace (Stun slam)")
def _():
    # Bot1 on a pillar 7 blocks up, holding a shield up toward the drop beside it; you fall past it from 19 blocks up
    # with an axe in hand and a mace in slot 4, Auto Mace's Trigger with Silent aim (Min fall 3): Bot1's health lost by
    # the time you land, Stun slam off (the mace hits the shield) and on (the axe takes the shield down first).
    if not CTL2:
        return None, "needs a second player"
    setting("automace", "min fall", "3")
    setting("automace", "silent aim", "true")
    lost = {}
    for name in ("false", "true"):
        reset()
        slot(0, "diamond_axe")
        slot(3, "mace")
        ctl("key", "1", "tap")
        setting("automace", "stun slam", name)
        bot_ready(1.5, 180, "give Bot1 minecraft:shield", f"setblock {X0} {Y + 7} {Z0 + 1} minecraft:stone",
                  f"tp Bot1 {X0 + 0.5} {Y + 8} {Z0 + 1.5} 180 0")
        ctl2("select", "1")
        ctl2("mouse", "right", "down")
        wait(10)
        before = bot_health()
        only("automace")
        cmd(f"tp @s {X0 + 0.5} {Y + 20} {Z0 + 0.5} 0 0")
        for _ in range(60):
            wait(1)
            if state()["player"].get("on_ground"):
                break
        wait(5)
        only()
        lost[name] = round(before - bot_health(), 1)
        ctl2("mouse", "right", "up")
        cmd(f"setblock {X0} {Y + 7} {Z0 + 1} minecraft:air")
    bot_done()
    setting("automace", "stun slam", "false")
    setting("automace", "silent aim", "false")
    setting("automace", "min fall", "10")
    return lost["true"] > lost["false"], f"Bot1's health lost to your fall past it: Stun slam off {lost['false']}, on {lost['true']}"


def x_window():
    """The X window of this session's game (its process has the session's probe path), for real key presses with
    xdotool on its display; None without xdotool or a window."""
    global XDISPLAY
    want = f"{session_dir()}/probe.log"
    for pid in subprocess.run(["pgrep", "horizon-client"], capture_output=True, text=True).stdout.split():
        try:
            with open(f"/proc/{pid}/environ", "rb") as f:
                env = dict(v.split("=", 1) for v in f.read().decode(errors="replace").split("\0") if "=" in v)
        except OSError:
            continue
        if env.get("SUNSET_PROBE") != want:
            continue
        XDISPLAY = env.get("DISPLAY", XDISPLAY)
        env = {**os.environ, "DISPLAY": XDISPLAY}
        try:
            found = subprocess.run(["xdotool", "search", "--pid", pid], capture_output=True, text=True, env=env).stdout.split()
        except FileNotFoundError:
            return None
        if not found:
            return None
        # Focused once ahead: the first press after the focus moves can be lost.
        subprocess.run(["xdotool", "windowfocus", "--sync", found[-1]], env=env, capture_output=True)
        time.sleep(0.5)
        return found[-1]
    return None


# The game's X display (each session of a side-by-side run has its own), learnt by x_window().
XDISPLAY = ":97"


def x_keys(window, *keys):
    """Real presses (XTEST) of `keys` with the game's window focused, a few ticks apart; "type:<text>" types text."""
    env = {**os.environ, "DISPLAY": XDISPLAY}
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

@test("Sticky keys (real input)")
def _():
    # Real presses (xdotool): a key or button held (it works: you walk, place, break), a screen opened over it, let go
    # behind the screen, the screen closed. Vanilla re-reads the keyboard and mouse as a screen closes, so nothing goes
    # on after: W walks no further (inventory, inventory with GUI Move, Sunset's menu, chat), right click holds no
    # shield up, left click breaks no more of a wall (creative), behind the inventory and Sunset's menu. And the game
    # holds no keys after.
    window = x_window()
    if not window:
        return None, "needs xdotool and the game's window"
    env = {**os.environ, "DISPLAY": XDISPLAY}

    def x(*args):
        subprocess.run(["xdotool", *args], env=env, capture_output=True)
        wait(6)

    def behind(hold, screen, close, measure):
        x("windowfocus", "--sync", window)
        # The pointer on the window (a real button goes where it is), and a click: after a screen the cursor is free,
        # and the first press only takes it back.
        x("mousemove", "--window", window, "200", "150")
        x("click", "3")
        before = measure()
        x(*(("mousedown", hold) if hold.isdigit() else ("keydown", hold)))
        wait(10)
        worked = measure() != before
        for k in screen:
            x("key", k)
        seen = state().get("screen") != "playing" or bool(ctl("status").get("sunset_menu"))
        x(*(("mouseup", hold) if hold.isdigit() else ("keyup", hold)))
        x("key", close)
        wait(4)
        return worked, seen

    def keys():
        return ctl("status").get("keys") or []

    got = {}
    for name, mods, screen, close in (("inventory", (), ("e",), "Escape"), ("inventory, GUI Move", ("guimove",), ("e",), "Escape"),
                                      ("Sunset menu", (), ("grave",), "grave"), ("chat", (), ("t",), "Escape")):
        reset()
        only(*mods)
        worked, seen = behind("w", screen, close, lambda: round(pos()[2], 2))
        at = pos()
        wait(20)
        got[name] = (worked, round(flat(at, pos()), 2), keys())
        only()
    for name, screen, close in (("inventory", ("e",), "Escape"), ("Sunset menu", ("grave",), "grave")):
        reset()
        slot(0, "shield")
        ctl("key", "1", "tap")
        using = lambda: bool(ctl("status").get("using"))
        worked, _ = behind("3", screen, close, using)
        wait(10)
        got[f"right click, {name}"] = (worked, int(using()), keys())
        if name == "Sunset menu":
            # RF Click's F is the right button: its release behind the menu lets go too.
            only("rfclick")
            worked, _ = behind("f", screen, close, using)
            wait(10)
            got["RF Click F, Sunset menu"] = (worked, int(using()), keys())
            only()
        reset("creative")
        cmd(f"fill {X0-1} {Y+1} {Z0+2} {X0+1} {Y+3} {Z0+6} minecraft:stone")
        ctl("look", "0", "10")
        wait(5)
        wall = lambda: count(X0 - 1, Y + 1, Z0 + 2, X0 + 1, Y + 3, Z0 + 6, "stone")
        worked, _ = behind("1", screen, close, wall)
        left = wall()
        wait(30)
        got[f"left click, {name}"] = (worked, left - wall(), keys())
        cmd(f"fill {X0-1} {Y+1} {Z0+2} {X0+1} {Y+3} {Z0+6} minecraft:air")
    stuck = [k for k, (_, n, held) in got.items() if n > 0.05 or held]
    idle = [k for k, (worked, _, _) in got.items() if not worked]
    return not stuck and not idle, (f"(held input worked, then after the screen closed: blocks walked / shield still up / blocks broken, "
                                    f"held keys): {got}; stuck: {stuck}; did nothing while held: {idle}")


# ---- .goto (Rust's PR #24): Horizon's own A* path and walking task, started from chat

def goto_walk(x, y, z, interrupt=None, limit=30):
    """.goto the arena block (x, y, z), sampling where you are until the walk ends. `interrupt` runs once you have
    walked 2 blocks. Gives what Sunset said, the end position, the distance walked, whether it stopped before the
    end, and the probe's (ticks, most move packets in a tick, server teleports) on a server (else None)."""
    mark = log_mark()
    wire = probe_mark() if probe_path() and SERVER else None
    say(f".goto {X0 + x} {y} {Z0 + z}")
    trail, t0, cut = [pos()], time.time(), False
    while time.time() - t0 < limit:
        time.sleep(0.1)
        st = state()
        trail.append(st["player"]["position"])
        if interrupt and not cut and flat(trail[0], trail[-1]) > 2:
            interrupt()
            cut = True
        if not st.get("busy"):
            break
    still = pos()
    wait(10)
    walked = sum(flat(a, b) for a, b in zip(trail, trail[1:]))
    return sunset_said(mark), still, walked, flat(still, pos()), wire_stats(wire) if wire is not None else None


def goto_clear():
    cmd(f"fill {X0-4} {Y+1} {Z0-5} {X0+16} {Y+4} {Z0+5} minecraft:air")


@test(".goto (routes)")
def _():
    # Flat 10 blocks east; the same behind a 3-high wall; up onto a 1-block step; down 2 blocks; and too far.
    got, wires = {}, []
    reset()
    route = {"flat": (10, Y + 1, 0, None)}
    route["wall"] = (10, Y + 1, 0, f"fill {X0+5} {Y+1} {Z0-3} {X0+5} {Y+3} {Z0+3} minecraft:stone")
    route["climb"] = (8, Y + 2, 0, f"fill {X0+3} {Y+1} {Z0-3} {X0+12} {Y+1} {Z0+3} minecraft:stone")
    route["drop"] = (6, Y + 1, 0, f"fill {X0-1} {Y+1} {Z0-1} {X0+1} {Y+2} {Z0+1} minecraft:stone")
    for name, (x, y, z, build) in route.items():
        goto_clear()
        reset(y=Y + 3 if name == "drop" else Y + 1)
        if build:
            cmd(build)
        if name == "drop":
            cmd(f"tp @s {X0 + 0.5} {Y + 3} {Z0 + 0.5} 0 0")
            wait(10)
        said, end, walked, _, wire = goto_walk(x, y, z)
        off = math.hypot(end[0] - (X0 + x + 0.5), end[2] - (Z0 + z + 0.5))
        got[name] = ("Arrived." in said, round(off, 2), round(end[1] - y, 2), round(walked, 1))
        if wire:
            wires.append(wire)
    goto_clear()
    reset()
    said, _, walked, _, _ = goto_walk(200, Y + 1, 0, limit=2)
    far = any("blocks away" in line for line in said) and walked < 0.5
    ticks = sum(w[0] for w in wires)
    most = max((w[1] for w in wires), default=0)
    teleports = sum(w[2] for w in wires)
    ok = (all(a and off < 0.7 and abs(dy) < 0.1 for a, off, dy, _ in got.values()) and got["wall"][3] > got["flat"][3] + 1
          and far and most <= 1 and teleports == 0 and hp() == 20)
    return ok, (f"(arrived, off target, height off, walked) {got}; 200 blocks refused: {far}; "
                + (f"{ticks} ticks, at most {most} move packets in one, {teleports} server teleports" if wires else "no probe"))


@test(".goto (stopping)")
def _():
    # 14 blocks east, stopped after 2 by: the inventory, the ClickGUI (a real ` press), A held (you walk left instead,
    # at once) and .goto stop. Each must leave you standing (no keys held by the walk). Not Panic: it would stop Sunset
    # for the rest of the run (all_off cancels the walk; the Panic test runs last).
    window = x_window()
    stops = {
        "inventory": lambda: ctl("key", "e", "tap"),
        "ClickGUI": (lambda: x_keys(window, "grave")) if window else None,
        "A held": lambda: ctl("key", "a", "down"),
        ".goto stop": lambda: say(".goto stop"),
    }
    got = {}
    for name, stop in stops.items():
        if stop is None:
            continue
        reset()
        _, end, walked, drift, _ = goto_walk(14, Y + 1, 0, interrupt=stop)
        if name == "A held":
            ctl("key", "a", "up")
        got[name] = (round(walked, 1), round(14.5 - (end[0] - X0), 1), round(drift, 2))
        if name in ("inventory", "ClickGUI"):
            ctl("key", "esc", "tap")
            wait(3)
        if state().get("screen") not in ("playing", None):
            ctl("close")
    stopped = all(left > 3 and (drift > 0.5 if name == "A held" else drift < 0.2) for name, (_, left, drift) in got.items())
    return stopped, f"(walked, blocks short, moved after: A held walks on by hand, the rest stand) {got}" + ("" if window else "; ClickGUI: no window")



# The Grim alerts of Sayer's two screenshots (3 Nov), the earlier one first: what the report's table must count.
GRIM_LINES = [
    *[l for v in range(102, 108) for l in (f"Simulation (x{v}.0) .420000", f"GroundSpoof (x{v - 73}.0) claimed false")],
    "Simulation (x108.0) .420000",
    "Post (x4.0) entity action v26.3", "Simulation (x166.0) .005880", "Simulation (x167.0) .005872",
    "Hitboxes (x3.0) type=player", "PacketOrderO (x22.0) type=ENTITY_ACTION", "Simulation (x168.0) .005880",
    "Simulation (x169.0) .005880", "Simulation (x170.0) .005880", "GroundSpoof (x45.0) claimed false",
    "Simulation (x171.0) .062500", "PacketOrderO (x23.0) type=ENTITY_ACTION", "Post (x5.0) entity action v26.3",
    "Simulation (x172.0) .005880", "BadPacketsX (x0.0)", "Simulation (x173.0) .003138",
]
GRIM_COUNTS = {"Simulation": (15, "173.0"), "GroundSpoof": (7, "45.0"), "Post": (2, "5.0"), "PacketOrderO": (2, "23.0"),
               "Hitboxes": (1, "3.0"), "BadPacketsX": (1, "0.0")}
REPORT_SECTIONS = ["# Sunset debug report", "## Versions and server", "## Player", "## Input", "## Modules",
                   "## Anticheat flags", "### Server position packets", "### Disconnects and kicks", "## Packets sent",
                   "## Chat"]


def game_display():
    """The X display the session's game draws on (runall gives each target its own), read from its environment."""
    home = f"HORIZON_HOME={session_dir()}/home".encode()
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            with open(f"/proc/{pid}/environ", "rb") as f:
                env = f.read().split(b"\0")
        except OSError:
            continue
        if home in env:
            return next((e[8:].decode() for e in env if e.startswith(b"DISPLAY=")), None)
    return None


def debug_report(trigger):
    """Runs a debug report (trigger: a callable) and returns (the chat line, the saved file's text or None)."""
    mark = log_mark()
    trigger()
    wait(10)
    said = [l for l in sunset_said(mark) if l.startswith("Debug report copied")]
    if not said or " saved to " not in said[-1]:
        return (said[-1] if said else None), None
    with open(said[-1].split(" saved to ", 1)[1]) as f:
        return said[-1], f.read()


def alert_table(text):
    rows = {}
    for line in text.split("## Anticheat flags", 1)[-1].split("\n### ", 1)[0].splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) == 8 and cells[1] == "pointofpressure":
            rows[cells[2]] = (int(cells[3]), cells[4])
    return rows


@test("Debug report")
def _():
    # The Grim lines come in as the server's chat (one coloured like Grim's own), a player's line as player chat;
    # ESP on with one setting changed, W held a few ticks. .debug writes the report: every section, the per-check counts, the changed
    # setting only, the player line not an alert, no colour codes, under 20k characters; the module's row (.t) does
    # the same once and goes off again; the clipboard holds the report where the display can be read.
    reset()
    toggle("ESP", True)
    setting("esp", "fill opacity", "0.5")
    cmd('tellraw @s ["",{"text":"Grim ","color":"aqua"},{"text":"» ","color":"dark_gray"},{"text":"pointofpressure '
        'failed ","color":"white"},{"text":"Simulation (x102.0) ","color":"white"},{"text":".420000","color":"gray"}]')
    cmd(*[f'tellraw @s "Grim » pointofpressure failed {l}"' for l in GRIM_LINES[1:]])
    ctl("chat", "sgudpi")
    wait(5)
    ctl("key", "w", "down")
    wait(4)
    ctl("key", "w", "up")
    wait(3)
    line, text = debug_report(lambda: say(".debug 60"))
    if text is None:
        return False, f"no report file: {line}"
    with open(os.path.join(SHOTS, "debug-report.md"), "w") as f:
        f.write(text)
    missing = [s for s in REPORT_SECTIONS if s not in text]
    counts = alert_table(text)
    counted = all(counts.get(k) == v for k, v in GRIM_COUNTS.items()) and len(counts) == len(GRIM_COUNTS)
    esp = next((l for l in text.splitlines() if l.startswith("- ESP")), "")
    esp_ok = "Fill opacity = 0.5 (default 0.3)" in esp and "Mode =" not in esp
    packets = text.split("## Packets sent", 1)[-1].split("## Chat", 1)[0]
    wire = ("PlayerInput 0x01 (forward)" in packets and "ChatCommand (x" in packets and "MovePos" in packets
            and "Last PlayerInput sent: 0x00 (none) at tick" in text)
    chat = text.split("## Chat", 1)[-1]
    chat_ok = "[player] <" in chat and "sgudpi" in chat and "[server] Grim » pointofpressure failed Simulation" in chat
    clean = "§" not in text and len(text) <= 20500
    say(".menu debug report")
    wait(6)
    ctl("screenshot", f"{SHOTS}/debug_report_menu.png")
    time.sleep(2)
    close_menu()
    _, again = debug_report(lambda: say(".t debugreport"))
    mark = log_mark()
    wait(20)
    once = again is not None and not [l for l in sunset_said(mark) if l.startswith("Debug report copied")]
    display, clip = game_display(), None
    if display and subprocess.run(["which", "xclip"], capture_output=True).returncode == 0:
        r = subprocess.run(["xclip", "-o", "-selection", "clipboard"], capture_output=True, text=True, timeout=10,
                           env={**os.environ, "DISPLAY": display})
        clip = r.stdout.startswith("# Sunset debug report") and "## Anticheat flags" in r.stdout
    ok = not missing and counted and esp_ok and wire and chat_ok and clean and once and clip is not False
    return ok, (f"missing sections {missing}; Grim table {counts}; ESP line {esp!r}; packet log has W's input, the "
                f"commands and moves {wire}; chat lines {chat_ok}; {len(text)} chars, no colour codes {clean}; module row copies once "
                f"and goes off {once}; clipboard {'unchecked' if clip is None else clip}")


@test("Debug report save folder")
def _():
    # The default stays under Sunset's debug folder. A relative setting resolves under Sunset's data folder,
    # while an absolute one is used directly.
    say(".reset debugreport")
    def saved_path():
        line, text = debug_report(lambda: say(".debug"))
        path = line.split(" saved to ", 1)[1] if line and " saved to " in line else None
        return path, text

    default_path, default_text = saved_path()
    if default_path is None or default_text is None:
        return False, "default report was not saved"
    data_dir = os.path.dirname(os.path.dirname(default_path))
    relative_dir = os.path.join(data_dir, "debug reports")
    absolute_dir = os.path.abspath(os.path.join(SHOTS, "sunset-debug-custom"))
    try:
        setting("debugreport", "save folder", "debug reports")
        relative_path, relative_text = saved_path()
        setting("debugreport", "save folder", absolute_dir)
        absolute_path, absolute_text = saved_path()
    finally:
        say(".reset debugreport")
    ok = (os.path.basename(os.path.dirname(default_path)) == "debug"
          and relative_path is not None and os.path.dirname(relative_path) == relative_dir
          and absolute_path is not None and os.path.dirname(absolute_path) == absolute_dir
          and relative_text is not None and absolute_text is not None)
    return ok, f"default {default_path}; relative {relative_path}; absolute {absolute_path}"


@test("Debug report (Grim alerts)")
def _():
    # On the anticheat rig's Grim server (25610) only: Flight held up for 30 s piles up Simulation flags past Grim's
    # alert line (VL 100 here, then every 40) and gets set back. The report's table must count each Grim alert line
    # its chat section holds for Tester, and list the setbacks.
    if not SERVER or not SERVER.endswith(":25610"):
        return None, "needs the anticheat rig's Grim server (127.0.0.1:25610)"
    reset()
    mark = len(ctl("chat-log", "50").get("chat", []))
    cmd("grim alerts")
    wait(5)
    if any("Alerts disabled" in l for l in ctl("chat-log", "5").get("chat", [])[:3]):
        cmd("grim alerts")
    toggle("Flight", True)
    ctl("key", "space", "down")
    ctl("key", "w", "down")
    wait(600)
    ctl("key", "w", "up")
    ctl("key", "space", "up")
    toggle("Flight", False)
    wait(60)
    _, text = debug_report(lambda: say(".debug 300"))
    if text is None:
        return False, f"no report file (chat before: {mark})"
    with open(os.path.join(SHOTS, "debug-report-grim.md"), "w") as f:
        f.write(text)
    rows = {}
    for line in text.split("## Anticheat flags", 1)[-1].split("\n### ", 1)[0].splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) == 8 and cells[0] == "Grim" and cells[1] == "Tester":
            rows[cells[2]] = int(cells[3])
    chat = text.split("## Chat", 1)[-1]
    lines = [l for l in chat.splitlines() if "[server] Grim » Tester failed " in l]
    per_check = {}
    for l in lines:
        check = l.split(" failed ", 1)[1].split(" ")[0]
        per_check[check] = per_check.get(check, 0) + 1
    setbacks = text.split("### Server position packets", 1)[-1].split("###", 1)[0].count("\n| ") - 2
    ok = bool(rows) and rows == per_check and setbacks > 0
    return ok, f"table {rows}; alert lines in chat {per_check}; setbacks listed {setbacks}"


# ---- combat ports, batch 2 (Fake Lag modes, Backtrack, Aim Assist's Silent aim and Hit flick, No Interact's Hit
# fix, No miss delay and Anti action, Shield, Key Throw's Crossbow key, Offhand's Double hand, Auto Weapon's Totem hit,
# W Tap and Jump Reset chances, Sprint's Keep sprint, Auto Web's falling webs, the macros' hurt-time waits)

PUNCH = 46


def move_ticks(mark):
    """Client ticks since mark, and how many of them sent a move packet."""
    ticks = probe_ticks(mark)
    return len(ticks), sum(any(line == "order move" for line in t) for t in ticks)


def strafe(steps=6, each=10):
    for step in range(steps):
        ctl("key", "ad"[step % 2], "down")
        wait(each)
        ctl("key", "ad"[step % 2], "up")


def tick_times(mark):
    """When each client tick went since mark (the wire probe's "tickat" lines, unix seconds)."""
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        return [float(line.split()[1]) for line in f if line.startswith("tickat ")]


@test("Fake Lag (Pulse)")
def _():
    # You strafe for 3 s, without and with Fake Lag on Pulse (chokes of 300 ms): what you send waits and goes at the end
    # of each choke, whole and in order. No move packet is merged or dropped (about as many client ticks carry one as
    # without it), none carries two, the ticks leave in bursts (gaps of 250 ms and more between tick ends), and you
    # stay playing.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    setting("fakelag", "mode", "Pulse")
    setting("fakelag", "shortest choke", "300")
    setting("fakelag", "longest choke", "300")
    setting("fakelag", "pulse gap", "0")
    got = {}
    for name, mods in (("off", ()), ("on", ("fakelag",))):
        cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
        wait(10)
        only(*mods)
        wait(5)
        mark = probe_mark()
        strafe()
        wait(10)
        only()
        wait(10)
        ticks, moved = move_ticks(mark)
        times = tick_times(mark)
        bursts = sum(1 for a, b in zip(times, times[1:]) if b - a >= 0.25)
        got[name] = (moved, ticks, order_stats(mark)[3], bursts)
    wait(10)
    playing = state().get("screen") == "playing"
    note = (f"(ticks with a move packet, ticks, most in a tick, gaps of 250 ms+) off {got['off']}, Pulse {got['on']}; "
            f"still playing {playing}")
    share = lambda g: g[0] / max(g[1], 1)
    ok = (playing and got["on"][2] <= 1 and got["off"][2] <= 1 and share(got["on"]) >= 0.8 * share(got["off"])
          and got["on"][3] >= 4 and got["off"][3] <= 1)
    return ok, note


@test("Fake Lag (teleport)")
def _():
    # Strafing with Fake Lag on Delay (500 ms), the server teleports you three times: what was held goes before the
    # answer, the answer (AcceptTeleportation) goes in the tick the teleport came, no tick carries two move packets, and
    # you end where the last teleport put you, still playing. A baseline run without Fake Lag first.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    setting("fakelag", "mode", "Delay")
    setting("fakelag", "delay", "500")
    got = {}
    for name, mods in (("off", ()), ("on", ("fakelag",))):
        cmd(f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0")
        wait(10)
        only(*mods)
        wait(5)
        mark = probe_mark()
        ctl("key", "a", "down")
        for k in range(3):
            wait(12)
            cmd(f"tp @s {X0 + 0.5 + 2 * (k + 1)} {Y + 1} {Z0 + 0.5} 0 0")
        wait(12)
        ctl("key", "a", "up")
        wait(15)
        only()
        wait(15)
        p = pos()
        ticks = probe_ticks(mark)
        accepted = sum(line == "send teleport" for t in ticks for line in t)
        got[name] = (accepted, order_stats(mark)[3], round(p[0] - X0 - 0.5, 1))
    playing = state().get("screen") == "playing"
    note = f"(teleports accepted, most moves in a tick, x after) off {got['off']}, Delay {got['on']}; still playing {playing}"
    ok = playing and got["on"][0] >= 3 and got["on"][1] <= 1 and got["off"][1] <= 1
    return ok, note


@test("Backtrack")
def _():
    # You hit Bot1 once, then it walks away from you for a second. From both clients' probes: Bot1's z as it has it and
    # as you see it, and how late you see it (the best fitting shift), without and with Backtrack (latency 300 ms):
    # Backtrack holds what the server sends while Bot1 moves away, so you see it later.
    if not CTL2:
        return None, "needs a second player"
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("backtrack", "max range", "10")
    setting("backtrack", "shortest latency", "300")
    setting("backtrack", "longest latency", "300")
    late = {}
    for name, mods in (("off", ()), ("on", ("backtrack",))):
        reset()
        cmd("effect give @s minecraft:resistance 600 4 true")
        slot(0, "wooden_sword")
        ctl("key", "1", "tap")
        bot_ready(2.5, 180)
        bot = next(e["id"] for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:player")
        only(*mods)
        ctl("look-at-entity", str(bot))
        wait(3)
        ctl("mouse", "left", "click")
        wait(5)
        mark, bmark = probe_mark(), bot_probe_mark()
        ctl2("key", "s", "down")
        wait(20)
        ctl2("key", "s", "up")
        wait(20)
        only()
        own = [(f.t, f.eye[2]) for f in bot_probe_frames(bmark)]
        seen = [(f.t, f.mobs[bot][0][2]) for f in probe_frames(mark) if bot in f.mobs]
        late[name] = seen_late(own, seen)
    bot_done()
    if late["off"] is None or late["on"] is None:
        return False, f"no fit: {late}"
    extra = late["on"] - late["off"]
    return 150 <= extra <= 450, f"you see Bot1 walk away {late['off']} ms late without Backtrack, {late['on']} ms with it (+{extra} ms for 300 ms)"


@test("Aim Assist (Silent aim)")
def _():
    # Bot1 2.5 blocks in front, you look 25 degrees to its side (a miss): 3 clicks with a sword. Without Silent aim
    # they hit nothing; with it (Aim Assist on Clicks only) each goes to Bot1, under a look the server gets first.
    if not CTL2:
        return None, "needs a second player"
    setting("aimassist", "mode", "Clicks only")
    setting("aimassist", "silent aim", "true")
    lost = {}
    for name, mods in (("off", ()), ("on", ("aimassist",))):
        reset()
        slot(0, "wooden_sword")
        ctl("key", "1", "tap")
        bot_ready(2.5, 180)
        before = bot_health()
        only(*mods)
        for _ in range(3):
            cmd(f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 3.0} 180 0")
            wait(2)
            ctl("look", "25", "5")
            wait(2)
            ctl("mouse", "left", "click")
            wait(14)
        only()
        lost[name] = round(before - bot_health(), 1)
    bot_done()
    say(".reset aimassist")
    return lost["off"] == 0 and lost["on"] > 0, f"Bot1's health lost to 3 clicks 25 degrees off: {lost['off']} without, {lost['on']} with Silent aim"


@test("Aim Assist (Hit flick)")
def _():
    # A click on Bot1 with Hit flick (60-75 degrees, not only sprinting): the looks the move packets carry after the
    # click turn at least 55 degrees away from Bot1 and then back within 15, before the hit, which lands; never two
    # move packets in a tick.
    if not CTL2:
        return None, "needs a second player"
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("aimassist", "mode", "Clicks only")
    setting("aimassist", "hit flick", "true")
    setting("aimassist", "flick needs sprint", "false")
    reset()
    slot(0, "wooden_sword")
    ctl("key", "1", "tap")
    bot_ready(2.5, 180)
    before = bot_health()
    only("aimassist")
    ctl("look", "0", "5")
    wait(3)
    mark = probe_mark()
    ctl("mouse", "left", "click")
    wait(15)
    only()
    lost = round(before - bot_health(), 1)
    bot_done()
    say(".reset aimassist")
    with open(probe_path(), errors="replace") as f:
        f.seek(mark)
        lines = [line.split() for line in f if line.startswith(("look ", "order attack"))]
    hit = next((i for i, w in enumerate(lines) if w[0] == "order"), None)
    looks = [abs(wrap(float(w[1]))) for w in lines[:hit] if w[0] == "look"] if hit is not None else []
    away = max(looks, default=0.0)
    back = looks[-1] if looks else None
    most = order_stats(mark)[3]
    ok = lost > 0 and away >= 55 and back is not None and back <= 15 and most <= 1
    return ok, f"looks before the hit (degrees off Bot1) {[round(a) for a in looks]}, Bot1 lost {lost}, most moves in a tick {most}"


@test("No Interact (No miss delay)")
def _():
    # 5 sword swings at the sky: each sends a swing packet without No miss delay, none with it.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("nointeract", "no miss delay", "true")
    got = {}
    for name, mods in (("off", ()), ("on", ("nointeract",))):
        reset()
        slot(0, "wooden_sword")
        ctl("key", "1", "tap")
        only(*mods)
        ctl("look", "0", "-80")
        wait(3)
        mark = probe_mark()
        for _ in range(5):
            ctl("mouse", "left", "click")
            wait(4)
        only()
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            got[name] = sum(1 for line in f if line.strip() == f"pkt {PUNCH}")
    return got["off"] >= 5 and got["on"] == 0, f"swing packets for 5 swings at nothing: {got['off']} without, {got['on']} with No miss delay"


@test("No Interact (Hit fix)")
def _():
    # Two clicks on Bot1 (2 blocks away, so the first hit's knockback leaves it in reach) 3 ticks apart with a sword:
    # the second lands in its hurt time and does nothing without Hit fix; with it, it waits until the hurt time is over
    # and lands then.
    if not CTL2:
        return None, "needs a second player"
    setting("nointeract", "hit fix", "true")
    lost = {}
    for name, mods in (("off", ()), ("on", ("nointeract",))):
        reset()
        slot(0, "stone_sword")
        ctl("key", "1", "tap")
        bot_ready(2.0, 180)
        bot = next(e["id"] for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:player")
        before = bot_health()
        only(*mods)
        ctl("look-at-entity", str(bot))
        wait(15)
        ctl("mouse", "left", "click")
        wait(3)
        ctl("look-at-entity", str(bot))
        ctl("mouse", "left", "click")
        wait(30)
        only()
        lost[name] = round(before - bot_health(), 1)
    bot_done()
    return lost["on"] > lost["off"] + 0.5, f"Bot1's health lost to two clicks 3 ticks apart: {lost['off']} without, {lost['on']} with Hit fix"


@test("No Interact (Anti action)")
def _():
    # Glowstone in hand, a right click on the floor in front: it is put down without Anti action, not with it.
    setting("nointeract", "anti action", "true")
    got = {}
    for name, mods in (("off", ()), ("on", ("nointeract",))):
        reset()
        slot(0, "glowstone", 8)
        ctl("key", "1", "tap")
        only(*mods)
        ctl("look-at", str(X0 + 0.5), str(Y + 1.0), str(Z0 + 2.5))
        wait(3)
        ctl("mouse", "right", "click")
        wait(10)
        only()
        got[name] = check(f"execute if block {X0} {Y + 1} {Z0 + 2} minecraft:glowstone")
        cmd(f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:air")
    return got["off"] and not got["on"], f"glowstone put on the floor: {got['off']} without, {got['on']} with Anti action"


@test("Shield")
def _():
    # Swap key: a shield in the main inventory comes into your hand on G and goes back on the next G. Auto block: a
    # sword in hand and a shield in the offhand, Bot1 with a sword 2.5 blocks away: the shield goes up (UseItem sent)
    # with Shield on, not without.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    reset()
    slot(0, "stone_sword")
    ctl("key", "1", "tap")
    cmd("item replace entity @s inventory.11 with minecraft:shield")
    setting("shield", "auto block", "false")
    setting("shield", "swap key", "g")
    only("shield")
    wait(5)
    ctl("key", "g", "tap")
    wait(10)
    swapped = (held().get("item") or "").endswith("shield")
    ctl("key", "g", "tap")
    wait(10)
    back = (held().get("item") or "").endswith("stone_sword")
    only()
    note = f"Swap key: shield in hand {swapped}, sword back {back}"
    if not CTL2:
        return swapped and back, note + "; Auto block needs a second player"
    setting("shield", "auto block", "true")
    cmd("item replace entity @s weapon.offhand with minecraft:shield")
    uses = {}
    for name, mods in (("off", ()), ("on", ("shield",))):
        bot_ready(2.5, 180, "give Bot1 minecraft:iron_sword")
        ctl2("select", "1")
        mark = probe_mark()
        only(*mods)
        wait(30)
        only()
        with open(probe_path(), errors="replace") as f:
            f.seek(mark)
            uses[name] = sum(1 for line in f if line.strip() == f"pkt {USE_ITEM}")
    bot_done()
    ok = swapped and back and uses["off"] == 0 and uses["on"] >= 1
    return ok, note + f"; shield raises (UseItem) in 1.5 s with Bot1 holding a sword near: {uses['off']} without, {uses['on']} with Auto block"


@test("Key Throw (Crossbow key)")
def _():
    # A crossbow in the main inventory and arrows: on the Crossbow key it comes into your hand and is loaded (an arrow
    # used); holding another slot puts it back in the inventory.
    reset()
    slot(0, "stone_sword")
    ctl("key", "1", "tap")
    cmd("item replace entity @s inventory.11 with minecraft:crossbow", "item replace entity @s inventory.12 with minecraft:arrow 16")
    setting("keythrow", "crossbow key", "g")
    only("keythrow")
    wait(5)
    ctl("key", "g", "tap")
    wait(45)
    inhand = (held().get("item") or "").endswith("crossbow")
    arrows = ctl("inventory").get("counts", {}).get("minecraft:arrow", 0)
    ctl("key", "2", "tap")
    wait(10)
    ctl("key", "1", "tap")
    wait(5)
    back = (held().get("item") or "").endswith("stone_sword")
    only()
    return inhand and arrows == 15 and back, f"crossbow in hand {inhand}, arrows left {arrows} of 16 (one loaded), sword back in slot 1 {back}"


@test("Offhand (Double hand)")
def _():
    # 6 health, a totem in hotbar slot 3, Bot1 3 blocks away and an end crystal beside you: Double hand puts the totem
    # in hand (the crystal could take more than you have); without Double hand it stays where it is.
    if not CTL2:
        return None, "needs a second player"
    setting("offhand", "double hand", "true")
    setting("offhand", "item", "None")
    got = {}
    for name, mods in (("off", ()), ("on", ("offhand",))):
        reset()
        slot(0, "stone_sword")
        slot(2, "totem_of_undying")
        ctl("key", "1", "tap")
        bot_ready(3.0, 180)
        cmd(f"setblock {X0 + 2} {Y} {Z0} minecraft:obsidian", f"summon end_crystal {X0 + 2.5} {Y + 1} {Z0 + 0.5} {{ShowBottom:0b}}")
        cmd("effect clear @s", "damage @s 14 minecraft:generic")
        wait(5)
        only(*mods)
        wait(10)
        only()
        got[name] = state().get("selected")  # 1 to 9
        cmd("kill @e[type=end_crystal]", f"setblock {X0 + 2} {Y} {Z0} minecraft:stone", "effect give @s minecraft:instant_health 1 5")
    bot_done()
    say(".reset offhand")
    return got["off"] == 1 and got["on"] == 3, f"selected slot (totem in 3): {got['off']} without, {got['on']} with Double hand"


@test("Auto Weapon (Totem hit)")
def _():
    # A totem in hand and a Knockback II sword in slot 1: one click on Bot1, without and with Totem hit. With it Auto
    # Weapon puts the sword in hand for the hit (in the hit's own tick, so the totem's charge counts and the sword's
    # Knockback rides along) and the totem comes back: Bot1 goes further.
    if not CTL2:
        return None, "needs a second player"
    pushed = {}
    for name, value in (("off", "false"), ("on", "true")):
        setting("autoweapon", "totem hit", value)
        reset()
        slot(0, "diamond_sword[enchantments={knockback:2}]")
        slot(1, "totem_of_undying")
        ctl("key", "2", "tap")
        bot_ready(2.5, 180)
        bot = next(e["id"] for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:player")
        only("autoweapon")
        wait(15)
        z = ctl2("state")["player"]["position"][2]
        ctl("look-at-entity", str(bot))
        mark = probe_mark() if probe_path() else None
        ctl("mouse", "left", "click")
        wait(15)
        # The click's tick: a slot change before its hit.
        swapped = mark is not None and any(
            "order attack" in t and f"pkt {SET_CARRIED_ITEM}" in t[:t.index("order attack")] for t in probe_ticks(mark))
        pushed[name] = (round(ctl2("state")["player"]["position"][2] - z, 2), state().get("selected"), swapped)
        only()
    bot_done()
    # Single player's server moves Bot1 for no single click here; elsewhere the Knockback must show.
    moved = pushed["off"][0] > 0.1
    ok = pushed["on"][2] and not pushed["off"][2] and pushed["on"][1] == 2 and (not moved or pushed["on"][0] > pushed["off"][0] + 1.0)
    return ok, (f"(Bot1 pushed back, slot held after, sword swapped in before the hit) one click holding a totem: "
                f"{pushed['off']} without, {pushed['on']} with Totem hit")


@test("W Tap and Jump Reset (Chance)")
def _():
    # With Chance 0 neither does anything. W Tap: as its own test, sprinting hits on Bot1 with Kill Aura give no sprint
    # hit after the first (as with W Tap off; servers that give none after the first either way say nothing). Jump
    # Reset: Bot1 hits you 3 times off and 3 times at 0%: you go back as far either way.
    if not CTL2:
        return None, "needs a second player"
    setting("killaura", "entities", "player")
    setting("wtap", "chance", "0")
    reset(z=-13.5)
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    bot_ready(-10.5, 180)
    sound = heard("minecraft:entity.player.attack.knockback")
    pushes = after_hits(("killaura", "wtap"), 60, lambda: ctl2("state")["player"]["position"][2], bot_health, ticks_after=3)
    sprints = heard("minecraft:entity.player.attack.knockback") - sound
    setting("jumpreset", "chance", "0")
    back = {}
    for name, mods in (("off", ()), ("0%", ("jumpreset",))):
        reset(z=-13.5)
        cmd("effect give @s minecraft:resistance 600 3 true")
        bot_ready(-11.0, 180, "give Bot1 minecraft:stick[enchantments={knockback:1}]")
        ctl2("select", "1")
        tester = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
        only(*mods)
        back[name] = []
        for _ in range(3):
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
        only()
    bot_done()
    jump_ok = len(back["off"]) >= 2 and len(back["0%"]) >= 2 and mean(back["0%"]) > mean(back["off"]) - 0.25
    tap_ok = len(pushes) < 3 or sprints <= 1
    note = (f"W Tap at 0%: {sprints} sprint hits in {len(pushes)}; Jump Reset knocked you back (blocks) off "
            f"{mean(back['off'])} {back['off']}, at 0% {mean(back['0%'])} {back['0%']}")
    return jump_ok and tap_ok, note


@test("Sprint (Keep sprint)")
def _():
    # As W Tap's test: you sprint at Bot1 with Kill Aura for 3 s, Sprint on, without and with Keep sprint (1-2 ticks):
    # after each hit forward is let go and pressed again, so more of the hits are sprint hits.
    if not CTL2:
        return None, "needs a second player"
    setting("killaura", "entities", "player")
    runs = {}
    for name, value in (("off", "false"), ("on", "true")):
        setting("sprint", "keep sprint", value)
        reset(z=-13.5)
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        bot_ready(-10.5, 180)
        sound, mark = heard("minecraft:entity.player.attack.knockback"), probe_mark() if probe_path() else None
        pushes = after_hits(("killaura", "sprint"), 60, lambda: ctl2("state")["player"]["position"][2], bot_health, ticks_after=3)
        commands = 0
        if mark is not None:
            with open(probe_path(), errors="replace") as f:
                f.seek(mark)
                commands = sum(1 for line in f if line.strip() == "pkt 42")
        runs[name] = (heard("minecraft:entity.player.attack.knockback") - sound, len(pushes), commands)
    bot_done()
    say(".reset sprint")
    note = f"(sprint hits, hits, sprint commands sent): off {runs['off']}, Keep sprint {runs['on']}"
    # Each hit with Keep sprint stops the sprint and starts it again (2 commands); without it the sprint stops once.
    # Whether the server then counts the next hit as a sprint hit varies with the rig's load (W Tap's test shows it).
    return runs["on"][1] >= 2 and runs["on"][2] >= runs["off"][2] + 2 * (runs["on"][1] - 1), note


@test("Auto Web (Only when falling)")
def _():
    # Bot1 drops from 6 blocks up 3 blocks in front: with Only when falling, Auto Web puts a web where it will land
    # before it gets there; without, none goes down while it falls.
    if not CTL2:
        return None, "needs a second player"
    setting("autoweb", "only when falling", "true")
    setting("autoweb", "rotate", "false")
    got = {}
    for name, mods in (("off", ()), ("on", ("autoweb",))):
        reset()
        slot(0, "cobweb", 8)
        bot_ready(3.0, 180)
        only(*mods)
        cmd(f"tp Bot1 {X0 + 0.5} {Y + 7} {Z0 + 3.5} 180 0")
        wait(30)
        only()
        got[name] = check(f"execute if block {X0} {Y + 1} {Z0 + 3} minecraft:cobweb")
        cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 2} {X0 + 1} {Y + 3} {Z0 + 4} minecraft:air")
    bot_done()
    say(".reset autoweb")
    return got["on"] and not got["off"], f"web where Bot1 lands: {got['off']} without, {got['on']} with Only when falling"


@test("Auto Spear (Spear damage indicator)")
def _():
    # A visual: charging a spear with the Damage indicator on draws its box under the crosshair (a screenshot).
    reset()
    slot(0, "iron_spear")
    ctl("key", "1", "tap")
    setting("autospear", "damage indicator", "true")
    only("autospear")
    ctl("mouse", "right", "down")
    wait(15)
    path = f"{SHOTS}/spear-indicator.png"
    ctl("screenshot", path)
    wait(5)
    ctl("mouse", "right", "up")
    only()
    say(".reset autospear")
    return None, f"look at {path}"


def items_of(item):
    return ctl("inventory").get("counts", {}).get(f"minecraft:{item}", 0)


def probe_looks(mark):
    """The looks the move packets carried since `mark`: [(yaw, pitch)]."""
    return [(float(w[1]), float(w[2])) for t in probe_ticks(mark) for w in (line.split() for line in t) if w[0] == "look"]


@test("Key Throw (Short pearl key)")
def _():
    # A sword in hand and pearls in slot 5: the Short pearl key throws one down ahead of you (Short pearl pitch 70) and
    # the sword stays. Silent: the pitch only on the wire (a move packet's look near 70), the slot and the throw before
    # the tick's move packet. Legit: the camera itself turns down and back, the slot is held and comes back. Each takes
    # you 0.3 to 6 blocks.
    got, fine = {}, True
    for mode in ("Silent", "Legit"):
        reset()
        slot(0, "diamond_sword")
        slot(4, "ender_pearl", 16)
        ctl("key", "1", "tap")
        ctl("look", "0", "0")
        setting("keythrow", "short pearl key", "G")
        setting("keythrow", "throw mode", mode)
        only("keythrow")
        wait(5)
        a, mark = pos(), probe_mark()
        ctl("key", "g", "tap")
        pitches = poll(60, 1, lambda: aim_view()[1])
        b = pos()
        only()
        moved = round(math.dist((a[0], a[2]), (b[0], b[2])), 2)
        late, doubled, crowded = cart_order(mark)
        wire = [p for _, p in probe_looks(mark)]
        down = max(pitches) if mode == "Legit" else max(wire, default=0)
        got[mode] = (moved, items_of("ender_pearl"), held().get("item", "").split(":")[-1], round(down, 1), round(pitches[-1], 1),
                     late, doubled, crowded)
        fine &= 0.3 < moved < 6 and got[mode][1] == 15 and "sword" in got[mode][2] and 68 <= down <= 71.5
        fine &= abs(pitches[-1]) < 1 and not doubled and not crowded and (mode == "Legit" or not late)
        fine &= (max(pitches) < 1) if mode == "Silent" else True
    setting("keythrow", "short pearl key", "None")
    setting("keythrow", "throw mode", "Silent")
    return fine, ("(moved, pearls left, in hand, lowest look, camera pitch after, actions after the move, two slot changes "
                  f"in a tick, two moves): {got}")


@test("Key Throw (Auto short pearl)")
def _():
    # Pearls in slot 5, a sword held: Bot1 hits you once; with Auto short pearl a pearl goes straight down at your
    # feet 30 to 70 ms later (one pearl used, the sword in hand after); without it, none.
    if not CTL2:
        return None, "needs a second player"
    setting("keythrow", "auto short pearl", "true")
    used = {}
    for name, mods in (("off", ()), ("on", ("keythrow",))):
        reset()
        cmd("effect give @s minecraft:resistance 600 3 true")
        slot(0, "diamond_sword")
        slot(4, "ender_pearl", 16)
        ctl("key", "1", "tap")
        bot_ready(2.5, 180, "give Bot1 minecraft:stick")
        tester = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
        only(*mods)
        wait(5)
        mark = probe_mark()
        ctl2("look-at-entity", str(tester))
        ctl2("mouse", "left", "click")
        wait(40)
        only()
        late, doubled, crowded = cart_order(mark)
        used[name] = (16 - items_of("ender_pearl"), held().get("item", "").split(":")[-1], late, doubled, crowded)
    bot_done()
    setting("keythrow", "auto short pearl", "false")
    on = used["on"]
    ok = used["off"][0] == 0 and on[0] == 1 and "sword" in on[1] and not any(on[2:])
    return ok, f"(pearls thrown, in hand after, actions after the move, two slot changes, two moves): {used}"


@test("Key Throw (Wind charge after pearl)")
def _():
    # The Pearl key throws a pearl 15 degrees up ahead; with Wind charge after pearl a wind charge goes straight down
    # the moment the pearl takes you (and you jump), so you go up from where you land; without it, nothing more.
    got = {}
    for name, value in (("off", "false"), ("on", "true")):
        reset()
        cmd(f"fill {X0-3} {Y} {Z0} {X0+3} {Y} {Z0+30} minecraft:stone", f"fill {X0-3} {Y+1} {Z0} {X0+3} {Y+20} {Z0+30} minecraft:air")
        slot(0, "diamond_sword")
        slot(4, "ender_pearl", 16)
        slot(5, "wind_charge", 16)
        ctl("key", "1", "tap")
        ctl("look", "0", "-15")
        setting("keythrow", "pearl key", "G")
        setting("keythrow", "wind charge after pearl", value)
        only("keythrow")
        wait(5)
        a = pos()
        ctl("key", "g", "tap")
        landed, top = None, None
        for t in range(120):
            wait(1)
            b = pos()
            if landed is None and math.dist(a, b) > 3:
                landed, top = b[1], b[1]
            if landed is not None:
                top = max(top, b[1])
            a = b
        only()
        got[name] = (16 - items_of("wind_charge"), None if landed is None else round(top - landed, 1))
    setting("keythrow", "pearl key", "None")
    setting("keythrow", "wind charge after pearl", "false")
    ok = got["off"][0] == 0 and got["on"][0] == 1 and (got["on"][1] or 0) > (got["off"][1] or 0) + 1
    return ok, f"(wind charges used, rise after the pearl took you): off {got['off']}, on {got['on']}"


@test("Key Throw (Pearl follow)")
def _():
    # Bot1, 10 blocks ahead, throws a pearl off to your right (it lands some 20 blocks from you): with Pearl follow you
    # throw one of yours after it 50 to 100 ms later and land within 3 blocks of where Bot1 lands; without it you stay.
    if not CTL2:
        return None, "needs a second player"
    setting("keythrow", "pearl follow", "true")
    got = {}
    for name, mods in (("off", ()), ("on", ("keythrow",))):
        reset()
        cmd(f"fill {X0-4} {Y} {Z0-4} {X0+34} {Y} {Z0+14} minecraft:stone", f"fill {X0-4} {Y+1} {Z0-4} {X0+34} {Y+20} {Z0+14} minecraft:air")
        slot(0, "diamond_sword")
        slot(4, "ender_pearl", 16)
        ctl("key", "1", "tap")
        bot_ready(10.5, -90, "give Bot1 minecraft:ender_pearl 4")
        ctl2("select", "1")
        only(*mods)
        wait(10)
        mark = probe_mark()
        cmd(f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 10.5} -90 -10")
        wait(5)
        ctl2("mouse", "right", "down")
        wait(3)
        ctl2("mouse", "right", "up")
        wait(80)
        only()
        me = pos()
        bot = (ctl2("state").get("player") or {}).get("position") or [0, 0, 0]
        late, doubled, crowded = cart_order(mark)
        got[name] = (16 - items_of("ender_pearl"), round(math.dist((me[0], me[2]), (bot[0], bot[2])), 1),
                     round(math.dist((me[0], me[2]), (X0 + 0.5, Z0 + 0.5)), 1), late, doubled, crowded)
    bot_done()
    setting("keythrow", "pearl follow", "false")
    on = got["on"]
    ok = got["off"][0] == 0 and on[0] == 1 and on[1] <= 3 and on[2] > 8 and not any(on[3:])
    return ok, f"(pearls thrown, from Bot1 after, from the start, actions after the move, two slot changes, two moves): {got}"


@test("Pearl Catch (Catch key)")
def _():
    # The Catch key in each Key mode, a sword held, pearls in slot 5 and wind charges in slot 6, looking 30 degrees up
    # (60 for the Diagonal ones): one pearl and one wind charge go, the sword is in hand after, the charge catches the
    # pearl (it takes you more than 5 blocks up; Diagonal macro's fixed delay along your look need not), all of it before
    # the ticks' move packets and one slot change a tick. A mode gets a second try if the first misses.
    setting("pearlcatch", "catch key", "G")
    got, fine = {}, True

    def attempt(mode):
        reset()
        # A catch 20 up and the fall after it would kill you.
        cmd("effect give @s minecraft:resistance 600 4 true")
        cmd(f"fill {X0-6} {Y} {Z0-6} {X0+6} {Y} {Z0+40} minecraft:stone", f"fill {X0-6} {Y+1} {Z0-6} {X0+6} {Y+60} {Z0+40} minecraft:air")
        slot(0, "diamond_sword")
        slot(4, "ender_pearl", 16)
        slot(5, "wind_charge", 16)
        cmd("item replace entity @s weapon.offhand with minecraft:shield")
        ctl("key", "1", "tap")
        ctl("look", "0", "-60" if mode.startswith("Diagonal") else "-30")
        setting("pearlcatch", "key mode", mode)
        only("pearlcatch")
        wait(5)
        a, ground, mark = pos(), pos()[1], probe_mark()
        ctl("key", "g", "tap")
        took, top = None, ground
        for t in range(100):
            wait(1)
            b = pos()
            if took is None and math.dist(a, b) > 3:
                took = t
            top, a = max(top, b[1]), b
        only()
        late, doubled, crowded = cart_order(mark)
        off = (ctl("inventory").get("offhand") or {}).get("item") or ""
        return (16 - items_of("ender_pearl"), 16 - items_of("wind_charge"), (held().get("item") or "").split(":")[-1], took,
                round(top - ground, 1), off.split(":")[-1], late, doubled, crowded)

    for mode in ("Up", "Fast", "Offhand", "Diagonal", "Diagonal macro"):
        tries = [attempt(mode)]
        if tries[0][4] <= 5 and mode != "Diagonal macro":
            tries.append(attempt(mode))
        r = tries[-1]
        got[mode] = (len(tries), *r)
        fine &= r[:2] == (1, 1) and "sword" in r[2] and "shield" in r[5] and not any(r[6:])
        fine &= r[4] > 5 or mode == "Diagonal macro"
        cmd("effect give @s minecraft:instant_health 1 10 true")
    setting("pearlcatch", "catch key", "None")
    setting("pearlcatch", "key mode", "Up")
    return fine, ("(tries, pearls, wind charges used, in hand, tick the pearl took you, highest, offhand after, actions "
                  f"after the move, two slot changes, two moves): {got}")


@test("Wind Jump (Elytra)")
def _():
    # An elytra in slot 2 and wind charges in slot 3, wearing nothing: Wind Jump's Elytra puts the elytra on, jumps,
    # opens it in the air, and throws a wind charge down as the glide nears the ground: in 200 ticks the elytra is worn,
    # you glided, and charges went (each before its tick's move packet).
    reset()
    cmd(f"fill {X0-20} {Y} {Z0-20} {X0+20} {Y} {Z0+60} minecraft:stone", f"fill {X0-20} {Y+1} {Z0-20} {X0+20} {Y+40} {Z0+60} minecraft:air")
    slot(0, "diamond_sword")
    slot(1, "elytra")
    slot(2, "wind_charge", 64)
    ctl("key", "1", "tap")
    ctl("look", "0", "0")
    setting("windjump", "elytra", "true")
    only("windjump")
    mark, ground = probe_mark(), pos()[1]
    states = poll(200, 1, lambda: state()["player"])
    only()
    setting("windjump", "elytra", "false")
    chest = (ctl("inventory").get("armor") or [None, None, None, None])
    worn = "elytra" in json.dumps(chest)
    glided = sum(1 for p in states if p.get("fall_flying"))
    top = round(max(p["position"][1] for p in states) - ground, 1)
    used = 64 - items_of("wind_charge")
    late, doubled, crowded = cart_order(mark)
    cmd("item replace entity @s armor.chest with minecraft:air")
    ok = worn and glided > 5 and used >= 1 and not late and not doubled and not crowded
    return ok, (f"elytra worn {worn}, ticks gliding {glided}, wind charges {used}, highest {top}; actions after the move "
                f"{late}, two slot changes {doubled}, two moves {crowded}")


@test("Wind Jump (Hit charges)")
def _():
    # Bot1, 10 blocks ahead, throws wind charges at you: with Hit charges (In air only off) you hit them (an attack
    # with the server's look on the charge, before the tick's move packet); without it, no attack.
    if not CTL2:
        return None, "needs a second player"
    setting("windjump", "hit charges", "true")
    setting("windjump", "in air only", "false")
    got = {}
    for name, mods in (("off", ()), ("on", ("windjump",))):
        reset()
        cmd("effect give @s minecraft:resistance 600 3 true")
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        bot_ready(10.5, 180, "give Bot1 minecraft:wind_charge 16")
        ctl2("select", "1")
        tester = next(e["id"] for e in ctl2("entities").get("entities", []) if e.get("type") == "minecraft:player")
        only(*mods)
        wait(5)
        mark = probe_mark()
        for _ in range(3):
            ctl2("look-at-entity", str(tester))
            ctl2("mouse", "right", "down")
            wait(2)
            ctl2("mouse", "right", "up")
            wait(25)
        only()
        hits = sum(t.count("order attack") for t in probe_ticks(mark))
        late, doubled, crowded = cart_order(mark)
        got[name] = (hits, late, doubled, crowded)
        # Charges knocked away fly on: one would catch the next test's pearl.
        cmd("kill @e[type=minecraft:wind_charge]")
    bot_done()
    setting("windjump", "hit charges", "false")
    setting("windjump", "in air only", "true")
    ok = got["off"][0] == 0 and got["on"][0] >= 1 and not any(got["on"][1:])
    return ok, f"(attacks sent, actions after the move, two slot changes, two moves): off {got['off']}, on {got['on']}"


@test("Anchor Aura (Pearl catch)")
def _():
    # Bot1, 30 blocks ahead, throws a pearl toward you (it lands about 4 blocks ahead): Pearl catch puts an anchor by
    # the landing just before it, charges it and sets it off once Bot1 is there; Bot1 loses more than the pearl's 5.
    # You hold anchors, glowstone and a totem, with Resistance IV and 100 health. Aura is off.
    if not CTL2:
        return None, "needs a second player"
    reset()
    setting("anchoraura", "aura", "false")
    setting("anchoraura", "pearl catch", "true")
    setting("anchoraura", "catch min damage", "4")
    setting("anchoraura", "catch max self damage", "30")
    cmd("difficulty normal", "attribute @s minecraft:max_health base set 100", "effect give @s minecraft:instant_health 1 28 true",
        "effect give @s minecraft:resistance 120 3 true")
    slot(0, "totem_of_undying")
    slot(1, "respawn_anchor", 8)
    slot(2, "glowstone", 16)
    slot(3, "obsidian", 16)
    ctl("key", "1", "tap")
    bot_respawned()
    cmd("gamemode survival Bot1", "clear Bot1", "effect clear Bot1", "attribute Bot1 minecraft:max_health base set 100",
        "effect give Bot1 minecraft:instant_health 1 28 true",
        f"fill {X0-1} {Y} {Z0+29} {X0+1} {Y} {Z0+31} minecraft:stone", f"fill {X0-1} {Y+1} {Z0+29} {X0+1} {Y+3} {Z0+31} minecraft:air",
        f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 30.5} 180 -10", "give Bot1 minecraft:ender_pearl 4")
    wait(20)
    ctl2("select", "1")
    wait(5)
    before, mark = bot_health(), probe_mark()
    only("anchoraura")
    wait(5)
    ctl2("mouse", "right", "down")
    wait(3)
    ctl2("mouse", "right", "up")
    anchors = 0
    for _ in range(40):
        anchors = max(anchors, count(X0 - 4, Y - 1, Z0, X0 + 4, Y + 3, Z0 + 10, "respawn_anchor"))
        wait(1)
    wait(20)
    lost = round(before - bot_health(), 1)
    used = 8 - items_of("respawn_anchor")
    late, doubled, crowded = cart_order(mark)
    only()
    bot_done()
    blast_done()
    for name in ("aura",):
        setting("anchoraura", name, "true")
    setting("anchoraura", "pearl catch", "false")
    hurt = lost > 5 or not SERVER
    ok = used >= 1 and hurt and not late and not doubled and not crowded
    return ok, (f"anchors used {used}, seen {anchors}; Bot1 lost {lost} (5 of it the pearl's); actions after the move "
                f"{late}, two slot changes {doubled}, two moves {crowded}")


@test("Trajectories (crossbow, pearls in flight)")
def _():
    # A loaded multishot crossbow held: Trajectories draws three arrow paths (more accent pixels than one plain
    # arrow's); with Pearls in flight, a pearl Bot1 throws gets its path and landing time drawn while it flies.
    # Screenshots in the shots folder.
    setting("trajectories", "pearls in flight", "true")
    reset()
    ctl("look", "0", "-10")
    accent = lambda r, g, b: r > 200 and 60 < g < 140 and b < 60
    got = {}
    for name, item in (("single", "crossbow[charged_projectiles=[{id:arrow}]]"),
                       ("multishot", "crossbow[charged_projectiles=[{id:arrow},{id:arrow},{id:arrow}],enchantments={multishot:1}]")):
        slot(0, item)
        ctl("key", "1", "tap")
        only("trajectories")
        wait(10)
        got[name] = shot_pixels(f"trajectories-{name}", accent)
        only()
    pearl = None
    if CTL2:
        slot(0, "diamond_sword")
        bot_ready(20.5, 180, "give Bot1 minecraft:ender_pearl 4")
        ctl2("select", "1")
        only("trajectories")
        ctl2("look", "180", "-30")
        wait(5)
        ctl2("mouse", "right", "down")
        wait(2)
        ctl2("mouse", "right", "up")
        wait(4)
        pearl = shot_pixels("trajectories-pearl", accent)
        only()
        bot_done()
    setting("trajectories", "pearls in flight", "false")
    ok = got["single"] > 0 and got["multishot"] > got["single"] * 1.5 and (pearl is None or pearl > 0)
    return ok, f"accent pixels: one arrow {got['single']}, multishot {got['multishot']}, a flying pearl {pearl}"


HEAL_POT = 'splash_potion[potion_contents={potion:"minecraft:healing"}]'


@test("Auto Gap (Auto pot)")
def _():
    # A sword in hand and four splash potions of healing in slots 6 to 9, health knocked down to 6: with Auto pot a potion
    # goes at your feet (Pot pitch 70 on the wire), health comes back up, the sword is in hand after, each throw
    # before its tick's move packet; without it no potion goes.
    setting("autogap", "auto pot", "true")
    setting("autogap", "health enabled", "false")
    got, fine = {}, True
    for name, mods in (("off", ()), ("on", ("autogap",))):
        reset()
        slot(0, "diamond_sword")
        for i in (5, 6, 7, 8):
            slot(i, HEAL_POT)
        ctl("key", "1", "tap")
        cmd("effect clear @s", "damage @s 14 minecraft:generic")
        wait(5)
        low = hp()
        mark = probe_mark() if probe_path() else None
        only(*mods)
        wait(40)
        only()
        pitches = [p for _, p in probe_looks(mark)] if mark is not None else []
        late, doubled, crowded = cart_order(mark) if mark is not None else (0, 0, 0)
        got[name] = (round(low, 1), round(hp(), 1), items_of("splash_potion"), held().get("item", "").split(":")[-1],
                     round(max(pitches, default=0), 1), late, doubled, crowded)
    say(".reset autogap")
    off, on = got["off"], got["on"]
    fine = off[2] == 4 and on[2] < 4 and on[1] > on[0] + 2 and "sword" in on[3] and not any(on[5:])
    fine &= mark is None or 68 <= on[4] <= 71
    return fine, f"(health before, after, potions left, in hand, lowest look, late, two slot changes, two moves): {got}"


@test("Auto Replenish (Inventory open, Key, Potion refill)")
def _():
    # Mode Inventory open: three stone in slot 1 and a stack in the main inventory are not touched with no screen
    # open, and topped up to 64 once the inventory is open. Key: with no screen, topped up only while R is held.
    # Potion refill: a splash potion of healing in the main inventory goes into the empty hotbar with the screen open.
    setting("autoreplenish", "mode", "Inventory open")
    setting("autoreplenish", "threshold", "32")
    setting("autoreplenish", "delay (ticks)", "2")
    setting("autoreplenish", "potion refill", "true")

    def setup():
        reset()
        cmd("item replace entity @s hotbar.0 with minecraft:stone 3", "item replace entity @s inventory.0 with minecraft:stone 64",
            f"item replace entity @s inventory.5 with minecraft:{HEAL_POT}")
        ctl("key", "1", "tap")

    setup()
    only("autoreplenish")
    wait(20)
    closed = held().get("count", 0)
    ctl("open-inventory")
    wait(20)
    inv = ctl("inventory")
    ctl("close")
    opened = (inv.get("hotbar") or [{}])[0].get("count", 0) if (inv.get("hotbar") or [None])[0] else 0
    potion_hotbar = sum(1 for s in (inv.get("hotbar") or []) if s and "splash_potion" in (s.get("item") or ""))
    only()
    setting("autoreplenish", "mode", "Key")
    setting("autoreplenish", "refill key", "R")
    setup()
    only("autoreplenish")
    wait(20)
    unheld = held().get("count", 0)
    ctl("key", "r", "down")
    wait(20)
    ctl("key", "r", "up")
    keyed = held().get("count", 0)
    only()
    say(".reset autoreplenish")
    ok = closed == 3 and opened == 64 and potion_hotbar == 1 and unheld == 3 and keyed == 64
    return ok, (f"Inventory open: {closed} with no screen, {opened} with it open, potions moved to the hotbar {potion_hotbar}; "
                f"Key: {unheld} before R, {keyed} while held")


@test("Inventory Manager (Loot key)")
def _():
    # Three totems (one a slot) and seven pearl stacks: holding the Loot key G with the inventory open drops totems down
    # to Keep totems (1) slots, then pearl stacks down to Keep pearl stacks (5); nothing goes without the key.
    setting("inventorymanager", "loot key", "G")
    setting("inventorymanager", "items", "rotten_flesh")
    reset()
    lines = [f"item replace entity @s inventory.{i} with minecraft:totem_of_undying" for i in range(3)]
    lines += [f"item replace entity @s inventory.{i} with minecraft:ender_pearl 16" for i in range(3, 10)]
    cmd(*lines)
    only("inventorymanager")
    ctl("open-inventory")
    wait(20)
    before = (items_of("totem_of_undying"), items_of("ender_pearl"))
    ctl("key", "g", "down")
    wait(40)
    ctl("key", "g", "up")
    after = (items_of("totem_of_undying"), items_of("ender_pearl"))
    ctl("close")
    only()
    cmd("kill @e[type=item]")
    say(".reset inventorymanager")
    ok = before == (3, 112) and after == (1, 80)
    return ok, f"(totems, pearls) before the key {before}, after holding it {after}"


@test("Auto Tool (Only below you, From inventory)")
def _():
    # A sword held and a pickaxe only in the main inventory: mining the obsidian under you swaps the pickaxe into your
    # hand (From inventory), and letting go swaps it back; with Only below you a stone block ahead isn't swapped for.
    setting("autotool", "only below you", "true")
    setting("autotool", "from inventory", "true")
    reset()
    slot(0, "diamond_sword")
    # Obsidian under you, so the pickaxe can't break it (and drop you into the floor) within the test.
    cmd("item replace entity @s inventory.4 with minecraft:diamond_pickaxe", f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:stone",
        f"setblock {X0} {Y} {Z0} minecraft:obsidian")
    ctl("key", "1", "tap")
    only("autotool")
    ctl("look-at-block", str(X0), str(Y + 1), str(Z0 + 2))
    wait(3)
    ctl("mouse", "left", "down")
    wait(4)
    ahead = held().get("item", "")
    ctl("mouse", "left", "up")
    wait(5)
    ctl("look", "0", "90")
    wait(3)
    ctl("mouse", "left", "down")
    wait(4)
    below = held().get("item", "")
    ctl("mouse", "left", "up")
    wait(6)
    after = held().get("item", "")
    main = json.dumps(ctl("inventory").get("main") or [])
    only()
    cmd(f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:air", f"setblock {X0} {Y} {Z0} minecraft:stone")
    say(".reset autotool")
    ok = "sword" in ahead and "pickaxe" in below and "sword" in after and "diamond_pickaxe" in main
    return ok, f"in hand: mining ahead {ahead}, mining below {below}, after {after}; pickaxe back in the main inventory: {'diamond_pickaxe' in main}"


@test("No Interact (Ghost place)")
def _():
    # A pig in front of a stone wall, cobweb in hand: a right click on the pig puts the web against the wall behind it
    # (on the wall's near face) with Ghost place; without it nothing is placed.
    setting("nointeract", "ghost place", "true")
    got = {}
    for name, mods in (("off", ()), ("on", ("nointeract",))):
        reset()
        cmd(f"fill {X0 - 1} {Y + 1} {Z0 + 3} {X0 + 1} {Y + 3} {Z0 + 3} minecraft:stone",
            f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 1.5} {NOAI}")
        slot(0, "cobweb", 8)
        ctl("key", "1", "tap")
        only(*mods)
        wait(10)
        # Through the top of the pig (a block short of the wall) to the wall's face.
        ctl("look-at", str(X0 + 0.5), str(Y + 1.3), str(Z0 + 3.0))
        wait(3)
        ctl("mouse", "right", "click")
        wait(10)
        only()
        got[name] = count(X0 - 1, Y + 1, Z0, X0 + 1, Y + 3, Z0 + 2, "cobweb")
        cmd(f"fill {X0 - 1} {Y + 1} {Z0} {X0 + 1} {Y + 3} {Z0 + 3} minecraft:air")
    say(".reset nointeract")
    note = f"cobwebs placed by the pig: {got['off']} without, {got['on']} with Ghost place"
    # Horizon's own server (single player's too) runs the use again as you, along your look, and meets the pig first.
    if (HORIZON_SERVER or not SERVER) and got["on"] == 0:
        return None, f"Horizon's server uses it on the pig itself: {note}"
    return got["off"] == 0 and got["on"] == 1, note


@test("No Interact (Anti trap)")
def _():
    # An armor stand 2 blocks ahead, a helmet in hand: a right click on the stand puts the helmet on it. With Anti trap
    # the stand is gone for you (one loaded before: Remove loaded; one summoned after: Drop spawns), so it never gets
    # it. Anti sign: a sign put down opens no editor.
    setting("nointeract", "anti trap", "true")
    got = {}
    for name, mods, before in (("off", (), True), ("loaded", ("nointeract",), True), ("spawned", ("nointeract",), False)):
        setting("nointeract", "remove loaded", "true" if name != "spawned" else "false")
        reset()
        slot(0, "diamond_helmet")
        ctl("key", "1", "tap")
        stand = f"summon armor_stand {X0 + 0.5} {Y + 1} {Z0 + 2.5} {{NoGravity:1b}}"
        if before:
            cmd(stand)
            wait(10)
        only(*mods)
        wait(5)
        if not before:
            cmd(stand)
            wait(10)
        ctl("look-at", str(X0 + 0.5), str(Y + 1.5), str(Z0 + 2.5))
        wait(3)
        ctl("mouse", "right", "click")
        wait(10)
        got[name] = check("execute if items entity @e[type=armor_stand,limit=1] armor.head minecraft:diamond_helmet")
        only()
    reset("creative")
    slot(0, "oak_sign", 2)
    only("nointeract")
    ctl("look", "0", f"{math.degrees(math.atan2(1.62, 2)):.3f}")
    wait(3)
    ctl("mouse", "right", "click")
    wait(10)
    sign = state().get("screen")
    ctl("key", "esc", "tap")
    only()
    cmd(f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:air")
    say(".reset nointeract")
    ok = got["off"] and not got["loaded"] and not got["spawned"] and sign == "playing"
    return ok, f"the stand took the helmet: {got}; screen after putting a sign down with Anti sign: {sign}"


@test("Liquid Filler (Drain)")
def _():
    # Bot1 3 blocks away, a bucket in hand: a water source someone else put 2 blocks ahead (in a hole) is scooped (Legit: under the
    # crosshair; Silent aim: with the camera turned away); water you put down yourself is left. Without an enemy near,
    # nothing is scooped.
    if not CTL2:
        return None, "needs a second player"
    setting("liquidfiller", "mode", "Drain")
    got = {}
    for name, aim, bot in (("legit", "Legit", True), ("silent", "Silent aim", True), ("alone", "Legit", False)):
        setting("liquidfiller", "drain aim", aim)
        reset()
        slot(0, "bucket")
        ctl("key", "1", "tap")
        if bot:
            bot_ready(-3.5, 0)
        # In a hole in the floor, so it stays one source.
        cmd(f"setblock {X0} {Y} {Z0 + 2} minecraft:water")
        ctl("look-at", str(X0 + 0.5), str(Y + 0.8), str(Z0 + 2.5))
        if aim != "Legit":
            ctl("look", "90", "0")
        wait(3)
        mark = probe_mark() if probe_path() else None
        only("liquidfiller")
        wait(20)
        only()
        late = cart_order(mark)[0] if mark is not None else 0
        got[name] = (not check(f"execute if block {X0} {Y} {Z0 + 2} minecraft:water"), held().get("item", "").split(":")[-1], late)
        cmd(f"setblock {X0} {Y} {Z0 + 2} minecraft:stone")
        if bot:
            bot_done()
    # Your own: placed with a water bucket, Bot1 near, Drain on.
    reset()
    slot(0, "water_bucket")
    ctl("key", "1", "tap")
    bot_ready(-3.5, 0)
    setting("liquidfiller", "drain aim", "Legit")
    only("liquidfiller")
    wait(10)
    ctl("look-at", str(X0 + 0.5), str(Y + 0.99), str(Z0 + 2.5))
    wait(3)
    ctl("mouse", "right", "click")
    wait(20)
    own = check(f"execute if block {X0} {Y + 1} {Z0 + 2} minecraft:water")
    only()
    bot_done()
    cmd(f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:air")
    say(".reset liquidfiller")
    ok = got["legit"][0] and got["silent"][0] and not got["alone"][0] and own and not got["legit"][2] and not got["silent"][2]
    return ok, f"(scooped, in hand, actions after the move): {got}; own water left: {own}"


@test("Safe Walk (Sneak)")
def _():
    # On a one-block pillar with a deep drop ahead, stone in hand, looking down: walking forward with Mode Sneak sneaks
    # at the edge and stays up; with Safe Walk off you walk off and fall.
    setting("safewalk", "mode", "Sneak")
    got = {}
    for name, mods in (("off", ()), ("on", ("safewalk",))):
        cmd(f"setblock {X0} {Y + 5} {Z0} minecraft:stone")
        reset(y=Y + 6)
        slot(0, "stone", 16)
        ctl("key", "1", "tap")
        ctl("look", "0", "80")
        only(*mods)
        wait(5)
        walk(30)
        got[name] = round(pos()[1] - (Y + 6), 2)
        only()
        cmd(f"setblock {X0} {Y + 5} {Z0} minecraft:air")
    say(".reset safewalk")
    return got["off"] < -2 and abs(got["on"]) < 0.2, f"height left after walking forward: {got['off']} off, {got['on']} with Sneak"


@test("Parkour (edge distance range)")
def _():
    # Edge distance 0.001 to Longest edge distance 0.01, rolled each side each tick: walking off a block edge still
    # jumps (you end above the floor at some point).
    setting("parkour", "longest edge distance", "0.01")
    reset()
    cmd(f"fill {X0 - 1} {Y} {Z0 + 2} {X0 + 1} {Y} {Z0 + 8} minecraft:air")
    only("parkour")
    ctl("key", "w", "down")
    ys = poll(30, 1, lambda: pos()[1])
    ctl("key", "w", "up")
    only()
    cmd(f"fill {X0 - 1} {Y} {Z0 + 2} {X0 + 1} {Y} {Z0 + 8} minecraft:stone")
    say(".reset parkour")
    rose = round(max(ys) - (Y + 1), 2)
    return rose > 0.5, f"highest above the floor while walking off the edge: {rose}"


@test("Chest Swap (Take off)")
def _():
    # An iron chestplate worn, an elytra and rockets in the main inventory, Auto jump on: Take off puts the elytra on,
    # jumps, opens the glide, fires a rocket, and after Fly time puts the chestplate back and turns itself off; each
    # use before its tick's move packet.
    setting("chestswap", "mode", "Take off")
    setting("chestswap", "auto jump", "true")
    reset()
    cmd(f"fill {X0-20} {Y} {Z0-20} {X0+20} {Y} {Z0+60} minecraft:stone", f"fill {X0-20} {Y+1} {Z0-20} {X0+20} {Y+40} {Z0+60} minecraft:air",
        "item replace entity @s armor.chest with minecraft:iron_chestplate", "item replace entity @s inventory.3 with minecraft:elytra",
        "item replace entity @s inventory.8 with minecraft:firework_rocket 8")
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    ctl("look", "0", "-20")
    wait(5)
    mark = probe_mark() if probe_path() else None
    only("chestswap")
    states = poll(80, 1, lambda: (state()["player"], json.dumps(ctl("inventory").get("armor") or [])))
    glided = sum(1 for p, _ in states if p.get("fall_flying"))
    elytra = any("elytra" in a for _, a in states)
    off = turned_itself_off("chestswap", "Chest Swap")
    only()
    armor = json.dumps(ctl("inventory").get("armor") or [])
    used = 8 - items_of("firework_rocket")
    late = cart_order(mark)[0] if mark is not None else 0
    cmd("item replace entity @s armor.chest with minecraft:air")
    say(".reset chestswap")
    ok = elytra and glided > 3 and used == 1 and "iron_chestplate" in armor and off and not late
    return ok, f"elytra worn {elytra}, ticks gliding {glided}, rockets {used}, chest after {'iron' if 'iron_chestplate' in armor else armor}, off {off}, late {late}"



def clear_chat():
    """F3 + D: the chat lines on screen go (so a toggle's line can't show in a frame)."""
    ctl("key", "f3", "down")
    ctl("key", "d", "tap")
    ctl("key", "f3", "up")
    wait(2)


WALL = (-3, 1, 6, 3, 6, 6)


def back_wall(block="stone"):
    """A wall 6 blocks ahead, behind what a frame looks at, so nothing moves behind it."""
    x1, y1, z1, x2, y2, z2 = WALL
    cmd(f"fill {X0 + x1} {Y + y1} {Z0 + z1} {X0 + x2} {Y + y2} {Z0 + z2} minecraft:{block}")


def on_off(name, mods, setup=None, settle=10, box=(240, 60, 1040, 560)):
    """A frame with `mods` off and one with them on (same scene, chat cleared): how many pixels in `box` changed
    colour. The frames stay in SHOTS as <name>-off.png and <name>-on.png."""
    frames = {}
    for state, turn in (("off", ()), ("on", mods)):
        only(*turn)
        clear_chat()
        if setup:
            setup()
        wait(settle)
        frames[state] = colour_frame(f"{name}-{state}").crop(box)
    only()
    return recoloured(frames["off"], frames["on"])


@test("Target ESP (Ghost, Square)")
def _():
    # A husk 3 blocks ahead under the crosshair, a wall behind: Target ESP's Ghost (trails circling it) and Square (a
    # turning square on it) each draw round it, as Brackets do. Screenshots target-esp-<style>-on.png.
    reset()
    back_wall()
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 3.5} {HUSK}")
    ctl("look-at", str(X0 + 0.5), str(Y + 1.6), str(Z0 + 3.5))
    setting("targetesp", "target", "Crosshair")
    got = {}
    for style in ("Brackets", "Ghost", "Square"):
        setting("targetesp", "style", style)
        got[style] = on_off(f"target-esp-{style.lower()}", ("targetesp",), settle=30)
    back_wall("air")
    say(".reset targetesp")
    return all(v > 200 for v in got.values()), f"pixels changed with each style on: {got}"


@test("Effects (circles, trails, hit particles, weather)")
def _():
    # Jump circles: a jump draws a ring at your feet (looking down). Trails: footsteps behind you after a walk (looking
    # back down). Hit particles: sparks off a husk you hit (a wall behind it). Weather: snow round you (looking down at
    # the floor). Each changes the frame against Effects off.
    reset()
    setting("effects", "trails", "true")
    setting("effects", "hit particles", "true")
    got = {}
    ctl("look", "0", "80")
    got["circle"] = on_off("effects-circle", ("effects",), setup=lambda: (clear_chat(), ctl("jump")), settle=4)

    def walked():
        reset()
        ctl("look", "0", "0")
        walk(15)
        ctl("look", "180", "60")

    got["trail"] = on_off("effects-trail", ("effects",), setup=walked, settle=2)
    reset()
    back_wall()
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 2.5} {HUSK}")
    slot(0, "stick")
    ctl("key", "1", "tap")
    ctl("look-at", str(X0 + 0.5), str(Y + 1.9), str(Z0 + 2.5))

    def hit():
        wait(5)
        ctl("mouse", "left", "click")

    got["hit"] = on_off("effects-hit", ("effects",), setup=hit, settle=4)
    back_wall("air")
    # Snow falling in front of a big wall 9 blocks ahead, looking straight at it.
    big = f"{X0 - 9} {Y + 1} {Z0 + 9} {X0 + 9} {Y + 14} {Z0 + 9}"
    cmd(f"fill {big} minecraft:stone")
    setting("effects", "weather", "true")
    setting("effects", "weather mode", "Snow")
    setting("effects", "trails", "false")
    setting("effects", "jump circles", "false")
    ctl("look", "0", "0")
    got["weather"] = on_off("effects-weather", ("effects",), settle=60)
    cmd(f"fill {big} minecraft:air")
    say(".reset effects")
    return all(v > 150 for v in got.values()), f"pixels changed with Effects on: {got}"


@test("ESP (2D style, health bar, shield colors)")
def _():
    # A husk ahead of a wall: ESP's 2D box with Corners, a health bar, its name and a glow draws differently from the
    # plain 2D box, and the health bar's green shows. With Bot1 holding a shield, Shield colors turns their box Lime.
    reset()
    back_wall()
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 4.5} {HUSK}")
    ctl("look-at", str(X0 + 0.5), str(Y + 1.5), str(Z0 + 4.5))
    setting("esp", "mode", "2D box")
    only("esp")
    clear_chat()
    wait(10)
    plain = colour_frame("esp-2d-plain")
    for k, v in (("2d style", "Corners"), ("health bar", "true"), ("2d names", "true"), ("glow", "true")):
        setting("esp", k, v)
    clear_chat()
    wait(10)
    styled = colour_frame("esp-2d-styled")
    only()
    bar = sum(1 for px in styled.getdata() if 100 < px[0] < 140 and px[1] > 225 and 120 < px[2] < 160)
    lime = None
    if CTL2:
        bot_ready(4.5, 180, "give Bot1 minecraft:shield", "kill @e[type=husk]")
        ctl2("select", "1")
        setting("esp", "shield colors", "true")
        only("esp")
        clear_chat()
        wait(15)
        lime = sum(1 for px in colour_frame("esp-shield").getdata() if 130 < px[0] < 160 and px[1] > 240 and 50 < px[2] < 80)
        only()
        bot_done()
    back_wall("air")
    say(".reset esp")
    diff = recoloured(plain.crop((240, 60, 1040, 560)), styled.crop((240, 60, 1040, 560)))
    ok = diff > 100 and bar > 0 and (lime is None or lime > 20)
    return ok, f"pixels changed by the 2D extras {diff}, health bar green pixels {bar}, Lime shield-box pixels {lime}"


@test("Nametags (Health colors)")
def _():
    # A husk at 8 of 20 health: with Health colors Bands its tag's health shows gold (to 10); Gradient shows yellow.
    reset()
    back_wall()
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 4.5} {{NoAI:1b,Invulnerable:1b,PersistenceRequired:1b,Silent:1b,Health:8f}}")
    setting("nametags", "mobs", "true")
    ctl("look", "0", "-5")
    got = {}
    for mode in ("Gradient", "Bands"):
        setting("nametags", "health colors", mode)
        only("nametags")
        clear_chat()
        wait(10)
        got[mode] = sum(1 for px in colour_frame(f"nametags-{mode.lower()}").getdata() if px[0] > 235 and 150 < px[1] < 190 and px[2] < 40)
        only()
    back_wall("air")
    say(".reset nametags")
    return got["Bands"] > 5 and got["Gradient"] < got["Bands"], f"gold tag pixels: {got}"


@test("Nametags (Totem pops)")
def _():
    # A husk holding a totem pops it (/damage past its health): with Totem pops its tag gains a grey " -1".
    reset()
    back_wall()
    cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 4.5} {{NoAI:1b,PersistenceRequired:1b,Silent:1b,Health:4f,"
        'equipment:{mainhand:{id:"minecraft:totem_of_undying",count:1}},drop_chances:{mainhand:0f}}')
    setting("nametags", "mobs", "true")
    setting("nametags", "distance", "false")
    ctl("look", "0", "-5")
    wait(10)
    cmd("damage @e[type=husk,limit=1] 10")
    wait(10)
    grey = {}
    for pops in ("false", "true"):
        setting("nametags", "totem pops", pops)
        only("nametags")
        clear_chat()
        wait(10)
        grey[pops] = sum(1 for px in colour_frame(f"nametags-pops-{pops}").getdata()
                         if 160 <= px[0] <= 180 and 160 <= px[1] <= 180 and 160 <= px[2] <= 180)
        only()
    back_wall("air")
    cmd("kill @e[type=husk]")
    say(".reset nametags")
    return grey["true"] > grey["false"] + 5, f"grey tag pixels with Totem pops off and on: {grey}"


@test("Hand View (Swing animation)")
def _():
    # A sword held, looking at the floor: Swing animation 3 holds it across the view (blocking) even still, so the hand
    # area differs from the game's own; 1 differs from the game's mid-swing (a swing caught a few frames in).
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    ctl("look", "0", "60")
    hand = (640, 300, 1280, 720)
    setting("handview", "swing animation", "3")
    still = on_off("hand-view-swing3", ("handview",), settle=10, box=hand)
    setting("handview", "swing animation", "1")
    setting("handview", "slow swing", "true")
    swung = on_off("hand-view-swing1", ("handview",), setup=lambda: ctl("mouse", "left", "click"), settle=2, box=hand)
    say(".reset handview")
    return still > 500 and swung > 200, f"pixels changed: sword held with style 3 {still}, mid-swing with style 1 {swung}"


@test("No Render (scoreboard, titles)")
def _():
    # A sidebar scoreboard and a title on screen, looking at the floor: Hide scoreboard and Hide titles take them away.
    reset()
    cmd("scoreboard objectives add sunsettest dummy", "scoreboard objectives setdisplay sidebar sunsettest",
        "scoreboard players set Sunset sunsettest 7")
    for k in ("hide scoreboard", "hide titles"):
        setting("norender", k, "true")
    ctl("look", "0", "80")
    got = on_off("no-render-hud", ("norender",), setup=lambda: cmd("title @s times 0 200 0", 'title @s title "SUNSET"'), settle=10,
                 box=(0, 0, 1280, 600))
    cmd("scoreboard objectives remove sunsettest", "title @s clear")
    say(".reset norender")
    return got > 500, f"pixels changed with the scoreboard and title hidden: {got}"


@test("Atmosphere (Custom fog)")
def _():
    # Atmosphere with Mode None (the game's sky) and Custom fog from 0 to 10 blocks in Red: the view turns red.
    reset()
    setting("atmosphere", "mode", "None")
    setting("atmosphere", "custom fog", "true")
    setting("atmosphere", "fog start", "0")
    setting("atmosphere", "fog end", "10")
    setting("atmosphere", "fog color", "Red")
    ctl("look", "0", "0")
    only("atmosphere")
    clear_chat()
    wait(20)
    img = colour_frame("atmosphere-fog")
    only()
    clear_chat()
    wait(10)
    red_of = lambda im: sum(1 for px in im.getdata() if px[0] > 150 and px[1] < 110 and px[2] < 110)
    red, plain = red_of(img), red_of(colour_frame("atmosphere-fog-off"))
    say(".reset atmosphere")
    share = red / (img.size[0] * img.size[1])
    return share > 0.3 and plain < red / 4, f"red share with Custom fog {share:.2f}; red pixels without {plain}"


# ---- the plugin API (version 2), through the example plugins (sunset-plugins: hello, afk) and the test plugin
# (tools/plugcheck), all three staged next to Sunset on the test machine. They answer in chat: the log has it.

def said_since(mark, *lines, settle=4):
    """Types each line, waits, and gives what Sunset said since mark."""
    for line in lines:
        say(line)
    wait(settle)
    return sunset_said(mark)


def plugcheck_events():
    """The test plugin's event counts (name: count), the priority order string and its last event line."""
    mark = log_mark()
    line = next((l for l in said_since(mark, ".plugcheck events") if l.startswith("plugcheck events")), "")
    counts = dict((k, int(v)) for k, v in re.findall(r"(\w+)=(\d+)", line))
    order = re.search(r" order (\S*) last", line)
    last = line.split(" last ", 1)[1] if " last " in line else ""
    return counts, (order.group(1) if order else ""), last


@test("Plugin API: .plugins and the refused names")
def _():
    # .plugins lists each plugin with its version, modules and commands (a failed one with why); the names a plugin
    # can't have (a module Sunset has, a built-in command, a bad command name, a bad plugin id) were each refused with a
    # chat line when it loaded; .help lists the plugins' commands.
    mark = log_mark()
    said = said_since(mark, ".plugins", ".help")
    text = " | ".join(said)
    listed = all(x in text for x in ("hello 0.2.0 - modules: Hello - commands: .hello",
                                     "afk 0.1.0 - modules: AFK Fisher - commands: .afk",
                                     "plugcheck 0.1.0 - modules: Plug Check - commands: .plugcheck",
                                     "plugin API 2"))
    bad_id = "Bad Id 0.1.0" in text and "OFF: its id" in text
    helped = ".hello - say Hello's message" in text and ".afk - AFK Fisher" in text and ".plugins - the plugins loaded" in text
    start = " | ".join(sunset_said(0))
    refused = ["a module called Kill Aura is already there" in start, "the command .set can't be added: that name is taken" in start,
               "the command .bad name can't be added: use letters" in start, "Plugin Bad Id is off" in start]
    # The clash didn't replace Sunset's Kill Aura (still Combat, with its Range).
    mark = log_mark()
    ka = said_since(mark, ".settings killaura")
    kept = any("Range" in l for l in ka)
    ok = listed and bad_id and helped and all(refused) and kept
    return ok, f"listed {listed}, bad id shown {bad_id}, .help {helped}, refused at load {refused}, Kill Aura kept {kept}"


@test("Plugin API: Hello module, settings and enable/disable")
def _():
    # Turned on and off from chat: its on_enable/on_disable lines; with Every 1 s and a new Message (.set, so the
    # plugin's module takes Sunset's typed settings) the message comes about once a second while on, never while off.
    mark = log_mark()
    setting("hello", "message", "Plugin test 123")
    setting("hello", "every (seconds)", "1")
    toggle("hello", True)
    wait(70)
    on_said = sunset_said(mark)
    toggle("hello", False)
    wait(4)
    mark2 = log_mark()
    wait(60)
    off_said = sunset_said(mark2)
    lines = sum(1 for l in on_said if l == "Plugin test 123")
    enabled = "Hello is on." in on_said
    disabled = "Hello is off." in sunset_said(mark)
    # .reset puts a plugin's module back to what the plugin registered (it once crashed the game).
    mark = log_mark()
    shown = " | ".join(said_since(mark, ".reset hello", ".settings hello"))
    reset_ok = "Hello reset." in shown and "Every (seconds): 10" in shown and "Message: Hello from a plugin" in shown
    alive = bool(state().get("player"))
    ok = enabled and disabled and 2 <= lines <= 5 and not any("Plugin test" in l for l in off_said) and reset_ok and alive
    return ok, (f"on line {enabled}, off line {disabled}, messages in 3.5 s on {lines}, while off {len(off_said)} lines: "
                f"{off_said[:3]}; .reset {reset_ok} (alive {alive}): {shown[:200]}")


@test("Plugin API: Hello command and timers")
def _():
    # .hello 3: three lines, a second apart (after()); a bad count: the usage line; .hello 9 is held to 5; .hello 5 then
    # a stop (plugcheck's api::stop_everything, when staged): the lines still waiting are dropped (a stop drops every
    # plugin's tasks); .hello total counts up and is kept in the plugin's data folder.
    setting("hello", "message", "Timer line")
    mark = log_mark()
    say(".hello 3")
    t0 = time.time()
    got = poll(80, 2, lambda: sum(1 for l in sunset_said(mark) if l == "Timer line"))
    first_three = next((i for i, n in enumerate(got) if n >= 3), None)
    three = got[-1]
    mark = log_mark()
    bad = said_since(mark, ".hello lots")
    mark = log_mark()
    say(".hello 9")
    wait(120)
    nine = sum(1 for l in sunset_said(mark) if l == "Timer line")
    mark = log_mark()
    stoppable = "plugcheck events" in " ".join(said_since(mark, ".plugcheck events"))
    mark = log_mark()
    say(".hello 5")
    if stoppable:
        say(".plugcheck stop")
        on.clear()
    wait(120)
    panicked = sum(1 for l in sunset_said(mark) if l == "Timer line") if stoppable else 0
    mark = log_mark()
    tot = said_since(mark, ".hello total", ".hello total")
    nums = [int(n) for l in tot for n in re.findall(r"ran (\d+) times", l)]
    data = [os.path.join(r, f) for r, _, fs in os.walk(session_dir()) for f in fs if r.endswith(os.path.join(".horizon-mod-data", "hello")) and f == "total"]
    kept = open(data[0]).read().strip() if data else None
    ok = (three == 3 and first_three is not None and first_three >= 15 and any("Use .hello [times]" in l for l in bad)
          and nine == 5 and panicked <= 1 and len(nums) == 2 and nums[1] == nums[0] + 1 and kept == str(nums[1]))
    return ok, (f".hello 3: {three} lines (3rd by poll {first_three}, {time.time() - t0:.0f} s); bad count {bad}; .hello 9: {nine}; "
                f".hello 5 then a stop: {panicked if stoppable else 'plugcheck not staged'}; totals {nums}, file {kept}")


@test("Plugin API: Hello cancels hits, draws HUD and world")
def _():
    # Hold back hits (an on_packet_out handler cancelling Attack): a pig clicked three times is never hurt and no
    # attack goes on the wire; with it off the same clicks hurt it. The HUD and world handlers draw each frame while
    # their settings are on, and stop when off (counted by the plugin); one screenshot for the eye.
    def clicks(hold_back):
        reset()
        pid = pig()
        ctl("look-at-entity", str(pid))
        setting("hello", "hold back hits", hold_back)
        toggle("hello", True)
        wait(5)
        mark = probe_mark() if probe_path() else None
        for _ in range(3):
            ctl("mouse", "left", "down")
            wait(2)
            ctl("mouse", "left", "up")
            wait(14)
        wait(10)
        m = mobs("pig")
        attacks = sum(t.count("order attack") for t in probe_ticks(mark)) if mark is not None else None
        toggle("hello", False)
        return (m[0]["health"] if m else 0), attacks

    def stats():
        mark = log_mark()
        line = next((l for l in said_since(mark, ".hello stats") if l.startswith("hud ")), "")
        return dict((k, int(v)) for k, v in re.findall(r"(\w+) (\d+)", line))

    s0 = stats()
    held, held_attacks = clicks("true")
    s1 = stats()
    hurt, hurt_attacks = clicks("false")
    reset()
    # Looking down, so the block under you is in sight.
    ctl("look", "0", "70")
    setting("hello", "box under me", "true")
    toggle("hello", True)
    wait(30)
    s2 = stats()
    ctl("screenshot", os.path.join(SHOTS, "plugin-hello.png"))
    setting("hello", "show on hud", "false")
    setting("hello", "box under me", "false")
    wait(10)
    s3 = stats()
    wait(30)
    s4 = stats()
    toggle("hello", False)
    blocked = s1.get("blocked", 0) - s0.get("blocked", 0)
    drawing = s2.get("hud", 0) > s1.get("hud", 0) and s2.get("world", 0) > s1.get("world", 0)
    stopped = s4.get("hud") == s3.get("hud") and s4.get("world") == s3.get("world")
    ok = held == 10 and blocked >= 3 and hurt < 10 and held_attacks in (0, None) and (hurt_attacks is None or hurt_attacks >= 1) and drawing and stopped
    return ok, (f"pig health with Hold back hits {held} ({blocked} attacks cancelled, {held_attacks} on the wire), without {hurt} "
                f"({hurt_attacks} on the wire); drawing {s1} -> {s2}; settings off {s3} -> {s4}")


@test("Plugin API: any module's settings and toggles")
def _():
    # api::set with a good value, a misspelt module (a suggestion), a misspelt setting (the list), a bad number, a
    # number past the range (held to its end), a plugin module's switch and a bad choice; api::toggle twice on (no
    # flip back), off by another spelling, and an unknown module.
    touched.update(("killaura", "plugcheck"))
    mark = log_mark()
    said = said_since(mark, ".plugcheck set", ".plugcheck toggle")
    text = " | ".join(said)
    ranges = re.findall(r"plugcheck ok: Range: ([\d.]+)", text)
    checks = {
        "set": "plugcheck ok: Range: 4.5" in text,
        "module": 'plugcheck err: No module called "Kil Aura". Did you mean Kill Aura' in text,
        "setting": 'Kill Aura has no setting "Rnge". Its settings: ' in text and "Range" in text,
        "number": re.search(r'"far" is not a value for Kill Aura > Range: give a number from [\d.]+ to [\d.]+', text) is not None,
        "clamped": len(ranges) == 2 and ranges[0] == "4.5" and 4.5 < float(ranges[1]) < 99,
        "switch": "plugcheck ok: Flag: ON" in text,
        "choice": "is not a value for Kill Aura > Rotation mode: give one of:" in text,
        "read back": re.search(r"plugcheck range Some\([\d.]+\) flag true", text) is not None and "range Some(4.5)" not in text,
        "on": "plugcheck ok: sprint true" in text and "plugcheck ok: sprint again true" in text,
        "off": "plugcheck ok: sprint off false" in text,
        "unknown": 'plugcheck err: No module called "Nope Module"' in text,
        "count": re.search(r"plugcheck modules (\d{3})", text) is not None,
    }
    ok = all(checks.values())
    return ok, f"{[k for k, v in checks.items() if not v] or 'all'} ok; {text[:600]}"


@test("Plugin API: refused sends, a refused replace, and a panicking plugin")
def _():
    # api::send refuses a move packet and a held slot change with the fix in the error; say_server refuses 300
    # characters and sends a short line; a packet turned into a move packet is refused (one move packet a tick). Then
    # plugcrash's command panics: the game goes on, plugcrash is off (.plugins says why) and its command says so.
    mark = log_mark()
    said = said_since(mark, ".plugcheck send", settle=10)
    text = " | ".join(said)
    sends = ["plugcheck err: Move packets can't be sent" in text, "plugcheck err: Change the held slot with api::hold" in text,
             "256 characters at most" in text, "plugcheck ok: said" in text,
             "a packet can't be replaced by a move packet" in text]
    mark = log_mark()
    said = said_since(mark, ".crash", settle=6)
    alive = bool(state().get("player"))
    said += said_since(log_mark(), ".plugins", ".crash")
    text = " | ".join(said)
    crashed = "Plugin plugcrash is off: its command code panicked: boom from plugcrash" in text or "plugcrash failed" in text
    listed = "plugcrash 0.1.0 - modules: none - commands: .crash - OFF: its command code panicked: boom from plugcrash" in text
    refused = ".crash is off: its plugin plugcrash failed" in text
    ok = all(sends) and alive and crashed and listed and refused
    return ok, f"send checks {sends}; game alive after the panic {alive}, crash line {crashed}, listed off {listed}, command refused {refused}"


@test("Plugin API: events (ticks, toggles, keys, chat, packets, hurt, low health, death, priorities)")
def _():
    # The test plugin counts each event from .plugcheck reset: ticks before and after the move (about as many),
    # HUD and world frames, a toggle (Sprint on), a key, two server chat lines with its marker (Sunset's own line with
    # it doesn't count), the server's time packets, hurts (with health before and after), low health once per crossing
    # under 10, a death, no join (a respawn is no reconnect). Priorities: the HIGH tick handler before the LOW one; the
    # HIGH packet handler cancels "/plugblock" so the LOW one never sees it.
    cmd("gamerule natural_health_regeneration false")
    say(".plugcheck reset")
    wait(40)
    toggle("sprint", True)
    ctl("key", "h", "tap")
    cmd("say plugcheck-chat one", "say plugcheck-chat two", "plugblock now")
    cmd("damage @s 6")
    wait(4)
    _, _, hurt_last = plugcheck_events()
    cmd("damage @s 6")
    wait(4)
    cmd("damage @s 2")
    wait(4)
    cmd("effect give @s minecraft:instant_health 1 10 true")
    wait(6)
    cmd("damage @s 12")
    wait(4)
    cmd("kill @s")
    wait(10)
    ensure_alive()
    wait(10)
    counts, order, _ = plugcheck_events()
    cmd("gamerule natural_health_regeneration true")
    c = counts.get
    checks = {
        "ticks": c("tick", 0) >= 40 and abs(c("tick", 0) - c("pretick", 0)) <= 3,
        "frames": c("hud", 0) > 10 and c("world", 0) > 10,
        "toggle": c("toggle", 0) >= 1,
        "key": c("hkey", 0) >= 1,
        "chat": c("chat", 0) == 2,
        "packets in": c("settime", 0) >= 1,
        "hurt": c("hurt", 0) >= 4 and hurt_last == "hurt 14 from 20",
        "low": c("low", 0) == 2,
        "death": c("death", 0) == 1,
        "no join": c("join", 0) == 0,
        "order": order.startswith("HL"),
        "cancel": c("blocked", 0) == 1 and c("lowsaw", 0) == 0,
    }
    ok = all(checks.values())
    return ok, f"failed {[k for k, v in checks.items() if not v]}; counts {counts}, order {order}, first hurt {hurt_last!r}"


@test("Plugin API: scheduler, data, stop everything, reads and friends")
def _():
    # after/every/cancel (an every cancelled from inside itself after 3 runs; a task cancelled before it ran, twice);
    # save/load (a count on disk, bad keys and Sunset's own id refused); stop_everything: modules off, tasks dropped,
    # every plugin's on_stop; reads against horizon-ctl's own view; friends added once, then removed.
    mark = log_mark()
    say(".plugcheck tasks")
    wait(60)
    tasks = sunset_said(mark)
    mark = log_mark()
    store = said_since(mark, ".plugcheck store", ".plugcheck store")
    runs = [int(n) for l in store for n in re.findall(r"plugcheck ok: runs (\d+)", l)]
    data = [os.path.join(r, f) for r, _, fs in os.walk(session_dir()) for f in fs if r.endswith(os.path.join(".horizon-mod-data", "plugcheck")) and f == "runs"]
    on_disk = open(data[0]).read().strip() if data else None
    toggle("fullbright", True)
    say(".plugcheck reset")
    mark = log_mark()
    say(".plugcheck stop")
    on.clear()
    wait(40)
    stop = sunset_said(mark)
    counts, _, last = plugcheck_events()
    slot(0, "dirt", 5)
    slot(3, "cobblestone", 3)
    pig(3.0)
    wait(5)
    mark = log_mark()
    reads = " ".join(said_since(mark, ".plugcheck reads", ".plugcheck friends"))
    st = state()["player"]
    m = re.search(r"plugcheck me (\S+) (\S+) (\S+) hp (\d+)", reads)
    near = re.search(r"near minecraft:pig ([\d.]+)", reads)
    t = " | ".join(tasks + store + stop)
    checks = {
        "tasks": "plugcheck tasks after 1 every 3 cancel true false" in t and "cancelled task ran" not in t,
        "store": len(runs) == 2 and runs[1] == runs[0] + 1 and on_disk == str(runs[1]),
        "store refused": t.count("plugcheck err: save(") == 4 and "plugcheck load Some(" in t and "missing None" in t,
        "stop": "Stopped everything: plugcheck" in t and "plugcheck tasks left 0 fullbright false" in t and "task after stop ran" not in t
                and counts.get("stop") == 1 and last == "stop plugcheck",
        "me": m is not None and all(abs(float(m.group(i + 1)) - st["position"][i]) < 0.2 for i in range(3)) and int(m.group(4)) == round(st["health"]),
        "reads": all(x in reads for x in ("under stone", "slot0 dirtx5", "count 8", "find Some(0)", "dim minecraft:overworld", "in_world true", "ground true", "slot 0")),
        "near": near is not None and abs(float(near.group(1)) - 3.0) < 0.6,
        "friends": "plugcheck friend true once 1 after false" in reads,
    }
    ok = all(checks.values())
    return ok, f"failed {[k for k, v in checks.items() if not v]}; {t[:400]} || {reads[:400]}"


def afk_status():
    mark = log_mark()
    return " | ".join(said_since(mark, ".afk"))


@test("AFK Fisher: starts Auto Fish, stops on danger")
def _():
    # No rod in the hotbar (none, or one in the main inventory): it says so and stays off. With a rod: Auto Fish on,
    # Auto-reconnect on. Health exactly at Leave below health (10) is no danger; one below stops everything (AFK Fisher
    # and Auto Fish off, the reason kept). Log out is off here (the server test logs out); a death stops it too.
    cmd("gamerule natural_health_regeneration false")
    mark = log_mark()
    # Each .t turns it on; it turns itself off again at once.
    say(".t afkfisher")
    wait(4)
    slot(0, "stone")
    cmd("item replace entity @s inventory.5 with minecraft:fishing_rod")
    say(".t afkfisher")
    wait(4)
    no_rod = sum(1 for l in sunset_said(mark) if "put a fishing rod in your hotbar first" in l)
    off_without = "AFK Fisher off" in afk_status()
    slot(1, "fishing_rod")
    setting("afkfisher", "log out", "false")
    mark = log_mark()
    toggle("afkfisher", True)
    wait(25)
    started = " ".join(sunset_said(mark))
    running = afk_status()
    cmd("damage @s 10")
    wait(6)
    at_line = afk_status()
    cmd("damage @s 1")
    wait(6)
    on.discard("afkfisher")
    after = afk_status()
    cmd("effect give @s minecraft:instant_health 1 10 true")
    wait(5)
    toggle("afkfisher", True)
    wait(6)
    cmd("kill @s")
    wait(10)
    on.discard("afkfisher")
    ensure_alive()
    died = afk_status()
    cmd("gamerule natural_health_regeneration true")
    checks = {
        "no rod": no_rod == 2 and off_without,
        "started": "AFK Fisher on: fishing; leaves below 10 health or a player comes within 32 blocks." in started,
        "running": "AFK Fisher on:" in running and "Auto Fish on" in running and "auto reconnect on" in running,
        "at the line": "AFK Fisher on:" in at_line,
        "stopped": "AFK Fisher off" in after and "Auto Fish off" in after and "Last stop: AFK Fisher: health 9 is below 10" in after,
        "death": "AFK Fisher off" in died and "Last stop: AFK Fisher: you died" in died,
    }
    ok = all(checks.values())
    return ok, f"failed {[k for k, v in checks.items() if not v]}; running: {running}; at 10: {at_line}; at 9: {after}; died: {died}"


@test("AFK Fisher: catches fish")
def _():
    # A pool in front, a Lure III rod: Auto Fish casts and reels in, AFK Fisher counts the fish (up to 75 s).
    reset()
    cmd(f"fill {X0-2} {Y-2} {Z0+2} {X0+2} {Y} {Z0+6} minecraft:water",
        "item replace entity @s hotbar.0 with minecraft:fishing_rod[enchantments={lure:3}]")
    ctl("look", "0", "35")
    toggle("afkfisher", True)
    caught = 0
    for _ in range(30):
        wait(50)
        m = re.search(r"AFK Fisher on: (\d+) fish", afk_status())
        caught = int(m.group(1)) if m else -1
        if caught >= 1:
            break
    toggle("afkfisher", False)
    cmd(f"fill {X0-2} {Y-2} {Z0+2} {X0+2} {Y} {Z0+6} minecraft:stone")
    return caught >= 1, f"fish counted {caught}"


@test("AFK Fisher: logs out on danger, keeps its state over a kick and rejoin")
def _():
    # On a server: danger with Log out on leaves the server (on purpose: nothing retried); back in, .afk still has the
    # reason and the test plugin saw the join. Then a kick: its reason comes to on_disconnect, not retried.
    if not SERVER:
        return None, "needs a server"
    cmd("gamerule natural_health_regeneration false")
    slot(0, "fishing_rod")
    say(".plugcheck reset")
    toggle("afkfisher", True)
    wait(25)
    cmd("damage @s 12")
    wait(30)
    on.discard("afkfisher")
    gone = not state().get("player")
    rejoin("AFK Fisher (danger)")
    kicks.pop() if kicks and kicks[-1][0] == "AFK Fisher (danger)" else None
    back = afk_status()
    counts, _, _ = plugcheck_events()
    cmd("gamerule natural_health_regeneration true")
    cmd("kick Tester plugcheck kick")
    time.sleep(3)
    rejoin("AFK Fisher (kick)")
    kicks.pop() if kicks and kicks[-1][0] == "AFK Fisher (kick)" else None
    kicked = afk_status()
    ok = (gone and "Last stop: AFK Fisher: health 8 is below 10" in back and counts.get("join", 0) >= 1
          and "plugcheck kick" in kicked and "not retried" in kicked)
    return ok, f"left the server {gone}; back: {back}; joins seen {counts.get('join')}; after the kick: {kicked}"


@test("AFK Fisher: a player comes near (friends don't count)")
def _():
    if not CTL2:
        return None, "needs Bot1"
    slot(0, "fishing_rod")
    setting("afkfisher", "log out", "false")
    say(".friend add Bot1")
    toggle("afkfisher", True)
    cmd("gamemode survival Bot1", f"tp Bot1 {X0 + 0.5} {Y + 1} {Z0 + 8.5}")
    wait(60)
    friend = afk_status()
    say(".friend remove Bot1")
    wait(40)
    on.discard("afkfisher")
    stranger = afk_status()
    ok = "AFK Fisher on:" in friend and "AFK Fisher off" in stranger and re.search(r"Last stop: AFK Fisher: Bot1 came within \d+ blocks", stranger) is not None
    return ok, f"friend near: {friend}; not a friend: {stranger}"


def hud_window():
    """The window's size: runall.sh's SIZE, or the display's."""
    if os.environ.get("SIZE"):
        return tuple(map(int, os.environ["SIZE"].split("x")))
    d = ctl("status")["display"]
    return d["width"], d["height"]


def hud_scale():
    """Screen pixels to a GUI pixel: the GUI scale set (as the game clamps it: no larger than keeps 320x240), or auto."""
    w, h = hud_window()
    value = ctl("options-get", "guiScale")["options"]["guiScale"]["value"]
    largest = max(1, min(w // 320, h // 240))
    scale = int(value or 0)
    return largest if scale <= 0 else min(scale, largest)


def hud_size():
    """The GUI's size in GUI pixels."""
    w, h = hud_window()
    k = hud_scale()
    return w / k, h / k


def hud_box(x0, y0, x1, y1):
    """A box in GUI pixels, in screen pixels for crop()."""
    k = hud_scale()
    return tuple(int(v * k) for v in (x0, y0, x1, y1))


def coloured(img, box, test):
    return sum(1 for px in img.crop(box).getdata() if test(*px))


def reds(r, g, b):
    return r > 190 and g < 110 and b < 110


def greens(r, g, b):
    return g > 170 and r < 140 and b < 110


# Armour with damage at about a tenth worn (or 9 tenths with worn=True): helmet 363, chestplate 240, leggings 105,
# boots 481 at most.
ARMOUR = (("head", "diamond_helmet", 363), ("chest", "iron_chestplate", 240), ("legs", "golden_leggings", 105),
          ("feet", "netherite_boots", 481))


def wear(worn=False, parts=("head", "chest", "legs", "feet")):
    for part, item, top in ARMOUR:
        if part in parts:
            cmd(f"item replace entity @s armor.{part} with minecraft:{item}[damage={int(top * (0.9 if worn else 0.1))}]")


def hud_scene():
    """You on your own, looking straight down at the floor (nothing in view moves), chat faded."""
    reset()
    ctl("look", "0", "90")
    for name, value in (("armor hud", "false"), ("armor hud style", "Boxes"), ("armor hud layout", "Horizontal"),
                        ("armor hud durability", "Bar"), ("armor hud hands", "false"), ("armor hud hide empty", "true"),
                        ("armor hud position", "Hotbar"), ("item durability", "false"), ("durability format", "Number"),
                        ("durability tooltip", "true"), ("food preview", "false"), ("saturation", "false"),
                        ("exhaustion", "false"), ("mount hud", "false"), ("shield status", "false")):
        setting("hud", name, value)


def hud_frame(name, *settings, colour=False):
    for k, v in settings:
        setting("hud", k, v)
    wait(225)
    return colour_frame(name) if colour else frame(name)


@test("Armor HUD")
def _():
    # Beside the hotbar on the right: nothing with no armour on (empty slots hidden), four boxes in a row with it,
    # none above the row; a fresh set's bars green, a worn set's red; the hands add a box for the sword held.
    hud_scene()
    w, h = hud_size()
    row = hud_box(w / 2 + 95, h - 23, w / 2 + 95 + 84, h)
    above = hud_box(w / 2 + 95, h - 100, w / 2 + 95 + 84, h - 26)
    hand = hud_box(w / 2 + 95 + 84, h - 23, w / 2 + 95 + 130, h)
    off = hud_frame("ah_off", colour=True)
    empty = hud_frame("ah_empty", ("armor hud", "true"), colour=True)
    wear()
    fresh = hud_frame("ah_fresh", colour=True)
    bare = hud_frame("ah_fresh_off", ("armor hud", "false"), colour=True)
    wear(worn=True)
    worn = hud_frame("ah_worn", ("armor hud", "true"), colour=True)
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    hands = hud_frame("ah_hands", ("armor hud hands", "true"), colour=True)
    nothing = recoloured(off.crop(row), empty.crop(row))
    boxes, over = recoloured(bare.crop(row), fresh.crop(row)), recoloured(bare.crop(above), fresh.crop(above))
    green, red = coloured(fresh, row, greens), coloured(worn, row, reds)
    worn_green = coloured(worn, row, greens)
    sword = recoloured(worn.crop(hand), hands.crop(hand))
    ok = nothing < 20 and boxes > 400 and over < 20 and green > 10 and red > 10 and worn_green < green and sword > 100
    return ok, (f"no armour {nothing} px changed; armour on {boxes} px in the row, {over} above it; green bar px "
                f"fresh {green}, worn {worn_green}; red worn {red}; a sword with Hands {sword} px further right")


@test("Armor HUD layouts")
def _():
    # Vertical: a column going up from the hotbar's foot instead of the row; Compact (Armor Status's look) with
    # Percent at the top left; Custom at the middle of the screen; Hide empty off with only a helmet: four boxes.
    hud_scene()
    w, h = hud_size()
    wear()
    off = hud_frame("al_off")
    column = hud_box(w / 2 + 95, h - 90, w / 2 + 95 + 22, h - 24)
    row_end = hud_box(w / 2 + 95 + 24, h - 23, w / 2 + 95 + 84, h)
    vertical = hud_frame("al_vertical", ("armor hud", "true"), ("armor hud layout", "Vertical"))
    corner = hud_box(0, 50, 80, 140)
    compact = hud_frame("al_compact", ("armor hud style", "Compact"), ("armor hud durability", "Percent"),
                        ("armor hud position", "Top left"))
    middle = hud_box(w / 2 + 20, h / 2 - 20, w / 2 + 120, h / 2 + 40)
    custom = hud_frame("al_custom", ("armor hud style", "Boxes"), ("armor hud layout", "Horizontal"),
                       ("armor hud position", "Custom"), ("armor hud x", "0.55"), ("armor hud y", "0.47"))
    cmd("clear @s")
    wear(parts=("head",))
    helmet_off = hud_frame("al_helmet_off", ("armor hud position", "Hotbar"), ("armor hud", "false"))
    helmet = hud_frame("al_helmet", ("armor hud", "true"))
    four = hud_frame("al_four", ("armor hud hide empty", "false"))
    up, along = changed(off, vertical, column), changed(off, vertical, row_end)
    top_left, mid = changed(off, compact, corner), changed(off, custom, middle)
    one, all4 = changed(helmet_off, helmet, row_end), changed(helmet_off, four, row_end)
    ok = up > 300 and along < 20 and top_left > 150 and mid > 300 and one < 20 and all4 > 300
    return ok, (f"Vertical: {up} px in the column, {along} along the row; Compact top left {top_left} px; Custom "
                f"{mid} px mid-screen; a helmet alone {one} px past its box, with Hide empty off {all4}")


@test("Armor HUD drag")
def _():
    # With the menu open, the Armor HUD in the bottom right corner is dragged to the middle left: it draws there
    # afterwards (Position becomes Custom), no longer in the corner.
    hud_scene()
    w, h = hud_size()
    wear()
    setting("hud", "armor hud position", "Bottom right")
    off = hud_frame("ad_off")
    corner = hud_box(w - 90, h - 30, w, h)
    target = hud_box(40, h / 2 - 15, 140, h / 2 + 15)
    setting("hud", "armor hud", "true")
    before = hud_frame("ad_before")
    say(".menu")
    wait(8)
    ctl("drag-at", str(w - 40), str(h - 12), str(w - 200), str(h / 2 - 20), str(80), str(h / 2))
    wait(4)
    close_menu()
    after = hud_frame("ad_after")
    was, left, moved = changed(off, before, corner), changed(off, after, corner), changed(off, after, target)
    return was > 300 and left < 20 and moved > 300, f"in the corner {was} px; after the drag {left} there, {moved} mid-left"


@test("Armor HUD (GUI scales)")
def _():
    # The row beside the hotbar at GUI scale 2 and 4 (screenshots ah_scale2, ah_scale4 to look at).
    scale = ctl("options-get", "guiScale")["options"]["guiScale"]["value"]
    notes, ok = [], True
    for n in (2, 4):
        ctl("options-set", "guiScale", str(n))
        hud_scene()
        w, h = hud_size()
        wear(worn=True)
        slot(0, "diamond_sword")
        ctl("key", "1", "tap")
        row = hud_box(w / 2 + 95, h - 23, w / 2 + 95 + 130, h)
        off = hud_frame(f"ah_scale{n}_off")
        on = hud_frame(f"ah_scale{n}", ("armor hud", "true"), ("armor hud hands", "true"),
                       ("armor hud durability", "Bar and percent"))
        px = changed(off, on, row)
        ok = ok and px > 200 * n * n / 4
        notes.append(f"scale {n} (drawn at {hud_scale():.0f}): {px} px")
    ctl("options-set", "guiScale", str(scale))
    return ok, ", ".join(notes)


def hotbar_corner(i):
    """The top left quarter of hotbar slot i's icon (where Item durability writes), in screen pixels."""
    w, h = hud_size()
    x, y = w / 2 - 91 + 3 + 20 * i, h - 19
    return hud_box(x, y, x + 9, y + 5)


@test("Item durability")
def _():
    # On the hotbar: a worn sword gets its number (and a different one as a percent), a fresh pickaxe too, a stack
    # of stone nothing; off, nothing; and in the inventory screen the same items change too.
    hud_scene()
    cmd("item replace entity @s hotbar.0 with minecraft:diamond_sword[damage=1200]",
        "item replace entity @s hotbar.1 with minecraft:iron_pickaxe", "item replace entity @s hotbar.2 with minecraft:stone 32")
    off = hud_frame("id_off", colour=True)
    number = hud_frame("id_number", ("item durability", "true"), colour=True)
    percent = hud_frame("id_percent", ("durability format", "Percent"), colour=True)
    sword, pick, stone = (recoloured(off.crop(hotbar_corner(i)), number.crop(hotbar_corner(i))) for i in range(3))
    as_percent = recoloured(number.crop(hotbar_corner(0)), percent.crop(hotbar_corner(0)))
    reds_on = coloured(number, hotbar_corner(0), lambda r, g, b: r > 150 and g < 200 and b < 90)
    ctl("key", "e", "tap")
    wait(10)
    inv_on = frame("id_inventory")
    setting("hud", "item durability", "false")
    wait(5)
    inv_off = frame("id_inventory_off")
    ctl("key", "escape", "tap")
    wait(5)
    inventory = changed(inv_off, inv_on)
    ok = sword > 4 and pick > 4 and stone == 0 and as_percent > 4 and reds_on > 0 and inventory > 20
    return ok, (f"px changed: worn sword {sword} ({reds_on} orange-red), fresh pickaxe {pick}, stone {stone}; "
                f"Percent against Number {as_percent}; inventory screen {inventory}")


@test("Item durability (GUI scales)")
def _():
    # Screenshots id_scale2 and id_scale4 of the hotbar with numbers, and each one's sword corner changed.
    scale = ctl("options-get", "guiScale")["options"]["guiScale"]["value"]
    notes, ok = [], True
    for n in (2, 4):
        ctl("options-set", "guiScale", str(n))
        hud_scene()
        cmd("item replace entity @s hotbar.0 with minecraft:diamond_sword[damage=1200]",
            "item replace entity @s hotbar.1 with minecraft:bow[damage=300]")
        off = hud_frame(f"id_scale{n}_off")
        on = hud_frame(f"id_scale{n}", ("item durability", "true"))
        px = changed(off, on, hotbar_corner(0)) + changed(off, on, hotbar_corner(1))
        ok = ok and px > 4
        notes.append(f"scale {n} (drawn at {hud_scale():.0f}): {px} px")
    ctl("options-set", "guiScale", str(scale))
    return ok, ", ".join(notes)


@test("Item durability tooltip")
def _():
    # In the inventory, the pointer on a worn sword: its tooltip is a line taller with Durability tooltip on.
    hud_scene()
    cmd("item replace entity @s hotbar.0 with minecraft:diamond_sword[damage=1200]")
    setting("hud", "item durability", "true")
    w, h = hud_size()
    shots = {}
    for name, value in (("off", "false"), ("on", "true")):
        setting("hud", "durability tooltip", value)
        wait(5)
        ctl("key", "e", "tap")
        wait(10)
        # A server can keep the recipe book open: it moves the inventory 77 to the right.
        book = 77 if (ctl("screen").get("recipe_book") or {}).get("open") else 0
        x, y = (w - 176) / 2 + book + 8 + 8, (h - 166) / 2 + 142 + 8
        ctl("drag-at", "4", "4", str(x), str(y))
        wait(10)
        shots[name] = frame(f"idt_{name}")
        ctl("key", "escape", "tap")
        wait(5)
    tip = hud_box(x, y - 40, x + 200, y + 40)
    px = changed(shots["off"], shots["on"], tip)
    return px > 40, f"{px} px of the tooltip changed with the line"


def hungry_to(to=12):
    """Food down to `to` or less (Hunger until then; not in Peaceful, which feeds you), no saturation left: the food
    the server last sent. The test puts Peaceful back."""
    cmd("difficulty easy", "effect give @s minecraft:hunger 20 100 true")
    for _ in range(60):
        wait(5)
        if (state()["player"].get("food") or 20) <= to:
            break
    cmd("effect clear @s minecraft:hunger")
    wait(10)


def food_row():
    w, h = hud_size()
    return hud_box(w / 2 + 10, h - 40, w / 2 + 92, h - 29)


@test("Food preview, Saturation, Exhaustion")
def _():
    # Full and fed (Saturation from reset): the gold frames show, a bread's preview adds nothing; hungry: the bread
    # adds flashing shanks, Saturation shows nothing (none left); jumps put an exhaustion bar under the row.
    hud_scene()
    slot(0, "bread", 8)
    ctl("key", "1", "tap")
    off = hud_frame("fp_full_off")
    sat = hud_frame("fp_full_sat", ("saturation", "true"))
    full_preview = hud_frame("fp_full_preview", ("saturation", "false"), ("food preview", "true"))
    setting("hud", "food preview", "false")
    hungry_to()
    food = state()["player"].get("food")
    low = hud_frame("fp_low_off")
    preview = hud_frame("fp_low_preview", ("food preview", "true"))
    no_sat = hud_frame("fp_low_sat", ("food preview", "false"), ("saturation", "true"))
    setting("hud", "saturation", "false")
    for _ in range(6):
        held_keys(("space",), 3)
        wait(10)
    plain = frame("fp_ex_off")
    ex = hud_frame("fp_ex", ("exhaustion", "true"))
    cmd("difficulty peaceful")
    r = food_row()
    a, b, c = changed(off, sat, r), changed(off, full_preview, r), changed(low, preview, r)
    d, e = changed(low, no_sat, r), changed(plain, ex, r)
    ok = a > 60 and b < 20 and c > 40 and d < 20 and e > 10
    return ok, (f"fed: Saturation {a} px, bread preview {b}; hungry (food {food}): preview {c}, Saturation {d}; "
                f"exhaustion bar {e}")


def ride_horse():
    cmd(f"summon horse {X0 + 0.5} {Y + 1} {Z0 + 2.5} {{Tame:1b,NoAI:1b,PersistenceRequired:1b,"
        f"equipment:{{saddle:{{id:\"minecraft:saddle\",count:1}}}}}}")
    wait(10)
    cmd("ride @s mount @e[type=horse,limit=1,sort=nearest]")
    wait(10)
    if not ctl("status").get("riding"):
        only("automount")
        wait(40)
        only()
    return ctl("status").get("riding")


@test("Mount HUD")
def _():
    # On a horse with some XP: off, the horse's hearts take the food row and the jump bar the XP bar; on, the food
    # row is back, the hearts a row up and the XP bar (green) shows; off the horse it changes nothing there.
    hud_scene()
    cmd("xp set @s 5 levels", "xp add @s 9 points")
    w, h = hud_size()
    bar = hud_box(w / 2 - 91, h - 29, w / 2 + 91, h - 24)
    # A horse's 15 hearts take two rows: off, the food row and the one over it; on, the two over the food row.
    above = hud_box(w / 2 + 10, h - 60, w / 2 + 92, h - 51)
    walk_off = hud_frame("mh_walk_off", colour=True)
    walk_on = hud_frame("mh_walk_on", ("mount hud", "true"), colour=True)
    setting("hud", "mount hud", "false")
    riding = ride_horse()
    off = hud_frame("mh_off", colour=True)
    on = hud_frame("mh_on", ("mount hud", "true"), colour=True)
    ctl("key", "shift", "down")
    wait(5)
    ctl("key", "shift", "up")
    cmd("kill @e[type=horse]", "xp set @s 0 levels", "xp set @s 0 points")
    walking = recoloured(walk_off.crop(food_row()), walk_on.crop(food_row())) + recoloured(walk_off.crop(bar), walk_on.crop(bar))
    food, up = recoloured(off.crop(food_row()), on.crop(food_row())), recoloured(off.crop(above), on.crop(above))
    xp_off, xp_on = coloured(off, bar, greens), coloured(on, bar, greens)
    ok = riding is not None and walking < 20 and food > 40 and up > 40 and xp_on > xp_off + 20
    return ok, (f"riding {riding}; on foot {walking} px changed; riding: food row {food} px, the row above {up}, "
                f"green XP bar px {xp_off} -> {xp_on}")


@test("Shield status")
def _():
    # Bot1 hits you with an axe behind a raised shield: once it is disabled the box left of the crosshair shows (its
    # #FF5555 bar; a skin's red is not that); with Shield status off, or with no hit, nothing. Each phase waits for
    # the chat to fade first: its lines and dark backing reach the box.
    if not CTL2:
        return None, "needs a second player"
    hud_scene()
    w, h = hud_size()
    box = hud_box(w / 2 - 80, h / 2 + 10, w / 2 - 10, h / 2 + 40)

    def bar(r, g, b):
        return r > 235 and 60 < g < 110 and 60 < b < 110 and abs(g - b) < 12

    def phase(name, on, hits):
        setting("hud", "shield status", "true" if on else "false")
        bot_ready(2.5, 180, "give Bot1 minecraft:diamond_axe", "effect give @s minecraft:resistance 60 4 true",
                  f"tp @s {X0 + 0.5} {Y + 1} {Z0 + 0.5} 0 0", "item replace entity @s hotbar.0 with minecraft:shield")
        ctl2("select", "1")
        ctl("key", "1", "tap")
        ctl("look", "0", "0")
        ctl("mouse", "right", "down")
        wait(225)
        best = 0
        for i in range(max(hits, 1)):
            if hits:
                ctl2("mouse", "left", "click")
                wait(4)
            best = max(best, coloured(colour_frame(f"ss_{name}_{i}"), box, bar))
            if best > 40:
                break
            wait(20)
        ctl("mouse", "right", "up")
        wait(110)
        return best

    shown, off, idle = phase("on", True, 6), phase("off", False, 3), phase("idle", True, 0)
    bot_done()
    cmd("effect clear @s")
    if shown == 0 and (HORIZON_SERVER or not SERVER):
        return None, f"Horizon's server disabled no shield in 6 axe hits (off {off}, no hit {idle})"
    return shown > 40 and off < 10 and idle < 10, f"#FF5555 px left of the crosshair: disabled {shown}, off {off}, no hit {idle}"


def cut_pig(block, *settings, mods=("ghosthand",)):
    """A tough pig 3 blocks ahead behind `block` (in the block before it, on a grass floor), one click at it with
    `mods` on: health it lost, and whether the block is still there."""
    reset()
    slot(0, "diamond_sword")
    ctl("key", "1", "tap")
    cmd(f"setblock {X0} {Y} {Z0 + 2} minecraft:grass_block", f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:{block}",
        f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 3.0} {TOUGH_PIG}")
    for name, value in (("blocks", "false"), ("hits", "false"), ("cut through", "false"), *settings):
        setting("ghosthand", name, value)
    wait(10)
    ctl("look", "0", "27.8")
    only(*mods)
    wait(5)
    ctl("mouse", "left", "click")
    wait(25)
    only()
    m = mobs("pig")
    there = check(f"execute if block {X0} {Y + 1} {Z0 + 2} minecraft:{block}")
    cmd(f"setblock {X0} {Y + 1} {Z0 + 2} minecraft:air", f"setblock {X0} {Y} {Z0 + 2} minecraft:stone")
    return (round(1000 - m[0]["health"], 1) if m else None), there


@test("Ghost Hand (Cut through)")
def _():
    # Through short grass and a cobweb to the pig behind with Cut through; off, the click breaks the grass and the
    # pig is not hit; glass stops Cut through but not Hits (through anything).
    off = cut_pig("short_grass", mods=())
    grass = cut_pig("short_grass", ("cut through", "true"))
    web = cut_pig("cobweb", ("cut through", "true"))
    glass = cut_pig("glass", ("cut through", "true"))
    walls = cut_pig("glass", ("hits", "true"))
    note = (f"pig health lost (block kept): off {off}, grass {grass}, cobweb {web}, glass {glass}, glass with Hits "
            f"{walls}")
    # Horizon's server (single player's too) hits along your look itself and stops at the grass: the click still
    # goes to the pig (the grass is kept), and nothing is hit.
    horizon = HORIZON_SERVER or not SERVER
    hit = (lambda r: r[0] == 0) if horizon else (lambda r: (r[0] or 0) > 0)
    ok = off[0] == 0 and not off[1] and hit(grass) and grass[1] and hit(web) and web[1] and glass[0] == 0
    return ok and (horizon or (walls[0] or 0) > 0), ("Horizon's server: " if horizon else "") + note


@test("Auto Clicker (Right clicker)")
def _():
    # Right held 15 ticks with 16 snowballs: Auto Clicker's Right with Right own CPS 20 throws more than the game's own
    # repeat; Right items `blocks` (a snowball is none) and Right sneak only (standing up) leave it at the game's rate,
    # Right items `snowball` does not; Right start delay 400 ms throws fewer than none.
    def thrown(mods=()):
        reset()
        slot(0, "snowball", 16)
        ctl("key", "1", "tap")
        only(*mods)
        wait(4)
        ctl("mouse", "right", "down")
        wait(15)
        ctl("mouse", "right", "up")
        only()
        return 16 - items_of("snowball")

    for k, v in (("left", "false"), ("right", "true"), ("right own cps", "true"), ("right min cps", "20"), ("right max cps", "20")):
        setting("autoclicker", k, v)
    got = {"game": thrown(), "own": thrown(("autoclicker",))}
    setting("autoclicker", "right items", "blocks")
    got["blocks"] = thrown(("autoclicker",))
    setting("autoclicker", "right items", "snowball")
    got["listed"] = thrown(("autoclicker",))
    setting("autoclicker", "right sneak only", "true")
    got["sneak only"] = thrown(("autoclicker",))
    setting("autoclicker", "right sneak only", "false")
    setting("autoclicker", "right start delay (ms)", "400")
    got["delayed"] = thrown(("autoclicker",))
    say(".reset autoclicker")
    g = got["game"]
    ok = (got["own"] >= g + 4 and got["listed"] >= g + 4 and got["blocks"] <= g + 1 and got["sneak only"] <= g + 1
          and g < got["delayed"] < got["own"])
    return ok, f"snowballs thrown in 15 ticks: {got}"


@test("Shield (Block hit)")
def _():
    # A sword in hand and a shield in the off hand, five clicks 15 ticks apart on a husk: Manual at 100% puts the shield
    # up (a UseItem) after each hit and down before the next (every click still a hit); at 0%, with Block hit Off, or
    # with Require mouse down and the use button up, it never goes up. Predict (with Bot1 near): hurt three times 15
    # ticks apart, the shield goes up before the fourth would come; Off, not.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("shield", "auto block", "false")
    setting("shield", "block hit min chance (%)", "100")
    setting("shield", "block hit max chance (%)", "100")

    def setup():
        reset()
        slot(0, "stone_sword")
        ctl("key", "1", "tap")
        cmd("item replace entity @s weapon.offhand with minecraft:shield")

    def clicks(mode, **kv):
        setting("shield", "block hit", mode)
        for k, v in kv.items():
            setting("shield", k.replace("_", " "), v)
        setup()
        cmd("difficulty easy", f"summon husk {X0 + 0.5} {Y + 1} {Z0 + 2.5} {HUSK}")
        wait(10)
        eid = next(e["id"] for e in ctl("entities")["entities"] if e.get("type") == "minecraft:husk")
        ctl("look-at-entity", str(eid))
        only("shield")
        mark = probe_mark()
        for _ in range(5):
            ctl("mouse", "left", "click")
            wait(15)
        only()
        ticks = probe_ticks(mark)
        cmd("kill @e[type=husk]")
        return sum(t.count("order attack") for t in ticks), sum(t.count(f"pkt {USE_ITEM}") for t in ticks)

    got = {"manual": clicks("Manual")}
    got["0%"] = clicks("Manual", **{"block_hit_min_chance_(%)": "0", "block_hit_max_chance_(%)": "0"})
    setting("shield", "block hit min chance (%)", "100")
    setting("shield", "block hit max chance (%)", "100")
    got["needs mouse"] = clicks("Manual", require_mouse_down="true")
    setting("shield", "require mouse down", "false")
    got["off"] = clicks("Off")
    ok = got["manual"][0] >= 4 and got["manual"][1] >= 4 and all(got[k][1] == 0 for k in ("0%", "needs mouse", "off"))
    note = f"(hits, shield raises) over five clicks: {got}"
    if CTL2:
        predicted = {}
        for mode in ("Off", "Predict"):
            setting("shield", "block hit", mode)
            setup()
            bot_ready(3.0, 180)
            only("shield")
            for i in range(3):
                cmd("damage @s 1 minecraft:generic")
                if i == 2:
                    mark = probe_mark()
                wait(15)
            wait(5)
            only()
            predicted[mode] = sum(t.count(f"pkt {USE_ITEM}") for t in probe_ticks(mark))
        bot_done()
        ok = ok and predicted["Off"] == 0 and predicted["Predict"] >= 1
        note += f"; Predict, raises before the fourth hurt: {predicted}"
    say(".reset shield")
    return ok, note


@test("No Interact (Hit select)")
def _():
    # Twelve clicks 2 ticks apart on Bot1 (2 blocks away, no knockback) with a sword: every click hits without Hit
    # select; Cancel at 100% holds those that come while Bot1 is still hurt, so far fewer go; Cancel at 0% chance rests
    # 8 ticks after each failed roll, so more go than at 100%; Sprint reset lets through only those landing as the
    # hurt time runs out.
    if not CTL2:
        return None, "needs a second player"

    def hits(mode=None, chance="100"):
        if mode:
            setting("nointeract", "hit select", mode)
            setting("nointeract", "hit select chance (%)", chance)
        reset()
        slot(0, "stone_sword")
        ctl("key", "1", "tap")
        bot_ready(2.0, 180, "attribute Bot1 minecraft:knockback_resistance base set 1")
        bot = next(e["id"] for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:player")
        only(*(("nointeract",) if mode else ()))
        wait(5)
        mark = probe_mark()
        for _ in range(12):
            ctl("look-at-entity", str(bot))
            ctl("mouse", "left", "click")
            wait(2)
        only()
        return sum(t.count("order attack") for t in probe_ticks(mark))

    got = {"off": hits(), "cancel": hits("Cancel"), "cancel 0%": hits("Cancel", "0"), "sprint reset": hits("Sprint reset")}
    cmd("attribute Bot1 minecraft:knockback_resistance base set 0")
    bot_done()
    say(".reset nointeract")
    ok = got["off"] >= 5 and got["cancel"] <= got["off"] / 2 and got["cancel 0%"] > got["cancel"] and got["sprint reset"] < got["off"]
    return ok, f"hits out of twelve clicks: {got}"


@test("No Slow (No item release)")
def _():
    # A golden apple held on the use button for half a second, then let go: the server never hears of the release with
    # No item release (food listed), so it finishes eating it; without it, or with only swords listed, it is not eaten.
    def eaten(mods=(), items="swords,food,potions"):
        setting("noslow", "no item release", "true")
        setting("noslow", "release items", items)
        reset()
        slot(0, "golden_apple", 4)
        ctl("key", "1", "tap")
        only(*mods)
        wait(4)
        ctl("mouse", "right", "down")
        wait(10)
        ctl("mouse", "right", "up")
        wait(45)
        only()
        return 4 - items_of("golden_apple")

    got = {"off": eaten(), "on": eaten(("noslow",)), "swords only": eaten(("noslow",), "swords")}
    say(".reset noslow")
    return got == {"off": 0, "on": 1, "swords only": 0}, f"apples eaten after letting go early: {got}"


@test("Key Throw (Pot key, Debuff key)")
def _():
    # Pot key: five healing splash potions in slots 5 to 9 at 10 health, looking down: Dynamic throws three (4 health
    # each for the 10 missing), Single one, Key Throw off none, and the sword is in hand after. Debuff key, looking
    # ahead: poison, weakness, poison and harming in slots 1 to 4: One of each throws three, All four, First one, and
    # One of each with Debuffs `poison` one.
    for k, v in (("pot key", "h"), ("debuff key", "j"), ("throw mode", "Silent")):
        setting("keythrow", k, v)

    def pots(mods=("keythrow",), amount="Dynamic"):
        setting("keythrow", "pot amount", amount)
        reset()
        slot(0, "diamond_sword")
        for i in range(4, 9):
            slot(i, HEAL_POT)
        ctl("key", "1", "tap")
        cmd("damage @s 10 minecraft:generic")
        ctl("look", "0", "90")
        only(*mods)
        wait(5)
        ctl("key", "h", "tap")
        wait(30)
        only()
        return 5 - items_of("splash_potion"), held().get("item", "").split(":")[-1]

    def debuffs(mode, kinds="harming,weakness,poison,slowness"):
        setting("keythrow", "debuff mode", mode)
        setting("keythrow", "debuffs", kinds)
        reset()
        immune()
        for i, p in enumerate(("poison", "weakness", "poison", "harming")):
            slot(i, f'splash_potion[potion_contents={{potion:"minecraft:{p}"}}]')
        slot(5, "diamond_sword")
        ctl("key", "6", "tap")
        ctl("look", "0", "-20")
        only("keythrow")
        wait(5)
        ctl("key", "j", "tap")
        wait(30)
        only()
        return 4 - items_of("splash_potion")

    got = {"dynamic": pots(), "single": pots(amount="Single"), "off": pots(())}
    got.update({"one of each": debuffs("One of each"), "all": debuffs("All"), "first": debuffs("First"),
                "poison only": debuffs("One of each", "poison")})
    say(".reset keythrow")
    ok = (got["dynamic"] == (3, "diamond_sword") and got["single"] == (1, "diamond_sword") and got["off"][0] == 0
          and (got["one of each"], got["all"], got["first"], got["poison only"]) == (3, 4, 1, 1))
    return ok, f"(potions thrown, in hand after) / debuffs thrown: {got}"


@test("Auto Gap (Soup)")
def _():
    # A sword in hand and mushroom stew in slot 4 at 10 health: with Soup the stew's slot is held and used (a UseItem),
    # each before its tick's move packet, and the sword is back after; at 20 health, or with Soup health 5, nothing.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("autogap", "health enabled", "false")
    setting("autogap", "soup", "true")

    def run(damage, soup_health="17"):
        setting("autogap", "soup health", soup_health)
        reset()
        slot(0, "diamond_sword")
        slot(3, "mushroom_stew")
        ctl("key", "1", "tap")
        if damage:
            cmd(f"damage @s {damage} minecraft:generic")
        wait(5)
        mark = probe_mark()
        only("autogap")
        wait(20)
        only()
        wait(5)
        ticks = probe_ticks(mark)
        uses = sum(t.count(f"pkt {USE_ITEM}") for t in ticks)
        return uses, held().get("item", "").split(":")[-1], cart_order(mark)

    got = {"hurt": run(10), "full": run(0), "under 5": run(10, "5")}
    say(".reset autogap")
    ok = got["hurt"][0] >= 1 and got["hurt"][1] == "diamond_sword" and not any(got["hurt"][2]) and got["full"][0] == 0 and got["under 5"][0] == 0
    return ok, f"(stew uses, in hand after, (late, two slot changes, two moves)): {got}"


@test("Arrow Dodge (Hit fireballs)")
def _():
    # A large fireball flying at you from 6 blocks: with Hit fireballs it is hit (an attack) and sent back away from
    # you; with Arrow Dodge on but Hit fireballs off, or Arrow Dodge off, it is not hit.
    if not probe_path():
        return None, "needs the probe (tools/probe)"
    setting("arrowdodge", "dodge", "false")

    def run(mods, hit="true"):
        setting("arrowdodge", "hit fireballs", hit)
        reset()
        immune()
        cmd("gamerule mob_griefing false")
        only(*mods)
        wait(5)
        mark = probe_mark()
        cmd(f"summon minecraft:fireball {X0 + 0.5} {Y + 2.2} {Z0 + 6.5} {{Motion:[0.0,0.0,-0.4],acceleration_power:0.0d}}")
        wait(25)
        only()
        attacks = sum(t.count("order attack") for t in probe_ticks(mark))
        balls = [e for e in ctl("entities").get("entities", []) if e.get("type") == "minecraft:fireball"]
        away = any(e.get("pos", e.get("position", [0, 0, 0]))[2] > Z0 + 6.5 for e in balls)
        cmd("kill @e[type=minecraft:fireball]")
        return attacks, away

    got = {"on": run(("arrowdodge",)), "hit off": run(("arrowdodge",), "false"), "module off": run(())}
    cmd("gamerule mob_griefing true")
    say(".reset arrowdodge")
    ok = got["on"][0] >= 1 and got["hit off"][0] == 0 and got["module off"][0] == 0
    return ok, f"(attacks, fireball sent back past where it came from): {got}"


@test("Auto Armor (Switch key)")
def _():
    # Wearing iron with a diamond set in the inventory, Mode Key with no Armor key, Switch sets `diamond,iron`: nothing
    # changes until G, then all four diamond pieces go on; G again puts the iron back; with the diamond boots gone the
    # set is not whole and G changes nothing.
    pieces = ("head", "chest", "legs", "feet")
    names = {"head": "helmet", "chest": "chestplate", "legs": "leggings", "feet": "boots"}

    def wearing(material):
        worn = [(a.get("item") or "").split(":")[-1] for a in ctl("inventory").get("armor", [])]
        return worn == [f"{material}_{names[p]}" for p in pieces]

    reset()
    cmd("clear @s", *[f"item replace entity @s armor.{p} with minecraft:iron_{names[p]}" for p in pieces],
        *[f"item replace entity @s inventory.{i} with minecraft:diamond_{names[p]}" for i, p in enumerate(pieces)])
    for k, v in (("armor mode", "Key"), ("armor key", "None"), ("switch key", "G"), ("switch sets", "diamond,iron"), ("armor delay (ticks)", "4")):
        setting("inventorymanager", k, v)
    only()
    setting("inventorymanager", "auto armor", "true")
    wait(30)
    before = wearing("iron")
    ctl("key", "g", "tap")
    wait(50)
    diamond = wearing("diamond")
    ctl("key", "g", "tap")
    wait(50)
    iron = wearing("iron")
    # The boots went back to the slot they came from (inventory.3); `clear @s <item>` is not the same on every server.
    cmd("item replace entity @s inventory.3 with minecraft:air")
    wait(5)
    boots = items_of("diamond_boots") == 0
    ctl("key", "g", "tap")
    wait(50)
    kept = wearing("iron")
    only()
    say(".reset inventorymanager")
    return before and diamond and iron and boots and kept, (f"iron before G {before}, diamond after G {diamond}, iron after G again {iron}, "
                                                            f"boots gone {boots}, iron kept without them {kept}")


@test("Loadouts (Auto hotbar)")
def _():
    # Rules `1:sword,9:ender_pearl`, dirt in slot 1, a stone sword in slot 4, a diamond sword and pearls in the main
    # inventory: Always puts the diamond sword in slot 1 and the pearls in slot 9; Inventory open does nothing with no
    # screen and the same once the inventory is open; with slot 1 locked only the pearls move.
    def run(mode, open_inv=False, locked=""):
        for k, v in (("auto hotbar", "true"), ("hotbar mode", mode), ("hotbar rules", "1:sword,9:ender_pearl"), ("locked slots", locked or "None")):
            setting("loadouts", k, v)
        reset()
        cmd("clear @s", "item replace entity @s hotbar.0 with minecraft:dirt 8", "item replace entity @s hotbar.3 with minecraft:stone_sword",
            "item replace entity @s inventory.2 with minecraft:diamond_sword", "item replace entity @s inventory.7 with minecraft:ender_pearl 16")
        only("loadouts")
        wait(20)
        if open_inv:
            ctl("open-inventory")
            wait(20)
            ctl("close")
        only()
        bar = ctl("inventory").get("hotbar", [])
        name = lambda i: ((bar[i] or {}).get("item") or "").split(":")[-1] if i < len(bar) else ""
        return name(0), name(8)

    got = {"always": run("Always"), "closed": run("Inventory open"), "open": run("Inventory open", True), "locked": run("Always", locked="1")}
    say(".reset loadouts")
    ok = (got["always"] == ("diamond_sword", "ender_pearl") and got["closed"] == ("dirt", "") and got["open"] == ("diamond_sword", "ender_pearl")
          and got["locked"] == ("dirt", "ender_pearl"))
    return ok, f"(slot 1, slot 9): {got}"


@test("Client Sort (Shift fill)")
def _():
    # The inventory open, the pointer on the first main inventory slot (empty), Shift and the left button held (real
    # presses, xdotool), then dirt put in that slot: Shift fill shift-clicks it into the hotbar; with Shift fill off,
    # or without Shift, it stays.
    window = x_window()
    if not window:
        return None, "needs xdotool and the game's window"
    env = {**os.environ, "DISPLAY": XDISPLAY}

    def xdo(*args):
        subprocess.run(["xdotool", *args], env=env, capture_output=True)

    def run(fill="true", shift=True):
        setting("clientsort", "shift fill", fill)
        reset()
        cmd("clear @s")
        only("clientsort")
        ctl("open-inventory")
        wait(6)
        w, h = gui_size()
        ctl("screenshot", f"{SHOTS}/shift_fill.png")
        time.sleep(2)
        from PIL import Image
        img = Image.open(f"{SHOTS}/shift_fill.png").convert("L")
        k = img.width / w
        book = 77 if img.getpixel((int(((w - 176) / 2 + 10) * k), int(((h - 166) / 2 + 60) * k))) < 90 else 0
        # Bot1's window (on the same display) can lie over this one: raised first, so the pointer's presses land here.
        xdo("windowraise", window)
        xdo("windowfocus", "--sync", window)
        xdo("mousemove", "--window", window, str(int(((w - 176) / 2 + book + 16) * k)), str(int(((h - 166) / 2 + 92) * k)))
        wait(4)
        if shift:
            xdo("keydown", "shift")
        xdo("mousedown", "1")
        wait(4)
        cmd("item replace entity @s inventory.0 with minecraft:dirt 10")
        wait(20)
        xdo("mouseup", "1")
        if shift:
            xdo("keyup", "shift")
        inv = ctl("inventory")
        ctl("close")
        only()
        main = (inv.get("main") or [{}])[0] or {}
        return "dirt" in (main.get("item") or "")

    got = {"on": run(), "fill off": run("false"), "no shift": run(shift=False)}
    say(".reset clientsort")
    return got == {"on": False, "fill off": True, "no shift": True}, f"dirt still in the main inventory slot: {got}"


@test("Auto Weapon (Mode)")
def _():
    # Kill Aura at a pig with nothing in hand, a sword in slot 2 and an axe in slot 3 (each kept 10 ticks after a hit,
    # Back after A delay): Mode Sword takes only the sword, Mode Axe only the axe, Custom with every section off neither.
    setting("killaura", "entities", "pig")
    setting("killaura", "attack when holding", "All")
    setting("autoweapon", "holding", "empty")
    setting("autoweapon", "back after", "A delay")
    setting("autoweapon", "shortest delay", "10")
    setting("autoweapon", "longest delay", "10")
    weapon()
    seen = {}
    for mode in ("Sword", "Axe", "Custom"):
        reset()
        slot(1, "diamond_sword")
        slot(2, "diamond_axe")
        ctl("key", "1", "tap")
        setting("autoweapon", "mode", mode)
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 2.0} {TOUGH_PIG}")
        wait(10)
        only("killaura", "autoweapon")
        items = poll(60, 2, lambda: held().get("item", ""))
        only()
        seen[mode] = sorted({i.replace("minecraft:", "") for i in items if i})
    setting("autoweapon", "mode", "Custom")
    setting("autoweapon", "back after", "Same tick")
    weapon("best weapon", "axe on shields")
    ok = seen["Sword"] == ["diamond_sword"] and seen["Axe"] == ["diamond_axe"] and seen["Custom"] == []
    return ok, f"items seen in hand: Sword {seen['Sword']}, Axe {seen['Axe']}, Custom with no sections {seen['Custom']}"


@test("Auto Spear (Lunge on hit)")
def _():
    # A pig 2.5 blocks ahead, a sword in hand and an iron spear with Lunge II in slot 3: your clicks. With Auto Spear's
    # Lunge on hit each click goes as a jab of the spear (the lunge carries you toward the pig) and the sword comes
    # back; off, plain sword hits and you stay put.
    got = {}
    for name, on in (("off", False), ("on", True)):
        reset()
        slot(0, "diamond_sword")
        slot(2, "iron_spear[enchantments={lunge:2}]")
        ctl("key", "1", "tap")
        setting("autospear", "holding", "sword,axe")
        setting("autospear", "lunge on hit", "true" if on else "false")
        cmd(f"summon pig {X0 + 0.5} {Y + 1} {Z0 + 3.0} {TOUGH_PIG}")
        wait(10)
        ctl("look-at-entity", str(mobs("pig")[0]["id"]))
        only("autospear")
        a = pos()
        for _ in range(3):
            ctl("mouse", "left", "click")
            wait(25)
        moved = round(flat(a, pos()), 2)
        only()
        m = mobs("pig")
        got[name] = (round(1000 - m[0]["health"], 1) if m else None, moved, held().get("item", "").replace("minecraft:", ""))
    setting("autospear", "lunge on hit", "false")
    setting("autospear", "holding", "sword,axe,spear,mace,trident,empty")
    off, on = got["off"], got["on"]
    ok = (on[0] or 0) > 0 and on[1] > off[1] + 0.5 and "sword" in on[2]
    return ok, f"(health lost, moved, hand after) without Lunge on hit {off}, with it {on}"


@test("No Render (Remove slowness)")
def _():
    # Slowness IV on you, walking forward 40 ticks: with No Render's Remove slowness (single player: the server here
    # is your own) you walk much further than with it off.
    walked = {}
    for name, on in (("off", False), ("on", True)):
        reset()
        cmd("effect give @s minecraft:slowness 60 3 true")
        setting("norender", "remove slowness", "true" if on else "false")
        only("norender")
        wait(10)
        a = pos()
        ctl("key", "w", "down")
        wait(40)
        ctl("key", "w", "up")
        walked[name] = round(flat(a, pos()), 2)
        only()
    setting("norender", "remove slowness", "false")
    cmd("effect clear @s minecraft:slowness")
    return walked["on"] > walked["off"] * 1.5, f"blocks walked in 40 ticks under Slowness IV: off {walked['off']}, on {walked['on']}"


def sharpness(name, box):
    """How much neighbouring pixels differ in `box` of a screenshot: lower when the picture is blurred."""
    from PIL import Image
    path = f"{SHOTS}/{name}.png"
    ctl("screenshot", path)
    time.sleep(2)
    img = Image.open(path).convert("L").crop(box)
    px = img.load()
    w, h = img.size
    return sum(abs(px[x, y] - px[x + 1, y]) for y in range(h) for x in range(w - 1)) / max(1, (w - 1) * h)


@test("ClickGUI (Inventory blur)")
def _():
    # The inventory open over the floor, Menu dim on: the world beside the inventory's panel is sharp without
    # Inventory blur and blurred with it.
    reset("creative")
    ctl("look", "0", "50")
    setting("clickgui", "menu dim", "true")
    box = (8, 40, 120, 200)
    got = {}
    for name, on in (("off", False), ("on", True)):
        setting("clickgui", "inventory blur", "true" if on else "false")
        ctl("open-inventory")
        wait(10)
        got[name] = round(sharpness(f"ib_{name}", box), 2)
        ctl("close")
        wait(5)
    setting("clickgui", "inventory blur", "false")
    setting("clickgui", "menu dim", "false")
    return got["on"] < got["off"] * 0.6, f"sharpness beside the inventory: off {got['off']}, on {got['on']}"


@test("ESP (Murderer finder)")
def _():
    # Bot1 with an iron sword in hand: ESP's Murderer finder with Callout says so in chat, once.
    if not CTL2:
        return None, "needs a second player"
    reset()
    setting("esp", "murderer finder", "true")
    setting("esp", "callout", "true")
    setting("esp", "callout delay (ms)", "0")
    setting("esp", "callout messages", "%s is the murderer!")
    bot_ready(2.5, 180, "give Bot1 minecraft:iron_sword")
    ctl2("select", "1")
    wait(10)
    only("esp")
    wait(40)
    only()
    bot_done()
    lines = [line for line in ctl("chat-log", "20").get("chat", []) if "is the murderer" in str(line)]
    setting("esp", "murderer finder", "false")
    setting("esp", "callout", "false")
    ok = len(lines) == 1 and "Bot1" in str(lines[0])
    return ok, f"chat lines naming a murderer: {lines}"


@test("HUD layout editor (chat)")
def _():
    # With the chat open, Coordinates (top left), Compass (top middle) and the Armor HUD (by the hotbar) are dragged to
    # the middle of the screen, the left and the top right: afterwards each draws there, no longer where it was.
    hud_scene()
    w, h = hud_size()
    wear()
    for name, value in (("watermark", "false"), ("module list", "false"), ("fps", "false"), ("speed", "false"),
                        ("facing", "false"), ("totems", "false"), ("coordinates position", "Top left"),
                        ("compass position", "Top")):
        setting("hud", name, value)
    off = hud_frame("le_off", ("coordinates", "false"), ("compass", "false"), ("armor hud", "false"))
    before = hud_frame("le_before", ("coordinates", "true"), ("compass", "true"), ("armor hud", "true"))
    places = {
        "coordinates": ((6, 6), (w / 2 - 20, h / 2 - 30), hud_box(0, 0, 60, 12), hud_box(w / 2 - 30, h / 2 - 40, w / 2 + 60, h / 2 - 20)),
        "compass": ((w / 2, 16), (100, h / 2 + 20), hud_box(w / 2 - 80, 8, w / 2 + 80, 28), hud_box(15, h / 2 + 5, 185, h / 2 + 35)),
        "armor hud": ((w / 2 + 110, h - 10), (w - 50, 40), hud_box(w / 2 + 95, h - 23, w / 2 + 179, h), hud_box(w - 100, 25, w, 55)),
    }
    ctl("key", "t", "tap")
    wait(5)
    for (x0, y0), (x1, y1), _, _ in places.values():
        ctl("drag-at", str(x0), str(y0), str((x0 + x1) / 2), str((y0 + y1) / 2), str(x1), str(y1))
        wait(4)
    ctl("key", "esc", "tap")
    after = hud_frame("le_after")
    report, ok = [], True
    for name, (_, _, old, new) in places.items():
        was, left, moved = changed(off, before, old), changed(off, after, old), changed(off, after, new)
        ok = ok and was > 100 and left < was / 4 and moved > 100
        report.append(f"{name}: {was} px where it was, {left} there after, {moved} where dropped")
    for name in ("coordinates", "compass", "armor hud"):
        setting("hud", f"{name} position", "Top left" if name == "coordinates" else "Top" if name == "compass" else "Hotbar")
    setting("hud", "compass", "false")
    return ok, "; ".join(report)


if __name__ == "__main__":
    import signal
    has_alarm = hasattr(signal, "SIGALRM")
    if has_alarm:
        signal.signal(signal.SIGALRM, overran)
    arena()
    # A test that ends Sunset's session (Hard Panic) goes after every other one, and nothing is cleaned up after it.
    for fn in sorted(TESTS, key=lambda t: getattr(t, "ends_session", False)):
        if ONLY and fn.test_name not in ONLY:
            continue
        if has_alarm:
            signal.alarm(600)
        try:
            ok, note = fn()
        except Exception as e:  # noqa: BLE001
            ok, note = False, f"error: {e!r}"
            traceback.print_exc()
        if has_alarm:
            signal.alarm(0)
        rejoin(fn.test_name)
        if ok is not None:
            results.append((fn.test_name, ok, note))
        print("SKIP" if ok is None else "PASS" if ok else "FAIL", fn.test_name, "-", note, flush=True)
        if getattr(fn, "ends_session", False):
            break
        (light_clean if getattr(fn, "light", False) else clean)()
    p = sum(1 for _, ok, _ in results if ok)
    print(f"\n{p}/{len(results)} passed")
    for test, screen in kicks:
        print(f"KICKED during {test}: {screen}")
