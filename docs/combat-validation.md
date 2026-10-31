# Combat validation

This feature branch targets Horizon `ba6344a59cb9e755b730dcf02285fc0d33b97d3c`.
It remains a normal native mod, built by official Horizon Mixins from pristine
host source. The incoming packet hook requires the newer official Mixins
`cb1ad23022392d88f006cf94cbf6552a73f5df12`; the earlier `0d6b103` planner rejects
opaque macros in that native handler. No Horizon source patch is required.

## Confirmed R4 player outcomes

[Recorded outcomes, binary hash and screenshot hashes](evidence/combat/player-proof-r4.json)
cover two actual native clients, `CodexAttacker` and `CodexTarget`, connected
to a fresh stock Java 26.3 world on loopback. Protocol is 777; the official
server JAR SHA-1 is `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.

- Crystal Aura placed an observed crystal, attacked that actual ID and removed
  it. Target health changed from 20 to **5.041667938232422**, with knockback.
- Anchor Aura produced observed charge states **0 → 1 → air**. Target health
  changed from 20 to **10.263781547546388**, with knockback.
- Both transactions restored the attacker's selected slot. Adding the target
  as a friend prevented a new attack in the corresponding negative control.
- The target had ordinary maximum health 20 and Resistance IV to survive one
  blast. The attacker had Resistance 255 as a separate fixture fail-safe.
  Both modules explicitly used Unmitigated self cap16, Anti suicide and minimum
  remaining health4. Fire damage and natural regeneration were disabled.

These are stationary, controlled fixtures. They establish actual player blast
damage and observed ordering, not moving-target performance or anticheat results.

The user's Horizon dedicated server was backed up before testing. Its explosion
path skipped damage to other connected players during a guest action. Crystal
removal without damage and an earlier fire-only Anchor health change were
retained as failures, not counted as player explosion proof. The private stock
Java backend avoids changing that host implementation.

## Equipment check correction

A durable plain-netherite fixture with no resistance rejected self cap2. Later
diagnostics measured the eligible canonical blast at **2.008**, above that cap.
That rejection is expected. A separate placement-only comparison at cap6
blocked Unmitigated placement and allowed Observed equipment placement; it
consumed one real crystal and restored the selected slot. No explosion was
performed in this comparison. The module's safety limits were not weakened.

The old failed fixture remains separate from the corrected test recipe.

## Renderer-free checks

The R7 foundation, ownership, macro, melee, inventory, packet and controller
helpers have **155 passing renderer-free checks**. These include actual pinned
engine damage, menu SWAP, protocol encoding/decoding, mining and outline-ray routines, not only simplified test models. They
do not prove native linking, incoming server acknowledgement or rendering.
Native build and runtime results for later waves must be recorded separately.

## R5c build checkpoint

The combined branch passed official Mixins planning/staging (52 support modules,
50 injections), the pinned native compile check, and an optimized playable
build. The client SHA-256 is
`18863f8fa988a194266a8a2ffe3bc32dc9cb3d039d41c9c301670480dad065f4`.
Five unused-code warnings remain in this checkpoint. All **19 isolated native
explosive cases passed**, with graceful exit0 and unchanged frozen inputs:
[outcomes](evidence/combat/runtime-r5c.json),
[source fingerprints](evidence/combat/source-r5c.json).
Its new melee, manual, inventory, packet and controller paths are entering
separate runtime testing; the explosive evidence must not be presented as
proof for all of them.

The immutable R5c test bundle still labelled its builder as `0d6b103`; build
provenance records the actual verified `cb1ad23` tool. The public package
manifest was corrected in `12f0c4a`. Frozen files and reports were preserved.

## R5c melee player outcomes

[Twelve passing server/input scenarios](evidence/combat/melee-r5c.json) cover
real hits, native reach, trigger critical conditions, direct and piercing
friend protection, spear Lunge, shield behavior, Mace, W Tap, Jump Reset and
recent-input/idle aim behavior.

- Real falling Mace damage was 31.690 versus grounded sword damage7.
- W Tap reduced early horizontal speed from approximately0.153 to0.084,
  0.046 and0.025 while forward/sprint input remained held.
- Actual-knockback jump height increased from0.965 to1.252 blocks.
- The same mouse input gained3.675 degrees toward the enemy with Aim Assist;
  after idle input, camera yaw stayed unchanged while the target moved.
- A raised shield blocked the sword control. After automatic axe use the
  server's use flag cleared and the subsequent charged sword dealt7.

The original target-UI shield assertion failed because its own native use
animation remained stale, despite authoritative server lowering. The initial
Mace fixture also missed due to integer-teleport centering. Both failed traces
are retained; neither is relabelled as a passing run. An initial Mace image
label was reused during correction, so that old image is not presented as
independent immutable failure evidence.

## R6c build checkpoint

The expanded implementation passed137 renderer-free checks, official Mixins
plan/stage (58 modules,51 injections), native compile check and optimized
playable build. Client SHA-256:
`c0a28e274eccb8f6827d19c13fa4960bdaefd0a6fd672bdcdce2b2bfd5418754`.
One unused-helper warning remains. The frozen package correctly pins Mixins
`cb1ad23`. City/Trap, visual Optimizer, controller persistence and the inventory
fixes require their own native outcomes; their build is not runtime proof.

All 19 isolated explosive cases also passed on this checkpoint. That does not
establish the new City/Trap, controller or packet behavior.

## R7 build checkpoint

The next checkpoint passed all 155 focused checks, official Mixins plan/stage
(58 support modules, 51 injections), native compile check and optimized build.
Client SHA-256:
`f856c2fe3f634aefac465517d4ca7ebb8d4e620340421499e174f488939ba696`.
One unused-helper warning remains. All **19 isolated native explosive cases
passed** again, with native exit0 and unchanged frozen inputs. See the compact
[R7 build and runtime record](evidence/combat/checkpoint-r7.json) and
[source fingerprints](evidence/combat/source-r7.json).

R7 addresses failures found with the earlier real-player fixtures:

- Original movement packets retain their original ClientTickEnd boundaries.
  Delaying positions while forwarding their boundaries made the stock server
  reject a later batch as multiple positions in one tick. Correction and
  same-connection respawn discard old coordinates while retaining only the
  original required boundaries; they do not invent new game ticks.
- Inventory swaps use a valid native SWAP click without local prediction, then
  require both actual server slot updates. The server silently accepts a
  correctly predicted click, so waiting for two echoes after prediction was
  an invalid acknowledgement assumption. Official Java bytecode and actual
  Horizon protocol round trips verify this distinction.
- A short Anchor Macro press survives until the next native tick. Held
  Single/Double controls, including Delay20, already completed real observed
  cycles; the quick-click path is tested separately.
- Shared cleanup retains only an existing ticket in the same live scene so
  master disable can restore a borrowed slot. Scene changes invalidate it
  before cleanup. Bots preserve user baselines in configuration and yield to
  explicit edits, including edits to the same temporarily applied value.

These changes have compiled and passed their focused checks. Their paired-player
runtime suites are still being run; no aggregate pass is implied here. Earlier
failures remain recorded separately from corrected recipes and later results.

Run the renderer-free checks with:

```sh
python tools/combat_tests.py /path/to/horizon-ba6344a --out combat-tests.json
```

## Native screenshots

These files are unmodified native framebuffer captures. The private pair used
a 1024×576 logical window at 1.25 scaling, producing **1280×720** captures.
There is no cropping, resizing, compositing or generated imagery.

![Crystal test](evidence/combat/crystal.png)

![Anchor test](evidence/combat/anchor.png)

![Target after anchor blast](evidence/combat/target-anchor.png)

![Protected friend](evidence/combat/friend-protected.png)
