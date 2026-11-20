# Sunset @VERSION@ on Horizon @REV@

This branch is upstream Horizon `@REV@` (@TAG@) plus the source of what Sunset's prebuilt client adds to it.
Horizon is LGPL-3.0-or-later (`LICENSE` at the root). Outside `sunset/`, the commits between `@REV@` and the first
`sunset/` commit are Sunset's hook patches (branch `sunset-hooks/@REV@` when there are any; one feature each, also
LGPL-3.0-or-later); the prebuilt's Horizon libraries are built from that tree with `tools/prebuilt_game.py`
(`HORIZON_SDK_BUILD_ID=@REV@`). The rest is unmodified upstream.

- `sunset/package/`: Sunset's .hmod package (PointOfPressure/sunset-client `@SRC@`, `client/sunset`). Its own
  licence is `sunset/package/LICENSE`; it is here so the shipped build can be reproduced.
- `sunset/patch/`: the function-replacement crates (`horizon_patched*`) the launcher extracts from this revision,
  with Sunset's mixins and support modules woven in by horizon-mixins `@MIXINS@`, and `horizon_patch.rs`, the
  launcher's entry module. Compiled, this is `libhorizon_patch.so`. A modified Horizon: LGPL-3.0-or-later as a
  whole, the woven-in code included. Read the commits one by one to see the extract
  first and then exactly what Sunset changes.

## Rebuild

With horizon-launcher @LAUNCHER@ (its own Rust, @RUST@) and Minecraft 26.3:

```sh
horizon-launcher install @TAG@
horizon-launcher prepare --version @TAG@
horizon-launcher instance create sunset && horizon-launcher instance version sunset @TAG@
horizon-launcher native-build <folder containing sunset/package renamed to sunset> --instance sunset --trust-native-code
```

The launcher downloads this revision's SDK (`sdk-@REV@`), regenerates `sunset/patch/` and links the two libraries;
the result in `versions/native-*` is the shipped client. To change Horizon itself, build this revision's SDK from
your modified tree and point the launcher's SDK cache at it, or replace the shared libraries directly: the shipped
client loads every Horizon library dynamically.
