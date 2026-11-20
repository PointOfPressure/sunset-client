# Relink kit: Sunset @VERSION@ on Horizon @SHORT@ (@PLATFORM@)

Sunset's prebuilt client is Horizon (LGPL-3.0-or-later) plus two libraries built by the stock horizon-launcher:
`libsunset_core.so` (Sunset's own code, linked dynamically to Horizon's libraries) and `libhorizon_patch.so`
(copies of Horizon functions with Sunset's mixins woven in, linked from the four `libhorizon_patched*.rlib` here).
This kit has the objects and the link line to make `libhorizon_patch.so` again; the complete source of every
Horizon part, modified or not, is the fork branch named in `HORIZON-NOTICE.txt`.

Contents: `libhorizon_patched*.rlib` (the compiled patched crates), `horizon_patch.rs` (the library's entry module),
`horizon_patch.link` (its `--extern` lines, SDK paths as `$SDK`), `*.externs` (what each rlib was compiled
against), `*.identity.json`, the shipped `libhorizon_patch.so` and `libsunset_core.so`, `RUST` (the compiler
version, @RUST@), `relink.sh`, and the licence files.

## Relink

You need rustc @RUST@ (`rustup toolchain install @RUST@`, or the launcher's own: `horizon-launcher rust install`
puts it in `<launcher data>/rust/@RUST@-<target>/bin/rustc`) and the Horizon SDK the prebuilt was
built on, as the stock launcher unpacks it: run `horizon-launcher native-build` once for a Sunset instance (see
the fork's `sunset/README.md`) and use `<launcher data>/native/prebuilt-package`. When the release has a
`sunset-sdk-…` file (Sunset's hook patches built into Horizon), the prebuilt is built on that SDK: unpack it and run
the launcher with `HORIZON_SDK_BASE=<unpacked>/fork-sdk` for that native-build (sunset-client's
`client/scripts/fork-sdk.sh seed` does the same). Then:

```sh
tar --zstd -xf sunset-@VERSION@-@SHORT@-@PLATFORM@-relink.tar.zst && cd sunset-@VERSION@-@SHORT@-@PLATFORM@-relink
./relink.sh <SDK> <game folder> [<path to rustc @RUST@>]
```

`<game folder>` is the installed `versions/native-…` folder (`-` only links, into `out/`). The script keeps the
original as `libhorizon_patch.so.orig` and updates the library's hash in `native-mods.json` (the game refuses a
library whose hash differs) and in `files.sha256`.

## Relinking against a modified Horizon

Rust links a library only against the exact crate builds its objects were compiled against (their metadata
hashes). So relinking with a changed Horizon works when the SDK you build from your modified source keeps those
crates' hashes (same rustc @RUST@, same features, the SDK build of `tools/prebuilt_game.py`, changes that
don't alter the crates `libhorizon_patched*` use). When it doesn't, rustc says which crate no longer matches: then
rebuild both libraries from source instead, which always works: the fork branch, `horizon-launcher native-build`
as in its `sunset/README.md`, with your Horizon changes on that branch and the SDK built from it
(`client/scripts/fork-sdk.sh` in sunset-client). Replacing Horizon's own shared libraries
(`libhorizon_core.so`, `libmod_*.so`, …) with your builds follows the same rule.

These terms are in `PREBUILT-TERMS.md`: you may modify the Horizon part for your own use and reverse engineer the
build to debug such modifications.
