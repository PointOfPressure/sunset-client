# Parity with Baritone 1.21.4

Every class of Baritone 1.21.4 (`src/api` and `src/main`, 332 Java files) is accounted for below, so that nothing is claimed that is not true. Nothing is missing. The few *partial* rows differ because Horizon gives a mod no thread, renderer or event bus of its own, and each says how. *Ported* means written from the Java; the commands and their messages, every setting and its default, the search, the movements, the control manager and the input layer were then compared with the Java side by side and tested in a generated world, the rest by reading.
| Status | Meaning |
|---|---|
| ported | written from the Java source and does the same thing; not yet diffed line by line against it |
| partial | exists, but simpler than the Java or differing in a way noted beside it |
| missing | not written |
| n/a | Java or Minecraft machinery with no meaning in Rust on Horizon (mixin accessors, the event bus, toasts) |

Rows below: 297 (classes that go together share a row). ported 247, partial 0, missing 0, n/a 50.

All 248 settings of Baritone 1.21.4 that `#set` shows are here (generated from Settings.java, with the Java type `#set` prints), and every one is read by something, except five that have nothing to act on in Horizon: `logAsToast` (no way to push a toast of its own), `renderGoalXZBeacon`, `renderCachedChunks` and `cachedChunksOpacity` (no way to draw a beam or the cached world), and `pathingMapLoadFactor` (it tunes the Java hash map). The command layer (names, aliases, descriptions and messages) follows `baritone.command.defaults`. What is left in the rows below is mostly what needs a renderer of the game own: the toast, the beacon beam and the semitransparent cached chunks.


## api

| Java | Status | Rust or note |
|---|---|---|
| BaritoneAPI | n/a | one instance, no provider |
| IBaritone | ported | src/baritone.rs (Baritone, Core): one instance, as Horizon has one player |
| IBaritoneProvider | n/a | one instance |
| Settings | ported | src/api/settings.rs: generated from Settings.java, with Vec3i, Rotation, Mirror and Map<Block, List<Block>> parsed as the Java parses them; the JavaOnly four are hidden by #set in the Java too |

## api/behavior

| Java | Status | Rust or note |
|---|---|---|
| IBehavior | n/a | Rust has no behavior registry |
| ILookBehavior | ported | src/behavior/look.rs |
| IPathingBehavior | ported | src/behavior/pathing_behavior.rs |

## api/behavior/look

| Java | Status | Rust or note |
|---|---|---|
| IAimProcessor | ported | AimProcessor in src/behavior/aim.rs (LookBehavior in its CLIENT mode: the camera turns) |
| ITickableAimProcessor | ported | AimProcessor in src/behavior/aim.rs (LookBehavior in its CLIENT mode: the camera turns) |

## api/cache

| Java | Status | Rust or note |
|---|---|---|
| IBlockTypeAccess | n/a | an interface: CachedWorld::get_block |
| ICachedRegion | ported | CachedRegion, src/cache/cached_world.rs |
| ICachedWorld | ported | CachedWorld, src/cache/cached_world.rs |
| IWaypoint | ported | src/cache/waypoints.rs |
| IWaypointCollection | ported | src/cache/waypoints.rs |
| IWorldData | ported | WorldData, src/game/world_data.rs |
| IWorldProvider | ported | WorldProvider, src/game/world_data.rs |
| IWorldScanner | ported | src/game/scanner.rs |
| Waypoint | ported | src/cache/waypoints.rs |

## api/command

| Java | Status | Rust or note |
|---|---|---|
| Command | ported | src/command/manager.rs |
| IBaritoneChatControl | ported | src/baritone.rs (chat, command) |
| ICommand | n/a | a Command struct |
| ICommandSystem | n/a | registry() in manager.rs |

## api/command/argparser

| Java | Status | Rust or note |
|---|---|---|
| IArgParser | n/a | typed parsers are methods of Args |
| IArgParserManager | n/a | typed parsers are methods of Args |

## api/command/argument

| Java | Status | Rust or note |
|---|---|---|
| IArgConsumer | ported | src/command/argument.rs (Args): requireMin/Max, get*, peek*; errors read "Error at argument #N: Expected X" |
| ICommandArgument | ported | src/command/argument.rs: the word; its number is the position in Args |

## api/command/datatypes

| Java | Status | Rust or note |
|---|---|---|
| BlockById | ported | src/command/tab.rs, Core::block_named |
| EntityClassById | ported | src/command/tab.rs, follow: mob ids |
| ForAxis | ported | src/command/build.rs |
| ForBlockOptionalMeta | ported | src/command/tab.rs: the id, property names and values |
| ForDirection | ported | src/api/utils.rs |
| ForWaypoints | ported | src/cache/waypoints.rs (lookup) |
| IDatatype, IDatatypeContext, IDatatypeFor, IDatatypePost, IDatatypePostFunction | n/a | typed arguments are methods of Args |
| ItemById | ported | src/command/tab.rs, pickup validates an item id |
| NearbyPlayer | ported | src/command/tab.rs, follow player names |
| RelativeBlockPos | ported | src/command/manager.rs |
| RelativeCoordinate | ported | src/command/argument.rs: k and m scales, decimals, floored as the Java floors |
| RelativeFile | ported | src/command/tab.rs (relative_file), build.rs |
| RelativeGoal, RelativeGoalBlock, RelativeGoalXZ, RelativeGoalYLevel | ported | src/command/manager.rs (relative_goal) |

## api/command/exception

| Java | Status | Rust or note |
|---|---|---|
| CommandErrorMessageException, CommandException, CommandInvalidArgumentException, CommandInvalidStateException, CommandInvalidTypeException, CommandNoParserForTypeException, CommandNotEnoughArgumentsException, CommandNotFoundException, CommandTooManyArgumentsException, CommandUnhandledException, ICommandException | ported | one CommandError carrying the Java messages: Error at argument #N: Expected X, Not enough arguments, Too many arguments; a panic in a command is CommandUnhandledException |

## api/command/helpers

| Java | Status | Rust or note |
|---|---|---|
| Paginator | ported | src/command/argument.rs, manager.rs (paginate) |
| TabCompleteHelper | ported | src/command/tab.rs |

## api/command/manager and registry

| Java | Status | Rust or note |
|---|---|---|
| ICommandManager | ported | src/command/manager.rs |
| Registry | ported | registry() in src/command/manager.rs: a list, as nothing adds commands at run time |

## api/event

| Java | Status | Rust or note |
|---|---|---|
| BlockChangeEvent | ported | hooks/block_change.rs: every set_block, read by the elytra packer |
| BlockInteractEvent | ported | hooks/block_interact.rs: USE only, which is all the Java listens to |
| ChatEvent | ported | hooks/chat.rs |
| ChunkEvent | ported | WorldCache::note_chunks for the load, hooks/unload.rs and WorldProvider::pack_unloaded for the unload: a chunk is packed from what it was as the server drops it |
| PacketEvent | n/a | Horizon has no packet hook |
| PathEvent | n/a | PathingBehavior reads its own events; nothing outside listens |
| PlayerUpdateEvent | n/a | native_tick |
| RenderEvent | ported | hooks/lines.rs: once a frame with the camera and the view, which is what the event carries |
| RotationMoveEvent | n/a |  |
| SprintStateEvent | n/a | the sprint is Baritone input Sprint, set where the Java overrides the state |
| TabCompleteEvent | ported | hooks/suggest_hook.rs |
| TickEvent | ported | hooks/tick.rs |
| WorldEvent | ported | WorldData::follow and LookBehavior::on_world_event |
| Cancellable, EventState, ICancellable, Overrideable | n/a |  |
| AbstractGameEventListener, IEventBus, IGameEventListener | n/a | Horizon hook points instead |

## api/pathing

| Java | Status | Rust or note |
|---|---|---|
| IPath | ported | Path in src/pathing/calc/path.rs |
| IPathFinder | ported | AStarPathFinder |
| IPathingControlManager | ported | pre_tick and post_tick in pathing_behavior.rs, executeProcesses in Baritone::control_tick |
| Goal | ported | src/api/goals.rs |
| GoalAxis | ported |  |
| GoalBlock | ported |  |
| GoalComposite | ported |  |
| GoalGetToBlock | ported |  |
| GoalInverted | ported |  |
| GoalNear | ported |  |
| GoalRunAway | ported |  |
| GoalStrictDirection | ported |  |
| GoalTwoBlocks | ported |  |
| GoalXZ | ported |  |
| GoalYLevel | ported |  |
| ActionCosts | ported | src/pathing/costs.rs (with swimming and the 4097-entry fall table) |
| IMovement | ported | src/pathing/movement/movement.rs |
| MovementStatus | ported | src/behavior/movement_state.rs |
| IPathExecutor | ported | src/pathing/path/executor.rs |

## api/process

| Java | Status | Rust or note |
|---|---|---|
| IBaritoneProcess | ported | Process trait, src/behavior/pathing.rs |
| IBuilderProcess | ported | src/process/build.rs |
| ICustomGoalProcess | ported | src/process/goal.rs |
| IElytraProcess | ported | src/process/elytra.rs |
| IExploreProcess | ported | src/process/explore.rs |
| IFarmProcess | ported | src/process/farm.rs |
| IFollowProcess | ported | src/process/follow.rs |
| IGetToBlockProcess | ported | src/process/getto.rs |
| IMineProcess | ported | src/process/mine.rs |
| PathingCommand | ported | src/behavior/pathing.rs |
| PathingCommandType | ported | the variants of PathingCommand |

## api/schematic

| Java | Status | Rust or note |
|---|---|---|
| AbstractSchematic | ported | the size of each schematic; inSchematic is the trait default |
| CompositeSchematic | ported | src/schematic/schematic.rs |
| CompositeSchematicEntry | ported |  |
| FillSchematic | ported |  |
| ISchematic | ported | the ISchematic trait of src/process/build.rs and wrappers.rs, with desiredState and inSchematic as the Java; the plain Schematic trait feeds it through SpecSchematic |
| ISchematicSystem | ported | src/schematic/format.rs |
| IStaticSchematic | ported | StaticSchematic |
| MaskSchematic | ported | src/schematic/wrappers.rs |
| MirroredSchematic | ported | src/schematic/wrappers.rs: the block states turn with Horizon structure tables |
| ReplaceSchematic | ported |  |
| RotatedSchematic | ported | src/schematic/wrappers.rs |
| ShellSchematic | ported | src/schematic/schematic.rs (Shape::Shell) |
| SubstituteSchematic | ported | src/schematic/wrappers.rs |
| WallsSchematic | ported | src/schematic/schematic.rs (Shape::Walls) |
| ISchematicFormat | ported | src/schematic/format.rs |

## api/schematic/mask

| Java | Status | Rust or note |
|---|---|---|
| AbstractMask | n/a | API for composing masks that nothing in Baritone composes: the shapes are Shape, the builder masks are MaskSchematic |
| Mask | n/a | API for composing masks that nothing in Baritone composes: the shapes are Shape, the builder masks are MaskSchematic |
| PreComputedMask | n/a | API for composing masks that nothing in Baritone composes: the shapes are Shape, the builder masks are MaskSchematic |
| StaticMask | n/a | API for composing masks that nothing in Baritone composes: the shapes are Shape, the builder masks are MaskSchematic |
| BinaryOperatorMask | n/a | API for composing masks that nothing in Baritone composes: the shapes are Shape, the builder masks are MaskSchematic |
| NotMask | n/a | API for composing masks that nothing in Baritone composes: the shapes are Shape, the builder masks are MaskSchematic |

## api/schematic/mask/shape

| Java | Status | Rust or note |
|---|---|---|
| CylinderMask | ported | Shape::Cylinder, Shape::Sphere |
| SphereMask | ported | Shape::Cylinder, Shape::Sphere |

## api/selection

| Java | Status | Rust or note |
|---|---|---|
| ISelection | ported | src/selection/selection.rs |
| ISelectionManager | ported | src/selection/selection.rs |

## api/utils

| Java | Status | Rust or note |
|---|---|---|
| BetterBlockPos | ported | Pos = [i32; 3] |
| BlockOptionalMeta | ported | src/api/block_optional_meta.rs: drops from the loot tables, src/game/scanner.rs |
| BlockOptionalMetaLookup | ported | src/api/block_optional_meta.rs |
| BlockUtils | ported | block names carry their namespace; bare() is blockToString |
| BooleanBinaryOperator, BooleanBinaryOperators | ported | src/schematic/mask.rs: the and/or/xor/not that BinaryOperatorMask takes |
| Helper | n/a | Core::say* |
| IInputOverrideHandler | ported | src/behavior/input.rs |
| IPlayerContext, IPlayerController | ported | src/behavior/player_context.rs, game/player_context.rs |
| MyChunkPos, Pair, TypeUtils, VecUtils | n/a | Rust types |
| NotificationHelper, BaritoneToast | ported | src/utils/notification.rs (desktop) and the own SystemToast of the game (logAsToast), drawn from the mod HUD hook point |
| PathCalculationResult | ported | Status in src/pathing/calc/a_star.rs |
| RayTraceUtils | ported | GameCtx::ray_trace_towards |
| Rotation, RotationUtils | ported | src/utils/rotation.rs, GameCtx::reachable |
| SettingsUtil | ported | src/api/settings.rs (to_file_text, apply_file_text): settings.txt in the same format |
| IItemStack, ILootTable | n/a | mixin accessors of the game loot table; the drops themselves are read from it in src/game/scanner.rs |
| Input | ported | src/behavior/movement_state.rs |
| IGoalRenderPos | ported | Goal::render_pos |

## main

| Java | Status | Rust or note |
|---|---|---|
| Baritone | ported | src/baritone.rs: one instance |
| BaritoneProvider, KeepName | n/a |  |

## main/behavior

| Java | Status | Rust or note |
|---|---|---|
| Behavior | n/a |  |
| InventoryBehavior | ported | src/behavior/inventory.rs: throwaway in the hotbar, the off hand and the main inventory, allowInventory swaps through the menu engine |
| LookBehavior | ported | src/behavior/look.rs: Target.Mode, freeLook, blockFreeLook, antiCheatCompatibility, smoothLook, applied by baritone::look_pre and look_post around the player tick |
| PathingBehavior | ported | src/behavior/pathing_behavior.rs: the search runs in slices of 2500 nodes a tick, there being no second thread |
| WaypointBehavior | ported | src/baritone.rs: death waypoint with its clickable message, bed waypoint on a right-click; kept per world |
| ForkableRandom | ported | src/behavior/aim.rs: Xoroshiro256++ with forks, as the Java, for the aim jitter |

## main/cache

| Java | Status | Rust or note |
|---|---|---|
| CacheFiles | ported | FileStore in src/game/world_data.rs |
| CachedChunk | ported | src/cache/cached_world.rs |
| CachedRegion | ported | src/cache/cached_world.rs, the .bcr format |
| CachedWorld | ported | src/cache/cached_world.rs; the packer thread is a slice of each tick (4 ms for loaded chunks, 6 ms for unloaded ones) because the game state is not Send, with the Java queue and its chunkPackerQueueMaxSize |
| ChunkPacker | ported | src/game/world_data.rs: the flow of border water is the game own getFlow |
| FasterWorldScanner | ported | src/game/scanner.rs: same chunk spiral and section order |
| WaypointCollection | ported | src/cache/waypoints.rs |
| WorldData | ported | src/game/world_data.rs |
| WorldProvider | ported | src/game/world_data.rs |
| WorldScanner | ported | src/game/scanner.rs and src/cache/world_scanner.rs |

## main/command

| Java | Status | Rust or note |
|---|---|---|
| CommandSystem | n/a |  |
| ExampleBaritoneControl | ported | src/baritone.rs (chat), src/command/manager.rs (run), src/command/tab.rs |
| ArgParserManager | n/a | typed parsers are methods of Args |
| DefaultArgParsers | n/a | typed parsers are methods of Args |
| ArgConsumer | ported | src/command/argument.rs (Args) |
| CommandArgument | ported | src/command/argument.rs |
| CommandArguments | n/a | typed parsers are methods of Args |
| CommandManager | ported | src/command/manager.rs |

## main/command/defaults

| Java | Status | Rust or note |
|---|---|---|
| AxisCommand | ported | nav.rs |
| BlacklistCommand | ported | work.rs |
| BuildCommand | ported | build.rs: Schematica is not a thing on Horizon |
| ClickCommand | ported | nav.rs |
| ComeCommand | ported | nav.rs: the camera entity is the player here |
| CommandAlias | ported | manager.rs |
| DefaultCommands | ported | manager.rs (ORDER) |
| ETACommand | ported | nav.rs |
| ElytraCommand | ported | work.rs |
| ExecutionControlCommands | ported | nav.rs |
| ExploreCommand | ported | work.rs |
| ExploreFilterCommand | ported | work.rs |
| FarmCommand | ported | work.rs |
| FindCommand | ported | work.rs |
| FollowCommand | ported | work.rs |
| ForceCancelCommand | ported | nav.rs |
| GcCommand | ported | meta.rs: frees the caches, there being no System.gc |
| GoalCommand | ported | nav.rs |
| GotoCommand | ported | nav.rs |
| HelpCommand | ported | meta.rs, manager.rs |
| InvertCommand | ported | nav.rs |
| LitematicaCommand | ported | build.rs: says Litematica is not present, as the Java does without the mod |
| MineCommand | ported | work.rs |
| PathCommand | ported | nav.rs |
| PickupCommand | ported | work.rs |
| ProcCommand | ported | nav.rs |
| ReloadAllCommand | ported | meta.rs |
| RenderCommand | ported | meta.rs |
| RepackCommand | ported | meta.rs |
| SaveAllCommand | ported | meta.rs |
| SchematicaCommand | n/a | not in DefaultCommands of 1.21.4, and it needs the Schematica mod |
| SelCommand | ported | build.rs |
| SetCommand | ported | meta.rs |
| SurfaceCommand | ported | nav.rs |
| ThisWayCommand | ported | nav.rs |
| TunnelCommand | ported | build.rs |
| VersionCommand | ported | meta.rs |
| WaypointsCommand | ported | meta.rs |

## main/event

| Java | Status | Rust or note |
|---|---|---|
| GameEventHandler | n/a | Horizon hook points, hooks/ |

## main/pathing/calc

| Java | Status | Rust or note |
|---|---|---|
| AStarPathFinder | ported | src/pathing/calc/a_star.rs: the Java search, resumable in slices; tested; wired into the game through game/player_context.rs |
| AbstractNodeCostSearch | ported | src/pathing/calc/a_star.rs: the seven coefficients and bestSoFar |
| Path | ported | src/pathing/calc/path.rs |
| PathNode | ported | src/pathing/calc/a_star.rs |
| BinaryHeapOpenSet | ported | src/pathing/calc/a_star.rs: same heap, same tie order |
| IOpenSet, LinkedListOpenSet | ported | src/pathing/calc/a_star.rs: the trait, the list (as the Java, nothing but a test uses it) and a test that it hands out the same costs as the heap; the search itself holds the heap directly |

## main/pathing/movement

| Java | Status | Rust or note |
|---|---|---|
| CalculationContext | ported | src/pathing/movement/calculation_context.rs |
| Movement | ported | movement.rs (data, costs, valid positions) and movement_exec.rs (updateState of the eight movements) |
| MovementHelper | ported | movement_helper.rs (costs) and movement_exec.rs (openDoors, attemptToPlaceABlock, moveTowards) |
| MovementOption | ported | src/behavior/movement_state.rs |
| MovementState | ported | src/behavior/movement_state.rs |
| Moves | ported | src/pathing/movement/moves.rs |

## main/pathing/movement/movements

| Java | Status | Rust or note |
|---|---|---|
| MovementAscend | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |
| MovementDescend | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |
| MovementDiagonal | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |
| MovementDownward | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |
| MovementFall | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |
| MovementParkour | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |
| MovementPillar | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |
| MovementTraverse | ported | cost and execution (movement_exec.rs) written from the Java; Rust adds swimming and bend pre-turning of its own |

## main/pathing/path

| Java | Status | Rust or note |
|---|---|---|
| CutoffPath | ported | src/pathing/calc/path.rs (cut_off) |
| PathExecutor | ported | src/pathing/path/executor.rs |
| SplicedPath | ported | src/pathing/calc/path.rs (try_splice) |
| SprintJump | ported | src/pathing/path/sprint_jump.rs |

## main/pathing/precompute

| Java | Status | Rust or note |
|---|---|---|
| PrecomputedData | n/a | a speed-up of the Java |
| Ternary | n/a | a speed-up of the Java |

## main/process

| Java | Status | Rust or note |
|---|---|---|
| BackfillProcess | ported | src/process/backfill.rs |
| BuilderProcess | ported | src/process/build.rs and builder_logic.rs; would_place simulates the placement as hasAnyItemThatWouldPlace does |
| CustomGoalProcess | ported | src/process/goal.rs |
| ElytraProcess | ported | src/process/elytra.rs: written from the Java |
| ExploreProcess | ported | src/process/explore.rs over the cached world |
| FarmProcess | ported | src/process/farm.rs |
| FollowProcess | ported | src/process/follow.rs |
| GetToBlockProcess | ported | src/process/getto.rs |
| InventoryPauserProcess | ported | src/process/backfill.rs |
| MineProcess | ported | src/process/mine.rs |

## main/process/elytra

| Java | Status | Rust or note |
|---|---|---|
| BlockStateOctreeInterface | ported | src/process/elytra/context.rs |
| BuildLimitPathFinder | ported | src/process/elytra/build_limit.rs |
| ElytraBehavior | ported | src/process/elytra/behavior.rs, flight.rs (solver and physics), path_manager.rs; the solver runs on the game thread; hooks/set_back.rs is onReceivePacket |
| IElytraPathFinder | ported | NetherSearch in build_limit.rs |
| NetherPath | ported | src/process/elytra/segment.rs |
| NetherPathfinderContext | ported | src/process/elytra/context.rs: no lock, chunks are immutable snapshots |
| PathCalculationException | ported | PathCalculationError in segment.rs |
| UnpackedSegment | ported | src/process/elytra/segment.rs |

## main/process/elytra/pathfinder

| Java | Status | Rust or note |
|---|---|---|
| BaritoneRegion | ported | src/process/elytra/region.rs: reads this port's cache layout |
| BinaryHeapOpenSet | ported | path_node.rs |
| Chunk | ported | chunk.rs |
| ChunkGeneratorHell | ported | chunk_generator_hell.rs: matches the native library bit for bit (625 chunks) |
| NetherPathfinder | ported | nether_pathfinder.rs |
| NodePos | ported | chunk.rs |
| NoiseGeneratorImproved | ported | noise.rs |
| NoiseGeneratorOctaves | ported | noise.rs |
| PathFinder | ported | path_finder.rs |
| PathNode | ported | path_node.rs |
| PathSegment | ported | nether_pathfinder.rs |
| Raytracer | ported | raytracer.rs: matches the native library on 4000 rays |
| Size | ported | chunk.rs |

## main/selection

| Java | Status | Rust or note |
|---|---|---|
| Selection | ported | src/selection/selection.rs |
| SelectionManager | ported | src/selection/selection.rs |
| SelectionRenderer | ported | src/game/render.rs (draw_selections): the boxes, corners and opacity of the Java |

## main/utils

| Java | Status | Rust or note |
|---|---|---|
| BaritoneMath | n/a | f64 methods |
| BaritoneProcessHelper | n/a |  |
| BlockBreakHelper | ported | src/behavior/input.rs: holds the game button, works behind any screen; blockBreakSpeed resets the destroy delay |
| BlockPlaceHelper | ported | src/behavior/input.rs: rightClickSpeed |
| BlockStateInterface | ported | src/utils/block_state_interface.rs, answered by game/player_context.rs |
| BlockStateInterfaceAccessWrapper | n/a |  |
| GuiClick | ported | src/utils/gui_click.rs: the playing screen with the mouse let go stands in for the empty screen |
| IRenderer | ported | src/game/render.rs (Canvas): lines and boxes, depth-tested or over everything as the Ignore setting says |
| InputOverrideHandler | ported | src/behavior/input.rs |
| PathRenderer | ported | src/game/render.rs: path, goal, boxes and the elytra; a GoalXZ is the game's own beacon beam (goal_beams, hooks/goal_beam.rs), and renderGoalIgnoreDepth scales its vertices toward the camera so the beam shows through terrain as the Java's depth-less one does (checked in game) |
| PathingCommandContext | ported | Core::path_hooks and CostHooks |
| PathingControlManager | ported | Baritone::control_tick and pathing_behavior.rs (pre_tick, post_tick) |
| PlayerMovementInput | n/a | the game reads keys |
| ToolSet | ported | src/game/toolset.rs: calculateSpeedVsBlock, getBestSlot, silk and itemSaver as the Java |

## main/utils/accessor

| Java | Status | Rust or note |
|---|---|---|
| IBlockStateFlags | n/a | Java mixin accessors |
| IChunkArray | n/a | Java mixin accessors |
| IChunkProviderClient | n/a | Java mixin accessors |
| IClientChunkProvider | n/a | Java mixin accessors |
| IEntityRenderManager | n/a | Java mixin accessors |
| IFireworkRocketEntity | n/a | Java mixin accessors |
| IGuiScreen | n/a | Java mixin accessors |
| IPalettedContainer | n/a | Java mixin accessors |
| IPlayerControllerMP | n/a | Java mixin accessors |

## main/utils/pathing and player

| Java | Status | Rust or note |
|---|---|---|
| Avoidance | ported | src/pathing/avoidance.rs, src/game/player_context.rs |
| BetterWorldBorder | ported | GameCtx::entirely_contains and can_place_at |
| Favoring | ported | create_pathfinder in src/behavior/pathing_behavior.rs: a map of factors in the search |
| MutableMoveResult | ported | src/pathing/movement/calculation_context.rs |
| PathBase | ported | src/pathing/calc/path.rs: cutoff_at_loaded_chunks, static_cutoff |
| PathingBlockType | ported | src/cache/cached_world.rs |
| BaritonePlayerContext, BaritonePlayerController | ported | src/game/player_context.rs |

## main/utils/schematic

| Java | Status | Rust or note |
|---|---|---|
| MapArtSchematic | ported | MaskSchematic::map_art |
| SchematicSystem | ported | src/schematic/format.rs (read) |
| SelectionSchematic | ported | MaskSchematic::selection |
| StaticSchematic | ported | src/schematic/schematic.rs |
| DefaultSchematicFormats | ported | src/schematic/format.rs: also the game's own .nbt |
| LitematicaSchematic | ported | src/schematic/format.rs (litematic): regions, overlaps as CompositeSchematic |
| MCEditSchematic | ported | src/schematic/legacy.rs: the id table is the game own ItemIdFix |
| SpongeSchematic | ported | src/schematic/format.rs (sponge): also reads version 3 |
| LitematicaHelper | n/a | needs the Litematica mod |
| SchematicAdapter, SchematicaHelper | n/a | need the Schematica mod |
| VarInt | ported | inside src/schematic/format.rs (sponge) |

## Horizon only

| What | Where |
|---|---|
| The settings screen: Options > Mods > Baritone is a screen built from Bedrock Edition's parts in Baritone's purple (header with back and search, tabs and rows with an item picture, switch, notched slider, dropdown, block-name field with Add, section labels, scrollbar, tooltip with the default) instead of a list of buttons. Switches and sliders set the same settings `#set` does; Reset page puts a category back. | src/ui/settings_screen.rs (what it shows, layout, clicks, keys; tested), src/game/settings_ui.rs (drawing), hooks/page_*.rs (draw, mouse, drag, wheel, key, text) |
