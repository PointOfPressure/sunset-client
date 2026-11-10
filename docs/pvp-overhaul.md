# PvP systems overhaul: progress

Working notes for the Kill Aura, Scaffold and Inventory Manager overhaul, so a later session can carry on.

## Status

| Phase | State |
|---|---|
| 1. Kill Aura: combat version and attack scheduler | Done (committed) |
| 3. Inventory Manager | In progress: planner written (`inventory_plan.rs`), wiring next |
| 2. Scaffold | Not started (see "Scaffold: findings so far") |
| 4. Integration checks | Not started |

Last good check: `bash /c/Users/jnowa/hzbuild/check.sh <name>` (applies the checkout's mixins to Horizon 66f3981 in a
fresh stage, `cargo check -p horizon-client --bin horizon-client`, then `cargo test -p horizon-client --lib -- sunset`).
Phase 1: 0 Rust warnings, 203 tests passed. The script lives outside the repository; its steps are those of the
"local compile check" (horizon-mixins apply with an out dir outside the repo, the launcher's Rust 1.99.1 and mingw on
`PATH`, `CARGO_HOME` the launcher's cargo). Nothing here has been tried in game yet.

## Phase 1: Kill Aura

- `aura_timing.rs` (new, unit tested) is the attack scheduler, separate from targeting and rotations:
  - **Combat version**: Auto, Legacy (1.8), Modern (1.9+). Auto is Legacy only when the server has raised your attack
    speed to 16 or more, which takes the cooldown away (what servers that restore 1.8 combat on a modern protocol
    do). A 1.8 server behind protocol translation that leaves the attribute alone looks Modern: pick Legacy there.
  - **Legacy**: a click clock between Min CPS and Max CPS (Random draws each gap, Drift wanders across the range,
    Steady holds the middle). Gaps are in ticks with the remainder carried, at most one hit a client tick, the first
    hit on a new target at once, and a due click that could not land goes on the next tick it can (nothing saved up).
  - **Modern**: the cooldown (Cooldown: the attack strength a hit waits for, 1 = full charge), Custom delay, or Hit
    select (also waits out the target's hurt time), as before.
- `aura.rs`: settings grouped Combat, Targeting, Rotations, Timing, Advanced (the list has no headers; the order
  groups them). Max targets stays before Attack mode (old-config migration depends on it). `.auradebug` shows the
  combat model and why.
- Visibility (`runtime.rs`, `setting_visible`): Legacy hides Attack timing, Cooldown and Hit delay; Modern hides
  the CPS settings; Auto shows both. Aim speed shows only for Silent aura.
- Audit, no change needed: Snap already turns instantly, so no separate Instant mode was added. Losing a target
  clears the look, the aim and queued turns, and Silent aura turns back at its own speed. Target identity survives
  world changes (AntiBot tokens, own entity id). Kill Aura has no sprint handling of its own; W Tap and Jump Reset
  act on its hits, so there is no second sprint reset.

## Phase 3: Inventory Manager (in progress)

`inventory_plan.rs` (written, unit tests written, not yet registered or compiled):
- Scores from the game's item data: weapons (attack damage and speed, by combat version; Sharpness 1.25 a level in
  Legacy, 0.5 + 0.5 a level in Modern; Fire Aspect, Knockback), armour (points, toughness, Protection), tools
  (mining speed with Efficiency), all scaled by wear (Unbreaking and Mending soften it; a piece about to break is
  worth half).
- Duplicates: per kind (sword, axe, pickaxe, shovel, hoe, the four armour slots), a piece goes only when another
  piece, worn or carried, is worth more and dominates it (no custom name, and every enchantment it has is also on the
  better one at the same level or higher). Maces, spears, tridents, bows and crossbows are never duplicates.
- `hotbar_step`: category layout (`sword,pickaxe,,blocks,...`), filled left to right with the best match, one
  hotbar-key swap a step; locked slots are neither taken from nor filled.
- `sort_step`: main inventory order (weapons, tools, armour, blocks, food, potions, the rest; then by name, bigger
  stacks first), one swap of two slots a step; equal items never shuffled.

Wiring still to do (player.rs): build `Item`s from stacks (`ItemInfo.attack_damage`/`attack_speed`/`max_damage`, the
`minecraft:armor` and `armor_toughness` modifiers, the best `ToolRule` speed, `stack.enchantments()`,
`stack.custom_name()`); rename the module to Inventory Manager with a `RENAMED` entry
(`inventorycleaner` → `inventorymanager`); new settings (Duplicates, Protected items, Hotbar layout, Sort inventory,
Only in inventory, Delay variation); one step per delay in the order hotbar, cleanup, sorting; skip while GUI Move is
busy, another module holds the hand (`combat_control`), or a slot is locked by Loadouts. Auto Armor keeps owning what
you wear. Update verify.py's `inventorycleaner` settings and the README.

## Scaffold: findings so far

Not started. Existing: Normal (`scaffold_normal.rs`), Legit/Godbridge (`scaffold_bridge.rs`), Telly
(`scaffold_telly.rs`), rotations through `sunset_rotation` (Scaffold's look goes before Kill Aura's in `sent()`).

## Known issues and limits

- Legacy on a server that does enforce the cooldown sends weak hits; Auto avoids that unless the attribute says so.
- In Legacy at most one hit a client tick (20 CPS ceiling).
