// Velocity: the push of each flowing fluid block on the player, scaled.
fn sunset_flow(value: Result<Vec3>) -> Result<Vec3> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    value.map(::sunset_core::motion::liquid_flow)
}
