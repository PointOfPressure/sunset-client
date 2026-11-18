// Safe Walk: the player keeps to the ground at edges as if sneaking.
fn sunset_ledge(value: Result<bool>) -> Result<bool> {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    value.map(crate::motion::sunset_motion::clip_at_ledge)
}
