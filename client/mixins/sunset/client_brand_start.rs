// Bind the profile to the shared connection cell before the network worker starts.
fn sunset_client_brand_start(
    address: &str,
    local: &Option<horizon_protocol::connection::Pipe>,
    counters: &std::sync::Arc<crate::net::sunset_client_brand::CountersCell>,
) {
    crate::net::sunset_client_brand::attach(address, local.is_some(), counters);
}
