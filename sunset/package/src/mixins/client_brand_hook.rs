// This runs on the session creator. The returned provider runs on that session's worker.
fn sunset_client_brand(
    address: &str,
    serial: u64,
    counters: &Arc<Mutex<Option<Arc<Counters>>>>,
) -> std::ops::ControlFlow<Option<ClientBrandProvider>> {
    let owner: std::sync::Arc<dyn Send + Sync> = counters.clone();
    if !::sunset_core::lifecycle::RUNTIME.running()
        || !::sunset_core::client_brand::attach(address, serial, std::sync::Arc::downgrade(&owner))
    {
        return std::ops::ControlFlow::Continue(());
    }
    std::ops::ControlFlow::Break(Some(Box::new(move || {
        if !::sunset_core::lifecycle::RUNTIME.running() {
            return None;
        }
        let brand = ::sunset_core::client_brand::payload(serial)?;
        let mut writer = horizon_protocol::buf::Writer::new();
        writer.string(&brand);
        Some(writer.data)
    })))
}
