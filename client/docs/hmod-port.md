# .hmod port (branch hmod-port, work in progress)

Horizon retired `.hnmod` (launcher 0.2.70+). Native mods are `.hmod` packages compiled against a prebuilt game by
function replacement: `[native]` pins plus `[[native.mixin]]` and `[[native.module]]` rows in `horizon-mod.toml`.
It pins **Nightly 2026-11-18 · 0b14f8a**, the first Horizon build with a published mod SDK
(`sdk-0b14f8acfd…`): the launcher (0.2.78) downloads the SDK by itself, so a player installs that Nightly and builds
the package from the Mods page or with `native-build`.

## Rebase onto Horizon 2e7d2c6 (branch rebase/horizon-2e7d2c6, 23 Nov 2026)

The package now pins Sunset's fork commit `9defaa0433` = upstream `2e7d2c6` (main, 23 Nov) plus the 13 hook patches
(`client/horizon-patches/2e7d2c6/hooks/`; only 0008 and 0009 needed hand merges: `before_render` runs after the new
`frame_export` draw gate, `filter_sprites` keeps the new `frame_size`). It is built through the newest launcher's
developer route instead of a Horizon Nightly: the fork checkout is installed as a developer base
(`horizon-launcher developer-install`), its SDK is built from the same checkout (`tools/prebuilt_game.py build`), and
`HORIZON_LAUNCHER_DEVELOPER_SDK=<sdk> horizon-launcher native-build` compiles the package. A developer base's revision
is its checkout's HEAD, and the launcher refuses mods pinned to another revision, so the pin names the fork commit.

What the rebase changed in the package:
- Horizon split `game/remote/mod.rs`: its functions are wrappers of `*_body` functions in `remote/tick.rs`,
  `player_state.rs`, ... Selector hooks moved to the bodies: `sent_look` to `remote_send_state_body`, `screen_keys`,
  `screen_move` and `screen_keys_after` to `tick_local_player_body`. HEAD/RETURN hooks on the wrappers still work.
  The mixins tool refuses `remote_send_state_body` as upstream writes it (`matches!`/`vec!` under `use super::*`
  could be shadowed), so fork patch 0012 spells them `::core::matches!` and `::std::vec!`.
- `debug_chat_signed` hooks `signed_chat_line_body` (`remote/chat.rs`). Upstream's `Renderer::release_surface`
  now calls the `pub(super)` `stop_display_reader`, which Atmosphere's replacement of it can't reach from its crate
  (only the game crate's items are widened in the SDK): fork patch 0013 makes it `pub`.
- `chunk_packet` hooks `ChunkFormat::column_with_scratch`: the decoder worker no longer calls `column`.
- `packet_tap_hook`: the backlog holds `(Received, Instant)` (BACKLOG_LIMIT counts from that time).

## Layout

- `sunset/horizon-mod.toml`: the manifest (id `sunset`), 208 support modules and ~105 mixins.
- `sunset/Cargo.toml`, `sunset/src/*.rs`: crate `sunset_core` (mod API only): state shared with the hooks below the
  game crate, and `bridge.rs`, which lets those hooks call registered game-side functions.
- `sunset/src/game/`: support modules (game crate only); `compat.rs` maps the removed `native_*` helpers, mob
  equipment, held keys and click queues onto this build's API.
- `sunset/src/mixins/`: helpers. `sunset/assets/sunset/`: the fonts (now package assets) and their licences.

## State

Launcher 0.2.78 builds the package against the published SDK of 0b14f8a (`native-0b14f8a-…`, ~150 s) and the game
starts with it (title screen, Sunset's data folder created). Modules not yet checked in a world.

Restored through other routes on this build (2026-11-18):
- Fonts: the game takes no fonts from package assets; the TrueType files are embedded and loaded into the game's
  font cache with its own loader (`game/font.rs` `ensure`).
- Safe Walk, Flight speed, Velocity's liquid push: mod API hooks `player_stays_on_ground_surface`,
  `living_flying_speed`, `fluid_flow` (client side only).
- GUI Move behind screens, No Slow (Grim, Skip), knockback slow, Scaffold movement correction: `tick_local_player`
  (the keys where they are stored, the player after `sync_player_body`, the view back before `seat_own_stands`).
- X-ray: the mesher's hidden-state table (`block_render_visibility`'s), rebuilt in `render.rs` `xray_hide`.
- No Render's menu sheet, ClickGUI's inventory blur: HEAD of `render_transparent_background` (the blur through
  `screen_blur_requested`, as the mod API's `apply_screen_blur`).
- Hide hurt cam: `bob_matrix`'s tilt argument. Hand View's No equip drop / 1.8 animations: `tick_hand`.
- Mount HUD food rows and overlays, mount XP, Sunset's crosshair: `hud_layers`.

Also restored later the same day:
- Tab game modes / Sunset Presence's sun on a server: the listed players' tab-list names, each tick (`tab_marks.rs`).
- Streamer Mode, where each text is made: chat (`Chat::add` after Debug Report's log; signed lines), name tags,
  the tab list, the sidebar, titles, the action bar, boss bars. Chat lines keep the replaced names once added.
- Debug Report: signed chat lines (`signed_chat_line`), the packet log of play packets (`send_play`).
- Native block outline switch (`native_block_highlight`), No Render's crack and warden-dig particles, Clear Water's
  tint (biome water colours), Client Spoofer's join packets and its brand after joining (Brand after join).
- Server Scan's command tree already came from the packet tap.

- Sunset's page in Options > Mods (its menu as the native page's setting rows), Streamer Mode in item tooltips.

Through Sunset's Horizon fork (18 Nov, `PointOfPressure/horizon` `sunset-hooks/0b14f8a`, patches in
`client/horizon-patches/0b14f8a/hooks/`, built as an SDK with `client/scripts/fork-sdk.sh`; asks in
`upstream-asks.md`):
- Ore Sim: `horizon_world::worldgen::decoration_replay` (patch 0001). Checked against real worlds: with the right seed
  7985 of 8855 predicted ores (90%) were ore blocks in single player, 5156 of 6175 (84%) on Paper; with the seed one
  off, 1%.
- Client Spoofer's brand in the configuration phase, one brand packet per connection (`net::brand`, 0002; Brand after
  join is gone). No Render's block-breaking and renderer particles (`ParticleEngine::push`, 0003). Debug Report's
  packet log of every packet sent, the network thread's keep-alives included (`observe_sent`, 0004; the log had no
  packet ids before, so it held nothing). Speed and Legacy Speed's jump (`MotionWorld::move_self`, 0005).
- Streamer Mode in Horizon's own menus, sign text (in the world and its edit screen) and item tooltips: the text
  display filter (`text::filter_display`, 0007; signs drawn before a change of names are made again through
  `display_changed`). Atmosphere's sky: `gfx::before_render` (0008; the sky shader now starts with Horizon's
  `HDR_LIGHT` prefix, which declares `HDR_WRITE_LIGHT`, and replaces `sky_base`, the pipeline the world pass draws
  the sky with on this build, as well as the mirrors' `sky`). Clear Water's opacity: `sprites::filter_sprites` (0009) fades
  the water sprites as the atlas takes them, so a change of opacity reloads the resources (a second after the slider
  stops; at once when turned off; the water colours are asked again for 5 s after, as a server's reload brought back
  the old tint). Xray's Show placed blocks: `mesher::show_hidden_at` (0010).
- Tab game modes and Streamer Mode in single player's tab list: `local_tab_players`'s return (no fork patch).
- verify.py's probe as an .hmod (`client/tools/probe-hmod`, its AntiBot hooks not ported).
- Seen once, not chased: in single player with Streamer Mode, a spectator guest's " [S]" tab tag read "[Player 2]"
  (Streamer Mode's name replacement taking the tag for a name; the server path marks the same way).

Still missing, and what Horizon would need to add: see the end of this file.
- Plugins: a plugin is now an .hmod package of its own built next to Sunset (its support module in `game.rs` calls
  `super::sunset_api`), which the launcher's Plugins tab builds in (`plugins/README.md`; test plugin
  `plugins/launcher-test`). sunset-plugins' template and examples, pack.py and the install scripts still use the
  .hnmod layout.

## Building it

```sh
horizon-launcher install nightly            # nightly-2026-11-18-0b14f8a
horizon-launcher prepare --version nightly-2026-11-18-0b14f8a
horizon-launcher instance create sunset-hmod
horizon-launcher instance version sunset-hmod nightly-2026-11-18-0b14f8a
horizon-launcher native-build PATH/TO/folder-with-sunset --instance sunset-hmod --trust-native-code
horizon-launcher play --instance sunset-hmod
```

A newer Nightly needs the `[native] horizon` pin moved to its commit (each build has its own SDK).

## What Horizon would need to add

All items but 9 are closed by the fork's patches (above); they stay listed as the upstream asks.

1. Speed, Legacy Speed jump: a client-side movement hook for the local player's own move (the `entity_move` hook
   fires on the server only), or a hook on the local travel's movement before collision, plus a writable velocity
   after `jumpFromGround`.
2. Client brand: a hook (or `#[replaceable]`) on `horizon_client_lib::net::brand` / the brand payload `net::run` sends.
3. Streamer Mode everywhere: `#[replaceable]` on the text paths (`HudBatch::styled_text`, `Font::width`,
   `Font::width_text`, `ui_theme::Ui::text_line`/`measure`, `block_entity_render::draw_text`), or a client
   text-display hook.
4. Packet log: `#[replaceable]` on `horizon_protocol::connection::ConnWriter::send`, or a raw "packet sent" hook.
5. Particles: `#[replaceable]` on `ParticleEngine::push`, or a hook on every particle created.
6. Clear Water's opacity, Atmosphere's sky: the mirror reaching a replaceable function's private child modules
   (`sprite_cache` for `Sprites::register`; gfx's children for `Renderer::render`), or fluid-alpha and sky hooks.
7. X-ray's placed blocks: a position-aware `block_render_visibility`.
8. Ore Sim: `Decoration`'s placed features, `FeaturePlacer`, `FeatureWorld` and `PlacementContext` in the SDK's
   internals, or an API that replays a chunk's decoration.
9. Tool limits that blocked other routes: `debug_assert!`/`log::`/`matches!`/`format!` in modules with glob imports
   ("macro identity is ambiguous"), hooks inside closures, selectors on struct-literal receivers, RETURN or
   cancellable HEAD on `-> Self`, RETURN on bodies with `?`, and the mirror keeping a grouped `use` whole
   (`motion.rs`'s `pub(crate) attribute_error`).
