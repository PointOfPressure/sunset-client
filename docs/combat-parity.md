# Combat behavior parity

Development branch: `native-combat-parity`. This follows the tested core batch
in PR #16; it is not a release or a claim that every reference behavior is
already verified in a running native client.

The behavior reference is the locally supplied **Anubis Client 1.0 (1).jar**,
SHA-256 `e60968d360d57e8cfcd68bb127f6f56c7801ddebcff331827102596a209b15f1`.
Its registration contains 25 Combat modules, including the two PvP bots.
Reference inspection and credit belong in source/documentation; the client
interface keeps Sunset branding. The Rust implementations and test adapters
are original native work; the reference Java, binaries and assets are excluded.

## Scope

- Melee: Aim Assist, Triggerbot, Kill Aura, Shield Breaker, Mace, Spear Swap,
  W Tap and Jump Reset.
- Explosives/positioning: Anchor Macro, Anchor Aura, Crystal Macro, Crystal
  Aura, Auto City, Auto Trap and Crystal Optimizer.
- Controllers: Sword PvP Bot and Crystal PvP Bot.
- Inventory/utilities: Auto Totem, Auto Inventory Totem, Hover Totem, FastXP,
  Key Throw and Friend Guard.
- Packet timing: Fake Lag and Knockback Delay.

## This branch

Aim Assist uses five reference controls, current player geometry, the recent
attack target, bounded target retention, a 150 ms mouse-input window and native
sensitivity-sized movement. Kill Aura and the melee helpers share actual
forwarded action ownership, critical timing, shield prediction and Mace state.
Sword PvP Bot owns its attacks and input instead of configuring Kill Aura.

Crystal and Anchor planning use copied RTT-led geometry, native contextual
collision and the world border. Forecasts never move an actual entity. A shared
hurt ledger adjusts target raw damage before native armor/magic reduction;
self and friend checks continue to evaluate the complete blast. Crystal Face
Place uses observed health plus absorption, horizontal cover or low armor.
Placement receipts require a matching server crystal spawn.

The Crystal bot adds concrete branches behind all 13 reference controls:
launch/follow-up, anchor guard and surround, hole/roof mining, supported descent,
XP mending, acknowledged hotbar refill, escape/chase pearls and rival history.
Manual input, context changes and temporary pauses release or preserve the
appropriate owned actions. Native compilation and live fixtures must establish
that these branches work through the complete client.

Inventory changes separate reaction, swap, close and return stages. Key Throw
retains a request for ten ticks. Macro modes use actual crosshair contacts and
real block/entity observations. Fake Lag buffers non-action packet types too,
with bounded FIFO recovery and actual protocol codecs.

Auto City is transient across saved configuration. Auto Trap defaults to the
reference ring/roof plan; its optional native roof-support/jump extension is
off by default. Crystal Optimizer preserves authoritative entities while
filtering a predicted ghost from rendering and native special hitboxes, then
restores it when the confirmation window expires.

## Verification boundaries

Focused policy and native-kernel tests are separate from full client compilation
and actual player fixtures. Test counts alone do not prove behavior parity,
server acceptance or performance gains. Evidence must identify the exact source,
client/package hashes, native host revision and commands used.

Known native differences remain explicit:

- Collision, population, prediction and queue budgets are bounded. Unknown
  chunks, item components or required observations cannot create positive proof.
- Macro transactions serialize behind actual prior world observations; the
  reference can retain more than one pending target.
- Native self/friend safety controls can refuse actions the reference permits.
- The pinned Horizon dedicated server omits some explosion source identity and
  position fields. Those omissions cannot confirm strict Crystal/Anchor hurt
  receipts; a private stock Java backend is required for those fixtures.
- Optional native controls and rendering use Horizon's own UI and renderer.
  They do not reuse reference graphics or promise pixel-identical presentation.

The [R25 immutable build record](evidence/combat/parity-build-r25.json) identifies
83 support modules, 54 official injections and 443 passing focused checks
(427 compiled/native adapters and 16 source contracts). Its optimized client
SHA-256 begins `915a82b01688`. It has 15 remaining unused-helper/API warnings.
This checkpoint is separate from subsequent lifecycle/behavior corrections.

Follow-up validation records distinguish implemented behavior, passing isolated
player checks and remaining gaps. Earlier core-batch evidence is not relabeled
as proof of the new controls, forecasts or staged actions. Raw failures and
inconclusive observation windows remain associated with their original build.
