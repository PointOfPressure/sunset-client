# Prebuilt client scripts

- `build-prebuilt.sh`: Linux x86_64 release files in `dist/prebuilt/` (see the script's header for its settings):
  `sunset-<version>-<pin>-linux-x86_64.tar.zst` (one `native-<pin>-<id>/` folder, ready to drop into the stock
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
account for `prepare --download`), about 5 GB of disk, and Git Bash with python3, zstd and minisign to run this
script once its Linux-only parts (`.so` names, `linux-x86_64`, the launcher download) take the platform. Sign on
the threadripper: copy the unsigned files back so the secret key never leaves it.

### Windows 2.0.0 prebuilt (vs.)

Build it from what the 2.0.0 Linux prebuilt was built from (its `manifest.json`): sunset-client `7fb2d3f`, Horizon
pin `0b14f8acfd97fa2c750e42795249dcc981178ca4` (`nightly-2026-11-18-0b14f8a`, game 26.3), horizon-mixins `e59da90`,
the fork SDK of PointOfPressure/horizon `sunset-hooks/0b14f8a` = `95b6ab99cb` (the 10 patches in
`../horizon-patches/0b14f8a/hooks/`), launcher 0.2.78 and its rustc 1.99.1 (`horizon-launcher rust install`,
`x86_64-pc-windows-gnu`). The SDK must be the fork's: Sunset's hooks call functions only it has. Not yet run on
Windows; the Linux build is the reference.

1. `git clone -b sunset-hooks/0b14f8a https://github.com/PointOfPressure/horizon horizon-fork` (HEAD `95b6ab99cb`) and
   sunset-client at `7fb2d3f`.
2. The fork's SDK, Windows leg, as `fork-sdk.sh build` makes the Linux one: in horizon-fork, with the launcher's
   rustc 1.99.1 first on PATH and `HORIZON_SDK_BUILD_ID=0b14f8acfd97fa2c750e42795249dcc981178ca4`:
   `python tools/prebuilt_game.py build out`, `… pack out legs/windows`, `… gather legs dist`, `… index dist`. Move
   `dist` to `<SDK>/sdk-0b14f8acfd97fa2c750e42795249dcc981178ca4`, unpack `git archive --prefix=source/ HEAD` into
   `<SDK>`, and write the pin to `<SDK>/source/.horizon-revision` and `git rev-parse HEAD` to `<SDK>/fork-commit`.
3. In sunset-client: `SDK=<SDK> FORK=<horizon-fork> HOOKS=sunset-hooks/0b14f8a MINECRAFT=<.minecraft with 26.3> SIGN=0
   client/scripts/build-prebuilt.sh`, once its Linux-only parts take the platform (`PLATFORM=linux-x86_64`, the `.so`
   names; the launcher download already knows `horizon-launcher-windows-x64.zip`). It seeds its launcher home with
   the SDK (`fork-sdk.sh seed`).
4. Copy the unsigned files to the threadripper, sign them there with `~/.config/sunset/minisign.key`, and run
   `check-prebuilt.sh` on them.
