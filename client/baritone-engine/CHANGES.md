# Changes from horizon-baritone 41e1b72

2026-11-16 — Sunset contributors, integration requested by Rust (zackmsa777-a11y)

- Preserve upstream source and the 180 pure tests and oracle fixtures. Correct
  the stale standalone lock/package version through a new locked manifest.
- Build `sunset_baritone_engine` as an independent cdylib and rlib. The game
  links only the MIT loader, and loads the engine through the frozen C ABI v1.
- Remove the unnecessary `mut` in upstream `cached_world::pack`; no warning
  suppression is used for the upstream warning.
- Add a Game-free, read-only host adapter for real block facts, tools, ray
  tracing, build bounds and action intents. Keep upstream movement/A* costs and
  PathExecutor. Searches expand at most 2048 nodes per pre-tick and restart on
  revisions. No host pointer, world pointer or callback survives dispatch.
- Retain only the engine's logical prior aim across Sunset's restored camera,
  so upstream obstacle mining can pass its prior-aim gate; validate actual
  attacks with fresh live-eye rays. Drop failed segments and replan from current
  feet up to three times per navigation goal, preserving executor diagnostics
  in status and stderr. Block revisions do not replenish that retry budget.
- Add goto and bounded visible-world mining orchestration, plus
  PredictedMineProcess for Ore Sim approach, protected exposure, reveal,
  exact-kind verification, ordinary mining and server-settled confirmation.
  Bound target attempts, radius, time, reveal wait, retries and consecutive
  misses; never write predicted ore into world/collision observations.
- Keep raw-pointer C transport in the MIT loader source tree, including the
  engine-side entry adapter compiled by explicit relative source path. All
  engine algorithms deny unsafe code. Original game input/render hooks remain
  inactive. V1 does not integrate builder, farm, follow or elytra commands.
- Keep one copy of the upstream source: this crate's `src/lib.rs` compiles the
  pure modules and upstream's tests from `mixins/baritone/upstream/src`, the copy
  the game builds, and replaces upstream's standalone `Cargo.toml`, `Cargo.lock`
  and `src/lib.rs` (2026-11-17).
- Add focused protocol, state-machine, host integration and transport tests.
- Sunset 2 (28 Nov 2026): the companion is an `.hmod` mod; the one upstream copy moved to
  `mixins/baritone/src/upstream` and the loader's transport to `mixins/baritone/src`, so the relative source paths
  in `src/lib.rs` follow them. The engine ships as a library file beside the game (no longer embedded in the
  companion); see `mixins/baritone/PORTING.md`.

Baritone engine credit: luhnox/horizon-baritone
41e1b720dc2028d06618cb8f7cf55f2697bba83c, derived from cabaletta/baritone 1.21.4
(leijurv and contributors); LGPL notices preserved.
