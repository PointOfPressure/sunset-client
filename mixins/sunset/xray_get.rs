// Xray: a block read while a mesh is built.
fn sunset_xray_get(chunk: &Chunk, x: i32, y: i32, z: i32, value: u32) -> u32 {
    crate::mesher::sunset_xray::filter(value, [chunk.x * 16 + (x & 15), y, chunk.z * 16 + (z & 15)])
}
