// Speed: the player's own move across the ground, when Speed sets it.
fn sunset_self_move(value: Vec3) -> Vec3 {
    if !::sunset_core::lifecycle::RUNTIME.running() { return value; }

    let [x, y, z] = ::sunset_core::motion::self_move([value.x, value.y, value.z]);
    Vec3::new(x, y, z)
}
