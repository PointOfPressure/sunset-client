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
filtering a predicted ghost from rendering and native picking, then
restores it when the confirmation window expires.

R28 also uses actual sent-ray Anchor contact for Double
eligibility and deferred clicks, the configured physical Use binding, synchronized
sprint for Crystal breaking, crouched Crystal actions and a mounted-action veto.
A pending Bot pearl plan owns its stationary window instead of allowing an old
positioning task to restart movement. Trap uses native item/block prediction
sequentially, so later cells can use an earlier block in the same bounded batch.
Its target Range is separate from the current block-interaction attribute.

Optimizer picking delegates the original native result unless its nearest
crystal is a predicted ghost. In that case native non-crystal picking and
dragon-part geometry remain in use, with unmasked crystals ranked normally.
ID-based picking distinguishes co-located entities. The position-only render
hook keeps co-located crystals visible instead of hiding the wrong entity.
Its immutable native checkpoint is recorded below; live receipts remain
separate from the compiled adapters.

## Verification boundaries

The current [R28 build record](evidence/combat/parity-build-r28.json) pins the
optimized native client and package to unchanged source. All **671 focused
checks pass**: 624 compiled/native adapter checks and 47 source contracts.
Official Mixins plans 84 support modules and 56 injections. The build retains
16 warnings for unused APIs/imports and one Option-loop style issue.
Live validation records identify their own
binary; an R26 result is not relabeled as an R27 test.

[Live player evidence](evidence/combat/parity-live.md) separates the passing
Aura, City and controller scenarios from retained failures and fixture errors.
Original PNGs and authoritative outcome receipts accompany those records.

R27 adds creative-mode manual macros, current attribute reach, entity-occluded
Crystal crosshair selection, success-only held Anchor repetition, immediate
inventory-scene retirement and conditional pearl reserves. Explicit debug
output exposes pearl owner resolution and bounded planner counters; it does
not run per-frame formatting or claim that a successful native use call is
server acceptance.

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
