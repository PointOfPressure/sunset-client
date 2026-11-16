# Baritone C ABI / JSON protocol v1

The MIT declarations in `abi-v1.rs` are canonical. `loader/src/abi.rs` and
`baritone-engine/src/abi.rs` are byte-identical copies. The game never depends on
the LGPL engine crate. The loader owns every game-side unsafe operation.

Discovery symbol: `horizon_baritone_api_v1`. The discovery output is uninitialized
until success; pass `size_of::<ApiV1>()`. ABI and exact table size must equal v1.
The DLL uses C calling conventions and the game's x86_64 MSVC architecture.
Return codes: 0 OK, 1 unsupported ABI/operation, 2 bad request, 3 host unavailable,
4 insufficient pure-query buffer, 5 engine failure. Each successful dispatch
emits exactly one complete response synchronously, with at most 4 MiB. A failing
dispatch emits no actions. The caller copies bytes before the callback returns.
Callbacks and API functions must not unwind. All pointers are borrowed only for
the current call; the library retains no host/user/game pointer or callback.
The instance is client-thread confined; calls cannot overlap or recurse.

All requests/responses are UTF-8 JSON objects with `"schema":1`. All identity and
revision fields below (world, replay, job_generation, lease_generation, tick,
registry_epoch, block_revision, settings_revision, target_revision, revision)
are **decimal strings**, including zero. Coordinates use signed JSON integers;
player vectors and angles use finite JSON numbers. World coordinates are bounded
to ±30,000,000 and build heights to -4096..4095. Max targets: 4096. Draw paths:
2048 points. Unknown fields are ignored for compatible extension.

## Dispatch operations

1. Configure, no host required:
   `{"schema":1,"settings":{"allowBreak":true,"allowPlace":false,
   "allowSprint":true},"anchor":[0,64,0],"radius":128,"min_y":-64,
   "max_y":319,"max_targets":64}`. Settings accepts upstream setting names and
   typed JSON scalars or their string representation. Bounds are absolute target
   Y, not path Y. Radius is fixed to the run's start anchor. Configuration is
   atomic and cancels the old plan. V1 disables parkour and elytra execution.
2. Start goto: identity envelope plus `"goal":[x,y,z]`.
3. Start visible mine: identity envelope plus `"blocks":["minecraft:iron_ore"]`.
   Exact names only, no arbitrary suffix match. The engine scans loaded live
   blocks incrementally and never substitutes prediction states.
4. Replace predicted targets: identity envelope plus
   `"source":{"world":"1","replay":"2","dimension":"minecraft:overworld"},
   "revision":"3","pending":false,"targets":[{"pos":[1,12,3],"kind":6}]`.
   First replacement starts a predicted job. Same-key replacements retain the
   active target and terminal exclusions; a changed source cancels the old job.
   Kinds: coal=0,copper=1,iron=2,lapis=3,redstone=4,gold=5,diamond=6,
   emerald=7,quartz=8,debris=9. Empty pending batches release input and wait at
   most 200 ticks. Ore Sim never falls back to an exploratory mine process.
5. Client pre-tick: identity envelope and
   `"tick":"1","registry_epoch":"1","block_revision":"1",
   "settings_revision":"1","target_revision":"1","input_available":true,
   "player":{"position":[0.5,64,0.5],"velocity":[0,0,0],"yaw":0,"pitch":0,
   "eye_height":1.62,"on_ground":true,"in_water":false,"in_lava":false,
   "swimming":false,"under_water":false,"crouching":false,
   "horizontal_collision":false,"in_wall":false,"food":20,"air":300,
   "max_air":300,"water_height":0,"sprinting":false,"on_climbable":false,
   "motion_effects":false,"movement_speed":0.13,"reach":4.5},
   "selected_slot":0,"hotbar":[{"slot":0,"name":"minecraft:iron_pickaxe",
   "count":1}],"min_y":-64,"max_y":319`.
   Optional physical fields have conservative defaults as shown; identities,
   tick, revisions, position, yaw/pitch, input availability are required.
   A* expands at most 2048 nodes/tick. Search is restarted on block revision,
   registry epoch, or settings changes. Executable moves are revalidated live.
6. Cancel, no host required: `{"schema":1,"reason":"user"}`. Idempotently
   drops search, process, intents, pending events and drawing. No auto resume.
7. Snapshot, no host required: `{"schema":1}`. Pure owned-copy diagnostics.

Identity envelope: `"world":"1","dimension":"minecraft:overworld",
"job_generation":"2","lease_generation":"3"`. Every tick response echoes
these **and tick** exactly. Mismatches cancel and emit empty input. Host must
discard any action with a stale identity/lease or invalid live observation.

Response example (all operations have the same root shape):
`{"schema":1,"engine":"sunset-baritone/1.1.52","capabilities":["goto",
"visible_mine","predicted_mine"],"world":"1","dimension":"minecraft:overworld",
"job_generation":"2","lease_generation":"3","tick":"5",
"movement":0,"yaw":null,"pitch":null,"selected_slot":null,"attack":null,
"use":null,"status":"Idle","detail":"","active":false,
"counters":{"mined":0,"skipped":0,"mismatch":0,"unconfirmed":0,
"unreachable":0,"attempted":0},"events":[],
"draw":{"path":[],"next_path":[],"goal":null,"target":null,
"breaking":[],"placing":[]}}`.

Movement bits: forward=1, back=2, left=4, right=8, jump=16, sneak=32,
sprint=64. Opposing axes are normalized away. Attack/use are either null or
`{"pos":[x,y,z],"state_id":123}`. The latter is the fresh effective state the
host must revalidate; mining final ore also requires no pending prediction,
matching exact kind, harvestable tool, ray hit and ordinary reach. Angles are
normalized yaw [-180,180), pitch [-90,90]. Optional slot is 0..8.

For a valid active movement lease, bit64 is the executor's desired sprint state.
When it is clear the host must explicitly stop native sprinting; releasing the
sprint key alone can leave vanilla sprint latched while forward remains held.

Events: `{"source":{"world":"1","replay":"2","dimension":"..."},
"pos":[x,y,z],"kind":6,"outcome":"mined"}`; outcomes are mined, skipped,
mismatch, unconfirmed, unreachable, rejected. No route-clearance block generates
a mined event. Local predicted air does not confirm removal. Wait for pending
prediction to settle and server-backed removal; a rollback permits at most one
recovery retry. Target budget 1200 ticks, reveal wait 40, consecutive miss limit
8, maximum two stance routes, pending empty wait 200 ticks.

## Host queries

All are pure and use JSON schema 1. First call may use null output/capacity 0;
write required byte count to `needed`, return 4 if insufficient, then retry.
Replies and needed are bounded to 4 MiB. Unsupported optional query returns 1;
affected actions are unavailable, never guessed. Fast block callback returns
loaded=0 for unknown/unloaded, never fabricated air. Flags are 0 or 1.

1. `{"schema":1,"state_id":N}` ->
   `{"schema":1,"name":"minecraft:stone","classes":["Block"],
   "properties":{"axis":"y"},"fluid":"none","fluid_amount":0,
   "hardness":1.5,"replaceable":false,"full_collision":true,
   "sturdy_top":true,"pathfindable":false,"bounds":[0,0,0,1,1,1]}`.
   Fluid: none/water/flowing_water/lava/flowing_lava. Classes are Java simple
   class names. Facts are cached only inside the DLL by registry epoch.
2. `{"schema":1,"state_id":N,"slot":0}` ->
   `{"schema":1,"strength":0.1,"harvestable":true}`. Strength is progress
   per ordinary mining tick, including hardness/effects and correct-tool rules.
3. `{"schema":1,"eye":[x,y,z],"yaw":0,"pitch":0,"reach":4.5,
   "sneak":false}` -> `{"schema":1,"pos":[x,y,z],"face":[0,1,0]}`
   or `{"schema":1,"pos":null}` for no hit.
4. `{"schema":1,"pos":[x,y,z],"reach":4.5,"sneak":false}` ->
   `{"schema":1,"yaw":0,"pitch":30}` or null yaw/pitch for unreachable.
5. `{"schema":1,"slot":0,"clicked":[x,y,z],"face":[0,1,0],
   "hit":[x,y,z],"yaw":0,"pitch":30,"check":true}` ->
   `{"schema":1,"state_id":N}` or null state_id. Optional; no placement when
   unsupported. Never performs placement or changes selected inventory slot.
6. `{"schema":1}` -> `{"schema":1,"mobs":[],"spawners":[],
   "falling":[],"items":[]}`. Mob/spawner/falling entries are block positions;
   items optionally contain position/name. Unsupported means empty avoidance
   additions, not fabricated mining targets.
7. `{"schema":1}` -> `{"schema":1,"min_y":-64,"max_y":319,
   "border_min_x":-29999984,"border_max_x":29999984,
   "border_min_z":-29999984,"border_max_z":29999984,
   "loaded_chunks":[[0,0]]}`. Required for pathing/mining.

## Replacement

Stock DLL: `data/engine/x86_64-pc-windows-msvc/sunset_baritone_engine.dll`.
Runtime override: `<native_data_dir("sunset_baritone")>/engine-override/
sunset_baritone_engine.dll`. Otherwise extract stock to `engine/1.1.52/<asset
hash>/sunset_baritone_engine.dll` using create-new semantics. An existing
modified extraction is preserved and loaded. Explicit invalid override fails
closed with no stock fallback. Absolute paths only, no PATH/CWD search.
Library lifetime covers engine destruction and all callbacks; restart after
replacement. Compatible modified engines are accepted without official hashes.
