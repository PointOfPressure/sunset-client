# .hmod port (work in progress, branch hmod-port)

Horizon retired `.hnmod` (launcher 0.2.70+). Native mods are now `.hmod` packages built by function replacement
against a prebuilt game (`[native]` + `[[native.mixin]]` / `[[native.module]]` in `horizon-mod.toml`).
This branch moves Sunset to that layout, pinned to Horizon main f86cffb (no published SDK exists yet; issue #17391).

Layout: `sunset/horizon-mod.toml`, `sunset/Cargo.toml` (crate `sunset_core`), `sunset/src/*.rs` (shared state and
`bridge.rs`: calls from hooks below the game crate into game-side code), `sunset/src/game/` (support modules, game
crate only), `sunset/src/mixins/` (helpers).

Done: package layout and manifest (100 mixins + 208 modules), 22 lower-crate state modules moved to sunset_core or
the game crate, entry points retargeted (tick_world, key_press_canceled, draw_extras AFTER_CALL, render_frame_pre,
native_mouse_scrolling, extras_mouse), lower replaceable hooks switched to sunset_core / the bridge.

Left:
- 31 hooks on functions below the game crate that are not `#[replaceable]` (commented out at the end of the
  manifest): move each to a game-crate caller (WRAP_OPERATION / MODIFY_EXPRESSION_VALUE) or ask upstream for markers.
  inventory_blur (blurs_background takes self by value) too.
- Register the bridge functions on the game side (sunset_packets::poll, scan_config, atmosphere render/release) and
  set wire ids (sunset_core::wire::set_ids).
- Particles: ParticleType is no longer an enum on main; picker/set need the new name lookup.
- The Options > Mods settings page (old native_settings buttons) is gone; mod_api settings would replace it.
- Compile against main: ~/hzbuild/check-main.sh (source-level horizon-mixins apply + cargo check), then the real
  route with a locally built SDK (~/hzbuild/build-sdk.sh -> ~/hzbuild/sdk-f86cffb, HORIZON_LAUNCHER_DEVELOPER_SDK).
- pack.py, install scripts, README, plugin API docs.
