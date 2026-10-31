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
- Item ESP caches parsed filters with original case/namespace semantics.
  Spawner Alerts uses hashed membership alongside ordered discoveries; picker
  names are deduplicated before allocation with cached search keys.
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

The standalone suites have **55 passing tests**: 13 shader/config tests,
20 runtime/cache/input/filter tests, 13 world/geometry tests, 3 targeting tests
and 6 discovery/picker equivalence tests.
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

The final build passes **41 distinct module checks and 59 sky checks**, with
zero compiler warnings and clean client exits. Results combine the full run
and the focused corrected-fixture run; repeated checks are counted once.

[performance-checks.json](performance-checks.json) records source pins, build
hashes and the final native results. Tests use fresh isolated singleplayer data,
private resources and generated files. Physical keyboard input and external
multiplayer are outside this validation. Upstream movement, Anti Vanish, F1 and
discovery features through `23bdc11` are retained; PR 12 remains separate.

A surface-free GTX 1650 Ti/Vulkan benchmark at 2000x1125 measures sky-only
render-pass GPU time, including identical attachment clear/store work.
Balanced medians: Space **1.520 to 0.735 ms**, Aurora **0.675 to 0.461 ms** and
Ringed Planet **1.685 to 0.854 ms**. Performance-tier reductions are 51.6%,
46.2% and 53.1%. All eight paired rounds improve every case. There are 6,272
valid timed passes, with at least 600 ms of alternating warmup before each
64-sample block. Every variant renders at the full native framebuffer size.

Both variants receive identical synthetic 1,088-byte Globals, not a captured
game buffer. Mobile graphics clocks varied 660-1905 MHz, temperature 50-60 C,
without clock locking or normalization. These observations exclude terrain,
HUD, game logic, presentation and CPU time. They establish a local shader
improvement, not a whole-game FPS gain. An uncapped game can still keep its
GPU busy by rendering more frames. The
[reproducible harness](../tools/shader-gpu-bench/README.md) records the method.

Early lab runs used per-command process launches, Linux-only image tooling,
monitor-based GUI dimensions and Windows-converted profile newlines. Corrected
fixtures use persistent official control, native GUI dimensions, Pillow,
canonical profile bytes and pause-aware waits. Item-label exact-color tests
select Minecraft's bitmap font; a native capture also shows the default-font
label. Zero-frame benchmark samples are excluded from performance claims.
The final record distinguishes first-run failures from corrected reruns.

[Claude Opus 5.5 review request](review-opus-5-5.md) identifies concrete
functions, correctness risks and the evidence to inspect.
