# Upstream asks (drafts, not sent)

What Sunset's Horizon fork (`PointOfPressure/horizon`, branch `sunset-hooks/<pin>`, series in
`client/horizon-patches/<pin>/hooks/`) adds to Horizon for the .hmod port, written as requests to upstream. Opening
issues or PRs is Sayer's call. Each section is one patch; all are small and none changes behaviour without a mod.

## 1. A decoration replay API (`horizon_world::worldgen::decoration_replay`)

Ask: a public way to replay a chunk's placed features against a caller's world view, without generating terrain.
Patch: `DecorationReplay::new(data, preset, dimension, build, generation)`, `features()`, `runs(id)`,
`biome_names()`, `biome_has(biome, id)` and `replay(seed, x, z, ids, view, set)`, which uses the generator's own
decoration seed, feature seeds, placement chains and features and reports each block they would set. `Decoration`,
`FeaturePlacer`, `FeatureWorld` and `PlacementContext` stay private. Use: ore finders (Sunset's Ore Sim), seed tools.

## 2. `#[replaceable]` on `horizon_client_lib::net::brand`

Ask: a replacement cell on the brand the configuration phase sends (`ClientBrandRetriever.getClientModName`).
Today a mod can only send another brand after joining, so the server sees "vanilla" first. One attribute.

## 3. `#[replaceable]` on `ParticleEngine::push`

Ask: a cell on the engine's ordinary queue, where block-breaking and the renderer's own particles join, past
`add_with`'s options path (which already has one). One attribute.

## 4. A sent-packet observer in `horizon_protocol::connection`

Ask: `observe_sent(fn(&[u8]))`, process-wide observers `ConnWriter::send` calls with each body (id + payload) before
adaptation and framing. Covers what the network thread sends by itself (login, configuration, keep-alives) as well
as play packets, for packet logs and test probes. `horizon-protocol` is below the crates function replacement
reaches, so it is a plain hook: four `OnceLock` slots, one atomic load per packet when unset. A replaceable
`ConnWriter::send` would do as well if the launcher extracted `horizon-protocol`.

## 5. `#[replaceable]` on the local player's travel move

Ask: a cell on `MotionWorld::move_self` (`LocalPlayer.move`), the move living travel hands the client's own player
after input, jumping and friction and before collision. (The staging pass refuses the marker on the trait impl,
`<MotionWorld as LivingAirWorld>::move_self`: "item not found".) `entity_move`'s hook fires on the server
only, so no client-side hook can change the local player's own movement or the velocity after `jumpFromGround`.
One attribute.

## 6. Building an SDK for a patched branch (`tools/prebuilt_game.py`)

Ask: `HORIZON_SDK_BUILD_ID`, so a branch of patches on a Nightly's commit builds an SDK with that commit's build id
(staging still copies HEAD's source). With it, a fork's SDK works with the stock launcher (kept `sdk.json` and
`HORIZON_SDK_BASE`); an official way to point the launcher at a locally built, self-signed SDK would replace that.

## 7. A display filter for shown text (`horizon_client_core::text::filter_display`)

Ask: one process-wide filter of the text the client shows. `Text::resolved`, which every draw and measure of a
component goes through (HUD, menus, sign text, tooltips), hands its result to it; `Font::width` and the menus' own
font (`ui_theme`'s layout) ask `shown_str` for plain strings, so measuring and drawing agree. A thread-local guard
keeps the filter's own resolving and measuring unfiltered. `display_changed` tells the caches of shown text (the
renderer's sign text, keyed by `display_generation`) to make it again. For streamer-style name hiding; one atomic
load per text when unset. The alternative, `#[replaceable]` on each text path, can't hand back a changed borrowed
string and misses `Text::resolved`'s callers in other crates.

## 8. `before_render` in `horizon_client_render::gfx`

Ask: up to four functions `Renderer::render` runs after its lost-device check, before anything of the frame is drawn,
with the renderer and the frame: for a mod's own passes (a sky). `render`'s body reaches gfx's private child modules
(`core_lines::draw`, `capture::write_capture`), so the launcher's copy of it can't be built for a mixin.

## 9. A sprite filter (`horizon_client_render::sprites::filter_sprites`)

Ask: one function `Sprites::register` hands each sprite's name and RGBA frames as the atlas takes it; a changed
sprite becomes translucent. For see-through water and similar texture changes (shown after a resource reload).
`register` is replaceable, but its signature names the private `sprite_cache` module, which the copy can't reach.

## 10. Position-aware hidden blocks (`horizon_client_render::mesher::show_hidden_at`)

Ask: when a mod hides states from the mesher (`block_render_visibility`), one function asked once per meshed section
for the places in it (and its border) whose block stays drawn: an ore finder showing the blocks the player placed.
The per-state table can't say where a block is.
