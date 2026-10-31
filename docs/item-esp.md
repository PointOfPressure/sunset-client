# Dropped-item ESP

The existing **ESP > Items** switch now supports names, stack counts, exact item-ID filters and tracers. It still uses ESP's drawing mode, Items color, Range and Fade distance. Items remains off by default; when enabled, boxes remain on and labels/tracers remain off.

Turn on ESP and item labels with chat commands:

```text
.t esp
.set esp items true
.set esp item labels true
.set esp item tracers true
```

**Item boxes** can be turned off for labels/tracers alone. **Items max** bounds the nearest visible matching drops (default 256, maximum 1024), independently of the existing entity Max. **Item label max** bounds how many nearest candidates can do name/layout work (default 32, maximum 64). Labels try four nearby positions, then disappear if they would overlap. **Item label background** controls the small dark backing. Names use the host's native localized tooltip-name API, including custom names; stacks larger than one append `x<count>`. Names are single line, stripped of formatting/control characters and limited to 48 Unicode characters before the count.

Use **Item filter** = `All`, `Include` or `Exclude` with **Item IDs**. Separate IDs with commas or whitespace. Bare IDs mean `minecraft:`; namespaces are matched exactly. IDs are normalized to lowercase; tags, wildcards and malformed IDs are ignored. Unknown but syntactically valid IDs are kept as exact IDs, so they can match content supplied by a mod. An empty/invalid Include list shows nothing; an empty/invalid Exclude list excludes nothing.

```text
.set esp item IDs diamond,netherite_ingot,example:rare_gem
.set esp item filter Include
.set esp items color Cyan
.set esp items max 64
.set esp item label max 16
```

Only the native `game.dropped` list is visited. Dead/empty/air drops, invalid positions, drops outside the 3D range and off-screen drops are rejected before name work. The nearest-result budget admits only drops that can draw an enabled primitive: an on-screen projected label/tracer anchor or a box with projectable geometry in its configured drawing mode. A bounded heap keeps the nearest matches without sorting the full world list. The heap, sorted results and formatted-text buffer are reused; overlap rectangles use fixed storage. Native name lookup returns an allocated string and is called at most Item label max times per frame, only when labels are enabled. No entity/name entries persist between frames, so pickup, death, disconnect and dimension changes leave no cached item markers. Disabled Items does not enter the renderer or lock its scratch buffers.

## Validation

Offline tests require only Rust and its platform linker, without game assets or network access:

```powershell
rustc --edition=2024 --test tools/item_esp_tests.rs -o "$env:TEMP/sunset-item-esp-tests.exe"
& "$env:TEMP/sunset-item-esp-tests.exe"
```

The harness imports the actual selector, filter, formatter, layout, renderer and native box-drawing module. Its recording Canvas uses the native camera's perspective/frustum formulas and projection near plane. It covers exact namespaces/unknown IDs, filter invalidation, nearest order/ties/hard caps, inclusive 3D range, dead/empty stacks, Unicode/custom-label handling, count retention, overlap, separate label/result caps, native name-work bounds and empty-world/filter transitions. Cap-one regressions exercise off-screen label/tracer anchors, unprojectable near-plane boxes and combinations of enabled primitives; a mode/shape/position sweep checks box preflight agrees with the actual drawing code. The pinned-host build and isolated live checks passed; see [validation and screenshots](discovery-validation.md).

For a disposable native test world, enable labels/tracers as above, face dropped diamond/stone stacks at different distances, then check Include/Exclude, change Items max to 1, pick up/remove the nearest stack and change dimensions. Verify labels follow the remaining nearest stacks and no old markers remain. Set Item boxes false and change Items color to check tracer/label colors separately. Return Item filter to All and Item labels/tracers false to recover the original box-only appearance.
