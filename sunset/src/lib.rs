//! Sunset's own crate (`sunset_core`): the switches and state that Sunset's game-side code (its support modules in the
//! game crate) and its hooks in the crates below the game share, and the bridge from those hooks to the game side.
//! It depends on the mod API alone. The client itself is in `src/game/` (support modules compiled into the game's
//! function-replacement mirror) and `src/mixins/` (the hooks); neither is part of this crate.
#![deny(unsafe_code)]

pub mod atmosphere;
pub mod block_overlay;
pub mod bridge;
pub mod chat_log;
pub mod client_brand;
pub mod fog;
pub mod font;
pub mod font_data;
pub mod hand_view;
pub mod input;
pub mod lifecycle;
pub mod motion;
pub mod overlay;
pub mod particles;
pub mod sync;
pub mod timer;
pub mod water;
pub mod wire;
pub mod xray;

#[derive(Default)]
struct Sunset;

#[mod_api::horizon_mod]
impl mod_api::Hooks for Sunset {}
