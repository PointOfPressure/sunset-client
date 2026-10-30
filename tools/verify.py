#!/usr/bin/env python3
# Runs each Solstice module in a flat test arena in a headless Horizon (see
# tools/README in the repo's notes: ctl = horizon-ctl on the test session) and
# checks a measurable result. Usage: verify.py CTL_PATH [test names...]
import json, subprocess, sys, time

CTL = sys.argv[1]
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
    say("/gamemode creative")
    say(f"/fill {X0-16} {Y} {Z0-16} {X0+16} {Y} {Z0+16} minecraft:stone")
    say(f"/fill {X0-16} {Y+1} {Z0-16} {X0+16} {Y+12} {Z0+16} minecraft:air")


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
    setting("antiafk", "seconds", "3")
    only("antiafk")
    y0 = state()["player"].get("yaw", 0)
    far = 0
    for _ in range(14):
        wait(10)
        far = max(far, abs(state()["player"].get("yaw", 0) - y0))
    only()
    return far > 1, f"yaw moved up to {far:.0f} degrees"


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
    say("/item replace entity @s hotbar.2 with minecraft:totem_of_undying 1")
    only("autototem")
    wait(40)
    ok = check("/execute if items entity @s weapon.offhand minecraft:totem_of_undying")
    only()
    return ok, "totem in the offhand" if ok else "offhand empty"


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


@test("Nuker")
def _():
    reset("creative")
    say(f"/fill {X0+1} {Y+1} {Z0+1} {X0+2} {Y+2} {Z0+2} minecraft:stone")
    setting("nuker", "keepground", "false")
    only("nuker")
    wait(40)
    ok = check(f"/execute unless block {X0+2} {Y+2} {Z0+2} minecraft:stone")
    only()
    return ok, "blocks broken"


if __name__ == "__main__":
    arena()
    for fn in TESTS:
        if ONLY and fn.test_name not in ONLY:
            continue
        try:
            ok, note = fn()
        except Exception as e:  # noqa: BLE001
            ok, note = False, f"error: {e}"
        results.append((fn.test_name, ok, note))
        print(("PASS" if ok else "FAIL"), fn.test_name, "-", note, flush=True)
    only()
    p = sum(1 for _, ok, _ in results if ok)
    print(f"\n{p}/{len(results)} passed")
