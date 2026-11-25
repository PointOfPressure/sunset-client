# Sunset @VERSION@ on Horizon @REV@

This branch is upstream Horizon `@REV@` plus the source of what Sunset's prebuilt client adds to it.
Horizon is LGPL-3.0-or-later (`LICENSE` at the root). Outside `sunset/`, the commits between `@REV@` and
`@HEAD@` are Sunset's hook patches (one feature each, also in sunset-client's `client/horizon-patches/`); the
build's revision is `@HEAD@`, the commit before the first `sunset/` commit. The rest is unmodified upstream.

- `sunset/package/`: Sunset's .hmod package (PointOfPressure/sunset-client `@SRC@`, `client/sunset`). Its own
  licence is `sunset/package/LICENSE`; it is here so the shipped build can be reproduced.
- `sunset/patch/`: the function-replacement crates (`horizon_patched*`) the launcher extracts from this revision,
  with Sunset's mixins and support modules woven in by horizon-mixins `@MIXINS@`, and `horizon_patch.rs`, the
  launcher's entry module. Compiled, this is `libhorizon_patch.so`. A modified Horizon: LGPL-3.0-or-later as a
  whole, the woven-in code included. Read the commits one by one to see the extract
  first and then exactly what Sunset changes.


## Rebuild

This revision has no Horizon release of its own: it is built as a developer base. At `@HEAD@`, with Rust @RUST@
and Minecraft 26.3 (Java 25 with javac for the exporters):

```sh
cargo build --locked --release -p horizon-official --bin horizon-official -p horizon-prepare -p horizon-server
cargo build --locked --release -p horizon-client-lib --bin horizon-ctl     # then copy horizon-official to horizon-client
python tools/prebuilt_game.py build <sdk>                                  # the SDK of this revision
horizon-prepare --prepare --root . --minecraft-dir <dir with 26.3> --server-jar <26.3 server jar>
horizon-prepare compile-exporters --root .                                 # copy its server/, client/, manifest.json to exporters/
(cd launcher && cargo build --release)
horizon-launcher developer-install . --program <bin>/horizon-client --trust-unsigned
horizon-launcher prepare --version <developer id> --minecraft <dir with 26.3>
git archive @HEAD@ into $HORIZON_LAUNCHER_HOME/native/source/@HEAD@/ (with a .horizon-revision file holding @HEAD@)
HORIZON_LAUNCHER_DEVELOPER_SDK=<sdk> horizon-launcher native-build <folder containing sunset/package renamed to sunset> --version <developer id> --trust-native-code
```

The launcher regenerates `sunset/patch/` and links the two libraries; the result in `versions/native-*` is the shipped
client (its `native-mods.json` then names the developer base, which the release names @TAG@). To change Horizon
itself, change this tree and build again, or replace the shared libraries directly: the shipped client loads every
Horizon library dynamically.
