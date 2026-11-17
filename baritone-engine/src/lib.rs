// The replaceable Baritone engine (LGPL-3.0, see NOTICE.md and CHANGES.md): the engine's own ABI, protocol, host and
// predicted-ore modules here, plus the game-free parts of horizon-baritone. Those have one copy, in
// mixins/baritone/upstream/src, which the game also builds as modules of game.rs (see mixins/baritone/horizon-mixin.toml);
// here they are modules of this crate with the same names, so `use super::baritone_goals::...` reads the same in both.
#![deny(unsafe_code)]
pub mod abi;
pub mod protocol;
pub mod host;
pub mod predicted;
pub mod engine;
// The only engine-side unsafe code is the audited C transport boundary.
#[allow(unsafe_code)]
#[path = "../../mixins/baritone/loader/src/engine_ffi.rs"]
mod ffi;
#[path = "../../mixins/baritone/upstream/src/api/utils.rs"]
pub mod baritone_utils;
#[path = "../../mixins/baritone/upstream/src/pathing/costs.rs"]
pub mod baritone_costs;
#[path = "../../mixins/baritone/upstream/src/api/goals.rs"]
pub mod baritone_goals;
#[path = "../../mixins/baritone/upstream/src/api/block_optional_meta.rs"]
pub mod baritone_bom;
#[path = "../../mixins/baritone/upstream/src/cache/world_scanner.rs"]
pub mod baritone_world_scanner;
#[path = "../../mixins/baritone/upstream/src/process/builder_logic.rs"]
pub mod baritone_builder_logic;
#[path = "../../mixins/baritone/upstream/src/schematic/mask.rs"]
pub mod baritone_schematic_mask;
#[path = "../../mixins/baritone/upstream/src/schematic/wrappers.rs"]
pub mod baritone_schematic_wrappers;
#[path = "../../mixins/baritone/upstream/src/schematic/legacy.rs"]
pub mod baritone_schematic_legacy;
#[path = "../../mixins/baritone/upstream/src/utils/notification.rs"]
pub mod baritone_notification;
#[path = "../../mixins/baritone/upstream/src/utils/gui_click.rs"]
pub mod baritone_gui_click;
#[path = "../../mixins/baritone/upstream/src/ui/settings_screen.rs"]
pub mod baritone_screen;
#[path = "../../mixins/baritone/upstream/src/process/elytra/chunk.rs"]
pub mod baritone_elytra_chunk;
#[path = "../../mixins/baritone/upstream/src/process/elytra/noise.rs"]
pub mod baritone_elytra_noise;
#[path = "../../mixins/baritone/upstream/src/process/elytra/chunk_generator_hell.rs"]
pub mod baritone_elytra_generator;
#[path = "../../mixins/baritone/upstream/src/process/elytra/path_node.rs"]
pub mod baritone_elytra_path_node;
#[path = "../../mixins/baritone/upstream/src/process/elytra/raytracer.rs"]
pub mod baritone_elytra_raytracer;
#[path = "../../mixins/baritone/upstream/src/process/elytra/nether_pathfinder.rs"]
pub mod baritone_elytra_pathfinder;
#[path = "../../mixins/baritone/upstream/src/process/elytra/region.rs"]
pub mod baritone_elytra_region;
#[path = "../../mixins/baritone/upstream/src/process/elytra/path_finder.rs"]
pub mod baritone_elytra_search;
#[path = "../../mixins/baritone/upstream/src/process/elytra/segment.rs"]
pub mod baritone_elytra_segment;
#[path = "../../mixins/baritone/upstream/src/process/elytra/flight.rs"]
pub mod baritone_elytra_flight;
#[path = "../../mixins/baritone/upstream/src/process/elytra/context.rs"]
pub mod baritone_elytra_context;
#[path = "../../mixins/baritone/upstream/src/process/elytra/build_limit.rs"]
pub mod baritone_elytra_build_limit;
#[path = "../../mixins/baritone/upstream/src/process/elytra/path_manager.rs"]
pub mod baritone_elytra_path_manager;
#[path = "../../mixins/baritone/upstream/src/cache/cached_world.rs"]
pub mod baritone_cached_world;
#[path = "../../mixins/baritone/upstream/src/api/settings.rs"]
pub mod baritone_settings;
#[path = "../../mixins/baritone/upstream/src/selection/selection.rs"]
pub mod baritone_selection;
#[path = "../../mixins/baritone/upstream/src/schematic/schematic.rs"]
pub mod baritone_schematic;
#[path = "../../mixins/baritone/upstream/src/cache/waypoints.rs"]
pub mod baritone_waypoints;
#[path = "../../mixins/baritone/upstream/src/command/argument.rs"]
pub mod baritone_argument;
#[path = "../../mixins/baritone/upstream/src/utils/block_state.rs"]
pub mod baritone_block_state;
#[path = "../../mixins/baritone/upstream/src/utils/block_state_interface.rs"]
pub mod baritone_block_state_interface;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/calculation_context.rs"]
pub mod baritone_calculation_context;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movement_helper.rs"]
pub mod baritone_movement_helper;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movements/traverse.rs"]
pub mod baritone_movement_traverse;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movements/ascend.rs"]
pub mod baritone_movement_ascend;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movements/descend.rs"]
pub mod baritone_movement_descend;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movements/diagonal.rs"]
pub mod baritone_movement_diagonal;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movements/downward.rs"]
pub mod baritone_movement_downward;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movements/parkour.rs"]
pub mod baritone_movement_parkour;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movements/pillar.rs"]
pub mod baritone_movement_pillar;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/moves.rs"]
pub mod baritone_moves;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movement.rs"]
pub mod baritone_movement;
#[path = "../../mixins/baritone/upstream/src/pathing/calc/a_star.rs"]
pub mod baritone_astar;
#[path = "../../mixins/baritone/upstream/src/pathing/calc/path.rs"]
pub mod baritone_calc_path;
#[path = "../../mixins/baritone/upstream/src/utils/rotation.rs"]
pub mod baritone_rotation;
#[path = "../../mixins/baritone/upstream/src/behavior/aim.rs"]
pub mod baritone_aim;
#[path = "../../mixins/baritone/upstream/src/behavior/look.rs"]
pub mod baritone_look;
#[path = "../../mixins/baritone/upstream/src/behavior/movement_state.rs"]
pub mod baritone_movement_state;
#[path = "../../mixins/baritone/upstream/src/behavior/player_context.rs"]
pub mod baritone_player_context;
#[path = "../../mixins/baritone/upstream/src/pathing/movement/movement_exec.rs"]
pub mod baritone_movement_exec;
#[path = "../../mixins/baritone/upstream/src/pathing/avoidance.rs"]
pub mod baritone_avoidance;
#[path = "../../mixins/baritone/upstream/src/pathing/path/sprint_jump.rs"]
pub mod baritone_sprint_jump;
#[path = "../../mixins/baritone/upstream/src/pathing/path/executor.rs"]
pub mod baritone_executor;
#[path = "../../mixins/baritone/upstream/src/behavior/pathing_behavior.rs"]
pub mod baritone_pathing_behavior;

#[cfg(test)]
#[path = "../../mixins/baritone/upstream/src/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "../../mixins/baritone/upstream/src/tests_pathing.rs"]
mod tests_pathing;
#[cfg(test)]
#[path = "../../mixins/baritone/upstream/src/tests_elytra.rs"]
mod tests_elytra;
