# Feature roadmap

Planning snapshot: 31 Oct 2026, Sunset main after 0.8.1 (Horizon Nightly `ba6344a`). A scoped plan for useful
Horizon-native features, with what each contribution needs to show.

## Landed contributions

Contributions are ported by hand onto the current tree (our names and style, tests in `tools/verify.py`); the
original PRs are closed rather than merged on GitHub:

- Radar (#1), profiles with recovery and export/import (#2), the text editor with Apply/Enter and Cancel/Esc (#4),
  mouse and Hold binds for modules that support Hold (#5): Rust (zackmsa777-a11y).
- Speed modes, Velocity modes, No Slow amounts, GUI Move speeds and Scaffold controls (#8): vstxx.
- Spawner Alerts and Item ESP names, counts, tracers and item filter (#11), Anti Vanish with per-server watchlists
  and the F1 fix for the open menu (#10), and this roadmap (#9): Rust (zackmsa777-a11y).

Old Solstice branches need rebasing or porting by hand before review.

## Next focused contributions

1. **Tighten shared scan accounting.** `Sweep` spreads Block, Hole, Tunnel, Void and Light scans over ticks and keeps
   the nearest results. Its nominal 12,000 allowance is checked after whole x slices: a full-height range-48 Block scan
   can read 18,818 blocks in one tick, and independent scans share no allowance. Keep nearest-result retention and
   the configurable Max when adding finer cursors and shared limits; measure frame times before claiming gains.
2. **Improve HUD control and information.** Start with placement for the watermark, module list and coordinates. Add
   armour durability and optional low-health feedback separately; then bound-module state, effects and cooldowns.
   Test input release, resized windows, F1/F3 and profile persistence.
3. **Colours.** List settings are already picked from the game's registries (blocks, items, entities). RGBA editing
   still needs palette migration, a preview and Apply/Cancel.
4. **More local alerts.** Spawner Alerts is the first; nearby-player arrivals with friend filters and some hysteresis
   at the range's edge could follow. Keep them apart from the server's own join/leave messages. Weather alerts need a
   rule for when Weather Changer overrides the weather you see.

Each code PR should state its base, its tests and its build status. Rendering and input need evidence from the game
itself; unit tests and a compile alone don't show appearance or usability.

## Reuse existing capabilities

Sunset already has broad combat, movement, inventory and rendering modules, including Item ESP, movable menu panels,
palette colours, Middle Click Extra, Auto Weapon and a Target HUD. New variants should say what workflow is missing.
Horizon itself has auto reconnect, Discord presence and a minimap: reuse them where they fit.

## Later research

- Shield-aware weapon selection and item-use binds need coordinated inventory ownership, cooldowns and reliable slot
  restoring; extend Auto Weapon and Middle Click Extra where possible. Sprint resets and knockback jumps need verified
  attack timing and multiplayer tests.
- Crystal and Anchor Aura and coordinated mining need damage and exposure calculations, placement and action ordering,
  and multiplayer tests.
- Vehicle flight and liquid walking need verified collision and vehicle hooks and server behaviour. Name protection
  needs consistent chat, tab and nametag coverage, rich text and an unchanged outgoing identity.
- Activity and chunk finders need bounded scans and measured false positives. Model outlines, post-processing and
  custom skies need original wgpu work.
- Server-specific tools (regions, economy and teleport macros) need specified server behaviour and a test server.

## Research credit

Planning used static behaviour research from Anubis, whose original-author metadata names **4ldenz**, alongside
Sunset and Horizon's source. This document describes functional requirements only, not Anubis source or assets; the
original authors keep their rights. Keep attribution for any material actually reused.
