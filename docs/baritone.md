# Baritone and Ore Sim

Sunset includes the original horizon-baritone runtime: its commands, processes, movement, cache, schematics,
selections, waypoints, settings page and input/render hooks. The original command prefix is `#`: start with
`#help`, `#set list` and `#proc`. This includes `#build`, `#farm`, `#follow`, `#explore`, `#tunnel` and `#elytra`,
alongside the original navigation, mining, pause/resume and selection commands.

Sunset's `.b` commands also provide its bounded mining controller, including Ore Sim predictions. That controller
uses a replaceable engine DLL. The full runtime and the Ore Sim controller share Sunset's movement ownership and
stop/Panic handling. The full runtime's own settings and rendering remain available instead of being reduced to
the smaller Ore Sim control panel.

## Install

The Baritone-enabled `sunset-<version>.hnmod` contains the full runtime and the Ore Sim engine payload. Install
that one package through Horizon's native-mod installation flow, then rebuild/restart the Native mods instance.
The package must match Sunset's recorded Horizon and mixin pins. A separate Baritone plugin is unnecessary.
Do not enable the standalone `baritone` mod or an older `sunset_baritone` companion beside the integrated package:
they would duplicate the same source modules and hook points.

For a manual installation, copy the integrated Sunset package into the selected instance's `horizon-native-mods/`
folder, enable Sunset in the launcher and rebuild Native mods. Keep one installed Sunset package. The included
runtime retains the internal data namespace `sunset_baritone`; this is not a second installation step.

For a local source build, use an optimized Horizon profile for playing. An ordinary `cargo build` produces an
unoptimized development client; Baritone's cold chunk scans can then block the render thread for seconds.
After `horizon-mixins apply`, run `tools/build-playable.ps1 -StageDirectory <stage> -Offline` to build the
client and control tool with Horizon's optimized `fast` profile. `-Profile release` is also supported. The
script copies matching executables and writes a build/hash receipt under `dist/playable/<profile>/`.

Keep the matching `baritone-engine-<engine-version>-source.zip` with the integrated package when distributing it. The source
archive contains the modified engine, full runtime source, Cargo lockfiles, build script, loader/interface and source notices.
The DLL is bundled under `mixins/sunset/data/baritone/engine/x86_64-pc-windows-msvc/sunset_baritone_engine.dll`; its import
library and debugging symbols are unnecessary for playing.

## Sunset mining commands

- `.b` or `.b status`: availability, job state, source, target and mined/skipped/mismatch/unconfirmed counters.
- `.b mine`: use the module's Source and Ores preferences for a new mining job.
- `.b goto <x> <y> <z>`: start a Baritone route to an absolute block position.
- `.b mine <ore[,ore...]>`: mine visible ores of the selected kinds.
- `.b mine oresim [ore[,ore...]]`: mine Ore Sim predictions; an explicit list overrides the Ores preference for this run.
- `.b targets [limit]`: inspect a deterministic prediction sample, including source identity and live block state;
  default 8, maximum 64. This diagnostic does not start a job.
- `.b stop`: cancel the current job and release its input immediately, leaving the module armed.
- `.b off`: stop and disable the module. `.b help` lists supported commands and engine notices.

`.baritone` is the long alias. Existing Sunset `.goto` remains available. Explicitly starting Baritone, `.goto` or
Auto Walk hands movement to that owner and cancels the previous Sunset owner. Jobs also stop on manual input,
ineligible screens, conflicting automation, disable, death, world/dimension/source changes and Panic in the bounded
Ore Sim controller. Its jobs and paths are never restored from profiles or resumed on reconnect. Physical
keyboard/controller input is preserved when synthetic input is released. The full original runtime keeps its
upstream behavior of walking with chat or menus open, while still observing disable, death, world changes,
movement handoff and Panic.

Other `.b` verbs, such as `.b follow` and `.b farm`, are forwarded to the full original command layer. `#` commands
and the native `/b` and `/baritone` aliases use that layer directly. Full-runtime commands arm the Baritone module;
`.b stop` and `.b off` cancel both drivers. The original settings page remains under Options > Mods.

## Mine the predictions

1. Turn on **Ore Sim**. Use `.seed auto` for SeedCracker's current-world verified seed, or `.seed <seed>` for a
   manual seed. A nonempty manual Seed retains its normal override priority. Baritone does not enable database
   fetching, change the seed or start another seed search.
2. Set Baritone's **Ores**, **Min Y**, **Max Y**, **Radius** and **Max targets**, then inspect `.b targets`.
3. Equip suitable tools and run `.b mine oresim`. With an unresolved automatic seed or an unfinished replay,
   the controller waits with its keys released.

Bare `.b mine` follows the module's **Source** and **Ores** preferences. Explicit visible/Ore Sim command arguments
override those preferences for that job.

Defaults are diamond, absolute Y -64 through 319, radius 128 and at most 64 attempted targets. Radius is a 3D
distance to block centres from the job's fixed starting position; walking does not move this boundary. Radius
is limited to 8..256 and target attempts to 1..4096. Target Y is clamped to the dimension's build range; an
inverted or wholly out-of-range band is rejected. Paths may use heights outside the requested ore band.

Kinds are `coal,copper,iron,lapis,redstone,gold,diamond,emerald,quartz,debris`. Corresponding Minecraft block IDs,
including deepslate ores, `nether_gold_ore`, `nether_quartz_ore` and `ancient_debris`, are accepted. Families are
exact: an iron ore cannot verify a predicted diamond. Unknown/empty lists are rejected.

Mining filters are independent of Ore Sim's **Ores**, **Hide shown ores** and drawing **Max**. Only loaded chunks
contribute candidates; unloaded data is unknown. Predictions are deduplicated by position and sorted by distance
with coordinate/kind tie breaks. Already air, unrelated live blocks and attempted/rejected positions are removed
before the target cap. Replay identities change on restart even for the same world and seed, invalidating old work.

Predictions remain candidates until verified. The engine approaches geological cover using actual live block
facts and ordinary mining input. It attacks the final ore only after the server exposes the selected kind and
ordinary reach/tool checks pass. Client-predicted air alone does not count as success: removal must settle.
The ledger distinguishes mined, skipped, mismatch and unconfirmed targets. Wrong seeds, mined terrain and
unrevealed coordinates consume a bounded attempt/time/miss budget; an empty source never starts endless exploration.

## Ore Sim controller drawing

**Path**, **Goal** and **Targets** independently control the overlays. Palette colours default to Sunset Orange
and Gold. **Shape mode** offers Lines/Sides/Both, with Sunset's normal light fill, **Fill opacity**, **Line width**
and a separate **Render cap**. Target boxes use live contextual selection shapes where available. These overlays
use the same projection and strokes as Sunset's ESP and Block Overlay, drawn over the frame without depth testing.
F1/hidden GUI and the Sunset menu hide the drawings. Drawing preferences do not change mining candidates.

## Build and replace the engine

Keep work, Cargo caches and temporary files on a drive with enough space. In the Windows development setup:

```powershell
$env:TEMP = 'D:\swarm-tmp'
$env:TMP = $env:TEMP
$env:CARGO_HOME = 'D:\cargo-home'
Set-Location D:\sunset-work\sunset-client
powershell -NoProfile -ExecutionPolicy Bypass -File tools\build-baritone.ps1
python tools\pack.py --with-baritone
```

The opt-in packer requires Python 3.11 or newer. The build script builds the independent Ore Sim engine; its DLL
remains replaceable. The full original runtime is compiled into Horizon from the included source modules.
Ordinary `python tools/pack.py` includes the full runtime when its sources are present. `--with-baritone` selects
that mode explicitly; `--without-baritone` is a development-only smaller subset. The built-in package consumes the
prebuilt engine, checks matching metadata/ABI declarations, and preserves notice
bytes. `--baritone-dll <path>` explicitly selects another already built compatible x86_64 engine for packaging.
Build that DLL from the source tree being packaged so the corresponding-source archive matches it.
See the engine's own build instructions and dated change/provenance files in the source archive.

To run a modified compatible engine, close the game and place its DLL at:

```text
<game.native_data_dir("sunset_baritone")>/engine-override/sunset_baritone_engine.dll
```

This is Baritone's internal native-data namespace within the selected game instance. The loader's error/status
reports identify loading problems. Normally it is
`<instance>/horizon-mods/.horizon-mod-data/sunset_baritone/engine-override/sunset_baritone_engine.dll`.
The override takes precedence over the bundled DLL. An invalid override fails
closed; remove or fix it to return to the stock engine. Restart after replacement. No game rebuild, official
signature or official engine hash is required for a compatible modified library. Architecture, ABI size/version
and capabilities must match [the interface](../mixins/baritone/interface/protocol-v1.md).

Without an override, stock bytes are extracted once under Baritone's native-data directory at
`engine/<engine-version>/<asset-hash>/sunset_baritone_engine.dll`. Existing modified extractions and override
files are preserved. Prefer `engine-override` so a bundled-engine update cannot select a different stock path.

## Limits and validation

The full runtime carries the supplied horizon-baritone source and all of its native hook points, including builder,
farm, follow, exploration, Elytra, parkour, placement, commands and UI. Its inherited limitations remain those of
the supplied port: in particular, the semitransparent cached-chunk drawing controlled by `renderCachedChunks`
is not implemented there. This does not claim parity with Java-only machinery beyond what the supplied port has.

The separate bounded Ore Sim controller still has its narrower host contract. It uses loaded live chunks;
placement remains unavailable through that controller while the host's placement-feasibility query is unsupported,
even if **Allow place** is selected. This does not disable the original runtime's own bridge/pillar placement.
Generation changes, custom ores, stale terrain and anti-xray reveal policy can make predictions wrong. A server
that never reveals the ore cannot satisfy verification; that target is skipped/unconfirmed. This is no promise
of an anti-xray or anticheat bypass. Other platforms require matching engine builds.

Focused checks live in `tools/verify.py`: `Baritone: unavailable without companion`, `Baritone: prediction targets`,
`Baritone: Ore Sim mining`, `Baritone: stop releases keys`, `Baritone: goto and stop`, `Baritone: ownership and boundaries`,
`Baritone: rendering and config`, and the final-session `Baritone: Panic cancels`. Run through the Windows
headless helper in a fresh disposable normal-terrain world. The legacy unavailable check needs a `--without-baritone`
development binary; the remaining engine checks need the built-in runtime. A passing ordinary single-player run demonstrates real
prediction mining; masked-server reveal/rollback and remote packet timing need their own tests. No four-server
compatibility result is implied by the existence of these checks.

## Source and licences

The full runtime uses the operator-supplied [horizon-baritone](https://github.com/luhnox/horizon-baritone) source by
[luhnox](https://github.com/luhnox), a Rust port of [Baritone](https://github.com/cabaletta/baritone) 1.21.4 by
leijurv and contributors. Its supplied [MIT permission text](../mixins/baritone/upstream/LICENSE) is preserved exactly
with that source in the native package and corresponding-source archive. Its [README](../mixins/baritone/upstream/README.md)
and [class inventory](../mixins/baritone/upstream/PARITY.md) are retained for comparison.

The previously vendored Ore Sim engine remains under its recorded LGPL-3.0 terms with its notices and corresponding
source. Adding the separately supplied runtime does not relabel those older engine files. Sunset supplies input
ownership, lifecycle handling and Ore Sim integration.
Rust ([zackmsa777-a11y](https://github.com/zackmsa777-a11y)) contributed this integration.

The vendored baseline is `41e1b720dc2028d06618cb8f7cf55f2697bba83c`. Preserve the engine's
[LICENSE](../baritone-engine/LICENSE), [GPL COPYING](../baritone-engine/COPYING),
[NOTICE](../baritone-engine/NOTICE.md), [upstream image](../baritone-engine/LICENSE-Part-2.jpg), provenance and dated
change notices. Exact copies, including dependency licence texts, are under the integrated package's
`mixins/sunset/data/notices/baritone/` and in the corresponding-source archive. Individual dependency licence
texts share one native-package text file to fit Horizon's archive-entry limit; their individual source files
are preserved byte for byte in the source archive. That archive preserves the
repository's `baritone-engine/` and `mixins/baritone/` build layout. The independently
written [loader and ABI are MIT](../mixins/baritone/LICENSE). Sunset's licence does not replace the separate
engine's licence; engine modification, replacement and debugging rights follow its upstream terms.

Sunset's own LICENSE is unchanged. The supplied runtime permission text, the existing engine notices and the MIT
loader/interface notices remain with their respective source and payloads.
