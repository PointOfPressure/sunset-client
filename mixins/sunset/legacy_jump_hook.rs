// Run after native jump/input processing, before moving: preserve a ballistic velocity as well as displacement.
// Merely overriding move_self's Y produces a one-tick position lift followed by the old vertical velocity.
fn sunset_legacy_jump(world: &mut MotionWorld<'_, '_, '_>) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }
    if let Some([x, z]) = crate::motion::sunset_motion::take_legacy_velocity()
        && x.is_finite() && z.is_finite()
    {
        world.player.velocity.x = x;
        world.player.velocity.z = z;
    }
    let Some(scale) = crate::motion::sunset_motion::take_ballistic_jump() else { return };
    if !scale.is_finite() || scale <= 0.0 { return; }
    let Ok(power) = horizon_world::living_motion::jump_power(world, scale as f32) else { return };
    if !power.is_finite() || power <= 0.0 { return; }
    world.player.velocity.y = f64::from(power);
    world.player.fall_distance = 0.0;
    world.player.needs_sync = true;
    crate::motion::sunset_motion::set_up(Some(f64::from(power)));
}
