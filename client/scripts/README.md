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

Build it from what the 2.0.0 Linux prebuilt was built from (its `manifest.json`): sunset-client `bd6ee8c` (client/sunset as
`7419693`, which vs.'s Windows build used), Horizon
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
