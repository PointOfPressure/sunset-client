# Sunset 2.1.0 on Horizon 2e7d2c6bb6f533224df1c4dfaf67548b9bd2c7c9

This branch is upstream Horizon `2e7d2c6bb6f533224df1c4dfaf67548b9bd2c7c9` plus the source of what Sunset's prebuilt client adds to it.
Horizon is LGPL-3.0-or-later (`LICENSE` at the root). Outside `sunset/`, the commits between `2e7d2c6bb6f533224df1c4dfaf67548b9bd2c7c9` and
`9defaa04337e68d6f368842491834af7bc5727f9` are Sunset's hook patches (one feature each, also in sunset-client's `client/horizon-patches/`); the
build's revision is `9defaa04337e68d6f368842491834af7bc5727f9`, the commit before the first `sunset/` commit. The rest is unmodified upstream.

- `sunset/package/`: Sunset's .hmod package (PointOfPressure/sunset-client `442d24b7a357c70a454036dceebceca76b6bbd43`, `client/sunset`). Its own
  licence is `sunset/package/LICENSE`; it is here so the shipped build can be reproduced.
- `sunset/patch/`: the function-replacement crates (`horizon_patched*`) the launcher extracts from this revision,
  with Sunset's mixins and support modules woven in by horizon-mixins `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`, and `horizon_patch.rs`, the
  launcher's entry module. Compiled, this is `horizon_patch.dll`. A modified Horizon: LGPL-3.0-or-later as a
  whole, the woven-in code included. Read the commits one by one to see the extract
  first and then exactly what Sunset changes.


## Rebuild

This revision has no Horizon release of its own: it is built as a developer base. At `9defaa04337e68d6f368842491834af7bc5727f9`, with Rust 1.99.1
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
git archive 9defaa04337e68d6f368842491834af7bc5727f9 into $HORIZON_LAUNCHER_HOME/native/source/9defaa04337e68d6f368842491834af7bc5727f9/ (with a .horizon-revision file holding 9defaa04337e68d6f368842491834af7bc5727f9)
HORIZON_LAUNCHER_DEVELOPER_SDK=<sdk> horizon-launcher native-build <folder containing sunset/package renamed to sunset> --version <developer id> --trust-native-code
```

The launcher regenerates `sunset/patch/` and links the two libraries; the result in `versions/native-*` is the shipped
client (its `native-mods.json` then names the developer base, which the release names nightly-2026-11-23-9defaa0). To change Horizon
itself, change this tree and build again, or replace the shared libraries directly: the shipped client loads every
Horizon library dynamically.
