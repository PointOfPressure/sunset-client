// Xray: a section's blocks read while a mesh is built.
fn sunset_xray_unpack(out: &mut [u32; 4096], value: ()) {
    crate::mesher::sunset_xray::filter_all(out);
    value
}
