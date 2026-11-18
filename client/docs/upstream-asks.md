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
