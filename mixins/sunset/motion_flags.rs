// What Sunset asks of the player's own movement, set once a tick by the game and read by the hooks on the
// movement code (the window's thread only, never a server's): Safe Walk keeps to edges as sneaking does (Sunset's
// ClipAtLedgeEvent), Speed sets the move across the ground (Sunset's PlayerMoveEvent), Velocity scales how
// flowing liquids push (the flow each fluid block gives, across and up), Flight sets the air's pull, GUI Move
// keeps a sprint going behind a screen, and Kill Aura's Target strafe turns the move across the ground.

use std::cell::Cell;

pub fn reset() {
    set_freeze(false);
    set_input_scale(None);
    set_move_yaw(None);
    set_no_sprint(false);
    set_no_slow_grim(false);
    set_no_slow_skip(false);
    set_screen_keys(None);
    set_flying_speed(None);
    set_safe_walk(false);
    set_across(None);
    set_up(None);
    set_strafe_dir(None);
    set_liquid_flow(None);
    ATTACK_SLOW.with(|c| c.set((0, 0)));
}

thread_local! {
    // GUI Move's pending inventory transaction: one movement consumes the override,
    // while the matching PlayerInput remains empty through the end of this tick.
    static FREEZE_MOVE: Cell<bool> = const { Cell::new(false) };
    static FREEZE_INPUT: Cell<bool> = const { Cell::new(false) };
    static INPUT_SCALE: Cell<Option<f32>> = const { Cell::new(None) };
    static SAFE_WALK: Cell<bool> = const { Cell::new(false) };
    // Blocks a tick across the ground, x and z.
    static ACROSS: Cell<Option<[f64; 2]>> = const { Cell::new(None) };
    static UP: Cell<Option<f64>> = const { Cell::new(None) };
    // Target strafe: the way (x, z, length 1) the move across the ground goes this tick, at its own speed.
    static STRAFE_DIR: Cell<Option<[f64; 2]>> = const { Cell::new(None) };
    static LIQUID: Cell<Option<[f64; 2]>> = const { Cell::new(None) };
    // Flight's Velocity mode: the air's pull on your input (Player.getFlyingSpeed).
    static FLYING: Cell<Option<f32>> = const { Cell::new(None) };
    // GUI Move: the move keys held behind a screen, which the game would not take.
    static SCREEN_KEYS: Cell<Option<crate::sprint::Keys>> = const { Cell::new(None) };
    // GUI Move: the keys this tick's movement took behind a screen, for the tick's PlayerInput.
    static SCREEN_MOVED: Cell<Option<crate::sprint::Keys>> = const { Cell::new(None) };
    // No Slow's Skip: the item-use slowdown left out of this tick's move.
    static NO_SLOW_SKIP: Cell<bool> = const { Cell::new(false) };
    // No Slow's GrimAC mode keeps Speed's movement override out of item-use ticks.
    static NO_SLOW_GRIM: Cell<bool> = const { Cell::new(false) };
    // Knockback hits since the last movement, and the game tick of the last: each cuts the speed across to 0.6 and
    // stops the sprint.
    static ATTACK_SLOW: Cell<(u32, u64)> = const { Cell::new((0, 0)) };
    // Scaffold's movement correction: the yaw the next movement goes by (the look the server is told), so the move
    // matches the keys it is told at that yaw. Taken by that movement.
    static MOVE_YAW: Cell<Option<f32>> = const { Cell::new(None) };
    // Scaffold's Sprint Off: the next movement goes without a sprint. Taken by that movement.
    static NO_SPRINT: Cell<bool> = const { Cell::new(false) };
}

pub fn set_freeze(on: bool) {
    FREEZE_MOVE.with(|c| c.set(on));
    FREEZE_INPUT.with(|c| c.set(on));
}

pub fn take_freeze() -> bool { FREEZE_MOVE.with(Cell::take) }
pub fn frozen() -> bool { FREEZE_INPUT.with(Cell::get) }

pub fn set_input_scale(scale: Option<f32>) {
    INPUT_SCALE.with(|c| c.set(scale.filter(|s| s.is_finite()).map(|s| s.clamp(0.01, 1.0))));
}

pub fn take_input_scale() -> Option<f32> { INPUT_SCALE.with(Cell::take) }

pub fn set_move_yaw(yaw: Option<f32>) {
    MOVE_YAW.with(|c| c.set(yaw));
}

pub fn take_move_yaw() -> Option<f32> {
    MOVE_YAW.with(Cell::take)
}

pub fn set_no_sprint(on: bool) {
    NO_SPRINT.with(|c| c.set(on));
}

pub fn take_no_sprint() -> bool {
    NO_SPRINT.with(Cell::take)
}

pub fn set_no_slow_grim(on: bool) {
    NO_SLOW_GRIM.with(|c| c.set(on));
}

pub fn no_slow_grim() -> bool {
    NO_SLOW_GRIM.with(Cell::get)
}

pub fn set_no_slow_skip(on: bool) {
    NO_SLOW_SKIP.with(|c| c.set(on));
}

pub fn no_slow_skip() -> bool {
    NO_SLOW_SKIP.with(Cell::get)
}

pub fn add_attack_slow(tick: u64) {
    ATTACK_SLOW.with(|c| c.set((c.get().0.saturating_add(1), tick)));
}

// The start of a tick: hits from before the last tick's movement that no movement took (riding, dead) are gone.
pub fn expire_attack_slow(tick: u64) {
    ATTACK_SLOW.with(|c| {
        if c.get().1 < tick {
            c.set((0, 0));
        }
    });
}

// The knockback hits the next movement starts with (then none).
pub fn take_attack_slow() -> u32 {
    ATTACK_SLOW.with(|c| c.replace((0, 0)).0)
}

pub fn set_screen_keys(keys: Option<crate::sprint::Keys>) {
    SCREEN_KEYS.with(|c| c.set(keys));
    SCREEN_MOVED.with(|c| c.set(None));
}

// The keys a tick's movement takes behind a screen while GUI Move walks you: the ones held, when the game gave it none
// (a screen keeps them from it) and some are held. None: the keys as they are.
pub fn screen_keys(keys: crate::sprint::Keys) -> Option<crate::sprint::Keys> {
    if frozen() { return None; }
    let pressed = |k: crate::sprint::Keys| k.direction() != [0.0, 0.0] || k.jump || k.shift;
    let moved = SCREEN_KEYS.with(Cell::get).filter(|held| !pressed(keys) && pressed(*held));
    // Kept till the next tick: the move's call again with these keys passes through here too.
    if moved.is_some() {
        SCREEN_MOVED.with(|c| c.set(moved));
    }
    moved
}

// The PlayerInput flags a tick sends with the keys GUI Move moved with behind a screen added (the game's have none
// there, and the server predicts the move from them); None when that adds nothing.
pub fn screen_input(input: u8) -> Option<u8> {
    if frozen() { return (input != 0).then_some(0); }
    let k = SCREEN_MOVED.with(Cell::get)?;
    let with = input
        | u8::from(k.forward)
        | u8::from(k.backward) << 1
        | u8::from(k.left) << 2
        | u8::from(k.right) << 3
        | u8::from(k.jump) << 4
        | u8::from(k.shift) << 5
        | u8::from(k.sprint) << 6;
    (with != input).then_some(with)
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

pub fn set_strafe_dir(dir: Option<[f64; 2]>) {
    STRAFE_DIR.with(|c| c.set(dir));
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
    if frozen() { return movement; }
    let y = UP.with(Cell::get).unwrap_or(movement.y);
    let [x, z] = ACROSS.with(Cell::get).unwrap_or([movement.x, movement.z]);
    match STRAFE_DIR.with(Cell::get) {
        Some([dx, dz]) => {
            let speed = x.hypot(z);
            horizon_core::geometry::Vec3::new(dx * speed, y, dz * speed)
        }
        None => horizon_core::geometry::Vec3::new(x, y, z),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_stop_clears_every_input_bit_and_expires_next_tick() {
        set_screen_keys(Some(crate::sprint::Keys { forward: true, sprint: true, shift: true, ..Default::default() }));
        let _ = screen_keys(Default::default());
        set_freeze(true);
        assert!(take_freeze());
        assert!(!take_freeze(), "recursive movement must not freeze twice");
        assert_eq!(screen_input(127), Some(0));
        assert_eq!(screen_input(0), None);
        set_freeze(false);
        assert_eq!(screen_input(0), Some(97));
        set_screen_keys(None);
    }

    #[test]
    fn movement_scaling_is_bounded_and_consumed_once() {
        set_input_scale(Some(0.6));
        assert_eq!(take_input_scale(), Some(0.6));
        assert_eq!(take_input_scale(), None);
        set_input_scale(Some(f32::NAN));
        assert_eq!(take_input_scale(), None);
    }
}
