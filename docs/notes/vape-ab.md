# vape-rs vs Sunset, anticheat A/B (3 Nov 2026, threadripper)

Sayer's rule: for each module vape-rs's approach is the default answer; Sunset fixes its own module only where vape-rs's is
both worse (works less well) and flags more. Both clients ran the same scenarios on the five AC servers of the sweep
(GrimAC 2.3.75, Intave 14.9.7, Spartan 4.0.9 limited, NoCheatPlus-Updated b281, Horizon's own AC at f4c87b4ee), each
installed alone, with Bot1 running the same binary with nothing on. Raw: ~/hzmeteor/vab1 (vape-rs), sab1 + sab2
(Sunset main 48452d0), after the changes: sab3/sab4 (Sunset, Grim), acs5 (the sweep's rows for every changed module,
Grim), vab2 (vape-rs No Slow again).

## Verdicts

| Module (Sunset / vape-rs) | Grim, Sunset before | Grim, vape-rs | Effect (Grim; elsewhere) | Verdict | Done |
|---|---|---|---|---|---|
| Kill Aura / SilentAura, KillAura | AimDuplicateLook x7, Reach x5 | SilentAura clean; KillAura Hitboxes x17 | 21 dmg (3 full hits) vs SilentAura 12 (3 hits, random 6-13 CPS); KillAura 12 | **port vape-rs** | Kill Aura Rotation mode **Silent aura** (new default): managed look in whole mouse counts, PID + jitter, sent through the game's position sender, hits only within vanilla reach. After: **clean, 21 dmg** |
| Aim Assist / AimAssist | clean | clean | 0 of 8 hits (Duelkit only adds to your mouse) vs **8 of 8** (turns on its own) | **port vape-rs** | Aim Assist Mode **Simple** (new default, Require mouse down on as vape-rs); Duelkit kept. After: clean, 8/8 |
| Anti AFK / Anti-AFK | AimModulo360 x18 (Server spin); Spartan gravity/baritone | clean everywhere | ours: invisible server spin / 7035 deg visible spin; vape-rs: 12 deg of turns, 0.7 blocks of steps in 50 s | **port vape-rs** | Anti AFK Mode **Wander** (new default); Meteor kept. After: clean, 14 deg / 0.94 blocks |
| Auto Tool / AutoTool | BadPacketsA x1, Intave protocolscanner x1 | clean | same (6 stone in 4 s, pickaxe taken) | **port vape-rs** (one SetCarriedItem) | the shared hold() fix. After: clean |
| No Slow / NoSlowdown | NoSlow x42 / x43 (two runs) | NoSlow x23 / x22 | 1.5 / 1.3 blocks kept vs **6.0 / 6.0** (Grim sets back); 12.6-13 on the others for both | **port vape-rs** | No Slow Mode **Skip** (new default): the game's use slowdown left out of the move, not in water, not after a knockback until you land; Scale (Ground %/Air %) kept. After: NoSlow x27, 4.6 blocks |
| Velocity / Velocity, KnockbackDelay | Scale AntiKB x6 (0 KB); KB delay AntiKB x6 + Simulation x6 | Velocity **Simulation x62** for 4% less KB; KnockbackDelay clean but no reduction | 0.00 vs 1.90 blocks a hit | vape-rs worse **and** flags more: ours stays | nothing to fix: any reduction past 0.001 is Grim AntiKB (one a hit, no setback); the sweep has no better route |
| Blink / Blink | clean, 6 hits | TimerLimit x65, 1 hit | ours lands hits while held, vape-rs doesn't | ours better, flags less: **leave ours** | - |
| Fake Lag / FakeLag | clean | clean | 8 hits both | neither (same) | - |
| No Fall / NoFall | Packet GroundSpoof x8, 0 dmg; No ground x1, 0 dmg | Normal GroundSpoof x8, **10 dmg**; AntiCheat Timer/TickTimer x25, 10 dmg | ours saves the fall on Grim, vape-rs's doesn't | ours better, flags no more: **leave ours** | - |
| Speed / Speed | Vanilla Simulation x26, Strafe x30 | Bhop x28+GroundSpoof, AntiCheat B x28 | both set back to ~3 blocks/3 s on Grim; Spartan: ours Vanilla clean 16.9, vape-rs Bhop gravity x3 24.2 | neither, both flag the same | - |
| Timer / Timer | Timer x120 (2.0) | Timer x7, TimerLimit x5 (1.07) | ours set back to 5 blocks (Grim), 26 elsewhere; vape-rs 9.9 (Grim), 13.9 elsewhere | neither, both flag the same check (a lower Multiplier only flags less often) | - |
| Flight / Fly | Simulation x16, 6.5 blocks kept | Simulation x7, 0 kept; Intave kick | ours goes further on Grim and isn't kicked by Intave | neither, both flag | - |
| Reach / Reach | Reach x8, hits cancelled; Spartan 8/8 hits | clean, but **0 hits anywhere** (needs a host hook this Horizon pin lacks) | ours works where the AC lets it; vape-rs's does nothing | neither: vape-rs's doesn't run on this pin; any reach past 3 is Grim Reach | - |
| Scaffold / Scaffold | Normal pass here (sweep: Post/PositionPlace); Legit and Godbridge clean | Legit, GodBridge clean; TellyBridge fell | Legit/GodBridge: same (we ported them in #30) | already ported; Telly not (falls in this test) | - |

Shared fixes, done regardless: **hold()** told the server the slot and left remote.carried stale, so the tick sent it again
(Grim BadPacketsA, Intave "sent slot twice"); it now records it (sunset_remote::tell_carried). **Looks**: Sunset's turn path
sent a look the server already had (an extra rotation packet each tick a module held one); a look equal to the last one
sent is now left out of the packet, and nothing extra is sent for it (what waited for it still runs, your own look comes
back the next tick as before). That is vape-rs's behaviour (its look goes through the game's own sender) and cleared every
AimDuplicateLook in the sweep's rows (table below).

## vape-rs build

- github.com/rdbtCVS/vape-rs at **d6e48b1** (1 Nov 2026 01:17, "Record the wind-down merge..."), MIT. Scratch only:
  ~/scratch-vape (clone, pinned horizon-mixins build, staged copy, target 4.5 GB). Nothing of it is in Sunset's repo.
- Safety scan: no sockets, processes, FFI or `unsafe` in the shipped code (its own pack step also refuses them by token;
  checked by grep too). Files: only <HORIZON_HOME>/native-mods/vape-rs (current.json, profiles/, atomic writes, symlinks
  refused); read-only /etc/localtime, /etc/timezone, /usr/share/zoneinfo and $TZ for the HUD clock. std::process::id and
  temp dirs only in #[cfg(test)]. Macros can send a chat command the user wrote (game chat, nothing else).
- Built with its own xtask `pack` (Rust 1.96.1) and its pinned horizon-mixins **0e9229d** (ours, 0d6b103, refuses its
  debug_key hook as an opaque macro), staged on our pin **ba6344a** (vape-rs pins 8f91f89, 1354 commits newer). Changes:
  horizon_revision -> ba6344a; two injections left out (keep-sprint on remote_player_attack, freecam-camera on
  player_camera_entity: neither exists at ba6344a, so KeepSprint and Freecam's camera are unwired); its aura's attack no
  longer calls remote_player_attack (8f91f89's attacker sprint slow-down after hitting a player, which ba6344a's own attack
  doesn't do either); cosmetic API gaps: visible_block_entities -> block_entities (no chunk-fade filter), world
  bottom_y/top_y -> layout().min_y/max_y, projectile touching_water -> false (Pearl Catch/Trajectories water drag), the
  name-tag unit argument dropped, custom potion effects -> none (Target Info/AutoHeal). 170 warnings, all vape-rs's.
- Headless control (ac/vapeab.py + ac/runall-ab.sh): vape-rs reads <HORIZON_HOME>/native-mods/vape-rs/current.json at
  start; VSEED=~/hzmeteor/ac/vseed seeds two profiles, `off` and `A`, bound to J and K (`profile_binds`). A switch loads
  the profile file from disk and applies its module states and settings, so each scenario writes profiles/A.json (its
  modules `{"enabled": true}` + settings like `"blatant.speed.mode": "AntiCheat B"`), taps K, checks current.json, and taps
  J to end. Gotchas: vape-rs drops keys while its window isn't focused, and a server join on Xvfb (no window manager)
  leaves it unfocused: `xdotool windowfocus` before each tap, and Bot1 on its own Xvfb display. Don't press F10 (Panic)
  between scenarios: a switch first saves the active profile's state over its file. Setting ids are
  `<module id>.<setting>` (registry.rs: combat.kill-aura, combat.silent-aura, combat.velocity, combat.knockback-delay,
  combat.aim-assist, combat.reach, blatant.fly/speed/timer/no-fall/no-slowdown/blink/scaffold, world.fake-lag,
  world.auto-tool, world.anti-afk).

## Method and caveats

Scenarios (ac/vapeab.py, the same play for both clients, each with its effect measured): fights against Bot1 (1000
health, knockback resistance 1, strafing; hits = Bot1's health drops sampled every 2 ticks), hand clicks at Bot1 from
5.2 blocks (Reach), clicks aimed once 0.6 to Bot1's side (Aim Assist), hit by Bot1 six times (Velocity: blocks pushed a
hit), 3 s walks in real time (Speed, Timer), a jump-then-forward flight, a 13-block fall in survival with regeneration
off (No Fall), 3 s walking while eating a golden apple (No Slow), 4 s mining a stone that comes back with a sword in hand
and a pickaxe in slot 5 (Auto Tool), 50 s standing (Anti AFK; vape-rs's starts after 30-40 s idle), bridging off an edge
for 5 s (Scaffold). Flags are Tester's, from each AC's log in the scenario window (abreport.py; the window counts only if
the module was confirmed on).

- One run each, except No Slow (twice for both: the gap held). Grim matters most; its setbacks are why the effect on Grim
  is far below the others for anything that moves you.
- NCP: its SurvivalFly VL stays high after the Speed/Flight kicks, so for both clients every row after Speed was kicked or
  not reached, baselines included: read NCP only up to Timer.
- Horizon AC: every fight landed one hit, baselines too (Bot1's knockback resistance isn't applied there, so the first
  hit knocks it out of reach). Its fight rows say nothing.
- The first Sunset wave lost Bot1 on NCP and Horizon AC to the GPU running out of memory with ten clients (6 GB); those
  two were rerun (sab2) with four.
- vape-rs Aim Assist ran with Require mouse down off (its default needs attack held; the clicks are momentary). Sunset's
  Simple mode was measured the same way after.

## Sweep rows for the changed modules on Grim, before and after

Rows for Anti AFK keep their sweep names: they set only Spin mode, so after the change they run the new default, Wander.

| Sweep row (Grim) | before (acs3/acs4, cb6f6b3) | after (acs5) | after: what it did |
|---|---|---|---|
| Sweep: killaura (rotate: Always) | pass | pass | 152 ticks played, still in the game |
| Sweep: offhand (silent open: true) | AimDuplicateLook x8 | pass | 152 ticks played, still in the game |
| Sweep: antiafk (spin mode: Server) | Simulation x18, BadPacketsJ x3, AimModulo360 x2 | pass | 152 ticks played, still in the game |
| Sweep: antiafk (spin mode: Client) | pass | pass | 152 ticks played, still in the game |
| Sweep: autotool (prefer: Fortune) | BadPacketsA x1 | pass | 152 ticks played, still in the game |
| Sweep: autotool (prefer: None) | BadPacketsA x1 | pass | 152 ticks played, still in the game |
| Sweep: autotool (prefer: Silk touch) | BadPacketsA x1 | pass | 152 ticks played, still in the game |
| Fight: Kill Aura (defaults) | Reach x5, AimDuplicateLook x2 | pass | Bot1 lost 21.0 health |
| Fight: Kill Aura (Rotation mode Snap) | Reach x5, AimDuplicateLook x1 | Reach x5 | Bot1 lost 21.0 health |
| Fight: Kill Aura (Rotation mode Normal) | Reach x5, AimDuplicateLook x2 | Reach x5 | Bot1 lost 21.0 health |
| Fight: Kill Aura (Rotation mode Smooth) | Reach x5 | Reach x5 | Bot1 lost 21.0 health |
| Fight: Kill Aura (Rotation mode Custom) | Reach x5, AimDuplicateLook x1, Hitboxes x1 | Reach x5 | Bot1 lost 21.0 health |
| Fight: Kill Aura (Rotate On hit) | Reach x4 | Reach x5 | Bot1 lost 21.0 health |
| Fight: Kill Aura (Attack mode Switch) | Reach x5, AimDuplicateLook x3 | pass | Bot1 lost 35.0 health |
| Fight: Aim Assist (defaults), hits by hand on a strafing Bot1 | pass | pass | Bot1 lost 0.0 health |
| Fight: Auto Weapon (defaults), hits by hand | pass | pass | Bot1 lost 42.0 health |
| Test: Kill Aura | AimDuplicateLook x26 | pass | pig 2.5 blocks away lost 10 health at Range 4.5, 0.0 at Range 1.5; behind a wall: 2 swings |
| Test: Crystal Aura | Post x47, BadPacketsA x31 | Post x42 | a crystal broken by hand beside Bot1 costs it 74.79168701171875; 1.6 crystals a second (16 |
| Test: Anchor Aura | AimDuplicateLook x75, Post x57, BadPacketsA x46 | Post x41 | a crystal broken by hand beside Bot1 costs it 74.79168701171875; 1.9 anchors a second (19  |
| Test: Auto Weapon (Axe on shields) | AimDuplicateLook x360, BadPacketsA x2 | Post x1 | Bot1 behind a raised shield lost 0.0 health in 10 s without Axe on shields, 21.0 with it;  |
| Test: Packet Mine | PositionBreakB x20, Post x13, FastBreak x6, BadPacketsA x6, AimDuplicateLook x4 | Post x9, FastBreak x6 | (ticks plain with the tool, ticks with Packet Mine holding a sword, in hand after, most mo |
| Test: Offhand (Auto Totem) | AimDuplicateLook x5 | pass | Item None: offhand empty; Item Totem: first slot's totem True, inventory closed after True |
| Test: Quiver | AimDuplicateLook x1 | pass | strength after Quiver: True; still on: False; tipped arrows left 1 of 2, plain arrows 16 |

## Tests

- SP + Paper while working: all changed modules' tests and the look-path users pass, except Quiver on SP and Pearl Catch
  on Paper, which fail the same way on main's binary (rerun side by side); a first build broke Quiver on Paper (your own
  look came back in the same tick as a look the server already had) and was fixed before the runs below.
- Grim after: sab3 (A/B scenarios), acs5 (sweep rows), sab4 (No Slow Skip).
- Four targets: ~/hzmeteor/rVfull (28 tests: every changed module, the look-path users, Speed): sp 26/27, horizon 24/26,
  vanilla 26/27, paper 26/27 after retries. The failures all fail the same way on main's earlier full runs (rE0full,
  rE1full): Crystal Aura and Anchor Aura on Horizon's server (no crystal damage there), Pearl Catch on Paper and vanilla,
  Quiver on SP. Rust tests of sunset_rotation (incl. the two new managed-look tests): 12/12. Zero warnings.

## Rerun

`cd ~/hzmeteor; BIN=~/scratch-vape/target/fast VSEED=~/hzmeteor/ac/vseed CLIENT=vape TWO=1 VERIFY=ac/vapeab.py
./ac/runall-ab.sh <dir> "" grim intave spartan ncp horizon-ac` (vape-rs; at most 8 clients at once on this GPU), the same
with `CLIENT=sunset` and no BIN/VSEED for Sunset; then `python3 ac/abreport.py <dir>` and `python3 ac/gen_ab.py
"name=run[,run]" ...` for the tables. vape-rs rebuild: ~/scratch-vape/hm/target/release/horizon-mixins apply --workspace
~/hzmeteor/horizon-new --mods ~/scratch-vape/mods --out ~/scratch-vape/stage, then cargo build --profile fast in stage with
--target-dir ~/scratch-vape/target.

## Every scenario, per anticheat ("flags / effect")

**Baseline: hits by hand on a strafing Bot1**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| vape-rs | clean / 8 hits, 56.0 dmg (7.0 a hit) | timer x1 / 8 hits, 55.1 dmg (6.9 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| Sunset after | clean / 8 hits, 56.0 dmg (7.0 a hit) | - | - | - | - |

**Kill Aura: Sunset Kill Aura (defaults) / vape KillAura (defaults)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | AimDuplicateLook x7, Reach x5 / 3 hits, 21.0 dmg (7.0 a hit) | attackraytrace x7 / 2 hits, 14.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 2 hits, 14.0 dmg (7.0 a hit) |
| vape-rs | Hitboxes x17 / 4 hits, 12.1 dmg (3.0 a hit) | attackraytrace x19, timer x1 / 2 hits, 1.8 dmg (0.9 a hit) | clean / 8 hits, 15.6 dmg (1.9 a hit) | clean / 11 hits, 21.8 dmg (2.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| Sunset after | clean / 3 hits, 21.0 dmg (7.0 a hit) | - | - | - | - |

**Kill Aura: Sunset Kill Aura (Range 3) / vape KillAura (attack range 3)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | AimDuplicateLook x1 / 3 hits, 21.0 dmg (7.0 a hit) | timer x1 / 5 hits, 12.6 dmg (2.5 a hit) | clean / 3 hits, 21.0 dmg (7.0 a hit) | clean / 3 hits, 21.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| vape-rs | clean / 1 hits, 7.0 dmg (7.0 a hit) | timer x1 / 1 hits, 7.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| Sunset after | clean / 3 hits, 21.0 dmg (7.0 a hit) | - | - | - | - |

**Reach: hits by hand from 5.2 blocks**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | Reach x8 / 0 hits, 0.0 dmg (0.0 a hit) | attackraytrace x7, timer x1 / 0 hits, 0.0 dmg (0.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| vape-rs | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) |
| Sunset after | Reach x8 / 0 hits, 0.0 dmg (0.0 a hit) | - | - | - | - |

**Baseline: hits by hand from 5.2 blocks**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 0 hits, 0.0 dmg (0.0 a hit) | timer x1, physics x1 / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) |
| vape-rs | clean / 0 hits, 0.0 dmg (0.0 a hit) | timer x1 / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) |
| Sunset after | clean / 0 hits, 0.0 dmg (0.0 a hit) | - | - | - | - |

**Aim Assist: clicks aimed once 0.6 to Bot1's side**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) |
| vape-rs | clean / 8 hits, 56.0 dmg (7.0 a hit) | timer x1 / 8 hits, 54.3 dmg (6.8 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| Sunset after | clean / 8 hits, 56.0 dmg (7.0 a hit) | - | - | - | - |

**Baseline: clicks aimed once 0.6 to Bot1's side**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 0 hits, 0.0 dmg (0.0 a hit) | timer x1 / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) |
| vape-rs | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) | clean / 0 hits, 0.0 dmg (0.0 a hit) |
| Sunset after | clean / 0 hits, 0.0 dmg (0.0 a hit) | - | - | - | - |

**Blink: hits by hand**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 6 hits, 42.0 dmg (7.0 a hit) | timer x2 / 3 hits, 21.0 dmg (7.0 a hit) | clean / 6 hits, 42.0 dmg (7.0 a hit) | clean / 6 hits, 42.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| vape-rs | TimerLimit x65 / 1 hits, 7.0 dmg (7.0 a hit) | timer x2 / 0 hits, 0.0 dmg (0.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) | timer x13 / 1 hits, 7.0 dmg (7.0 a hit) |
| Sunset after | clean / 6 hits, 42.0 dmg (7.0 a hit) | - | - | - | - |

**Fake Lag: hits by hand**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| vape-rs | clean / 8 hits, 56.0 dmg (7.0 a hit) | timer x1 / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 8 hits, 56.0 dmg (7.0 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |
| Sunset after | clean / 8 hits, 56.0 dmg (7.0 a hit) | - | - | - | - |

**Baseline: hit by Bot1**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / pushed 1.98/hit | clean / pushed 1.98/hit | gravity-simulation x6 / pushed 1.98/hit | clean / pushed 1.98/hit | clean / pushed 1.98/hit |
| vape-rs | clean / pushed 1.98/hit | timer x1 / pushed 1.98/hit | gravity-simulation x6 / pushed 1.98/hit | clean / pushed 1.99/hit | clean / pushed 1.98/hit |
| Sunset after | clean / pushed 1.98/hit | - | - | - | - |

**Velocity: Sunset Scale (defaults) / vape Velocity (defaults)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | AntiKB x6 / pushed 0.00/hit | physics x5, timer x1 / pushed 0.00/hit | clean / pushed 0.00/hit | clean / pushed 0.00/hit | clean / pushed 0.00/hit |
| vape-rs | Simulation x62 / pushed 1.90/hit | timer x1 / pushed 1.58/hit | gravity-simulation x6 / pushed 1.90/hit | clean / pushed 0.00/hit | clean / pushed 1.89/hit |
| Sunset after | AntiKB x6 / pushed 0.00/hit | - | - | - | - |

**Velocity: Sunset Blend**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | Simulation x18, AntiKB x6, GroundSpoof x6 / pushed 1.44/hit | physics x5, timer x1 / pushed 0.62/hit | clean / pushed 0.74/hit | SurvivalFly x1 / pushed 0.16/hit | clean / pushed 0.74/hit |
| Sunset after | Simulation x18, AntiKB x6, GroundSpoof x6 / pushed 1.44/hit | - | - | - | - |

**Velocity: Sunset Knockback delay / vape KnockbackDelay**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | AntiKB x6, Simulation x6 / pushed 0.68/hit | physics x3, timer x1 / pushed 0.00/hit | clean / pushed 0.00/hit | clean / pushed 0.00/hit | clean / pushed 0.00/hit |
| vape-rs | clean / pushed 1.98/hit | timer x1 / pushed 1.98/hit | gravity-simulation x6 / pushed 1.95/hit | clean / pushed 1.98/hit | clean / pushed 1.98/hit |
| Sunset after | AntiKB x6, Simulation x5 / pushed 0.63/hit | - | - | - | - |

**Baseline: walk 3 s**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 12.95 blocks/3 s | physics x2, timer x1 / 12.95 blocks/3 s | clean / 12.95 blocks/3 s | clean / 12.95 blocks/3 s | clean / 12.95 blocks/3 s |
| vape-rs | clean / 12.95 blocks/3 s | timer x1 / 12.95 blocks/3 s | clean / 13.16 blocks/3 s | clean / 12.95 blocks/3 s | clean / 12.95 blocks/3 s |
| Sunset after | clean / 12.95 blocks/3 s | - | - | - | - |

**Speed: Sunset Vanilla (defaults) / vape Bhop (defaults)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | Simulation x26 / 3.21 blocks/3 s | physics x61, timer x2 / 3.08 blocks/3 s | clean / 16.94 blocks/3 s | SurvivalFly x1 / 0.11 blocks/3 s | prediction x58 / 16.94 blocks/3 s |
| vape-rs | Simulation x28, GroundSpoof x2 / 3.23 blocks/3 s | physics x34, timer x1 / 13.23 blocks/3 s | gravity-simulation x3 / 24.17 blocks/3 s | SurvivalFly x2 / 0.39 blocks/3 s | prediction x66 / 23.69 blocks/3 s |
| Sunset after | Simulation x26 / 3.21 blocks/3 s | - | - | - | - |

**Speed: Sunset Strafe / vape AntiCheat B**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | Simulation x30 / 2.43 blocks/3 s | physics x32, timer x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | gravity-simulation x26 / 22.32 blocks/3 s | SurvivalFly x1 / 0.13 blocks/3 s | prediction x59 / 22.32 blocks/3 s |
| vape-rs | Simulation x28, GroundSpoof x1 / 0.00 blocks/3 s | physics x33, timer x2 / 6.72 blocks/3 s | speed-simulation x35, gravity-simulation x7 / 22.26 blocks/3 s | SurvivalFly x2 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | prediction x61 / 25.41 blocks/3 s |
| Sunset after | Simulation x30 / 2.43 blocks/3 s | - | - | - | - |

**Timer: walk 3 s (defaults: Sunset 2.0, vape 1.07)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | Timer x120, TimerLimit x9, Simulation x6 / 4.96 blocks/3 s | timer x28, physics x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / 26.12 blocks/3 s | SurvivalFly x2 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | timer x7 / 26.12 blocks/3 s |
| vape-rs | Timer x7, TimerLimit x5, Simulation x4 / 9.92 blocks/3 s | timer x10, physics x1 / 13.91 blocks/3 s | clean / 13.81 blocks/3 s | clean / 14.03 blocks/3 s | clean / 14.03 blocks/3 s |
| Sunset after | Timer x112, TimerLimit x22, Simulation x5 / 3.88 blocks/3 s | - | - | - | - |

**Fly: Sunset Flight (defaults) / vape Fly (defaults)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | Simulation x16 / rose 0.22, 6.47 blocks forward while on, 6.47 kept; fall dmg 0.0 | physics x29 / rose 6.98, 5.59 blocks forward while on, 5.60 kept; fall dmg 0.0 | exploits/ground_spoof x19, speed-simulation x14, gravity-simulation x12 / rose 13.80, 22.15 blocks forward while on, 22.15 kept; fall dmg 4.0 | SurvivalFly x1 / rose 0.30, 1.96 blocks forward while on, 1.96 kept; fall dmg 0.0 | prediction x92, ground x38 **KICK** / error: TypeError("'NoneType' object is not subscriptable") |
| vape-rs | Simulation x7 / rose 1.29, 0.00 blocks forward while on, 0.00 kept; fall dmg 0.0 | physics x40, timer x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | gravity-simulation x25, speed-simulation x11 / rose 4.00, 12.50 blocks forward while on, 12.50 kept; fall dmg 0.0 | SurvivalFly x3 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | prediction x80 / rose 4.00, 20.00 blocks forward while on, 20.51 kept; fall dmg 0.0 |
| Sunset after | Simulation x16 / rose 0.22, 6.47 blocks forward while on, 6.47 kept; fall dmg 0.0 | - | - | - | - |

**Baseline: fall 13 blocks**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 10.0 dmg | timer x1 / 10.0 dmg | gravity-simulation x1 / 10.0 dmg | SurvivalFly x2 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / 10.0 dmg |
| vape-rs | clean / 10.0 dmg | timer x1 / 10.0 dmg | gravity-simulation x1 / 10.0 dmg | SurvivalFly x2 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / 10.0 dmg |
| Sunset after | clean / 10.0 dmg | - | - | - | - |

**No Fall: Sunset Packet (defaults) / vape Normal (defaults)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | GroundSpoof x8 / 0.0 dmg | physics x16, timer x1 / 10.0 dmg | gravity-simulation x16, exploits/ground_spoof x14 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | SurvivalFly x2 **KICK** (not on) / error: TypeError("'NoneType' object is not subscriptable") | ground x18 / 10.0 dmg |
| vape-rs | GroundSpoof x8 / 10.0 dmg | physics x11, timer x1 / 10.0 dmg | gravity-simulation x16, exploits/ground_spoof x12 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | SurvivalFly x2 **KICK** (not on) / error: TypeError("'NoneType' object is not subscriptable") | ground x11 / 10.0 dmg |
| Sunset after | GroundSpoof x8 / 0.0 dmg | - | - | - | - |

**No Fall: Sunset No ground / vape AntiCheat**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | GroundSpoof x1 / 0.0 dmg | clean / 10.0 dmg | gravity-simulation x1 / 0.0 dmg | SurvivalFly x1 **KICK** (not on) / error: TypeError("'NoneType' object is not subscriptable") | clean / 0.0 dmg |
| vape-rs | TickTimer x14, Timer x10, Simulation x2, TimerLimit x1 / 10.0 dmg | clean / 1.0 dmg | gravity-simulation x1 / 1.0 dmg | SurvivalFly x2 **KICK** (not on) / error: TypeError("'NoneType' object is not subscriptable") | clean / 1.0 dmg |
| Sunset after | GroundSpoof x1 / 0.0 dmg | - | - | - | - |

**Baseline: walk 3 s eating**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 2.59 blocks/3 s | physics x7 / 3.54 blocks/3 s | clean / 2.59 blocks/3 s | SurvivalFly x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / 2.59 blocks/3 s |
| vape-rs | clean / 2.63 blocks/3 s | clean / 2.59 blocks/3 s | clean / 2.63 blocks/3 s | SurvivalFly x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / 2.59 blocks/3 s |
| vape-rs again | clean / 2.59 blocks/3 s | - | - | - | - |
| Sunset after | clean / 2.59 blocks/3 s | - | - | - | - |
| Sunset after (No Slow Skip) | clean / 2.59 blocks/3 s | - | - | - | - |

**No Slow: walk 3 s eating**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | NoSlow x42 / 1.51 blocks/3 s | physics x12, timer x1 / 8.31 blocks/3 s | clean / 12.95 blocks/3 s | SurvivalFly x3 **KICK** (not on) / module not on | prediction x60 / 12.95 blocks/3 s |
| vape-rs | NoSlow x23 / 6.00 blocks/3 s | physics x15, timer x1 / 12.60 blocks/3 s | clean / 12.82 blocks/3 s | SurvivalFly x3 **KICK** (not on) / module not on | prediction x60 / 12.60 blocks/3 s |
| vape-rs again | NoSlow x22 / 5.99 blocks/3 s | - | - | - | - |
| Sunset after | NoSlow x43 / 1.29 blocks/3 s | - | - | - | - |
| Sunset after (No Slow Skip) | NoSlow x27 / 4.57 blocks/3 s | - | - | - | - |

**Baseline: mine with a sword in hand**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 0 stone broken in 4 s; slots held [1] | timer x1 / 0 stone broken in 4 s; slots held [1] | clean / 0 stone broken in 4 s; slots held [1] | SurvivalFly x4 **KICK** / 48 stone broken in 4 s; slots held [1] | clean / 0 stone broken in 4 s; slots held [1] |
| vape-rs | clean / 0 stone broken in 4 s; slots held [1] | timer x1 / 0 stone broken in 4 s; slots held [1] | clean / 0 stone broken in 4 s; slots held [1] | SurvivalFly x1 / 48 stone broken in 4 s; slots held [1] | clean / 0 stone broken in 4 s; slots held [1] |
| Sunset after | clean / 0 stone broken in 4 s; slots held [1] | - | - | - | - |

**Auto Tool: mine with a sword in hand**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | BadPacketsA x1 / 6 stone broken in 4 s; slots held [5] | protocolscanner x1 / 6 stone broken in 4 s; slots held [5] | clean / 6 stone broken in 4 s; slots held [5] | SurvivalFly x1 **KICK** (not on) / module not on | clean / 6 stone broken in 4 s; slots held [5] |
| vape-rs | clean / 6 stone broken in 4 s; slots held [5] | clean / 6 stone broken in 4 s; slots held [5] | clean / 6 stone broken in 4 s; slots held [5] | clean / 6 stone broken in 4 s; slots held [5] | clean / 6 stone broken in 4 s; slots held [5] |
| Sunset after | clean / 6 stone broken in 4 s; slots held [5] | - | - | - | - |

**Baseline: stand 50 s**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | timer x1 / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | SurvivalFly x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s |
| vape-rs | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s |
| Sunset after | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | - | - | - | - |

**Anti AFK: Sunset Spin Server (defaults) / vape Anti-AFK (defaults)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | AimModulo360 x18 / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | timer x1 / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | gravity-simulation x28, exploits/baritone_2 x11 / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | SurvivalFly x4 **KICK** (not on) / module not on | clean / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s |
| vape-rs | clean / view turned 12 degrees, feet moved up to 0.77 blocks in 50 s | timer x1 / view turned 12 degrees, feet moved up to 0.70 blocks in 50 s | clean / view turned 12 degrees, feet moved up to 0.76 blocks in 50 s | SurvivalFly x2 **KICK** / view turned 8 degrees, feet moved up to 0.70 blocks in 50 s | clean / view turned 12 degrees, feet moved up to 0.70 blocks in 50 s |
| Sunset after | AimModulo360 x18 / view turned 0 degrees, feet moved up to 0.00 blocks in 50 s | - | - | - | - |

**Anti AFK: Sunset Spin Client**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / view turned 7035 degrees, feet moved up to 0.00 blocks in 50 s | timer x1 / view turned 7035 degrees, feet moved up to 0.00 blocks in 50 s | gravity-simulation x19, exploits/baritone_2 x9 / view turned 7035 degrees, feet moved up to 0.00 blocks in 50 s | SurvivalFly x1 **KICK** (not on) / module not on | clean / view turned 7035 degrees, feet moved up to 0.00 blocks in 50 s |
| Sunset after | clean / view turned 7035 degrees, feet moved up to 0.00 blocks in 50 s | - | - | - | - |

**Scaffold: Sunset Normal (defaults), forward**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 3 blocks placed, 0.0 blocks past the edge | clean / 3 blocks placed, 0.0 blocks past the edge | exploits/baritone_2 x1 / 3 blocks placed, 0.0 blocks past the edge | SurvivalFly x1 (not on) / module not on | clean / 3 blocks placed, 0.0 blocks past the edge |
| Sunset after | clean / 3 blocks placed, 0.0 blocks past the edge | - | - | - | - |

**Scaffold: Sunset Legit / vape Legit (defaults), backwards**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 9 blocks placed, 9.3 blocks past the edge | clean / 8 blocks placed, 7.8 blocks past the edge | gravity-simulation x1 / 7 blocks placed, 7.3 blocks past the edge | clean (not on) / module not on | clean / 7 blocks placed, 7.3 blocks past the edge |
| vape-rs | clean / 8 blocks placed, 7.7 blocks past the edge | clean / 8 blocks placed, 7.9 blocks past the edge | gravity-simulation x1 / 7 blocks placed, 7.3 blocks past the edge | clean (not on) / module not on | clean / 8 blocks placed, 8.3 blocks past the edge |
| Sunset after | clean / 7 blocks placed, 6.6 blocks past the edge | - | - | - | - |

**Scaffold: Sunset Godbridge / vape GodBridge, backwards**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 12 blocks placed, 12.1 blocks past the edge | placementanalysis x1 / 12 blocks placed, 12.1 blocks past the edge | impossible-actions/scaffold_analysis x4, gravity-simulation x1 / 12 blocks placed, 12.1 blocks past the edge | clean (not on) / module not on | clean / 12 blocks placed, 12.1 blocks past the edge |
| vape-rs | clean / 12 blocks placed, 11.4 blocks past the edge | clean / 12 blocks placed, 11.4 blocks past the edge | clean / 11 blocks placed, 10.8 blocks past the edge | SurvivalFly x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / 12 blocks placed, 11.4 blocks past the edge |
| Sunset after | clean / 12 blocks placed, 12.1 blocks past the edge | - | - | - | - |

**Baseline: bridge backwards by hand**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset before | clean / 1 blocks placed, 16.7 blocks past the edge, fell | timer x1 / 2 blocks placed, 16.8 blocks past the edge, fell | gravity-simulation x1 / 1 blocks placed, 16.7 blocks past the edge, fell | clean / 1 blocks placed, 16.7 blocks past the edge, fell | clean / 1 blocks placed, 13.8 blocks past the edge, fell |
| vape-rs | clean / 1 blocks placed, 16.7 blocks past the edge, fell | clean / 1 blocks placed, 16.7 blocks past the edge, fell | gravity-simulation x1 / 1 blocks placed, 16.7 blocks past the edge, fell | clean / 1 blocks placed, 16.7 blocks past the edge, fell | clean / 1 blocks placed, 13.8 blocks past the edge, fell |
| Sunset after | clean / 2 blocks placed, 16.8 blocks past the edge, fell | - | - | - | - |

**Kill Aura: vape SilentAura (defaults)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| vape-rs | clean / 3 hits, 12.0 dmg (4.0 a hit) | timer x1 / 4 hits, 10.7 dmg (2.7 a hit) | clean / 2 hits, 14.0 dmg (7.0 a hit) | clean / 3 hits, 10.5 dmg (3.5 a hit) | clean / 1 hits, 7.0 dmg (7.0 a hit) |

**Scaffold: vape TellyBridge, backwards**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| vape-rs | clean / 10 blocks placed, 5.9 blocks past the edge, fell | placementanalysis x1 / 10 blocks placed, 5.9 blocks past the edge, fell | gravity-simulation x3 / 10 blocks placed, 5.9 blocks past the edge, fell | SurvivalFly x1 **KICK** / error: TypeError("'NoneType' object is not subscriptable") | clean / 10 blocks placed, 5.9 blocks past the edge, fell |

**Kill Aura: Sunset Kill Aura (Rotation mode Snap, the old default)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset after | Reach x5 / 3 hits, 21.0 dmg (7.0 a hit) | - | - | - | - |

**Anti AFK: Sunset Wander (defaults, vape-rs's)**

| client | grim | intave | spartan | ncp | horizon-ac |
|---|---|---|---|---|---|
| Sunset after | clean / view turned 14 degrees, feet moved up to 0.94 blocks in 50 s | - | - | - | - |
