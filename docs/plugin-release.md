# Plugin release check

Before publishing a Sunset release, build its SDK examples and test plugin with that exact host:

```sh
python tools/plugin_release.py --horizon /path/to/pinned-horizon \
  --mixins /path/to/horizon-mixins-executable --sdk /path/to/sunset-plugins \
  --resources /path/to/game-resources/26.3 --target-dir /path/to/cargo-target \
  --out /path/to/new-release-check
```

Use the Horizon and horizon-mixins revisions recorded in `horizon-native.json`, with the pinned native Rust
toolchain and compiler on PATH. Pass the built horizon-mixins executable, not its source directory; its hash is
recorded in the receipt. Use a dedicated cargo target directory if other builds are running concurrently.
`--profile fast` is the default; `--profile release` is also supported. On Windows,
use Windows paths and one command line (or PowerShell continuations). The output directory must be new.
An optional `--stage-dir` reuses a dedicated generated source directory between runs, retaining Cargo's source paths
and cached work. It retains the prior stage under the new output directory, regenerates the stage from pristine
source, and preserves timestamps only for identical files. Keep stage and output on the same filesystem, and do
not edit or share the stage with a simultaneous build.

The command checks the SDK pins, stages Hello, AFK Fisher and `tools/plugcheck` beside Sunset, builds the client
and control tool, then runs eight selected `verify.py` checks in a fresh offscreen single-player world. It validates
the reported host API version and requires one PASS for every selected check. Failures, skips, missing results,
source changes during the run, build failures and abnormal game shutdown all prevent packing. A renderer and the
normal Minecraft resources are still required for offscreen play.

The checks cover registration and refused names, Hello's module/settings/commands/timers, module access, packet
guards and panic isolation, events, scheduler/storage/stop behavior, and AFK Fisher's startup and danger stop.
This is a single-player release smoke check; server reconnects, fishing endurance and a multi-server matrix remain
separate tests. The test client uses its own home, socket and save and disables live presence/telemetry.

Only after the checks pass does the command call `tools/pack.py` and copy the package into the output directory.
Publish that package with its `result.json` and retain its build/verifier logs. The receipt records source and binary
hashes. `tools/pack.py` alone remains a local packer and provides no gameplay validation.

Native dependencies still need their normal build toolchains. In particular, when the Baritone host integration is
used, its engine payload preparation and corresponding-source/notices requirements still apply; the plugin check
does not replace them. This check does not upload artifacts or publish a release.

When the API 3 host integration lands, update the temporary main/API 2 wording and `pr37-port` links in the SDK
reference to the merged release or commit, then run this check with the resulting host and SDK before publishing.
