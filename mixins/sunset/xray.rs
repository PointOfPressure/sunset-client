// Xray: while a section mesh is built, only the listed blocks are there.
fn sunset_xray_hook(this: &mut Mesher<'_>, region: &[[Option<&Chunk>; 3]; 3], sy: i32) -> std::ops::ControlFlow<SectionMesh> {
    use std::sync::atomic::Ordering::Relaxed;
    if !sunset_xray::ON.load(Relaxed) || sunset_xray::meshing() {
        return std::ops::ControlFlow::Continue(());
    }
    if let Some(c) = region[1][1] {
        sunset_xray::set_origin([c.x * 16, c.min_y() + sy * 16, c.z * 16]);
    }
    sunset_xray::set_meshing(true);
    let mesh = this.mesh(region, sy);
    sunset_xray::set_meshing(false);
    std::ops::ControlFlow::Break(mesh)
}
