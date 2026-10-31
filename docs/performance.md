# Native module performance

This pass covers the shared runtime, world/entity work and native sky rendering.
The Horizon and Mixins revisions remain pinned; no engine-source patch is needed.
Normal module defaults, framebuffer resolution and rendering modes are retained.

## Reduced recurring work

- Tick, HUD and GUI readers share immutable module snapshots. A live-value
  comparison refreshes the cache after GUI edits, key holds, profiles or plugin
  registration. Named dispatch uses an index; the existing snapshot API remains.
- ESP, Tracers and Nametags reuse candidate and text buffers. Friend snapshots
  are shared and player names are borrowed. Candidate order, visibility and Max
  accounting retain their previous behavior. World labels and GUI text share
  one font scale, avoiding repeated glyph-atlas resets.
- Zero-alpha ESP fills emit no invisible faces. Radar computes its heading
  once per frame. Block/tunnel/light scans avoid duplicate reads, and large
  sweep results partition before sorting the retained nearest entries.
- Visibility rays inspect each sampled voxel once. Entity actions filter mobs
  before creating owned Targets, removing repeated searches for their metadata.
  Kill Aura keeps its source mob, and its look-only path borrows just one item.
  Aim Assist skips collision rays for candidates that cannot beat a visible
  angular winner. Auto EXP and Fast Use borrow their item metadata.
- Atmosphere defers ordinary sky shading until it contributes to the blend,
  rejects empty star/planet support early and reduces procedural noise work.
  Performance/Balanced/Detailed specialize 2/3/5 nebula octaves, 1/2/3 star layers
  and 1/2/3 aurora curtains. The nebula pattern changes; palettes and world
  anchoring remain. Every tier renders the full native framebuffer.

## Correctness checks

The standalone suites have **45 passing tests**: 13 shader/config tests,
16 runtime/cache/input tests, 13 world/geometry tests and 3 targeting tests.
The shader suite includes 18 specialization cases within one test, not 18
additional tests. The world suite compares 1,125 ray cases and 625 tunnel
configurations. Tests cover stale edits, nested buffer ownership, target ties,
transparent geometry, conservative celestial culls and small-framebuffer stars.

Warm synthetic allocation checks record 5,140,000 to zero allocations for
20,000 module-snapshot reads, and 2,840,000 to zero for 20,000 overlay
buffer/profile cycles. These counts describe those operations only, not the
whole client. An eight-block sampled ray drops from 63 world reads to 9.

Run the source-extracted tests with a compiler matching the pinned build:

```sh
python tools/runtime_perf_tests.py --rustc /path/to/rustc
python tools/perf_visual_tests.py --rustc /path/to/rustc
cargo test --locked --manifest-path tools/performance-tests/Cargo.toml
```

Set `SUNSET_TERRAIN_WGSL` to the pinned Horizon checkout's
`crates/horizon-client-render/src/terrain.wgsl`, then run:

```sh
cargo test --locked --manifest-path tools/atmosphere-tests/Cargo.toml
```

## Native validation and timing

Native validation and paired timing records are recorded in
[performance-checks.json](performance-checks.json). GPU measurements are total
frame timestamps on one machine; CPU summaries include the client frame work.
They are not an isolated shader timer or a portable FPS guarantee. High GPU
utilization at a high frame cap alone does not establish a regression.

The controlled comparison keeps the camera, world seed, time, native resolution,
frame cap and module settings matched, samples each state three times and checks
restored native rendering before and after sky modes. A 96-mob scene exercises
HUD/ESP/Nametags, with the GUI closed and at scale two. No existing worlds,
real accounts or multiplayer servers are involved.

The audit also reviewed pending PRs 11 and 12. Their spawner/index and Item ESP
work is bounded; their additional modules remain separate pending contributions.
This change does not claim those unmerged features are installed or tested here.
