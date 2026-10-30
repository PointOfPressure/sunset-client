// Xray: a block read while a mesh is built.
fn sunset_xray_get(value: u32) -> u32 {
    crate::mesher::sunset_xray::filter(value)
}
