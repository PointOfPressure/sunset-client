# PvP systems overhaul: progress

Working notes for the Kill Aura, Scaffold and Inventory Manager overhaul, so a later session can carry on.

## Status

| Phase | State |
|---|---|
| 1. Kill Aura: combat version and attack scheduler | Done (committed) |
| 3. Inventory Manager | Done (committed) |
| 2. Scaffold | Done for this round (committed): Sprint Vanilla, presets |
| 4. Integration checks | Reviewed in code; in-game checks pending |

Last good check: `bash /c/Users/jnowa/hzbuild/check.sh <name>` (applies the checkout's mixins to Horizon 66f3981 in a
fresh stage, `cargo check -p horizon-client --bin horizon-client`, then `cargo test -p horizon-client --lib -- sunset`).
Phase 1: 0 Rust warnings, 203 tests passed. Phase 3: 209. Phase 2: 211, 0 warnings. The script lives outside the repository; its steps are those of the
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

## Phase 3: Inventory Manager

Inventory Cleaner is now **Inventory Manager**: the module is renamed and saved settings carry over (`RENAMED`:
`inventorycleaner` → `inventorymanager`). Its presets, junk lists, limits and Loot key are kept.

`inventory_plan.rs` (pure, unit tested) plans; `player.rs` (`inventory_manager`, `plan_item`) runs one step every
Delay ticks (plus up to Delay variation), in this order: hotbar layout, then a junk stack or worse duplicate thrown,
then a sorting swap.
- Scores come from the game's item data (`ItemInfo.attack_damage`/`attack_speed` modifier amounts, the
  `minecraft:armor` and `armor_toughness` modifiers, the best `ToolRule` speed, `stack.enchantments()`):
  weapons by combat version (Kill Aura's Combat version setting, else Auto from your attack speed), armour, tools,
  each scaled by wear.
- **Duplicates**: a piece goes only when another one, worn or carried, is worth more and dominates it (no custom
  name, each of its enchantments on the better one at the same level or higher). Maces, spears, tridents, bows and
  crossbows never count as duplicates. Unlike the old tier pass, this also runs with the Custom preset; Duplicates
  turns it off.
- **Protected items** (default totems, pearls, golden apples, elytra, mace, trident) and slots Loadouts locks are
  never thrown.
- **Hotbar layout**: `sword,pickaxe,,blocks,...`, filled left to right with the best match by hotbar-key swaps.
  It doesn't overlap with Loadouts, which places exact items: Loadouts' locked slots are left alone.
- **Sort inventory**: the main inventory in order, one swap of two slots (three pickup clicks) a step.
- **Only in inventory** (on by default): with it off, steps also run with no screen open.
- Coordination: nothing runs while GUI Move holds clicks or another module holds the hand
  (`combat_control::owner()`); armour you wear stays Auto Armor's (only slots 0 to 35 are touched); the offhand
  stays Offhand's; Chest Stealer works in containers, where Inventory Manager doesn't run.

Limits: no stack merging; no manual run key (the Loot key stays); Auto Armor still ranks armour by material, so a
carried piece Inventory Manager rates higher (Protection IV iron over plain diamond) is kept but not worn.

## Phase 2: Scaffold

Scaffold was already complete in its algorithms (Normal with silent/visible looks, ray cast and movement correction
modes, Late click, Same Y, Expand, Fast tower; Legit's edge sneaking; Godbridge; Telly with its own sprint, Keep Y
and tower; Scaffold's look goes before Kill Aura's in `sunset_rotation::sent()`, and Kill Aura can pause it with
Disable with Kill Aura). Those were tuned against GrimAC traces in earlier work, so this phase adds to them without
rewriting them:
- **Sprint** (Normal): Keep, **Vanilla** (new: sprint only while the keys' move, seen from the look the server has,
  has a forward impulse of 0.8 or more, as vanilla requires; `scaffold_normal::sprint_allowed`, unit tested), Off.
- **Preset**: Custom, **Speedbridge** (Legit with 0-50 ms sneak delays), **Blatant** (Normal: Snap rotations, silent
  look with movement correction, no ray cast, sprint kept, Expand 2, fast tower while moving). They fill in
  existing settings when picked and add no algorithm, so placement goes through the same engine and its one move
  packet a tick. A test checks every preset field is a real setting that takes its value.

Not done (needs in-game work on the tuned engine): a rework of tower recovery after a missed placement, placement
feedback messages, and Telly/Godbridge sprint unification. No mode is claimed to pass any anticheat.

## Phase 4: integration (by reading the code; not tried in game)

- Kill Aura + Scaffold: Scaffold's look wins in `sent()`; Disable with Kill Aura pauses Scaffold.
- Kill Aura + W Tap / Jump Reset: Kill Aura has no sprint control; they act on its hits (`melee.rs`).
- Inventory Manager + Kill Aura / Auto Mace / Scaffold / Auto Tool: no step while another module holds the hand.
- Inventory Manager + Auto Armor / Offhand: it never touches worn armour or the offhand.
- Inventory Manager + Loadouts: locked slots are never moved from, filled or thrown.
- Inventory Manager + Chest Stealer: Chest Stealer works in containers, Inventory Manager only in your inventory
  (or with no screen open).
- Inventory Manager + GUI Move: no step while GUI Move holds clicks.

## Known issues and limits

- Legacy on a server that does enforce the cooldown sends weak hits; Auto avoids that unless the attribute says so.
- In Legacy at most one hit a client tick (20 CPS ceiling).
