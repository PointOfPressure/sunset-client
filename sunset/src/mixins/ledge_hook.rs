// Safe Walk: the player keeps to the ground at edges as if sneaking.
fn sunset_ledge(value: Result<bool>) -> Result<bool> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    value.map(::sunset_core::motion::clip_at_ledge)
}
