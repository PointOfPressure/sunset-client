// Client Spoofer: the brand of the connection being made, Horizon's own otherwise.
fn sunset_client_brand() -> std::ops::ControlFlow<Vec<u8>> {
    if !::sunset_core::lifecycle::RUNTIME.running() { return std::ops::ControlFlow::Continue(()); }

    match ::sunset_core::client_brand::payload() {
        Some(data) => std::ops::ControlFlow::Break(data),
        None => std::ops::ControlFlow::Continue(()),
    }
}
