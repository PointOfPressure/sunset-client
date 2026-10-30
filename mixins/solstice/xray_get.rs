// Xray: a block read while a mesh is built.
fn solstice_xray_get(value: u32) -> u32 {
    crate::mesher::solstice_xray::filter(value)
}
