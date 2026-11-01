// Copies of the packets Crystal Aura and Anchor Aura learn from, as the game takes them in.
fn sunset_packet_tap(value: Vec<Event>) -> Vec<Event> {
    crate::net::sunset_packets::seen(&value);
    value
}
