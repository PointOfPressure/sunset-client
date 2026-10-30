// Safe Walk: the player keeps to the ground at edges as if sneaking.
fn sunset_ledge(value: Result<bool>) -> Result<bool> {
    value.map(crate::motion::sunset_motion::clip_at_ledge)
}
