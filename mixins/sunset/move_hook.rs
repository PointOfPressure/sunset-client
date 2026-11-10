// Speed: the player's own move across the ground, when Speed sets it.
fn sunset_self_move(value: Vec3) -> Vec3 {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    crate::motion::sunset_motion::self_move(value)
}
