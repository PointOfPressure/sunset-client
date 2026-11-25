# Prebuilt client scripts

- `build-prebuilt.sh`: Linux x86_64 or Windows x64 release files in `dist/prebuilt/` (see the script's header for
  its settings; `PLATFORM` defaults to the machine's): `sunset-<version>-<pin>-<platform>.tar.zst` (one `native-<pin>-<id>/` folder, ready to drop into the stock
  launcher's `versions/`), `…-relink.tar.zst`, `manifest.json`, `SHA256SUMS` and a `.minisig` for each, signed with
  `~/.config/sunset/minisign.key` (public key: `launcher/sunset.pub`). Uploading them is a manual step.
  `FORK=~/dev/horizon-src` (required) also writes the LGPL source branch and `../horizon-patches/<pin>/`, and gives
  the release Horizon's source (`…-horizon-source.tar.zst`, `…-horizon-series.bundle`) and its licence files
  (`../release/`: the game folder's `licences/` and the same beside the release; needs
  `cargo install cargo-about --version 0.9.3 --locked --features cli`).
- `check-prebuilt.sh dist/prebuilt`: signatures (a tampered package, an edited manifest and another key must fail),
  checksums, the package layout, and no Sunset source in the package.

Measured on the threadripper (18 Nov 2026, pin 0b14f8a, launcher 0.2.78): an empty `WORK` takes 330-340 s (launcher,
Nightly, prepare, Rust, SDK, native-build 222-230 s); a source change about 120 s (native-build 77 s); no change about
70 s (packaging). Package 86 MB (353 MiB unpacked, most of it the SDK's `libhorizon_mod_host.so`), relink kit 21 MB.
Two builds at the same `WORK` path give identical libraries.

## Pin 2e7d2c6: the developer route (rebase/horizon-2e7d2c6)

`build-prebuilt.sh` still drives launcher 0.2.78 against a Nightly. The rebased package (fork `sunset-hooks/2e7d2c6`,
head `9defaa0433`: upstream `2e7d2c6` plus `../horizon-patches/2e7d2c6/hooks/`) is built with the fork's own launcher
instead, in a launcher home of its own (`HORIZON_LAUNCHER_HOME`). Windows, Git Bash, the launcher's Rust 1.99.1
windows-gnu on PATH, `-c core.autocrlf=false` checkouts. Checked on 23 Nov 2026 (native-build 257 s; a
`--test-terrain --world-frames 600` start reached the world with Sunset loaded):

1. In the fork: `python tools/prebuilt_game.py build <SDK>` (build id = HEAD; 37 min cold, ~36 min after a hooks
   commit). Game and companions: `cargo build --locked --release -p horizon-official --bin horizon-official -p
   horizon-prepare -p horizon-server` and `-p horizon-client-lib --bin horizon-ctl`, `horizon-official.exe` copied to
   `horizon-client.exe`; `cargo-horizon-mod wasm-std <bin>/horizon-wasm-std.wasm` with rustup's 1.99.1 (it needs
   `rust-src`, which the launcher's toolchain lacks); the launcher is its own workspace: `cargo build --release` in
   `launcher/`.
2. Exporters: `horizon-prepare --prepare --root <fork> --minecraft-dir <dir with 26.3> --server-jar <26.3 server jar>`,
   then `horizon-prepare compile-exporters --root <fork>` and copy its `server/`, `client/` and `manifest.json` into
   `<fork>/exporters/` (Java 25 with javac: Mojang's `java-runtime-epsilon`).
3. `horizon-launcher developer-install <fork> --program <bin>/horizon-client.exe --trust-unsigned`, then
   `horizon-launcher prepare --version <developer tag> --minecraft <dir>`.
4. Seed the pinned source the mixins copy functions from (the launcher otherwise downloads it from
   horizon-rust/horizon, which doesn't have the fork's commits): `git archive HEAD` of the fork into
   `$HORIZON_LAUNCHER_HOME/native/source/<fork head>/`, plus a `.horizon-revision` file holding that commit.
5. `HORIZON_LAUNCHER_DEVELOPER_SDK=<SDK> horizon-launcher native-build client/sunset --version <developer tag>
   --trust-native-code` installs `native-<fork head 7>-<key>`. A developer base's revision is its checkout's HEAD, so
   `client/sunset` (and `client/tools/probe-hmod`, `plugins/launcher-test`) pin the fork head: a new hooks commit
   means a new SDK, a new developer install and new pins.

The Sunset launcher installs such a build as it is (checked on 23 Nov 2026 with a TEST-key release: install, base
from the package, prepare, instance). Its manifest pins the newest stock CLI, `launcher@0.2.107` (built from
`41d546ef2f`, 216 commits before 2e7d2c6; its runtime-lease and launch-entry code is the same as at 2e7d2c6), and
the package's `native-mods.json` names its base as a Nightly-style tag (`nightly-2026-11-23-9defaa0`) instead of the
developer install it was built on, without `developer.json` and `developer-source.tar`. The launcher sets that base
up from the package (`core.rs` `base_from_package`). Play on 0.2.107 still needs a check with a signed-in account.

The Plugins tab's "sdk" artifact is the fork SDK in its published layout, made from the step 1 folder in the fork:
`prebuilt_game.py pack <SDK> <W>/legs/windows`, `gather <W>/legs <W>/dist`, `index <W>/dist --nightly
nightly-2026-11-23-9defaa0` (pack about 7 min), then `<W>/dist` as `fork-sdk/sdk-<fork head>/` beside `git archive`
of the fork as `fork-sdk/source/` (with `.horizon-revision`) and a `fork-sdk/fork-commit` file. Its index's Rust
(1.99.1) and package format (6) are what CLI 0.2.107 reads; the SDK reader is the same at 41d546ef2f and 2e7d2c6.
A plugin build on 0.2.107 was checked with `plugins/launcher-test` (see `plugins/README.md`).

Release packaging on this route (Linux, 2.1.0): `build-prebuilt.sh` with `DEVELOPER=<fork checkout at the hooks
head>`, `UPSTREAM=2e7d2c6`, `DEV_LAUNCHER=<the fork's launcher/target/release/horizon-launcher>`, `DEV_BIN=<step 1's
programs>`, `DEV_SDK=<step 1's SDK as built>`, `SDK=<its published layout>` and `NIGHTLY=nightly-2026-11-23-<head 7>`
does steps 3 to 5 in its own launcher home under `WORK`, then packages, signs and writes the manifest as on the
Nightly route: the package drops `developer.json` and `developer-source.tar` and names its base `NIGHTLY`; the LGPL
series and `horizon-patches/2e7d2c6/` go from `UPSTREAM`; the fork README is `../horizon-patches/fork-README-developer.md`.
The fork head both platforms build on is `9defaa0`: its commits can be made again anywhere from `hooks/` (`git am`,
then each commit re-made with committer vstxx and the committer times of the Windows branch), which gives the same
ids, so a Linux release, the Windows release and the plugins all pin `9defaa0`.

## Sunset launcher

`build-launcher.sh` builds the launcher's release files: `sunset-launcher-linux-x86_64.tar.gz` (one `sunset-launcher/`
folder: program, LICENSE, THIRD-PARTY-LICENSES.txt) and `sunset-launcher-windows-x86_64.exe`, cross-built from Linux
(`PLATFORMS` picks). Pass one to `build-prebuilt.sh` as `SUNSET_LAUNCHER`: it is signed with the release's files and named
in the manifest (`"launcher": {version, file, size, sha256, signature}`), and launchers from 2.1.0 on update themselves to
it when its version is newer (`launcher/src/selfupdate.rs`; read apart from the rest of the manifest, so a manifest an old
launcher can't install still updates it). Windows' manifest names the `.exe`: build it before the Windows prebuilt.

## Horizon CLI mirror

github.com/horizon-rust/horizon has returned 404 since 24 Nov 2026, so a fresh install can no longer download the
stock `horizon-launcher` CLI a manifest pins from upstream's `launcher@<version>` release (installs that have it keep
working). The CLI is therefore attached to Sunset's own release: `build-prebuilt.sh` takes `CLI_ARCHIVE` (the stock
archive, `horizon-launcher-linux-x86_64.tar.gz` or `horizon-launcher-windows-x64.zip`), copies it into `OUT` and
pins its SHA-256 with a `client-v<version>` release address; the Sunset launcher accepts a CLI address under
`horizon-rust/horizon` or `PointOfPressure/sunset-client` releases (`core.rs` `Manifest::check`) and checks the hash
from the signed manifest as before. 2.1.0 pins `launcher@0.2.108` (`7954f21558`): upstream's own signed archives, checked
with Horizon's `horizon-release-sign verify --tag launcher@0.2.108 --only <archive>` against its SHA256SUMS and
SHA256SUMS.sig, and attached untouched with `git archive` of that commit as the source. The 2e7d2c6 package needs a CLI
from 0.2.107 on: with 0.2.78 the game refuses Play ("... isn't registered by the launcher").

Use upstream's own binary, not a rebuild: launcher.yml bakes `HORIZON_MSA_CLIENT_ID` (a repository variable) into
the release build, and without it the CLI says sign-in is not configured. An archive is just the binary at its top
level (`tar czf` / zip of `horizon-launcher[.exe]`). The launcher is LGPL: attach its source beside it, `git archive`
of the `launcher@<version>` tag (0.2.78 is `0d34b3e34c`, 0.2.107 is `41d546ef2f`; both are in the PointOfPressure/horizon
fork, fetch by full commit id).

## Windows

The SDK's Windows build is `x86_64-pc-windows-gnu`, so MSVC/xwin can't link against it. What was tried here:

- Cross from Linux (`rustc 1.99.1 --target x86_64-pc-windows-gnu`, rust-std 1.99.1, mingw-w64 linker, the SDK's
  Windows slices, replaying the launcher's rustc calls): fails on the first crate. The SDK's proc macros
  (`host/*.dll`) are Windows programs a Linux rustc can't load (E0462), and the Linux SDK's proc macros carry other
  crate hashes (E0463). Making this work would need Linux builds of those proc macros with the Windows SDK's exact
  metadata, which only upstream's build knows.
- Wine 10: the Windows launcher 0.2.78 doesn't start (`icuuc.dll`, a Windows 10+ system library, is missing).

A Windows builder needs: Windows 10/11 x64 (a VM or a GitHub Actions `windows-latest` runner), launcher 0.2.78
(it installs its own Rust 1.99.1, rust-mingw and WinLibs GCC), Minecraft 26.3 files for `prepare` (or a signed-in
account for `prepare --download`), about 5 GB of disk, and Git Bash with python3 (or `python`: the
script puts a `python3` of its own on PATH when Windows' is the Store stub), zstd and unzip; `build-prebuilt.sh` takes
`PLATFORM=windows-x86_64` there (the default on Windows): `.dll` names, the launcher's `.zip`, `C:/sunset-prebuilt`
as `WORK`, and a Python copy where Git for Windows has no rsync. `SIGN=0` on Windows: copy the unsigned files to
the threadripper and sign them there, so the secret key never leaves it. `check-prebuilt.sh` reads the platform
from `manifest.json` (PE imports instead of `ldd` for a Windows package).

Horizon's Nightly is one rolling release now, so a fresh `WORK` can't `install` an old pin's Nightly ("No Horizon
release is tagged"). `BASE=<folder> NIGHTLY=nightly-<date>-<pin>` installs a game folder of the pin built here as
that Nightly instead (with `HOOKS`: the fork SDK stands in for the Nightly's signed SDK index). The folder is
what a Nightly's archive holds, built from the fork as `.github/workflows/game-build.yml` does: `cargo build
--release --locked -p horizon-official --bin horizon-official -p horizon-client-lib --bin horizon-ctl -p
horizon-prepare --bin horizon-prepare -p horizon-server --bin horizon-server` (Windows: the MSVC toolchain and
`RUSTFLAGS=-C target-feature=+crt-static`), `horizon-official` copied to `horizon-client`, `cargo run --locked -p
cargo-horizon-mod -- wasm-std <bin>/horizon-wasm-std.wasm`, `horizon-prepare compile-exporters` (a 26.3 import in
`.local/minecraft`, Java 25 with javac: Mojang's runtime has it) into `exporters/`, then `cargo run --locked -p
xtask -- package <bin>/horizon-client[.exe] <folder>` and on Windows `python tools/ci/fetch_dxc.py <folder>`.

### Windows 2.0.0 prebuilt (vs.)

Build it from what the 2.0.0 Linux prebuilt was built from (its `manifest.json`): sunset-client `e2f82e3` (its client/sunset has the
ClickGUI Theme default Grey, so a Windows build from `7419693` or `bd6ee8c` is older), Horizon
pin `0b14f8acfd97fa2c750e42795249dcc981178ca4` (`nightly-2026-11-18-0b14f8a`, game 26.3), horizon-mixins `e59da90`,
the fork SDK of PointOfPressure/horizon `sunset-hooks/0b14f8a` = `95b6ab99cb` (the 10 patches in
`../horizon-patches/0b14f8a/hooks/`), launcher 0.2.78 and its rustc 1.99.1 (`horizon-launcher rust install`,
`x86_64-pc-windows-gnu`). The SDK must be the fork's: Sunset's hooks call functions only it has. First built on
Windows 11 on 21 Nov 2026 from sunset-client `7419693`: SDK 64 min and base 39 min (in parallel, cold), then
`build-prebuilt.sh` about 6 min warm (native-build 150-180 s); package 88 MB (359 MiB unpacked).

1. `git clone -b sunset-hooks/0b14f8a https://github.com/PointOfPressure/horizon horizon-fork` (HEAD `95b6ab99cb`, with
   a git identity: the script commits the LGPL branch there) and sunset-client with `-c core.autocrlf=false` (the
   script refuses a CRLF checkout: CRLF would be woven into the package).
2. The fork's SDK, Windows leg: `FORK=<horizon-fork> RUST_BIN=<launcher>/rust/1.99.1-x86_64-pc-windows-gnu/bin
   client/scripts/fork-sdk.sh build` (the toolchain's `mingw64/bin` on PATH too), as for Linux.
3. The base: Nightly `nightly-2026-11-18-0b14f8a` can't be installed any more, so build the fork's game folder as
   "Windows" above says (`HORIZON_NIGHTLY="2026-11-18 0b14f8a"`).
4. In sunset-client: `SDK=<SDK> FORK=<horizon-fork> HOOKS=sunset-hooks/0b14f8a MINECRAFT=<.minecraft with 26.3>
   BASE=<base folder> NIGHTLY=nightly-2026-11-18-0b14f8a SIGN=0 RUSTUP_TOOLCHAIN=1.96.1
   client/scripts/build-prebuilt.sh`. `RUSTUP_TOOLCHAIN` (with rust-docs) is the rustc the Sunset launcher was built
   with: `launcher/THIRD-PARTY-LICENSES.txt` is regenerated with the host's rustc and names its standard library.
   The fork branch `sunset/0b14f8a` it makes is Windows' own (other commits than Linux's): push it under its own
   name, or the two releases' notices point at one branch.
5. Copy the unsigned files to the threadripper, sign them there with `~/.config/sunset/minisign.key`, and run
   `check-prebuilt.sh` on them.
