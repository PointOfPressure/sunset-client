// Flight: the air's pull on your input, in Velocity mode.
fn sunset_flying(value: Result<f32>) -> Result<f32> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    value.map(::sunset_core::motion::flying_speed)
}
