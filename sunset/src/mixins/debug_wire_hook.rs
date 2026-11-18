// Debug Report's packet log: each play packet the game sends, as it goes to the connection (what the network thread
// sends by itself, keep-alives and the login and configuration phases, isn't seen here).
fn sunset_debug_wire(packet: &S) {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return;
    }
    if let Ok(body) = packet.try_encode() {
        ::sunset_core::wire::sent(&body);
    }
}
