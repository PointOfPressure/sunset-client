// Run after native jump/input processing, before moving: preserve a ballistic velocity as well as displacement.
// Merely overriding move_self's Y produces a one-tick position lift followed by the old vertical velocity.
fn sunset_legacy_jump(world: &mut MotionWorld<'_, '_, '_>) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }
    let across = ::sunset_core::motion::take_legacy_velocity();
    if let Some(scale) = ::sunset_core::motion::take_ballistic_jump() {
        let power = if scale.is_finite() && scale > 0.0 {
            horizon_world::living_motion::jump_power(world, scale as f32).ok()
        } else { None };
        let Some(power) = power.filter(|p| p.is_finite() && *p > 0.0) else {
            // A zero/invalid jump attribute or block factor must not become a horizontal-only launch.
            ::sunset_core::motion::set_across(None);
            return;
        };
        world.player.velocity.y = f64::from(power);
        world.player.fall_distance = 0.0;
        world.player.needs_sync = true;
        ::sunset_core::motion::set_up(Some(f64::from(power)));
    }
    if let Some([x, z]) = across && x.is_finite() && z.is_finite() {
        world.player.velocity.x = x;
        world.player.velocity.z = z;
    }
}
