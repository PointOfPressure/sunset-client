# Native combat development

This branch implements the core jobs of the 25 inspected Combat capabilities.
They remain experimental and disabled by default. Implementation is distinct
from runtime coverage; consult [validation](combat-validation.md) for the exact
checkpoint, observed outcomes, failures and remaining work.
The [work limits](combat-budgets.md) document the caps and their scope.

- **Aim Assist, Trigger Bot and Kill Aura:** recent-input aim assistance,
  crosshair attacks, or bounded visible-target selection. Weapon charge uses
  the candidate item, and critical conditions use actual native movement.
- **Shield Breaker, Mace and Spear Swap:** observed shield handling, native
  mace/enchantment selection during real falls, and key-triggered Lunge or
  optional crosshair reach poke. Borrowed slots return conditionally.
- **W Tap and Jump Reset:** short native input changes after an actual charged
  sprint hit or observed knockback. No upward velocity is invented.
- **Crystal Aura and Anchor Aura:** shared bounded opportunity selection,
  observed transactions, collider exposure and equipment-aware safety. A
  hypothetical mined wall changes exposure without changing damage source.
- **Crystal Macro and Anchor Macro:** user-directed crystal use or observed
  place/charge/detonate transactions. Anchor Double means two acknowledged
  cycles. Crystal Macro requires held use and a legal current crosshair ray.
- **Auto Totem, Auto Inventory Totem and Hover Totem:** empty-offhand refill,
  staged inventory/hotbar transactions, or an actual inventory-pointer target.
  Native SWAP avoids carried-stack pickup, and both server slot updates are
  required. The two automatic refill providers are mutually exclusive.
- **FastXP and Key Throw:** native bottle-use pacing and one pearl/wind charge
  per key edge. Wind jump uses the real jump input and a downward carried look;
  it does not inject movement. Pearl/Wind keys and Wind jump are configurable.
- **Friend Guard:** direct-hit, actual carried-look spear and actual-hand use
  checks, including known friends along a piercing path. Teleport acceptance
  updates that carried look before the next movement packet.
- **Auto City and Auto Trap:** one native mining operation or acknowledged
  obsidian placement around the full target footprint. City rechecks current
  health/equipment before each mining step. Trap roof completion uses an
  outside support and at most two real jump requests per unchanged footprint.
- **Crystal Optimizer:** temporarily masks only native crystal geometry after
  an accepted outgoing hit. Authoritative entities remain available to Aura
  and collision checks. Missing confirmation unmasks the entity and suppresses
  repeated predictions for that rejected ID.
- **Sword PvP Bot and Crystal PvP Bot:** bounded movement/target controllers
  coordinating the existing combat modules and real food use. They yield to
  manual input and each other, retain per-field ownership, and serialize a
  detached baseline so temporary settings do not overwrite user configuration.
- **Fake Lag and Knockback Delay:** bounded outgoing movement delay retaining
  original tick-end markers, and selected own-impulse delay. Network control
  passes immediately; gameplay actions flush movement first. An explosion packet's visual/audio metadata
  shares its delay. These are not general-purpose packet interception tools.

Open Combat in the menu, or use `.settings <module>` for controls and `.bind`
for module toggles. Spear Lunge, pearl and wind keys are separate settings.
`.combatdebug` formats decisions only on request; it adds no per-frame log.

The controllers are local arena controllers, not pathfinding or full replicas
of the reference's autonomous systems. Motion prediction, full incremental
hurt clocks, automatic Aura obsidian/face placement, repairs,
pearl retreats and learned opponent history are outside the present scope.
All damage bounds depend on observed world state and cannot guarantee survival
after movement, latency, changed cover, other attacks or server modifications.

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
while leaving its other safety checks in place. Runtime proof for this R10
addition remains pending; earlier R7–R9 results do not test it.
