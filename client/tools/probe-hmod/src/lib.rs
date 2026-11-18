//! verify.py's test probe: its code is all in the game-side modules (src/game/) and their hooks; the mod itself
//! listens to nothing.

#[derive(Default)]
struct Probe;

#[mod_api::horizon_mod]
impl mod_api::Hooks for Probe {}
