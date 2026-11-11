# Scaffold and rotations: Rise, LiquidBounce, vape-rs and Sunset

Goal: a Scaffold mode for servers with weaker anticheats (NCP, Spartan, Vulcan/Matrix-class, Horizon's own), much faster
than Legit/Godbridge, with rotations in the style of Rise. Grim stays on Legit/Godbridge.

Nothing here claims a bypass. Every behaviour below needs the test matrix (`docs/test-matrix.md`) before it is called
working.

## 0. Sources and what could be read

| Source | What it gave | Licence / rule |
|---|---|---|
| Rise 6 (`Cheats/RiseClient`) | **No module code** in that copy (see 0.1 for the 6.9.5 source). `Standalone.jar` (98 MB) holds 118 classes, all a loader (`rip/vantage/{security,network,runtime}`) that fetches/decrypts the client at run time. Running it or its decryption is forbidden, so the code is out of reach. What can be read is `save/Rise/configs/*.json` (4 saved configs, plain JSON): the full setting surface of Scaffold, Kill Aura, Sprint, Keep Sprint, Legit Scaffold, with chosen values. | Untrusted, static only, copy nothing. Option lists beyond the chosen values are from public knowledge of Rise and marked *(public)*. |
| LiquidBounce Nextgen | Real source of the most complete open scaffold + rotation manager (ModuleScaffold, techniques, towers, RotationManager, MovementCorrection, FaceTargetPositionFactory, PointTracker, KillAura). Read on GitHub, behaviour only. | GPL-3: behaviour only, write fresh. |
| vape-rs (`~/vape-rs`, d6e48b1) | Legit, GodBridge, TellyBridge, managed rotation (whole mouse counts, PID). Sunset already ports Legit/GodBridge and the managed rotation. | MIT: may be ported, credit kept. |
| `docs/notes/anticheat-sweep.md` (upstream) | What each anticheat flags on Sunset's current Scaffold. | Ours. |

## 0.1 Rise 6.9.5 source (added later)

A Rise 6.9.5 source tree turned up later (`docs/Rise-6.9.5-main.zip`, untracked, never to be committed). It was read as
text inside the zip, nothing extracted or run, and Sunset's Normal mode (`mixins/sunset/scaffold_normal.rs`) follows
its behaviour, written fresh. What it does:

- **RotationComponent**: a module asks for a look and a speed each tick (speed setting x 36 degrees a tick, plus up to
  1). The step limits the straight-line distance in yaw and pitch together, then snaps it to whole mouse counts of
  the sensitivity (the Minecraft `(s*0.6+0.2)^3*8*0.15` step) with tiny noise. The look goes in the tick's move
  packet; the camera is untouched. The target resets to the camera at the end of every tick, so once a module stops
  asking the look turns back at the same speed, and lets go within 1 degree.
- **Movement fix**: `Rise` turns the move keys to the one of the eight that, at the sent yaw, walks nearest where you
  pressed, and moves (strafe and jump) by the sent yaw; `Traditional` only moves by the sent yaw; `Backwards Sprint`
  drops sprint when the sent yaw is over 45 degrees from the walk.
- **Scaffold, block**: under you (Same Y: the row you started on), Expand blocks on along the walk; if that is not
  air, the nearest air spot with a solid side within 5 blocks. Of its sides, the ones around first, sorted by how
  near their way is to the look, then the one below.
- **Scaffold, when**: once the block under you has been air for more than Place Delay ticks, and the crosshair of the
  sent look is on the block (Ray Cast Normal) or on the block and side (Strict: then it is the game's own right click).
- **Rotation "Normal"**: kept while the sent look's ray is still on the side; otherwise, for the eight ways around
  the camera yaw, the point where a level line at the block's middle height (plus or minus 0.05) meets the side, and
  of the looks at those points that really hit it, the nearest to the look now; else the side's centre.
- **Snap**: as Normal, but the yaw faces the way you walk whenever no block is wanted or the look is already on it.
- **Telly**: jumps on the ground; looks forward until 3 ticks in the air, then turns to the side (yaw on the 45s,
  pitch searched from 82 by 1.5s), places, and faces forward again past 7 (10 with Same Y) ticks in the air.
- **Godbridge**: yaw on the 90s minus 180 plus or minus 45, pitch 76.4, keys swapped, sneaks every 10 ticks.
- **Sprint**: Normal (kept), Legit (off when the sent yaw is over 90 from the camera), Bypass/Disabled (off), and
  Watchdog/Vulcan/Matrix/Verus variants tied to those servers.
- **Towers**: Vanilla (0.42 up with a block below), Normal (0.42 on the ground, and -0.0784 once the block under is
  placed, snapping you onto it), NCP (0.42 from a whole block height, else set onto it), Legit (plain jumps).

## 1. Rise's Scaffold, from its saved settings

Settings found (values from `hypixel`, `blocksmc`, `latest`, `test`):

| Setting | Values seen | What it does *(public + inferred from name)* |
|---|---|---|
| Mode | Normal, Eagle | Placement technique. Others *(public)*: Snap, Telly, Watchdog, Breezily, Vulcan (has its own "Speed in Vulcan Mode" 1.3). |
| Rotation Mode | Normal | Normal = smoothed look at the placement point. *(public)*: Simple/Backwards (move yaw + 180, pitch searched), Snap (turn only on the place tick), Godbridge (yaw ±45), Off. |
| Rotation Speed | 1..1, 1..3, 10..10 (range pair) | Random speed between min and max each tick. 10 is effectively instant. |
| Ray Cast | Strict, Off | Strict: place only if the *sent* look's ray hits that block **and face**. Off: place with the computed hit point regardless of look. |
| Movement Correction | true / false | Physics uses the sent yaw, inputs remapped so you still go where you press. Off = camera yaw moves you while another yaw is sent. |
| Sprint | Disabled, Normal, Watchdog Jump | Normal keeps sprint while looking back (omni-sprint). Watchdog Jump: sprint-jump bridging. *(public)*: Legit, Bypass (stop-sprint before place). |
| Same Y | Off, Auto Jump | Keep placing at the start height while jumping; Auto Jump jumps for you: the classic speed bridge. |
| Tower | Disabled, Normal, Legit | Normal: motion tower (0.42 kicks snapped to block height). Legit: real jumps, place under at the top. |
| Downwards (Press Sneak) | Off | Sneak to build down. |
| Place Delay | 0..0, 1..2 ticks | Random gap between places. |
| Expand | 0 | Place up to N blocks ahead along the look. |
| Timer | 1.0 | Game speed while scaffolding. |
| Speed Multiplier | 1.0 | Horizontal speed scaling while bridging. |
| Safe Walk | true/false | Never walk off. |
| Sneak, Start/Stop Sneaking, Sneak every x blocks, Sneaking Speed | 1..2 ticks, every 1 block, 0.2/0.45 | Eagle-style sneak pulses mixed in, to look legit to placement-timing checks. |
| Yaw Offset | "0" | Constant offset added to the scaffold yaw. |
| Ignore Speed Effect | false | Don't widen timings under Speed. |
| Render, Advanced, Up Side Down | | Visual / UI. |

Rise's Kill Aura settings show the same rotation core: `Rotation speed` range, `Movement correction` (Off / "Rise"),
`Ray cast`, `Keep sprint`, `Rotation Mode: Legit/Normal`, and `Attack whilst Scaffolding` (the two share one rotation
owner: Scaffold wins unless that is on).

What this tells us about the design: one **rotation component** shared by all modules (target + speed range + movement
correction), placement gated on a **ray cast of the sent look**, and a scaffold that is a set of orthogonal options
(technique × rotation × sprint × same-Y × tower) rather than fixed modes.

## 2. The rotation pipeline every advanced client has

Same shape in Rise (from settings), LiquidBounce (from source) and vape-rs (from source):

1. **Request.** A module asks for a target look with a priority and an expiry. Highest priority wins; a request
   expires after N ticks (LiquidBounce: Scaffold priority above Kill Aura).
2. **Step, at tick start**, from the last *sent* look toward the target:
   - Linear: per-tick cap, random within [min, max] (LiquidBounce default 180/180 = instant).
   - Smooth/sigmoid/acceleration curves: speed depends on how far is left (slower near the target).
   - vape-rs: 50 sub-steps a tick in **whole mouse counts** of the sensitivity (GCD fix), PID with jitter.
3. **Send in the tick's own move packet.** The camera stays where the player put it (silent). Packet-only turns
   outside the move packet are what Grim's AimDuplicateLook and Post catch.
4. **Act next tick.** The action (place / attack) happens at the next tick start, gated on a ray cast from the look
   the server already has. This is the order every check expects: look arrives, then the interaction.
5. **Release.** When no request is left, step back toward the camera and let go only within a threshold
   (LiquidBounce: Ticks until reset 5, reset threshold 2°). A snap back is a 100°+ jump in one packet.
6. **Movement correction** (the part most clients get wrong):
   - *Off*: physics moves you with the camera yaw while the server is told another yaw. Any prediction anticheat
     (Grim, Intave, Polar, Vulcan's prediction) sees motion that does not match the sent yaw; omni-sprint is visible.
   - *Strict*: physics uses the sent yaw with your keys as they are, so W walks where the rotation faces.
   - *Silent*: physics uses the sent yaw, and the keys are remapped to the one of 8 directions (relative to the sent
     yaw) closest to where you pressed. You go roughly where you wanted, and the motion matches the yaw the server has.
     Sprint only stays legal if the remapped input is forward-ish (within 45°).
   - Both modes also apply to the sprint-jump boost, which uses yaw too.

### Kill Aura rotations (reference only, Sunset's Kill Aura is not changed)

- **Point on the hitbox**: predicted box (target velocity), slightly inflated; visible points from the eye; nearest to
  the current look (least change), with exemptions and a Gaussian/random offset. Rise: `Rotation Mode Legit/Normal`.
- **Timing**: Normal (aim continuously), Snap (aim only on the click tick), On tick (aim the tick before).
- **Gate**: attack only if the sent look's ray hits the entity within range (`Ray cast`).
- **Lazy**: keep the current look while it still hits.

## 3. Scaffold anatomy (LiquidBounce source, Rise settings)

### 3.1 Which block

- Position: predicted next-tick position (velocity added), feet Y − 1.
- **Same Y**: Y captured on each ground tick and kept while airborne. Variants: always, only when jump key is up,
  only while falling (vy < 0.2), Hypixel (one up every second jump).
- **Expand**: walk N blocks along the look and take the first placeable one.
- **Down**: sneak held, target Y − 2.
- Priority: tower > down > same Y > Y − 1.

### 3.2 Neighbour and face

- Candidates: the target and its neighbours, sorted by distance to the planned line (the line through the last two
  placed blocks when it agrees with the input within 60°), else to the predicted position.
- For each candidate, each face whose normal points toward the eye (Expand also allows away-facing ones).
- Pick the face whose normal points most directly at the eye.
- **Vanilla placement context must resolve to the target block**, or the plan is dropped. Placing "against" a face
  the click could not really produce is what NCP's *Against* and Intave's *placementanalysis* check.

### 3.3 Hit point on the face (the "aim mode")

Faces are trimmed 15% per side (never hit an edge). Then:

| Mode | Point |
|---|---|
| Center | face centre (exact 0.5 cursor: easy to fingerprint) |
| Random | random point on the trimmed face |
| Nearest rotation | the point closest to the current look's ray: least turning |
| **Stabilized** (LB default) | Nearest rotation, restricted to the part of the face on your projected path: the look barely moves from block to block |
| Reverse / Diagonal / Angle yaw | the point where the face meets the plane at move yaw ± 180 / 75 / 45 |
| Edge point | the far vertex while moving |

### 3.4 The rotation target (what makes "Rise rotations")

- **Normal**: look at the hit point, smoothed at the speed range. Every block the look moves a little; with
  Stabilized points it is nearly still.
- **Backwards** (Rise's characteristic look while sprint-bridging): yaw = move yaw + 180 (rounded to 45 in LB's
  Breezily/GodBridge), **pitch searched** so the ray hits the face. Closed form, no scan needed: with yaw fixed, the
  ray's horizontal direction is fixed, so intersect it with the face plane; the pitch is `atan(drop / horizontal
  distance)` and is valid if the hit lands inside the trimmed face. Typical results: straight ≈ 80°, diagonal ≈ 75.6°.
- **Godbridge**: move yaw ± 45, pitch ≈ 75.7 (LB); the side is picked from where you stand and which block is next.
- **Snap**: no look change except on the place tick (one turn there, one back): fast, but two big deltas a block.
- **Telly**: on the ground the look faces forward (sprint is legal), you sprint-jump, and after *Straight* air ticks
  (LB 0..5, default 0) it turns back to the face and places mid-air. Then it lets the look return (reset) or rounds
  yaw to 45 and keeps pitch ≥ 45 (reverse). Jump delay re-rolled each jump.

### 3.5 Placing

- Timing: Normal (rotate, place when the ray agrees), On tick (rotate and place the same tick), On tick snap.
- Ray check (Rise's `Ray Cast`): Strict = same block and same face from the **sent** look; Normal = the block;
  Off = trust the computed point.
- Delay range, Simulate placement attempts (extra clicks when a place fails, capped CPS), swing visible or hidden.
- Sprint control: keep / force off / off on the place tick / off on ground.
- Ledge: if the look needs more ticks to arrive than you have before falling off, sneak (or jump, or stop) for that
  long. This is what lets fast scaffolds go full speed without missing blocks.
- Stabilize movement: input nudged back onto the line when off it by more than 0.2 blocks.

### 3.6 Towers (jump held)

| Tower | Behaviour (LB numbers) |
|---|---|
| Motion | each time you are 0.78 above the jump-off, vy = 0.42 (x/z × Slow 1.0) |
| Pulldown | after the jump, once vy < 0.1 and a block is below, vy = −1.0 (snaps you down onto it) |
| Karhu | timer 5 in the air, then pulldown at vy < 0.06 |
| Vulcan | vy 0.7 / 0.42 (moving) / 0.6 alternating ticks |
| Hypixel | 1.8 only, standing still |
| Legit (Rise) | plain jumps, place under you at the top |

Sunset's existing Fast tower (vy = Tower speed, snap to block height at the ceiling) is a Motion-style tower.

## 4. What the weaker anticheats look at

From the sweep on Sunset's current Normal Scaffold:

| Anticheat | Flags on Normal Scaffold | Check it means |
|---|---|---|
| Intave | interactionraytrace, placementanalysis, physics | the look in the move packet does not reach the clicked face; cursor/face/timing pattern; motion vs yaw |
| Spartan | gravity-simulation, fast-place, scaffold_direction, tower, scaffold_analysis | places faster than possible; placing on a face you are not facing; tower vy |
| NCP | Scaffold, Against, SurvivalFly (kick) | look vs face angle, place rate, impossible support face |
| Grim | Post, PositionPlace, AimDuplicateLook, Simulation | packets after the move; place not from the look; repeated look; motion |
| Horizon's own | prediction, timer | motion only |

Legit and Godbridge pass all of them except Spartan's scaffold_analysis (x2–x6) and NCP SurvivalFly x1 on Godbridge.

What this means for the design:

1. **Look and face must agree** (Intave raytrace, NCP Scaffold, Spartan direction). The fix is ray-gated placing from
   the sent look, not a different snap.
2. **The look must be in the move packet, the place after it, at tick start** (Grim Post; also the cleanest order for
   everyone else).
3. **No exact face centres** (placement analysis): trimmed, varied points, Stabilized or Nearest.
4. **Place rate** ≤ 1 block a tick, and on Spartan around 1 per 2 ticks with a range (fast-place).
5. **Motion must match the sent yaw** if the anticheat predicts (Intave, Vulcan): movement correction Silent/Strict.
   NCP/Spartan only check speed limits: omni-sprint at sprint speed is the main risk there (SurvivalFly).
6. **Towers**: Motion tower at 0.42/0.78 is close to a real jump; pulldown and timer towers are the ones simulation
   checks catch.

## 5. Why Sunset's Normal mode flags today

- It snaps the look to the face centre per block (`turned_to` → `turn_then`), then snaps back: two large deltas a
  block, a repeated look when nothing changes (AimDuplicateLook), and a face-centre cursor every time.
- The place runs from `post_tick`, after the move packet (Grim Post) and is not gated on the sent look's ray
  (Intave interactionraytrace, NCP Scaffold).
- `place_block` swaps the slot in and back each block (two SetCarriedItem a place) and sends an extra `Punch`.
- When the block below cannot be placed it searches a whole cube (Closest block range 4) and may place against any
  face in reach, including faces turned away from you (NCP Against, Spartan direction).
- Fast tower sets vy directly and claims ground at the ceiling (Spartan tower, gravity-simulation).

## 6. Proposal: a "Fast" mode alongside Normal, Legit and Godbridge

Kept: Normal, Legit, Godbridge, Kill Aura as they are. New mode `Fast` in `mixins/sunset/scaffold_fast.rs` (new file),
built on what Sunset already has.

### 6.1 Pieces Sunset already has

| Need | Sunset hook / helper |
|---|---|
| Silent look in the tick's own move packet | `sent_look` MODIFY_ARG on `position_sender.update` → `sunset_rotation::sent` (Kill Aura Silent aura uses it) |
| Physics yaw and keys for movement correction | `screen_keys` HEAD hook on `controlled_simulation_tick_with_sprint_window` (has `world.player.yaw` and the `Keys`; the jump boost reads the same yaw) |
| Tick-start actions with the game's sequence | `pre_tick` (Legit/Godbridge already place there) |
| GCD | `sunset_rotation::degrees_per_count` |
| What the server has | `combat_wire::sent_look`, `sent_sprinting` |

### 6.2 Rotation component (`scaffold_fast.rs`, shared shape for later modules)

- State: the look the server has, the target, ticks since the last request.
- Each tick before the move: step toward the target at a random speed in **Rotation speed** (min, max degrees a tick),
  snap the step to whole mouse counts, write it to `sent`. Never repeat the exact previous look while a target is
  held (add one count of jitter instead).
- Released: keep stepping toward the camera; let go once within 2° (**Reset ticks** cap).
- Movement correction **Off / Strict / Silent** in the `screen_keys` hook: set `player.yaw` to the sent yaw for the
  tick and remap keys (Silent) to the 8-way input nearest your intent, then put the camera yaw back. Sprint dropped
  when the remapped input is not forward.

### 6.3 Scaffold logic

- Target: predicted position, **Same Y** (Off / On / Auto jump), **Expand** 0–4, Down on sneak.
- Neighbour/face: LB-style candidates, faces toward the eye only, vanilla-context check, nearest to the line.
- Hit point: trimmed face, **Aim point** Stabilized (default) / Nearest / Random.
- **Rotation**: Normal (smoothed to the point) / Backwards (move yaw + 180, closed-form pitch) / Godbridge (±45) /
  Snap / Telly (forward on ground, back after Telly ticks in the air).
- Place in `pre_tick` with **Ray cast** Strict / Normal / Off from the look the server has; Strict by default.
  **Place delay** range in ticks, one block a tick at most, no slot round trip (hold the block slot while bridging,
  give it back once when the mode stops: one SetCarriedItem each way), one swing.
- **Sprint**: Keep / Legit (only when the sent yaw is within 45° of the motion) / Off / Off on place.
- **Tower**: Off / Motion (0.42 at 0.78) / Legit; the old Fast tower stays in Normal.
- **Ledge**: sneak for the ticks the look still needs before it reaches the face.
- Safe walk, Eagle pulses (every N blocks), Swing visible.

Defaults aim at "fast but plausible": Backwards rotation, speed 40..60, Silent movement correction, Ray cast Strict,
delay 0..0, Sprint Legit, Same Y Off, Tower Motion.

### 6.4 Tests

- `verify.py`: `@test("Scaffold Fast")` (straight and diagonal bridge, Same Y auto jump, tower), asserting blocks
  placed, no fall, look in move packets only, place after the move that carries its look.
- Unit tests: closed-form pitch hits the face; GCD steps; Silent remap keeps you within 22.5° of the intended
  direction; reset reaches the camera without a jump over the threshold.
- Matrix: single player, Paper with NCP, Spartan, Intave (and Grim, to record, not to target).

### 6.5 Open risks

- Silent movement correction on Horizon: the hook sets the yaw for one tick; anything else that reads
  `player.yaw` in the same tick (sprint checks, swimming) sees the sent yaw too. That is intended, but needs a check.
- Telly and Same Y Auto jump are the fastest options and the most likely to trip Spartan's scaffold_analysis and NCP
  SurvivalFly; they stay off by default.
- Kill Aura's Silent aura also writes `sent`. Scaffold Fast pauses with Kill Aura (existing *Disable with Kill
  Aura*); with it off, Scaffold's request wins, as in Rise and LiquidBounce.
