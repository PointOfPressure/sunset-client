// Each packet as it goes on the wire.
fn probe_wire(body: &[u8]) {
    crate::connection::probe_wire::sent(body);
}
