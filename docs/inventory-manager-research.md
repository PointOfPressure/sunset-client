# Inventory Manager repair notes

Reference sources were inspected as text, not executed or copied. Rise's bundled licence prohibits reuse without
permission; the Rust changes are independent implementations of inventory-management behaviour.

## Reference map

| Source | Relevant logic | What carries over |
| --- | --- | --- |
| `Rise-6.9.5-main.zip`, `module/impl/player/Manager.java` | Scans inventory/equipped armour; ranks tools, weapons and armour; assigns hotbar roles; handles one action category before rescanning. Delays work after attacks, placement and foreign containers. Protects custom items. | Separate selection from execution, deterministic assignments, input/container checks, paced actions. This is the most complete manager located in this audit. |
| Rise `player/inventory/{InventoryScan,ItemScorer,SlotStack}.java` | Snapshot/result objects and scoring interface used by Manager. | Pure planner fed from current contents. |
| Rise `InventorySync.java`, `InventoryMove.java` and bypass modes | Old transaction IDs, inventory opening/closing and movement coordination. | Protocol-specific: do not transplant the 1.8 transaction/open-inventory scheme into Horizon 26.3. |
| client-a `app/inventory/InventoryUtil.java`, `app/module/misc/AutoLoot.java` | Inventory-screen click helpers, hotbar slot conversion, queued swaps, delayed totem/pearl cleanup. | Keep menu/cursor ownership and pacing consistent; Loot must not bypass the manager's executor. |
| client-b `app/InventoryUtils.java`, `InventorySnapshot.java` | Validates slot ownership, insertion/take permissions and hotbar swaps; delayed restoration checks player/world identity and item use. | Respect the active menu, current input and lifecycle. These helpers are not evidence of a complete general-purpose manager. |
| client-d `.../pipeline/socket/BlockCacheController.java`, `.../vertex/resource/PoolZoneAllocator.java` | Paced screen-only cleanup with allowed/excluded slots and conservative pearl removal; cursor-empty checks before totem swaps. | Do not remove an entire useful stack merely because it crosses a quota; yield to the player's cursor. |
| client-c | Inventory accessors found; much behaviour uses obfuscated intermediary names/VM dispatch. | No complete manager was reliably reconstructed; no claim of equivalent behaviour. |
| macro-app | Electron keyboard/mouse macro application. | No game inventory model suitable for item scoring or transactional planning was located. |

Rise also clears/drops cursor contents and can issue bursts of partial-stack clicks. Sunset deliberately yields to a
player-owned cursor and trims whole surplus stacks. Nonzero quotas can retain one partial stack above the limit;
protected/assigned stacks can independently exceed it.

## Repairs

- Canonical item components exclude network Wire bookkeeping and runtime bundle selection. Original wire counts
  must not make compatible stacks look different. Actual component changes must invalidate cached plans.
- Full hotbar assignment considers every candidate, using nine destination bits rather than an exponential search
  with a three-candidate cutoff. Repeated roles can occupy all nine slots.
- Merge sources come from the main inventory; hotbar stacks are destinations. This prevents layout/merge loops.
- Settings, locks, Loot mode, screen context and combat model invalidate decisions. Disconnect and Panic reset them.
- Cadence follows Delay/variation, at most one logical operation per client tick. Zero delay can process independent
  slots every tick, without a global per-action ping wait. Ping bounds rollback observation and reservations on
  recently touched slots: dependent swaps/merges/drops wait, so rejecting a swap cannot turn cleanup into dropping
  the valuable item still at its source on the server. Reservation expiry invalidates idle plans. User input yields while
  held, without an additional ten-tick pause on release. Auto Armor retains its separate settings/explicit holds.
- Corrections suppress affected slots while other work can continue. Context changes cannot inherit an old pending
  operation's refusal. User input/settings changes cannot shorten an action deadline or another owner's hold.
- Loot uses the same cursor, input, movement, combat-owner and settle gates as ordinary management. The executor
  records a send only when the native menu API accepts it.

Compatibility relies on normal native menu prediction and existing server-known idle-input/sprint checks. This audit
does not establish a bypass for any deployed anti-cheat, and no live mainstream anti-cheat server was tested.

## Validation

31 planner/executor tests pass using `rustc --edition 2024 --test -O tools/inventory_tests.rs` and running the
resulting test binary. Native test compilation passed, and the complete local tree built successfully through
the launcher as `native-66f3981-d45c7c3ff296` for final testing. These results do not establish server acceptance.

## Manual checks

Use a disposable inventory: this module intentionally drops configured junk and surplus items.

1. Put unequal partial stone stacks in hotbar/main inventory: they should merge without a stuck cursor. Put two
   partial stacks in two configured Blocks slots: neither hotbar slot should repeatedly empty/refill.
2. Configure nine Blocks slots and provide nine stacks: all nine slots should fill, then stop changing.
3. Change slot roles, protected items and limits while the inventory stays open: decisions should update immediately.
4. Pick up/drag an item or hold attack/use: management should yield. Release it: work resumes after settling.
5. Hold Loot with excess totems/pearls, then move/sprint or use the cursor: no competing cleanup clicks should run.
6. Test rejected clicks/high latency, disconnect/rejoin, and Panic/resume: no rapid retry loop or overlapping steps;
   Panic must leave all modules off. Test Auto Armor alongside the manager for cursor/input conflicts.
