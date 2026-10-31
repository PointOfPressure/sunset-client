# Crystal Aura and Anchor Aura

Experimental native combat modules for the pinned Horizon `ba6344a` host. Both
start disabled. They are separate from Crystal Optimizer and the movement
module named Anchor.

Crystal Aura places on observed obsidian/bedrock, waits for an actual crystal
entity, then attacks its observed ID. Place and Break can be controlled
separately. Anchor Aura places on inert solid supports, waits for the anchor,
charges it with glowstone, waits for the charge update, then uses a non-fuel
main-hand item to detonate. It refuses to operate in the Nether. Offhand
glowstone requires full charge before the main-hand detonation can bypass the
offhand-fuel branch.

Crystal's optional **Auto obsidian** is off by default. It only proposes a base
when no ready break or existing-base opportunity wins, uses native support and
dry-placement checks, and waits for a real obsidian block update before fresh
crystal scoring. Pauses retain the bounded setup retry history.

Both modules default **Protect loot** on: planned blasts pause near observed
player-death locations for 30 seconds. This guards recorded locations rather
than following item entities. [Exact setup and death-guard limits](combat-modules.md)
include the margin, lifecycle boundaries and explicit clear operation.

## Using the modules

Open Combat in the ClickGUI, or use `.settings crystalaura` and
`.settings anchoraura`. The default entity list is `player`; an explicit list
such as `husk` or the existing `@hostile` selector also works. Place crystals,
anchors and glowstone in the hotbar or offhand. Anchor detonation also needs a
non-glowstone hotbar slot; an empty slot is sufficient.

The modules pause for menus, item use, manual attacks/uses, mining, death,
spectator/creative mode, Blink, an in-progress Criticals action, enabled Nuker
or Scaffold, active gapple eating, or another retained combat action ticket.
Melee, macros, inventory and controller food actions use that shared ownership
ledger. Merely leaving Kill Aura or Trigger Bot enabled does not block an
otherwise idle hand. Hotbar changes and
restoration happen inside the accepted deferred action, after its look has
been sent. Only one aura can own a queued action. When both are enabled, they
share a planner: ready attacks take precedence over setup, then estimated
finishes and useful damage relative to self damage decide. Exact ties alternate.

## Damage limits

Explosion exposure uses Horizon's real collider shapes and the pinned native
sampling grid. Anchor predictions omit only the anchor block, which disappears
before its explosion. Damage estimates use the engine's armor, resistance,
protection and source-scaling kernels. Hidden remote attributes and effects
are not known, so target damage remains an estimate.

**Damage model** selects the self bound. The default **Observed equipment**
uses current local armor, toughness, resistance and protection. It falls back
to unmitigated damage if a piece could break from the blast or its item,
components or durability effects are unknown. It gives no Unbreaking credit
and does not change equipment to calculate the result. **Unmitigated** gives
no equipment/effect credit. **Max friend damage always uses unmitigated damage.**

Remote player safety assumes Hard difficulty because
Horizon has no reliable flag distinguishing an authoritative difficulty update
from a default/stale snapshot. Anti suicide additionally requires the configured
remaining health after the selected self bound. The strict model can require
cover even when wearing armor.
Friends are checked even when excluded from the chosen target entity list.
Missing world data, unknown player identity, invalid numbers and exhausted
work budgets refuse the candidate.

Bounds describe one blast against the current client world and entity
positions. They cannot guarantee survival after movement, latency, changed
cover, another attack, TNT detonation or server-specific damage modifications.
No armor/inventory/world data is mutated to calculate a prediction.

## Work limits and ordering

- Crystal Aura reads at most 96 new placement cells and 64 existing crystals
  per tick. It evaluates at most four blast candidates with a shared budget
  of 24 predictions and 4,096 rays. Population overflow refuses action.
- Anchor Aura reads 128 new scan cells, retains at most 64 anchor positions,
  considers up to four 27-cell placement neighborhoods, and evaluates at most
  four damage candidates with 48 predictions and 4,096 rays. Protected-entity
  overflow refuses action. Placement uses a documented whitelist of inert
  solid supports, avoiding unintended container/workbench screens.
- Accepted actions recheck settings, session identity, current entities,
  range, visibility, materials and damage before sending. A queued action
  expires after 200 ms; it cannot switch slots after a rejected claim.
- Progress relies on observed server state, never predicted entity IDs or
  client-side crystal removal. The acknowledgement timeout is 30 ticks.
  Anchor timeouts suppress the unchanged failed state rather than spamming it.
- Crystal place/break clocks are independent, with a minimum two-tick action
  gap and at most three sent attempts per observed crystal ID. Recent hurt
  animation defers another attack; this conservative rule does not recreate
  the reference client's full incremental-damage clock.
- `.combatdebug` reports preserved numeric decisions, rejects, send and
  acknowledgement counters. Formatting occurs only when explicitly requested.
- These paths run on ticks, with no added render effects or per-frame scans.

## Reproducing checks

The renderer-free tests compile production helpers and actual pinned engine
damage/exposure routines. They do not establish live packet or world behavior:

```sh
python tools/explosive_tests.py /path/to/horizon-ba6344a --out explosive-unit.json
```

`tools/explosive_runtime_cases.py` supplies fixtures for an already running,
disposable native client through official `horizon-ctl`. It never launches or
connects by itself. The controller must provide isolated data, capture raw
replies, and invoke `run(ctl, mod, shot, check)`. Positive fixtures explicitly
set `Entities=husk`, self cap 16 and delays 20, choose Unmitigated, keep Anti suicide on,
and give the fixture player resistance as a separate test fail-safe. Their
claims concern observed block/entity/item/health changes, not public-server
compatibility, anticheat behavior or competitive performance.

[Behavior reference](explosive-reference.md) records the exact Anubis 1.0 JAR
inspected and the remaining capability gaps. This contribution is not a claim
of full reference-client parity.
