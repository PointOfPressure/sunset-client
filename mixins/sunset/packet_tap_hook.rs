// Copies of the packets Crystal Aura and Anchor Aura learn from, as the game takes them in (after Anti trap dropped its
// spawns, Backtrack held back what came while its player moved away, and Knockback Delay the pushes it delays), and
// Client Spoofer's watch for the Login packets its join packets follow. The session comes too: what Backtrack and
// Knockback Delay hold belongs to the session it came from, and the connection's boundaries are counted for Fake Lag.
fn sunset_packet_tap(session: &Session, value: Vec<Event>) -> Vec<Event> {
    crate::net::sunset_packets::note(session, &value);
    let value = crate::net::sunset_packets::trap_filter(value);
    let value = crate::net::sunset_packets::backtrack_hold(session, value);
    let value = crate::net::sunset_packets::hold(session, value);
    crate::net::sunset_packets::seen(&value);
    crate::net::sunset_client_brand::seen(&value);
    crate::net::sunset_packets::gui_move_hold(value)
}
