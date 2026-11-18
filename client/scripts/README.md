# Prebuilt client scripts

- `build-prebuilt.sh`: Linux x86_64 release files in `dist/prebuilt/` (see the script's header for its settings):
  `sunset-<version>-<pin>-linux-x86_64.tar.zst` (one `native-<pin>-<id>/` folder, ready to drop into the stock
  launcher's `versions/`), `…-relink.tar.zst`, `manifest.json`, `SHA256SUMS` and a `.minisig` for each, signed with
  `~/.config/sunset/minisign.key` (public key: `launcher/sunset.pub`). Uploading them is a manual step.
  `FORK=~/dev/horizon-src` also writes the LGPL source branch and `../horizon-patches/<pin>/`.
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
