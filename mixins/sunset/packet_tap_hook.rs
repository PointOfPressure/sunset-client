// Copies of the packets Crystal Aura and Anchor Aura learn from, as the game takes them in (after Anti trap dropped its
// spawns, Backtrack held back what came while its player moved away, and Knockback Delay the pushes it delays).
fn sunset_packet_tap(value: Vec<Event>) -> Vec<Event> {
    let value = crate::net::sunset_packets::trap_filter(value);
    let value = crate::net::sunset_packets::backtrack_hold(value);
    let value = crate::net::sunset_packets::hold(value);
    crate::net::sunset_packets::seen(&value);
    value
}
