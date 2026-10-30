// Sunset's plugin API, version 1.
//
// A plugin is another native mod bundle built together with Sunset. It adds
// a [[module]] of its own to crates/horizon-client/src/game.rs and hooks
// Horizon's own native hook points (native_tick, native_hud, native_key); from
// there it uses this module to add modules to the click GUI and to read what
// the player chose. See docs/plugin-api.md in the repository, and
// examples/hello-plugin for a whole one.

#[allow(dead_code)]
pub(super) const API_VERSION: u32 = 1;

#[allow(unused_imports)]
pub(super) use super::sunset::{Camera, Category, Module, Shape, hold, look_at, outline, line};
#[allow(unused_imports)]
pub(super) use super::sunset_util::{
    PRIORITIES, Target, can_place, center_of, clear_line, dist, every_living, facts, find_hotbar, find_inventory, in_reach,
    is_air, listed, other_players, place_block, player_target, replaceable, sequence, short, show, toggle_off,
};

// Adds a module to the click GUI, the .modules list and the config file. Call
// it every tick if you like: a name that is already there is left alone.
#[allow(dead_code)]
pub(super) fn register(module: Module) {
    super::sunset::register_module(module);
}

// The module named `name` when the player has it on: its current settings.
#[allow(dead_code)]
pub(super) fn module(name: &str) -> Option<Module> {
    super::sunset::with(|s| s.module(name).filter(|m| m.on).cloned())
}

// A line in chat from Sunset.
#[allow(dead_code)]
pub(super) fn chat(game: &mut super::Game<'_>, text: &str) {
    super::sunset::say(game, text);
}

// Turns a module off (its key bind and the GUI keep working).
#[allow(dead_code)]
pub(super) fn turn_off(game: &mut super::Game<'_>, name: &str) {
    toggle_off(game, name);
}
