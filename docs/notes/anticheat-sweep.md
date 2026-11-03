# Anticheat sweep (3 Nov 2026, threadripper)

Sunset at origin/main cb6f6b3 (Horizon pin ba6344a, binary target-new/fast 05:19 incl. Snap Tap) against five anticheats, each on its own server copy. Report only: no Sunset code was changed. Rig scripts: ~/hzmeteor/ac/ (acsweep.py scenarios, acreport.py log parser, gentable.py, runall-ac.sh/servers-ac.sh copies with the AC ports, rcon.py, NOTES.txt). Raw data: ~/hzmeteor/acs1 (Horizon AC, Intave, Spartan), acs2 (NCP), acs3 (Grim), acs4 (fights again on the four Paper ACs); each has flags.json/flags.txt.

## What loaded on Paper 26.3 (same Paper jar for all)

| Anticheat | Version | Result |
|---|---|---|
| GrimAC | 2.3.75-1299a2a (Modrinth, lists 26.3) | loads natively, no ViaVersion needed. **Swept**, port 25610 |
| Intave | 14.9.7-b1 (Modrinth dev-cc7d30b2, free, cloud off) | loads; downloads ProtocolLib 5.5.0-SNAPSHOT itself. **Swept**, 25612 |
| Spartan | 4.0.9.0 limited free (≤5 players, 2 h of detection per start) | loads. **Swept**, 25613 |
| NoCheatPlus-Updated | 3.17.1-SNAPSHOT b281 (CodeMC CI, 28 Oct) | loads (+ProtocolLib). **Swept**, 25614 |
| Horizon anticheat | upstream main f4c87b4ee (has 876d0003d; no anticheat commits since, upstream at a4a3afb85 when checked) | dedicated server from ~/dev/horizon-src (target-upstream). **Swept**, 25611 |
| VoidAC | 2.9.0 (Grim fork, GPL) | loads; not swept (Grim fork) |
| Counter | 1.3.1 (+packetevents 2.14.0) | loads; not swept |
| OmniAnticheat | 1.0.1 (lists up to 26.2) | loads; not swept |
| BS-AntiCheat | 1.1.2 | loads; not swept |
| Paragon | 1.2-26.3 | **fails**: NPE in its shaded packetevents (NMS_ITEM_STACK_CLASS null) |
| LightAntiCheat-Plus | 2.0.2 (successor of LightAC, lists 26.2) | disables itself without packetevents (not retried with it) |
| Jester | 0.0.6-beta | needs packetevents (not retried) |
| Vulcan, Polar | paid only (no free/legacy build for 26.3) | not tested |

Protocol: our client (ba6344a) and upstream f4c87b4ee both use protocol 777; Tester and Bot1 joined the upstream server with no problem.

Changes from stock AC configs, all for the harness, none to the checks: Grim verbose print-to-console on (every flag logged; Grim's own alerts start at VL 100 for Simulation); Intave `kickip` replaced by `kick` (Tester and Bot1 share 127.0.0.1) and its ban command turned into a kick saying "Intave would ban"; NCP chat checks off (it tempkicked the harness for command rate) and LuckPerms negating `nocheatplus`, `nocheatplus.checks`, `nocheatplus.shortcut.bypass`, `nocheatplus.bypass` for the default group (NCP exempts ops otherwise; Grim/Intave/Spartan don't). Horizon AC: server.toml with every check on, `ban_after = 0`, timer alert 5, prediction alert 15 (upstream defaults otherwise; timer and prediction never set back or kick by default).

## Method

`ac/acsweep.py` drives Tester (Sunset) and Bot1 (the same client with no module on) through runall's harness. Each scenario writes its start/end time; `acreport.py` puts every AC log line (Grim console, Intave logs/intave*.log, Spartan logs/*.yml, NCP nocheatplus.log, Horizon logs/anticheat.log, kick reasons from the consoles) into the scenario window. A window counts only if Sunset printed "<module> on" in it (else "invalid").
- **Baseline**: no module, the same play, hits on Bot1 by hand, being hit by Bot1, sprint hits.
- **Sweep**: verify.py's sweeps, every mode of each module that talks to the server with otherwise default settings, over the same 6 s of play (walk to a wall, jump, strafe, sneak, dig, use, hold jump, walk back) beside a husk and a cow.
- **Fight**: combat modules against Bot1 (1000 health, knockback resistance 1, strafing): Kill Aura in every rotation/rotate/attack mode, Kill Aura+Criticals, Criticals by hand, Trigger Bot, Aim Assist, Auto Clicker, Hitboxes (hits 0.55 off), Reach (from 5.2 blocks feet-to-feet), W Tap, Jump Reset/Velocity (hit by Bot1), Auto Weapon, Fake Lag, Blink.
- **Defaults**: every other module, defaults, same 6 s of play. Many of these only act with a trigger (crystals, a target player, a fall, low health...) and sat idle, so "pass" there means no flag from having it on, not that its action passes. The **Test** rows (verify.py's measured tests, non-default settings) are where those act.

Cells: Grim/Spartan/NCP/Horizon `check xN` = flags in the window; Intave `check xN +W` = flags and the VL they added (physics/timer under +1 are left out: its physics check adds 0.02-0.06 per teleport/stage setup; attackraytrace at +0 is kept, those hits were still cancelled); Spartan gravity-simulation up to x6 is left out (the baseline has x2-5). **KICK** = Tester kicked in that window. Flags are Tester's only; Bot1's are under Baseline findings.

Caveats: VLs carry over between windows, so a lone x1 just after a heavy-flag window may be left over from it. NCP logs flags at a throttled rate (its log: actions), so its counts are low; after the Self Trap kick its next 15 sweeps never got Sunset on (invalid). The first Grim pass (acs1) lost Sunset to a `.panic` typed on the death screen after Tester suffocated at 06:30; acs3 is a full rerun with a resync. In the long runs the hand-click fights on the Paper servers stopped landing after ~15 fights (cause not found; isolated they work), so all Paper fights were rerun on their own with a full clean between them (acs4); Horizon's fights in acs1 landed throughout.

## Grim status (defaults and main modes that flag GrimAC 2.3.75)

Nothing was removed or disabled. Grim's default punishments never kick; Simulation sets back at once (setback threshold 1): the client counted 15-24 server put-backs per Flight sweep. Grim cancels hits that fail Reach (Bot1 took 0 damage from Reach hits at 4.5).
1. **Flight**, all 6 mode x anti-kick combinations: Simulation x15-38 per 6 s, set back.
2. **Speed** Vanilla and Strafe: Simulation x28 (Test: x71 + Timer x14 with the Timer setting).
3. **No Fall** Packet: GroundSpoof x11; No ground: GroundSpoof x2. Air place and Place pass.
4. **Kill Aura**, every Rotation mode (Snap/Normal/Smooth/Custom), Rotate Always/On hit, Attack Single/Switch/Multi: Reach x3-6 per 5 s (3.4-4.8 blocks; default Range 4.5) and AimDuplicateLook x1-3; Rotate None: Hitboxes x7. Test: AimDuplicateLook x26.
5. **Criticals** Packet: GroundSpoof x3-13 + Simulation x3-8; Mini jump: GroundSpoof x10-16 + Simulation x10-16. **Jump passes** (by hand); with Kill Aura it shows Kill Aura's own Hitboxes/Reach.
6. **Reach** (Entity reach): Reach x8; Grim cancels the hits.
7. **Velocity** Scale (defaults 0/0): AntiKB x5; Knockback delay: AntiKB x5; Blend: AntiKB x1 + Simulation x3.
8. **Surround** Center Never: Post x20 + Simulation x9; Center Always: Simulation x53 (centering move) + Post. On activate/Incomplete: Post x2-3.
9. **Self Trap** Top / Anti face place / Full: AirLiquidPlace, PositionPlace and Post x70+ each, Simulation x12, AimDuplicateLook x4, BadPacketsJ x3. Top mode None passes.
10. **Scaffold** Normal (default mode): Post x14, PositionPlace x9, AimDuplicateLook x8 (Test: Post x55, PositionPlace x38, Simulation x37). **Legit and Godbridge pass** (all three Test rows clean).
11. **Nuker** default modes: Post/PositionBreak x1-3 per sweep; Packet mine on: FastBreak x75. Test (survival): Post x12, PositionBreakA x9, FastBreak x8.
12. **Packet Mine** (Test): PositionBreakB x20, Post x13, FastBreak x6, BadPacketsA x6. **Break Delay** (Test): PositionBreakB x21, FastBreak x4 (its default sweep passes).
13. **Timer**: Timer x92 (Test: Timer x196, TimerLimit x9).
14. **Spider**: Simulation x19, GroundSpoof x3. **Glide**: Simulation x2 (Test x42). **High Jump**: Simulation x2 (Test x2).
15. **Anti AFK** Spin mode Server: Simulation x18, BadPacketsJ x3, AimModulo360 x2. Client passes.
16. **Anti Hunger** (Sprint): GroundSpoof x2.
17. **Auto Tool**: BadPacketsA x1 per tool switch (the same slot sent twice, see Other findings). Same check in Crystal Aura (x31), Anchor Aura (x46), Packet Mine (x6), Auto Weapon (x2) Tests.
18. **Offhand** Silent open on: AimDuplicateLook x8. Item modes pass.
19. **Crystal Aura** (Test): Post x47, BadPacketsA x31. **Anchor Aura** (Test): AimDuplicateLook x75, Post x57, BadPacketsA x46. **Auto City / Auto Trap** (Test): Post x182, AirLiquidPlace x172, PositionPlace x172.
20. **Auto Weapon** (Axe on shields Test): AimDuplicateLook x360.
21. **No Slow** (Test): NoSlow x40, Simulation x11.
22. **Fake Player**: Simulation x24 (you collide with the client-only player, Grim doesn't know it).
23. Test-only (non-default settings, may include harness teleports): Step x8, GUI Move x12, Snap Tap x10, Parkour x147, Anchor x31, Wind Jump x6 + GroundSpoof x3, Arrow Dodge x8, Entity Control (boat) x3, W Tap x71: all Simulation.

Pass on Grim (defaults/main modes, module acting): Step (sweeps), GUI Move (sweeps), No Fall Air place/Place, Self Web, Burrow (x1 only), Trigger Bot, Auto Clicker, Aim Assist, Hitboxes (fight and Test), Jump Reset, Fake Lag, Blink (x1), Auto Weapon (default fight), Fast Use (all modes), Speed Mine (all modes), Fast Place, Auto Eat, Liquid Filler, Spawn Proofer, Long Jump, Air Jump, Sprint, Auto Walk, Safe Walk, Reverse Step, Fast Climb, Sneak, No Jump Delay, Scaffold Legit/Godbridge, Ghost Hand, Fast Use (Throw modes), Aim Assist (players) Test, all render modules together.

## The other anticheats in short

- **Horizon anticheat** (upstream): kicks only Flight (Abilities x3 anti-kick modes, Velocity/Normal: prediction ~60 + ground ~45, "Kicked by the server's anticheat"). Logs without acting: Speed Vanilla (prediction x8), Timer (timer x12), Anti AFK Server spin (prediction x12), Velocity-mode Flight (ground x30), No Fall Test (ground x17, claim dropped). Its combat check (attacks without swing / two targets a tick) never fired: Kill Aura in every mode passes. Everything else passes.
- **Intave**: kicks (it bans by default; here a kick): Flight Velocity/Packet, Speed Vanilla, Offhand Crystal, Auto Tool Silk touch (protocolscanner "sent slot twice", VL 100 = ban), Velocity Knockback delay (physics VL 1000; physics VL builds up across windows, so the Offhand Crystal kick came mostly from earlier ones), and in Tests Speed, Step, Parkour, No Slow, Crystal Aura, Anchor Aura, Auto Weapon, W Tap, Packet Mine, Sword PvP Bot. Flags physics on Flight/Speed/Surround/Self Trap/Burrow/Offhand/Anti AFK Server/High Jump/Spider/Timer, breakspeedlimiter on Nuker packet mine, interactionraytrace/placementanalysis on Scaffold Normal and Nuker, attackraytrace on Kill Aura (mostly 0-weight; Snap +60), heuristics "rotation exact"/"follows entity movement too precisely" on Kill Aura. Intave got noisier as the run went on (Baseline (2): physics +360 after many setbacks).
- **Spartan** (limited free): kicks Flight Velocity/Normal, No Fall No ground (gravity-simulation), Tests Flight and Spider. Flags flight/speed/no fall/criticals jump modes (gravity-simulation, speed-simulation, exploits/ground_spoof, criticals/packet x2). Kill Aura, Reach and Hitboxes barely register (hit-reach x1).
- **NoCheatPlus-Updated**: kicks Flight (4 of 6), Speed Vanilla/Strafe ("Moved unexpectedly", SurvivalFly), Self Trap Anti face place ("Invalid block placements", Against), and in Tests Flight, Air Jump, Parkour, Anchor, GUI Move, Scaffold, W Tap, Packet Mine, Burrow, Auto City/Auto Trap. Flags Kill Aura Visible/Direction x1-2.

## Baseline findings (no Sunset module)

- Plain play, hits by hand and being hit pass every anticheat for Tester.
- **Grim flags plain sprint hits from Horizon's client**: "Baseline: sprint hits" gave Simulation x28 in 2 of 4 Grim runs (acs4, smoke2), clean in the other 2. Not Sunset's; W Tap's x4 is within that.
- **Horizon's own anticheat flags a vanilla Horizon client**: Bot1 (no module) got prediction x108 (VL 349) and x39 while being hit by Tester with knockback resistance 1 (its replay doesn't model the attribute). Grim gave Bot1 Simulation x1-7 while hit in the W Tap/Jump Reset Tests; Intave gives Bot1 timer flags of weight 0 throughout.

## vape-rs (github.com/rdbtCVS/vape-rs, d6e48b1, MIT) per failing Grim module

| Sunset module (Grim checks) | vape-rs | Would its approach avoid the flag? |
|---|---|---|
| Kill Aura (Reach, Hitboxes, AimDuplicateLook) | SilentAura `src/modules/combat/silent_aura.rs` + `rotation.rs` | **Mostly yes.** It attacks only when its managed rotation's ray hits the target at vanilla's 3.0 (`src/core/combat_runtime.rs:208` passes `BASE_RANGE`; `silent_aura.rs:497`, `:709-751`), else it swings and misses (Click::Miss): no Reach, no Hitboxes. Rotation moves in whole mouse counts of the sensitivity (`rotation.rs:137`, `:425`, GCD-like) and goes into the tick's own move packet (`docs/hooks.md:43` MODIFY_ARG of `position_sender.update`), which should stop AimDuplicateLook. Untested here. Their plain KillAura (`kill_aura.rs:113-114`: attack range 3.5, never turns) would still flag Reach and Hitboxes. Our Range default 4.5 (`mixins/sunset/aura.rs:42`) is where the Reach flags come from. |
| Reach (Reach, hits cancelled) | `combat/reach.rs`, README: needs host support pinned Horizon lacks | No. Any reach past 3.0 is what Grim's Reach check measures. |
| Criticals Packet/Mini jump (GroundSpoof, Simulation) | no Criticals module (only hit_select.rs times hits) | No equivalent. Our Jump mode already passes Grim by hand. |
| Velocity Scale/Knockback delay (AntiKB) | `combat/velocity.rs`: Chance 40%, Horizontal 90%, Vertical 100%, Ticks 1 (scales later instead of replacing the packet) | Fewer flags, not none: 40% of hits at 90% is still a reduction past Grim's 0.001 threshold; scaling a tick later also differs from the simulated knockback. |
| Flight, Speed, Timer, No Fall Packet/No ground | `blatant/fly.rs`, `blatant/speed.rs` (AntiCheat B, Bhop), `blatant/mod.rs:51` (setTimerSpeed), `blatant/no_fall.rs` (Normal claims ground; AntiCheat sends a status pair) | No. Same moves Grim's simulation and Timer check catch; the status pair is an extra move packet (26.x kicks a second one a tick). |
| Spider, Glide, High Jump, Anti Hunger | none | No equivalent. |
| Surround, Self Trap, Auto City/Auto Trap, Crystal Aura, Anchor Aura (Post, AirLiquidPlace, PositionPlace) | none ported (README: CrystalAura, AutoAnchor, Block-In still to port) | No module to compare. Their general method would fix Post: actions run at client-tick HEAD, before the tick's move packet (`docs/hooks.md:141`, `:156`; pre-motion actions `:65`). AirLiquidPlace/PositionPlace need placing on a real face along the crosshair, as Scaffold Legit does. |
| Scaffold Normal (Post, PositionPlace, AimDuplicateLook) | Legit (`movement/scaffold.rs`), GodBridge (`movement/god_bridge.rs`), TellyBridge | **Yes**: our ports of their Legit and GodBridge (vs.'s #30) pass Grim in all three Tests. Visible rotation through the shared rotation manager, placement from the real crosshair at client-tick HEAD (`docs/hooks.md:156`, `:183`). |
| Nuker, Packet Mine, Break Delay (FastBreak, PositionBreak, Post) | none | No equivalent. |
| Auto Tool, and BadPacketsA in Crystal/Anchor Aura, Packet Mine, Auto Weapon | `utility/auto_tool.rs:152` switches only when the best slot differs and uses the host's slot change | **Yes.** It sends one SetCarriedItem. Ours sends two (Other findings). |
| Anti AFK Server spin (Simulation, BadPacketsJ, AimModulo360) | `world/anti_afk.rs`: small visible turns (Max yaw change 10) at a random speed, short walks, no server spin | **Yes**: nothing like the 360 spin Grim checks for. |
| Offhand Silent open, Auto Weapon, Anchor Aura (AimDuplicateLook) | rotation through the host's move packet (`docs/hooks.md:43`) | Likely: the host sends a look only when it changed. Untested. |
| No Slow (NoSlow) | `blatant/no_slowdown.rs`: skips the use slowdown | No, same thing. |
| Fake Player (Simulation from colliding with it) | none | No equivalent. |
| Blink (Simulation x1), Fake Lag (pass) | `blatant/outbound.rs` | Ours already passes or nearly passes. Nothing to gain. |

## Other findings

- **Sunset sends the held slot twice.** `hold()` (`mixins/sunset/runtime.rs:2817`) sends SetCarriedItem and sets `game.selected` but not `remote.carried`, so the game's tick (`horizon-new crates/horizon-client/src/game/remote.rs:4867`) sends it again. Grim flags BadPacketsA. Intave flags protocolscanner "sent slot twice", and its third flag is a ban (Auto Tool Silk touch sweep). hold() is used about 60 times in 15 files (Auto Tool, Auto Weapon, Surround, Crystal/Anchor Aura, Packet Mine, Loadouts...). Report only, not fixed.
- Kill Aura's default Range 4.5 is above survival reach (3.0); every Kill Aura fight flagged Grim Reach, and Grim cancelled those hits.
- Panic + death screen: a `.panic` typed while dead is lost, so Sunset stays hidden and every later `.command` goes to the server as chat. That's how the first Grim run broke. Harness issue, but a player who taps Panic while dead would see the same.
- Paper-family fights in the long runs: after ~15 fights, clicks from ctl stopped landing on both clients until a full clean(). Not explained; acs4 reran the fights with a full clean each.

## Table: scenario (module + mode) x anticheat

Rows where every anticheat passes are kept for Sweeps/Fights/Defaults and dropped for Tests. See Method for what the cells mean.

| Scenario | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Baseline: play, no module (1) | pass | pass | pass | pass | pass |
| Baseline: hits on Bot1 by hand, no module | pass | pass | pass | pass | pass |
| Baseline: hit by Bot1, no module | pass | pass | pass | pass | pass |
| Baseline: hits from 4.5 blocks, no module | pass | pass | pass | pass | pass |
| Baseline: sprint hits, no module | Simulation x28 | pass | pass | pass | pass |
| Baseline: play, no module (2) | pass | pass | pass | pass | pass |
| Baseline: hits on Bot1 by hand, no module (2) | pass | pass | pass | pass | pass |
| Sweep: flight (mode: Abilities, anti kick mode: Packet) | Simulation x35 | physics x45 +431 | gravity-simulation x20, exploits/ground_spoof x9, speed-simulation x1 | SurvivalFly x3 | prediction x58, ground x40 **KICK** |
| Sweep: flight (mode: Abilities, anti kick mode: Normal) | Simulation x15 | physics x31 +271 | gravity-simulation x13, exploits/ground_spoof x13, speed-simulation x3 | SurvivalFly x3 | prediction x59, ground x48 **KICK** |
| Sweep: flight (mode: Abilities, anti kick mode: None) | Simulation x19 | physics x31 +281 | exploits/ground_spoof x14, gravity-simulation x9, speed-simulation x7 | SurvivalFly x3 **KICK** | prediction x61, ground x41 **KICK** |
| Sweep: flight (mode: Velocity, anti kick mode: Packet) | Simulation x38 | physics x2 +17 **KICK** | speed-simulation x31, exploits/ground_spoof x7 | SurvivalFly x1 **KICK** | ground x31, prediction x6 |
| Sweep: flight (mode: Velocity, anti kick mode: Normal) | Simulation x38 | physics x64 +322 | speed-simulation x9 **KICK** | SurvivalFly x1 **KICK** | prediction x65, ground x23 **KICK** |
| Sweep: flight (mode: Velocity, anti kick mode: None) | Simulation x38 | physics x46 +348 | speed-simulation x34, gravity-simulation x16, exploits/ground_spoof x10 | SurvivalFly x1 **KICK** | ground x29, prediction x5 |
| Sweep: speed (mode: Vanilla) | Simulation x28 | physics x35 +330 **KICK** | pass | SurvivalFly x1 **KICK** | prediction x8 |
| Sweep: speed (mode: Strafe) | Simulation x28 | physics x51 +375 | gravity-simulation x22, speed-simulation x6 | SurvivalFly x1 **KICK** | pass |
| Sweep: nofall (mode: Packet) | GroundSpoof x11 | physics x2 +1 | exploits/ground_spoof x3 | SurvivalFly x1 | pass |
| Sweep: nofall (mode: No ground) | GroundSpoof x2 | pass | gravity-simulation x15 **KICK** | SurvivalFly x3 | pass |
| Sweep: nofall (mode: Air place) | pass | pass | pass | pass | pass |
| Sweep: nofall (mode: Place) | pass | pass | pass | pass | pass |
| Sweep: step (active when: Always) | pass | pass | pass | pass | pass |
| Sweep: step (active when: Sneaking) | pass | pass | pass | pass | pass |
| Sweep: step (active when: Not sneaking) | pass | pass | pass | pass | pass |
| Sweep: guimove (screens: All menus) | pass | pass | pass | pass | pass |
| Sweep: guimove (screens: Inventory) | pass | pass | pass | pass | pass |
| Sweep: killaura (rotate: Always) | pass | pass | pass | pass | pass |
| Sweep: killaura (rotate: On hit) | pass | pass | pass | pass | pass |
| Sweep: killaura (rotate: None) | pass | pass | pass | pass | pass |
| Sweep: killaura + criticals (rotate: Always) | pass | pass | pass | pass | pass |
| Sweep: killaura + criticals (rotate: None) | pass | pass | pass | pass | pass |
| Sweep: killaura (attack when holding: All) | pass | pass | pass | pass | pass |
| Sweep: surround (center: Never) | Post x20, Simulation x9 | physics x3 +20, placementanalysis x1 +0 | fast-place x8 | pass | pass |
| Sweep: surround (center: On activate) | Post x2, Simulation x1 | physics x9 +10 | pass | SurvivalFly x1 | prediction x1 |
| Sweep: surround (center: Always) | Simulation x53, Post x2 | physics x52 +440 | pass | SurvivalFly x1 | prediction x1 |
| Sweep: surround (center: Incomplete) | Post x3 | physics x9 +10 | pass | pass | pass |
| Sweep: selftrap (top mode: Top) | Simulation x3, AirLiquidPlace x1, PositionPlace x1, Post x1, GroundSpoof x1 | physics x10 +20 | pass | SurvivalFly x1 | prediction x1 |
| Sweep: selftrap (top mode: Anti face place) | AirLiquidPlace x74, PositionPlace x73, Post x72, Simulation x12, AimDuplicateLook x4, BadPacketsJ x3 | physics x13 +31 | pass | Against x2, SurvivalFly x1 **KICK** | pass |
| Sweep: selftrap (top mode: Full) | AirLiquidPlace x74, PositionPlace x73, Post x69, Simulation x12, AimDuplicateLook x4, BadPacketsJ x3 | physics x11 +30 | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: selftrap (top mode: None) | Post x3 | physics x10 +20 | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: selfweb (mode: Normal) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: selfweb (mode: Smart) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: burrow (block: Obsidian) | GroundSpoof x1, Simulation x1 | physics x11 +20 | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: burrow (block: Held) | GroundSpoof x1, Simulation x1 | physics x12 +30 | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: offhand (item: Crystal) | pass | physics x16 +72 **KICK** | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: offhand (item: Totem) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: offhand (item: Gapple) | pass | physics x17 +151 | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: offhand (item: Enchanted gapple) | pass | physics x36 +330 | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: offhand (item: None) | pass | physics x29 +210 | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: offhand (silent open: false) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: offhand (silent open: true) | AimDuplicateLook x8 | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: autoexp (mode: Both) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: autoexp (mode: Armor) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: autoexp (mode: Hands) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: fastuse (mode: All) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Sweep: fastuse (mode: Some) | pass | pass | pass | pass | pass |
| Sweep: fastuse (throwables: true, throw mode: Delay) | pass | pass | pass | pass | pass |
| Sweep: fastuse (throwables: true, throw mode: Every tick) | pass | pass | pass | pass | pass |
| Sweep: fastuse (throwables: true, throw mode: Burst) | pass | pass | pass | pass | pass |
| Sweep: speedmine (mode: Damage) | pass | pass | pass | pass | pass |
| Sweep: speedmine (mode: Normal) | pass | pass | pass | pass | pass |
| Sweep: speedmine (mode: Haste) | pass | pass | pass | pass | pass |
| Sweep: antiafk (spin mode: Server) | Simulation x18, BadPacketsJ x3, AimModulo360 x2 | physics x32 +177 | exploits/baritone_2 x1 | SurvivalFly x2 | prediction x12 |
| Sweep: antiafk (spin mode: Client) | pass | pass | exploits/baritone_2 x1 | pass | pass |
| Sweep: autoeat (threshold mode: Any) | pass | pass | pass | pass | pass |
| Sweep: autoeat (threshold mode: Health) | pass | pass | pass | pass | pass |
| Sweep: autoeat (threshold mode: Hunger) | pass | pass | pass | pass | pass |
| Sweep: autoeat (threshold mode: Both) | pass | pass | pass | pass | pass |
| Sweep: autotool (prefer: Fortune) | BadPacketsA x1 | protocolscanner x1 +0 | pass | pass | pass |
| Sweep: autotool (prefer: None) | BadPacketsA x1 | protocolscanner x1 +0 | pass | pass | pass |
| Sweep: autotool (prefer: Silk touch) | BadPacketsA x1 | protocolscanner x1 +100 **KICK** | pass | pass | pass |
| Sweep: nuker (shape: Sphere) | Post x1 | pass | pass | pass | pass |
| Sweep: nuker (shape: Cube) | Post x1 | pass | pass | pass | pass |
| Sweep: nuker (shape: Uniform cube) | Post x1 | pass | pass | pass | pass |
| Sweep: nuker (mode: All) | Post x3, WrongBreak x1, FastBreak x1 | interactionraytrace x1 +20, placementanalysis x1 +0 | pass | pass | pass |
| Sweep: nuker (mode: Smash) | PositionBreakB x2 | pass | pass | pass | pass |
| Sweep: nuker (sort mode: None) | Post x1 | placementanalysis x1 +0 | pass | pass | pass |
| Sweep: nuker (sort mode: Furthest) | PositionBreakA x1, Post x1 | interactionraytrace x1 +20 | pass | pass | pass |
| Sweep: nuker (sort mode: Top down) | PositionBreakA x1, Post x1 | pass | pass | pass | pass |
| Sweep: nuker (packet mine: true) | FastBreak x75, Post x11, PositionBreakA x6, Simulation x5 | breakspeedlimiter x96 +745, interactionraytrace x3 +60, placementanalysis x1 +0 | pass | pass | timer x1, prediction x1 |
| Sweep: liquidfiller (place in: Both) | pass | pass | pass | pass | pass |
| Sweep: liquidfiller (place in: Water) | pass | pass | pass | pass | pass |
| Sweep: liquidfiller (place in: Lava) | pass | pass | pass | pass | pass |
| Sweep: spawnproofer (mode: Both) | pass | pass | pass | pass | pass |
| Sweep: spawnproofer (mode: Always) | pass | pass | pass | pass | pass |
| Sweep: spawnproofer (mode: Potential) | pass | pass | pass | pass | pass |
| Sweep: antihunger (sprint: true) | GroundSpoof x2 | pass | pass | pass | pass |
| Sweep: velocity | pass | pass | pass | pass | pass |
| Sweep: blink | Simulation x1 | pass | pass | pass | pass |
| Sweep: glide | Simulation x2 | invalid (module never on: kicked or desync) | pass | pass | pass |
| Sweep: longjump | pass | pass | pass | pass | pass |
| Sweep: highjump | Simulation x2 | physics x10 +21 | pass | pass | pass |
| Sweep: airjump | pass | pass | pass | pass | pass |
| Sweep: spider | Simulation x19, GroundSpoof x3 | physics x23 +54 | gravity-simulation x9 | SurvivalFly x1 | pass |
| Sweep: parkour | pass | pass | pass | pass | pass |
| Sweep: safewalk | pass | pass | pass | pass | pass |
| Sweep: sprint | pass | pass | pass | pass | pass |
| Sweep: autowalk | pass | pass | pass | pass | pass |
| Sweep: reversestep | pass | pass | pass | pass | pass |
| Sweep: anchor | pass | pass | pass | pass | pass |
| Sweep: fastclimb | pass | pass | pass | pass | pass |
| Sweep: autojump | pass | pass | gravity-simulation x12 | pass | pass |
| Sweep: sneak | pass | pass | pass | pass | pass |
| Sweep: noslow | pass | pass | pass | FastBreak x1 | pass |
| Sweep: nojumpdelay | pass | pass | pass | pass | pass |
| Sweep: antivoid | pass | pass | pass | pass | pass |
| Sweep: elytraboost | pass | pass | pass | pass | pass |
| Sweep: chestswap | pass | pass | pass | pass | pass |
| Sweep: scaffold | Post x14, PositionPlace x9, AimDuplicateLook x8, Simulation x2 | interactionraytrace x9 +5, placementanalysis x3 +27, physics x3 +30 | pass | pass | pass |
| Sweep: timer | Timer x92 | physics x27 +1, timer x18 +73 | pass | pass | timer x12 |
| Sweep: fastplace | pass | pass | pass | pass | timer x1 |
| Sweep: breakdelay | pass | pass | pass | pass | pass |
| Sweep: autoclicker | pass | pass | pass | pass | pass |
| Sweep: autoweapon | pass | pass | pass | pass | pass |
| Sweep: triggerbot | pass | pass | pass | pass | pass |
| Sweep: autoarmor | pass | pass | pass | pass | pass |
| Sweep: autoreplenish | pass | pass | pass | pass | pass |
| Sweep: autogap | pass | pass | pass | pass | pass |
| Sweep: airplace | pass | pass | pass | pass | pass |
| Sweep: reach | pass | pass | pass | pass | pass |
| Sweep: autotrap | pass | pass | pass | pass | pass |
| Sweep: autoweb | pass | pass | pass | pass | pass |
| Sweep: holefiller | pass | pass | pass | pass | pass |
| Sweep: aimassist | pass | pass | pass | pass | pass |
| Sweep: bowspam | pass | pass | pass | pass | pass |
| Sweep: crystaloptimizer | pass | pass | pass | pass | pass |
| Sweep: flamethrower | pass | pass | pass | pass | pass |
| Sweep: autoshearer | pass | pass | pass | pass | pass |
| Sweep: automount | pass | pass | pass | pass | pass |
| Sweep: autosteal | pass | pass | pass | pass | pass |
| Sweep: inventorycleaner | pass | pass | pass | pass | pass |
| Sweep: middleclickextra | pass | pass | pass | pass | pass |
| Sweep: autofish | pass | pass | pass | pass | pass |
| Fight: Kill Aura (defaults) | Reach x5, AimDuplicateLook x2 | attackraytrace x3 +0 | pass | pass | pass |
| Fight: Kill Aura (Rotation mode Snap) | Reach x5, AimDuplicateLook x1 | attackraytrace x7 +60 | pass | pass | pass |
| Fight: Kill Aura (Rotation mode Normal) | Reach x5, AimDuplicateLook x2 | attackraytrace x3 +0 | pass | pass | pass |
| Fight: Kill Aura (Rotation mode Smooth) | Reach x5 | attackraytrace x3 +0 | pass | pass | pass |
| Fight: Kill Aura (Rotation mode Custom) | Reach x5, AimDuplicateLook x1, Hitboxes x1 | attackraytrace x3 +0 | pass | pass | pass |
| Fight: Kill Aura (Rotate Always) | Reach x6 | attackraytrace x5 +0 | pass | pass | pass |
| Fight: Kill Aura (Rotate On hit) | Reach x4 | attackraytrace x5 +0, heuristics x1 +4 | pass | pass | pass |
| Fight: Kill Aura (Rotate None) | Hitboxes x7 | attackraytrace x7 +0 | pass | pass | pass |
| Fight: Kill Aura (Attack mode Switch) | Reach x5, AimDuplicateLook x3 | attackraytrace x3 +0 | hit-reach/simple x1 | pass | pass |
| Fight: Kill Aura (Attack mode Multi) | Reach x5, AimDuplicateLook x3 | attackraytrace x3 +0 | pass | pass | pass |
| Fight: Kill Aura + Criticals (Packet) | GroundSpoof x13, Simulation x8, Reach x2, Hitboxes x2 | physics x7 +9, attackraytrace x5 +84, heuristics x1 +4 | criticals/packet x2, hit-reach/simple x1 | pass | prediction x2 |
| Fight: Kill Aura + Criticals (Jump) | Hitboxes x5 | attackraytrace x5 +0, heuristics x1 +4 | gravity-simulation x15 | pass | pass |
| Fight: Kill Aura + Criticals (Mini jump) | GroundSpoof x10, Simulation x10, Reach x4 | physics x12 +120, attackraytrace x4 +16 | gravity-simulation x16, hit-reach/simple x1, kill-aura/move_direction_1 x1 | SurvivalFly x1 | pass |
| Fight: Trigger Bot (defaults) | pass | pass | pass | pass | pass |
| Fight: Criticals (Packet), hits by hand | GroundSpoof x3, Simulation x3 | physics x16 +20 | pass | pass | pass |
| Fight: Criticals (Jump), hits by hand | pass | attackraytrace x3 +0 | gravity-simulation x18 | pass | pass |
| Fight: Criticals (Mini jump), hits by hand | GroundSpoof x16, Simulation x16 | physics x17 +170 | gravity-simulation x20, kill-aura/move_direction_1 x2 | SurvivalFly x1 | pass |
| Fight: Aim Assist (defaults), hits by hand on a strafing Bot1 | pass | pass | pass | pass | pass |
| Fight: Auto Clicker (defaults), left held on Bot1 | pass | pass | pass | pass | pass |
| Fight: Hitboxes (defaults), hits 0.5 off Bot1 | pass | attackraytrace x7 +28 | pass | pass | pass |
| Fight: Reach (defaults), hits from 4.5 blocks | Reach x8 | attackraytrace x7 +0 | hit-reach/simple x1 | pass | pass |
| Fight: W Tap (defaults), sprint hits | Simulation x4 | pass | pass | pass | pass |
| Fight: Jump Reset (defaults), hit by Bot1 | pass | pass | pass | pass | pass |
| Fight: Velocity (Scale), hit by Bot1 | AntiKB x5 | physics x78 +503 | pass | pass | pass |
| Fight: Velocity (Blend), hit by Bot1 | Simulation x3, AntiKB x1, GroundSpoof x1 | physics x8 +24 | pass | SurvivalFly x1 | pass |
| Fight: Velocity (Knockback delay), hit by Bot1 | AntiKB x5 | physics x20 +153 **KICK** | pass | pass | pass |
| Fight: Auto Weapon (defaults), hits by hand | pass | pass | pass | pass | pass |
| Fight: Fake Lag (defaults), hits by hand | pass | pass | pass | pass | pass |
| Fight: Blink (defaults), hits by hand | pass | pass | kill-aura/move_direction_1 x1 | pass | pass |
| Defaults: Auto City | pass | pass | pass | pass | pass |
| Defaults: Crystal Aura | pass | pass | pass | pass | pass |
| Defaults: Anchor Aura | pass | pass | pass | pass | pass |
| Defaults: Crystal Macro | pass | pass | pass | pass | pass |
| Defaults: Anchor Macro | pass | pass | pass | pass | pass |
| Defaults: Friend Guard | pass | pass | pass | pass | pass |
| Defaults: Key Throw | pass | pass | pass | pass | pass |
| Defaults: Sword PvP Bot | pass | pass | pass | pass | pass |
| Defaults: Anti Anvil | pass | pass | pass | pass | pass |
| Defaults: Self Anvil | pass | pass | pass | pass | pass |
| Defaults: Auto Anvil | pass | pass | pass | pass | pass |
| Defaults: Anti Bed | pass | pass | pass | pass | pass |
| Defaults: Quiver | pass | pass | pass | pass | pass |
| Defaults: Arrow Dodge | pass | pass | pass | pass | pass |
| Defaults: Bow Aimbot | pass | pass | pass | pass | pass |
| Defaults: Wind Jump | pass | pass | pass | pass | pass |
| Defaults: Pearl Catch | pass | pass | pass | pass | pass |
| Defaults: Snap Tap | pass | pass | pass | pass | pass |
| Defaults: Entity Control | pass | pass | pass | pass | pass |
| Defaults: Auto Respawn | pass | pass | pass | pass | pass |
| Defaults: Death Location | pass | pass | pass | pass | pass |
| Defaults: Auto Log | pass | pass | pass | pass | pass |
| Defaults: Notifier | pass | invalid (module never on: kicked or desync) | pass | pass | pass |
| Defaults: Hold to Eat | pass | pass | pass | pass | pass |
| Defaults: Loadouts | pass | pass | pass | pass | pass |
| Defaults: Rotation | pass | pass | pass | pass | pass |
| Defaults: No Rotate | pass | pass | pass | pass | pass |
| Defaults: No Status Effects | pass | pass | pass | pass | pass |
| Defaults: Potion Saver | pass | pass | pass | pass | pass |
| Defaults: No Mining Trace | pass | pass | pass | pass | pass |
| Defaults: Portals | pass | pass | pass | pass | pass |
| Defaults: Multitask | pass | pass | pass | pass | pass |
| Defaults: Liquid Interact | pass | pass | pass | pass | pass |
| Defaults: No Interact | pass | pass | pass | pass | pass |
| Defaults: Fake Player | Simulation x24, AntiKB x1 | pass | pass | pass | pass |
| Defaults: RF Click | pass | pass | pass | pass | pass |
| Defaults: Spam | pass | pass | pass | pass | pass |
| Defaults: Ghost Hand | pass | pass | pass | pass | pass |
| Defaults: No Break Reset | pass | pass | pass | pass | pass |
| Defaults: Anti Vanish | pass | pass | pass | pass | pass |
| Defaults: Server Scan | pass | pass | pass | pass | pass |
| Defaults: Freecam | pass | pass | pass | pass | pass |
| Defaults: Free Look | pass | pass | pass | pass | pass |
| Defaults: Shulker Peek | pass | pass | pass | pass | pass |
| Defaults: Packet Mine | Post x1 | pass | pass | pass | pass |
| Defaults: Auto Nametag | pass | pass | pass | pass | pass |
| Defaults: Spawner Alerts | pass | pass | pass | pass | pass |
| Defaults: Base Finder | pass | pass | pass | pass | pass |
| Defaults: Unlock All Recipes | pass | pass | pass | pass | pass |
| Defaults: Profiles | pass | pass | pass | pass | pass |
| Defaults: Client Sort | pass | pass | pass | pass | pass |
| Defaults: render modules at once (esp, tracers, nametags, storageesp, blockesp, holeesp, breadcrumbs, waypoints, trajectories, lightoverlay, tunnelesp, voidesp, radar, logoutspots, xray, fullbright, norender, clearwater, crosshair, blockoverlay, targetesp, atmosphere, handview, zoom, timechanger, weatherchanger) | pass | pass | pass | pass | pass |
| Test: Speed | Simulation x71, Timer x14 | physics x117 +1000 **KICK** | speed-simulation x45, gravity-simulation x17 | SurvivalFly x4 | prediction x146, timer x1 |
| Test: Flight | Simulation x91, NoFall x13, GroundSpoof x5 | physics x156 +955 | exploits/ground_spoof x48, gravity-simulation x29, speed-simulation x18 **KICK** | SurvivalFly x3 **KICK** | prediction x157, ground x45 **KICK** |
| Test: No Fall | GroundSpoof x9 | physics x15 +8 | gravity-simulation x11, exploits/ground_spoof x5 | invalid (module never on: kicked or desync) | ground x17 |
| Test: Step | Simulation x8 | physics x4 +38 **KICK** | pass | invalid (module never on: kicked or desync) | prediction x4 |
| Test: Glide | Simulation x42, GroundSpoof x3 | physics x80 +196 | gravity-simulation x45 | invalid (module never on: kicked or desync) | prediction x124 |
| Test: Spider | Simulation x16, GroundSpoof x2 | physics x10 +100 | **KICK** | invalid (module never on: kicked or desync) | prediction x44 |
| Test: High Jump | Simulation x2 | physics x2 +20 | gravity-simulation x7 | invalid (module never on: kicked or desync) | prediction x51 |
| Test: Long Jump | Simulation x1 | physics x1 +10 | pass | invalid (module never on: kicked or desync) | prediction x1 |
| Test: Air Jump | Simulation x1 | physics x1 +10 | pass | SurvivalFly x2 **KICK** | prediction x22 |
| Test: Parkour | Simulation x147 | physics x69 +668 **KICK** | gravity-simulation x9 | SurvivalFly x5 **KICK** | prediction x94 |
| Test: Anchor | Simulation x31, AntiKB x1 | pass | pass | SurvivalFly x5 **KICK** | prediction x5 |
| Test: Velocity (knockback) | AntiKB x2, Simulation x2, GroundSpoof x1 | physics x2 +20 | pass | invalid (module never on: kicked or desync) | pass |
| Test: Timer | Timer x196, TimerLimit x9 | pass | pass | pass | timer x29 |
| Test: GUI Move | Simulation x12 | physics x29 +270 | pass | SurvivalFly x1 **KICK** | prediction x34 |
| Test: Snap Tap | Simulation x10 | physics x18 +170 | pass | SurvivalFly x1 | prediction x26 |
| Test: Scaffold | Post x55, PositionPlace x38, Simulation x37, AimDuplicateLook x6 | physics x68 +371, interactionraytrace x68 +140, placementanalysis x13 +0 | gravity-simulation x31, fast-place x15, impossible-actions/scaffold_direction x6, impossible-actions/tower x2, impossible-actions/scaffold_analysis x1 | Scaffold x2, Against x2, SurvivalFly x1 **KICK** | prediction x8, timer x2 |
| Test: Scaffold (Legit, backwards) | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Test: Scaffold (Godbridge, backwards) | pass | pass | impossible-actions/scaffold_analysis x2 | SurvivalFly x1 | pass |
| Test: Scaffold (Godbridge, jump) | pass | pass | impossible-actions/scaffold_analysis x6 | pass | pass |
| Test: Nuker (survival) | Post x12, PositionBreakA x9, FastBreak x8 | breakspeedlimiter x9 +55, interactionraytrace x5 +65, placementanalysis x4 +0 | pass | pass | pass |
| Test: Fast Place (speeds) | PositionBreakB x1 | pass | pass | pass | pass |
| Test: Break Delay | PositionBreakB x21, FastBreak x4 | breakspeedlimiter x2 +10 | pass | pass | pass |
| Test: No Slow (eating, Ground %) | NoSlow x40, Simulation x11 | physics x17 +168 **KICK** | pass | SurvivalFly x1 | prediction x54 |
| Test: Kill Aura | AimDuplicateLook x26 | attackraytrace x5 +0 | pass | pass | pass |
| Test: Criticals | AimDuplicateLook x19, GroundSpoof x14, Simulation x14 | physics x15 +98 | gravity-simulation x7 | SurvivalFly x2 | pass |
| Test: Crystal Aura | Post x47, BadPacketsA x31 | protocolscanner x2 +100 **KICK** | pass | pass | pass |
| Test: Anchor Aura | AimDuplicateLook x75, Post x57, BadPacketsA x46 | protocolscanner x1 +100 **KICK** | pass | pass | pass |
| Test: Auto Weapon (Axe on shields) | AimDuplicateLook x360, BadPacketsA x2 | attackraytrace x2 +0, protocolscanner x1 +100 **KICK** | pass | pass | pass |
| Test: W Tap | Simulation x71, AimDuplicateLook x2 | physics x73 +490, heuristics x15 +60, attackraytrace x2 +0 **KICK** | kill-aura/aim_complex x3 | SurvivalFly x12 **KICK** | pass |
| Test: Jump Reset | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Test: Fake Lag | pass | pass | pass | invalid (module never on: kicked or desync) | pass |
| Test: Packet Mine | PositionBreakB x20, Post x13, FastBreak x6, BadPacketsA x6, AimDuplicateLook x4 | protocolscanner x1 +100 **KICK** | pass | SurvivalFly x1 **KICK** | pass |
| Test: Reach (Entity reach) | Reach x5 | attackraytrace x5 +0 | pass | pass | pass |
| Test: Sword PvP Bot | Simulation x1 | heuristics x13 +52, physics x1 +10 **KICK** | gravity-simulation x21 | pass | pass |
| Test: Entity Control (boat) | Simulation x3 | physics x156 +29 | pass | pass | pass |
| Test: Wind Jump | Simulation x6, GroundSpoof x3 | physics x3 +30 | gravity-simulation x19 | pass | prediction x101 |
| Test: Pearl Catch | BadPacketsJ x1 | pass | pass | pass | prediction x2 |
| Test: Aim Assist (players) | pass | invalid (module never on: kicked or desync) | pass | pass | pass |
| Test: Offhand (Auto Totem) | AimDuplicateLook x5 | pass | pass | pass | pass |
| Test: Surround | Post x8 | placementanalysis x1 +0 | pass | pass | pass |
| Test: Burrow | GroundSpoof x1, Simulation x1 | physics x3 +30 | pass | Against x1 **KICK** | prediction x4 |
| Test: Ghost Hand (Hits) | pass | attackraytrace x5 +0 | pass | pass | pass |
| Test: Ghost Hand (Blocks) | Post x1 | pass | impossible-actions/interact x1 | pass | pass |
| Test: Bow Spam (Crossbows) | Post x1 | pass | pass | pass | pass |
| Test: Quiver | AimDuplicateLook x1 | pass | pass | pass | pass |
| Test: Arrow Dodge | Simulation x8 | physics x7 +70 | speed-simulation x2 | SurvivalFly x1 | prediction x1 |
| Test: Auto City and Auto Trap (rates) | Post x182, AirLiquidPlace x172, PositionPlace x172, AimDuplicateLook x8, FastBreak x5 | interactionraytrace x4 +80, breakspeedlimiter x2 +20 | pass | FastBreak x1, Against x1 **KICK** | pass |
| Test: Plain mining (held button) | invalid (module never on: kicked or desync) | invalid (module never on: kicked or desync) | invalid (module never on: kicked or desync) | invalid (module never on: kicked or desync) | invalid (module never on: kicked or desync) |

## Rerun

`cd ~/hzmeteor; TWO=1 DISPLAY0=111 VERIFY=ac/acsweep.py ./ac/runall-ac.sh <dir> "" grim` (any of grim horizon-ac intave spartan ncp; ACFIGHTS=1 for baselines and fights only), then `python3 ac/acreport.py <dir>` and `python3 ac/gentable.py` (edit its LONG/FIGHTS run names). Servers: ~/hzmeteor/servers/{grim,horizon-ac,intave,spartan,ncp}, ports 25610-25614, rcon port+100 (password sunset).
