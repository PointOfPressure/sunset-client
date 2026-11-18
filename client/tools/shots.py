#!/usr/bin/env python3
# Takes the README screenshots in a headless Horizon (same session as verify.py). Usage: shots.py CTL_PATH OUT_DIR
import sys
sys.argv = [sys.argv[0], sys.argv[1]] + sys.argv[3:] if False else sys.argv
CTL = sys.argv[1]
OUT = sys.argv[2]
sys.argv = [sys.argv[0], CTL]
exec(open("/home/sayer/hzmeteor/verify.py").read().split("if __name__")[0])


def shot(name):
    ctl("screenshot", f"{OUT}/{name}.png")


def scene():
    reset("creative")
    say("/gamemode creative")
    say("/time set 6000")
    say("/weather clear")


def settle():
    # Chat lines fade after ten seconds; the screenshots shouldn't have the command log on them.
    wait(260)


def shot_after(name):
    settle()
    shot(name)
    time.sleep(2)


ctl("options-set", "tutorialStep", "none")
arena()
only()
# 1. the click GUI, with Kill Aura's settings open, then a search
scene()
settle()
say(".menu")
wait(8)
ctl("click-at", "76", "50")
wait(4)
shot("gui")
time.sleep(2)
say(".menu esp")
wait(8)
shot("gui-search")
time.sleep(2)
for _ in range(2):
    ctl("key", "esc", "tap")
    time.sleep(1)
if state().get("screen") == "paused":
    ctl("key", "esc", "tap")
    time.sleep(1)
wait(4)
# 2. entities and storage through the walls
scene()
for n, (dx, dz, kind) in enumerate([(5, 6, "pig"), (-6, 8, "cow"), (9, 3, "sheep"), (-3, 11, "zombie"), (3, 14, "villager"), (-9, 5, "chicken")]):
    say(f"/summon {kind} {X0+dx+0.5} {Y+1} {Z0+dz+0.5} {{NoAI:1b,PersistenceRequired:1b}}")
say(f"/setblock {X0+7} {Y+1} {Z0+9} minecraft:chest")
say(f"/setblock {X0-4} {Y+1} {Z0+7} minecraft:barrel")
say(f"/setblock {X0+1} {Y+1} {Z0+12} minecraft:ender_chest")
say(f"/fill {X0-3} {Y} {Z0+13} {X0+3} {Y} {Z0+15} minecraft:diamond_ore")
only("esp", "tracers", "nametags", "storageesp", "blockesp")
say(".set blockesp blocks Diamond")
ctl("look", "0", "12")
shot_after("esp")
only()
# 3. xray: ores in the rock under the arena
scene()
ORES = ["diamond_ore", "emerald_ore", "gold_ore", "redstone_ore", "lapis_ore", "ancient_debris"]
import random
random.seed(7)
for i in range(60):
    ore = ORES[i % len(ORES)]
    x, z, y = X0 + random.randint(-8, 8), Z0 + random.randint(2, 18), Y - random.randint(2, 14)
    say(f"/fill {x} {y} {z} {x+random.randint(0,1)} {y+random.randint(0,1)} {z+random.randint(0,1)} minecraft:{ore}")
say(f"/tp @s {X0+0.5} {Y+1} {Z0+0.5}")
only("xray")
say(".set xray blocks " + ",".join(ORES))
ctl("look", "0", "35")
wait(80)
shot_after("xray")
only()
# 4. freecam: the camera leaves the player standing among the mobs
scene()
say(f"/tp @s {X0+0.5} {Y+1} {Z0+0.5}")
for dx, dz, kind in [(3, 3, "pig"), (-3, 4, "cow")]:
    say(f"/summon {kind} {X0+dx+0.5} {Y+1} {Z0+dz+0.5} {{NoAI:1b,PersistenceRequired:1b}}")
wait(20)
only("freecam")
ctl("key", "w", "down"); wait(26); ctl("key", "w", "up")
ctl("key", "space", "down"); wait(14); ctl("key", "space", "up")
ctl("mouse-move", "1200", "250")
wait(10)
shot_after("freecam")
only()
wait(10)
print("done")
