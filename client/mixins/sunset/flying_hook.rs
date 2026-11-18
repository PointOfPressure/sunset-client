// Flight: the air's pull on your input, in Velocity mode.
fn sunset_flying(value: Result<f32>) -> Result<f32> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    value.map(crate::motion::sunset_motion::flying_speed)
}
