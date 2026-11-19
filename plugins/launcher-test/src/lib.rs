//! The launcher's test plugin: its code is the game-side module (src/game/plugin.rs) and its tick hook; the mod
//! itself listens to nothing.

#[derive(Default)]
struct LauncherTest;

#[mod_api::horizon_mod]
impl mod_api::Hooks for LauncherTest {}
