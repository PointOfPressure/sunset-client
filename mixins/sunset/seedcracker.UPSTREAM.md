# SeedCrackerX provenance and local changes

Vendored on 2026-11-16 from [luhnox/horizon-seedcrackerx](https://github.com/luhnox/horizon-seedcrackerx),
version **0.2.3**, commit **44f0c9d5dda1a0cb9acb99b582e476b5c81c3a09** (2026-11-05).
This is luhnox's MIT Rust port of SeedCrackerX by KaptainWutax, maintained by 19MisterX98 and contributors.
The exact upstream copyright and permission texts remain in `seedcracker.LICENSE` and `seedcracker.NOTICE.md`;
neither file is edited. All three files ship in Sunset's `.hnmod`.

Paths below are relative to the upstream repository at that commit. The 22 engine modules are copied from
its pure Rust library; the Horizon game adapter, network/database integration and hooks are Sunset-owned.

| Sunset file (`mixins/sunset/`) | Upstream source |
| --- | --- |
| `seedcracker_biome.rs` | `src/core/biome.rs` |
| `seedcracker_config.rs` | `src/config/config.rs` |
| `seedcracker_cracker_data.rs` | `src/cracker/data.rs` |
| `seedcracker_cracker_storage.rs` | `src/cracker/storage.rs` |
| `seedcracker_cracker_time_machine.rs` | `src/cracker/time_machine.rs` |
| `seedcracker_database.rs` | `src/config/database.rs` |
| `seedcracker_dimension.rs` | `src/core/dimension.rs` |
| `seedcracker_feature.rs` | `src/feature/feature.rs` |
| `seedcracker_features.rs` | `src/feature/features.rs` |
| `seedcracker_finder.rs` | `src/finder/finder.rs` |
| `seedcracker_finder_piece.rs` | `src/finder/piece.rs` |
| `seedcracker_finder_structures.rs` | `src/finder/structures_dsl.rs` |
| `seedcracker_finders.rs` | `src/finder/finders.rs` |
| `seedcracker_lang.rs` | `src/config/lang.rs` |
| `seedcracker_lcg.rs` | `src/core/lcg.rs` |
| `seedcracker_level.rs` | `src/finder/level.rs` |
| `seedcracker_mth.rs` | `src/core/mth.rs` |
| `seedcracker_rand.rs` | `src/core/rand.rs` |
| `seedcracker_seeds.rs` | `src/core/seeds.rs` |
| `seedcracker_sha256.rs` | `src/core/sha256.rs` |
| `seedcracker_version.rs` | `src/core/version.rs` |
| `seedcracker_worldgen_random.rs` | `src/core/worldgen_random.rs` |
| `seedcracker_packet_hook.rs` | New Sunset lifecycle hook; no copied upstream file |
| `seedcracker_world.rs` | New Sunset decoded-chunk adapter; implements `src/finder/level.rs`'s interface |
| `seedcracker.rs` | New Sunset controller/Host; replaces the integration role of `src/game/seedcracker.rs` |

## Changes to the copied engine

- **Module names:** flat injected modules use `sunset_scx_*`; all `super::scx_*` references in the copied
  modules become `super::sunset_scx_*`. `seedcracker_level.rs` also renames its bare `scx_biome::by_name`
  reference. `horizon-mixin.toml` records the file/module mapping so upstream and Sunset can coexist.
  Files not listed below otherwise retain the upstream engine implementation.
- **Host and metrics (`seedcracker_cracker_data.rs`):** add default cancellation/progress callbacks,
  `SearchStage` and `SearchProgress`; suppress cancelled progress; expose `ScheduledSet::total_size()`
  including scheduled additions.
- **Storage and event caps (`seedcracker_cracker_storage.rs`):** cap observations at 4096 (including pending
  data and pillars) and scheduled events at 4097. Serialize additions so concurrent producers cannot exceed
  the cap. Keep one stable time machine per run; latch retirement permanently. Atomically claim dispatch,
  take scheduled events before spawning, use a named worker and an RAII running guard, handle spawn failure
  and stop dispatch after cancellation. Expose `observation_count()`.
- **`clear()` fix (`seedcracker_cracker_storage.rs`, `seedcracker_cracker_time_machine.rs`):** permanently
  retire/terminate the old run, synchronize additions, clear the hash, all scheduled/observed data and solver
  state; do not replace the time machine underneath a still-running worker. New runs use new storage/Host.
- **Cancellation and worker limits (`seedcracker_cracker_time_machine.rs`):** check Host/storage retirement
  throughout pillar, lifting, structure, reduction, decorator, hash and biome searches, including before
  publishing results. Cancel sibling workers on panic. Add atomic dispatch claiming, bounded default thread
  use and a 1–8 thread clamp. Expose structure/world candidate count accessors without cloning seed sets.
  Report per-phase checked/total progress; dispatch owns the running flag across all queued events.
- **Hash priority and optional hashes (`seedcracker_cracker_time_machine.rs`):** try an available hash before
  decorator/biome recovery, retain mismatches, and treat `Some(0)` as a real hash rather than missing data.
  A decorator singleton cannot block a later hash search. Only a singleton verified against the current
  hash is delivered through `world_seed_found`; unverified seeds remain candidates.
- **Cancellable hash expansion (`seedcracker_seeds.rs`):** add `world_seed::from_hash_cancellable()`, checking
  every 256 candidates and once after enumeration, discarding partial results on cancellation. The original
  `from_hash()` delegates to it with cancellation disabled.
- **Database lookup (`seedcracker_database.rs`):** add `get_seed_by_hash()`; a matching address alone is not
  verification. The adapter independently rehashes every returned seed.
- **Finder cancellation (`seedcracker_level.rs`, `seedcracker_finder.rs`, `seedcracker_finder_piece.rs`,
  `seedcracker_finders.rs`):** add a default `Level::cancelled()` hook and check it/storage retirement before
  dispatch and within scans/templates, marker collection, gateway descent and data publication.
- **Neighbour checks (`seedcracker_finder_piece.rs`, `seedcracker_finders.rs`):** template cells must belong
  to loaded chunks; all End pillar marker chunks must be present; shipwreck neighbour checks and dungeon
  floor checks reject unloaded chunks; dungeons require their surrounding chunks. Missing data cannot
  satisfy air/template predicates.

## Sunset integration

`seedcracker_world.rs` owns immutable decoded-chunk snapshots, bounded registry/biome tables, conservative
block/tag/entity predicates and quart-biome conversion. `seedcracker.rs` owns per-world/run identity,
cancellation and retirement/joining, bounded scan queues, late-decoration/neighbour rescans, the local save
shortcut, optional read-only bounded database access (off by default), progress and independent final hash
verification. Its identity and Host carry `Option<i64>` hashes, keeping a missing hash distinct from `Some(0)`.
Only verified automatic results enter its 64-entry process-memory LRU cache; matching hashes
can reuse them after Login/Respawn/reconnect. Manual seeds and profiles never populate or persist that cache.
`SUNSET_SEEDCRACKER_FORCE_CRACK=1` bypasses local-seed shortcuts and cache reuse for diagnostic runs.
The optional diagnostic-only `SUNSET_SEEDCRACKER_START_DELAY` (0–300 seconds, default 0) allows collecting a
stronger batch before invoking the unchanged solver; it is ignored unless FORCE_CRACK is enabled.
`seedcracker_packet_hook.rs` resets observations and Ore Sim at Login/Respawn/Disconnect boundaries.
`ore_sim.rs` consumes the resulting seed, preserves manual precedence and reports its origin. The standalone
regression harness is `tools/seedcracker_tests.rs`; adapter/world tests live in the new adapter modules.

The inherited feature tables still identify themselves as Minecraft 1.21.3. General biome-source recovery,
lattice recovery, database submissions, authentication and anti-xray bypasses are not integrated into Sunset.
