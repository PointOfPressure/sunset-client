// Configuration payloads otherwise do not become game events. Observe the identifier only.
fn sunset_server_scan_config_channel(
    payload: &horizon_protocol::packets::configuration::CustomPayload,
) {
    crate::net::sunset_packets::scan_config(payload);
}
