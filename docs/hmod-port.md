# .hmod port (branch hmod-port, work in progress)

Horizon retired `.hnmod` (launcher 0.2.70+). Native mods are `.hmod` packages compiled against a prebuilt game by
function replacement: `[native]` pins plus `[[native.mixin]]` and `[[native.module]]` rows in `horizon-mod.toml`.
It pins **Nightly 2026-11-18 · 0b14f8a**, the first Horizon build with a published mod SDK
(`sdk-0b14f8acfd…`): the launcher (0.2.78) downloads the SDK by itself, so a player installs that Nightly and builds
the package from the Mods page or with `native-build`.

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

Still missing, and what Horizon would need to add: see the end of this file.
- Speed, Legacy Speed jump; Client Spoofer's brand in configuration (the server gets "vanilla" first); Streamer
  Mode in Horizon's own menus, signs and text tooltips; the packet log of the network thread's own packets; block-break
  particles and particles the renderer makes itself; Clear Water's opacity; Atmosphere's sky; X-ray's Show placed
  blocks; Ore Sim; Tab game modes in single player.
- Plugins (`api.rs`, sunset-plugins): not adapted to separate .hmod packages yet; pack.py, install scripts, README too.

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
