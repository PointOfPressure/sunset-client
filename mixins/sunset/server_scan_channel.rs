// Configuration payloads otherwise do not become game events. Observe the identifier and
// minecraft:register's channel list only.
fn sunset_server_scan_config_channel(
    payload: &horizon_protocol::packets::configuration::CustomPayload,
) {
    if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() { return; }

    crate::net::sunset_packets::scan_config(payload);
}
