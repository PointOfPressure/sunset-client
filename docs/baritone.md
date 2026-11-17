# Baritone and Ore Sim

Sunset includes the original horizon-baritone runtime: its commands, processes, movement, cache, schematics,
selections, waypoints, settings page and input/render hooks. The original command prefix is `#`: start with
`#help`, `#set list` and `#proc`. This includes `#build`, `#farm`, `#follow`, `#explore`, `#tunnel` and `#elytra`,
alongside the original navigation, mining, pause/resume and selection commands.

Sunset's `.b` commands also provide its bounded mining controller, including Ore Sim predictions. That controller
uses a replaceable engine library. The full runtime and the Ore Sim controller share Sunset's movement ownership and
stop/Panic handling. The full runtime's own settings and rendering remain available instead of being reduced to
the smaller Ore Sim control panel.

## Install

Sunset's package (`sunset-<version>.hnmod`) contains the full runtime and the Ore Sim engine for Linux and Windows
(x86_64). Install it the normal way and rebuild Native mods; nothing else is needed. Do not also enable the
standalone `baritone` mod: it would add the same modules and hooks twice. The runtime keeps its data under the
native-data namespace `sunset_baritone`.

Keep `baritone-engine-<engine-version>-source.zip` with the package when distributing it: it holds the engine,
the full runtime source, the loader and ABI, Cargo lockfiles, `tools/pack.py` and the notices.

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

`python3 tools/pack.py` (Python 3.11+) builds the engine from `baritone-engine/` for both targets before packing,
so no prebuilt library is kept in the repository. It needs cargo with the `x86_64-unknown-linux-gnu` and
`x86_64-pc-windows-gnu` targets (`rustup target add x86_64-pc-windows-gnu`) and the mingw-w64 linker
(`x86_64-w64-mingw32-gcc`, Debian/Ubuntu package `gcc-mingw-w64-x86-64`). The Windows engine is a plain C ABI DLL,
so the MSVC-built game loads it like any other. `--without-baritone` packs a development subset without Baritone.

To build the engine yourself: `cargo build --release --locked --target <target>` in `baritone-engine/`. To run a
modified compatible engine, close the game and put the library at
`<instance>/horizon-mods/.horizon-mod-data/sunset_baritone/engine-override/` as `libsunset_baritone_engine.so`
(Linux) or `sunset_baritone_engine.dll` (Windows). It takes precedence over the bundled one and needs no game
rebuild, signature or hash; an invalid override disables the engine with one message until it is fixed or
removed. The architecture, ABI size/version and capabilities must match
[the interface](../mixins/baritone/interface/protocol-v1.md). Without an override the bundled library is extracted
once to `engine/<engine-version>/<hash>/` in the same folder; existing files there are never overwritten.

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
of an anti-xray or anticheat bypass.

Focused checks live in `tools/verify.py`: `Ore Sim seed source`, `Baritone: prediction targets`, `Baritone: Ore Sim
mining`, `Baritone: stop releases keys`, `Baritone: goto and stop`, `Baritone: #goto, #help and stopping`,
`Baritone: ownership and boundaries`, `Baritone: rendering and config` and, last in a session, `Baritone: Panic
cancels`. The mining and prediction checks need a fresh single-player normal-terrain world (its seed is known
locally); the rest also run on servers. `Baritone: unavailable without companion` needs a `--without-baritone` build.

## Source and licences

The runtime and the engine are [horizon-baritone](https://github.com/luhnox/horizon-baritone) `41e1b72` by
[luhnox](https://github.com/luhnox), a Rust port of [Baritone](https://github.com/cabaletta/baritone) 1.21.4 by
leijurv and contributors, brought into Sunset by Rust ([zackmsa777-a11y](https://github.com/zackmsa777-a11y)).
Both are LGPL-3.0, like Baritone: [LICENSE](../baritone-engine/LICENSE), [COPYING](../baritone-engine/COPYING),
[NOTICE](../baritone-engine/NOTICE.md) and [LICENSE-Part-2.jpg](../baritone-engine/LICENSE-Part-2.jpg), unchanged, with
the same files in [mixins/baritone/upstream](../mixins/baritone/upstream/README.md), whose README lists the files
changed for Sunset; the engine's changes are in [CHANGES.md](../baritone-engine/CHANGES.md). The package carries
these notices under `mixins/sunset/data/notices/baritone/` (dependency licences joined in one file to stay under
Horizon's 512-entry limit; the source archive keeps them separate). The loader and ABI are independently written and
[MIT](../mixins/baritone/LICENSE).

How the LGPL parts combine with Sunset (LGPL-3 §4): the engine is a separate shared library loaded at run time
through the ABI and can be replaced as above. The in-game runtime is compiled from source together with Sunset on
your machine when Native mods are built; the package contains its complete source, so you can change it and rebuild.
Sunset's own licence does not apply to the LGPL parts.
