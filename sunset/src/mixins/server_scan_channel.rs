// Configuration payloads otherwise do not become game events. Observe the identifier and
// minecraft:register's channel list only (Server Scan, on the game's side).
fn sunset_server_scan_config_channel(
    payload: &horizon_protocol::packets::configuration::CustomPayload,
) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    if let Some(seen) = ::sunset_core::bridge::CONFIG_PAYLOAD.get() {
        seen(payload as *const horizon_protocol::packets::configuration::CustomPayload as *mut u8);
    }
}
