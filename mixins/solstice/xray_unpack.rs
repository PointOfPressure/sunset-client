// Xray: a section's blocks read while a mesh is built.
fn solstice_xray_unpack(out: &mut [u32; 4096], value: ()) {
    crate::mesher::solstice_xray::filter_all(out);
    value
}
