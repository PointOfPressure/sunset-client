// Client Spoofer: substitute the initial brand for this connection.
fn sunset_client_brand() -> std::ops::ControlFlow<Vec<u8>> {
    if horizon_protocol::connection::sunset_lifecycle::RUNTIME.running()
        && let Some(data) = crate::net::sunset_client_brand::payload()
    {
        return std::ops::ControlFlow::Break(data);
    }
    std::ops::ControlFlow::Continue(())
}
