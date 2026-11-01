# Native combat development

The `native-combat-parity` branch develops the 25 Combat capabilities inspected
in the local 1.0 reference. Every module remains experimental and starts off.
[Parity notes](combat-parity.md) identify the exact reference and remaining
native adaptations. [Validation](combat-validation.md) retains historical core
fixtures separately from the new parity work.

- **Aim Assist:** five reference controls, recent physical mouse motion,
  current player geometry, recent-hit retention and native sensitivity steps.
- **Trigger Bot / Kill Aura:** crosshair reaction/charge and critical timing,
  or native-range enemy selection with Distance/Health/Angle priorities. Kill
  Aura has four controls; the removed configurable Range is not displayed.
- **Shield Breaker / Mace / Spear Swap:** native shield disabling, falling Mace
  and enchantment selection, buffered Lunge or optional crosshair reach poke.
  Borrowed slots return only while the module still owns them.
- **W Tap / Jump Reset:** temporary native input after a real forwarded sprint
  hit/stab or accepted own knockback. No invented upward velocity.
- **Friend Guard:** direct/manual hits and all native piercing-spear victims,
  using the actual carried look. Optional named action-bar feedback is limited
  to one message per 1500 ms.
- **Crystal Aura / Anchor Aura:** native-range staged planning, RTT-led copied
  geometry, hurt-window incremental offense and complete self/friend safety.
  Their [16/6 controls and limits](explosive-auras.md) replace legacy selectors,
  reach and selectable damage-model settings.
- **Crystal Macro / Anchor Macro:** held/manual sequencing with actual contacts,
  material routing, charge states and observed acknowledgements. Single/Double,
  optional guard cover, offhand behavior and conditional slot return are separate
  from Aura targeting. Pending native actions remain serialized.
- **Auto Totem / Auto Inventory Totem / Hover Totem:** empty-offhand refill,
  staged inventory/hotbar actions, or the actual hovered item. Real state IDs and
  both server slot updates confirm swaps; local prediction is not an ACK.
  The two automatic refill providers remain mutually exclusive.
- **FastXP / Key Throw:** native bottle pacing and buffered one-shot pearl/wind
  use. Wind jump uses actual native jump/use input and a scoped downward look.
- **Auto City / Auto Trap:** native mining or bounded acknowledged obsidian
  placement. City does not persist enabled state. Trap defaults to the reference
  ring/roof ordering; its native roof-support/jump extension is optional and off.
- **Crystal Optimizer:** next-tick visual/special-hitbox masking with a measured
  4–20-tick confirmation window. Authoritative entities remain present; timeout
  restores visuals and prevents repeated masking of a rejected ID.
- **Sword PvP Bot:** owns its actual attacks and movement independently of Kill
  Aura configuration. Patient/Aggressive timing, counters, spacing, critical
  jumps and native eating retain explicit input/hand ownership.
- **Crystal PvP Bot:** 13 controls cover melee launch/follow-up, anchor defense,
  surround, hole/roof work, descent, XP Mending, acknowledged hotbar refill,
  escape/chase pearls and rival history. Manual input and context pauses retain
  or release the specific action being owned.
- **Fake Lag / Knockback Delay:** bounded original-packet FIFO or selected
  incoming-impulse delay, with explicit flush/replay/context rules. Timing
  randomness does not establish server acceptance or detection avoidance.

The reference inspection informs observable behavior; this repository does not
bundle its Java, JAR or assets. Native work budgets, unknown-data refusal and
framework/UI choices remain documented differences. Implementation, focused
adapter tests, full compilation and real-player outcomes are separate claims.

## Recent player-death protection (R10 source)

Crystal Aura, Anchor Aura, Crystal Macro and Anchor Macro each expose **Protect
loot**, defaulting to true while the modules themselves remain off by default.
Module-planned blasts pause for 30 seconds near an observed player death. The
radius is the full native blast radius plus a two-block drop margin: 14 blocks for
crystals and 12 for anchors. No cover reduction applies to this guard.

This protects a recorded death location, not tracked item positions. It does
not detect collection, follow moving drops, guarantee item survival, cancel an
already sent blast, or intercept every manual attack/use outside these modules.
Fresh input does not bypass the timer. Recognized protected anchor clicks are
consumed before unsupported-hand/inventory fallbacks could detonate via vanilla.
An active sequence newly blocked by a death uses normal owned-slot cleanup.

The guard retains at most 16 locations. Overflow or an unavailable position for
a recognized player temporarily refuses all protected blasts for 30 seconds.
Same-connection respawns in the same exact dimension retain zones; login,
connection changes and dimension changes clear them. Missing scene identity
refuses protected plans until lifecycle information is available.

`.lootguard status` and `.combatdebug` show the current reason, lazy-expired
location count and blocked-check count. The counter counts evaluations, not
packets or prevented explosions. `.lootguard clear` explicitly clears zones;
disabling one module's Protect loot setting bypasses this guard for that module
while leaving its other safety checks in place. The [R10 native proof](evidence/combat/loot-player-r10.md)
covers all four owners and 16 guard scenarios, including Anchor Macro's checkbox
bypass. It does not claim four separate checkbox tests or a global item guard.

## Aura base setup and historical core proof

Crystal Aura exposes **Auto Obsidian**, on by default. The parity control set
has no separate Obsidian Delay.
It is a fallback when no ready break or ordinary existing-base placement wins.
Both materials must be available. A real support ray and
native dry placement preview must accept the destination; existing solid
terrain is not mined or overwritten. Hotbar and actual offhand obsidian work
through ordinary native use and conditional slot return.

An attempted cell requires an actual obsidian block payload and matching
observed world before Crystal Place can proceed. It then recomputes target,
self, friend and loot checks using the now-observed terrain. Future-base exposure uses a copied obsidian block; fresh authoritative scoring
can still reject a setup after the world or actors change, leaving the base unused. It is not cached permission to explode.

Setup uses the existing scan/action planner, with a 64-cell retry ledger,
three sends per unchanged cell and a 30-tick wait. Ordinary repeated air
rejections, pauses and action contention cannot replenish retries. Module/context
reset clears that ledger. Once 64 distinct cells are retained, new setup cells
are refused until reset; existing ready breaks and placements still work.
No extra renderer, fake block or predicted crystal ID is introduced.
[R14 paired-player setup proof](evidence/combat/auto-base-r14.md) passes nine
primary scenarios and a distinct post-base loot-veto case, while retaining two
earlier harness failures. The uninterrupted cycle is separate from staged
cases that pause Break to capture an actual crystal ID.
