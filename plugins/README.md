# Plugins

Plugins compile into Sunset. The plugin API is `client/sunset/src/game/api.rs` in the .hmod client (and
`client/mixins/sunset/api.rs` in the .hnmod one); its reference, template and example plugins are in
[sunset-plugins](https://github.com/PointOfPressure/sunset-plugins). Keep them aligned when the API changes.

## Plugins in the launcher

The Sunset launcher's Plugins tab builds plugins into Sunset on the player's computer. Plugins stay off until its
switch is turned on; with plugins on and at least one installed, the launcher stages Sunset's own .hmod package (the
release's signed `source` artifact) beside each plugin and runs the stock horizon-launcher's `native-build` for the
`sunset` instance against the pinned Nightly (with the release's signed `sdk` artifact, the Horizon fork's SDK, when
Sunset's hooks need it). Turning plugins off, or removing the last one, points the instance back at the signed
version. `sunset-launcher plugins …` does the same from a terminal. Code: `launcher/src/plugins.rs`.

A plugin for the launcher is an .hmod package of its own (a folder, or the zip `cargo horizon-mod build` writes):

- `horizon-mod.toml` with the same `[native]` pins as `client/sunset/horizon-mod.toml` (it is made for exactly that
  build) and `[dependencies] sunset = "^2.0"`;
- a `[[native.module]]` in `crates/horizon-client/src/game.rs`, where `super::sunset_api` is Sunset's plugin API;
- a `[[native.mixin]]` HEAD on `…::client_tick_post::Game<'_>::native_client_tick_post` that calls
  `sunset_api::plugin(game, ID, VERSION, setup)` each tick;
- `Cargo.toml` and `src/lib.rs` (an empty `mod_api::Hooks`).

`launcher-test/` is such a plugin: it checks the whole path in game (a chat line on joining, a badge at the top of
the screen, the block under your feet boxed while its Test Beacon module is on, and `.testplugin`). Add its folder
from the Plugins tab. It is a test plugin, not shipped.

## Sunset 2.1.0 (Horizon main `2e7d2c6`)

The plugin API (`client/sunset/src/game/api.rs`) is unchanged from 2.0.x: same calls, same `api::needs` number.
What a plugin has to change:

- **Pins.** `[native] horizon = "9defaa04337e68d6f368842491834af7bc5727f9"` (Sunset's Horizon fork commit: upstream
  `2e7d2c6` plus Sunset's hook patches, not `2e7d2c6` itself), `game = "26.3"`, `mixins =
  "e59da9094f0c2fdda6c627fe9eff22dec6a906f0"`. A plugin pinned to 2.0's `0b14f8a…` is refused when it is added
  ("made for Horizon 0b14f8a").
- **The tick hook** stays `horizon_client::game::native_hooks::client_tick_post::Game<'_>::native_client_tick_post`.
- **Your own mixins on game functions** may need new targets. Horizon split `game/remote/mod.rs`: its functions are
  now wrappers of `*_body` functions in `remote/tick.rs`, `player_state.rs`, `chat.rs`... HEAD and RETURN hooks on
  the wrappers still work; hooks with a selector or a variable (`MODIFY_ARG`, `MODIFY_VARIABLE`, `BEFORE_CALL`,
  `AFTER_CALL`) go on the bodies: `remote::player_state::Game<'_>::remote_send_state_body`,
  `remote::tick::Game<'_>::tick_local_player_body`, `remote::chat::Game<'_>::signed_chat_line_body`. The chunk
  decoder calls `remote_chunks::ChunkFormat::column_with_scratch`, no longer `column`. The full list is in
  `client/docs/hmod-port.md`.
- `game.*` internals (Horizon's own types) may have changed with the new Horizon; the `api::` calls are kept working.

The release carries the plugin kit as before: `source` (Sunset's package) and `sdk`, now the fork SDK of `9defaa0` in
its published layout (`fork-sdk/sdk-<rev>/` with `sdk.json`, the Windows client slices, build data and surface, plus
the fork's `source/` and `fork-commit`). The launcher seeds that `sdk.json` as the kept index of the Nightly-style
base `nightly-2026-11-23-9defaa0` and runs the stock CLI 0.2.107's `native-build` with `HORIZON_SDK_BASE` on it.
That base is set up from Sunset's package without a game program, and on Windows `native-build` reads the base's
`horizon-client.exe` for its PE machine type only, so the launcher links the package's program into the base before
a plugin build (`plugins.rs` `base_program`). Checked on 23 Nov 2026 with `launcher-test/`: add, on, build (SDK
download and seed, 538 s compile), the build has every file of the signed one plus the plugin's library, a
`--test-terrain` start reached the world and the plugin's tick ran (its `api::save` key was written), then off and
remove put the instance back on the signed build.

[sunset-plugins](https://github.com/PointOfPressure/sunset-plugins)' template and examples still use the .hnmod layout
and Horizon `9f42064`: they need the .hmod layout and the pins above before they build for 2.1.0.

## Test tools

- `plugcheck/`: the test plugin verify.py's `Plugin API:` checks drive (built into Sunset on the test machine only,
  never shipped). It keeps the .hnmod layout, like the SDK's examples; plugsmoke.py wraps them as .hmod plugin packages.
- `plugsmoke.py`: builds the .hmod Sunset with the SDK's example plugins and plugcheck, then runs those checks on single
  player. Run it before a release; its options are at the top of the script, its tests in `test_plugsmoke.py`.
