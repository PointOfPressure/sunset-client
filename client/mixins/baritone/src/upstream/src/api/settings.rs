// baritone.api.Settings: every setting, by Baritone's name, with its type, default and what it does.
// Only settings that something here reads are listed, so none of them is a switch that does nothing.
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bool,
    Int,
    Float,
    Text,
    /// `RRGGBB`.
    Color,
    /// Block names, comma separated: `stone,dirt`.
    List,
    /// `x,y,z`.
    Vec3,
    /// `NONE`, `CLOCKWISE_90`, `CLOCKWISE_180` or `COUNTERCLOCKWISE_90`.
    Rotation,
    /// `NONE`, `LEFT_RIGHT` or `FRONT_BACK`.
    Mirror,
    /// Blocks to the blocks that stand for them: `stone->cobblestone,andesite,oak_planks->birch_planks`.
    BlockMap,
}

pub struct Def {
    pub name: &'static str,
    pub kind: Kind,
    pub default: &'static str,
    #[allow(dead_code)]
    pub desc: &'static str,
}

macro_rules! def {
    ($name:literal, $kind:ident, $default:literal, $desc:literal) => {
        Def { name: $name, kind: Kind::$kind, default: $default, desc: $desc }
    };
}

/// Every setting of Baritone 1.21.4 that `#set` shows (the ones marked JavaOnly are left out, as the Java
/// command leaves them out), in the Java's order, with its default and the first sentence of its
/// documentation. Generated from `baritone/api/Settings.java`. The last one is Horizon's own.
pub const DEFS: &[Def] = &[
    def!("allowBreak", Bool, "true", "Allow Baritone to break blocks"),
    def!("allowBreakAnyway", List, "", "Blocks that baritone will be allowed to break even with allowBreak set to false"),
    def!("allowSprint", Bool, "true", "Allow Baritone to sprint"),
    def!("allowPlace", Bool, "true", "Allow Baritone to place blocks"),
    def!("allowPlaceInFluidsSource", Bool, "true", "Allow Baritone to place blocks in fluid source blocks"),
    def!("allowPlaceInFluidsFlow", Bool, "true", "Allow Baritone to place blocks in flowing fluid"),
    def!("allowInventory", Bool, "false", "Allow Baritone to move items in your inventory to your hotbar"),
    def!("ticksBetweenInventoryMoves", Int, "1", "Wait this many ticks between InventoryBehavior moving inventory items"),
    def!("inventoryMoveOnlyIfStationary", Bool, "false", "Come to a halt before doing any inventory moves. Intended for anticheat such as 2b2t"),
    def!("assumeExternalAutoTool", Bool, "false", "Disable baritone's auto-tool at runtime, but still assume that another mod will provide auto tool functionality Specifically, path calculation will still assume that an auto tool will run at execution"),
    def!("autoTool", Bool, "true", "Automatically select the best available tool"),
    def!("blockPlacementPenalty", Float, "20", "It doesn't actually take twenty ticks to place a block, this cost is so high because we want to generally conserve blocks which might be limited"),
    def!("blockBreakAdditionalPenalty", Float, "2", "This is just a tiebreaker to make it less likely to break blocks if it can avoid it"),
    def!("jumpPenalty", Float, "2", "Additional penalty for hitting the space bar (ascend, diagonal ascend, pillar, or parkour) because it uses hunger"),
    def!("walkOnWaterOnePenalty", Float, "3", "Walking on water uses up hunger really quick, so penalize it"),
    def!("strictLiquidCheck", Bool, "false", "Don't allow breaking blocks next to liquids. Enable if you have mods adding custom fluid physics."),
    def!("allowWaterBucketFall", Bool, "true", "Allow Baritone to fall arbitrary distances and place a water bucket beneath it. Reliability: questionable."),
    def!("assumeWalkOnWater", Bool, "false", "Allow Baritone to assume it can walk on still water just like any other block"),
    def!("assumeWalkOnLava", Bool, "false", "If you have Fire Resistance and Jesus then I guess you could turn this on lol"),
    def!("assumeStep", Bool, "false", "Assume step functionality; don't jump on an Ascend."),
    def!("assumeSafeWalk", Bool, "false", "Assume safe walk functionality; don't sneak on a backplace traverse"),
    def!("allowJumpAtBuildLimit", Bool, "false", "If true, parkour is allowed to make jumps when standing on blocks at the maximum height, so player feet is y=256 Defaults to false because this fails on constantiam"),
    def!("allowParkourAscend", Bool, "true", "This should be monetized it's so good Defaults to true, but only actually takes effect if allowParkour is also true"),
    def!("allowDiagonalDescend", Bool, "false", "Allow descending diagonally Safer than allowParkour yet still slightly unsafe, can make contact with unchecked adjacent blocks, so it's unsafe in the nether"),
    def!("allowDiagonalAscend", Bool, "false", "Allow diagonal ascending Actually pretty safe, much safer than diagonal descend tbh"),
    def!("allowDownward", Bool, "true", "Allow mining the block directly beneath its feet Turn this off to force it to make more staircases and less shafts"),
    def!("acceptableThrowawayItems", List, "dirt,cobblestone,netherrack,stone", "Blocks that Baritone is allowed to place (as throwaway, for sneak bridging, pillaring, etc.)"),
    def!("blocksToAvoid", List, "tripwire", "Blocks that Baritone will attempt to avoid (Used in avoidance)"),
    def!("blocksToDisallowBreaking", List, "", "Blocks that Baritone is not allowed to break"),
    def!("blocksToAvoidBreaking", List, "crafting_table,furnace,chest,trapped_chest", "blocks that baritone shouldn't break, but can if it needs to."),
    def!("avoidBreakingMultiplier", Float, "0.1", "this multiplies the break speed, if set above 1 it's \"encourage breaking\" instead"),
    def!("buildIgnoreBlocks", List, "", "A list of blocks to be treated as if they're air"),
    def!("buildSkipBlocks", List, "", "A list of blocks to be treated as correct"),
    def!("buildValidSubstitutes", BlockMap, "", "A mapping of blocks to blocks treated as correct in their position"),
    def!("buildSubstitutes", BlockMap, "", "A mapping of blocks to blocks to be built instead"),
    def!("okIfAir", List, "", "A list of blocks to become air If a schematic asks for a block on this list, only air will be accepted at that location (and nothing on buildIgnoreBlocks)"),
    def!("buildIgnoreExisting", Bool, "false", "If this is true, the builder will treat all non-air blocks as correct. It will only place new blocks."),
    def!("buildIgnoreDirection", Bool, "false", "If this is true, the builder will ignore directionality of certain blocks like glazed terracotta."),
    def!("buildIgnoreProperties", List, "", "A list of names of block properties the builder will ignore."),
    def!("avoidUpdatingFallingBlocks", Bool, "true", "If this setting is true, Baritone will never break a block that is adjacent to an unsupported falling block"),
    def!("allowVines", Bool, "false", "Enables some more advanced vine features"),
    def!("allowWalkOnBottomSlab", Bool, "true", "Slab behavior is complicated, disable this for higher path reliability. Leave enabled if you have bottom slabs everywhere in your base."),
    def!("allowParkour", Bool, "false", "You know what it is But it's very unreliable and falls off when cornering like all the time so"),
    def!("allowParkourPlace", Bool, "false", "Actually pretty reliable. Doesn't make it any more dangerous compared to just normal allowParkour th"),
    def!("considerPotionEffects", Bool, "true", "For example, if you have Mining Fatigue or Haste, adjust the costs of breaking blocks accordingly."),
    def!("sprintAscends", Bool, "true", "Sprint and jump a block early on ascends wherever possible"),
    def!("headHitters", Bool, "false", "Sprint jump in 1x2 corridors and whenever walking under a low ceiling, bonking our head on it"),
    def!("headHittersDiagonal", Bool, "true", "Also sprint jump under low ceilings on flat diagonal movements when #headHitters is enabled"),
    def!("sprintJumping", Bool, "false", "Sprint jump along straight stretches of path, up single block steps and down small hills"),
    def!("sprintJumpingDiagonals", Bool, "true", "Also sprint jump along flat diagonal stretches of path when #sprintJumping is enabled."),
    def!("overshootTraverse", Bool, "true", "If we overshoot a traverse and end up one block beyond the destination, mark it as successful anyway. This helps with speed exceeding 20m/s"),
    def!("pauseMiningForFallingBlocks", Bool, "true", "When breaking blocks for a movement, wait until all falling blocks have settled before continuing"),
    def!("rightClickSpeed", Int, "4", "How many ticks between right clicks are allowed. Default in game is 4"),
    def!("randomLooking113", Float, "2", "How many degrees to randomize the yaw every tick. Set to 0 to disable"),
    def!("blockReachDistance", Float, "4.5", "Block reach distance"),
    def!("blockBreakSpeed", Int, "6", "How many ticks between breaking a block and starting to break the next block"),
    def!("randomLooking", Float, "0.01", "How many degrees to randomize the pitch and yaw every tick. Set to 0 to disable"),
    def!("costHeuristic", Float, "3.563", "This is the big A* setting"),
    def!("pathingMaxChunkBorderFetch", Int, "50", "The maximum number of times it will fetch outside loaded or cached chunks before assuming that pathing has reached the end of the known area, and should therefore stop."),
    def!("backtrackCostFavoringCoefficient", Float, "0.5", "Set to 1.0 to effectively disable this feature @see Issue #18"),
    def!("avoidance", Bool, "false", "Toggle the following 4 settings They have a noticeable performance impact, so they default off Specifically, building up the avoidance map on the main thread before pathing starts actually takes a not"),
    def!("mobSpawnerAvoidanceCoefficient", Float, "2", "Set to 1.0 to effectively disable this feature Set below 1.0 to go out of your way to walk near mob spawners"),
    def!("mobSpawnerAvoidanceRadius", Int, "16", "Distance to avoid mob spawners."),
    def!("mobAvoidanceCoefficient", Float, "1.5", "Set to 1.0 to effectively disable this feature Set below 1.0 to go out of your way to walk near mobs"),
    def!("mobAvoidanceRadius", Int, "8", "Distance to avoid mobs."),
    def!("rightClickContainerOnArrival", Bool, "true", "When running a goto towards a container block (chest, ender chest, furnace, etc), right click and open it once you arrive."),
    def!("enterPortal", Bool, "true", "When running a goto towards a nether portal block, walk all the way into the portal instead of stopping one block before."),
    def!("minimumImprovementRepropagation", Bool, "true", "Don't repropagate cost improvements below 0.01 ticks. They're all just floating point inaccuracies, and there's no point."),
    def!("cutoffAtLoadBoundary", Bool, "false", "After calculating a path (potentially through cached chunks), artificially cut it off to just the part that is entirely within currently loaded chunks"),
    def!("maxCostIncrease", Float, "10", "If a movement's cost increases by more than this amount between calculation and execution (due to changes in the environment / world), cancel and recalculate"),
    def!("costVerificationLookahead", Int, "5", "Stop 5 movements before anything that made the path COST_INF"),
    def!("pathCutoffFactor", Float, "0.9", "Static cutoff factor. 0.9 means cut off the last 10% of all paths, regardless of chunk load state"),
    def!("pathCutoffMinimumLength", Int, "30", "Only apply static cutoff for paths of at least this length (in terms of number of movements)"),
    def!("planningTickLookahead", Int, "150", "Start planning the next path once the remaining movements tick estimates sum up to less than this value"),
    def!("pathingMapDefaultSize", Int, "1024", "Default size of the Long2ObjectOpenHashMap used in pathing"),
    def!("pathingMapLoadFactor", Float, "0.75", "Load factor coefficient for the Long2ObjectOpenHashMap used in pathing Decrease for faster map operations, but higher memory usage"),
    def!("maxFallHeightNoWater", Int, "3", "How far are you allowed to fall onto solid ground (without a water bucket)? 3 won't deal any damage"),
    def!("maxFallHeightBucket", Int, "20", "How far are you allowed to fall onto solid ground (with a water bucket)? It's not that reliable, so I've set it below what would kill an unarmored player (23)"),
    def!("allowOvershootDiagonalDescend", Bool, "true", "Is it okay to sprint through a descend followed by a diagonal? The player overshoots the landing, but not enough to fall off"),
    def!("simplifyUnloadedYCoord", Bool, "true", "If your goal is a GoalBlock in an unloaded chunk, assume it's far enough away that the Y coord doesn't matter yet, and replace it with a GoalXZ to the same place before calculating a path"),
    def!("repackOnAnyBlockChange", Bool, "true", "Whenever a block changes, repack the whole chunk that it's in"),
    def!("movementTimeoutTicks", Int, "100", "If a movement takes this many ticks more than its initial cost estimate, cancel it"),
    def!("primaryTimeoutMS", Int, "500", "Pathing ends after this amount of time, but only if a path has been found If no valid path (length above the minimum) has been found, pathing continues up until the failure timeout"),
    def!("failureTimeoutMS", Int, "2000", "Pathing can never take longer than this, even if that means failing to find any path at all"),
    def!("planAheadPrimaryTimeoutMS", Int, "4000", "Planning ahead while executing a segment ends after this amount of time, but only if a path has been found If no valid path (length above the minimum) has been found, pathing continues up until the fa"),
    def!("planAheadFailureTimeoutMS", Int, "5000", "Planning ahead while executing a segment can never take longer than this, even if that means failing to find any path at all"),
    def!("slowPath", Bool, "false", "For debugging, consider nodes much much slower"),
    def!("slowPathTimeDelayMS", Int, "100", "Milliseconds between each node"),
    def!("slowPathTimeoutMS", Int, "40000", "The alternative timeout number when slowPath is on"),
    def!("doBedWaypoints", Bool, "true", "allows baritone to save bed waypoints when interacting with beds"),
    def!("doDeathWaypoints", Bool, "true", "allows baritone to save death waypoints"),
    def!("chunkCaching", Bool, "true", "The big one. Download all chunks in simplified 2-bit format and save them for better very-long-distance pathing."),
    def!("pruneRegionsFromRAM", Bool, "true", "On save, delete from RAM any cached regions that are more than 1024 blocks away from the player Temporarily disabled Temporarily reenabled @see Issue #248"),
    def!("chunkPackerQueueMaxSize", Int, "2000", "The chunk packer queue can never grow to larger than this, if it does, the oldest chunks are discarded The newest chunks are kept, so that if you're moving in a straight line quickly then stop, your i"),
    def!("backfill", Bool, "false", "Fill in blocks behind you"),
    def!("logAsToast", Bool, "false", "Shows popup message in the upper right corner, similarly to when you make an advancement"),
    def!("chatDebug", Bool, "false", "Print all the debug messages to chat"),
    def!("chatControl", Bool, "true", "Allow chat based control of Baritone. Most likely should be disabled when Baritone is imported for use in something else"),
    def!("chatControlAnyway", Bool, "false", "Some clients like Impact try to force chatControl to off, so here's a second setting to do it anyway"),
    def!("renderPath", Bool, "true", "Render the path"),
    def!("renderPathAsLine", Bool, "false", "Render the path as a line instead of a frickin thingy"),
    def!("renderGoal", Bool, "true", "Render the goal"),
    def!("renderGoalAnimated", Bool, "true", "Render the goal as a sick animated thingy instead of just a box (also controls animation of GoalXZ if #renderGoalXZBeacon is enabled)"),
    def!("renderSelectionBoxes", Bool, "true", "Render selection boxes"),
    def!("renderGoalIgnoreDepth", Bool, "true", "Ignore depth when rendering the goal"),
    def!("renderGoalXZBeacon", Bool, "false", "Renders X/Z type Goals with the vanilla beacon beam effect. Combining this with #renderGoalIgnoreDepth will cause strange render clipping."),
    def!("renderSelectionBoxesIgnoreDepth", Bool, "true", "Ignore depth when rendering the selection boxes (to break, to place, to walk into)"),
    def!("renderPathIgnoreDepth", Bool, "true", "Ignore depth when rendering the path"),
    def!("pathRenderLineWidthPixels", Float, "5", "Line width of the path when rendered, in pixels"),
    def!("goalRenderLineWidthPixels", Float, "3", "Line width of the goal when rendered, in pixels"),
    def!("fadePath", Bool, "false", "Start fading out the path at 20 movements ahead, and stop rendering it entirely 30 movements ahead. Improves FPS."),
    def!("freeLook", Bool, "true", "Move without having to force the client-sided rotations"),
    def!("blockFreeLook", Bool, "false", "Break and place blocks without having to force the client-sided rotations. Requires #freeLook."),
    def!("elytraFreeLook", Bool, "true", "Automatically elytra fly without having to force the client-sided rotations."),
    def!("smoothLook", Bool, "false", "Forces the client-sided yaw rotation to an average of the last #smoothLookTicks of server-sided rotations."),
    def!("elytraSmoothLook", Bool, "false", "Same as #smoothLook but for elytra flying."),
    def!("smoothLookTicks", Int, "5", "The number of ticks to average across for #smoothLook;"),
    def!("remainWithExistingLookDirection", Bool, "true", "When true, the player will remain with its existing look direction as often as possible"),
    def!("antiCheatCompatibility", Bool, "true", "Will cause some minor behavioral differences to ensure that Baritone works on anticheats"),
    def!("pathThroughCachedOnly", Bool, "false", "Exclusively use cached chunks for pathing Never turn this on"),
    def!("sprintInWater", Bool, "true", "Continue sprinting while in water"),
    def!("allowSwimming", Bool, "false", "Actually swim through water instead of bobbing along the bottom of it Swimming works the same way it does for a player: hold sprint (ctrl) while in water to enter the swim state, then steer with yaw a"),
    def!("blacklistClosestOnFailure", Bool, "true", "When GetToBlockProcess or MineProcess fails to calculate a path, instead of just giving up, mark the closest instance of that block as \"unreachable\" and go towards the next closest"),
    def!("renderCachedChunks", Bool, "false", " Render cached chunks as semitransparent"),
    def!("cachedChunksOpacity", Float, "0.5", "0.0f = not visible, fully transparent (instead of setting this to 0, turn off renderCachedChunks) 1.0f = fully opaque"),
    def!("prefixControl", Bool, "true", "Whether or not to allow you to run Baritone commands with the prefix"),
    def!("prefix", Text, "#", "The command prefix for chat control"),
    def!("shortBaritonePrefix", Bool, "false", "Use a short Baritone prefix [B] instead of [Baritone] when logging to chat"),
    def!("useMessageTag", Bool, "false", "Use a modern message tag instead of a prefix when logging to chat"),
    def!("echoCommands", Bool, "true", "Echo commands to chat when they are run"),
    def!("censorCoordinates", Bool, "false", "Censor coordinates in goals and block positions"),
    def!("censorRanCommands", Bool, "false", "Censor arguments to ran commands, to hide, for example, coordinates to #goal"),
    def!("itemSaver", Bool, "false", "Stop using tools just before they are going to break."),
    def!("itemSaverThreshold", Int, "10", "Durability to leave on the tool when using itemSaver"),
    def!("preferSilkTouch", Bool, "false", "Always prefer silk touch tools over regular tools"),
    def!("walkWhileBreaking", Bool, "true", "Don't stop walking forward when you need to break blocks in your way"),
    def!("splicePath", Bool, "true", "When a new segment is calculated that doesn't overlap with the current one, but simply begins where the current segment ends, splice it on and make a longer combined path"),
    def!("maxPathHistoryLength", Int, "300", "If we are more than 300 movements into the current path, discard the oldest segments, as they are no longer useful"),
    def!("pathHistoryCutoffAmount", Int, "50", "If the current path is too long, cut off this many movements from the beginning."),
    def!("mineGoalUpdateInterval", Int, "5", "Rescan for the goal once every 5 ticks. Set to 0 to disable."),
    def!("maxCachedWorldScanCount", Int, "10", "After finding this many instances of the target block in the cache, it will stop expanding outward the chunk search."),
    def!("mineMaxOreLocationsCount", Int, "64", "Mine will not scan for or remember more than this many target locations"),
    def!("minYLevelWhileMining", Int, "0", "Sets the minimum y level whilst mining - set to 0 to turn off. if world has negative y values, subtract the min world height to get the value to put here"),
    def!("maxYLevelWhileMining", Int, "2031", "Sets the maximum y level to mine ores at."),
    def!("allowOnlyExposedOres", Bool, "false", "This will only allow baritone to mine exposed ores, can be used to stop ore obfuscators on servers that use them."),
    def!("allowOnlyExposedOresDistance", Int, "1", "When allowOnlyExposedOres is enabled this is the distance around to search"),
    def!("exploreForBlocks", Bool, "true", "When GetToBlock or non-legit Mine doesn't know any locations for the desired block, explore randomly instead of giving up."),
    def!("worldExploringChunkOffset", Int, "0", "While exploring the world, offset the closest unloaded chunk by this much in both axes"),
    def!("exploreChunkSetMinimumSize", Int, "10", "Take the 10 closest chunks, even if they aren't strictly tied for distance metric from origin."),
    def!("exploreMaintainY", Int, "64", "Attempt to maintain Y coordinate while exploring -1 to disable"),
    def!("replantCrops", Bool, "true", "Replant normal Crops while farming and leave cactus and sugarcane to regrow"),
    def!("replantNetherWart", Bool, "false", "Replant nether wart while farming. This setting only has an effect when replantCrops is also enabled"),
    def!("farmMaxScanSize", Int, "256", "Farming will scan for at most this many blocks."),
    def!("farmWaitForGrowth", Bool, "false", "Keep the farm process active when crops are present but none are mature enough to harvest yet. Disabled by default to preserve legacy behavior."),
    def!("extendCacheOnThreshold", Bool, "false", "When the cache scan gives less blocks than the maximum threshold (but still above zero), scan the main world too"),
    def!("buildInLayers", Bool, "false", "Don't consider the next layer in builder until the current one is done"),
    def!("layerOrder", Bool, "false", "false = build from bottom to top true = build from top to bottom"),
    def!("layerHeight", Int, "1", "How high should the individual layers be?"),
    def!("startAtLayer", Int, "0", "Start building the schematic at a specific layer. Can help on larger builds when schematic wants to break things its already built"),
    def!("skipFailedLayers", Bool, "false", "If a layer is unable to be constructed, just skip it."),
    def!("buildOnlySelection", Bool, "false", "Only build the selected part of schematics"),
    def!("buildRepeat", Vec3, "0,0,0", "How far to move before repeating the build. 0 to disable repeating on a certain axis, 0,0,0 to disable entirely"),
    def!("buildRepeatCount", Int, "-1", "How many times to buildrepeat. -1 for infinite."),
    def!("buildRepeatSneaky", Bool, "true", "Don't notify schematics that they are moved. e.g. replacing will replace the same spots for every repetition Mainly for backward compatibility."),
    def!("breakFromAbove", Bool, "false", "Allow standing above a block while mining it, in BuilderProcess Experimental"),
    def!("goalBreakFromAbove", Bool, "false", "As well as breaking from above, set a goal to up and to the side of all blocks to break. Never turn this on without also turning on breakFromAbove."),
    def!("mapArtMode", Bool, "false", "Build in map art mode, which makes baritone only care about the top block in each column"),
    def!("okIfWater", Bool, "false", "Override builder's behavior to not attempt to correct blocks that are currently water"),
    def!("incorrectSize", Int, "100", "The set of incorrect blocks can never grow beyond this size"),
    def!("breakCorrectBlockPenaltyMultiplier", Float, "10", "Multiply the cost of breaking a block that's correct in the builder's schematic by this coefficient"),
    def!("placeIncorrectBlockPenaltyMultiplier", Float, "2", "Multiply the cost of placing a block that's incorrect in the builder's schematic by this coefficient"),
    def!("schematicOrientationX", Bool, "false", "When this setting is true, build a schematic with the highest X coordinate being the origin, instead of the lowest"),
    def!("schematicOrientationY", Bool, "false", "When this setting is true, build a schematic with the highest Y coordinate being the origin, instead of the lowest"),
    def!("schematicOrientationZ", Bool, "false", "When this setting is true, build a schematic with the highest Z coordinate being the origin, instead of the lowest"),
    def!("buildSchematicRotation", Rotation, "NONE", "Rotates the schematic before building it. NONE, CLOCKWISE_90, CLOCKWISE_180 or COUNTERCLOCKWISE_90"),
    def!("buildSchematicMirror", Mirror, "NONE", "Mirrors the schematic before building it. FRONT_BACK mirrors along its local x axis, LEFT_RIGHT along its local z axis"),
    def!("schematicFallbackExtension", Text, "schematic", "The fallback used by the build command when no extension is specified"),
    def!("builderTickScanRadius", Int, "5", "Distance to scan every tick for updates"),
    def!("mineScanDroppedItems", Bool, "true", "While mining, should it also consider dropped items of the correct type as a pathing destination (as well as ore blocks)?"),
    def!("mineDropLoiterDurationMSThanksLouca", Int, "250", "While mining, wait this number of milliseconds after mining an ore to see if it will drop an item instead of immediately going onto the next one Thanks Louca"),
    def!("distanceTrim", Bool, "true", "Trim incorrect positions too far away, helps performance but hurts reliability in very large schematics"),
    def!("cancelOnGoalInvalidation", Bool, "true", "Cancel the current path if the goal has changed, and the path originally ended in the goal but doesn't anymore"),
    def!("axisHeight", Int, "120", "The \"axis\" command (aka GoalAxis) will go to a axis, or diagonal axis, at this Y level."),
    def!("disconnectOnArrival", Bool, "false", "Disconnect from the server upon arriving at your goal"),
    def!("legitMine", Bool, "false", "Disallow MineBehavior from using X-Ray to see where the ores are"),
    def!("legitMineYLevel", Int, "-59", "What Y level to go to for legit strip mining"),
    def!("legitMineIncludeDiagonals", Bool, "false", "Magically see ores that are separated diagonally from existing ores"),
    def!("forceInternalMining", Bool, "true", "When mining block of a certain type, try to mine two at once instead of one"),
    def!("internalMiningAirException", Bool, "true", "Modification to the previous setting, only has effect if forceInternalMining is true If true, only apply the previous setting if the block adjacent to the goal isn't air."),
    def!("followOffsetDistance", Float, "0", "The actual GoalNear is set this distance away from the entity you're following For example, set followOffsetDistance to 5 and followRadius to 0 to always stay precisely 5 blocks north of your follow t"),
    def!("followOffsetDirection", Float, "0", "The actual GoalNear is set in this direction from the entity you're following. This value is in degrees."),
    def!("followRadius", Int, "3", "The radius (for the GoalNear) of how close to your target position you actually have to be"),
    def!("followTargetMaxDistance", Int, "0", "The maximum distance to the entity you're following"),
    def!("disableCompletionCheck", Bool, "false", "Turn this on if your exploration filter is enormous, you don't want it to check if it's done, and you are just fine with it just hanging on completion"),
    def!("cachedChunksExpirySeconds", Int, "-1", "Cached chunks (regardless of if they're in RAM or saved to disk) expire and are deleted after this number of seconds -1 to disable I would highly suggest leaving this setting disabled (-1)"),
    def!("verboseCommandExceptions", Bool, "false", "Print out ALL command exceptions as a stack trace to stdout, even simple syntax errors"),
    def!("yLevelBoxSize", Float, "15", "The size of the box that is rendered when the current goal is a GoalYLevel"),
    def!("colorCurrentPath", Color, "FF0000", "The color of the current path"),
    def!("colorNextPath", Color, "FF00FF", "The color of the next path"),
    def!("colorBlocksToBreak", Color, "FF0000", "The color of the blocks to break"),
    def!("colorBlocksToPlace", Color, "00FF00", "The color of the blocks to place"),
    def!("colorBlocksToWalkInto", Color, "FF00FF", "The color of the blocks to walk into"),
    def!("colorBestPathSoFar", Color, "0000FF", "The color of the best path so far"),
    def!("colorMostRecentConsidered", Color, "00FFFF", "The color of the path to the most recent considered node"),
    def!("colorGoalBox", Color, "00FF00", "The color of the goal box"),
    def!("colorInvertedGoalBox", Color, "FF0000", "The color of the goal box when it's inverted"),
    def!("colorSelection", Color, "00FFFF", "The color of all selections"),
    def!("colorSelectionPos1", Color, "000000", "The color of the selection pos 1"),
    def!("colorSelectionPos2", Color, "FFC800", "The color of the selection pos 2"),
    def!("selectionOpacity", Float, "0.5", "The opacity of the selection. 0 is completely transparent, 1 is completely opaque"),
    def!("selectionLineWidth", Float, "2", "Line width of the goal when rendered, in pixels"),
    def!("renderSelection", Bool, "true", "Render selections"),
    def!("renderSelectionIgnoreDepth", Bool, "true", "Ignore depth when rendering selections"),
    def!("renderSelectionCorners", Bool, "true", "Render selection corners"),
    def!("useSwordToMine", Bool, "true", "Use sword to mine."),
    def!("desktopNotifications", Bool, "false", "Desktop notifications"),
    def!("notificationOnPathComplete", Bool, "true", "Desktop notification on path complete"),
    def!("notificationOnFarmFail", Bool, "true", "Desktop notification on farm fail"),
    def!("notificationOnBuildFinished", Bool, "true", "Desktop notification on build finished"),
    def!("notificationOnExploreFinished", Bool, "true", "Desktop notification on explore finished"),
    def!("notificationOnMineFail", Bool, "true", "Desktop notification on mine fail"),
    def!("elytraSimulationTicks", Int, "20", "The number of ticks of elytra movement to simulate while firework boost is not active. Higher values are computationally more expensive."),
    def!("elytraPitchRange", Int, "25", "The maximum allowed deviation in pitch from a direct line-of-sight to the flight target. Higher values are computationally more expensive."),
    def!("elytraFireworkSpeed", Float, "1.2", "The minimum speed that the player can drop to (in blocks/tick) before a firework is automatically deployed."),
    def!("elytraFireworkSetbackUseDelay", Int, "15", "The delay after the player's position is set-back by the server that a firework may be automatically deployed. Value is in ticks."),
    def!("elytraMinimumAvoidance", Float, "0.2", "The minimum padding value that is added to the player's hitbox when considering which point to fly to on the path"),
    def!("elytraConserveFireworks", Bool, "false", "If enabled, avoids using fireworks when descending along the flight path."),
    def!("elytraRenderRaytraces", Bool, "false", "Renders the raytraces that are performed by the elytra fly calculation."),
    def!("elytraRenderHitboxRaytraces", Bool, "false", "Renders the raytraces that are used in the hitbox part of the elytra fly calculation. Requires #elytraRenderRaytraces."),
    def!("elytraRenderSimulation", Bool, "true", "Renders the best elytra flight path that was simulated each tick."),
    def!("elytraAutoJump", Bool, "false", "Automatically path to and jump off of ledges to initiate elytra flight when grounded."),
    def!("elytraNetherSeed", Int, "146008555100680", "The seed used to generate chunks for long distance elytra path-finding in the nether. Defaults to 2b2t's nether seed."),
    def!("elytraPredictTerrain", Bool, "false", "Whether nether-pathfinder should generate terrain based on #elytraNetherSeed. If false all chunks that haven't been loaded are assumed to be air."),
    def!("elytraAutoSwap", Bool, "true", "Automatically swap the current elytra with a new one when the durability gets too low"),
    def!("elytraMinimumDurability", Int, "5", "The minimum durability an elytra can have before being swapped"),
    def!("elytraMinFireworksBeforeLanding", Int, "5", "The minimum fireworks before landing early for safety"),
    def!("elytraAllowEmergencyLand", Bool, "true", "Automatically land when elytra is almost out of durability, or almost out of fireworks"),
    def!("elytraTimeBetweenCacheCullSecs", Int, "180", "Time between culling far away chunks from the nether pathfinder chunk cache"),
    def!("elytraCacheCullDistance", Int, "5000", "Maximum distance chunks can be before being culled from the nether pathfinder chunk cache"),
    def!("elytraAllowLandOnNetherFortress", Bool, "false", "Should elytra consider nether brick a valid landing block"),
    def!("elytraTermsAccepted", Bool, "false", "Has the user read and understood the elytra terms and conditions"),
    def!("elytraChatSpam", Bool, "false", "Verbose chat logging in elytra mode"),
    def!("elytraAllowTightSpaces", Bool, "false", "Allow the pathfinder to attempt flight in tighter spaces, useful in caves but can be dangerous."),
    def!("elytraAllowAboveRoof", Bool, "false", "Allow the pathfinder to fly above y 128 in the nether."),
    def!("elytraUseCache", Bool, "true", "Allow the pathfinder to access the baritone cache to improve pathing"),
    def!("elytraAllowAboveBuildLimit", Bool, "true", "Allow the pathfinder to fly above the build limit in the overworld and end."),
    def!("elytraLongDistanceThreshold", Int, "500", "Minimum distance in blocks of an elytra trip before the pathfinder will try to fly above build limit"),
    def!("allowWalkOnMagmaBlocks", Bool, "false", "Sneak when magma blocks are under feet"),
    // Not Baritone's: most servers ban automation, so Baritone is off on them unless this is true.
];


/// A value of `kind` from text, in its canonical spelling.
pub fn parse(kind: Kind, v: &str) -> Option<String> {
    let v = v.trim();
    match kind {
        Kind::Bool => match v.to_ascii_lowercase().as_str() {
            "true" | "on" | "yes" => Some("true".into()),
            "false" | "off" | "no" => Some("false".into()),
            _ => None,
        },
        Kind::Int => v.parse::<i64>().ok().map(|n| n.to_string()),
        Kind::Float => v.parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| n.to_string()),
        Kind::Text => (!v.is_empty() && v.len() <= 12 && !v.contains(char::is_whitespace)).then(|| v.to_owned()),
        Kind::Color => {
            let hex = v.trim_start_matches('#');
            (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| hex.to_ascii_uppercase())
        }
        Kind::List => Some(
            v.split(',')
                .map(|s| s.trim().trim_start_matches("minecraft:").to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(","),
        ),
        Kind::Vec3 => {
            let parts: Vec<i32> = v.split(',').map(|p| p.trim().parse::<i32>()).collect::<Result<_, _>>().ok()?;
            (parts.len() == 3).then(|| format!("{},{},{}", parts[0], parts[1], parts[2]))
        }
        Kind::Rotation => {
            let upper = v.to_ascii_uppercase();
            ROTATIONS.contains(&upper.as_str()).then_some(upper)
        }
        Kind::Mirror => {
            let upper = v.to_ascii_uppercase();
            MIRRORS.contains(&upper.as_str()).then_some(upper)
        }
        Kind::BlockMap => {
            let entries = block_map_entries(v)?;
            Some(entries.iter().map(|(k, vs)| format!("{k}->{}", vs.join(","))).collect::<Vec<_>>().join(","))
        }
    }
}

/// `Rotation` as the Java names it, by how many quarter turns clockwise it is (the index).
pub const ROTATIONS: [&str; 4] = ["NONE", "CLOCKWISE_90", "CLOCKWISE_180", "COUNTERCLOCKWISE_90"];
/// `Mirror` as the Java names it.
pub const MIRRORS: [&str; 3] = ["NONE", "LEFT_RIGHT", "FRONT_BACK"];

/// The `Map<Block, List<Block>>` parser of SettingsUtil: an entry is `source->substitute,substitute`, and a comma only
/// starts a new entry where what follows it up to the next comma holds the arrow. None for text that is not that.
pub fn block_map_entries(text: &str) -> Option<Vec<(String, Vec<String>)>> {
    let clean = |s: &str| s.trim().trim_start_matches("minecraft:").to_ascii_lowercase();
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    if text.trim().is_empty() {
        return Some(entries);
    }
    for token in text.split(',') {
        if let Some((key, first)) = token.split_once("->") {
            let (key, first) = (clean(key), clean(first));
            if key.is_empty() || first.is_empty() {
                return None;
            }
            entries.push((key, vec![first]));
        } else {
            let value = clean(token);
            match entries.last_mut() {
                Some((_, values)) if !value.is_empty() => values.push(value),
                _ => return None,
            }
        }
    }
    Some(entries)
}

/// The Java type of each setting, as `SettingsUtil.settingTypeToString` prints it in `#set`.
pub const JAVA_TYPES: &[(&str, &str)] = &[
    ("allowBreak", "Boolean"),
    ("allowBreakAnyway", "List<Block>"),
    ("allowSprint", "Boolean"),
    ("allowPlace", "Boolean"),
    ("allowPlaceInFluidsSource", "Boolean"),
    ("allowPlaceInFluidsFlow", "Boolean"),
    ("allowInventory", "Boolean"),
    ("ticksBetweenInventoryMoves", "Integer"),
    ("inventoryMoveOnlyIfStationary", "Boolean"),
    ("assumeExternalAutoTool", "Boolean"),
    ("autoTool", "Boolean"),
    ("blockPlacementPenalty", "Double"),
    ("blockBreakAdditionalPenalty", "Double"),
    ("jumpPenalty", "Double"),
    ("walkOnWaterOnePenalty", "Double"),
    ("strictLiquidCheck", "Boolean"),
    ("allowWaterBucketFall", "Boolean"),
    ("assumeWalkOnWater", "Boolean"),
    ("assumeWalkOnLava", "Boolean"),
    ("assumeStep", "Boolean"),
    ("assumeSafeWalk", "Boolean"),
    ("allowJumpAtBuildLimit", "Boolean"),
    ("allowParkourAscend", "Boolean"),
    ("allowDiagonalDescend", "Boolean"),
    ("allowDiagonalAscend", "Boolean"),
    ("allowDownward", "Boolean"),
    ("acceptableThrowawayItems", "List<Item>"),
    ("blocksToAvoid", "List<Block>"),
    ("blocksToDisallowBreaking", "List<Block>"),
    ("blocksToAvoidBreaking", "List<Block>"),
    ("avoidBreakingMultiplier", "Double"),
    ("buildIgnoreBlocks", "List<Block>"),
    ("buildSkipBlocks", "List<Block>"),
    ("buildValidSubstitutes", "Map<Block, List<Block>>"),
    ("buildSubstitutes", "Map<Block, List<Block>>"),
    ("okIfAir", "List<Block>"),
    ("buildIgnoreExisting", "Boolean"),
    ("buildIgnoreDirection", "Boolean"),
    ("buildIgnoreProperties", "List<String>"),
    ("avoidUpdatingFallingBlocks", "Boolean"),
    ("allowVines", "Boolean"),
    ("allowWalkOnBottomSlab", "Boolean"),
    ("allowParkour", "Boolean"),
    ("allowParkourPlace", "Boolean"),
    ("considerPotionEffects", "Boolean"),
    ("sprintAscends", "Boolean"),
    ("headHitters", "Boolean"),
    ("headHittersDiagonal", "Boolean"),
    ("sprintJumping", "Boolean"),
    ("sprintJumpingDiagonals", "Boolean"),
    ("overshootTraverse", "Boolean"),
    ("pauseMiningForFallingBlocks", "Boolean"),
    ("rightClickSpeed", "Integer"),
    ("randomLooking113", "Double"),
    ("blockReachDistance", "Float"),
    ("blockBreakSpeed", "Integer"),
    ("randomLooking", "Double"),
    ("costHeuristic", "Double"),
    ("pathingMaxChunkBorderFetch", "Integer"),
    ("backtrackCostFavoringCoefficient", "Double"),
    ("avoidance", "Boolean"),
    ("mobSpawnerAvoidanceCoefficient", "Double"),
    ("mobSpawnerAvoidanceRadius", "Integer"),
    ("mobAvoidanceCoefficient", "Double"),
    ("mobAvoidanceRadius", "Integer"),
    ("rightClickContainerOnArrival", "Boolean"),
    ("enterPortal", "Boolean"),
    ("minimumImprovementRepropagation", "Boolean"),
    ("cutoffAtLoadBoundary", "Boolean"),
    ("maxCostIncrease", "Double"),
    ("costVerificationLookahead", "Integer"),
    ("pathCutoffFactor", "Double"),
    ("pathCutoffMinimumLength", "Integer"),
    ("planningTickLookahead", "Integer"),
    ("pathingMapDefaultSize", "Integer"),
    ("pathingMapLoadFactor", "Float"),
    ("maxFallHeightNoWater", "Integer"),
    ("maxFallHeightBucket", "Integer"),
    ("allowOvershootDiagonalDescend", "Boolean"),
    ("simplifyUnloadedYCoord", "Boolean"),
    ("repackOnAnyBlockChange", "Boolean"),
    ("movementTimeoutTicks", "Integer"),
    ("primaryTimeoutMS", "Long"),
    ("failureTimeoutMS", "Long"),
    ("planAheadPrimaryTimeoutMS", "Long"),
    ("planAheadFailureTimeoutMS", "Long"),
    ("slowPath", "Boolean"),
    ("slowPathTimeDelayMS", "Long"),
    ("slowPathTimeoutMS", "Long"),
    ("doBedWaypoints", "Boolean"),
    ("doDeathWaypoints", "Boolean"),
    ("chunkCaching", "Boolean"),
    ("pruneRegionsFromRAM", "Boolean"),
    ("chunkPackerQueueMaxSize", "Integer"),
    ("backfill", "Boolean"),
    ("logAsToast", "Boolean"),
    ("chatDebug", "Boolean"),
    ("chatControl", "Boolean"),
    ("chatControlAnyway", "Boolean"),
    ("renderPath", "Boolean"),
    ("renderPathAsLine", "Boolean"),
    ("renderGoal", "Boolean"),
    ("renderGoalAnimated", "Boolean"),
    ("renderSelectionBoxes", "Boolean"),
    ("renderGoalIgnoreDepth", "Boolean"),
    ("renderGoalXZBeacon", "Boolean"),
    ("renderSelectionBoxesIgnoreDepth", "Boolean"),
    ("renderPathIgnoreDepth", "Boolean"),
    ("pathRenderLineWidthPixels", "Float"),
    ("goalRenderLineWidthPixels", "Float"),
    ("fadePath", "Boolean"),
    ("freeLook", "Boolean"),
    ("blockFreeLook", "Boolean"),
    ("elytraFreeLook", "Boolean"),
    ("smoothLook", "Boolean"),
    ("elytraSmoothLook", "Boolean"),
    ("smoothLookTicks", "Integer"),
    ("remainWithExistingLookDirection", "Boolean"),
    ("antiCheatCompatibility", "Boolean"),
    ("pathThroughCachedOnly", "Boolean"),
    ("sprintInWater", "Boolean"),
    ("allowSwimming", "Boolean"),
    ("blacklistClosestOnFailure", "Boolean"),
    ("renderCachedChunks", "Boolean"),
    ("cachedChunksOpacity", "Float"),
    ("prefixControl", "Boolean"),
    ("prefix", "String"),
    ("shortBaritonePrefix", "Boolean"),
    ("useMessageTag", "Boolean"),
    ("echoCommands", "Boolean"),
    ("censorCoordinates", "Boolean"),
    ("censorRanCommands", "Boolean"),
    ("itemSaver", "Boolean"),
    ("itemSaverThreshold", "Integer"),
    ("preferSilkTouch", "Boolean"),
    ("walkWhileBreaking", "Boolean"),
    ("splicePath", "Boolean"),
    ("maxPathHistoryLength", "Integer"),
    ("pathHistoryCutoffAmount", "Integer"),
    ("mineGoalUpdateInterval", "Integer"),
    ("maxCachedWorldScanCount", "Integer"),
    ("mineMaxOreLocationsCount", "Integer"),
    ("minYLevelWhileMining", "Integer"),
    ("maxYLevelWhileMining", "Integer"),
    ("allowOnlyExposedOres", "Boolean"),
    ("allowOnlyExposedOresDistance", "Integer"),
    ("exploreForBlocks", "Boolean"),
    ("worldExploringChunkOffset", "Integer"),
    ("exploreChunkSetMinimumSize", "Integer"),
    ("exploreMaintainY", "Integer"),
    ("replantCrops", "Boolean"),
    ("replantNetherWart", "Boolean"),
    ("farmMaxScanSize", "Integer"),
    ("farmWaitForGrowth", "Boolean"),
    ("extendCacheOnThreshold", "Boolean"),
    ("buildInLayers", "Boolean"),
    ("layerOrder", "Boolean"),
    ("layerHeight", "Integer"),
    ("startAtLayer", "Integer"),
    ("skipFailedLayers", "Boolean"),
    ("buildOnlySelection", "Boolean"),
    ("buildRepeat", "Vec3i"),
    ("buildRepeatCount", "Integer"),
    ("buildRepeatSneaky", "Boolean"),
    ("breakFromAbove", "Boolean"),
    ("goalBreakFromAbove", "Boolean"),
    ("mapArtMode", "Boolean"),
    ("okIfWater", "Boolean"),
    ("incorrectSize", "Integer"),
    ("breakCorrectBlockPenaltyMultiplier", "Double"),
    ("placeIncorrectBlockPenaltyMultiplier", "Double"),
    ("schematicOrientationX", "Boolean"),
    ("schematicOrientationY", "Boolean"),
    ("schematicOrientationZ", "Boolean"),
    ("buildSchematicRotation", "Rotation"),
    ("buildSchematicMirror", "Mirror"),
    ("schematicFallbackExtension", "String"),
    ("builderTickScanRadius", "Integer"),
    ("mineScanDroppedItems", "Boolean"),
    ("mineDropLoiterDurationMSThanksLouca", "Long"),
    ("distanceTrim", "Boolean"),
    ("cancelOnGoalInvalidation", "Boolean"),
    ("axisHeight", "Integer"),
    ("disconnectOnArrival", "Boolean"),
    ("legitMine", "Boolean"),
    ("legitMineYLevel", "Integer"),
    ("legitMineIncludeDiagonals", "Boolean"),
    ("forceInternalMining", "Boolean"),
    ("internalMiningAirException", "Boolean"),
    ("followOffsetDistance", "Double"),
    ("followOffsetDirection", "Float"),
    ("followRadius", "Integer"),
    ("followTargetMaxDistance", "Integer"),
    ("disableCompletionCheck", "Boolean"),
    ("cachedChunksExpirySeconds", "Long"),
    ("verboseCommandExceptions", "Boolean"),
    ("yLevelBoxSize", "Double"),
    ("colorCurrentPath", "Color"),
    ("colorNextPath", "Color"),
    ("colorBlocksToBreak", "Color"),
    ("colorBlocksToPlace", "Color"),
    ("colorBlocksToWalkInto", "Color"),
    ("colorBestPathSoFar", "Color"),
    ("colorMostRecentConsidered", "Color"),
    ("colorGoalBox", "Color"),
    ("colorInvertedGoalBox", "Color"),
    ("colorSelection", "Color"),
    ("colorSelectionPos1", "Color"),
    ("colorSelectionPos2", "Color"),
    ("selectionOpacity", "Float"),
    ("selectionLineWidth", "Float"),
    ("renderSelection", "Boolean"),
    ("renderSelectionIgnoreDepth", "Boolean"),
    ("renderSelectionCorners", "Boolean"),
    ("useSwordToMine", "Boolean"),
    ("desktopNotifications", "Boolean"),
    ("notificationOnPathComplete", "Boolean"),
    ("notificationOnFarmFail", "Boolean"),
    ("notificationOnBuildFinished", "Boolean"),
    ("notificationOnExploreFinished", "Boolean"),
    ("notificationOnMineFail", "Boolean"),
    ("elytraSimulationTicks", "Integer"),
    ("elytraPitchRange", "Integer"),
    ("elytraFireworkSpeed", "Double"),
    ("elytraFireworkSetbackUseDelay", "Integer"),
    ("elytraMinimumAvoidance", "Double"),
    ("elytraConserveFireworks", "Boolean"),
    ("elytraRenderRaytraces", "Boolean"),
    ("elytraRenderHitboxRaytraces", "Boolean"),
    ("elytraRenderSimulation", "Boolean"),
    ("elytraAutoJump", "Boolean"),
    ("elytraNetherSeed", "Long"),
    ("elytraPredictTerrain", "Boolean"),
    ("elytraAutoSwap", "Boolean"),
    ("elytraMinimumDurability", "Integer"),
    ("elytraMinFireworksBeforeLanding", "Integer"),
    ("elytraAllowEmergencyLand", "Boolean"),
    ("elytraTimeBetweenCacheCullSecs", "Long"),
    ("elytraCacheCullDistance", "Integer"),
    ("elytraAllowLandOnNetherFortress", "Boolean"),
    ("elytraTermsAccepted", "Boolean"),
    ("elytraChatSpam", "Boolean"),
    ("elytraAllowTightSpaces", "Boolean"),
    ("elytraAllowAboveRoof", "Boolean"),
    ("elytraUseCache", "Boolean"),
    ("elytraAllowAboveBuildLimit", "Boolean"),
    ("elytraLongDistanceThreshold", "Integer"),
    ("allowWalkOnMagmaBlocks", "Boolean"),
];

pub fn java_type(name: &str) -> &'static str {
    JAVA_TYPES.iter().find(|(n, _)| *n == name).map_or("", |(_, t)| t)
}

pub fn find(name: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.name.eq_ignore_ascii_case(name))
}

#[derive(Clone, Default, Debug)]
pub struct Settings {
    values: HashMap<&'static str, String>,
}

impl Settings {
    /// Every setting at its default, then the stored values that still parse.
    pub fn load(stored: impl Fn(&str) -> Option<String>) -> Self {
        let mut values = HashMap::new();
        for d in DEFS {
            let v = stored(d.name).and_then(|v| parse(d.kind, &v)).unwrap_or_else(|| d.default.to_owned());
            values.insert(d.name, v);
        }
        Self { values }
    }

    /// `SettingsUtil.save`: one `name value` line for each setting that is not at its default.
    pub fn to_file_text(&self) -> String {
        DEFS.iter().filter(|d| self.modified(d)).map(|d| format!("{} {}\n", d.name, self.get(d.name))).collect()
    }

    /// `SettingsUtil.readAndApply`, from the text of a settings file: what could not be read comes back as the
    /// lines to tell the player, and everything else still applies.
    pub fn apply_file_text(&mut self, text: &str) -> Vec<String> {
        let mut complaints = Vec::new();
        for line in text.lines() {
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let Some((name, value)) = line.split_once(' ').filter(|(n, v)| !n.is_empty() && !v.trim_start_matches(' ').is_empty()) else {
                complaints.push(format!("Invalid syntax in setting file: {line}"));
                continue;
            };
            let name = if name.eq_ignore_ascii_case("allowjumpat256") { "allowJumpAtBuildLimit" } else { name };
            if self.set(name, value.trim_start_matches(' ')).is_err() {
                complaints.push(format!("Unable to parse line {line}"));
            }
        }
        complaints
    }

    pub fn get(&self, name: &str) -> &str {
        self.values
            .get(name)
            .map(String::as_str)
            .or_else(|| find(name).map(|d| d.default))
            .unwrap_or("")
    }
    pub fn flag(&self, name: &str) -> bool {
        self.get(name) == "true"
    }
    pub fn int(&self, name: &str) -> i32 {
        self.get(name).parse().unwrap_or(0)
    }
    /// A 64 bit setting (a seed): `int` would read anything past 32 bits as 0.
    pub fn long(&self, name: &str) -> i64 {
        self.get(name).parse().unwrap_or(0)
    }
    pub fn float(&self, name: &str) -> f64 {
        self.get(name).parse().unwrap_or(0.0)
    }
    pub fn text(&self, name: &str) -> String {
        self.get(name).to_owned()
    }
    pub fn list(&self, name: &str) -> Vec<String> {
        self.get(name).split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect()
    }
    /// A `x,y,z` setting.
    pub fn vec3(&self, name: &str) -> [i32; 3] {
        let mut out = [0; 3];
        for (slot, part) in out.iter_mut().zip(self.get(name).split(',')) {
            *slot = part.trim().parse().unwrap_or(0);
        }
        out
    }
    /// A rotation setting: quarter turns clockwise, 0 to 3.
    pub fn rotation(&self, name: &str) -> usize {
        ROTATIONS.iter().position(|r| *r == self.get(name)).unwrap_or(0)
    }
    /// A mirror setting: 0 none, 1 left-right, 2 front-back.
    pub fn mirror(&self, name: &str) -> usize {
        MIRRORS.iter().position(|m| *m == self.get(name)).unwrap_or(0)
    }
    /// A block map setting: each source block and the blocks standing for it.
    pub fn block_map(&self, name: &str) -> Vec<(String, Vec<String>)> {
        block_map_entries(self.get(name)).unwrap_or_default()
    }
    /// A color setting as 0xAARRGGBB.
    pub fn color(&self, name: &str) -> u32 {
        0xFF00_0000 | u32::from_str_radix(self.get(name), 16).unwrap_or(0xFFFFFF)
    }
    pub fn modified(&self, d: &Def) -> bool {
        self.get(d.name) != d.default
    }
    /// Sets a setting from text; the canonical value, or why not.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(&'static Def, String), String> {
        let d = find(name).ok_or_else(|| format!("There is no setting called {name}. #set lists them."))?;
        let v = parse(d.kind, value).ok_or_else(|| format!("\"{}\" is not a valid value for {}.", value.trim(), d.name))?;
        self.values.insert(d.name, v.clone());
        Ok((d, v))
    }
    pub fn reset(&mut self, d: &'static Def) {
        self.values.insert(d.name, d.default.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_keeps_only_what_changed_and_reads_back() {
        let mut s = Settings::load(|_| None);
        assert_eq!(s.to_file_text(), "");
        s.set("allowBreak", "false").unwrap();
        s.set("prefix", "$").unwrap();
        let text = s.to_file_text();
        assert!(text.contains("allowBreak false\n") && text.contains("prefix $\n"), "{text}");
        let mut read = Settings::load(|_| None);
        let complaints = read.apply_file_text(&format!("# a comment\n// another\n\n{text}allowsprint   false\nnoSuchSetting 1\nalone\n"));
        assert_eq!(complaints, ["Unable to parse line noSuchSetting 1", "Invalid syntax in setting file: alone"]);
        assert!(!read.flag("allowBreak") && !read.flag("allowSprint"));
        assert_eq!(read.text("prefix"), "$");
    }

    #[test]
    fn the_exotic_settings_parse_as_the_java_does() {
        let mut s = Settings::default();
        s.set("buildRepeat", "0, 0, 12").unwrap();
        assert_eq!(s.vec3("buildRepeat"), [0, 0, 12]);
        assert!(s.set("buildRepeat", "1,2").is_err());
        s.set("buildSchematicRotation", "clockwise_90").unwrap();
        assert_eq!(s.rotation("buildSchematicRotation"), 1);
        s.set("buildSchematicRotation", "counterclockwise_90").unwrap();
        assert_eq!(s.rotation("buildSchematicRotation"), 3);
        assert!(s.set("buildSchematicRotation", "sideways").is_err());
        s.set("buildSchematicMirror", "front_back").unwrap();
        assert_eq!(s.mirror("buildSchematicMirror"), 2);
        assert_eq!(s.rotation("buildSchematicMirror"), 0);
    }

    #[test]
    fn a_block_map_splits_where_the_java_splits_it() {
        let mut s = Settings::default();
        s.set("buildSubstitutes", "stone->cobblestone,andesite,minecraft:oak_planks->birch_planks,acacia_planks,glass").unwrap();
        assert_eq!(
            s.block_map("buildSubstitutes"),
            vec![
                ("stone".to_owned(), vec!["cobblestone".to_owned(), "andesite".to_owned()]),
                ("oak_planks".to_owned(), vec!["birch_planks".to_owned(), "acacia_planks".to_owned(), "glass".to_owned()]),
            ]
        );
        assert_eq!(s.get("buildSubstitutes"), "stone->cobblestone,andesite,oak_planks->birch_planks,acacia_planks,glass");
        assert!(s.set("buildSubstitutes", "stone").is_err());
        assert!(s.set("buildSubstitutes", "->dirt").is_err());
        assert!(s.block_map("buildValidSubstitutes").is_empty());
    }

    #[test]
    fn set_prints_the_java_type_of_a_setting() {
        assert_eq!(java_type("allowBreak"), "Boolean");
        assert_eq!(java_type("buildSubstitutes"), "Map<Block, List<Block>>");
        assert_eq!(java_type("blocksToAvoid"), "List<Block>");
        assert_eq!(java_type("pathCutoffFactor"), "Double");
    }
}
