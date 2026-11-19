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
  build) and `[dependencies] sunset = "^1.2"`;
- a `[[native.module]]` in `crates/horizon-client/src/game.rs`, where `super::sunset_api` is Sunset's plugin API;
- a `[[native.mixin]]` HEAD on `…::client_tick_post::Game<'_>::native_client_tick_post` that calls
  `sunset_api::plugin(game, ID, VERSION, setup)` each tick;
- `Cargo.toml` and `src/lib.rs` (an empty `mod_api::Hooks`).

`launcher-test/` is such a plugin: it checks the whole path in game (a chat line on joining, a badge at the top of
the screen, the block under your feet boxed while its Test Beacon module is on, and `.testplugin`). Add its folder
from the Plugins tab. It is a test plugin, not shipped.

## Test tools

- `plugcheck/`: the test plugin verify.py's `Plugin API:` checks drive (staged next to Sunset on the test machine
  only, never shipped). Still in the .hnmod layout.
- `plugsmoke.py`: builds Sunset with the SDK's example plugins and plugcheck, then runs those checks. Run it before a
  release; its options are at the top of the script.
