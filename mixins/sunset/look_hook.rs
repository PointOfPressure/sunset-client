// Kill Aura's Silent aura: the look the game's position sender is given, so its own move packet carries it (and only
// when it changed, as the game sends any look). From vape-rs's send-state-rotation hook (MIT).
fn sunset_sent_look(value: (f32, f32)) -> (f32, f32) {
    crate::game::sunset_rotation::sent(value)
}
