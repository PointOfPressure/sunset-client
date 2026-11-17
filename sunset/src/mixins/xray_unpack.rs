// Xray: a section's blocks read while a mesh is built.
fn sunset_xray_unpack(out: &mut [u32; 4096], value: ()) {
    if !::sunset_core::lifecycle::RUNTIME.running() { return; }

    ::sunset_core::xray::filter_all(out);
    value
}
