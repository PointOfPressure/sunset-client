// What Sunset asks of the player's own movement, set once a tick by the game and read by the hooks on the
// movement code (the window's thread only, never a server's): Safe Walk keeps to edges as sneaking does (Meteor's
// ClipAtLedgeEvent), Speed sets the move across the ground (Meteor's PlayerMoveEvent), Velocity scales how
// flowing liquids push (the flow each fluid block gives, across and up), Flight sets the air's pull, and GUI Move
// keeps a sprint going behind a screen.

use std::cell::Cell;

thread_local! {
    static SAFE_WALK: Cell<bool> = const { Cell::new(false) };
    // Blocks a tick across the ground, x and z.
    static ACROSS: Cell<Option<[f64; 2]>> = const { Cell::new(None) };
    static UP: Cell<Option<f64>> = const { Cell::new(None) };
    static LIQUID: Cell<Option<[f64; 2]>> = const { Cell::new(None) };
    // Flight's Velocity mode: the air's pull on your input (Player.getFlyingSpeed).
    static FLYING: Cell<Option<f32>> = const { Cell::new(None) };
    // GUI Move: the move keys held behind a screen, which the game would not take.
    static SCREEN_KEYS: Cell<Option<crate::sprint::Keys>> = const { Cell::new(None) };
}

pub fn set_screen_keys(keys: Option<crate::sprint::Keys>) {
    SCREEN_KEYS.with(|c| c.set(keys));
}

// The keys a tick's movement takes behind a screen while GUI Move walks you: the ones held, when the game gave it none
// (a screen keeps them from it) and some are held. None: the keys as they are.
pub fn screen_keys(keys: crate::sprint::Keys) -> Option<crate::sprint::Keys> {
    let pressed = |k: crate::sprint::Keys| k.direction() != [0.0, 0.0] || k.jump || k.shift;
    SCREEN_KEYS.with(Cell::get).filter(|held| !pressed(keys) && pressed(*held))
}

pub fn set_flying_speed(speed: Option<f32>) {
    FLYING.with(|c| c.set(speed));
}

pub fn flying_speed(speed: f32) -> f32 {
    FLYING.with(Cell::get).unwrap_or(speed)
}

pub fn set_safe_walk(on: bool) {
    SAFE_WALK.with(|c| c.set(on));
}

pub fn set_across(across: Option<[f64; 2]>) {
    ACROSS.with(|c| c.set(across));
}

pub fn set_up(up: Option<f64>) {
    UP.with(|c| c.set(up));
}

pub fn set_liquid_flow(share: Option<[f64; 2]>) {
    LIQUID.with(|c| c.set(share));
}

pub fn liquid_flow(flow: horizon_core::geometry::Vec3) -> horizon_core::geometry::Vec3 {
    match LIQUID.with(Cell::get) {
        Some([h, v]) => horizon_core::geometry::Vec3::new(flow.x * h, flow.y * v, flow.z * h),
        None => flow,
    }
}

pub fn clip_at_ledge(sneaking: bool) -> bool {
    sneaking || SAFE_WALK.with(Cell::get)
}

pub fn self_move(movement: horizon_core::geometry::Vec3) -> horizon_core::geometry::Vec3 {
    let y = UP.with(Cell::get).unwrap_or(movement.y);
    match ACROSS.with(Cell::get) {
        Some([x, z]) => horizon_core::geometry::Vec3::new(x, y, z),
        None => horizon_core::geometry::Vec3::new(movement.x, y, movement.z),
    }
}
