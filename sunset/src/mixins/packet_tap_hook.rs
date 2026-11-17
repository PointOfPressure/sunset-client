// The packets the game takes in, on Sunset's game side (crate::game::sunset_packets::poll): copies for Crystal Aura,
// Anchor Aura and the plugin API, what Anti trap drops, Backtrack and Knockback Delay hold, GUI Move's held inventory
// updates and Client Spoofer's watch for the Login packets its join packets follow. The session comes too: what is
// held belongs to the session it came from.
fn sunset_packet_tap(session: &Session, value: Vec<Event>) -> Vec<Event> {
    let mut value = value;
    if let Some(poll) = ::sunset_core::bridge::SESSION_POLL.get() {
        poll(session as *const Session as *mut u8, &mut value as *mut Vec<Event> as *mut u8);
    }
    value
}
