// Client Spoofer: the brand the configuration phase sends (network thread), Horizon's own when no plan names one.
fn sunset_client_brand() -> std::ops::ControlFlow<Vec<u8>> {
    if !::sunset_core::lifecycle::RUNTIME.running() {
        return std::ops::ControlFlow::Continue(());
    }
    match ::sunset_core::client_brand::payload() {
        Some(brand) => {
            let mut w = horizon_protocol::buf::Writer::new();
            w.string(&brand);
            std::ops::ControlFlow::Break(w.data)
        }
        None => std::ops::ControlFlow::Continue(()),
    }
}
