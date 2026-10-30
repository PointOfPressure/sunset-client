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
    say("/difficulty easy")
    say("/time set 6000")
    say("/weather clear")


arena()
only()
# 1. the click GUI, with Kill Aura's settings open
scene()
say(".menu")
wait(8)
ctl("click-at", "76", "50")
wait(4)
shot("gui")
for k in "esp":
    ctl("key", k, "tap")
wait(4)
shot("gui-search")
ctl("key", "esc", "tap"); wait(2); ctl("key", "esc", "tap"); wait(4)
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
ctl("look", "0", "5")
wait(40)
shot("esp")
only()
# 3. xray
scene()
say(f"/tp @s {X0+0.5} {Y+40} {Z0+0.5}")
wait(10)
ctl("look", "0", "50")
only("xray")
wait(120)
shot("xray")
only()
wait(60)
# 4. freecam
scene()
say(f"/tp @s {X0+0.5} {Y+1} {Z0+0.5}")
wait(20)
only("freecam")
ctl("key", "s", "down"); wait(30); ctl("key", "s", "up")
ctl("key", "space", "down"); wait(20); ctl("key", "space", "up")
wait(10)
shot("freecam")
only()
wait(10)
# 5. Surround and the HUD
scene()
say("/gamemode survival")
say("/item replace entity @s hotbar.0 with minecraft:obsidian 8")
say(f"/tp @s {X0+0.5} {Y+1} {Z0+0.5}")
wait(20)
only("surround", "trajectories")
wait(40)
ctl("look", "30", "15")
wait(10)
shot("surround")
only()
print("done")
