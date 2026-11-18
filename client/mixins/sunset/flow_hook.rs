// Velocity: the push of each flowing fluid block on the player, scaled.
fn sunset_flow(value: Result<Vec3>) -> Result<Vec3> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    value.map(crate::motion::sunset_motion::liquid_flow)
}
