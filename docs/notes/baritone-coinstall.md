# Baritone co-install test (3 Nov 2026, agent BT, report only: no Sunset code changed)

**Verdict: works.** horizon-baritone builds on our pin after 3 small path fixes and runs in the same game as Sunset.
In SP and on Paper: both load and both show up in Options > Mods, every shared hook fires for both, Sunset's modules
work while `#goto` / `#mine` run, at most one move packet a tick, no kick, no crash. The only real interaction is
Auto Walk + `#goto` (both hold W, see Conflicts). One `#mine` failure in SP is Baritone's own: a Baritone-only build
fails it the same way.

## What was used
- horizon-baritone **0a944c4** (github.com/luhnox/horizon-baritone, 3 Nov 08:14 +0800, "Add BlockOptionalMeta, cost
  hooks for the calculation context"), their pin Nightly 142388e (1 Nov), horizon-mixins 0d6b103 (same as ours).
- Built on our pin **ba6344a** (~/hzmeteor/horizon-new) with Sunset as in ~/hzmeteor/mod/mixins/sunset (= main 6dc3dfc)
  plus the test-only wire probe (tools/probe). One stage, one target, `--profile fast`: 6 min cold, 0 errors,
  163 warnings. A second build (Baritone + probe only, same stage path, 4 min) was used to isolate the SP `#mine` result.
- Everything is in **~/scratch-baritone**, outside the Sunset repo: clone `horizon-baritone/` (untouched), patched
  copy `mods-b/baritone/`, both-mod set `mods-both/`, binaries `bin-both/` and `bin-bonly/`, test scripts `tools/`
  (bt.py, mods_page.py, tab.py). The 3.7 GB target dir was deleted afterwards (disk 12 GB free). Nothing of Baritone
  is in Sunset: LGPL-3.0-derived, it stays its own .hnmod. Their repo still has no LICENSE/COPYING (NOTICE.md says
  to add them before publishing).

## Safety scan (0a944c4)
No network (no std::net or HTTP crates; Cargo.lock has no dependencies), no processes, no unsafe, no file writes.
Two file reads, both started by the user: `#build <file>` reads a schematic (relative names resolve under
`native_data_dir("baritone")/schematics`, absolute paths are taken as given) and `#explorefilter <path>` reads a
JSON file. Threads only in tests. It sends no packets of its own: it drives the game's keys and look, so the game
sends its usual one move packet a tick. Off on servers until `#set allowOnServers true` (checked: it refuses on
Paper until set).

## Pin problems (142388e -> ba6344a): what broke, what changed (scratch copy only)
1. `E_REVISION`: horizon-mixins requires the exact revision. Changed `horizon_revision` to ba6344a.
2. `E_MODULE "module destination must be an indexed Rust source file"` for `crates/horizon-client/src/chat.rs`:
   at ba6344a `ChatScreen` is still in **horizon-client-render**/src/chat.rs (by 142388e it had moved to
   horizon-client). Changed the `mod_suggest_bridge` module file to `crates/horizon-client-render/src/chat.rs`,
   the `suggest` injection target to `horizon_client_render::chat::ChatScreen::update_command_info`, and one path in
   src/baritone.rs (`horizon_client::chat::baritone_suggest::set` -> `horizon_client_render::chat::...`).
   Nothing else: every other target and game API it uses is the same at ba6344a (native hooks API 3 here, it asks for 1).
   Checked: Tab completion works (`#go` suggests goal/goto, Tab gives `#goal`).
3. Not checked: the 0.9.0 retarget to stable v0.2.97 (a3acb5f, older still). The chat.rs move and any other changes
   there would need the same look.

## Shared hooks (in the staged source and in game)
Both mods are priority 100. horizon-mixins chains same-target HEAD hooks alphabetically, so **Baritone runs first**:
- `handle_chat_input` (both cancellable): Baritone takes `#...` lines, Sunset takes `.`. Both answered (`#help` 10-12
  lines, `.t fullbright` on/off), and plain chat still reaches the server. Only risk: Baritone's `prefix` setting set
  to `.` would swallow Sunset's commands, because Baritone goes first.
- `Game::tick` (Baritone keys_tick, Sunset pretick), `native_tick`: both run (Auto Walk and Kill Aura with paths going).
- `native_mods` / `native_settings` / `native_settings_click`: Options > Mods lists "Baritone 1.0.0: ON" and
  "Sunset 0.8.1: ON", and each Settings page opens its own buttons.
- Drawing: Sunset's HUD (watermark, XYZ/FPS) and ESP box plus Baritone's path line and goal box all show in the
  same frame (~/hzmeteor/rBT1/sp/bt-goto-silent.png, bt-mine.png). Baritone's `camera_relative` takeover doesn't hide
  Sunset's drawing.
- Both splice a module into crates/horizon-client/src/game/agent.rs (sunset_goto, baritone_bridge): compiles fine.
- Hooks only Baritone has (sync_player_body, tick_avatar, tick_remote, ChatScreen suggestions, native_command) touch
  nothing Sunset hooks.

## Results (~/hzmeteor/rBT1..rBT7; flat arena, wall with a gap so the path has to go around it, 12 blocks to the goal)
| test | SP | Paper |
|---|---|---|
| Both load (#help + .t) | PASS | PASS |
| Auto Walk, Baritone idle | 4.32 b/s PASS | 4.32 b/s PASS |
| #goto alone | 82 ticks, 1 move/tick | 82 ticks (rBT1) |
| #goto + Kill Aura Silent aura + ESP + HUD | 85 ticks, pig killed, 1 move/tick | 85 ticks, pig killed, 1 move/tick |
| #goto + Kill Aura Snap | 83 ticks, pig killed | 74 ticks, pig killed |
| #goto + Auto Walk | arrives, then keeps walking (see below) | same |
| #mine gold_ore (5 ores), alone / + Kill Aura | 3 of 5 both ways (Baritone's own, see below) | 5 of 5 in ~290 ticks both ways |
| plain chat passthrough | PASS | PASS |
Most move packets in any tick, every run: 1. Teleports (server corrections): 0. Kicks: 0. Crashes: 0.

## Conflicts / findings
- **Auto Walk + `#goto`**: both press W. Baritone gets there faster (72 vs 82 ticks), but on arrival Auto Walk keeps
  walking (2.4 blocks past when it arrives, 4.4 more 20 ticks later). This is how they're expected to combine, not a
  bug. Sunset's Auto Walk shouldn't hold W while it hands the walking to Baritone (plan below).
- **Kill Aura Snap vs Baritone's look**: Snap turns the real view and Baritone steers by it, but on this course the path
  still finished as fast (74-83 vs 82 ticks). Silent aura (the default) leaves the local look alone. Not tested on
  long or jumpy paths, where a snap mid-parkour could throw a jump.
- **`#mine` in SP** leaves the 2 ores set into the 1-block floor ("Unable to find any path ... blacklisting") and then
  digs off after natural gold (ends near y=-56, 100+ blocks away). Same with Kill Aura on or off, and the same with the
  **Baritone-only build** (rBT4), so it isn't caused by Sunset. On Paper the same layout clears all 5. Report it
  upstream if anyone cares; not ours.

## Plan (report only, no code): Sunset driving Baritone through `#` commands
Arm's length only: Sunset passes `#` lines through the chat path, the same text a player types. No linking, no
shared types, no copied code. Sunset works the same without Baritone.
- **How**: call `game.handle_chat_input("#...")` from a Sunset module (it's `pub(super)` in `game::chat`, and
  Sunset's module sits under `game`), so the hook chain runs and Baritone's hook takes the line (it never reaches the
  server). Find out whether Baritone is there by calling `native_mods` and looking for id `baritone`
  (`native_on("baritone")` alone is true for an unknown id). Feedback: watch the player's position against the goal,
  or read the `[Baritone] Arrived at` / `Unable to find any path` chat lines. Never set `allowOnServers` behind the
  user's back: on a server, say in chat that Baritone needs `#set allowOnServers true`.
- **Auto Walk**: new mode "Baritone" (Meteor's Smart mode). On: `#thisway 1000` (a goal in the look direction), or
  `#goto x z` 1000 blocks ahead, sent again when the view turns more than ~30 degrees. Off: `#stop`. In this mode
  Sunset must not hold W.
- **Base Finder goto**: a clickable "goto" on each Base line / `.basefinder goto [n]` sends `#goto <x> <z>` (an XZ
  goal, since the height isn't known). Sunset's own `.goto` could take `--baritone` to hand the walking over when
  Baritone is there.
- **Excavator** (Meteor's): two corners picked with the crosshair, then `#sel pos1 x y z`, `#sel pos2 x y z`,
  `#sel cleararea`. This port lists `sel` (check PARITY.md for cleararea before relying on it). Vein Miner and
  Infinity Miner could map to `#mine <block>`, Highway Builder to `#tunnel`. Same rule as Excavator: check PARITY.md first.
- **Combat while it walks**: Kill Aura Silent aura is safe with Baritone steering. Snap/Normal turn the real view:
  pause them while a Baritone path runs, or warn about it.

Rerun: `cd ~/hzmeteor; BIN=~/scratch-baritone/bin-both VERIFY=../scratch-baritone/tools/bt.py ./runall.sh <dir> "" sp paper`.
Rebuild: horizon-mixins apply --workspace ~/hzmeteor/horizon-new --mods ~/scratch-baritone/mods-both --out
~/scratch-baritone/stage, then build.sh's cargo line in that stage with --target-dir ~/scratch-baritone/target.
