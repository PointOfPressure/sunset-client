# For Claude Opus 5.5 — review request

Please review the native performance changes against upstream `23bdc11`.
The production source reviewed and built for R5 is `b3807ef`; later evidence
and documentation commits may follow. This is a review request, not a claim
that Claude Opus 5.5 has already reviewed or approved the changes.

The intended result is less recurring allocation, lookup, scan and shader
work while preserving module behavior. Retain the upstream Anti Vanish,
F1/menu handling, item ESP, Spawner Alerts, picker, movement and server-kick
fixes. Check the final PR diff against upstream as well as the earlier
performance implementation so a merge cannot silently drop those features.

## Six review targets

1. **Snapshot ownership and invalidation** — In
   `mixins/sunset/runtime.rs`, inspect `shared_modules`, `shared_snapshot`,
   `shared_friends`, `EntityScratch::take` and `EntityScratch::recycle`.
   Confirm edits, profiles, hold binds and plugin registration refresh views;
   old readers remain immutable; nested Canvas callbacks cannot share live
   scratch buffers or retain a cache mutex. Check disabled-module costs too.

2. **Item ESP and text scale** — In `mixins/sunset/runtime.rs`, inspect
   `ItemList`, `shared_item_list`, `item_name_into`, `world_overlays` and `hud`.
   Compare All/Whitelist/Blacklist behavior with `sunset_util::listed`,
   including whitespace, namespace prefixes, case and empty tokens. Preserve
   custom/translated names, counts, tracer output and boxes near the camera
   plane. Confirm GUI scale changes do not churn the font atlas or corrupt
   existing text/geometry when the batch scale is restored.

3. **World scans and spawner state** — In `mixins/sunset/world.rs`, inspect
   `tick`, `spawner_label`, `spawner_alerts` and `spawner_labels`; also inspect
   `Sweep::nearest` and `clear_line` in `mixins/sunset/util.rs` and `side` in
   `mixins/sunset/render.rs`. Check ordered spawner labels stay consistent with
   membership, including disable/world/dimension resets, ordinary and trial
   spawners, and missing spawn data. Preserve nearest-result limits, ray sample
   semantics, scan budgets and tunnel classification.

4. **Picker equivalence** — In `mixins/sunset/picker.rs`, inspect
   `from_names`, `readable`, `rows` and `toggled`. Confirm borrowed-name
   deduplication and cached search keys retain registry IDs, readable sorting,
   entity groups, chosen-first rows and search behavior. Review the native
   picker test evidence, not only the extracted helper tests.

5. **Target selection and borrowed metadata** — In
   `mixins/sunset/target_order.rs`, inspect `candidates` and `closest_visible`;
   follow callers in `aura.rs` and `combat.rs`. Verify first-winner ties,
   blocked nearer candidates, look-only selection, friends, age/name filters
   and wall-range rules. Check direct-mob selectors in `world.rs`, Auto EXP
   borrowing in `combat.rs` and Fast Use item lookup in `player.rs`.

6. **Sky work, fallback and resource lifetime** — Review
   `mixins/sunset/atmosphere_shader.rs`, `atmosphere_renderer.rs`,
   `atmosphere_flags.rs` and their hooks in `horizon-mixin.toml`.
   Check derivative uniformity, conservative filtered star/planet bounds,
   seams, horizon/day blending and all quality tiers. Preserve native fallback
   for excluded contexts and validation failure. Check cache bounds, surface
   release and normal-exit cleanup, especially GPU handles in thread-local
   storage outliving renderer/device teardown.

## Evidence and its limits

- The **55 standalone tests passed**. They exercise production helpers and
  shader validation; they do not establish full-client FPS or GUI correctness.
- The **R5 compile check and linked native build passed**, with no compiler
  warnings reported for the compile check.
- **41 distinct module checks and 59 sky checks passed** on the R5 native
  client across the recorded isolated runs. This covers movement, names/filters,
  spawner alerts, picker, GUI editor/binds/F1, sky restoration, an actual native
  shader-pack conflict, settings persistence and an aspect-changing restart.
  [performance-checks.json](performance-checks.json) preserves first-run failures,
  corrected reruns, source/binary hashes and explicit coverage gaps. A restart
  notification repeated 13 checks; they passed and are not counted twice.
- Native clients and the validator exited zero. The initial full-suite wrapper
  and duplicate-count assertion returned nonzero; the evidence explains both.
  Intermittent zero-frame offscreen readings are excluded from timing claims.
- The [GPU benchmark](../tools/shader-gpu-bench/README.md) is a synthetic
  shader workload using a 1,088-byte Globals buffer. Driver/GPU clock changes
  (DVFS), warm-up and run order affect timings. It supports only the recorded
  workload comparison, not a full-mod FPS improvement or a portable guarantee.
- [performance.md](performance.md) describes the changes and reproduction
  commands. Reconcile its final counts and claims with the evidence artifact.

Please report actionable findings with file, line, a concrete triggering case,
the observed or expected regression and a focused fix when clear. Separate
confirmed defects from coverage gaps or hypotheses. Prefer reproducible
findings over broad rewrites or unrelated feature changes.
