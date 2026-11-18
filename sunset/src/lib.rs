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
impl mod_api::Hooks for Sunset {
    // Safe Walk: the player holds to edges as if sneaking. The client's own prediction only: the switches are the
    // client thread's, so the integrated server's tick of the player is unchanged.
    fn player_stays_on_ground_surface(
        &self,
        context: &dyn mod_api::LoopContext,
        event: &mut mod_api::PlayerStaysOnGroundSurfaceEvent,
    ) {
        if lifecycle::RUNTIME.running() && matches!(context.side(), mod_api::LoopSide::Client) {
            event.staying = motion::clip_at_ledge(event.staying);
        }
    }

    // Flight: the air's pull on the player's input, in Velocity mode.
    fn living_flying_speed(&self, context: &dyn mod_api::LoopContext, event: &mut mod_api::LivingFlyingSpeedEvent<'_>) {
        if lifecycle::RUNTIME.running()
            && matches!(context.side(), mod_api::LoopSide::Client)
            && event.kind == "minecraft:player"
        {
            event.speed = motion::flying_speed(event.speed);
        }
    }

    // Velocity: the push of each flowing fluid block, scaled. On the client only the player is pushed by its own
    // tick (other entities move as the server says).
    fn fluid_flow(&self, context: &dyn mod_api::LoopContext, event: &mut mod_api::FluidFlowEvent<'_>) {
        if lifecycle::RUNTIME.running() && matches!(context.side(), mod_api::LoopSide::Client) {
            *event.flow = motion::liquid_flow(*event.flow);
        }
    }
}
