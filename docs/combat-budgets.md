# Combat work limits

These are implementation bounds, not measured FPS gains. Multiple enabled
modules and deferred validation can each consume a budget in the same tick.
The limits must not be added up as though they were one global scheduler cap.
The native renderer, world size, other mods and the server still affect cost.

- Crystal Aura advances at most 96 scan cells, considers four nearby targets
  and evaluates at most four candidate actions per planning pass. Its damage
  budget allows 24 predictions. Anchor Aura advances 128 cells of its bounded
  radius-five scan, evaluates at most four candidates and allows 48 predictions.
  Both share one selected deferred hand action. Execution revalidates current
  geometry, target, equipment and damage rather than trusting the old plan.
- Every damage budget reserves exposure rays before casting: at most 256 rays
  per prediction and 4096 rays per budget, including explicit line checks.
  Exhaustion rejects the action. Unknown cover, equipment or malformed bounds
  do not become a zero-damage safety estimate.
- Optional Aura obsidian setup reuses the same 96-cell scan and four candidate
  damage evaluations. It adds at most 48 support-ray checks per planning pass,
  with separate fresh validation for an accepted deed. Its 64-cell ledger
  waits 30 ticks, permits at most three sends per unchanged cell and requires
  actual obsidian payload plus observed-world agreement. The optional setting
  defaults off. Unrelated packets and empty setup history exit the observer
  before per-block work. Ordinary pauses and hand-action contention retain the
  retry/acknowledgement history; explicit module or scene reset clears it.
- Melee/controller target scans stop above 512 observed entities. The Crystal
  controller additionally rejects a scene above 128 crystals, scans at most 64
  anchor cells per tick and evaluates at most four blast sources per threat
  sample. Its short cache is keyed to source geometry and actor displacement;
  an incomplete scan is not described as a complete safety result.
- Inventory actions inspect fixed native inventory slots. Double Hand and
  emergency offhand checks inspect every observed crystal only when there are
  at most 16; overflow is treated as danger. A swap retains one transaction,
  two expected slot acknowledgements, a deadline and bounded retries. A correctly
  predicted client click is not treated as an acknowledgement.
- City selects among three nearby targets and their four adjacent walls with
  a 24-prediction selection budget. Each native mining step has a fresh
  two-prediction safety budget. Mining waits at most 200 ticks and the final
  authoritative block acknowledgement at most 30. Trap retains at most 64
  placement records, with three attempts per unchanged cell and a 30-tick wait.
  Roof completion requests at most two real jumps per unchanged footprint,
  at least 20 ticks apart.
- Fake Lag retains at most 64 original movement/tick-end packets, at most
  4 KiB, and at most one second. Capacity pressure flushes existing packets in
  order. It does not manufacture tick-end packets. Knockback Delay retains at
  most 16 selected impulse packets, 256 KiB total, 32 KiB per packet and one
  second. Correction, world transition, menu and disable paths have separate
  ordering/cleanup rules covered by the focused and native tests.
- Crystal Optimizer retains at most 64 visual masks and 128 rejected IDs.
  Its confirmation window is clamped to 10–40 ticks. Rejection overflow
  disables prediction for that scene. The authoritative crystal list remains
  untouched; the native geometry/beam cull skips only the masked body. With no
  masks, the render hook exits after an atomic read.

Shared hand actions use an exact generation ticket plus connection, dimension
and entity identity. A late completion cannot release a newer action with the
same owner name. Temporary movement and slot restoration are conditional on
still owning the applied state. Configuration serialization operates on a
detached copy and projects retained controller leases back to the user's
baseline; it does not mutate gameplay while saving.

Diagnostics format text only when `.combatdebug` is requested. There is no new
per-frame combat log, background network service or render-time attack loop.
See [validation](combat-validation.md) for what has actually run and
[module scope](combat-modules.md) for remaining reference differences.

## Recent-death guard

The R10 guard has no tick hook, dropped-item scan or background render work.
Player death observation uses two O(1) native identity lookups and one bounded
16-cell update. Protected candidate checks inspect at most 16 fixed locations;
TTL expiry is lazy. Dimensions use an exact, bounded 256-byte key. Invalid
positions and capacity overflow fail closed instead of evicting live zones.
Explicit diagnostic commands allocate their formatted output; update paths do
not format messages. Existing blast prediction budgets remain unchanged.
