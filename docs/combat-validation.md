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

The current foundation, macro, melee, inventory, packet and sword-controller
helpers have 94 passing renderer-free checks. These include actual pinned
engine damage and menu SWAP routines, not only simplified test models. They
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
