// Flight: the air's pull on your input, in Velocity mode.
fn sunset_flying(value: Result<f32>) -> Result<f32> {
    value.map(crate::motion::sunset_motion::flying_speed)
}
