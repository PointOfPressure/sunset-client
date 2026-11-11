# Scaffold Normal: research and design (local, do not commit)

Status: research complete; implementation on branch `scaffold-normal` (worktree `C:\Users\jnowa\MeteorHorizon-scaffold`,
based on `origin/main` a168435), compiled and unit-tested against Horizon ba6344a, **not yet tested in game**.

This file cites third-party code by path and line and describes behaviour only. It contains no copied source. It lives
beside `docs/sourcecodes/` (untracked) and must stay out of commits and PRs, as must everything under `sourcecodes/`.

`docs/scaffold-research.md` (same folder) is the write-up of an earlier, abandoned attempt, kept in the old checkout's
uncommitted `scaffold_normal.rs`. Section 5.3 says what was kept from it and why the rest was dropped.

Evidence labels used for every anticheat statement:
**[source]** confirmed by inspected source · **[doc]** supported by documentation · **[inferred]** inferred from
implementation behaviour · **[test]** requires runtime testing.

---

## 1. The six inputs

| Input | What it is | Language / MC / loader | Complete? | Licence / provenance | Scaffold? |
|---|---|---|---|---|---|
| `client-a/` | A crystal / mace PvP client ("Client", ZKM-protected), recovered by a deobfuscation project. `README.md` and `docs/notes/*` describe the recovery: class and package names restored, method parameters still obfuscated, a lossy source tree that builds and boots. | Java, MC 1.21.11, Fabric (Yarn 1.21.11+build.4) | Mostly. Module bodies readable; helpers renamed by a script (`docs/classmap.tsv`). | None stated. A commercial client, decompiled. | **No.** `module/movement/FastBridge.java` is a plain eagle. |
| `client-b/` | "Core" 3.0.0-build.14 (`fabric.mod.json`), a full client with a large Scaffold. | Java, MC 1.21.11, Fabric | Decompiled. Yarn names present, method names synthesised (`updateStateForClientPlayerEntity3`…), anti-tamper noise (`ThreadLocalRandom.nextInt` no-ops). Logic intact. | None stated. | **Yes**: `app/ScaffoldModule.java` (2772 lines) + `BlockPlacementUtils`, `MovementInputOverride`, `RotationManager`, `CombatTickBudget`. |
| `client-c/` | "Client" 2.0.1, `All-Rights-Reserved`, DonutSMP-oriented (DonutRegions, SellChestFlow, TpaFlow…). | Java, MC 1.21.11, Fabric | **Damaged.** Strings AES-encrypted behind invokedynamic bootstraps; part of the code virtualised into a native VM (`app/qvm`, `libvm`, run-time decrypted). Names differ only by i/I/l case, so the Windows extraction overwrote files: 452 files on disk for 630 mapped classes. | All rights reserved. | **Named but unrecoverable.** `META-INF/client-module-ids.properties` maps `app.obf.liiIIiIiiiiIIiIi=Scaffold`, but that file holds another class (`liiIiiIiiiiIiiIi`); `Scaffold$MoveTask` likewise. |
| `client-d/` | A cheat hidden inside a repackaged Clumps mod (`fabric.mod.json` claims Jared's MIT "Clumps" 29.0.0.1; code under `net/caffeinemc/mods/lithium/lattice/...`). | Java (intermediary/Yarn mix), MC 1.21.11, Fabric | Obfuscated: word-salad class names; module names char-array encrypted; strings split into fragments. | Misattributed. Real provenance unknown. | **No.** Crystal/anchor PvP: `pipeline/socket/ProcessorModule.java` is an anchor macro, `RenderCacheView.java` a fast-place block filter. |
| `macro-app/` | "Macros" 1.1.1: an Electron app driving the OS keyboard and mouse (robotjs, keysender, AutoHotkey) for crystal and sword macros, with licensing, anti-debug, anti-VM and "stealth" code and a native `macro_secure.node`. | JavaScript + AHK, no Minecraft code | Complete as an app; nothing was executed. | None. | **No.** It has no world state, so it cannot plan placements. |
| `Rise-6.9.5-main.zip` | Rise 6.9.5, leaked and deobfuscated ("Rise 6.9.5 but... OpenSource"; "full offline, deobf, rename, fix, patch auth"). Read inside the zip, not extracted. | Java, MC **1.8.9** (MCP names: `C08PacketPlayerBlockPlacement`, `EntityPlayerSP`) | Readable; partially obfuscated fields (`aEg`, `pl`, `cqL`). | `src/main/resources/LICENSE`: "Copyright Alan 2021. None of this code to be reused without my written permission." | **Yes**: `module/impl/player/Scaffold.java` (93 KB) with `scaffold/{tower,sprint,downwards}/*`, `component/impl/player/RotationComponent.java`, `.../rotationcomponent/MovementFix.java`. |

Supporting sources (not in the six; used for anticheat ground truth and Horizon behaviour):

- **GrimAC** `GrimAnticheat/Grim` branch `2.0` at `41f6f6ec8f` (2026-11-02), GPL-3.0; fetched read-only via the GitHub API.
- **NoCheatPlus** `Updated-NoCheatPlus/NoCheatPlus` master (2026-10-28), GPL-3.0.
- **Horizon** at the pinned revision ba6344a: `native/source/ba6344a…` in the launcher folder.

Vulcan, Matrix, Intave, Spartan and Polar are closed source. Nothing below about them is from source.

---

## 2. Where the relevant code is

### client-b (Core 3.0.0): the most complete modern implementation

| Concern | File:lines | Behaviour |
|---|---|---|
| Tick pipeline | `ScaffoldModule.java:1574-1811` `onTick` | Simulates the player 7 ticks ahead (40 for telly) with a physics predictor, collects gap cells along the path, plans a placement, rotates, places, sets sprint/jump/sneak. |
| Gap cells | `:1522-1557` `updateStateForPredictedEntityPath2`; `BlockPlacementUtils.java:938-1000` `computeList2`; `:761-780` `testWorld5` | Walks predicted per-tick boxes; once a box loses support at the bridge Y, rasterises the line from the last supported centre with a **4-connected 2D DDA** and queues non-solid cells (max 10). |
| Support chain | `BlockPlacementUtils.java:711-755` `computeListForWorld` | BFS (≤3 steps, horizontal + down) from an unsupported gap cell back to existing blocks; queues the chain. |
| Placement per cell | `BlockPlacementUtils.java:100-185` `computeScaffoldPlacementForWorld` | For each non-air, non-interactive neighbour with an outline: candidate points on its face, then each point's rotation **ray-checked through vanilla placement resolution**; the cheapest valid one wins, else the cheapest unchecked one (for turning toward). |
| Candidate points | `:393-425` `computeVec3dForVec3d2` (current look ray ∩ face, clamped, inset 0.2); `:69-92` (stable ±0.1 jitter hashed per block); `:561-613` (level lines at the movement yaw + k·45° meeting the face); `:615-680` (3×3 inset grid, sorted by distance) | |
| Visibility | `:902-935` `testVec3d` | Eye strictly outside the face plane. |
| Cost | `:1014-1026` `computeDouble2/3` | Angular distance from the current look + 2 × the yaw's distance to the nearest 45° multiple from the movement-fix yaw. |
| Lazy look | `ScaffoldModule.java:2640-2708` | If the current look already resolves to a valid placement on a wanted cell (within 4° of the 45° grid), keep it. |
| Place gating | `:678-748`, `:893-905` | Place delay, places-per-20-ticks window, one action per tick (`CombatTickBudget`), "slot switch next tick", a "hesitation" humaniser. |
| Place action | `:912-932` | `interactionManager.interactBlock` (vanilla sequence + prediction); visible or hidden swing. |
| Deferred revalidation | `:1813-1843` | A scheduled place re-checks world, slot, rotation and raycast at execution, and defers if the cell meets this tick's swept player box. |
| Result tracking | `:1183-1209` `updateState10` | Placed cell + tick; still replaceable ≥2 ticks later counts a failure; 3 failures blacklist the cell for 40 ticks; forgotten after 20. |
| Rotation step | `:1996-2077` | Acceleration-capped smoothing (≤ max(2.6 × last step, 30 + (1−h)·60)); micro-jitter only if the jittered look still hits; a one-GCD nudge when the look would not change. |
| Movement fix | `MovementInputOverride.java:237-270` | 8-way key remap to the direction nearest the intended one at the sent yaw (ties → single key). |
| Diagonal rate | `ScaffoldModule.java:480-499` | Cells/s ≈ 7.2·(|sin θ|+|cos θ|); drops sprint when that exceeds the place-rate cap. |
| Slot choice | `:840-887` | Count + 50 if held + 25 if last used (hysteresis); restore on disable `:170-201`. |
| Sprint / jump / edge | `:2368-2476` | Ground-telly takeoff pacing, edge sneak with release delay, sprint Auto/Vanilla/Off. |
| Block filter | `BlockPlacementUtils.java:187-286`, `:288-317` | Excludes falling, TNT, magma, cactus, slime/honey/soul sand, interactive (`BlockWithEntity`, doors, trapdoors, gates, buttons, levers, beds, note block, cake, bell, composter, flower pot, anchor, repeater, comparator). |

### Rise 6.9.5 (MC 1.8.9)

| Concern | File:lines | Behaviour |
|---|---|---|
| Settings | `Scaffold.java:99-749` | Mode, Rotation Mode (Normal/Grim), Ray Cast (Off/Normal/Strict), Sprint, Tower, Same Y, Expand, Movement Correction, Safe Walk, Sneak pulses, Yaw Offset. |
| Normal re-aim | `Scaffold.java:198-300` `D(int)`; `:1451-1475` | Only re-aims when the mouse-over no longer hits the block and face. Re-aim: drop the eye to the block's middle height (±0.05 random), cast level rays at 45° steps, turn each validated hit into a real look from the actual eye, keep the one nearest the current look; else the face centre. |
| Backwards/Godbridge-ish | `:1386-1441` `findPlacementRotation` | Yaw on the 45s (camera, ±45, move direction), pitch searched, nearest to the current look. |
| Hit vector | `:1734-1770` `getHitVec` | The server look's real hit point when on the face, else a random point on it. |
| Place | `:350-390` `placeBlock` | Strict: the game's own right click. **"Grim" mode: a `C06PacketPlayerPosLook` with the target look, the click, then another C06 with the real look plus noise (three position packets in one tick).** |
| Rotation component | `component/impl/player/RotationComponent.java` | Target + speed (setting ×36 °/tick + random), applied in PreMotion (the move packet); target falls back to the camera every tick unless re-requested; release within 1°; sensitivity patch on release. |
| Movement fix | `.../rotationcomponent/MovementFix.java`, `RotationComponent` onMove/onStrafe/onJump | NORMAL: keys remapped + strafe/jump by sent yaw; TRADITIONAL: strafe/jump only; BACKWARDS_SPRINT: drop sprint >45° off. |
| Towers / sprints | `scaffold/tower/*`, `scaffold/sprint/*` | Server-specific (Watchdog, Vulcan, Matrix, Verus, NCP…) motion tricks. |

### client-a

`module/movement/FastBridge.java` (eagle: sneak when holding a block, pitch ≥ 70 and the 3 cells below are air);
`combat/RotationManager.java` (40%-of-remaining smoothing, no GCD, swaps the player's yaw/pitch around the player tick,
so physics uses the silent yaw); `combat/AimManager.java` (mouse-button press/release scheduler, not aim).

### Horizon ba6344a (what Sunset hooks)

| Fact | Where |
|---|---|
| Tick order: Sunset `pre_tick` (HEAD `Game::tick`) → `remote_handle_input` (slot sync, then clicks: `UseItemOn`) → entity tick (`controlled_simulation_tick_with_sprint_window`) → `remote_send_state` (PlayerInput, move packet with the `sent()` look) → `native_tick` → Sunset `post_tick`. | `game/remote.rs:4015-4131`, `:4850-4876`, `:5980-6040` |
| `PlayerInput` flags are built from `game.keys` **before** the simulation. | `game/remote.rs:4148-4166` |
| `pick()` returns `forced_hit` first; the game's use path places with its own sequence and predicts the block at once (`remote_can_place` → `predict_place` → `set_block`). | `game.rs:2323-2331`; `game/remote.rs:5255-5400`, `:5583-5629` |
| `remote_can_place` is a dry run of vanilla `place_block` (placement context: replaceable clicked block, entity collisions, `may_build`). | `game/remote.rs:5601-5629` |
| The slot is synced (`SetCarriedItem`) by the game once, before the keybind pass, when `selected` changed. | `game/remote.rs:5993-6004` |
| Horizon's own server (LAN host / dedicated) `UseItemOn`: ack the sequence; require cursor within 1.0000001 of the block centre and the block within reach. No raycast. | `game/lan_actions.rs:1256-1300` |

---

## 3. Anticheat findings

### GrimAC 2.0 (41f6f6ec8f)

| Check | What it does | Label | Consequence for Normal |
|---|---|---|---|
| `RotationPlace` | Post-flying: a ray from any possible eye height along (yaw, pitch), (lastYaw, pitch), and for 1.9+ (lastYaw, lastPitch) must meet the clicked block's full cube within `BLOCK_INTERACTION_RANGE`. Pre-flying only after a recent flag. | [source] | The look the server had **or** the look sent right after the place may justify the click. Strict satisfies both by holding the look on place ticks. |
| `PositionPlace` | Eye positions must be outside the clicked face (against the block's hitbox). | [source] | Only faces the eye is outside of are planned (`Side::faces`). |
| `FarPlace` | Eye to clicked block box ≤ interaction range + ~0.04. | [source] | Default Max placement distance 4.5; capped by the game's reach. |
| `MultiPlace` (experimental) | Two different places in one tick. | [source] | ≤1 click a tick. |
| `DuplicateRotPlace` (experimental) | On a place tick with yaw delta >2°, the same delta as the last place tick (±0.0001). | [source] | Strict: no turn on place ticks. Speeds re-rolled every tick. |
| `AirLiquidPlace` | Clicked block air/liquid in Grim's world: cancel at VL 0. | [source] | Supports must be non-replaceable. Air place is offered only with Ray cast Off / Rotate off. |
| `FabricatedPlace` / `InvalidPlaceCursor` / `InvalidPlaceFace` | Cursor outside [0,1] (±1 float ulp), non-finite, face ∉ 0..5. | [source] | The cursor is the ray's real hit on the block (or an inset point with Off). |
| `Post` | Place/use/slot/dig/entity-action/… packets after a flying packet and before the next transaction. | [source] | **Main's old Normal placed in `post_tick`, after the move packet: exposed.** Everything now happens in `pre_tick`. |
| `AimDuplicateLook` | A rotation packet identical to the previous one. | [source] | Horizon sends a look only when it changed; holding costs nothing. |
| `AimModulo360` | Yaw delta >320° after one <30°. | [source] | Look kept continuous; the turn back unwinds the real yaw difference (a release at camera ±360 would be a one-packet full turn). |
| `BadPacketsA` | The same slot sent twice in a row. | [source] | The game syncs the slot only on change. |
| Prediction (`PredictionEngine.loopVectors`) | For 1.21.2+ clients the input comes **from the PlayerInput packet** (`knownInput`); sprinting forces forward; motion is predicted at the sent yaw. | [source] | Silent correction must change `game.keys` before the tick so PlayerInput and motion agree; correction Off desyncs. |

### NoCheatPlus (Updated-NCP master)

`checks/blockplace/Scaffold.java` with `DefaultConfig` values [source]:
**Angle** (look vs placed face >90°), **Time** (average interval < `scaffold.time.avg` = 2 ticks while pitch >70° and no
sneak in the last 3 ticks), **Sprint** (sprint within 8 ticks of a place at pitch >70°), **Rotate** (yaw change >90° in
the tick after a place), **ToolSwitch** (slot changed in the tick after a place). FastPlace: 15 limit, 6 per 10 ticks
short-term [source].
Consequences: on NCP use Delay ≥ 1, Sprint Off; Ledge sneak resets the Time window; Swap back waits 2 ticks after a
place [inferred from source]. Whether NCP's Direction/Against use the pre-place look on 26.3 via ViaVersion: [test].

### Others

Intave, Vulcan, Matrix, Spartan, Polar: no source. The flags listed in `anticheat-sweep.md` (private notes) were
observed on the old Normal. Whether the new pipeline changes them: [test].

---

## 4. Comparison matrix (best design per subsystem)

| Component | Best source | Why | Weakness | Fit with Horizon | Sunset before | Integration |
|---|---|---|---|---|---|---|
| Target selection | client-b `updateStateForPredictedEntityPath2` + `computeList2` | Path-ordered, 4-connected cells: diagonals become a staircase whose every cell has a face to go against. | Needs a full physics predictor (7–40 ticks) for its timing. | The predictor isn't reachable without cloning `MotionWorld`; a 2-tick velocity line + Expand covers what Normal needs. | Floor of next position; whole-cube fallback. | `line_cells` from position to position+2·velocity (+Expand). |
| Placement geometry | client-b `computeScaffoldPlacementForWorld` | Candidate points (look-ray projection, grid) per visible face, each ray-checked. | Many raycasts; cost has no cap. | `game.clip_ray` is the game's own outline ray. | Face centre, no visibility, side picked by dot product. | `choices` + `plan`: cost-sorted, ≤24 rays/tick. |
| Placement resolution | client-b (`InteractionActions`) | Validate a click by what vanilla would actually place. | | `remote_can_place` is exactly that, dry. | Not checked. | `places_into` in `remote_seq.rs`, before every click. |
| Rotations | Rise `RotationComponent` (speed range, move-packet delivery, release) + client-b cost term + Sunset's GCD | Simple, predictable, configurable. | Rise's release snaps within 1° (fine); client-b's humaniser is complex. | `sent()` hook already exists; `rotation::step` and `degrees_per_count` exist. | Snap to face centre via `turn_then`, back each block. | Silent slot in `sent()`, `step` + `whole_counts`. |
| Raycasting | client-b ray check of each candidate; Rise Strict mouse-over | Validates the exact click a look produces. | | `clip_ray` + `places_into`. | Sampled line test (optional). | `placing_ray` / `plan`. |
| Scheduling | Vanilla timing as Grim models it (place, then the move packet with the look) | Everything before the move packet, ≤1 click a tick. | | `pre_tick` + `press_use` + `forced_hit` go through the game's input pass. | `post_tick` after the move (Grim Post). | Ray cast Strict / Normal / Off. |
| Networking state | Horizon itself (sequence, prediction, ack rollback) | No duplicate protocol logic. | | Native. | Manual `UseItemOn`, no prediction, extra `Punch`, slot in and back each block. | Game's own click. |
| Recovery | client-b `updateState10` | Distinguishes "sent" from "kept". | | Prediction makes the client side immediate; rollbacks show as the cell turning air again. | Duplicate cooldown timer. | `watch_results` + `fail`/`cooled`. |
| Movement | Rise movement fix + Grim's knownInput finding | Correction that matches what the server simulates. | Rise remaps inside the input event; on 26.3 the input packet must agree. | `screen_keys` hook swaps the physics yaw; keys remapped in `pre_tick`. | None (camera yaw while another look is sent). | Movement correction Silent/Strict/Off; Ledge sneak. |
| Inventory | client-b hysteresis + Sunset's existing `scaffold_slot` (biggest stack, kept) | Already good; no churn. | | | Slot in and back every block (NCP ToolSwitch, 2 packets/block). | Hold while bridging, Swap back ≥2 ticks after the last place. |
| Performance | client-b's early pruning | | client-b allocates freely. | Fixed arrays, reused Vec, no per-tick allocation after warm-up. | Cube search each tick when unsupported. | Bounded rays, bounded search. |

---

## 5. Sunset before this change

### 5.1 Main's Normal (runtime.rs `scaffold`/`scaffold_place`, removed)

1. Ran in `post_tick` (module_tick), **after** the move packet; its clicks went out after the flying packet: Grim `Post` [source].
2. Placed with hand-built `UseItemOn` (`util::place_block`): no client prediction, so the cell stayed air until the
   server's update and was retried (hence "Duplicate cooldown"); an extra `Punch` packet each place.
3. Swapped to the block slot and back **every block**: two `SetCarriedItem` per place; NCP ToolSwitch [source].
4. Cursor always the face centre; face chosen by dot product, with no visibility test (PositionPlace) and no reach test
   against the box (FarPlace).
5. Rotate with Rotation speed < 180 turned the **camera**; at 180 it used `turn_then` (a snap there and back).
6. Without a side, searched a whole cube of radius Closest block range each time.
7. Air place + Radius + Blocks per tick placed several blocks a tick (Grim MultiPlace [source]).
8. Default Max placement distance 6 (> vanilla 4.5; FarPlace).

### 5.2 Kept unchanged

Legit and Godbridge (`scaffold_bridge.rs`), Kill Aura, the block filter, the block counter, Render, Fast tower,
Safe walk, `scaffold_slot` and its stack hysteresis, Disable with Kill Aura, the combat-control gate.

### 5.3 The earlier uncommitted attempt (old checkout)

Kept as ideas: placing through `press_use` + `forced_hit`; a Scaffold slot in `sent()` ahead of Kill Aura's; the
`screen_keys` hook for physics yaw. Dropped:

- **Silent correction remapped keys only inside the simulation.** PlayerInput then said "W" while the move was "S" at
  the sent yaw: a guaranteed Grim Simulation mismatch for 1.21.2+ clients [source].
- Its release took the short way to the camera's yaw, which can end at camera ±360: AimModulo360.
- Rise-style Ray cast "Normal" (block only, any face) can click a face other than the planned one.
- Pitch scans of 87 raycasts per yaw: replaced by the closed-form `Side::pitches`.
- No result tracking, no visibility/reach model of the face, no diagonal handling.
- Its default Movement correction was Off.

---

## 6. Architecture (implemented)

`mixins/sunset/scaffold_normal.rs`, one module, called from `runtime::pre_tick` after Legit/Godbridge and from
`runtime::post_tick` first (it gives back keys before Snap Tap gives back its own).

```text
pre_tick
  gates (slot, flying, Only on click, combat control, Kill Aura / Blink pause)  → else release()
  result tracking   watch_results: predicted blocks gone again → fail(cell); 2 fails → 40-tick wait
  target            row (Same Y / under next position / Down), line_cells → Cells (≤12, path order)
  validation        can_place (incl. entities), not cooled
  click (Strict)    placing_ray(eye, look the server has) → places_into → cell ∈ Cells → click()
  planning          choices(): per cell, per visible reachable support face, cost-sorted looks
                    plan(): first cell with a ray-checked look; bare first cell → bounded search
  rotation          holding? keep : mode target (Normal / Backwards / Snap / Telly); turn() in mouse counts;
                    idle → turn_back()
  click (Normal/Off) this tick's look / planned side / Air place
  apply             sent() slot or camera (Visible); movement correction; Sprint; Ledge sneak; auto jump
post_tick
  forced hit cleared; click result: predicted → watch, refused → fail; keys / right / jump given back
```

State is explicit (`struct Normal`), fixed-size where possible (`Cells`, `watch[8]`, `failed[8]`, `recent[20]`), one
reused `Vec<Choice>`. No allocation per tick after warm-up. No stale plans: everything is recomputed each tick from the
world. Cleanup: `release()` while off or paused (turns back, slot back, sneak off), `stop()` on Panic / no world,
`correction()` on a server position packet.

---

## 7. Algorithms selected

1. **Path cells**: a 4-connected DDA from the current position to position + 2·velocity, plus Expand along the walk
   (client-b's raster, without its predictor). Each new cell shares a face with the one before.
2. **Support faces**: the 4 horizontals + below; outline union box; not replaceable, not a block entity, not a
   use-on-click class (`USED`); eye outside the face plane; box distance ≤ reach.
3. **Look candidates**: the current look's ray projected onto the inset face (least turn), a 3×3 inset grid; for
   Backwards/Telly, yaws behind the walk on the 45s with the **closed-form pitch window** for that yaw.
4. **Cost**: angle from the current look, + 2 × distance to the 8 ways around the walk under Silent correction
   (client-b's insight: the key remap is exact on the 45s).
5. **Check**: `clip_ray` from the eye of the tick the click will happen (next tick's for Strict) must hit that block
   and face; the click is then resolved by `places_into` and must fill a wanted cell.
6. **Hold**: if the current look already clicks a wanted cell, it stays (Rise Normal / client-b lazy).
7. **Step**: `rotation::step("Custom")` at a speed drawn each tick in [Min, Max], quantised by `whole_counts` to whole
   mouse counts (Sunset's `degrees_per_count`).
8. **Release**: `turn_back` unwinds the actual yaw difference, lets go within 1°.
9. **Correction**: Silent remaps WASD in `game.keys` (so PlayerInput agrees) and runs the tick's physics at the sent
   yaw; Strict only the yaw; vanilla's own sprint rules then apply.
10. **Recovery**: the game predicts the block at once; a cell that turns air again within 20 ticks is "taken back";
    two failures put it on a 40-tick wait.

## 8. Rejected

| Approach | Source | Why rejected |
|---|---|---|
| Extra position packets around the click ("Grim" rotation mode) | Rise `Scaffold.java:350-390` | Three move packets in a tick: Horizon kicks for a second one (AGENTS.md); Grim timer/BadPackets. |
| Full physics predictor for planning | client-b | Would need a cloned `MotionWorld` per tick; little gain over 2-tick velocity for placement. Timing-sensitive telly takeoff pacing is what really needs it (deferred). |
| Humanisation (hesitation, curved paths, micro-jitter) | client-b | Its value against current checks is unproven [test]; it adds failure modes (missed places). Random speed range + mouse-count quantisation kept. |
| Motion towers (Pulldown, Timer, Watchdog, Vulcan…) | Rise `scaffold/tower/*` | Server-specific motion edits that prediction anticheats simulate; out of scope. The existing Fast tower is kept as it was. |
| Multi-place per tick (Radius, Blocks per tick) | main | One coherent scheduler; Grim MultiPlace; NCP FastPlace. |
| Server-branded presets ("Grim", "Watchdog") | Rise | No evidence such a preset passes; the existing movement presets are kept. |
| Camera-moving rotation as default | main | User requirement: no unintended camera movement. Available as Look → Visible. |
| Snap rotation as default | Rise | Two large turns per block; NCP Rotate (>90° in the tick after a place) [source]. Offered, not default. |
| Sneak pulses ("Sneak every x blocks") | Rise | Cosmetic; Ledge sneak covers the real need (not falling while the look is late). |

## 9. Licensing

- Rise: "None of this code to be reused without my written permission." Nothing copied; only its observed behaviour
  (speed range, release, re-aim when the mouse-over fails) informed Sunset's own code.
- client-b/-c/-d, client-a: no licence; decompiled proprietary software. Behaviour only.
- GrimAC, NCP: GPL-3.0. Read to understand what servers check; no code used.
- Nothing ported from MIT sources in this change (vape-rs's managed rotation and GCD were already in Sunset), so no
  new LICENSE notice or README credit is needed.
- Keep `docs/sourcecodes/` and this file out of every commit and PR.

## 10. Uncertainties ([test] unless stated)

- Whether ACTest's anticheat flags any of: Silent correction at Rotation speed 45–75, Strict holding, Ledge sneak
  toggles, Snap/Telly turns, the 2-tick swap back.
- Diagonal bridging at sprint speed: the 2-tick lookahead may be short at full sprint (≈0.28 b/t). Expand 1 helps.
- Whether Horizon's sprint double-tap window reacts to the W↔S remap flipping between ticks.
- Air place through `forced_hit` on an air block: the use path accepts it in source; the server side is
  server-specific.
- NCP's place checks seen through ViaVersion with 26.3 clients.
- Another module's `turn_then` look arriving while Normal holds a silent look (both rewrite the move packet's look).
