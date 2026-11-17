# Independent Windows engine build

Keep sources, caches, TEMP/TMP, target directories and game instances on D:.
Use the same x86_64 MSVC Rust toolchain as Horizon. This engine builds without
Horizon/Game or horizon-mixins; the companion never depends on the engine crate.

From the repository root:

```powershell
pwsh -File tools/build-baritone.ps1 -Offline
```

The script runs the locked engine tests with warnings denied, builds the
release DLL into `D:\swarm-tmp\baritone-target-wA`, then copies only the DLL
to `mixins/baritone/data/engine/x86_64-pc-windows-msvc/`. `-TargetDir` selects
another D: target directory. `-EnvironmentScript` selects a local toolchain
setup script (or pass an empty string to use an already configured shell).
Remove `-Offline` for the first dependency fetch. `-SkipTests` is available for
iteration, and does not count as validation. Native game builds belong to the
team build service and are not performed by this script.

`python tools/pack.py --with-baritone` consumes that already built DLL and emits
the companion and corresponding-source archive. Ordinary packaging stays
Sunset-only. Preserve the archive's relative `baritone-engine/`,
`mixins/baritone/loader/`, `mixins/baritone/interface/`, MIT licence and build
script layout when building modified sources. The engine-side transport source
is `mixins/baritone/loader/src/engine_ffi.rs`; it is not a game dependency.

Loader unit tests can run as `cargo test -p sunset-baritone-loader` inside the
staged native workspace, or from a disposable D: copy of `loader/` (do not
generate Cargo.lock in the shipped loader tree). Use `--no-default-features`
for the disposable copy; the default `bundled-engine` feature includes DLL
bytes from the native stage's `native-data/` directory. Four ignored integration
tests additionally accept `SUNSET_BARITONE_TEST_DLL`,
`SUNSET_BARITONE_MODIFIED_TEST_DLL`, and
`SUNSET_BARITONE_INCOMPATIBLE_TEST_DLL` and
`SUNSET_BARITONE_NULL_CALLBACK_TEST_DLL`, then run with `-- --include-ignored`.
Build the compatible modified engine from these sources with
`SUNSET_BARITONE_BUILD_MARKER=swarm-modified`; the distribution build leaves
that variable unset. Compile the separate negative-test
`loader/src/incompatible_fixture.rs` using `rustc --edition 2024 --crate-type
cdylib -Dwarnings -o D:\swarm-tmp\incompatible.dll`. It is never compiled into
the production loader or distributed engine. Build it a second time with
`SUNSET_BARITONE_FIXTURE_NULL_CALLBACK=1` to produce the null-callback fixture.
These tests prove transport and
replacement without relinking the loader; gameplay/server QA is separate.

Install both native packages. For a compatible modified library, place it at
`<game>/horizon-mods/.horizon-mod-data/sunset_baritone/engine-override/
sunset_baritone_engine.dll`, then restart. No official hash/signature is
required. Invalid explicit overrides disable the service with one diagnostic;
there is no fallback that bypasses the chosen override. Existing extracted or
overridden files are never overwritten. The engine is LGPL with upstream
notices; the companion loader/interface is MIT. See the repository's Baritone
documentation for application distribution terms and current gameplay limits.
