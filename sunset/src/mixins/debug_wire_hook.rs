// Debug Report's packet log: each packet the client sends, as it goes on the wire.
fn sunset_debug_wire(body: &[u8]) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    ::sunset_core::wire::sent(body);
}
