# The companion on Sunset 2 (.hmod, Horizon fork 2e7d2c6)

Sunset 1.x shipped this companion inside its `.hnmod` (`horizon-mixin.toml`, pinned to 9f42064). Sunset 2 builds it
as an `.hmod` mod of its own, `sunset_baritone`, beside `sunset`: `horizon-mod.toml` here, one crate (`Cargo.toml`,
`src/lib.rs`: the MIT loader of the engine library, `src/abi.rs`, `src/engine_ffi.rs`), and every source the
manifest names under `src/` (the launcher refuses helpers and `.rs` files elsewhere). horizon-baritone's vendored
tree is `src/upstream/`; none of it is in `client/sunset`.

Its modules all go into the game crate's `game.rs` (support modules may not go below it on this build); they compile
into the launcher's patched crate, not the game's, so horizon-baritone's `impl Game` bridge is an extension trait
(`BaritoneBridge`) and Sunset's own game methods come through Sunset's traits (`SunsetGame`, `SunsetBaritone`).
No fork hook was needed: the fork pin, the SDK and the plugin pin are unchanged.

## The 28 hooks

| 1.x id | 1.x target (9f42064) | Sunset 2 target (2e7d2c6) |
|---|---|---|
| register, full_tick | `native_hooks::Game::native_tick` | `native_hooks::client_tick_post::Game::native_client_tick_post` (HEAD) |
| full_command | `native_hooks::Game::native_command` | `native_hooks::base::Game::native_command` |
| full_chat | `chat::Game::handle_chat_input` | same |
| full_page_draw, _mouse, _drag, _wheel, _key | `options::Game::{draw_options, options_mouse, options_drag, options_scroll, options_key}` | same |
| full_page_text | `control::Game::type_chars_into_screen` | same |
| full_keys_before, _after, _tick, _remote | `Game::sync_player_body`, `fov::Game::tick_avatar`, `Game::tick`, `remote::Game::tick_remote` | same |
| full_goal_beam | `beacon::Game::beacon_geometry` | same |
| full_set_back | `remote::Game::remote_teleport` | same |
| full_block_change | `remote::Game::set_block` | same |
| full_click_press, full_click_escape | `extras::sorting::Game::{sort_mouse_press, sort_key_press}` | same |
| full_block_interact | `remote::Game::remote_use_item_on` | `remote::interaction_prediction::Game::remote_use_item_on` |
| full_click_key | `native_hooks::Game::native_key` (keys and buttons by name) | `play_input::Game::play_mouse` (`src/mixins/click_mouse.rs`: only the mouse buttons mattered) |
| full_toast_draw | `native_hooks::Game::native_hud` | RETURN of `Game::hud_layers` (`src/mixins/toast_draw.rs`) |
| full_lines | `horizon_client_lib::gizmos::camera_relative` (below the game crate) | RETURN of `debug_keys::Game::debug_gizmos` (`src/mixins/lines.rs`, store `src/game/lines.rs`): the lines join the frame's gizmos |
| full_unload | `horizon_client_render::streaming::Streaming::forget_columns` (below the game crate) | HEAD of `remote::level_state::Game::forget_remote_columns_body` (`src/mixins/unload.rs`) |
| full_list | `native_hooks::Game::native_mods` | left out: the manifest lists the mod in Options > Mods |
| full_settings, full_click | `native_hooks::Game::{native_settings, native_settings_click}` | left out: this build has no mod settings buttons (`native_page_settings` is typed settings). Baritone's own settings page cannot be opened; `#set` and `.set baritone` work |
| full_suggest | `horizon_client_render::chat::ChatScreen::update_command_info` (below the game crate, not replaceable) | left out: no TAB completion for `#` lines. Bringing it back needs a fork hook (a `#[replaceable]` marker on it) |

## The engine library

The engine (`client/baritone-engine`, LGPL-3.0) is no longer embedded in the companion: the prebuilt ships
`libsunset_baritone_engine.so` / `sunset_baritone_engine.dll` beside the game's executable
(`client/scripts/build-prebuilt.sh`), and `shipped_engine_path` uses
`<instance>/horizon-mods/.horizon-mod-data/sunset_baritone/engine-override/` first, then that file.

## Registry dependencies

`Cargo.lock` came from `horizon-launcher native-lock`, then the crates the game also uses were pinned to the game's
versions (the build refuses a lock that moves a game package: itoa 1.0.18, syn 3.0.6, unicode-ident 1.0.26). Redo
that after a fork rebase.
