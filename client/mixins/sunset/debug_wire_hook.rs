// Debug Report's packet log: each packet the client sends, as it goes on the wire.
fn sunset_debug_wire(body: &[u8]) {
    if !crate::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::connection::sunset_wire::sent(body);
}
