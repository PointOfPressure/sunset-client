# .hmod port (branch hmod-port, work in progress)

Horizon retired `.hnmod` (launcher 0.2.70+). Native mods are `.hmod` packages compiled against a prebuilt game by
function replacement: `[native]` pins plus `[[native.mixin]]` and `[[native.module]]` rows in `horizon-mod.toml`.
No Horizon release publishes the SDK this needs yet (horizon-rust/horizon#17391), so nobody can build Sunset with a
released launcher today. This branch pins Horizon main **f86cffb** and is checked with a locally built SDK.

## Layout

- `sunset/horizon-mod.toml`: the manifest (id `sunset`), 208 support modules and ~105 mixins.
- `sunset/Cargo.toml`, `sunset/src/*.rs`: crate `sunset_core` (mod API only): state shared with the hooks below the
  game crate, and `bridge.rs`, which lets those hooks call registered game-side functions.
- `sunset/src/game/`: support modules (game crate only); `compat.rs` maps the removed `native_*` helpers, mob
  equipment, held keys and click queues onto this build's API.
- `sunset/src/mixins/`: helpers. `sunset/assets/sunset/`: the fonts (now package assets) and their licences.

## State

The launcher's real native build compiles the package (`native-build` with launcher 0.2.72 and
`HORIZON_LAUNCHER_DEVELOPER_SDK`). Not yet run in game.

Not ported (commented at the end of the manifest): "xray" "equip-hold" "hurt-cam" "particle-push" "xray-get" "xray-unpack" "menu_dark" "inventory_blur" "safe_walk" "speed_move" "legacy_speed_jump" "liquid_flow" "flying_speed" "tab_modes_name" "tab_modes_layout" "streamer_text" "streamer_width" "streamer_width_text" "streamer_ui" "streamer_ui_measure" "streamer_sign" "client_brand" "crosshair_rect" "debug_wire" "debug_chat_signed" "hunger_bars" "mount_xp" "screen_keys" "screen_keys_after" "water-sprites" "block_outline_draw" "atmosphere_render".

Why, by group:
- Functions below the game crate without `#[horizon_mod_api_macros::replaceable]` and no usable game-side caller:
  X-ray (mesher, chunk reads), Streamer Mode (font/HUD text), Tab game modes, custom crosshair mark, menu dark sheet,
  hunger/mount XP bars, hurt cam, particle push, equip hold, Client Spoofer's brand, Debug Report wire log and signed
  chat lines. Upstream `replaceable` markers, or a redesign against game-side callers, would bring them back.
- Launcher limits found while porting (worth reporting upstream):
  - the mirror keeps a grouped `use` whole, so `motion.rs`'s `use crate::{Result, ..., living_attributes::attribute_error}`
    makes every function of `horizon_client_lib::motion` uncopyable: Safe Walk, Speed, Legacy Speed jump, Velocity's
    liquid push, Flight speed, and the screen-keys hook (GUI Move behind screens, No Slow Skip, Scaffold movement
    correction, knockback slow);
  - a lower crate's mirror root drops its `pub use` re-exports unless a helper names them (worked around in the chat
    and font hooks), and can't reach private child modules (`sprite_cache`: Clear Water) or child-private methods
    (`gfx`: Atmosphere's sky, the native block outline);
  - functions whose bodies use `format!`/`matches!`/`vec!` in modules with glob imports are refused by horizon-mixins
    (`remote::apply_packet`, `poll_remote_events`), so hooks there moved to callers.
- Ore Sim: the world generator's decoration is a private module; Ore Sim reports it is unavailable.
- Options > Mods: the package is listed by the launcher itself; the old settings-page buttons are gone (ClickGUI only).
- Plugins (`api.rs`, sunset-plugins): not adapted to separate .hmod packages yet; pack.py, install scripts, README too.

## Building it locally

Scripts in `~/hzbuild` (not in the repo): `build-sdk.sh` (`tools/prebuilt_game.py build` of a Horizon checkout at the
pin, ~45 min), `setup-lhome.sh` (an isolated launcher data folder with a `developer-local` base and the pinned source),
`nb.sh <mods folder> <log>` (launcher 0.2.72 `native-build`, ~2-3 min). To play it the base still needs prepared
resources for this commit and a signed-in account in that folder.
