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

Still missing, and why:
- Speed, Legacy Speed jump: they replace `move_self`'s movement inside the motion (`LivingAirWorld::move_self`), which
  is not replaceable; the mod API's `entity_move` fires on the server only. Needs an upstream client-side hook.
- Streamer Mode: it rewrote every text at the font/batch level (`HudBatch::styled_text`, `Font::width`,
  `Ui::text_line`, sign text); none is replaceable. Possible per source in the game crate (chat via the replaceable
  `Chat::add`, tab list, name tags, sidebar, titles), not done yet.
- Tab game modes: the tab list names are made in the render crate; possible by editing the listed players' display
  names in the game crate, not done yet.
- Client Spoofer's brand: the first brand is sent by `net::run` (not replaceable). A second brand could be sent in
  the configuration phase from `receive_brand` (replaceable), but servers would see two brands.
- Debug Report's wire log (`ConnWriter::send`) and signed chat lines (`Chat::add_signed`); particle push
  (`ParticleEngine::push`); Clear Water (`Sprites::register` copy needs the private `sprite_cache`; its mesh tint
  went with the old mesher hook); the native block outline and Atmosphere's sky (`Renderer::draw_outline`/`render`
  copies need private gfx children); Server Scan's command tree (`chat_tree_for` copy repeats a private import).
- X-ray's Show placed blocks: the hidden-state table is per state, not per position.
- Ore Sim: the decoration's features and placer are private (only `Decoration::decorate` is exposed).
- Options > Mods: the launcher lists the package; the old settings-page buttons are gone (ClickGUI only).
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
