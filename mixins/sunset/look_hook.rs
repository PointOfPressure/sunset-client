// Kill Aura's Silent aura: the look the game's position sender is given, so its own move packet carries it (and only
// when it changed, as the game sends any look). The sender hook records the rotation.
fn sunset_sent_look(value: (f32, f32)) -> (f32, f32) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return value; }

    crate::game::sunset_rotation::sent(value)
}
