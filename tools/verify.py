#!/usr/bin/env python3
# Runs each Sunset module in a flat test arena in a headless Horizon (see
# tools/README in the repo's notes: ctl = horizon-ctl on the test session) and
# checks a measurable result. Usage: verify.py CTL_PATH [test names...]
# Tests that need a second player (on a dedicated server) use the control
# wrapper in the CTL2 environment variable and are skipped without it.
# The per-frame Aim Assist tests read the test probe's file (tools/probe,
# staged next to Sunset on the test machine; the game writes it to the path in
# SUNSET_PROBE): PROBE names it, else <session>/probe.log. They move mobs with
# a small data pack written into the world folder: WORLD names it, else the
# single player world of the session or ~/hzmeteor/srv/world on a server.
import json, math, os, subprocess, sys, time

CTL = sys.argv[1]
CTL2 = os.environ.get("CTL2")
ONLY = sys.argv[2:]
on = set()
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
    ctl("key", "t", "tap")
    wait(1)
    ctl("type", text)
    ctl("key", "enter", "tap")
    wait(1)


def state():
    return ctl("state")


def pos():
    return state()["player"]["position"]


def hp():
    return state()["player"]["health"]


def chat_first():
    c = ctl("chat-log", "1").get("chat", [])
    return c[0] if c else ""


def check(cmd):
    say(cmd)
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
    say(f".set {mod} {name} {value}")


X0 = Z0 = 0
Y = 120


def arena():
    global X0, Z0
    p = pos()
    X0, Z0 = int(p[0] // 1), int(p[2] // 1)
    # Unattended, the game drops to 30 frames a second after a minute ("afk"): keep the full rate.
    ctl("options-set", "inactivityFpsLimit", "minimized")
    say("/gamemode creative")
    say("/difficulty peaceful")
    say("/time set 6000")
    say("/weather clear")
    # A world with real terrain (a server's) has hills round the platform: clear a wide ring first.
    say(f"/fill {X0-24} {Y-1} {Z0-24} {X0+24} {Y+4} {Z0+24} minecraft:air")
    say(f"/fill {X0-24} {Y+5} {Z0-24} {X0+24} {Y+12} {Z0+24} minecraft:air")
    say(f"/fill {X0-16} {Y} {Z0-16} {X0+16} {Y} {Z0+16} minecraft:stone")


def ensure_alive():
    for _ in range(3):
        st = state()
        if st.get("screen") != "dead" and "player" in st:
            return
        ctl("click", "respawn")
        wait(30)


def reset(mode="survival", x=0.5, z=0.5, y=Y + 1):
    ensure_alive()
    say(f"/gamemode {mode}")
    say("/clear @s")
    say("/effect clear @s")
    say("/effect give @s minecraft:instant_health 1 10 true")
    say("/effect give @s minecraft:saturation 1 10 true")
    say("/kill @e[type=!player]")
    say(f"/tp @s {X0 + x} {y} {Z0 + z}")
    wait(20)
    ctl("look", "0", "0")


def walk(ticks, key="w", extra=None):
    ctl("key", key, "down")
    if extra:
        ctl("key", extra, "down")
    wait(ticks)
    ctl("key", key, "up")
    if extra:
        ctl("key", extra, "up")
    wait(3)


def test(name):
    def deco(fn):
        fn.test_name = name
        TESTS.append(fn)
        return fn
    return deco


TESTS = []


def dist_walk(*mods, ticks=30, extra=None):
    reset()
    only(*mods)
    p0 = pos()
    walk(ticks, extra=extra)
    p1 = pos()
    only()
    return ((p1[0] - p0[0]) ** 2 + (p1[2] - p0[2]) ** 2) ** 0.5


@test("Speed")
def _():
    base = dist_walk()
    fast = dist_walk("speed")
    return fast > base * 1.5, f"{base:.1f} -> {fast:.1f} blocks"


@test("Sprint")
def _():
    base = dist_walk()
    sp = dist_walk("sprint")
    return sp > base * 1.15, f"{base:.1f} -> {sp:.1f} blocks"


@test("Auto Walk")
def _():
    reset()
    only("autowalk")
    p0 = pos()
    wait(30)
    p1 = pos()
    only()
    return p1[2] - p0[2] > 2, f"moved {p1[2]-p0[2]:.1f} blocks"


@test("Flight")
def _():
    reset()
    only("flight")
    y0 = pos()[1]
    ctl("key", "space", "down")
    wait(30)
    ctl("key", "space", "up")
    y1 = pos()[1]
    wait(30)
    y2 = pos()[1]
    only()
    return y1 > y0 + 3 and abs(y2 - y1) < 1, f"up {y1-y0:.1f}, then held {y2-y1:+.1f}"


@test("No Fall")
def _():
    reset()
    say(f"/tp @s {X0+0.5} {Y+12} {Z0+0.5}")
    wait(50)
    base = hp()
    reset()
    only("nofall")
    say(f"/tp @s {X0+0.5} {Y+12} {Z0+0.5}")
    wait(50)
    mine = hp()
    only()
    return mine == 20 and base < 20, f"health {base} without, {mine} with"


def wall(height, at=3):
    say(f"/fill {X0-3} {Y+1} {Z0+at} {X0+3} {Y+height} {Z0+at} minecraft:stone")


def unwall():
    say(f"/fill {X0-3} {Y+1} {Z0+1} {X0+3} {Y+12} {Z0+6} minecraft:air")


@test("Step")
def _():
    reset()
    unwall()
    wall(1)
    walk(40)
    base = pos()[2]
    reset()
    only("step")
    walk(40)
    mine = pos()[2]
    only()
    unwall()
    return mine > Z0 + 4 and base < Z0 + 3, f"z {base-Z0:.1f} blocked, {mine-Z0:.1f} with step"


@test("Step height")
def _():
    reset()
    unwall()
    wall(2)
    only("step")
    setting("step", "height", "2.25")
    walk(40)
    high = pos()[2]
    setting("step", "active when", "Sneaking")
    reset()
    walk(40)
    held = pos()[2]
    setting("step", "active when", "Always")
    setting("step", "height", "1.25")
    only()
    unwall()
    return high > Z0 + 4 and held < Z0 + 3, f"z {high-Z0:.1f} over 2 blocks, {held-Z0:.1f} when only active sneaking"


def brightness(name):
    path = f"/tmp/{name}.png"
    ctl("screenshot", path)
    time.sleep(2)
    out = subprocess.run(["identify", "-format", "%[fx:mean]", path], capture_output=True, text=True).stdout
    return float(out)


@test("No Render (blindness)")
def _():
    reset("creative")
    say("/effect give @s minecraft:blindness 90 0 true")
    wait(40)
    dark = brightness("nr_blind")
    only("norender")
    wait(30)
    clear = brightness("nr_clear")
    only()
    say("/effect clear @s")
    return clear > dark * 1.5, f"brightness {dark:.2f} blinded, {clear:.2f} with No Render"


@test("No Render (water)")
def _():
    reset("creative")
    say(f"/fill {X0-2} {Y+1} {Z0-2} {X0+2} {Y+6} {Z0+2} minecraft:water")
    say(f"/tp @s {X0+0.5} {Y+3} {Z0+0.5}")
    wait(40)
    wet = brightness("nr_wet")
    only("norender")
    wait(30)
    dry = brightness("nr_dry")
    only()
    say(f"/fill {X0-2} {Y+1} {Z0-2} {X0+2} {Y+6} {Z0+2} minecraft:air")
    return dry > wet * 1.2, f"brightness {wet:.2f} in water, {dry:.2f} with No Render"


def inventory_brightness(name):
    ctl("open-inventory")
    wait(10)
    value = brightness(name)
    ctl("close")
    return value


@test("Menu dim (inventory)")
def _():
    reset("survival")
    ctl("look", "0", "10")
    only()
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
    only()
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
        only()
        setting("clickgui", "menu dim", "true")
        dim = inventory_brightness("md_scale_dark")
        setting("clickgui", "menu dim", "false")
        clear = inventory_brightness("md_scale_clear")
        ok = ok and clear > dim * 1.3
        notes.append(f"scale {n}: {dim:.2f} -> {clear:.2f}")
    ctl("options-set", "guiScale", str(scale))
    return ok, ", ".join(notes)


@test("Flight (no fall damage, level hover)")
def _():
    reset("survival")
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
    return abs(drift) < 0.05 and health >= 20, f"hovered {drift:+.3f} blocks in 5 s, health {health} after flying up 15 and sneaking down"


@test("No Fall (after Flight)")
def _():
    reset("survival")
    only("flight", "nofall")
    ctl("key", "space", "down")
    wait(80)
    ctl("key", "space", "up")
    only("nofall")
    wait(80)
    health = hp()
    only()
    return health >= 20, f"health {health} after dropping from {pos()[1]-Y:.0f}"


@test("GUI Move")
def _():
    reset("survival")
    only("guimove")
    ctl("open-inventory")
    wait(6)
    start = pos()
    ctl("key", "w", "down")
    wait(30)
    ctl("key", "w", "up")
    end = pos()
    ctl("close")
    only()
    moved = ((end[0] - start[0]) ** 2 + (end[2] - start[2]) ** 2) ** 0.5
    return moved > 2.0, f"walked {moved:.1f} blocks with the inventory open"


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
    return ((end[0] - start[0]) ** 2 + (end[2] - start[2]) ** 2) ** 0.5


def leave_pause():
    ctl("resume")


@test("GUI Move (pause menu)")
def _():
    reset("survival")
    only("guimove")
    ctl("pause")
    wait(4)
    stopped = state().get("paused")
    ctl("resume")
    if stopped:
        only()
        return None, "single player stops the world behind the pause menu: nothing can move there"
    wait(4)
    all_menus = walked_in(lambda: ctl("pause"), leave_pause)
    setting("guimove", "screens", "Inventory")
    reset("survival")
    inventory_only = walked_in(lambda: ctl("pause"), leave_pause)
    setting("guimove", "screens", "All menus")
    only()
    return all_menus > 2.0 and inventory_only < 0.5, f"walked {all_menus:.1f} blocks in the pause menu, {inventory_only:.1f} with Screens on Inventory"


@test("GUI Move (not in chat)")
def _():
    reset("survival")
    only("guimove")

    def open_chat():
        ctl("key", "t", "tap")

    def close_chat():
        ctl("key", "esc", "tap")

    moved = walked_in(open_chat, close_chat)
    only()
    return moved < 0.5, f"walked {moved:.1f} blocks with the chat open"


@test("GUI Move (with Flight)")
def _():
    reset("survival")
    only("guimove", "flight")
    ctl("open-inventory")
    wait(6)
    start = pos()
    ctl("key", "w", "down")
    wait(30)
    ctl("key", "w", "up")
    end = pos()
    ctl("close")
    only()
    moved = ((end[0] - start[0]) ** 2 + (end[2] - start[2]) ** 2) ** 0.5
    return moved > 6.0, f"flew {moved:.1f} blocks in 1.5 s with the inventory open"


@test("Nuker (survival)")
def _():
    reset("survival")
    say("/item replace entity @s hotbar.0 with minecraft:iron_pickaxe")
    ctl("key", "1", "tap")
    wait(2)
    say(f"/fill {X0-1} {Y+1} {Z0+1} {X0+1} {Y+2} {Z0+2} minecraft:stone")
    wait(4)
    only("nuker")
    wait(60)
    left = sum(1 for dx in (-1, 0, 1) for dy in (1, 2) for dz in (1, 2)
               if check(f"/execute if block {X0+dx} {Y+dy} {Z0+dz} minecraft:stone"))
    only()
    broken = 12 - left
    return broken >= 6, f"{broken} of 12 stone blocks broken in 3 s with an iron pickaxe"


@test("Nuker (creative shred)")
def _():
    reset("creative")
    say(f"/fill {X0-3} {Y+1} {Z0-3} {X0+3} {Y+4} {Z0+3} minecraft:stone")
    say(f"/fill {X0} {Y+1} {Z0} {X0} {Y+2} {Z0} minecraft:air")
    say(f"/tp @s {X0+0.5} {Y+1} {Z0+0.5}")
    wait(10)
    only("nuker")
    wait(20)
    n = 0
    for dx in range(-3, 4):
        for dy in range(1, 5):
            for dz in range(-3, 4):
                if ctl("block", str(X0 + dx), str(Y + dy), str(Z0 + dz)).get("block") == "minecraft:stone":
                    n += 1
    only()
    broken = 194 - n
    return broken >= 150, f"{broken} of 194 blocks broken in 1 s in creative"


@test("Auto Steal")
def _():
    reset("survival")
    say(f"/setblock {X0+1} {Y+1} {Z0} minecraft:chest[facing=west]{{Items:[{{Slot:0b,id:\"minecraft:diamond\",count:5}},{{Slot:1b,id:\"minecraft:gold_ingot\",count:3}},{{Slot:5b,id:\"minecraft:emerald\",count:2}}]}}")
    wait(6)
    only("autosteal")
    ctl("interact", str(X0 + 1), str(Y + 1), str(Z0))
    wait(30)
    counts = ctl("inventory").get("counts", {})
    got = counts.get("minecraft:diamond") == 5 and counts.get("minecraft:emerald") == 2
    ctl("close")
    only()
    return got, "chest emptied into the inventory" if got else "items still in the chest"


@test("Inventory Cleaner")
def _():
    reset("survival")
    say("/item replace entity @s inventory.3 with minecraft:rotten_flesh 12")
    say("/item replace entity @s inventory.4 with minecraft:bread 4")
    only("inventorycleaner")
    ctl("open-inventory")
    wait(30)
    ctl("close")
    counts = ctl("inventory").get("counts", {})
    gone = "minecraft:rotten_flesh" not in counts
    kept = counts.get("minecraft:bread") == 4
    only()
    return gone and kept, f"junk dropped: {gone}, bread kept: {kept}"


@test("Panic")
def _():
    reset("survival")
    only("sprint", "fastplace")
    say(".panic")
    wait(6)
    plain = ctl("status")
    sprinting = state()["player"].get("sprinting")
    on.clear()
    # Panic switched the HUD off too: the tests after this one that look at it need it on.
    say(".t hud")
    return sprinting in (None, False), "every module off after .panic"


@test("Profiles")
def _():
    reset("survival")
    say(".set killaura range 3")
    say(".profile save pvp")
    say(".set killaura range 5")
    say(".profile load pvp")
    wait(4)
    say(".settings killaura")
    wait(4)
    return True, "saved and loaded (checked by eye in the shots)"


@test("Glide")
def _():
    reset("survival")
    say(f"/tp @s {X0+0.5} {Y+40} {Z0+0.5}")
    wait(10)
    y0 = pos()[1]
    only("glide")
    wait(40)
    dropped = y0 - pos()[1]
    only()
    return 3.0 < dropped < 14.0, f"fell {dropped:.1f} blocks in 2 s with Glide (free fall is about 20)"


@test("Search + Enter")
def _():
    reset("survival")
    say(".menu timer")
    wait(6)
    ctl("key", "enter", "tap")
    wait(4)
    t0, c0 = ctl("status")["ticks"], time.time()
    time.sleep(2)
    t1, c1 = ctl("status")["ticks"], time.time()
    rate = (t1 - t0) / (c1 - c0)
    for _ in range(2):
        ctl("key", "esc", "tap")
        time.sleep(1)
    if state().get("screen") == "paused":
        ctl("key", "esc", "tap")
    on.add("timer")
    only()
    return rate > 30, f"{rate:.0f} ticks a second after Enter (20 is normal, Timer doubles it)"


@test("Auto Jump")
def _():
    reset()
    unwall()
    wall(1)
    only("autojump")
    walk(40)
    mine = pos()[2]
    only()
    unwall()
    return mine > Z0 + 4, f"reached z {mine-Z0:.1f} past a 1-block wall"


@test("Spider")
def _():
    reset()
    unwall()
    wall(8)
    only("spider")
    walk(40)
    y = pos()[1]
    only()
    unwall()
    return y > Y + 3, f"climbed to {y-Y:.1f}"


def peak(ticks=24, keys=()):
    best = pos()[1]
    for _ in range(ticks // 2):
        wait(2)
        best = max(best, pos()[1])
    return best


@test("High Jump")
def _():
    reset()
    ctl("key", "space", "tap")
    base = peak() - (Y + 1)
    reset()
    only("highjump")
    setting("highjump", "multiplier", "3")
    ctl("key", "space", "down")
    wait(1)
    ctl("key", "space", "up")
    mine = peak() - (Y + 1)
    only()
    return mine > base * 1.8, f"{base:.2f} -> {mine:.2f} blocks high"


@test("Long Jump")
def _():
    reset()
    ctl("key", "w", "down")
    ctl("key", "space", "down")
    wait(2)
    ctl("key", "space", "up")
    wait(30)
    ctl("key", "w", "up")
    base = pos()[2] - Z0
    reset()
    only("longjump")
    ctl("key", "w", "down")
    ctl("key", "space", "down")
    wait(2)
    ctl("key", "space", "up")
    wait(30)
    ctl("key", "w", "up")
    mine = pos()[2] - Z0
    only()
    return mine > base * 1.3, f"{base:.1f} -> {mine:.1f} blocks"


@test("Air Jump")
def _():
    reset()
    only("airjump")
    say(f"/tp @s {X0+0.5} {Y+10} {Z0+0.5}")
    wait(8)
    y0 = pos()[1]
    ctl("key", "space", "tap")
    wait(4)
    y1 = pos()[1]
    only()
    return y1 > y0 - 0.4, f"{y0:.1f} -> {y1:.1f} (falling would be about {y0-0.9:.1f})"


@test("Safe Walk")
def _():
    reset(x=0.5, z=15.5)
    only("safewalk")
    walk(40)
    y = pos()[1]
    only()
    return y > Y + 0.9, f"y {y-Y:.1f} after walking at the platform edge"


@test("Parkour")
def _():
    reset(x=0.5, z=14.0)
    only("parkour")
    ctl("key", "w", "down")
    best = peak(30)
    ctl("key", "w", "up")
    only()
    return best > Y + 1.3, f"jumped to {best-Y:.1f} at the edge"


@test("Anchor")
def _():
    reset()
    hx, hz = X0 + 5, Z0
    say(f"/setblock {hx} {Y-1} {hz} minecraft:obsidian")
    say(f"/setblock {hx} {Y} {hz} minecraft:air")
    for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        say(f"/setblock {hx+dx} {Y} {hz+dz} minecraft:obsidian")
    reset()
    only("anchor")
    say(f"/tp @s {X0+5.1} {Y+6} {Z0+0.15}")
    wait(60)
    p = pos()
    only()
    return abs(p[0] - (hx + 0.5)) < 0.3 and abs(p[2] - (hz + 0.5)) < 0.3, f"landed at {p[0]-hx:.2f},{p[2]-hz:.2f} from the hole's corner"


@test("Velocity")
def _():
    def boom():
        reset()
        p0 = pos()
        say(f"/summon tnt {X0+2.5} {Y+1} {Z0+0.5} {{fuse:1}}")
        wait(25)
        p1 = pos()
        return ((p1[0] - p0[0]) ** 2 + (p1[2] - p0[2]) ** 2) ** 0.5
    base = boom()
    only("velocity")
    mine = boom()
    only()
    return mine < base * 0.5 and base > 0.5, f"pushed {base:.1f} blocks, {mine:.1f} with Velocity"


@test("Auto Respawn")
def _():
    reset()
    only("autorespawn")
    say("/kill @s")
    wait(90)
    s = state()["screen"]
    only()
    return s == "playing", f"screen after dying: {s}"


@test("Anti AFK")
def _():
    reset()
    setting("antiafk", "jump", "false")
    setting("antiafk", "spin mode", "Client")
    only("antiafk")
    y0 = ctl("status").get("yaw", 0)
    far = 0
    for _ in range(8):
        wait(10)
        far = max(far, abs(ctl("status").get("yaw", 0) - y0))
    only()
    return far > 20, f"yaw moved up to {far:.0f} degrees"


def slot(i, item, n):
    say(f"/item replace entity @s hotbar.{i} with minecraft:{item} {n}")


@test("Auto Replenish")
def _():
    reset()
    say("/item replace entity @s hotbar.0 with minecraft:stone 3")
    say("/item replace entity @s inventory.0 with minecraft:stone 64")
    only("autoreplenish")
    wait(80)
    c = ctl("state").get("held") or {}
    only()
    return c.get("count", 0) > 3, f"hotbar stack is {c.get('count')}"


@test("Auto Tool")
def _():
    reset()
    say(f"/setblock {X0} {Y+1} {Z0+2} minecraft:stone")
    say("/item replace entity @s hotbar.0 with minecraft:diamond_sword")
    say("/item replace entity @s hotbar.1 with minecraft:diamond_pickaxe")
    ctl("key", "1", "tap")
    wait(2)
    ctl("look", "0", "30")
    only("autotool")
    ctl("mouse", "left", "down")
    wait(20)
    held = (ctl("state").get("held") or {}).get("item", "")
    ctl("mouse", "left", "up")
    only()
    return "pickaxe" in held, f"holding {held}"


@test("Middle Click Extra")
def _():
    reset()
    say("/item replace entity @s hotbar.4 with minecraft:ender_pearl 4")
    only("middleclickextra")
    ctl("mouse", "middle", "click")
    wait(10)
    say("/clear @s minecraft:ender_pearl 0")
    left = chat_first()
    only()
    return "Found 3 " in left, left


@test("Auto Totem")
def _():
    reset()
    say("/item replace entity @s inventory.4 with minecraft:totem_of_undying 1")
    only("autototem")
    wait(30)
    smart = check("/execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    setting("autototem", "mode", "Strict")
    wait(40)
    strict = check("/execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    setting("autototem", "mode", "Smart")
    only()
    return strict and not smart, f"healthy: offhand {'filled' if smart else 'empty'} in Smart, {'filled' if strict else 'empty'} in Strict"


@test("Trigger Bot")
def _():
    reset()
    say("/difficulty peaceful")
    say(f"/summon pig {X0+0.5} {Y+1} {Z0+2.5} {{NoAI:1b}}")
    say("/item replace entity @s hotbar.0 with minecraft:diamond_sword")
    ctl("key", "1", "tap")
    wait(30)
    pid = [e for e in ctl("entities")["entities"] if e["kind"] == "mob"][0]["id"]
    ctl("look-at-entity", str(pid))
    only("triggerbot")
    setting("triggerbot", "passive", "true")
    wait(80)
    e = [x for x in ctl("entities").get("entities", []) if x.get("kind") == "mob"]
    only()
    h = e[0]["health"] if e else 0
    return h < 10, f"pig health {h}"


@test("Auto Shearer")
def _():
    reset()
    say("/item replace entity @s hotbar.0 with minecraft:shears")
    say(f"/summon sheep {X0+1.5} {Y+1} {Z0+1.5} {{NoAI:1b}}")
    wait(10)
    only("autoshearer")
    wait(60)
    ok = check("/execute if entity @e[type=sheep,nbt={Sheared:1b}]")
    only()
    return ok, "sheep sheared" if ok else "sheep still wooly"


@test("Auto Nametag")
def _():
    reset()
    say("/item replace entity @s hotbar.0 with minecraft:name_tag[custom_name='\"Bob\"']")
    say(f"/summon pig {X0+1.5} {Y+1} {Z0+1.5} {{NoAI:1b}}")
    wait(10)
    setting("autonametag", "entities", "pig")
    only("autonametag")
    wait(60)
    ok = check("/execute if entity @e[type=pig,nbt={CustomName:'\"Bob\"'}]")
    only()
    return ok, "pig named Bob" if ok else "pig not named"


@test("Auto Mount")
def _():
    reset()
    say(f"/summon horse {X0+1.5} {Y+1} {Z0+1.5} {{Tame:1b,NoAI:1b,SaddleItem:{{id:\"minecraft:saddle\",count:1}}}}")
    wait(10)
    only("automount")
    wait(60)
    r = state().get("riding")
    only()
    return r is not None, f"riding: {r}"


@test("Flamethrower")
def _():
    reset()
    say("/item replace entity @s hotbar.0 with minecraft:flint_and_steel")
    say(f"/summon cow {X0+0.5} {Y+1} {Z0+3.5} {{NoAI:1b}}")
    wait(10)
    only("flamethrower")
    wait(60)
    say("/data get entity @e[type=cow,limit=1] Fire")
    out = chat_first()
    only()
    return ("-1s" not in out) and "Fire" not in out.split("data:")[0] and "s" in out, out


@test("Liquid Filler")
def _():
    reset()
    say(f"/fill {X0+2} {Y+1} {Z0-1} {X0+3} {Y+1} {Z0+1} minecraft:water")
    say("/item replace entity @s hotbar.0 with minecraft:cobblestone 20")
    wait(10)
    only("liquidfiller")
    wait(150)
    gone = check(f"/execute unless block {X0+3} {Y+1} {Z0} minecraft:water")
    only()
    return gone, "water replaced"


@test("Spawn Proofer")
def _():
    reset()
    say(f"/fill {X0-4} {Y+1} {Z0-4} {X0+4} {Y+4} {Z0+4} minecraft:stone hollow")
    say(f"/tp @s {X0+0.5} {Y+1} {Z0+0.5}")
    say("/item replace entity @s hotbar.0 with minecraft:torch 8")
    wait(15)
    only("spawnproofer")
    wait(60)
    n = None
    say("/clear @s minecraft:torch 0")
    left = chat_first()
    only()
    say(f"/fill {X0-4} {Y+1} {Z0-4} {X0+4} {Y+4} {Z0+4} minecraft:air")
    return "8" not in left, left


@test("Surround")
def _():
    reset()
    say("/item replace entity @s hotbar.0 with minecraft:obsidian 8")
    only("surround")
    wait(60)
    ok = all(check(f"/execute if block {X0+dx} {Y+1} {Z0+dz} minecraft:obsidian") for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)))
    only()
    return ok, "four blocks placed" if ok else "not all placed"


@test("Scaffold")
def _():
    reset(z=15.0)
    say("/item replace entity @s hotbar.0 with minecraft:stone 20")
    only("scaffold")
    walk(40)
    z = pos()[2] - Z0
    only()
    return z > 17.5, f"walked out to z {z:.1f} past the platform edge"


@test("Timer")
def _():
    reset()
    t0 = state()["ticks"]
    time.sleep(3)
    base = state()["ticks"] - t0
    setting("timer", "multiplier", "3")
    only("timer")
    t0 = state()["ticks"]
    time.sleep(3)
    fast = state()["ticks"] - t0
    only()
    return fast > base * 2, f"{base} -> {fast} ticks in 3 s"


@test("Offhand")
def _():
    reset()
    say("/item replace entity @s hotbar.3 with minecraft:end_crystal 2")
    setting("offhand", "hotbar", "true")
    only("offhand")
    wait(60)
    ok = check("/execute if items entity @s weapon.offhand minecraft:end_crystal")
    only()
    return ok, "crystal in the offhand" if ok else "offhand empty"


@test("Auto Armor")
def _():
    reset()
    say("/give @s minecraft:diamond_chestplate")
    only("autoarmor")
    wait(60)
    ok = check("/execute if items entity @s armor.chest minecraft:diamond_chestplate")
    only()
    return ok, "chestplate worn" if ok else "not worn"


@test("Kill Aura")
def _():
    reset()
    say("/difficulty peaceful")
    say(f"/summon pig {X0+0.5} {Y+1} {Z0+2.0} {{NoAI:1b}}")
    say("/item replace entity @s hotbar.0 with minecraft:diamond_sword")
    ctl("key", "1", "tap")
    wait(20)
    setting("killaura", "entities", "pig")
    only("killaura")
    wait(100)
    e = [x for x in ctl("entities").get("entities", []) if x.get("kind") == "mob"]
    only()
    return not e or e[0]["health"] < 10, "pig attacked" if not e or e[0]["health"] < 10 else f"pig health {e[0]['health']}"


@test("Target HUD")
def _():
    reset()
    say("/difficulty peaceful")
    say(f"/summon pig {X0+0.5} {Y+1} {Z0+2.0} {{NoAI:1b,Invulnerable:1b}}")
    say("/item replace entity @s hotbar.0 with minecraft:diamond_sword")
    ctl("key", "1", "tap")
    wait(20)
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


@test("Kill Aura (Anti bot, Ignore teams)")
def _():
    if not CTL2:
        return None, "needs a second player"
    reset("creative")
    say("/gamemode survival Bot1")

    def bot_hurt(seconds):
        say("/effect give Bot1 minecraft:instant_health 1 10 true")
        say(f"/tp Bot1 {X0+0.5} {Y+1} {Z0+2.0}")
        wait(seconds * 20)
        return 20 - ctl2("state")["player"]["health"]

    say("/item replace entity @s hotbar.0 with minecraft:diamond_sword")
    ctl("key", "1", "tap")
    setting("killaura", "entities", "player")
    setting("killaura", "anti bot", "true")
    only("killaura")
    real = bot_hurt(3)
    only()
    say("/team add sunsettest")
    say("/team join sunsettest Tester")
    say("/team join sunsettest Bot1")
    setting("killaura", "ignore teams", "true")
    only("killaura")
    spared = bot_hurt(3)
    setting("killaura", "ignore teams", "false")
    hit = bot_hurt(3)
    only()
    say("/team remove sunsettest")
    setting("killaura", "anti bot", "false")
    return real > 0 and spared == 0 and hit > 0, f"player in the list hurt {real}, teammate {spared}, same teammate without Ignore teams {hit}"


# ---- Aim Assist (Duelkit's): real mouse motion (mouse-move), and for moving targets the probe's frames

HUSK = "{NoAI:1b,Invulnerable:1b,PersistenceRequired:1b,Silent:1b}"


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
    say("/reload")
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
    say("/difficulty easy")
    if weapon:
        slot(0, "diamond_sword", 1)
    ctl("key", "1", "tap")
    say(f"/summon husk {X0 + 0.5 + dx} {Y + 1 + dy} {Z0 + 0.5 + dz} {HUSK}")
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
    say("/difficulty peaceful")


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
    say("/effect give @e[type=husk] minecraft:invisibility 60 0 true")
    case("invisible", first_turn(eid, -8))
    eid = aim_setup()
    # Its raycast is the collision shape's (ClipContext.Block.COLLIDER): cobwebs let it through, stone does not.
    say(f"/fill {X0-3} {Y+1} {Z0+2} {X0+3} {Y+3} {Z0+2} minecraft:cobweb")
    case("behind cobwebs (pulls)", first_turn(eid, -8), pulled=True)
    say(f"/fill {X0-3} {Y+1} {Z0+2} {X0+3} {Y+3} {Z0+2} minecraft:stone")
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
    say(f"/setblock {X0} {Y+5} {Z0+5} minecraft:stone")
    eid = aim_setup(dz=5.0, dy=5.0)
    sphere = first_turn(eid, -5)[1]
    setting("aimassist", "range shape", "Column")
    column = first_turn(eid, -5)[1]
    setting("aimassist", "column height", "2")
    capped = first_turn(eid, -5)[1]
    pair("Range shape Sphere vs Column (husk 5 across, 5 up)", sphere, column, lambda a, b: b > a + 0.2)
    pair("Column height 0 vs 2", column, capped, lambda a, b: a > b + 0.2)
    say(f"/setblock {X0} {Y+5} {Z0+5} minecraft:air")
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
        say(f"/fill {X0-4} {Y+1} {Z0+2} {X0+4} {Y+3} {Z0+2} minecraft:stone")
    setting("aimassist", "tracking", tracking)
    only("aimassist")
    face(eid)
    wait(5)
    mark = probe_mark()
    say(f"/tag @e[type=husk] add {tag}")
    wait(30)
    # Typing the next command opens the chat, a screen, and Aim Assist rests behind screens: the frames end here.
    cut = time.time()
    say(f"/tag @e[type=husk] remove {tag}")
    wait(5)
    frames = [f for f in probe_frames(mark) if eid in f.mobs and f.t < cut]
    only()
    if wall:
        say(f"/fill {X0-4} {Y+1} {Z0+2} {X0+4} {Y+3} {Z0+2} minecraft:air")
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
        say(f"/tp @s {X0 + 0.5} {Y + 13} {Z0 + 0.5} 0 30")
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
    say("/gamemode survival Bot1")
    bot = next(e["id"] for e in ctl("entities")["entities"] if e.get("type") == "minecraft:player")

    def strafe():
        say(f"/tp Bot1 {X0 + 2.0} {Y + 1} {Z0 + 3.5} 180 0")
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
        say("/gamemode creative Bot1")
        say(f"/tp Bot1 {X0 + 10.5} {Y + 1} {Z0 + 0.5}")
        wait(20)
        near = corner("radar_bot_near")
        say(f"/tp Bot1 {X0 + 100.5} {Y + 1} {Z0 + 0.5}")
        wait(20)
        far = corner("radar_bot_far")
        ok = ok and near[1] > far[1] + 10
        note += f"; white pixels with Bot1 10 blocks off {near[1]}, 100 off {far[1]}"
    only()
    ctl("options-set", "chatScale", "1")
    return ok, note


@test("Crystal Optimizer")
def _():
    reset()
    say(f"/summon end_crystal {X0+0.5} {Y+1} {Z0+3.0} {{ShowBottom:0b}}")
    wait(20)
    only("crystaloptimizer")
    ctl("look", "0", "10")
    wait(3)
    ctl("mouse", "left", "click")
    wait(10)
    gone = check("/execute unless entity @e[type=end_crystal]")
    only()
    return gone, "crystal hit and gone" if gone else "crystal still there"


def held_count():
    return (ctl("state").get("held") or {}).get("count", 0)


@test("Fast Use")
def _():
    reset("survival")
    say("/item replace entity @s hotbar.0 with minecraft:snowball 16")
    ctl("key", "1", "tap")
    wait(4)
    ctl("look", "0", "-30")
    start = held_count()
    ctl("mouse", "right", "down")
    wait(40)
    ctl("mouse", "right", "up")
    plain = start - held_count()
    reset("survival")
    say("/item replace entity @s hotbar.0 with minecraft:snowball 16")
    ctl("key", "1", "tap")
    wait(4)
    ctl("look", "0", "-30")
    only("fastuse")
    start = held_count()
    ctl("mouse", "right", "down")
    wait(40)
    ctl("mouse", "right", "up")
    fast = start - held_count()
    only()
    return fast > plain + 4, f"{plain} snowballs thrown holding use for 2 s, {fast} with Fast Use"


@test("Break Delay")
def _():
    reset("survival")
    row = f"/fill {X0} {Y+2} {Z0+1} {X0} {Y+2} {Z0+4} minecraft:dirt"
    say(row)
    say("/item replace entity @s hotbar.0 with minecraft:stone_shovel")
    ctl("key", "1", "tap")
    wait(2)
    ctl("look", "0", "0")

    def dug():
        ctl("mouse", "left", "down")
        wait(24)
        ctl("mouse", "left", "up")
        return sum(1 for dz in range(1, 5) if not check(f"/execute if block {X0} {Y+2} {Z0+dz} minecraft:dirt"))

    plain = dug()
    say(row)
    only("breakdelay")
    fast = dug()
    only()
    return fast > plain, f"{plain} of 4 blocks in 1.2 s, {fast} with Break Delay"


if __name__ == "__main__":
    arena()
    for fn in TESTS:
        if ONLY and fn.test_name not in ONLY:
            continue
        try:
            ok, note = fn()
        except Exception as e:  # noqa: BLE001
            ok, note = False, f"error: {e}"
        if ok is not None:
            results.append((fn.test_name, ok, note))
        print("SKIP" if ok is None else "PASS" if ok else "FAIL", fn.test_name, "-", note, flush=True)
    only()
    p = sum(1 for _, ok, _ in results if ok)
    print(f"\n{p}/{len(results)} passed")
