// Copies of the packets Crystal Aura and Anchor Aura learn from, as the game takes them in (after Knockback Delay held
// back the pushes it delays).
fn sunset_packet_tap(value: Vec<Event>) -> Vec<Event> {
    let value = crate::net::sunset_packets::hold(value);
    crate::net::sunset_packets::seen(&value);
    value
}
